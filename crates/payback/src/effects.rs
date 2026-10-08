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
//! Weather records (types 12–14, spawned by `crate::weather`) wrap around
//! the view rectangle of the game camera and respawn when they reach the
//! ground (OS4 `FUN_010386e8`, rendering.md §4).
//!
//! Lens flares (`AddFlare` OS4 `FUN_0103451c`, drawn by `FUN_01034804`) are
//! collected each frame in [`Flares`] (type 11 records here, model faces in
//! `crate::objects`) and drawn at a fixed screen size with the ghost chain
//! through the screen centre, per [`FxMode`], separately for every view
//! (split screen) with its own occlusion test. [`ScreenFlash`] is the
//! palette brightening of explosions and lightning (`DAT_0114c748`), applied
//! by the views' final pass (`crate::camera::postfx`).
//!
//! Not reproduced:
//! - deletion of records outside the view rectangle (the simulation has no
//!   view; records still expire through size and z);
//! - type 15's looping 13 Flames sound (the per-type dynamic lights of OS4
//!   `FUN_01036574` are in `crate::lighting`).
//!
//! Debris (`FUN_01035438`, Mac `Split`) is a separate list, updated after
//! the effects: `crate::debris`.
//!
//! Rendering: every (view, layer, blend) triple, plus one lens-flare pass per
//! layer, is one mesh rebuilt each frame on the view's render layer from
//! horizontal quads ("screen-aligned" for the overhead camera: aligned with
//! that view's yaw, which differs in the rotating view) that sample a
//! 512x256 atlas of the effect textures. [`EffectMaterial`] sets the blend
//! equation per [`Blend`], which gives the Alpha.256 operations exactly (in
//! linear colour rather than palette space; the final pass snaps the result
//! to the palette), including subtraction via
//! `ReverseSubtract`. Sprites are depth-tested against the city, lifted 16
//! h-units; flares and weather are not (the original draws list 3 after
//! everything). Layers are drawn bottom to top through the material's sort
//! bias, after the city's transparent faces.
//!
//! Hardware renderer ([`crate::render_mode::RenderMode::Hardware`]; OS4
//! `FUN_01033d50` with `DAT_0114c7d8 != 0`, docs/re/rendering.md "Hardware
//! (Warp3D) renderer"): the textures are bilinear W3D textures converted at
//! load, and the tables become W3D blend modes: tables 0, 2, 4, 9, 10 draw
//! ONE/ONE (additive, so the 50 % average is additive too), tables 1 and 3
//! SRC_ALPHA/ONE_MINUS_SRC_ALPHA with textures converted to carry their own
//! alpha (smoke and shadows black with alpha = max(R, G, B), blood with
//! saturated channels, gas pure green, dirt its colours, all alpha = max),
//! tables 5-8 ONE/ONE with a red / amber / green / blue vertex colour. Lens
//! flares are never occlusion-tested there; rain streaks and splashes are
//! white lines / points blended (7, 8) at alpha 0.4.

use crate::camera::{ExtraView, GameCam, GameCamera, RotatingView, view_layer};
use crate::data::GameData;
use crate::sim::VisualRng;
use crate::world::{City, SQUARE, to_render, z_of_h};
use std::collections::HashMap;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::visibility::{NoFrustumCulling, RenderLayers},
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
const NEARER: f32 = crate::world::OBJECT_LIFT;
/// Snow lands at street level: a flake deeper than this is removed and
/// leaves its mark there (Mac `UpdateEffects` @ 100aa1f0: `0x4dc0`, 19904;
/// OS4 `FUN_010386e8` let flakes fall to `Z_LIMIT` and marked at 0x57e4,
/// below the streets; rendering D1).
const SNOW_GROUND_Z: i32 = 0x4dc0;
/// Rain splashes start at `0x5780`.
const SPLASH_Z: i32 = 0x5780;
/// Base screen of the original (320x256): flare sizes are in its pixels.
const SCREEN_H: i64 = 256;
/// `AddFlare` keeps at most this many flares per frame.
const MAX_FLARES: usize = 199;

/// Lens-flare option (`DAT_0114c428`, set by `FUN_01035960`): 0 off, 1
/// glow only (LensFlare.2xx), 2 glow plus the rings and ghosts. The front
/// end may change it; default 2.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxMode(pub u8);

impl Default for FxMode {
    /// 2, or `--fx <n>` on the command line.
    fn default() -> Self {
        FxMode(crate::arg("--fx").and_then(|v| v.parse().ok()).unwrap_or(2))
    }
}

/// Palette flash (`DAT_0114c748`): added to every palette RGB channel
/// (clamped to 255) by `FUN_0103ad6c` while positive, and decayed ×3/4 per
/// game tick (main loop in OS4 `FUN_010692bc`). Explosions add
/// `power / 250000` (`FUN_0104405c`), lightning `100 + rand(150)` per flash.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ScreenFlash(pub i32);

/// A lens-flare source for this frame (`AddFlare(x, y, size, blend)`).
#[derive(Clone, Copy, Debug)]
pub struct FlareSource {
    /// Render-space position.
    pub pos: Vec3,
    /// `size`; the flare radius is `size >> 4` base-screen pixels.
    pub size: i32,
    /// Alpha.256 table of the face (`blend >> 16`; 3 counts as 0).
    pub table: u8,
    /// Hidden when the city is in front of it (`AddFlare` with a positive
    /// size; explosion flares pass a negative one).
    pub occlude: bool,
}

/// Flares added this frame; drawn and cleared by the effect renderer.
#[derive(Resource, Default)]
pub struct Flares(pub Vec<FlareSource>);

impl Flares {
    pub fn add(&mut self, f: FlareSource) {
        if self.0.len() < MAX_FLARES {
            self.0.push(f);
        }
    }
}

/// Effect drawing (mesh rebuild); flare collectors run before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectsDraw;

