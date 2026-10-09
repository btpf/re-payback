//! The 3-D direction arrows and the checkpoint marker (docs/re/rendering.md
//! §"Direction arrows").
//!
//! * **Seek target** (Mac `SetSeek__Fiii` @ 100648e0, 68k `FUN_000eff2a`
//!   → `0x34c288/94/90`): every trigger that has an arrow pushes a point
//!   (x, y, floor) while `CheckForTriggers` @ 100f4a50 (68k `0xa4a68`)
//!   walks the table; the target is one of them, the next one every 20
//!   ticks, round-robin. None when nothing was pushed.
//! * **Red arrow** (`amiga_levelloop` draw block, Mac ~28513; 68k
//!   `0xe1a90..0xe1b32`): `objects/misc/arrow_red.obj`, one triangle 100..148
//!   model units in front of its origin, placed at the followed object
//!   (the vehicle when driving) and turned to `GetAngle(obj − target)`, i.e.
//!   pointing at the target; drawn after the map with `DrawObject(…, 100)`.
//! * **Deathmatch arrows**: in each view, one per other live player in that
//!   player's colour (red, green, yellow, blue), pointing at them.
//! * **White arrow**: with every cube layer drawn, a white arrowhead centred
//!   on the followed object, along its heading, when a cube with a top face
//!   is over it (the player is under a roof).
//! * **Checkpoint marker** (`DrawCheckpoint__Fiiiiii` @ 10095240): with
//!   `ShowCheckpointMarker` on, the flashing `Checkpoint.red/blue` icon at
//!   the target, drawn by the HUD from [`HudData::checkpoint_at`].
//!
//! None of this is drawn during the level-start camera drop (`0x23` ticks)
//! or replay playback. The arrows are render-only entities (not `Obj`); the
//! seek state is kept for drawing and is not part of the simulation.

use crate::camera::{CameraTarget, RotatingView, view_layer};
use crate::data::GameData;
use crate::hud::HudData;
use crate::level::{AppState, GameMode, LevelEntity};
use crate::mission::LevelClock;
use crate::mission::triggers::{Trigger, kind};
use crate::objects::{ObjectLibrary, ObjectMaterial, static_vertex};
use crate::sim::{Obj, heading_of};
use crate::world::{City, heading_to_render, object_to_render, stack_height, to_render, z_of_h};
use bevy::{
    camera::visibility::RenderLayers,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use payback_formats::map::{MAP_SIZE, Slope, TOP};
use std::collections::HashMap;

/// Ticks of the level-start camera drop (Mac `_DAT_1017c374 = 0x23` at
/// level start, counted down once per tick in `UpdateMapPosition__Fi`
/// @ 10068cb0); the arrows and the marker wait for it (`== 0`).
const INTRO_TICKS: u32 = 0x23;

/// Ticks each seek target is shown before the next (`0x13 < acc`, −0x14).
const SEEK_PERIOD: i32 = 20;

/// Arrow models (Mac `InitObjects` loads them into `_DAT_1017c414[0..4]` and
/// `_DAT_1017c40c`; 68k `0x34c0a8..b4`, `0x34c0c8`): player colours
/// red, green, yellow, blue, then the white one.
const MODELS: [&str; 5] = [
    "misc/arrow_red.obj",
    "misc/arrow_green.obj",
    "misc/arrow_yellow.obj",
    "misc/arrow_blue.obj",
    "misc/arrow_white.obj",
];
const WHITE: usize = 4;

pub struct ArrowPlugin;

impl Plugin for ArrowPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ArrowMaterial>::default())
            .init_resource::<Seek>()
            .add_systems(OnEnter(AppState::Playing), reset_seek)
            .add_systems(OnExit(AppState::Playing), reset_seek)
            .add_systems(Update, draw.run_if(in_state(AppState::Playing)));
    }
}

/// The seek target and the round-robin state (Mac `CheckForTriggers`:
/// arrays `_DAT_1017cfa8/a4/a0`, counters in the TOC; 68k `0x2c1008`,
/// `0x2c17d8`, `0x2c1fa8`, `0x1196f8/fc`). The arrays and counters are
/// never cleared, so they are kept across levels as in the original.
#[derive(Resource, Default)]
pub struct Seek {
    /// Points pushed so far (x, y, floor); entries beyond this tick's count
    /// keep older values.
    buf: Vec<IVec3>,
    /// Tick accumulator and current index.
    acc: i32,
    idx: usize,
    /// `0x34c288/94/90`: (x, y, floor), `None` = −1.
    pub target: Option<IVec3>,
    /// Phone number → (square centre, floor) of the phone triggers.
    phones: HashMap<i32, IVec3>,
}

