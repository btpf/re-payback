//! Effect particles: flames, smoke, dust, sparks, blood, ground decals,
//! explosion shockwaves and flashes (docs/re/combat_police.md §5.1,
//! docs/re/rendering.md §7).
//!
//! The original keeps 4 lists of at most 5000 records, one per layer (cube
//! storey + 1). Records are spawned by `AddEffect` (OS4 `FUN_01035da0`, 68k
//! `FUN_0003d97a`; the updater's own spawns use `FUN_01034468`) and updated
//! and drawn in one pass by OS4 `FUN_010386e8` (68k `FUN_0003b3c2`): each
//! record is drawn, then moved and aged. Here [`Effects::tick`] runs in
//! `FixedPreUpdate`, before the game logic that spawns new records, so a new
//! record is first drawn where it was spawned, as in the original.
//!
//! Not reproduced:
//! - deletion of records outside the view rectangle (the simulation has no
//!   view; records still expire through size and z);
//! - weather (types 12–14 wrapping around the view and respawning, spawned
//!   by OS4 `FUN_010692bc`): here they only move and expire at z > 24000;
//! - type 15's looping 13 Flames sound and the per-type dynamic lights
//!   (OS4 `FUN_01036574`);
//! - debris fragments (`FUN_01035438`), a separate list;
//! - Blood.dark (gore option / night) and the full lens-flare ghosts; type
//!   11 is drawn as the LensFlare.2xx glow only.
//!
//! Rendering: every (layer, blend) pair, plus one lens-flare pass per layer,
//! is one mesh rebuilt each frame from horizontal quads ("screen-aligned"
//! for the overhead camera: aligned with the camera's yaw) that sample a
//! 512x256 atlas of the effect textures. [`EffectMaterial`] sets the blend
//! equation per [`Blend`], which gives the Alpha.256 operations exactly (in
//! linear colour rather than palette space), including subtraction via
//! `ReverseSubtract`. Sprites are depth-tested against the city, lifted 16
//! h-units; flares are not. Layers are drawn bottom to top through the
//! material's sort bias, after the city's transparent faces.

use crate::camera::GameCamera;
use crate::data::GameData;
use crate::sim::Rng;
use crate::world::{H_PER_SQUARE, SQUARE, to_render};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::visibility::NoFrustumCulling,
    image::{ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, CompareFunction,
        Extent3d, PrimitiveTopology, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};

/// Effect lists, one per layer.
const LAYERS: usize = 4;
/// Records per list.
const MAX_PER_LAYER: usize = 5000;
/// Gravity of types 2, 11 and 14, per tick.
const GRAVITY: i32 = 17;
/// Records deeper than this are removed (below ground).
const Z_LIMIT: i32 = 24000;
/// Type 5 (bubble) is removed once it rises to this depth.
const BUBBLE_SURFACE_Z: i32 = 23000;
/// The original projects effects with depth `z + camH·16 − 256`, i.e. as if
/// 16 h-units nearer the camera; in render units.
const NEARER: f32 = 16.0 / H_PER_SQUARE;
/// Half-size of the type 11 flash glow per unit of its size. The original
/// draws flares at a fixed screen size, (2/3)·(size/16)·scale/16 pixels;
/// a world size is an approximation chosen to look similar.
const FLASH_SCALE: f32 = 2.0;

/// Alpha.256 table an effect is drawn through (rendering.md §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code, reason = "public API; not every table has a caller yet")]
pub enum Blend {
    /// Table 0x00000: `min(d + s, 255)` (flares, flames, dust).
    Add,
    /// Table 0x10000: `max(d − s, 0)` (smoke, scorch, shadows).
    Subtract,
    /// Table 0x30000: `s + d·(255 − max(s))/256` (blood, dirt).
    Tint,
    /// Table 0x40000: `(d + s)/2`; texel 0 is not transparent.
    Average,
}

/// Effect texture (`EffectTextures/*`, 64x64 chunky).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "public API; not every texture has a caller yet")]
pub enum Tex {
    Dust,
    /// The smoke texture: `Dust` loaded with a hardware-alpha flag, so the
    /// same pixels in the software renderer.
    Dust2,
    Flare,
    /// Animated: Flame.1 → 2 → 3 → 4 → 1, one step per update.
    Flame,
    Blood,
    Gas,
    Dirt,
    DirtSkidmark,
    Shad,
    /// Shockwave.1xx, the first ring stage, drawn as 4 quarters.
    Shockwave,
    /// The LensFlare.2xx glow, drawn as 4 quarters.
    LensFlare,
}