/// The game camera as the effect updater sees it: position in object units,
/// `camH` (h-units) and the half-size of the drawn area in squares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct View {
    x: i32,
    y: i32,
    h: i32,
    half: (i32, i32),
}

impl View {
    /// Weather only runs in single player, so the one-view square range.
    fn of(cam: &GameCam, rotating: bool) -> View {
        View {
            x: (cam.x >> 5) as i32,
            y: (cam.y >> 5) as i32,
            h: cam.h,
            half: cam.visible_squares(1, rotating),
        }
    }

    /// The view rectangle `[x0, x1] × [y0, y1]` in object units: the drawn
    /// half-size plus one square either side of the camera, `cx·(camH +
    /// 1500) / (fx << 12) + 1` squares (`GameCam::visible_squares`, also
    /// the rotating and split-screen variants). Mac `UpdateEffects` @
    /// 100aa1f0; the OS4 `FUN_010386e8` scaled `(camH·16 + 0x2af2)·W` with
    /// the screen width instead, so its rectangle grew with the resolution
    /// (rendering D2).
    fn rect(&self) -> (i32, i32, i32, i32) {
        let (hw, hh) = (self.half.0 * 0x800, self.half.1 * 0x800);
        (self.x - hw, self.x + hw, self.y - hh, self.y + hh)
    }
}

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
                    let vel = jitter(rand, 10);
                    let pos = self.pos + jitter(rand, 50);
                    let frame = rand(4) as u8;
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
                    let vz = rand(100) - 650;
                    let vy = rand(20) - 10;
                    let vx = rand(20) - 10;
                    let pos = self.pos + jitter(rand, 50);
                    let frame = rand(4) as u8;
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
        let size = base + rand(range);
        let vel = self.vel + jitter(rand, 10);
        let pos = self.pos + jitter(rand, 100);
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

    /// Weather wrap: a record leaving the view rectangle re-enters at the
    /// opposite side (`FUN_010386e8`, types 12-14).
    fn wrap(&mut self, (x0, x1, y0, y1): (i32, i32, i32, i32)) {
        if self.pos.x < x0 {
            self.pos.x += x1 - x0;
        } else if self.pos.x > x1 {
            self.pos.x -= x1 - x0;
        }
        if self.pos.y < y0 {
            self.pos.y += y1 - y0;
        } else if self.pos.y > y1 {
            self.pos.y -= y1 - y0;
        }
    }

    /// Removal test after the update.
    fn expired(&self) -> bool {
        if self.kind == 5 {
            self.pos.z <= BUBBLE_SURFACE_Z
        } else if self.kind == 12 && self.pos.z > SNOW_GROUND_Z {
            true
        } else {
            self.pos.z > Z_LIMIT || self.size < 1
        }
    }
}

/// `rand(2h) − h` on each axis, drawn z, y, x: the original evaluates
/// `AddEffect`'s arguments last first (Mac `UpdateEffects` @ 100aa1f0 and
/// the 68k build; only OS4 `FUN_010386e8` draws x, y, z), so every spawn in
/// the updater draws its last argument first.
fn jitter(rand: &mut impl FnMut(i32) -> i32, half: i32) -> IVec3 {
    let z = rand(2 * half) - half;
    let y = rand(2 * half) - half;
    let x = rand(2 * half) - half;
    IVec3::new(x, y, z)
}

/// All live effects.
#[derive(Resource, Default)]
pub struct Effects {
    lists: [Vec<Effect>; LAYERS],
    /// Camera for the weather wrap/respawn, set before each tick.
    view: Option<View>,
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
    pub fn clear(&mut self) {
        self.lists.iter_mut().for_each(Vec::clear);
    }

    fn is_empty(&self) -> bool {
        self.lists.iter().all(Vec::is_empty)
    }