/// Level start (68k `0xeea3a` path, `0x34c288 = -1`): no target.
fn reset_seek(mut seek: ResMut<Seek>, mut hud: ResMut<HudData>) {
    seek.target = None;
    seek.phones.clear();
    hud.target_pos = None;
    hud.checkpoint_at = None;
}

/// The point trigger `t` pushes onto the seek list, if any (Mac
/// `CheckForTriggers` @ 100f4a50, the stores into `_DAT_1017cfa8/a4/a0`;
/// exactly the types with an arrow, `Trigger::has_arrow`). `actor` is the
/// watched object's (position, floor) when `GetObject` finds it.
pub fn seek_point(world: &mut World, t: &Trigger, actor: Option<(IVec3, usize)>) -> Option<IVec3> {
    use kind::*;
    if !t.has_arrow() {
        return None;
    }
    match t.kind {
        // The object's x, y and floor (`+0x8b0`).
        ENTER_ACTOR | ACTOR_SEES_PLAYER | NEAR_ACTOR | NEAR_ACTOR_1000 | ACTOR_KILLED
        | ACTOR_ATTACKED => actor.map(|(p, level)| IVec3::new(p.x, p.y, level as i32)),
        // The trigger's own point.
        AT_ACTOR_ON_FOOT | AT_POINT | STOPPED_AT_POINT | NEAR_POINT_10000 | ON_FOOT_AT_POINT
        | AT_POINT_400 => Some(IVec3::new(t.a, t.b, t.c)),
        // The square's centre.
        SCENERY_GONE => Some(IVec3::new(t.a * 0x800 + 0x400, t.b * 0x800 + 0x400, t.c)),
        // The garage's square centre and floor (stored as floor + 1 here).
        GARAGE => Some(IVec3::new(t.a << 11 | 0x400, t.b << 11 | 0x400, t.c - 1)),
        // The phone's square (the Mac stores it when the trigger is added).
        PHONE => phone_point(world, t),
        _ => None,
    }
}

fn phone_point(world: &mut World, t: &Trigger) -> Option<IVec3> {
    if t.a == -1 {
        let p = crate::mission::Script::new(world).phone_square_pos(t.b as usize, t.c as usize, t.d as usize);
        return Some(IVec3::new(p.x, p.y, t.d));
    }
    if let Some(&p) = world.resource::<Seek>().phones.get(&t.a) {
        return Some(p);
    }
    let (pos, level) = crate::mission::Script::new(world).phone_coords(t.a)?;
    let p = IVec3::new(pos.x, pos.y, level as i32);
    world.resource_mut::<Seek>().phones.insert(t.a, p);
    Some(p)
}

/// End of the trigger pass (`ticks` game ticks): pick the target (Mac
/// `CheckForTriggers` tail; 68k `0xa6482..0xa6620`) and show it on the
/// minimap (`DisplayMinimap__Fiiiiii` @ 10091630 plots it when x ≥ 0).
pub fn seek_tick(world: &mut World, pushed: Vec<IVec3>, ticks: i32) {
    let mut seek = world.resource_mut::<Seek>();
    if pushed.is_empty() {
        seek.target = None;
    } else {
        let last = pushed.len() - 1;
        if seek.buf.len() < pushed.len() {
            seek.buf.resize(pushed.len(), IVec3::ZERO);
        }
        seek.buf[..pushed.len()].copy_from_slice(&pushed);
        seek.acc += ticks;
        if seek.acc > SEEK_PERIOD - 1 {
            seek.acc -= SEEK_PERIOD;
            seek.idx += 1;
            if seek.idx > last {
                seek.idx = 0;
            }
        }
        // The index may point past this tick's list until it next moves:
        // the original then reads the stale array entry.
        let i = seek.idx.min(seek.buf.len() - 1);
        seek.target = Some(seek.buf[i]);
    }
    let target = seek.target;
    world.resource_mut::<HudData>().target_pos = target.map(|t| t.truncate());
}