/// One effect record (OS4 0x2c bytes at `DAT_0118ff2c`), in original units.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Effect {
    kind: u8,
    tex: Tex,
    /// Flame.N step (0..3) for [`Tex::Flame`]; ring stage (0..3) for type 10.
    frame: u8,
    blend: Blend,
    pos: IVec3,
    /// Per tick; stored as the spawn argument / 3.
    vel: IVec3,
    /// Sprite half-extent in world units, and the remaining life.
    size: i32,
}

impl Effect {
    fn new(kind: u8, tex: Tex, frame: u8, pos: IVec3, vel: IVec3, size: i32, blend: Blend) -> Self {
        Effect {
            kind,
            tex,
            frame,
            blend,
            pos,
            vel: vel / 3,
            size,
        }
    }

    /// One tick of `FUN_010386e8` (ticks = 1). Returns a record to spawn.
    fn update(&mut self, rand: &mut impl FnMut(i32) -> i32) -> Option<Effect> {
        self.pos += self.vel;
        if matches!(self.kind, 1..=4 | 7) && self.tex == Tex::Flame {
            self.frame = (self.frame + 1) % 4;
        }
        match self.kind {
            1 => self.size -= 20,
            // Burning debris: drops a flame (`rand(5) < ticks`).
            2 => {
                self.vel.z += GRAVITY;
                self.size -= 3;
                if rand(5) < 1 {
                    let frame = rand(4) as u8;
                    let pos = self.pos + jitter(rand, 50);
                    let vel = jitter(rand, 10);
                    return Some(Effect::new(4, Tex::Flame, frame, pos, vel, 400, Blend::Add));
                }
            }
            3 => self.size -= 57,
            4 => {
                self.size -= 23;
                return self.smoke(rand, self.size + 50, 200);
            }
            7 => {
                self.size -= 66;
                return self.smoke(rand, 350, 150);
            }
            6 => self.size -= 13,
            8 => self.size -= 1,
            9 => self.size -= 7,
            // Shockwave: grows; Shockwave.1xx..4xx by size.
            10 => {
                self.size += 300;
                self.frame = (self.size / 2500) as u8;
                if self.size > 9999 {
                    self.size = 0;
                }
            }
            11 => {
                self.vel.z += GRAVITY;
                self.size -= 5;
            }
            14 => self.vel.z += GRAVITY,
            // Fire source: feeds flames rising at vz ≈ −650/3.
            15 => {
                self.size -= 2;
                if rand(5) < 1 {
                    let frame = rand(4) as u8;
                    let pos = self.pos + jitter(rand, 50);
                    let vx = rand(20) - 10;
                    let vy = rand(20) - 10;
                    let vz = rand(100) - 650;
                    let vel = IVec3::new(vx, vy, vz);
                    return Some(Effect::new(
                        4,
                        Tex::Flame,
                        frame,
                        pos,
                        vel,
                        self.size,
                        Blend::Add,
                    ));
                }
            }
            _ => {}
        }
        None
    }

    /// Types 4 and 7: while the size is 100..=250, with chance
    /// `rand(2) < ticks`, emit a dark smoke puff (type 1, Dust2, subtract)
    /// of size `base + rand(range)`.
    fn smoke(&self, rand: &mut impl FnMut(i32) -> i32, base: i32, range: i32) -> Option<Effect> {
        if !(100..=250).contains(&self.size) || rand(2) >= 1 {
            return None;
        }
        let pos = self.pos + jitter(rand, 100);
        let vel = self.vel + jitter(rand, 10);
        let size = base + rand(range);
        Some(Effect::new(
            1,
            Tex::Dust2,
            0,
            pos,
            vel,
            size,
            Blend::Subtract,
        ))
    }

    /// Removal test after the update.
    fn expired(&self) -> bool {
        if self.kind == 5 {
            self.pos.z <= BUBBLE_SURFACE_Z
        } else {
            self.pos.z > Z_LIMIT || self.size < 1
        }
    }
}