    /// Records that light the city under Gouraud shading (`FUN_01036574`):
    /// (type, position, size) of fireballs (2), explosions (7), explosion
    /// flares (11) and fires (15).
    pub fn light_sources(&self) -> impl Iterator<Item = (u8, Vec3, i32)> + '_ {
        self.lists
            .iter()
            .flatten()
            .filter(|e| matches!(e.kind, 2 | 7 | 0xb | 0xf))
            .map(|e| (e.kind, e.pos.as_vec3(), e.size))
    }

    /// Advance every effect one game tick. Lists run from last to first;
    /// spawned records are appended, and a removed record is replaced by the
    /// last one (`RemoveEffect`, OS4 `FUN_01035f48`), so neither is updated
    /// again this tick.
    #[cfg(test)]
    fn tick(&mut self, rand: &mut impl FnMut(i32) -> i32) {
        self.tick_with(rand, &|_, _| None);
    }

    /// [`Effects::tick`] with the ground height (z) under an (x, y), for
    /// settling snow.
    fn tick_with(
        &mut self,
        rand: &mut impl FnMut(i32) -> i32,
        ground: &dyn Fn(i32, i32) -> Option<i32>,
    ) {
        let view = self.view;
        for layer in 0..LAYERS {
            for i in (0..self.lists[layer].len()).rev() {
                let list = &mut self.lists[layer];
                let mut e = list[i];
                let spawned = e.update(rand);
                if let (12..=14, Some(v)) = (e.kind, view) {
                    e.wrap(v.rect());
                }
                list[i] = e;
                if let Some(child) = spawned
                    && list.len() < MAX_PER_LAYER
                {
                    list.push(child);
                }
                if e.expired() {
                    list.swap_remove(i);
                    if let Some(v) = view {
                        self.respawn_weather(&e, v, rand, ground);
                    }
                }
            }
        }
    }

    fn push(&mut self, layer: usize, e: Effect) {
        let list = &mut self.lists[layer];
        if list.len() < MAX_PER_LAYER {
            list.push(e);
        }
    }

    /// A snowflake or raindrop reached the ground (`UpdateEffects` @
    /// 100aa1f0, OS4 `FUN_010386e8`): snow leaves a mark (type 8 Dust,
    /// additive, list 0) and a new flake starts at the camera; rain starts a
    /// new drop and splashes 3 times (type 14). New records go to list 3.
    /// Values are the call-site ones, /3 on entry; the arguments are drawn
    /// last first, as the Mac and 68k builds evaluate them (OS4: first to
    /// last).
    fn respawn_weather(
        &mut self,
        e: &Effect,
        v: View,
        rand: &mut impl FnMut(i32) -> i32,
        ground: &dyn Fn(i32, i32) -> Option<i32>,
    ) {
        // New records start at the camera plane: z = 0x100 - camH*16.
        let top = 0x100 - v.h * 16;
        match e.kind {
            12 => {
                // The Mac puts the mark at street level (z 19904) and draws
                // list 0 after the ground tops; here it sits on the ground.
                let size = rand(50) + 200;
                let z = ground(e.pos.x, e.pos.y).unwrap_or(SNOW_GROUND_Z);
                let pos = IVec3::new(e.pos.x, e.pos.y, z);
                self.push(
                    0,
                    Effect::new(8, Tex::Dust, 0, pos, IVec3::ZERO, size, Blend::Add),
                );
                let size = rand(50) + 100;
                let vz = rand(100) + 200;
                let vy = rand(20) - 10;
                let vx = rand(20) + 10;
                let vel = IVec3::new(vx, vy, vz);
                let y = rand(0xa000) + v.y - 0x5000;
                let x = rand(0x10000) + v.x - 0x8000;
                let flake = Effect::new(
                    12,
                    Tex::Dust,
                    0,
                    IVec3::new(x, y, top),
                    vel,
                    size,
                    Blend::Add,
                );
                self.push(3, flake);
            }
            13 => {
                let vz = rand(200) + 1000;
                let vy = rand(20) - 10;
                let vx = rand(30) + 50;
                let y = rand(0x5000) + v.y - 0x2800;
                let x = rand(0x8000) + v.x - 0x4000;
                let vel = IVec3::new(vx, vy, vz);
                let drop = Effect::new(13, Tex::Dust, 0, IVec3::new(x, y, top), vel, 1, Blend::Add);
                self.push(3, drop);
                for _ in 0..3 {
                    let vz = -200 - rand(200);
                    let vy = rand(100) - 50;
                    let vx = rand(100) - 50;
                    let vel = IVec3::new(vx, vy, vz);
                    let pos = IVec3::new(e.pos.x, e.pos.y, SPLASH_Z);
                    self.push(3, Effect::new(14, Tex::Dust, 0, pos, vel, 1, Blend::Add));
                }
            }
            _ => {}
        }
    }
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/effects.wgsl");
        app.add_plugins(MaterialPlugin::<EffectMaterial>::default())
            .init_resource::<Effects>()
            .init_resource::<FxMode>()
            .init_resource::<ScreenFlash>()
            .init_resource::<Flares>()
            .add_systems(FixedPreUpdate, update_effects)
            .add_systems(
                OnExit(crate::level::AppState::Playing),
                |mut fx: ResMut<Effects>, mut flash: ResMut<ScreenFlash>| {
                    fx.clear();
                    flash.0 = 0;
                },
            )
            .configure_sets(PostUpdate, EffectsDraw.after(TransformSystems::Propagate))
            .add_systems(PostUpdate, (sync_blood, sync_mode, build_meshes).chain().in_set(EffectsDraw));
    }
}

/// One tick of the effect lists. Their random numbers come from the
/// cosmetic generator (`random3` in Mac `UpdateEffects` @ 100aa1f0), so
/// effects never move the game's sequence.
pub(crate) fn update_effects(
    mut effects: ResMut<Effects>,
    mut flash: ResMut<ScreenFlash>,
    rng: Option<ResMut<VisualRng>>,
    eyes: Res<crate::camera::ViewEyes>,
    rotating: Res<RotatingView>,
    city: Option<Res<City>>,
) {
    // The flash decays x3/4 per tick before anything adds to it.
    flash.0 = flash.0 * 3 >> 2;
    let Some(mut rng) = rng else { return };
    // Wrapped around view 0's eye (rotating view, replay TV camera).
    let cam = eyes.cams.first().copied().unwrap_or_default();
    effects.view = Some(View::of(&cam, rotating.0));
    if !effects.is_empty() {
        // Top of the ground cube under (x, y), as an object z.
        let ground = |x: i32, y: i32| {
            let city = city.as_ref()?;
            let (sx, sy, fx, fy) = City::locate(x as f32, y as f32)?;
            let c = city.square_cubes(sx, sy)[0];
            c.solid.then(|| z_of_h(c.top_at(fx, fy)) as i32)
        };
        effects.tick_with(&mut |n| rng.rand(n), &ground);
    }
}

// ---------------------------------------------------------------------------
// Rendering

const TILE: usize = 64;
const ATLAS_COLS: usize = 8;
const ATLAS_ROWS: usize = 7;
const ATLAS_W: usize = TILE * ATLAS_COLS;
const ATLAS_H: usize = TILE * ATLAS_ROWS;

