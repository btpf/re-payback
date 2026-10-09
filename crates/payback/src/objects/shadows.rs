//! Object and scenery shadows (Mac `DrawShadow` @ 1007f7d0 = OS4
//! `FUN_010c9f28`, Mac `DrawSceneryShadow` @ 1007ea20 = OS4 `FUN_010c9360`;
//! analysis/mac/rendering.md §6.1, docs/re/rendering.md §7).
//!
//! The city pass (Mac `DrawMap`, OS4 `FUN_01049374`) draws, square by
//! square after the effects of a layer, the shadow of every scenery model
//! and once per frame (flag `+0x816`) the shadow of every object:
//!
//! - Vehicles: a 3×3 grid over the model's box, x ∈ {−r, 0, r} (r =
//!   `+0x8120`, max |X|), y ∈ {−minY, 0, maxY} (`+0x8128`, `+0x8124`),
//!   turned with the heading, each point on its own ground sample (the
//!   object's 9 floor samples, `+0x8b8`), as 8 triangles of the object
//!   texture `shadow` at u ∈ {2, 0x7f, 0xfd}, v ∈ {1, 0x3f, 0x7e} through
//!   Alpha.256 table 1 (subtract); in hardware `(7, 8)` with `shadow`
//!   loaded black, alpha = max(R, G, B) (`API_3D_Set_Shadow`).
//! - People and helicopters (`+0x8165 == 1` or `+0x8186`): the effect
//!   sprite `Shad` (`Draw2DEffect`, table 1), a square of half-size
//!   1.5·r (`r·0x300` over the depth ·16) on the centre ground sample.
//! - Every point moves by `(z_ground − z + K)/4` in +x and +y, K = 0x80
//!   for people, helicopters and the RC car (`+0x8160`), 0x200 for other
//!   vehicles: shadows fall to the south-east and grow with height.
//! - `DrawShadow` returns for objects in state 6 and for those with both
//!   `+0x81a` and `+0x818` set: the shots that fly straight. People
//!   (`do_Person` @ 1013fbd0) and vehicles (`DoVehicleFire` @ 10070c20)
//!   set both on every bullet, pellet, petrol drop and missile they fire,
//!   and `MoveObject_General` @ 1006e4f0 sets them again each tick for
//!   those models; grenades and bombs fall under gravity and keep theirs.
//! - Scenery, by day only (`GetObjectNight() == 0`): every face of the
//!   model flattened onto the ground of its cube, each vertex moved by
//!   −z/4 in +x and +y (z up is negative), the slot picked by the projected
//!   winding (slot 0 when clockwise on screen) and drawn if active. A face
//!   whose slot 0 has mode 5 (whatever its `active` byte) is a line between
//!   its first two vertices in palette colour 1 (`DrawLine(…, 1)`; Warp3D
//!   `DrawLine`). Spinning models (pickups, `+0x8164 ≥ 0`) are flattened at
//!   their spin heading `DAT_0114c770 + rotation·0x1680 >> 8` (Mac
//!   `DrawMap` @ 1005a260 calls), the yaw only (the info sign's tumble is
//!   not applied). Top-down, nothing is drawn when the cube top is at depth
//!   ≤ 6000 (`6000 < depth` test; the rotating view halves the depth and
//!   always draws). Mode-3 faces: the face texture through table 2 (d − 16
//!   where the texel is not 0); in hardware `Tree`/`Tree2` faces use
//!   `Tree2.clone` and `tanne` faces `tanne.clone` (GenlockShadow: black,
//!   alpha 0x32, texel 0 transparent), other mode-3 faces flat black at the
//!   shadow alpha. Other faces: the `shadow` texel (10, 5) through table 1;
//!   in hardware flat black.
//!
//! Port: shadows are horizontal triangles just above the ground,
//! depth-tested, so an object hides its own shadow as in the painter's
//! order (a deliberate approximation of the original's draw order). The
//! game views hold gamma-encoded palette RGB (`crate::camera::postfx`), so
//! the table-1 subtraction `d − s` and table 2's −16 are exact blends on
//! the palette bytes (`shaders/shadows.wgsl`). In hardware the tree faces
//! use the face texture's own key for the clones: `Tree2.clone` and
//! `tanne.clone` are byte copies of `Tree2` and `tanne` (CONFIRMED, the
//! shipped files compare equal). Line shadows use the object material (an
//! opaque palette colour); in hardware their colour is INFERRED the same.
//! The depth cut and the spin heading are applied per frame
//! ([`scenery_shadow_frame`]); with split screen a shadow shows when any
//! view is far enough (the entities are shared).