/// `rand(2h) − h` on each axis, drawn x, y, z.
fn jitter(rand: &mut impl FnMut(i32) -> i32, half: i32) -> IVec3 {
    let x = rand(2 * half) - half;
    let y = rand(2 * half) - half;
    let z = rand(2 * half) - half;
    IVec3::new(x, y, z)
}

/// All live effects.
#[derive(Resource, Default)]
pub struct Effects {
    lists: [Vec<Effect>; LAYERS],
}

impl Effects {
    /// `AddEffect(type, tex, x, y, z, vx, vy, vz, size, layer, blend)`
    /// (OS4 `FUN_01035da0`). Pass the original call-site values: velocities
    /// are per tick and divided by 3 on entry, as the original does. Values
    /// are truncated to the original's integers; layers above 3 go to the
    /// top list. The effect is dropped when its list is full.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        kind: u8,
        tex: Tex,
        pos: Vec3,
        vel: Vec3,
        size: f32,
        layer: u8,
        blend: Blend,
    ) {
        let list = &mut self.lists[(layer as usize).min(LAYERS - 1)];
        if list.len() < MAX_PER_LAYER {
            let e = Effect::new(
                kind,
                tex,
                0,
                pos.as_ivec3(),
                vel.as_ivec3(),
                size as i32,
                blend,
            );
            list.push(e);
        }
    }

    /// Remove every effect (OS4 `FUN_01033d14` resets the counts).
    #[allow(dead_code, reason = "public API; no caller yet")]
    pub fn clear(&mut self) {
        self.lists.iter_mut().for_each(Vec::clear);
    }

    fn is_empty(&self) -> bool {
        self.lists.iter().all(Vec::is_empty)
    }

    /// Advance every effect one game tick. Lists run from last to first;
    /// spawned records are appended, and a removed record is replaced by the
    /// last one (`RemoveEffect`, OS4 `FUN_01035f48`), so neither is updated
    /// again this tick.
    fn tick(&mut self, rand: &mut impl FnMut(i32) -> i32) {
        for list in &mut self.lists {
            for i in (0..list.len()).rev() {
                let mut e = list[i];
                let spawned = e.update(rand);
                list[i] = e;
                if let Some(child) = spawned
                    && list.len() < MAX_PER_LAYER
                {
                    list.push(child);
                }
                if e.expired() {
                    list.swap_remove(i);
                }
            }
        }
    }
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/effects.wgsl");
        app.add_plugins(MaterialPlugin::<EffectMaterial>::default())
            .init_resource::<Effects>()
            .add_systems(Startup, setup)
            .add_systems(FixedPreUpdate, update_effects)
            .add_systems(PostUpdate, build_meshes.after(TransformSystems::Propagate));
    }
}

fn update_effects(mut effects: ResMut<Effects>, rng: Option<ResMut<Rng>>) {
    let Some(mut rng) = rng else { return };
    if !effects.is_empty() {
        effects.tick(&mut |n| rng.rand(n));
    }
}

// ---------------------------------------------------------------------------
// Rendering

const TILE: usize = 64;
const ATLAS_COLS: usize = 8;
const ATLAS_ROWS: usize = 4;
const ATLAS_W: usize = TILE * ATLAS_COLS;
const ATLAS_H: usize = TILE * ATLAS_ROWS;

/// Atlas tiles, in order. Quartered sprites (`<set>AB`: A = 0 left / 1
/// right, B = 0 top / 1 bottom) occupy 4 tiles at `first + 2A + B`.
const FILES: [&str; 31] = [
    "Dust",
    "Flare",
    "Flame.1",
    "Flame.2",
    "Flame.3",
    "Flame.4",
    "Blood",
    "Gas",
    "Dirt",
    "DirtSkidmark",
    "Shad",
    "Shockwave.100",
    "Shockwave.101",
    "Shockwave.110",
    "Shockwave.111",
    "Shockwave.200",
    "Shockwave.201",
    "Shockwave.210",
    "Shockwave.211",
    "Shockwave.300",
    "Shockwave.301",
    "Shockwave.310",
    "Shockwave.311",
    "Shockwave.400",
    "Shockwave.401",
    "Shockwave.410",
    "Shockwave.411",
    "LensFlare.200",
    "LensFlare.201",
    "LensFlare.210",
    "LensFlare.211",
];
const FLAME_TILE: usize = 2;
const SHOCKWAVE_TILE: usize = 11;
const LENS_GLOW_TILE: usize = 27;