/// Atlas tiles, in order. Quartered sprites (`<set>AB`: A = 0 left / 1
/// right, B = 0 top / 1 bottom) occupy 4 tiles at `first + 2A + B`. The
/// empty name is a solid white tile.
const FILES: [&str; 48] = [
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
    "",
    "LensFlare.1100",
    "LensFlare.1101",
    "LensFlare.1110",
    "LensFlare.1111",
    "LensFlare.1200",
    "LensFlare.1201",
    "LensFlare.1210",
    "LensFlare.1211",
    "LensFlare.1",
    "LensFlare.3_4",
    "LensFlare.5",
    "LensFlare.6_7",
    "LensFlare.8",
    "LensFlare.9",
    "LensFlare.10",
    "Blood.dark",
];
const FLAME_TILE: usize = 2;
const SHOCKWAVE_TILE: usize = 11;
const LENS_GLOW_TILE: usize = 27;
/// Solid white: rain streaks, splashes and the screen flash.
const SOLID_TILE: usize = 31;
/// LensFlare.11xx and .12xx rings (4 quarters each).
const LENS_RING_TILE: usize = 32;
const LENS_HALO_TILE: usize = 36;
/// LensFlare.1, .3_4, .5, .6_7, .8, .9, .10.
const GHOST_TILE: usize = 40;
/// `Blood` and its night replacement (`FUN_01035250`).
const BLOOD_TILE: usize = 6;
const BLOOD_DARK_TILE: usize = 47;
/// Hardware atlas only: `Dust` loaded black with alpha = max(R, G, B)
/// (flag `DAT_0114c3cc`, `FUN_0104feb0`), the smoke sprite.
const SMOKE_HW_TILE: usize = 48;

/// Full-mode flare elements (`FUN_01034804`): tile, position factor f (the
/// element sits at `c + f·(p − c)`, c = screen centre, p = flare) and
/// half-size divisor (half = r/div base-screen pixels, r = size >> 4).
const GHOSTS: [(usize, f32, f32); 9] = [
    (GHOST_TILE, 1.5, 6.0),
    (GHOST_TILE + 1, 0.75, 12.0),
    (GHOST_TILE + 1, 2.0 / 3.0, 7.0),
    (GHOST_TILE + 2, 0.5, 9.0),
    (GHOST_TILE + 3, 1.0 / 3.0, 40.0),
    (GHOST_TILE + 3, 0.1, 30.0),
    (GHOST_TILE + 4, -0.1, 4.0),
    (GHOST_TILE + 5, -1.0 / 6.0, 9.0),
    (GHOST_TILE + 6, -1.0 / 3.0, 6.0),
];

/// Rain streak and splash colour: palette colour 0x67 added through table 0
/// (`DAT_0114c440 + 0x6700`).
const RAIN_COLOUR: usize = 0x67;
/// Hardware rain: white lines and points blended at this alpha.
const RAIN_HW_ALPHA: f32 = 0.4;

/// Render batch of a sprite: one of the blends, or additive without depth
/// test (lens flares, weather, the screen flash: the original draws flares
/// at a fixed screen size after the world, and list 3 after every layer).
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
    /// Type 13 rain: a 1-pixel line from the previous position.
    Line,
    /// Type 14 splash: one pixel.
    Dot,
    /// Type 11: a lens flare at the centre (`AddFlare(x, y, −size)`).
    Flare,
}

impl Effect {
    /// Sprite, world half-size and pass.
    fn sprite(&self) -> (Sprite, f32, Pass) {
        let size = self.size as f32;
        let sprite = match (self.kind, self.tex) {
            (10, _) => Sprite::Quarters(SHOCKWAVE_TILE + 4 * self.frame as usize),
            (11, _) => Sprite::Flare,
            (12, _) => return (Sprite::Single(0), size, Pass::Flare),
            (13, _) => return (Sprite::Line, size, Pass::Flare),
            (14, _) => return (Sprite::Dot, size, Pass::Flare),
            (_, Tex::Shockwave) => Sprite::Quarters(SHOCKWAVE_TILE),
            (_, Tex::LensFlare) => Sprite::Quarters(LENS_GLOW_TILE),
            (_, Tex::Dust | Tex::Dust2) => Sprite::Single(0),
            (_, Tex::Flare) => Sprite::Single(1),
            (_, Tex::Flame) => Sprite::Single(FLAME_TILE + self.frame as usize),
            (_, Tex::Blood) => Sprite::Single(BLOOD_TILE),
            (_, Tex::Gas) => Sprite::Single(7),
            (_, Tex::Dirt) => Sprite::Single(8),
            (_, Tex::DirtSkidmark) => Sprite::Single(9),
            (_, Tex::Shad) => Sprite::Single(10),
        };
        (sprite, size, Pass::Blend(self.blend))
    }
}

/// How the hardware renderer converts an effect texture to a W3D texture
/// (`FUN_010024fc` flags set around the loads in `FUN_0104feb0`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HwFormat {
    /// No flag: A1R5G5B5, colour 0 transparent.
    Keyed,
    /// `DAT_0114c3cc`: black, alpha = max(R, G, B) (smoke, shadows).
    Dark,
    /// `DAT_0114c3c8`: every non-zero channel 255, alpha = max (blood).
    Saturated,
    /// `DAT_0114c3c6`: pure green, alpha = max (gas).
    Green,
    /// `DAT_0114c3c4`: the colours, alpha = max (dirt).
    Coverage,
}

fn hw_format(name: &str) -> HwFormat {
    match name {
        "Shad" => HwFormat::Dark,
        "Blood" | "Blood.dark" => HwFormat::Saturated,
        "Gas" => HwFormat::Green,
        "Dirt" | "DirtSkidmark" => HwFormat::Coverage,
        _ => HwFormat::Keyed,
    }
}