use super::{ObjectLibrary, PALETTE_SLOT, Scenery, sid, static_vertex};
use crate::data::GameData;
use crate::level::LevelEntity;
use crate::sim::driving::Drive;
use crate::sim::people::Person;
use crate::sim::combat::Projectile;
use crate::sim::{Obj as SimObj, State, Trig, collide};
use crate::vehicles::Vehicle;
use crate::world::{City, H_PER_SQUARE, to_render};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    image::ImageSampler,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
        PrimitiveTopology, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};
use std::collections::HashMap;

/// Shadows sit this far (render units, 1 h-unit) above the ground.
const LIFT: f32 = 1.0 / H_PER_SQUARE;

/// Per-vertex kind (vertex colour red · 4), see `shaders/shadows.wgsl`.
mod kind {
    /// A texel of the `shadow` page: subtract it; hardware black, alpha =
    /// max(R, G, B).
    pub const PAGE: f32 = 0.0;
    /// A `Tree`/`Tree2`/`tanne` face: table 2 where the texel is not 0;
    /// hardware the clone (black, alpha 0x32).
    pub const TREE: f32 = 1.0;
    /// Another face: the `shadow` texel (10, 5); hardware flat black.
    pub const FLAT: f32 = 2.0;
    /// The `Shad` sprite: subtract; hardware black, alpha = max.
    pub const SPRITE: f32 = 3.0;
    /// Another mode-3 face: table 2 where the texel is not 0; hardware
    /// flat black.
    pub const KEYED: f32 = 4.0;
}

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<ShadowMaterial>::default())
        .init_resource::<SceneryShadows>()
        .add_systems(
            PostUpdate,
            (setup, object_shadows, scenery_shadows)
                .chain()
                .after(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            scenery_shadow_frame.before(TransformSystems::Propagate),
        );
}

/// The shadow material: the object atlas and the `Shad` sprite through
/// table 1 (or the hardware blend).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(ShadowKey)]
pub struct ShadowMaterial {
    #[texture(0)]
    #[sampler(1)]
    atlas: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    sprite: Handle<Image>,
    /// x = 1 for the hardware look (`shaders/shadows.wgsl`).
    #[uniform(4)]
    params: Vec4,
    hardware: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowKey {
    hardware: bool,
}

impl From<&ShadowMaterial> for ShadowKey {
    fn from(m: &ShadowMaterial) -> Self {
        ShadowKey { hardware: m.hardware }
    }
}

impl Material for ShadowMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/shadows.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
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
        let hardware = key.bind_group_data.hardware;
        let component = |src_factor, dst_factor, operation| BlendComponent {
            src_factor,
            dst_factor,
            operation,
        };
        let gamma = super::gamma_view(key.mesh_key);
        let color = if hardware {
            // W3D_SetBlendMode(SRC_ALPHA, ONE_MINUS_SRC_ALPHA).
            component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha, BlendOperation::Add)
        } else if gamma {
            // Alpha.256 table 1, max(d − s, 0) on the palette bytes the
            // game view holds (`crate::camera::postfx::GammaWorld`).
            component(BlendFactor::One, BlendFactor::One, BlendOperation::ReverseSubtract)
        } else {
            // The same on a linear target, as a darkening factor: dst ·
            // (1 − k), k from the shader.
            component(BlendFactor::Zero, BlendFactor::OneMinusSrc, BlendOperation::Add)
        };
        descriptor.primitive.cull_mode = None;
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if gamma {
                fragment.shader_defs.push("GAMMA_TARGET".into());
            }
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color,
                    alpha: component(BlendFactor::Zero, BlendFactor::One, BlendOperation::Add),
                });
            }
        }
        Ok(())
    }
}