/// Render batch of a sprite: one of the blends, or lens flares (additive
/// and never occluded: type 11 calls AddFlare with −size, which skips the
/// occlusion test, and sits at z 21000, below the ground surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pass {
    Blend(Blend),
    Flare,
}

impl Pass {
    /// Draw order within a layer: darkening first, glows last.
    const ORDER: [Pass; 5] = [
        Pass::Blend(Blend::Subtract),
        Pass::Blend(Blend::Tint),
        Pass::Blend(Blend::Average),
        Pass::Blend(Blend::Add),
        Pass::Flare,
    ];

    /// Index in [`Pass::ORDER`].
    fn rank(self) -> usize {
        match self {
            Pass::Blend(Blend::Subtract) => 0,
            Pass::Blend(Blend::Tint) => 1,
            Pass::Blend(Blend::Average) => 2,
            Pass::Blend(Blend::Add) => 3,
            Pass::Flare => 4,
        }
    }
}

/// How an effect is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sprite {
    /// One texture, mirrored left-right (`FUN_01033d50(tex, 0, …)`).
    Single(usize),
    /// A 128x128 image as 4 quarters meeting at the centre.
    Quarters(usize),
}

impl Effect {
    /// Sprite, world half-size and pass.
    fn sprite(&self) -> (Sprite, f32, Pass) {
        let size = self.size as f32;
        let sprite = match (self.kind, self.tex) {
            (10, _) => Sprite::Quarters(SHOCKWAVE_TILE + 4 * self.frame as usize),
            // Drawn as a lens flare (AddFlare at the centre).
            (11, _) => {
                return (
                    Sprite::Quarters(LENS_GLOW_TILE),
                    size * FLASH_SCALE,
                    Pass::Flare,
                );
            }
            (_, Tex::Shockwave) => Sprite::Quarters(SHOCKWAVE_TILE),
            (_, Tex::LensFlare) => Sprite::Quarters(LENS_GLOW_TILE),
            (_, Tex::Dust | Tex::Dust2) => Sprite::Single(0),
            (_, Tex::Flare) => Sprite::Single(1),
            (_, Tex::Flame) => Sprite::Single(FLAME_TILE + self.frame as usize),
            (_, Tex::Blood) => Sprite::Single(6),
            (_, Tex::Gas) => Sprite::Single(7),
            (_, Tex::Dirt) => Sprite::Single(8),
            (_, Tex::DirtSkidmark) => Sprite::Single(9),
            (_, Tex::Shad) => Sprite::Single(10),
        };
        (sprite, size, Pass::Blend(self.blend))
    }
}