/// Atlas of [`FILES`]: palette RGB, alpha = max(R, G, B) (the tint table's
/// coverage) and 0 for texel 0. With `hardware`, the W3D conversions of
/// [`HwFormat`] (5-bit channels for the A1R5G5B5 ones), the smoke tile, and
/// bilinear filtering.
fn build_atlas(data: &GameData, hardware: bool, blood: BloodSlot) -> Image {
    let mut rgba = vec![0u8; ATLAS_W * ATLAS_H * 4];
    let files = FILES.iter().copied().chain(std::iter::once("Dust"));
    for (i, name) in files.enumerate() {
        let (name, mac) = blood.file(i, name, data);
        let format = match (hardware, i) {
            (false, _) => None,
            (true, SMOKE_HW_TILE) => Some(HwFormat::Dark),
            (true, _) => Some(hw_format(name)),
        };
        let (tx, ty) = (i % ATLAS_COLS * TILE, i / ATLAS_COLS * TILE);
        if name.is_empty() {
            for j in 0..TILE * TILE {
                let p = ((ty + j / TILE) * ATLAS_W + tx + j % TILE) * 4;
                rgba[p..p + 4].fill(255);
            }
            continue;
        }
        let texels = match data.read_variant(format!("EffectTextures/{name}"), mac) {
            Ok(b) if b.len() >= TILE * TILE => b,
            _ => {
                warn!("missing effect texture {name}");
                continue;
            }
        };
        for (j, &c) in texels[..TILE * TILE].iter().enumerate() {
            let p = ((ty + j / TILE) * ATLAS_W + tx + j % TILE) * 4;
            let c = c as usize;
            let rgb = [data.palette[c * 3], data.palette[c * 3 + 1], data.palette[c * 3 + 2]];
            let max = rgb[0].max(rgb[1]).max(rgb[2]);
            let five = |v: u8| (v & 0xf8) | (v >> 5);
            let (out, a) = match format {
                None => (rgb, if c == 0 { 0 } else { max }),
                Some(HwFormat::Keyed) => (rgb.map(five), if c == 0 { 0 } else { 255 }),
                Some(HwFormat::Dark) => ([0, 0, 0], max),
                Some(HwFormat::Saturated) => (rgb.map(|v| if v != 0 { 255 } else { 0 }), max),
                Some(HwFormat::Green) => ([0, 255, 0], max),
                Some(HwFormat::Coverage) => (rgb, max),
            };
            rgba[p..p + 3].copy_from_slice(&out);
            rgba[p + 3] = a;
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
    image.sampler = ImageSampler::Descriptor(if hardware {
        ImageSamplerDescriptor::linear()
    } else {
        ImageSamplerDescriptor::nearest()
    });
    image
}

/// Effect sprite material: the atlas through one Alpha.256-like blend
/// (`shaders/effects.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(MaterialKey)]
struct EffectMaterial {
    #[texture(0)]
    #[sampler(1)]
    atlas: Handle<Image>,
    pass: Pass,
    /// Transparent-pass sort bias: later layers and passes draw on top.
    order: f32,
    /// Warp3D blend modes instead of the Alpha.256 operations.
    hardware: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MaterialKey {
    pass: Pass,
    hardware: bool,
}

impl From<&EffectMaterial> for MaterialKey {
    fn from(material: &EffectMaterial) -> Self {
        MaterialKey {
            pass: material.pass,
            hardware: material.hardware,
        }
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
        let MaterialKey { pass, hardware } = key.bind_group_data;
        let alpha_blend = component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha, add);
        let color = match pass {
            // W3D_SetBlendMode(SRC_ALPHA, ONE_MINUS_SRC_ALPHA) for tables 1
            // and 3, ONE/ONE for the others.
            Pass::Blend(Blend::Subtract | Blend::Tint) if hardware => alpha_blend,
            // Flares and snow ONE/ONE, rain (7, 8) at alpha 0.4: the shader
            // premultiplies, so ONE / ONE_MINUS_SRC_ALPHA does both (additive
            // sprites write alpha 0).
            Pass::Flare if hardware => component(one, BlendFactor::OneMinusSrcAlpha, add),
            _ if hardware => component(one, one, add),
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
            if hardware {
                fragment.shader_defs.push("EFFECT_HARDWARE".into());
                if pass == Pass::Flare {
                    fragment.shader_defs.push("EFFECT_HW_OVERLAY".into());
                }
            } else if pass == Pass::Blend(Blend::Average) {
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

/// One mesh entity per (view, layer, pass), on the view's render layer
/// (`crate::camera::view_layer`): sprites face each view's camera and
/// flares sit on its screen, as the original draws every view's effects
/// and flares in its own pass (`FUN_0105bd98` → `FUN_01049374`,
/// `FUN_01034804`).
#[derive(Component)]
struct EffectBatch {
    view: usize,
    layer: usize,
    pass: Pass,
}

/// Materials by `layer * Pass::ORDER.len() + pass rank`.
#[derive(Resource)]
struct BatchMaterials(Vec<Handle<EffectMaterial>>);

/// The software and the hardware atlas.
#[derive(Resource)]
struct Atlases {
    software: Handle<Image>,
    hardware: Handle<Image>,
}

/// Linear RGB of the rain colour.
#[derive(Resource)]
struct RainColour([f32; 4]);

/// Start-up stage (`startup.rs`): the effect textures and batches.
pub fn load(world: &mut World) {
    use bevy::ecs::system::RunSystemOnce;
    if let Err(e) = world.run_system_once(setup) {
        error!("effects: {e}");
    }
}

fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<EffectMaterial>>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    language: Option<Res<crate::hud::Language>>,
) {
    let blood = BloodSlot::new(settings.as_deref(), language.as_deref());
    commands.insert_resource(blood);
    let atlas = images.add(build_atlas(&data, false, blood));
    let mut handles = Vec::new();
    for layer in 0..LAYERS {
        for pass in Pass::ORDER {
            handles.push(materials.add(EffectMaterial {
                atlas: atlas.clone(),
                pass,
                order: 1.0e4 * (1 + layer * Pass::ORDER.len() + pass.rank()) as f32,
                hardware: false,
            }));
        }
    }
    spawn_batches(&mut commands, &handles, 0);
    commands.insert_resource(BatchMaterials(handles));
    commands.insert_resource(Atlases {
        software: atlas,
        hardware: images.add(build_atlas(&data, true, blood)),
    });
    let rgb = &data.palette[RAIN_COLOUR * 3..RAIN_COLOUR * 3 + 3];
    let c = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear();
    commands.insert_resource(RainColour([c.red, c.green, c.blue, 1.0]));
}

/// What the blood slot (atlas tiles `BLOOD_TILE` and its night
/// replacement `BLOOD_DARK_TILE`) shows. `mac` (`Variants::blood`): the Mac
/// `Blood`, smaller and darker, day and night: the Mac ships no
/// `Blood.dark` and `InitAll` @ 100632a0 loads only `Blood`. Otherwise (or
/// without Mac data) the Amiga `Blood` with `Blood.dark` at night
/// (`FUN_01035250`). `gas` (`Variants::gas` with the language Deutsch):
/// `Gas` instead, day and night, as the Mac `InitAll` and `DoSetLanguage`
/// @ 1008c2b0 load it for German (Amiga `FUN_0104feb0` always loads
/// `Blood`).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BloodSlot {
    pub mac: bool,
    pub gas: bool,
}

impl BloodSlot {
    fn new(settings: Option<&crate::frontend::progress::Settings>, language: Option<&crate::hud::Language>) -> Self {
        let variants = settings.map(|s| s.variants).unwrap_or_default();
        BloodSlot {
            mac: variants.blood,
            gas: variants.gas && language.is_some_and(|l| crate::data::same_language(&l.0, "Deutsch")),
        }
    }

    /// The file for atlas tile `tile` (listed as `name`), and whether to
    /// read the Mac copy.
    fn file(self, tile: usize, name: &'static str, data: &GameData) -> (&'static str, bool) {
        match tile {
            BLOOD_TILE | BLOOD_DARK_TILE if self.gas => ("Gas", false),
            BLOOD_TILE => ("Blood", self.mac),
            BLOOD_DARK_TILE if self.mac && data.mac_path("EffectTextures/Blood").is_some() => {
                ("Blood", true)
            }
            _ => (name, false),
        }
    }
}

/// Rebuild the atlases when the blood or gas setting or the language
/// changes (the Mac `DoSetLanguage` reloads the slot).
fn sync_blood(
    data: Res<GameData>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    language: Option<Res<crate::hud::Language>>,
    current: Option<ResMut<BloodSlot>>,
    atlases: Option<Res<Atlases>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(mut current), Some(atlases)) = (current, atlases) else { return };
    if !settings.as_ref().is_some_and(|s| s.is_changed()) && !language.as_ref().is_some_and(|l| l.is_changed()) {
        return;
    }
    let blood = BloodSlot::new(settings.as_deref(), language.as_deref());
    if blood == *current {
        return;
    }
    *current = blood;
    for (handle, hardware) in [(&atlases.software, false), (&atlases.hardware, true)] {
        if let Some(mut image) = images.get_mut(handle) {
            *image = build_atlas(&data, hardware, blood);
        }
    }
}

/// Follow the renderer (blend modes and atlas).
fn sync_mode(
    mode: Res<crate::render_mode::RenderMode>,
    batch: Option<Res<BatchMaterials>>,
    atlases: Option<Res<Atlases>>,
    mut materials: ResMut<Assets<EffectMaterial>>,
) {
    let (Some(batch), Some(atlases)) = (batch, atlases) else { return };
    let hardware = mode.hardware();
    let atlas = if hardware { &atlases.hardware } else { &atlases.software };
    for handle in &batch.0 {
        if materials.get(handle).is_some_and(|m| m.hardware != hardware)
            && let Some(mut m) = materials.get_mut(handle)
        {
            m.hardware = hardware;
            m.atlas = atlas.clone();
        }
    }
}

fn spawn_batches(commands: &mut Commands, materials: &[Handle<EffectMaterial>], view: usize) {
    for layer in 0..LAYERS {
        for pass in Pass::ORDER {
            let material = materials[layer * Pass::ORDER.len() + pass.rank()].clone();
            commands.spawn((
                EffectBatch { view, layer, pass },
                MeshMaterial3d(material),
                Transform::default(),
                Visibility::Hidden,
                NoFrustumCulling,
                RenderLayers::layer(view_layer(view)),
            ));
        }
    }
}

const WHITE: [f32; 4] = [1.0; 4];

/// Quads of one batch, in render space.
#[derive(Default)]
struct Quads {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Quads {
    /// A quad with corners top-left, top-right, bottom-right, bottom-left
    /// of atlas tile `tile`, multiplied by `color`.
    fn push_corners(&mut self, corners: [Vec3; 4], tile: usize, mirror: bool, color: [f32; 4]) {
        const INSET: f32 = 0.01;
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
        for (corner, (i, j)) in corners.into_iter().zip([(0, 0), (1, 0), (1, 1), (0, 1)]) {
            self.positions.push(corner.to_array());
            self.uvs.push([u[i], v[j]]);
            self.colors.push(color);
        }
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|k| base + k));
    }

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
        let (right, down) = axes;
        let at = |i: usize, j: usize| centre + right * x[i] + down * y[j];
        let corners = [at(0, 0), at(1, 0), at(1, 1), at(0, 1)];
        self.push_corners(corners, tile, mirror, WHITE);
    }

    /// Emit the sprite of `e` (`FUN_010386e8` draw part).
    fn effect(&mut self, view: &ViewInfo, e: &Effect, sprite: Sprite, half: f32, rain: [f32; 4]) {
        let axes = view.axes;
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
            Sprite::Line => {
                let from = to_render((e.pos - e.vel).as_vec3()) + Vec3::Y * NEARER;
                self.line(view, from, centre, rain);
            }
            Sprite::Dot => {
                let w = view.pixel(centre) * 0.5;
                if w > 0.0 {
                    let (r, d) = axes;
                    let corners = [
                        centre - r * w - d * w,
                        centre + r * w - d * w,
                        centre + r * w + d * w,
                        centre - r * w + d * w,
                    ];
                    self.push_corners(corners, SOLID_TILE, false, rain);
                }
            }
            Sprite::Flare => {}
        }
    }

    /// A one-pixel-wide line from `a` to `b`, facing the camera.
    fn line(&mut self, view: &ViewInfo, a: Vec3, b: Vec3, color: [f32; 4]) {
        let mid = (a + b) * 0.5;
        let w = view.pixel(mid) * 0.5;
        if w <= 0.0 {
            return;
        }
        let side = (b - a).cross(mid - view.pos).normalize_or(view.axes.0) * w;
        self.push_corners(
            [a - side, a + side, b + side, b - side],
            SOLID_TILE,
            false,
            color,
        );
    }

    fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// The game camera, for screen-aligned geometry.
struct ViewInfo {
    pos: Vec3,
    forward: Vec3,
    /// Sprite axes: right and down the screen, horizontal.
    axes: (Vec3, Vec3),
    /// tan of half the vertical field of view.
    tan_half: f32,
}

impl ViewInfo {
    /// Size of one base-screen pixel (1/256 of the screen height) at the
    /// depth of `p`; 0 behind the camera.
    fn pixel(&self, p: Vec3) -> f32 {
        let depth = (p - self.pos).dot(self.forward);
        (depth * 2.0 * self.tan_half / SCREEN_H as f32).max(0.0)
    }
}

/// Screen-space quads (lens flares, the flash) on a plane just in front of
/// the camera. Positions are NDC; sizes are base-screen pixels, kept
/// square.
struct Screen<'a> {
    camera: &'a Camera,
    transform: &'a GlobalTransform,
    /// NDC per base-screen pixel, (x, y).
    px: Vec2,
}