// ---- Drawing ---------------------------------------------------------------

/// The arrows' material: the object atlas through `objects.wgsl`, drawn
/// over everything (the original draws them after the map, and has no
/// z-buffer in either renderer, rendering.md §11).
pub type ArrowMaterial = ExtendedMaterial<StandardMaterial, OnTop>;

#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub struct OnTop {
    /// x = 1 for the hardware look (as `objects::EnvMapped`).
    #[uniform(100)]
    pub params: Vec4,
}

impl MaterialExtension for OnTop {
    fn vertex_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_compare = Some(CompareFunction::Always);
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// The arrow face is mode 4 (`Phone`, param 3). `DrawObject__FP6Objectiiis`
/// @ 10075880 is called with 100, which points the software mode-4 table at
/// `Alpha.256 + 0x40000` (50 % average) instead of table 0 (add); Warp3D
/// draws mode 4 with the default ONE/ONE blend whatever the argument.
fn blend(m: &mut ArrowMaterial, hardware: bool) {
    m.extension.params.x = if hardware { 1.0 } else { 0.0 };
    m.base.base_color = Color::srgba(1.0, 1.0, 1.0, if hardware { 1.0 } else { 0.5 });
    m.base.alpha_mode = if hardware { AlphaMode::Add } else { AlphaMode::Blend };
}

/// One arrow entity: view and model (`MODELS` index).
#[derive(Component)]
struct ArrowSprite {
    view: usize,
    model: usize,
}

/// Meshes and material, built once per level.
struct Kit {
    material: Handle<ArrowMaterial>,
    hardware: bool,
}

/// The mesh of an arrow model: its one face goes to the additive overlay of
/// [`ObjectLibrary::menu_parts`].
fn arrow_mesh(library: &mut ObjectLibrary, data: &GameData, meshes: &mut Assets<Mesh>, path: &str) -> Option<Handle<Mesh>> {
    let i = library.load(data, meshes, path)?;
    let obj = library.model(i).obj.clone();
    let (main, overlays) = library.menu_parts(&obj, static_vertex);
    let mesh = overlays.into_iter().next().map(|(m, _)| m).unwrap_or(main);
    Some(meshes.add(mesh))
}

/// What a view shows this frame: (model, object position, heading).
type Shown = Vec<(usize, IVec3, i32)>;

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    data: Res<GameData>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    object_materials: Res<Assets<ObjectMaterial>>,
    mut materials: ResMut<Assets<ArrowMaterial>>,
    mut kit: Local<Option<Kit>>,
    mut sprites: Query<(&ArrowSprite, &mut Transform, &mut Visibility)>,
    world: (
        Res<Seek>,
        Res<GameMode>,
        Option<Res<crate::multiplayer::Match>>,
        Res<crate::replay::ReplayState>,
        Res<LevelClock>,
        Option<Res<crate::mapedit::CityRender>>,
        Option<Res<City>>,
        Res<RotatingView>,
        Res<crate::render_mode::RenderMode>,
    ),
    followed: Query<&Obj, With<CameraTarget>>,
    mut hud: ResMut<HudData>,
) {
    let (seek, mode, game, replay, clock, render, city, rotating, render_mode) = world;
    let Some(mut library) = library else { return };
    let views = mode.players();
    // Spawn the arrows once per level (they are level entities).
    if sprites.is_empty() {
        let texture = object_materials
            .get(&library.material)
            .and_then(|m| m.base.base_color_texture.clone());
        let hardware = render_mode.hardware();
        let mut m = ArrowMaterial {
            base: StandardMaterial {
                base_color_texture: texture,
                unlit: true,
                cull_mode: Some(bevy::render::render_resource::Face::Back),
                // Sorted after every other blended pass.
                depth_bias: 1.0e7,
                ..default()
            },
            extension: OnTop::default(),
        };
        blend(&mut m, hardware);
        let material = materials.add(m);
        for (model, path) in MODELS.iter().enumerate() {
            let Some(mesh) = arrow_mesh(&mut library, &data, &mut meshes, path) else { continue };
            for view in 0..views {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    RenderLayers::layer(view_layer(view)),
                    ArrowSprite { view, model },
                    LevelEntity,
                ));
            }
        }
        *kit = Some(Kit { material, hardware });
        return;
    }
    if let Some(k) = kit.as_mut()
        && k.hardware != render_mode.hardware()
    {
        k.hardware = render_mode.hardware();
        if let Some(mut m) = materials.get_mut(&k.material) {
            blend(&mut m, k.hardware);
        }
    }

    // `_DAT_1017c374 == 0` (intro over) and not `_DAT_1017c594` (movie
    // playback); 68k `0x34c198 == 0`.
    let on = clock.0 >= INTRO_TICKS && !replay.playing();
    let mut shown: Vec<Shown> = vec![Vec::new(); views];
    hud.checkpoint_at = None;
    if on {
        let objs: Vec<&Obj> = followed.iter().collect();
        let view_obj = |v: usize| -> Option<&Obj> {
            if mode.is_multiplayer() {
                objs.iter().copied().find(|o| o.player == Some(v as u8))
            } else {
                objs.first().copied()
            }
        };
        for (v, list) in shown.iter_mut().enumerate() {
            let Some(me) = view_obj(v) else { continue };
            if !mode.is_multiplayer() {
                // Single player: the red arrow at the seek target.
                if let Some(t) = seek.target {
                    list.push((0, me.pos, heading_of(t.x - me.pos.x, t.y - me.pos.y)));
                    if hud.checkpoint
                        && let Some(city) = &city
                    {
                        hud.checkpoint_at = Some(marker_point(city, t, rotating.0));
                    }
                }
            } else {
                // Deathmatch: every other player present and not waiting to
                // respawn (`_DAT_1017c520[j] < 1`), in their colour.
                for j in 0..views.min(4) {
                    if j == v || game.as_ref().is_some_and(|g| g.respawn[j] > 0) {
                        continue;
                    }
                    let Some(other) = view_obj(j) else { continue };
                    list.push((j, me.pos, heading_of(other.pos.x - me.pos.x, other.pos.y - me.pos.y)));
                }
            }
            // The white arrow when a cube with a top face is over the
            // followed object and every layer is drawn (`_DAT_1017c5e4 == 3`).
            if render.as_ref().is_some_and(|r| r.visible_layers == 3)
                && let Some(city) = &city
                && roofed(city, me)
            {
                list.push((WHITE, me.pos, me.heading));
            }
        }
    }
    for (sprite, mut tf, mut vis) in &mut sprites {
        let hit = shown
            .get(sprite.view)
            .and_then(|l| l.iter().find(|s| s.0 == sprite.model));
        let want = if let Some(&(_, pos, heading)) = hit {
            tf.translation = object_to_render(pos.as_vec3());
            tf.rotation = heading_to_render(heading as f32);
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        vis.set_if_neq(want);
    }
}