/// Atlas of [`FILES`]: palette RGB, alpha = max(R, G, B) (the tint table's
/// coverage) and 0 for texel 0.
fn build_atlas(data: &GameData) -> Image {
    let mut rgba = vec![0u8; ATLAS_W * ATLAS_H * 4];
    for (i, name) in FILES.iter().enumerate() {
        let texels = match data.read(format!("EffectTextures/{name}")) {
            Ok(b) if b.len() >= TILE * TILE => b,
            _ => {
                warn!("missing effect texture {name}");
                continue;
            }
        };
        let (tx, ty) = (i % ATLAS_COLS * TILE, i / ATLAS_COLS * TILE);
        for (j, &c) in texels[..TILE * TILE].iter().enumerate() {
            let p = ((ty + j / TILE) * ATLAS_W + tx + j % TILE) * 4;
            let c = c as usize;
            let rgb = &data.palette[c * 3..c * 3 + 3];
            rgba[p..p + 3].copy_from_slice(rgb);
            rgba[p + 3] = if c == 0 {
                0
            } else {
                rgb[0].max(rgb[1]).max(rgb[2])
            };
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: ATLAS_W as u32,
            height: ATLAS_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// Effect sprite material: the atlas through one Alpha.256-like blend
/// (`shaders/effects.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(Pass)]
struct EffectMaterial {
    #[texture(0)]
    #[sampler(1)]
    atlas: Handle<Image>,
    pass: Pass,
    /// Transparent-pass sort bias: later layers and passes draw on top.
    order: f32,
}

impl From<&EffectMaterial> for Pass {
    fn from(material: &EffectMaterial) -> Self {
        material.pass
    }
}

impl Material for EffectMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/effects.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let component = |src_factor, dst_factor, operation| BlendComponent {
            src_factor,
            dst_factor,
            operation,
        };
        let (one, add) = (BlendFactor::One, BlendOperation::Add);
        let pass = key.bind_group_data;
        let color = match pass {
            Pass::Blend(Blend::Add) | Pass::Flare => component(one, one, add),
            Pass::Blend(Blend::Subtract) => component(one, one, BlendOperation::ReverseSubtract),
            Pass::Blend(Blend::Tint) => component(one, BlendFactor::OneMinusSrcAlpha, add),
            Pass::Blend(Blend::Average) => {
                component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha, add)
            }
        };
        descriptor.primitive.cull_mode = None;
        if pass == Pass::Flare
            && let Some(depth) = descriptor.depth_stencil.as_mut()
        {
            depth.depth_compare = Some(CompareFunction::Always);
        }
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if pass == Pass::Blend(Blend::Average) {
                fragment.shader_defs.push("EFFECT_AVERAGE".into());
            }
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color,
                    // Keep the destination alpha.
                    alpha: component(BlendFactor::Zero, one, add),
                });
            }
        }
        Ok(())
    }
}

/// One mesh entity per (layer, pass).
#[derive(Component)]
struct EffectBatch {
    layer: usize,
    pass: Pass,
}

fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<EffectMaterial>>,
) {
    let atlas = images.add(build_atlas(&data));
    for layer in 0..LAYERS {
        for pass in Pass::ORDER {
            let material = materials.add(EffectMaterial {
                atlas: atlas.clone(),
                pass,
                order: 1.0e4 * (1 + layer * Pass::ORDER.len() + pass.rank()) as f32,
            });
            commands.spawn((
                EffectBatch { layer, pass },
                MeshMaterial3d(material),
                Transform::default(),
                Visibility::Hidden,
                NoFrustumCulling,
            ));
        }
    }
}

/// Quads of one batch, in render space.
#[derive(Default)]
struct Quads {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Quads {
    /// A quad spanning `[x0, x1] × [y0, y1]` in sprite units (x right, y
    /// down the screen) around `centre`, textured with atlas tile `tile`.
    fn push(
        &mut self,
        axes: (Vec3, Vec3),
        centre: Vec3,
        x: [f32; 2],
        y: [f32; 2],
        tile: usize,
        mirror: bool,
    ) {
        const INSET: f32 = 0.01;
        let (right, down) = axes;
        let (col, row) = (
            (tile % ATLAS_COLS * TILE) as f32,
            (tile / ATLAS_COLS * TILE) as f32,
        );
        let mut u = [
            (col + INSET) / ATLAS_W as f32,
            (col + TILE as f32 - INSET) / ATLAS_W as f32,
        ];
        if mirror {
            u.swap(0, 1);
        }
        let v = [
            (row + INSET) / ATLAS_H as f32,
            (row + TILE as f32 - INSET) / ATLAS_H as f32,
        ];
        let base = self.positions.len() as u32;
        for (i, j) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
            self.positions
                .push((centre + right * x[i] + down * y[j]).to_array());
            self.uvs.push([u[i], v[j]]);
        }
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|k| base + k));
    }

    /// Emit the sprite of `e` (`FUN_010386e8` draw part).
    fn effect(&mut self, axes: (Vec3, Vec3), e: &Effect, sprite: Sprite, half: f32) {
        let centre = to_render(e.pos.as_vec3()) + Vec3::Y * NEARER;
        let h = half / SQUARE;
        match sprite {
            Sprite::Single(tile) => self.push(axes, centre, [-h, h], [-h, h], tile, true),
            Sprite::Quarters(first) => {
                for a in 0..2 {
                    for b in 0..2 {
                        let x = [(a as f32 - 1.0) * h, a as f32 * h];
                        let y = [(b as f32 - 1.0) * h, b as f32 * h];
                        self.push(axes, centre, x, y, first + 2 * a + b, false);
                    }
                }
            }
        }
    }

    fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Rebuild the batch meshes from the effect lists.