/// This level's shadow material and the per-frame object shadow mesh.
#[derive(Resource)]
struct ShadowAssets {
    material: Handle<ShadowMaterial>,
    mesh: Handle<Mesh>,
    /// Atlas slot of the `shadow` page.
    page: Option<u32>,
    hardware: bool,
    /// The library these were made for (its material).
    library: AssetId<super::ObjectMaterial>,
}

/// Scenery shadow meshes (faces, lines) by (model, heading in 5760ths),
/// for the current library.
#[derive(Resource, Default)]
struct SceneryShadows(HashMap<(usize, i32), ShadowMeshes>);

#[derive(Clone, Default)]
struct ShadowMeshes {
    faces: Option<Handle<Mesh>>,
    lines: Option<Handle<Mesh>>,
}

impl SceneryShadows {
    fn get(&mut self, library: &ObjectLibrary, meshes: &mut Assets<Mesh>, model: usize, yaw: i32, page: Option<u32>) -> ShadowMeshes {
        self.0
            .entry((model, yaw))
            .or_insert_with(|| {
                let rot = crate::world::heading_to_render(yaw as f32);
                let (faces, lines) = scenery_mesh(library, model, rot, page);
                ShadowMeshes {
                    faces: faces.map(|m| meshes.add(m)),
                    lines: lines.map(|m| meshes.add(m)),
                }
            })
            .clone()
    }
}

/// A scenery shadow (faces through the shadow material, or the mode-5
/// lines through the object material), a child of its scenery entity.
#[derive(Component)]
struct SceneryShadow {
    lines: bool,
}

/// Heading (5760ths) a scenery model's shadow is flattened at: the cube
/// rotation, plus the pickup spin for spinning models.
fn shadow_yaw(sc: &Scenery, spin: i32) -> i32 {
    let yaw = sc.rotation as i32 * 0x1680 >> 8;
    if sid::SPINNING.contains(&sc.id) { (yaw + spin) % 0x1680 } else { yaw }
}

/// The `Shad` effect texture (64×64): palette RGB, alpha = max(R, G, B),
/// texel 0 clear.
fn sprite_image(data: &GameData) -> Image {
    let texels = data.read("EffectTextures/Shad").unwrap_or_default();
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for (j, &c) in texels.iter().take(64 * 64).enumerate() {
        let c = c as usize;
        let rgb = [data.palette[c * 3], data.palette[c * 3 + 1], data.palette[c * 3 + 2]];
        rgba[j * 4..j * 4 + 3].copy_from_slice(&rgb);
        rgba[j * 4 + 3] = if c == 0 { 0 } else { rgb[0].max(rgb[1]).max(rgb[2]) };
    }
    let mut image = Image::new(
        Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

fn empty_mesh() -> Mesh {
    Tris::default().mesh()
}

/// Shadow triangles under construction: positions, UVs and kinds.
#[derive(Default)]
struct Tris {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    kinds: Vec<[f32; 4]>,
}

impl Tris {
    fn push(&mut self, p: Vec3, uv: [f32; 2], kind: f32) {
        self.positions.push(p.to_array());
        self.uvs.push(uv);
        self.kinds.push([kind / 4.0, 0.0, 0.0, 1.0]);
    }

    fn mesh(self) -> Mesh {
        let n = self.positions.len();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.kinds)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
    }
}

/// Per level (a new [`ObjectLibrary`]) and per renderer: the material and
/// the object-shadow entity.
#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    library: Option<Res<ObjectLibrary>>,
    assets: Option<ResMut<ShadowAssets>>,
    mut cache: ResMut<SceneryShadows>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ShadowMaterial>>,
    std_materials: Res<Assets<super::ObjectMaterial>>,
    existing: Query<Entity, With<ObjectShadows>>,
) {
    let Some(library) = library else { return };
    let hardware = library.hardware;
    // The level loader may insert the same library more than once; a new
    // library has a new material.
    let id = library.material.id();
    if let Some(mut a) = assets {
        if a.hardware != hardware
            && let Some(mut m) = materials.get_mut(&a.material)
        {
            m.hardware = hardware;
            m.params.x = if hardware { 1.0 } else { 0.0 };
            a.hardware = hardware;
        }
        if a.library == id && !existing.is_empty() {
            return;
        }
    }
    cache.0.clear();
    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(atlas) = std_materials
        .get(&library.material)
        .and_then(|m| m.base.base_color_texture.clone())
    else {
        return;
    };
    let material = materials.add(ShadowMaterial {
        atlas,
        sprite: images.add(sprite_image(&data)),
        params: Vec4::new(if hardware { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
        hardware,
    });
    let mesh = meshes.add(empty_mesh());
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::IDENTITY,
        NoFrustumCulling,
        ObjectShadows,
        LevelEntity,
    ));
    commands.insert_resource(ShadowAssets {
        material,
        mesh,
        page: library.slots.get("shadow").copied(),
        hardware,
        library: id,
    });
}