/// A cube above `o`'s floor on its square with a cube (slope byte ≠ 0) and
/// a top face (type ≠ 0) (Mac ~28575; 68k `0xe1b9a`).
fn roofed(city: &City, o: &Obj) -> bool {
    let (sx, sy) = (o.pos.x >> 11, o.pos.y >> 11);
    if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) {
        return false;
    }
    let sq = city.map.square(sx as usize, sy as usize);
    (o.level + 1..3).any(|l| sq.cubes[l].slope != Slope::Off && sq.cubes[l].faces[TOP].visible())
}

/// Where `DrawCheckpoint__Fiiiiii` @ 10095240 projects the marker: the
/// target at the top of its cube (`GetCubeHeight__FP3Mapiii` @ 100ae7d0,
/// square and floor clamped to 0..127 and 0..3), with the objects' lift in
/// the top-down view (`camH·16 + z − 0x100`) and without it in the rotating
/// one.
fn marker_point(city: &City, t: IVec3, rotating: bool) -> Vec3 {
    let sx = (t.x >> 11).clamp(0, 127) as usize;
    let sy = (t.y >> 11).clamp(0, 127) as usize;
    let layer = t.z.clamp(0, 3) as usize;
    let cubes = city.square_cubes(sx, sy);
    let h = if layer < 3 {
        let g = &cubes[layer];
        g.base + g.corners.iter().copied().fold(f32::MIN, f32::max)
    } else {
        cubes[2].base + stack_height(&city.map.square(sx, sy).cubes[2])
    };
    let p = Vec3::new(t.x as f32, t.y as f32, z_of_h(h));
    if rotating { to_render(p) } else { object_to_render(p) }
}