fn build_meshes(
    mut commands: Commands,
    effects: Res<Effects>,
    camera: Query<&GlobalTransform, With<GameCamera>>,
    mut batches: Query<(Entity, &EffectBatch, Option<&Mesh3d>, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Sprites face up, with their top towards the top of the screen.
    let right = camera
        .single()
        .map(|t| t.right().with_y(0.0).normalize_or(Vec3::X))
        .unwrap_or(Vec3::X);
    let axes = (right, right.cross(Vec3::Y));

    let mut quads: [[Quads; Pass::ORDER.len()]; LAYERS] = Default::default();
    for (layer, list) in effects.lists.iter().enumerate() {
        for e in list {
            let (sprite, half, pass) = e.sprite();
            quads[layer][pass.rank()].effect(axes, e, sprite, half);
        }
    }

    for (entity, batch, mesh, mut visibility) in &mut batches {
        let q = std::mem::take(&mut quads[batch.layer][batch.pass.rank()]);
        if q.indices.is_empty() {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let new_mesh = q.into_mesh();
        if let Some(mut m) = mesh.and_then(|m| meshes.get_mut(&m.0)) {
            *m = new_mesh;
        } else {
            commands.entity(entity).insert(Mesh3d(meshes.add(new_mesh)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(kind: u8, size: i32) -> Effect {
        Effect::new(
            kind,
            Tex::Dust,
            0,
            IVec3::new(1000, 1000, 20000),
            IVec3::ZERO,
            size,
            Blend::Add,
        )
    }

    /// `rand` that always returns `n − 1`: no chance-based spawns fire.
    fn never(n: i32) -> i32 {
        n - 1
    }

    #[test]
    fn size_change_per_type() {
        for (kind, delta) in [
            (1, -20),
            (2, -3),
            (3, -57),
            (4, -23),
            (6, -13),
            (7, -66),
            (8, -1),
            (9, -7),
            (11, -5),
            (15, -2),
            (10, 300),
            (5, 0),
        ] {
            let mut e = effect(kind, 1000);
            assert_eq!(e.update(&mut never), None, "type {kind}");
            assert_eq!(e.size, 1000 + delta, "type {kind}");
        }
    }

    #[test]
    fn velocity_is_divided_by_three() {
        let mut fx = Effects::default();
        fx.add(
            9,
            Tex::Flare,
            Vec3::new(100.0, 200.0, 300.0),
            Vec3::new(10.0, -10.0, 5.0),
            50.0,
            7,
            Blend::Add,
        );
        let e = fx.lists[3][0];
        assert_eq!(e.vel, IVec3::new(3, -3, 1));
        fx.tick(&mut never);
        assert_eq!(fx.lists[3][0].pos, IVec3::new(103, 197, 301));
    }

    #[test]
    fn gravity() {
        for kind in [2, 11, 14] {
            let mut e = effect(kind, 1000);
            e.vel = IVec3::new(0, 0, -30);
            e.update(&mut never);
            assert_eq!((e.pos.z, e.vel.z), (20000 - 30, -30 + 17), "type {kind}");
        }
        let mut e = effect(9, 1000);
        e.vel.z = -30;
        e.update(&mut never);
        assert_eq!(e.vel.z, -30);
    }

    #[test]
    fn removal() {
        let mut fx = Effects::default();
        fx.add(
            1,
            Tex::Dust,
            Vec3::new(0.0, 0.0, 20000.0),
            Vec3::ZERO,
            20.0,
            0,
            Blend::Add,
        );
        fx.add(
            1,
            Tex::Dust,
            Vec3::new(0.0, 0.0, 20000.0),
            Vec3::ZERO,
            21.0,
            0,
            Blend::Add,
        );
        fx.add(
            8,
            Tex::Dust,
            Vec3::new(0.0, 0.0, 23990.0),
            Vec3::new(0.0, 0.0, 60.0),
            500.0,
            0,
            Blend::Add,
        );
        fx.tick(&mut never);
        // Size 20 − 20 = 0 and z 24010 are gone; size 1 survives.
        assert_eq!(fx.lists[0].len(), 1);
        assert_eq!(fx.lists[0][0].size, 1);

        let mut bubble = effect(5, 10);
        bubble.pos.z = 23001;
        bubble.vel.z = -1;
        assert!(!bubble.expired());
        bubble.update(&mut never);
        assert!(bubble.expired());
    }

    #[test]
    fn shockwave() {
        let mut e = effect(10, 300);
        let mut ticks = 0;
        let mut frames = Vec::new();
        while !e.expired() {
            e.update(&mut never);
            frames.push(e.frame);
            ticks += 1;
        }
        // 300 + 300·33 = 10200 > 9999.
        assert_eq!(ticks, 33);
        assert_eq!(frames[0], 0);
        assert_eq!(frames[7], 1); // size 2700
        assert_eq!(frames[31], 3); // size 9900
        let (sprite, half, _) = Effect {
            size: 5000,
            frame: 2,
            ..e
        }
        .sprite();
        assert_eq!(
            (sprite, half),
            (Sprite::Quarters(SHOCKWAVE_TILE + 8), 5000.0)
        );
        assert_eq!(FILES[SHOCKWAVE_TILE + 8 + 2], "Shockwave.310");
        assert_eq!(FILES[LENS_GLOW_TILE + 1], "LensFlare.201");
    }

    #[test]
    fn flame_cycles() {
        let mut e = effect(7, 5000);
        e.tex = Tex::Flame;
        let frames: Vec<u8> = (0..5)
            .map(|_| {
                e.update(&mut never);
                e.frame
            })
            .collect();
        assert_eq!(frames, [1, 2, 3, 0, 1]);
        assert_eq!(e.sprite().0, Sprite::Single(FLAME_TILE + 1));
    }

    #[test]
    fn spawns() {
        // Burning debris drops a flame of size 400.
        let mut e = effect(2, 800);
        let child = e.update(&mut |_| 0).unwrap();
        assert_eq!(
            (child.kind, child.tex, child.size, child.blend),
            (4, Tex::Flame, 400, Blend::Add)
        );
        assert_eq!(child.pos, e.pos - IVec3::splat(50));
        assert_eq!(child.vel, IVec3::splat(-10 / 3));

        // A flame at 100..=250 emits dark smoke; above that it doesn't.
        let mut e = effect(4, 300);
        assert!(e.update(&mut |_| 0).is_none());
        e.size = 200;
        let smoke = e.update(&mut |_| 0).unwrap();
        assert_eq!(
            (smoke.kind, smoke.tex, smoke.blend),
            (1, Tex::Dust2, Blend::Subtract)
        );
        assert_eq!(smoke.size, 177 + 50);
        let mut e = effect(7, 300);
        assert_eq!(e.update(&mut |_| 0).unwrap().size, 350);

        // Fire source feeds rising flames of its own size.
        let mut e = effect(15, 500);
        let flame = e.update(&mut |_| 0).unwrap();
        assert_eq!((flame.kind, flame.size, flame.vel.z), (4, 498, -650 / 3));
    }

    #[test]
    fn pass_order() {
        for (i, pass) in Pass::ORDER.into_iter().enumerate() {
            assert_eq!(pass.rank(), i);
        }
    }

    #[test]
    fn list_limit_and_layers() {
        let mut fx = Effects::default();
        for _ in 0..MAX_PER_LAYER + 10 {
            fx.add(8, Tex::Blood, Vec3::ZERO, Vec3::ZERO, 100.0, 1, Blend::Tint);
        }
        assert_eq!(fx.lists[1].len(), MAX_PER_LAYER);
        assert!(fx.lists[0].is_empty());
        fx.clear();
        assert!(fx.is_empty());
    }

    #[test]
    fn children_are_not_updated_on_their_first_tick() {
        let mut fx = Effects::default();
        fx.add(
            2,
            Tex::Flame,
            Vec3::new(0.0, 0.0, 10000.0),
            Vec3::ZERO,
            800.0,
            0,
            Blend::Add,
        );
        fx.tick(&mut |_| 0);
        let list = &fx.lists[0];
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].size, 400);
    }
}