/// The entity holding every object shadow of the frame.
#[derive(Component)]
struct ObjectShadows;

/// Atlas UV of a texel of the `shadow` page (`row` in texture rows).
fn page_uv(page: Option<u32>, u: i32, row: i32) -> [f32; 2] {
    let slot = page.unwrap_or(PALETTE_SLOT);
    ObjectLibrary::atlas_uv(slot, u, row * 2, false)
}

/// Rebuild the object shadows (`DrawShadow`).
#[allow(clippy::too_many_arguments)]
fn object_shadows(
    assets: Option<Res<ShadowAssets>>,
    library: Option<Res<ObjectLibrary>>,
    city: Option<Res<City>>,
    trig: Option<Res<Trig>>,
    mut meshes: ResMut<Assets<Mesh>>,
    smooth: Res<crate::frame::SmoothOffsets>,
    q: Query<(Entity, &SimObj, Option<&Person>, Option<&Vehicle>, Option<&Drive>, Option<&Projectile>), With<Mesh3d>>,
) {
    let (Some(assets), Some(library), Some(city), Some(trig)) = (assets, library, city, trig) else {
        return;
    };
    let mut tris = Tris::default();
    for (e, obj, person, vehicle, drive, shot) in &q {
        if obj.state == State::Removed || obj.model >= library.models.len() {
            continue;
        }
        // `+0x81a` and `+0x818` both set: a shot flying straight.
        if shot.is_some_and(|p| !p.kind.gravity()) {
            continue;
        }
        let m = library.model(obj.model);
        let heli = vehicle.is_some_and(|v| v.kind == crate::vehicles::kind::HELICOPTER);
        let small = person.is_some() || heli;
        let k = if small || vehicle.is_some_and(|v| v.kind == crate::vehicles::kind::RC_CAR) {
            0x80
        } else {
            0x200
        };
        let ground = |x: i32, y: i32| collide::floor(&city, x, y, obj.pos.z).map_or(obj.pos.z, |f| f.0);
        // A ground point moved by (z_ground − z + K)/4 in +x and +y.
        // Smooth motion: moved with the object as drawn (`crate::frame`).
        let shift = smooth.get(e) * Vec3::new(1.0, 0.0, 1.0);
        let point = |x: i32, y: i32, zg: i32| {
            let d = (zg - obj.pos.z + k) >> 2;
            to_render(Vec3::new((x + d) as f32, (y + d) as f32, zg as f32)) + Vec3::Y * LIFT + shift
        };
        if small {
            // `Draw2DEffect(Shad, …, 0x10000)`: half-size 1.5·r.
            let zg = ground(obj.pos.x, obj.pos.y);
            let c = point(obj.pos.x, obj.pos.y, zg);
            let h = (m.info.radius * 3 / 2) as f32 / crate::world::SQUARE;
            let corner = |sx: f32, sy: f32| c + Vec3::new(sx * h, 0.0, sy * h);
            let (a, b, cc, d) = (corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0));
            for (p, uv) in [(a, [0.0, 0.0]), (b, [1.0, 0.0]), (cc, [1.0, 1.0]), (a, [0.0, 0.0]), (cc, [1.0, 1.0]), (d, [0.0, 1.0])] {
                tris.push(p, uv, kind::SPRITE);
            }
            continue;
        }
        // The 3×3 grid on the box (`+0x8120`, `+0x8128`, `+0x8124` of the
        // drawn model) and its ground samples.
        let r = m.shape.radius;
        let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
        let mut grid = [Vec3::ZERO; 9];
        for (i, lat) in [-r, 0, r].into_iter().enumerate() {
            for (j, fwd) in [-m.shape.min_y, 0, m.shape.max_y].into_iter().enumerate() {
                // Model +X = (cos h, −sin h), +Y = (sin h, cos h).
                let x = obj.pos.x + ((c * lat + s * fwd) >> 8);
                let y = obj.pos.y + ((-s * lat + c * fwd) >> 8);
                let zg = match drive {
                    Some(d) => d.heights[i * 3 + j],
                    None => ground(x, y),
                };
                grid[i * 3 + j] = point(x, y, zg);
            }
        }
        const U: [i32; 3] = [2, 0x7f, 0xfd];
        const V: [i32; 3] = [1, 0x3f, 0x7e];
        for i in 0..2 {
            for j in 0..2 {
                let cell = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j), (i + 1, j + 1), (i, j + 1)];
                for (a, b) in cell {
                    tris.push(grid[a * 3 + b], page_uv(assets.page, U[a], V[b]), kind::PAGE);
                }
            }
        }
    }
    if let Some(mut mesh) = meshes.get_mut(&assets.mesh) {
        *mesh = tris.mesh();
    }
}