impl Screen<'_> {
    /// NDC depth of the overlay plane (reverse Z: twice the near distance).
    const DEPTH: f32 = 0.5;

    fn world(&self, ndc: Vec2) -> Option<Vec3> {
        self.camera
            .ndc_to_world(self.transform, ndc.extend(Self::DEPTH))
    }

    fn quad(&self, q: &mut Quads, min: Vec2, max: Vec2, tile: usize, color: [f32; 4]) {
        let corners = [Vec2::new(min.x, max.y), max, Vec2::new(max.x, min.y), min];
        let mut world = [Vec3::ZERO; 4];
        for (w, c) in world.iter_mut().zip(corners) {
            let Some(p) = self.world(c) else { return };
            *w = p;
        }
        q.push_corners(world, tile, false, color);
    }

    /// A single sprite of half-size `half` pixels at `c` (not mirrored:
    /// `FUN_01033d50(tex, 1, …)`).
    fn single(&self, q: &mut Quads, c: Vec2, half: f32, tile: usize, color: [f32; 4]) {
        let h = self.px * half;
        self.quad(q, c - h, c + h, tile, color);
    }

    /// Four quarters meeting at `c` (`FUN_010345ec`).
    fn quarters(&self, q: &mut Quads, c: Vec2, half: f32, first: usize, color: [f32; 4]) {
        let h = self.px * half;
        for a in 0..2 {
            for b in 0..2 {
                // b = 0 is the top half: +y in NDC.
                let x0 = c.x + (a as f32 - 1.0) * h.x;
                let y0 = c.y - b as f32 * h.y;
                let min = Vec2::new(x0, y0);
                let max = Vec2::new(x0 + h.x, y0 + h.y);
                self.quad(q, min, max, first + 2 * a + b, color);
            }
        }
    }

    /// One flare (`FUN_01034804`): radius `r` pixels at NDC `p`.
    fn flare(&self, q: &mut Quads, p: Vec2, r: f32, color: [f32; 4], full: bool) {
        if full {
            for (tile, f, div) in GHOSTS {
                self.single(q, p * f, r / div, tile, color);
            }
            self.quarters(q, p * (-2.0 / 3.0), r / 2.0, LENS_RING_TILE, color);
            self.quarters(q, -p, r, LENS_HALO_TILE, color);
        }
        self.quarters(q, p, 2.0 * r / 3.0, LENS_GLOW_TILE, color);
    }
}