/// The shadow of each new scenery object (`DrawSceneryShadow`), as
/// children that cancel the parent's turn (the −z/4 shift is along the map
/// axes): the faces and the mode-5 lines.
#[allow(clippy::too_many_arguments)]
fn scenery_shadows(
    mut commands: Commands,
    assets: Option<Res<ShadowAssets>>,
    library: Option<Res<ObjectLibrary>>,
    clock: Res<super::SceneryClock>,
    mut cache: ResMut<SceneryShadows>,
    mut meshes: ResMut<Assets<Mesh>>,
    q: Query<(Entity, &Scenery, &Transform), Added<Scenery>>,
) {
    let (Some(assets), Some(library)) = (assets, library) else {
        return;
    };
    // `DrawSceneryShadow` returns at night (`GetObjectNight`).
    if library.night {
        return;
    }
    for (e, sc, tf) in &q {
        let spinning = sid::SPINNING.contains(&sc.id);
        let m = cache.get(&library, &mut meshes, sc.model, shadow_yaw(sc, clock.spin), assets.page);
        let place = Transform::from_translation(tf.rotation.inverse() * Vec3::Y * (LIFT + sc.ground - tf.translation.y))
            .with_rotation(tf.rotation.inverse());
        // Spinning models get both children (their meshes change per frame).
        if m.faces.is_some() || spinning {
            commands.spawn((
                Mesh3d(m.faces.clone().unwrap_or_default()),
                MeshMaterial3d(assets.material.clone()),
                // On the ground under a lifted model (`sunk_lift`).
                place,
                if m.faces.is_some() { Visibility::Inherited } else { Visibility::Hidden },
                SceneryShadow { lines: false },
                ChildOf(e),
            ));
        }
        if m.lines.is_some() || spinning {
            commands.spawn((
                Mesh3d(m.lines.clone().unwrap_or_default()),
                MeshMaterial3d(library.material.clone()),
                place,
                if m.lines.is_some() { Visibility::Inherited } else { Visibility::Hidden },
                SceneryShadow { lines: true },
                ChildOf(e),
            ));
        }
    }
}

/// Per frame: spinning models' shadows follow the spin (their parent turns
/// and, for the info sign, tumbles and lifts: the child cancels it), and
/// the top-down depth cut (`DrawSceneryShadow`: drawn only at depth
/// `(camH + 1500 − top)·16 > 6000`, INFERRED units: the cube top's depth
/// below the eye, 16 per h-unit).
#[allow(clippy::too_many_arguments)]
fn scenery_shadow_frame(
    assets: Option<Res<ShadowAssets>>,
    library: Option<Res<ObjectLibrary>>,
    clock: Res<super::SceneryClock>,
    eyes: Res<crate::camera::ViewEyes>,
    rotating: Res<crate::camera::RotatingView>,
    mut cache: ResMut<SceneryShadows>,
    mut meshes: ResMut<Assets<Mesh>>,
    parents: Query<(&Scenery, &Transform), Without<SceneryShadow>>,
    mut shadows: Query<(&SceneryShadow, &ChildOf, &mut Mesh3d, &mut Transform, &mut Visibility)>,
) {
    let (Some(assets), Some(library)) = (assets, library) else {
        return;
    };
    for (kind, parent, mut mesh, mut tf, mut vis) in &mut shadows {
        let Ok((sc, ptf)) = parents.get(parent.parent()) else { continue };
        let near = !rotating.0
            && !eyes.cams.is_empty()
            && eyes
                .cams
                .iter()
                .all(|c| ((c.h + 1500) as f32 - sc.ground * H_PER_SQUARE) * 16.0 <= 6000.0);
        let mut shown = !near;
        if sid::SPINNING.contains(&sc.id) {
            let m = cache.get(&library, &mut meshes, sc.model, shadow_yaw(sc, clock.spin), assets.page);
            let want = if kind.lines { m.lines } else { m.faces };
            shown &= want.is_some();
            if let Some(h) = want
                && mesh.0 != h
            {
                mesh.0 = h;
            }
            let inv = ptf.rotation.inverse();
            let place = Transform::from_translation(inv * Vec3::Y * (LIFT + sc.ground - ptf.translation.y))
                .with_rotation(inv);
            if *tf != place {
                *tf = place;
            }
        } else if kind.lines || mesh.0 == Handle::default() {
            shown &= mesh.0 != Handle::default();
        }
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// The flattened faces of scenery `model` turned by `rot`, around its
/// origin on the ground, and its mode-5 shadow lines.
fn scenery_mesh(library: &ObjectLibrary, model: usize, rot: Quat, page: Option<u32>) -> (Option<Mesh>, Option<Mesh>) {
    let obj = &library.model(model).obj;
    let shift = |v: &payback_formats::obj::Vertex| {
        // −z/4 object units per ×16 unit: −4·z per file unit, in +x and +y.
        let d = (-4 * v.z) as f32 / crate::world::SQUARE;
        let p = rot * Vec3::from(static_vertex(v));
        Vec3::new(p.x + d, 0.0, p.z + d)
    };
    let mut tris = Tris::default();
    let mut lines: Vec<[f32; 3]> = Vec::new();
    for face in &obj.faces {
        let p = face.indices.map(|i| shift(&obj.vertices[i]));
        // Slot 0 mode 5, active or not: a line, palette colour 1.
        if face.slots[0].mode == 5 {
            lines.extend([p[0].to_array(), p[1].to_array()]);
            continue;
        }
        // Screen x = east (render x), y = south (render z).
        let cross = (p[1].x - p[0].x) * (p[2].z - p[0].z) - (p[2].x - p[0].x) * (p[1].z - p[0].z);
        let slot = &face.slots[if cross < 0.0 { 0 } else { 1 }];
        if !slot.active || slot.mode == 5 {
            continue;
        }
        let tex = library.slots.get(&slot.texture.to_lowercase()).copied();
        for (k, &pk) in p.iter().enumerate() {
            let (kind, uv) = match (slot.mode, tex) {
                (3, Some(t)) => {
                    let name = slot.texture.to_lowercase();
                    let tree = matches!(name.as_str(), "tree" | "tree2" | "tanne");
                    let (u, v) = slot.uv[k];
                    (if tree { kind::TREE } else { kind::KEYED }, ObjectLibrary::atlas_uv(t, u, v, true))
                }
                _ => (kind::FLAT, page_uv(page, 10, 5)),
            };
            tris.push(pk, uv, kind);
        }
    }
    let lines = (!lines.is_empty()).then(|| {
        let n = lines.len();
        Mesh::new(
            PrimitiveTopology::LineList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, lines)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![ObjectLibrary::palette_uv(1); n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[-1.0f32; 2]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
    });
    ((!tris.positions.is_empty()).then(|| tris.mesh()), lines)
}