/// Colour mask of the Alpha.256 table a flare is drawn through: tables 5–8
/// add red / red+green / green / blue only (rendering.md §3).
fn table_colour(table: u8) -> [f32; 4] {
    match table {
        5 => [1.0, 0.0, 0.0, 1.0],
        6 => [1.0, 1.0, 0.0, 1.0],
        7 => [0.0, 1.0, 0.0, 1.0],
        8 => [0.0, 0.0, 1.0, 1.0],
        _ => WHITE,
    }
}

/// Rebuild every view's batch meshes from the effect lists and the
/// frame's flares. (The palette flash is applied by the views' final pass,
/// `crate::camera::postfx`.)
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_meshes(
    mut commands: Commands,
    effects: Res<Effects>,
    mode: Res<FxMode>,
    rain: Option<Res<RainColour>>,
    materials: Option<Res<BatchMaterials>>,
    mut flares: ResMut<Flares>,
    cameras: Query<(
        &Camera,
        &GlobalTransform,
        &Projection,
        Option<&ExtraView>,
        Has<GameCamera>,
    )>,
    mut batches: Query<(Entity, &EffectBatch, Option<&Mesh3d>, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    city: Option<Res<City>>,
    render_mode: Res<crate::render_mode::RenderMode>,
) {
    // Night maps draw `Blood.dark` instead of `Blood` (`FUN_01035250`).
    let night = city.as_ref().is_some_and(|c| c.map.night);
    let hardware = render_mode.hardware();
    let mut sources = std::mem::take(&mut flares.0);
    // Warp3D: white at alpha 0.4 under W3D_SetBlendMode(7, 8), i.e.
    // SRC_ALPHA / ONE_MINUS_SRC_ALPHA (OS4 `FUN_010360fc` DrawLine /
    // DrawPoint; Mac `UpdateEffects` @ 100aa1f0 with the colour from the
    // float pool, 0.4). A vertex alpha below 1 marks it for the hardware
    // overlay batch, which blends it instead of adding.
    let rain = if hardware {
        [1.0, 1.0, 1.0, RAIN_HW_ALPHA]
    } else {
        rain.map_or(WHITE, |r| r.0)
    };
    // Type 11 records are flares too (AddFlare with −size: never occluded).
    for list in &effects.lists {
        for e in list {
            if e.kind == 11 && sources.len() < MAX_FLARES {
                sources.push(FlareSource {
                    pos: to_render(e.pos.as_vec3()) + Vec3::Y * NEARER,
                    size: e.size,
                    table: 0,
                    occlude: false,
                });
            }
        }
    }

    let views: Vec<(usize, &Camera, &GlobalTransform, &Projection)> = cameras
        .iter()
        .filter_map(|(cam, tf, projection, extra, primary)| {
            let view = match (extra, primary) {
                (Some(v), _) => v.0 as usize,
                (None, true) => 0,
                (None, false) => return None,
            };
            cam.is_active.then_some((view, cam, tf, projection))
        })
        .collect();
    // Batches for views that have none yet (split screen).
    if let Some(materials) = &materials {
        for &(view, ..) in &views {
            if !batches.iter().any(|(_, b, ..)| b.view == view) {
                spawn_batches(&mut commands, &materials.0, view);
            }
        }
    }

    let mut built: HashMap<usize, [[Quads; Pass::ORDER.len()]; LAYERS]> = HashMap::new();
    for &(view_index, cam, cam_tf, projection) in &views {
        // Sprites face up, with their top towards the top of the screen.
        let right = cam_tf.right().with_y(0.0).normalize_or(Vec3::X);
        let tan_half = match projection {
            Projection::Perspective(p) => (p.fov * 0.5).tan(),
            _ => 0.5,
        };
        let view = ViewInfo {
            pos: cam_tf.translation(),
            forward: cam_tf.forward().into(),
            axes: (right, right.cross(Vec3::Y)),
            tan_half,
        };
        let mut quads: [[Quads; Pass::ORDER.len()]; LAYERS] = Default::default();
        for (layer, list) in effects.lists.iter().enumerate() {
            for e in list {
                let (mut sprite, half, pass) = e.sprite();
                if night && sprite == Sprite::Single(BLOOD_TILE) {
                    sprite = Sprite::Single(BLOOD_DARK_TILE);
                }
                if hardware && e.tex == Tex::Dust2 && sprite == Sprite::Single(0) {
                    sprite = Sprite::Single(SMOKE_HW_TILE);
                }
                if sprite == Sprite::Flare {
                    continue;
                }
                quads[layer][pass.rank()].effect(&view, e, sprite, half, rain);
            }
        }

        // Flares go last, in the top list's additive batch; the original
        // keeps at most 199 per view and tests whether each one's centre
        // pixel was overdrawn (here: whether the city is in the way).
        let top = &mut quads[LAYERS - 1][Pass::Flare.rank()];
        let aspect = cam
            .logical_viewport_size()
            .map_or(1.6, |s| s.x / s.y.max(1.0));
        let ky = 2.0 / SCREEN_H as f32;
        let screen = Screen {
            camera: cam,
            transform: cam_tf,
            px: Vec2::new(ky / aspect, ky),
        };
        if mode.0 > 0 {
            let eye = cam_tf.translation();
            let mut drawn = 0;
            for f in &sources {
                if drawn >= MAX_FLARES {
                    break;
                }
                let Some(ndc) = cam.world_to_ndc(cam_tf, f.pos) else {
                    continue;
                };
                let on_screen = ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0;
                if !on_screen || ndc.z <= 0.0 || ndc.z > 1.0 {
                    continue;
                }
                // Warp3D draws every flare (no overdraw test).
                if f.occlude
                    && !hardware
                    && let Some(city) = &city
                    && crate::objects::occluded(city, f.pos, eye)
                {
                    continue;
                }
                drawn += 1;
                let r = (f.size >> 4) as f32;
                screen.flare(top, ndc.truncate(), r, table_colour(f.table), mode.0 >= 2);
            }
        }
        built.insert(view_index, quads);
    }

    for (entity, batch, mesh, mut visibility) in &mut batches {
        let q = built
            .get_mut(&batch.view)
            .map(|quads| std::mem::take(&mut quads[batch.layer][batch.pass.rank()]))
            .unwrap_or_default();
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

    #[test]
    fn weather_wraps_and_respawns() {
        let mut fx = Effects::default();
        fx.view = Some(View {
            x: 100_000,
            y: 100_000,
            h: 1000,
            half: (7, 6),
        });
        let (x0, x1, _, _) = fx.view.unwrap().rect();
        // A drop just past the right edge wraps to the left side.
        fx.add(
            13,
            Tex::Dust,
            Vec3::new(x1 as f32, 100_000.0, 0.0),
            Vec3::new(30.0, 0.0, 300.0),
            1.0,
            3,
            Blend::Add,
        );
        fx.tick(&mut never);
        assert!(fx.lists[3][0].pos.x < x0 + 100);
        // A drop hitting the ground becomes a new drop and 3 splashes.
        fx.lists[3][0].pos.z = 23_950;
        fx.tick(&mut never);
        let kinds: Vec<u8> = fx.lists[3].iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [13, 14, 14, 14]);
        assert_eq!(fx.lists[3][0].pos.z, 0x100 - 16_000);
        // Snow leaves a mark in list 0.
        let mut fx = Effects::default();
        fx.view = Some(View {
            x: 0,
            y: 0,
            h: 1000,
            half: (7, 6),
        });
        // It dies at street level (z > 0x4dc0), not at 24000.
        fx.add(
            12,
            Tex::Dust,
            Vec3::new(0.0, 0.0, 19_900.0),
            Vec3::new(0.0, 0.0, 300.0),
            120.0,
            3,
            Blend::Add,
        );
        fx.tick(&mut never);
        assert_eq!((fx.lists[0][0].kind, fx.lists[3][0].kind), (8, 12));
    }
}
