//! People: the player on foot and pedestrians (docs/re/people.md).
//!
//! Movement, gravity, collision, the walk animation and the pedestrian AI
//! follow the 68k `FUN_000e3a58` people branch. All rates are per game tick.

use super::collide;
use super::combat::{Arsenal, Panic};
use super::{Class, Obj, Rng, State, Stick, Trig, heading_of, wrap};
use crate::audio::{Listener, Sound, SoundPos, id};
use crate::camera::{CameraTarget, GameCam, ViewCams};
use crate::data::GameData;
use crate::level::{AppState, GameMode, LevelEntity, LevelLoaded, StartPoints};
use crate::objects::{ObjectLibrary, corner_positions};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use payback_formats::map::{self, FaceType, MAP_SIZE};
use payback_formats::{Motion, obj::Vertex};

/// Collision radius of every person: MaleNormal's max |X| / 3 (people.md §2.1).
pub const PERSON_RADIUS: i32 = 192;
/// Drowning depth (z) for people in water.
const DROWN_Z: i32 = 27000;
/// Ticks between player health regeneration steps.
pub const REGEN_TICKS: i32 = 250;
/// Respawn delay after death, ticks.
pub const RESPAWN_TICKS: i32 = 80;
/// Hard limit of the original object table.
pub const MAX_OBJECTS: usize = 32;

/// People model files: the player's model by weapon (`DAT_012a6600`) and
/// pedestrians. Index = the original table index.
pub const PEOPLE_MODELS: [&str; 16] = [
    "people/malenormal.obj",
    "people/malenormalbazooka.obj",
    "people/malenormalflamethrower.obj",
    "people/malenormalgrenade.obj",
    "people/malenormalminigun.obj",
    "people/malenormalshotgun.obj",
    "people/pedmale1.obj",
    "people/policemale1.obj",
    "people/pedmale2.obj",
    "people/pedmale3.obj",
    "people/pedmale4.obj",
    "people/pedmale5_shotgun.obj",
    "people/pedmale6_kkk.obj",
    "people/malenormalpistol.obj",
    "people/malenormalbomb.obj",
    "people/pedmale7_shotgun.obj",
];

/// Model masses (+0x48ec) by people table index (people.md §2.1).
fn person_mass(index: usize) -> i32 {
    match index {
        9 => 120,
        10 | 11 => 100,
        15 => 95,
        _ => 90,
    }
}

/// Per-person state beyond the core object (people.md §8).
#[derive(Component, Clone, Debug)]
pub struct Person {
    /// Motion frames: next (N), previous (P), blend t 0..256 (-1 = none).
    pub frame_next: i32,
    pub frame_prev: i32,
    pub blend: i32,
    /// Lying-down timer (> 0: stunned).
    pub stun: i32,
    /// Flee timer (state Flee).
    pub flee: i32,
    /// Wander target heading T (+0x7d0; < 0: walking straight). Starts as
    /// the spawn heading (Mac `CreateObject` @ 10075630).
    pub wander: i32,
    /// Wander re-aim timer (Mac +0x900, `do_Person` @ 1013fbd0).
    pub wander_timer: i32,
    /// Wander anchor (Mac +0x7d4/+0x7d8): centre of the last pavement
    /// square walked on; (-1, -1) until then, except for spawner peds
    /// (`AddThing` @ 10058350 sets the spawn square centre).
    pub anchor: IVec2,
    /// Hit a wall this tick.
    pub hit_wall: bool,
    pub splashed: bool,
    /// Entity fled from / targeted.
    pub target: Option<Entity>,
    /// Index into PEOPLE_MODELS of the model shown (follows the weapon for
    /// the player).
    pub kind: usize,
    /// Firing/punch pose model shown instead for a frame or two.
    pub pose_model: Option<&'static str>,
    /// The model follows the carried weapon (the player).
    pub follows_weapon: bool,
    /// The entity's own animated mesh, and the model it was built for.
    pub mesh: Handle<Mesh>,
    shown: usize,
}

/// The local player's session state (lives, respawn).
#[derive(Resource)]
pub struct PlayerSession {
    pub lives: i32,
    /// Bonus multiplier 1..4 (+1 per mission complete, reset on death).
    pub multiplier: i32,
    /// On-foot health kept while driving (`DAT_012a6304`).
    pub foot_health: i32,
    pub respawn_timer: i32,
    pub regen: i32,
    /// The score at death, given back to the respawned player (Mac
    /// `amiga_levelloop` @ 10100f50 stores `+0x864` into `DAT_1017c524[p]`
    /// when the player dies and copies it into the new object's `+0x864`
    /// on respawn; `GetPlayerScore` @ 10052f80 reads it while down).
    pub saved_score: i32,
    /// Script-forced respawn square (sx, sy, level).
    pub respawn_square: Option<(usize, usize, usize)>,
    /// Start points used recently (avoid repeats).
    pub recent_starts: Vec<usize>,
    /// On-foot health of multiplayer players 2..4 while driving (player 1
    /// uses `foot_health`).
    pub others_foot_health: [i32; 3],
}

impl PlayerSession {
    /// On-foot health kept while player `p` drives.
    pub fn foot_health_mut(&mut self, p: Option<u8>) -> &mut i32 {
        match p.unwrap_or(0) {
            0 => &mut self.foot_health,
            n => &mut self.others_foot_health[(n as usize - 1).min(2)],
        }
    }
}

impl Default for PlayerSession {
    fn default() -> Self {
        PlayerSession {
            lives: 3,
            multiplier: 1,
            foot_health: 100,
            respawn_timer: 0,
            regen: 0,
            saved_score: 0,
            respawn_square: None,
            recent_starts: Vec::new(),
            others_foot_health: [100; 3],
        }
    }
}

/// One player's held keys and latched presses, collected every frame and
/// consumed by the next fixed tick (so short presses aren't lost between
/// ticks).
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct PlayerInput {
    pub left: bool,
    pub right: bool,
    pub accel: bool,
    pub reverse: bool,
    pub fire: bool,
    pub brake: bool,
    pub use_pressed: bool,
    pub weapon_pressed: Option<u8>,
}

/// Controls of every player, by player number (single player: index 0).
#[derive(Resource, Default)]
pub struct PlayerInputs(pub [PlayerInput; 4]);

#[derive(Resource)]
pub struct HumanMotion(pub Motion);

/// The player has been spawned at level start.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerSpawned;

/// People move, then decide what to do (controls and AI fill the stick),
/// as `object_MainLoop` @ 1014a670 moves each object before its
/// `do_Person` (AI-1; per phase, not interleaved per object).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeopleThink;

/// People steer (the stick sets the next tick's velocity) and animate;
/// other systems order themselves after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeopleSet;

pub struct PeoplePlugin;

impl Plugin for PeoplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerSession>()
            .init_resource::<PlayerInputs>()
            .add_message::<Panic>()
            .add_systems(Startup, load_motion)
            .add_systems(
                OnEnter(AppState::Playing),
                spawn_player.in_set(LevelLoaded).in_set(PlayerSpawned),
            )
            .add_systems(OnExit(AppState::Playing), |mut commands: Commands| {
                commands.insert_resource(PlayerSession::default());
                commands.insert_resource(PlayerInputs::default());
            })
            .add_systems(PreUpdate, read_keyboard)
            .add_systems(
                FixedUpdate,
                (
                    (
                        swim_test.run_if(|| crate::args().any(|a| a == "--swim-test")),
                        move_people,
                        player_stick,
                        panic_peds,
                        ped_ai,
                        super::ai::people_orders,
                    )
                        .chain()
                        .in_set(PeopleThink),
                    (
                        steer_people,
                        animate_people,
                        regen_and_respawn,
                        despawn_far,
                    )
                        .chain()
                        .in_set(PeopleSet)
                        .after(PeopleThink),
                ),
            )
            .add_systems(Update, (pose_people, update_listener));
    }
}

fn load_motion(mut commands: Commands, data: Res<GameData>) {
    let motion = data
        .read("Motion/Human.motion")
        .ok()
        .and_then(|b| Motion::parse(&b).ok())
        .unwrap_or_else(|| {
            warn!("Unable to load motion file Motion/Human.motion");
            Motion {
                swing: [[0; 14]; 12],
                lift: [[0; 14]; 12],
            }
        });
    commands.insert_resource(HumanMotion(motion));
}

/// Spawn a person of `kind` (PEOPLE_MODELS index). Returns None if the
/// model is missing.
#[allow(clippy::too_many_arguments)]
pub fn spawn_person(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    kind: usize,
    pos: IVec3,
    heading: i32,
    state: State,
) -> Option<Entity> {
    let model = library.load(data, meshes, PEOPLE_MODELS[kind])?;
    let m = library.model_mut(model);
    m.info.radius = PERSON_RADIUS;
    m.info.mass = person_mass(kind);
    let foot = library.model(model).foot as f32;
    let mesh = meshes.add(library.build_mesh(&library.model(model).obj, |v| standing(v, 0.0, 0.0, foot)));
    let mut obj = Obj::new(Class::Person, model, pos, heading);
    obj.state = state;
    let person = Person {
        frame_next: 0,
        frame_prev: 11,
        blend: 0,
        stun: 0,
        flee: 0,
        wander: heading,
        wander_timer: 0,
        anchor: IVec2::splat(-1),
        hit_wall: false,
        splashed: false,
        target: None,
        kind,
        pose_model: None,
        follows_weapon: false,
        mesh: mesh.clone(),
        shown: model,
    };
    let e = commands
        .spawn((
            obj,
            person,
            Stick::default(),
            Mesh3d(mesh),
            MeshMaterial3d(library.material.clone()),
            Transform::from_translation(object_to_render(pos.as_vec3())),
            LevelEntity,
        ))
        .id();
    Some(e)
}

#[allow(clippy::too_many_arguments)]
fn spawn_player(
    mut commands: Commands,
    data: Res<GameData>,
    starts: Option<Res<StartPoints>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mode: Res<GameMode>,
    mut rng: ResMut<Rng>,
    mut session: ResMut<PlayerSession>,
) {
    let (Some(starts), Some(mut library)) = (starts, library) else {
        return;
    };
    if mode.is_multiplayer() {
        // Every player at a different start point.
        let mut taken: Vec<Vec3> = Vec::new();
        for p in 0..mode.players() {
            let Some(pos) = crate::multiplayer::pick_start(&starts, &taken, &mut rng) else {
                warn!("map has no start points");
                return;
            };
            taken.push(pos);
            if let Some(e) = spawn_person(
                &mut commands,
                &data,
                &mut library,
                &mut meshes,
                0,
                pos.as_ivec3(),
                0,
                State::Player,
            ) {
                make_player(&mut commands, e, p as u8);
            }
        }
        return;
    }
    // Rampage places the player like a respawn, at a random start point
    // (`amiga_startlevel` @ 100fff40 calls `StartObject` when the Rampage
    // flag `DAT_1017c334` is set, else `InitialStartObject` @ 100ade40 with
    // the level's start square; the same branch in 68k `FUN_000ec686`
    // ~0xee31a and OS4 `FUN_010692bc` ~49288). Most user maps have no
    // special 2/3 square, only the random pool.
    let pos = if mode.is_rampage() {
        random_start(&starts, &mut session, &mut rng)
    } else {
        None
    };
    let Some(pos) = pos.or(starts.level_start).or_else(|| starts.list.first().copied()) else {
        warn!("map has no start points");
        return;
    };
    if let Some(e) = spawn_person(
        &mut commands,
        &data,
        &mut library,
        &mut meshes,
        0,
        pos.as_ivec3(),
        0,
        State::Player,
    ) {
        make_player(&mut commands, e, 0);
    }
}

/// A random start point from LoadMap's pool, avoiding the last 4 used
/// (Mac `StartObject` @ 100adb70: `random(n)` up to 20 draws; 68k
/// `FUN_00043c96`, OS4 `FUN_0106ef6c`). None if the map has no pool.
fn random_start(starts: &StartPoints, session: &mut PlayerSession, rng: &mut Rng) -> Option<Vec3> {
    if starts.list.is_empty() {
        return None;
    }
    let mut i = rng.rand(starts.list.len() as i32) as usize;
    for _ in 0..19 {
        if !session.recent_starts.contains(&i) {
            break;
        }
        i = rng.rand(starts.list.len() as i32) as usize;
    }
    session.recent_starts.push(i);
    if session.recent_starts.len() > 4 {
        session.recent_starts.remove(0);
    }
    Some(starts.list[i])
}

/// Turn a freshly spawned person into player `player`.
pub fn make_player(commands: &mut Commands, e: Entity, player: u8) {
    commands
        .entity(e)
        .insert((CameraTarget, Arsenal::default()));
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_mut::<Person>(e) {
            p.follows_weapon = true;
        }
        if let Some(mut o) = world.get_mut::<Obj>(e) {
            o.player = Some(player);
            // Owner side 0 (`+0x824`, set at the level start and respawn
            // in Mac `amiga_levelloop` @ 10100f50).
            o.side = 0;
        }
    });
}

/// Single-player controls: the keyboard and the first gamepad, through
/// the control setup's tables (`frontend::bindings`, `DAT_012a63fc`
/// tables 0 and 1). Digits 0..7 pick a weapon directly; next/previous
/// weapon step through the carried ones. The pad's stick and d-pad steer;
/// their up/down drive only with the "Left stick forward/back" option
/// (`PadOptions::game_directions`). The horn (port addition,
/// `bindings::HORN`) is the shoot bit while the player is in a vehicle.
fn read_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<GameMode>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    pads: Query<(Entity, &Gamepad)>,
    player: Query<(&Obj, &crate::sim::combat::Arsenal, Has<crate::vehicles::Vehicle>), With<crate::camera::CameraTarget>>,
    mut inputs: ResMut<PlayerInputs>,
    mut dialog: ResMut<crate::hud::Dialog>,
    smooth: Res<crate::frame::Smooth>,
    (slots, axes): (Res<crate::frontend::bindings::PadSlots>, Res<crate::frontend::bindings::PadAxes>),
    (mut latch, mut logged): (Local<(u64, [bool; 6])>, Local<String>),
) {
    use crate::frontend::bindings::{self as b, key_held, key_just, pad_held, pad_just};
    // Multiplayer devices are read by `multiplayer::read_devices`.
    if mode.is_multiplayer() {
        return;
    }
    let defaults;
    let controls = match &settings {
        Some(s) => &s.controls,
        None => {
            defaults = b::Controls::default();
            &defaults
        }
    };
    let (k, p) = (&controls.keys, &controls.pads[0]);
    // The lowest-numbered connected pad (`bindings::PadSlots`).
    let pad = slots.first().and_then(|e| pads.get(e).ok()).map(|(e, g)| axes.state(e, g));
    let pad = pad.as_ref();
    // Left stick (past the deadzone) and d-pad: left, right, and up / down
    // with the stick-drive option.
    let [sl, sr, su, sd] = pad.map_or([false; 4], |p| p.game_directions(&controls.opts));
    // The player's object (the vehicle while driving).
    let me = player.iter().find(|(o, ..)| o.player.unwrap_or(0) == 0);
    let driving = me.is_some_and(|(.., v)| v);
    let held = |a: usize| key_held(&keys, k[a], true) || pad.is_some_and(|g| pad_held(g, p[a]));
    let just = |a: usize| key_just(&keys, k[a], true) || pad.is_some_and(|g| pad_just(g, p[a]));
    let input = &mut inputs.0[0];
    input.left = held(b::LEFT) || sl;
    input.right = held(b::RIGHT) || sr;
    input.accel = held(b::ACCELERATE) || su;
    input.reverse = held(b::REVERSE) || sd;
    input.fire = b::fire_held(held, driving);
    input.brake = held(b::BRAKE);
    // The original reads the input once per tick; here it is read every
    // frame, so a press that starts and ends between two ticks is kept
    // until the next tick has seen it (`Smooth::ticks` counts the ticks).
    let (seen, held_since) = &mut *latch;
    if *seen != smooth.ticks() {
        *seen = smooth.ticks();
        *held_since = [false; 6];
    }
    for (h, now) in held_since.iter_mut().zip([
        &mut input.left,
        &mut input.right,
        &mut input.accel,
        &mut input.reverse,
        &mut input.fire,
        &mut input.brake,
    ]) {
        *h |= *now;
        *now = *h;
    }
    if just(b::IN_OUT) {
        input.use_pressed = true;
    }
    // `--pad-debug`: the controls as the sim gets them, on each change.
    if b::pad_debug_enabled() {
        let names = ["left", "right", "accel", "reverse", "fire", "brake", "in/out"];
        let on = [input.left, input.right, input.accel, input.reverse, input.fire, input.brake, input.use_pressed];
        let now: Vec<&str> = names.iter().zip(on).filter(|(_, o)| *o).map(|(n, _)| *n).collect();
        let now = format!("[{}]{}", now.join(" "), if driving { " in a vehicle" } else { " on foot" });
        if *logged != now {
            let at = me.map(|(o, ..)| o.posf().round());
            b::pad_log(format_args!("player 1 input {now}, at {at:?}"));
            *logged = now;
        }
    }
    // Repeat the last message (`FUN_01015a10`): on the key's release
    // (`FUN_0103f3d0`), on the button's press (`FUN_0103da68`). Only the
    // message box changes, so replays are unaffected.
    let released = k[b::REPEAT_MESSAGE].is_some_and(|key| keys.any_just_released(b::key_variants(key)));
    if released || pad.is_some_and(|g| pad_just(g, p[b::REPEAT_MESSAGE])) {
        dialog.repeat();
    }
    let digits = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, k) in digits.iter().enumerate() {
        if keys.just_pressed(*k) {
            input.weapon_pressed = Some(i as u8);
        }
    }
    let step = if just(b::NEXT_WEAPON) || keys.any_just_pressed(b::NEXT_WEAPON_ALIASES) {
        1
    } else if just(b::PREV_WEAPON) || keys.any_just_pressed(b::PREV_WEAPON_ALIASES) {
        -1
    } else {
        0
    };
    if step != 0 {
        let ars = me.map(|(_, a, _)| a);
        if let Some(slot) = ars.and_then(|a| crate::multiplayer::cycle_weapon(a, step)) {
            input.weapon_pressed = Some(slot);
        }
    }
}

/// Player controls -> virtual stick (people.md §4.1).
fn player_stick(mut inputs: ResMut<PlayerInputs>, mut q: Query<(&Obj, &mut Stick)>) {
    for (obj, mut stick) in &mut q {
        if obj.state != State::Player {
            continue;
        }
        let input = &inputs.0[obj.player.unwrap_or(0).min(3) as usize];
        stick.steer = if input.left {
            -1
        } else if input.right {
            1
        } else {
            0
        };
        stick.throttle = if input.accel {
            -1
        } else if input.reverse {
            1
        } else {
            0
        };
        stick.brake = input.brake;
        if input.brake {
            stick.throttle = obj.speed.signum().max(-1);
            if obj.speed == 0 {
                stick.throttle = 0;
            }
        }
        stick.fire = input.fire;
        stick.use_vehicle = input.use_pressed;
    }
    for input in &mut inputs.0 {
        input.use_pressed = false;
    }
}

/// Civilians within 10000 units of a shot or blast scream and flee for
/// 100 ticks (68k `FUN_000ccabe`, OS4 `FUN_010435f4` / `FUN_01043788`).
fn panic_peds(
    mut commands: Commands,
    mut events: MessageReader<Panic>,
    mut rng: ResMut<Rng>,
    mut q: Query<(Entity, &mut Obj, &mut Person)>,
    models: Option<Res<ObjectLibrary>>,
) {
    let Some(models) = models else {
        events.clear();
        return;
    };
    for ev in events.read() {
        // Object-table order (`table_order`): replays must reproduce it.
        for e in crate::sim::table_order(q.iter().map(|(e, o, _)| (e, o))) {
            let Ok((_, mut o, mut p)) = q.get_mut(e) else { continue };
            if o.state != State::Wander || models.model(o.model).info.police {
                continue;
            }
            let d = (o.pos - ev.at).as_i64vec3() / 8;
            if d.length_squared() < 0x17d784 {
                o.state = State::Flee;
                p.flee = 100;
                p.target = Some(ev.from);
                let at = SoundPos::At(o.posf());
                commands.spawn(Sound::once(id::OW1 + rng.rand(3) as u16, 5e6, 8000.0, at));
            }
        }
    }
}

/// Pedestrian AI: wandering and fleeing (people.md §7.3).
fn ped_ai(
    city: Option<Res<City>>,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    targets: Query<&Obj, Without<Person>>,
    mut q: Query<(Entity, &mut Obj, &mut Person, &mut Stick)>,
) {
    let Some(city) = city else { return };
    // Every wandering ped draws from the generator every few ticks: visit
    // them in object-table order (the original's `do_Objects` loop), not
    // the ECS order.
    for e in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((_, mut obj, mut p, mut stick)) = q.get_mut(e) else { continue };
        // The player's stick comes from `player_stick`.
        if obj.state == State::Player {
            continue;
        }
        *stick = Stick::default();
        match obj.state {
            State::Wander => wander(&city, &trig, &mut rng, &obj, &mut p, &mut stick),
            State::Flee => {
                // Mac `do_Person` @ 1013fbd0 (l.105683-105700): the timer
                // ends the flight, but this tick still steers.
                if p.flee > 0 {
                    p.flee -= 1;
                    if p.flee < 1 {
                        obj.state = State::Wander;
                        p.flee = 0;
                    }
                }
                let from = p
                    .target
                    .and_then(|e| targets.get(e).ok())
                    .map(|o| o.pos)
                    .unwrap_or(obj.pos);
                let away = heading_of(obj.pos.x - from.x, obj.pos.y - from.y);
                // The shared AI steering (l.105830-105876): throttle only
                // when facing the goal, released above 0x18 while fleeing.
                let (steer, ahead) = steer_toward(obj.heading, away, 0xc9, 200);
                stick.steer = steer;
                if ahead && !(obj.state == State::Flee && obj.speed > 0x18) {
                    stick.throttle = -1;
                }
            }
            _ => {}
        }
    }
}

/// The original's steering toward heading `t`: d1 = (t − h) and d2 =
/// (h − t), both mod 0x1680. The shorter way is steered unless it is under
/// `near_left` (d1) / `near_right` (d2), which counts as facing `t`
/// (Mac `do_Person` l.105830-105876: 0xc9/200; the wander uses 0xc9/0xa0).
/// Returns (steer, facing).
fn steer_toward(heading: i32, t: i32, near_left: i32, near_right: i32) -> (i32, bool) {
    let d1 = (t - heading).rem_euclid(0x1680);
    let d2 = (heading - t).rem_euclid(0x1680);
    // d1 + d2 = 0x1680 (or both 0), so the shorter one is below 0xb41.
    if d1 < d2 {
        if d1 < near_left { (0, true) } else { (-1, false) }
    } else if d2 < near_right {
        (0, true)
    } else {
        (1, false)
    }
}

/// Wandering pedestrian, the Mac rule (Mac `do_Person` @ 1013fbd0,
/// l.105953-106096; the Amiga's "straight for 50 ticks, then turn
/// 90-270°" of 68k `FUN_000e3a58` ~122295 is gone). While the current
/// square and the one 512 ahead are pavement (flag 0x20) and no wall was
/// hit, every 3 ticks T is re-rolled to heading ± 500 (1 in 50: straight
/// on) and the anchor follows the square's centre; otherwise every 20
/// ticks T aims back at the anchor. T then snaps to the 90° grid.
fn wander(city: &City, trig: &Trig, rng: &mut Rng, obj: &Obj, p: &mut Person, stick: &mut Stick) {
    if p.wander_timer > 0 {
        p.wander_timer -= 1;
    }
    let ahead = |v: i32, d: i32| ((v + d * 2) >> 11).clamp(0, MAP_SIZE as i32 - 1) as usize;
    let (ax, ay) = (ahead(obj.pos.x, trig.sin(obj.heading)), ahead(obj.pos.y, trig.cos(obj.heading)));
    let keep = !p.hit_wall
        && pavement(city, obj.pos, obj.level)
        && city.map.square(ax, ay).cubes[obj.level].flags & 0x20 != 0;
    if keep {
        if p.wander_timer < 1 {
            if rng.rand(0x32) == 0 {
                p.wander = -1;
            } else {
                // Not wrapped: below 0 it walks straight this tick.
                p.wander = obj.heading + rng.rand(1000) - 500;
                p.wander_timer = 3;
            }
        }
        p.anchor = IVec2::new((obj.pos.x & !0x7ff) + 0x400, (obj.pos.y & !0x7ff) + 0x400);
    } else if p.wander_timer < 1 {
        p.wander = heading_of(p.anchor.x - obj.pos.x, p.anchor.y - obj.pos.y);
        p.wander_timer = 0x14;
    }
    let mut t = p.wander;
    if t >= 0 {
        // Ease toward the nearest multiple of 90° by 16 a tick.
        let snapped = (t + 0x168) / 0x5a0 * 0x5a0;
        if t < snapped {
            t += 16;
        } else if snapped < t {
            t -= 16;
        }
        if t < 0 {
            t += 0x1680;
        } else if t > 0x167f {
            t -= 0x1680;
        }
        let (steer, facing) = steer_toward(obj.heading, t, 0xc9, 0xa0);
        stick.steer = steer;
        if facing {
            t = -1;
        }
    }
    // Mac: also walk on (slowly) while turning.
    let cap = if t < 0 { 0x30 } else { 0x18 };
    if obj.speed < cap {
        stick.throttle = -1;
    }
    p.wander = t;
}

/// Whether the cube at `level` under `pos` lets pedestrians walk (0x20).
fn pavement(city: &City, pos: IVec3, level: usize) -> bool {
    let (sx, sy) = ((pos.x >> 11) as usize, (pos.y >> 11) as usize);
    sx < MAP_SIZE && sy < MAP_SIZE && city.map.square(sx, sy).cubes[level].flags & 0x20 != 0
}

/// Movement, gravity, collision and water for every person (people.md §4;
/// `MoveObject_General` @ 1006e4f0 and the swim part of `do_Person`); the
/// controls that set the velocity for the next tick come after the AI, in
/// [`steer_people`].
#[allow(clippy::type_complexity)]
fn move_people(
    mut commands: Commands,
    city: Option<Res<City>>,
    trig: Res<Trig>,
    library: Option<Res<crate::objects::ObjectLibrary>>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<crate::effects::Effects>,
    mut q: Query<(Entity, &mut Obj, &mut Person)>,
) {
    let Some(city) = city else { return };
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, mut p)) = q.get_mut(e__) else { continue };
        if obj.state == State::Removed {
            continue;
        }
        // Vertical motion (one sub-step per tick).
        let floor_z = obj.floor_z;
        if !obj.in_water {
            if obj.pos.z < floor_z {
                obj.on_ground = false;
            } else {
                obj.vel.z = 32;
                obj.pos.z = floor_z;
                obj.on_ground = true;
            }
            obj.vel.z += 32;
            obj.pos.z += obj.vel.z;
        } else if !obj.on_ground {
            if obj.pos.z >= floor_z {
                obj.vel.z = 0;
                obj.pos.z = floor_z;
                obj.on_ground = true;
                if !p.splashed {
                    p.splashed = true;
                    let at = SoundPos::At(obj.posf());
                    commands.spawn(Sound::once(id::SPLASH, 90.0 * 10000.0, 8350.0, at));
                }
            }
            obj.vel.z += 32;
            obj.pos.z += obj.vel.z;
        }
        // Move with last tick's velocity.
        let mut next = obj.pos;
        next.x += obj.vel.x;
        next.y += obj.vel.y;

        // Walls, then the floor at the new position.
        let (resolved, hit) = collide::resolve_person(&city, next, obj.prev_pos, PERSON_RADIUS);
        if hit.x {
            obj.vel.x = -obj.vel.x;
        }
        if hit.y {
            obj.vel.y = -obj.vel.y;
        }
        p.hit_wall = hit.x || hit.y;
        next = resolved;
        if let Some((fz, level, water)) = collide::floor(&city, next.x, next.y, next.z) {
            obj.floor_z = fz;
            obj.level = level;
            obj.in_water = water;
        }
        if obj.floor_z <= next.z {
            obj.vel.z = 0;
            obj.on_ground = true;
            if !obj.in_water {
                next.z = obj.floor_z;
            }
        }
        // Scenery pushes people out and stops them, without damage
        // (`AddCollideObj` @ 10055c70; `collide::scenery_collide`).
        if let Some(library) = &library {
            let body = collide::Body {
                radius: PERSON_RADIUS,
                front: 0,
                rear: 0,
                vehicle: false,
                rc: false,
                dz: [obj.floor_z - next.z, 0, 0, 0],
            };
            let mut vel = obj.vel;
            let (heading, level) = (obj.heading, obj.level);
            let bumps = collide::scenery_collide(&city, library, &trig, &mut next, &mut vel, heading, level, &body);
            obj.vel = vel;
            if !bumps.is_empty() {
                obj.speed = 0;
            }
        }

        // Swimming and drowning: sink 24 per tick with bubbles at the box
        // points (Mac `do_Person` @ 1013fbd0, the same loop as a sinking
        // vehicle's; 68k `FUN_000e3a58` ~122510).
        if obj.in_water && next.z >= obj.floor_z {
            if next.z < DROWN_Z {
                next.z += 24;
                if let Some(library) = &library {
                    let info = &library.model(obj.model).info;
                    let (hw, front, rear) = (info.radius.max(1), info.max_y, info.min_y);
                    super::driving::box_bubbles(&mut effects, &mut rng, &trig, &obj, next, hw, front, rear);
                }
            } else if obj.state != State::Dead {
                if obj.player.is_some() {
                    // A drowned player stays (hidden) as a corpse so the
                    // respawn timer starts like any other death.
                    obj.state = State::Dead;
                    obj.health = obj.health.min(0);
                    commands.entity(e).insert(Visibility::Hidden);
                } else {
                    obj.state = State::Removed;
                    commands.entity(e).despawn();
                    continue;
                }
            }
        }

        obj.pos = next;
        obj.prev_pos = next;
    }
}

/// The second half of `do_Person` @ 1013fbd0 for every person, after its
/// AI ([`PeopleThink`]) and boarding: stun, turning, speed and the velocity
/// used by the next tick's move. The original moves an object
/// (`MoveObject_General` @ 1006e4f0 / 68k `FUN_000e0e46`), then runs its
/// AI and controls (`object_MainLoop` @ 1014a670); the port does the same
/// per phase: every person moves ([`move_people`]), then every person
/// thinks, then every person steers.
fn steer_people(trig: Res<Trig>, mut q: Query<(Entity, &mut Obj, &mut Person, &Stick)>) {
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((_, mut obj, mut p, stick)) = q.get_mut(e__) else { continue };
        if obj.state == State::Removed {
            continue;
        }
        // Controls (the AI and player input filled the stick after this
        // tick's move).
        let (mut steer, mut throttle) = (stick.steer, stick.throttle);
        if p.stun > 0 {
            p.stun -= 1;
        }
        let down = obj.state == State::Dead || p.stun > 0;
        if down {
            steer = 0;
            throttle = 0;
            obj.speed = 0;
            obj.pitch = 0;
        } else {
            obj.pitch = 0x5a0;
        }

        // Turning.
        if steer > 0 {
            if obj.turn_rate > 0 {
                obj.turn_rate = 0;
            }
            obj.turn_rate = (obj.turn_rate - 15).max(-100);
        } else if steer < 0 {
            if obj.turn_rate < 0 {
                obj.turn_rate = 0;
            }
            obj.turn_rate = (obj.turn_rate + 15).min(100);
        } else {
            obj.turn_rate = 0;
        }
        obj.heading = wrap(obj.heading + obj.turn_rate);

        // Speed.
        obj.prev_speed = obj.speed;
        let h = obj.health;
        if throttle < 0 {
            let (acc, lo, hi) = if obj.in_water {
                (6, 6, 24)
            } else {
                (24, 24, 96)
            };
            obj.speed = (obj.speed + acc).min((h + 50).clamp(lo, hi));
        } else if throttle > 0 {
            let (acc, lo, hi) = if obj.in_water {
                (6, 6, 16)
            } else {
                (24, 24, 64)
            };
            obj.speed = (obj.speed - acc).max(-(h * 70 / 100 + 35).clamp(lo, hi));
        } else {
            obj.speed -= obj.speed.signum() * obj.speed.abs().min(8);
        }
        // The new velocity only closes 2/3 of the gap each tick (Mac
        // `do_Person` l.106712-106721; 68k `FUN_000e3a58` ~123026 set it to
        // `sin·speed >> 8`), so wall bounces and contact pushes carry over.
        let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
        obj.vel.x = (obj.vel.x + (obj.speed * s >> 7)) / 3;
        obj.vel.y = (obj.vel.y + (obj.speed * c >> 7)) / 3;

        if obj.health < 1 && obj.state != State::Dead {
            obj.state = State::Dead;
        }
    }
}

/// `--swim-test [ticks]`: once, after `ticks` (default 0), put the on-foot
/// player on the water surface of the nearest water square, to watch them
/// sink and drown (debug only).
fn swim_test(
    city: Option<Res<City>>,
    mut q: Query<&mut Obj, (With<Person>, With<CameraTarget>)>,
    mut done: Local<bool>,
    mut ticks: Local<u32>,
) {
    let Some(city) = city else { return };
    if *done {
        return;
    }
    *ticks += 1;
    if *ticks <= crate::arg("--swim-test").and_then(|v| v.parse().ok()).unwrap_or(0) {
        return;
    }
    let Some(mut obj) = q.iter_mut().find(|o| o.state == State::Player) else { return };
    let (cx, cy) = (obj.pos.x >> 11, obj.pos.y >> 11);
    let mut best: Option<(i32, IVec3, usize)> = None;
    for y in 0..MAP_SIZE as i32 {
        for x in 0..MAP_SIZE as i32 {
            let (wx, wy) = (x * 2048 + 1024, y * 2048 + 1024);
            let Some((fz, level, true)) = collide::floor(&city, wx, wy, collide::PLANE * 16) else { continue };
            let d = (x - cx).pow(2) + (y - cy).pow(2);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, IVec3::new(wx, wy, fz), level));
            }
        }
    }
    *done = true;
    let Some((_, pos, level)) = best else { return };
    info!("swim-test: player to {pos}");
    obj.pos = pos;
    obj.prev_pos = pos;
    obj.floor_z = pos.z;
    obj.level = level;
    obj.in_water = true;
    obj.on_ground = true;
    obj.vel = IVec3::ZERO;
}

/// Walk-cycle frame advance and footsteps (people.md §3.4).
fn animate_people(
    mut commands: Commands,
    city: Option<Res<City>>,
    data: Res<GameData>,
    trig: Res<Trig>,
    mut effects: ResMut<crate::effects::Effects>,
    mut q: Query<(&Obj, &mut Person)>,
) {
    let Some(city) = city else { return };
    let mut step = |commands: &mut Commands, obj: &Obj, frame: i32| {
        footstep(commands, &mut effects, &trig, &city, &data, obj, frame);
    };
    for (obj, mut p) in &mut q {
        if obj.speed == 0 {
            if obj.prev_speed != 0 {
                p.frame_next = -1;
                p.blend = 0;
            }
            if p.frame_prev != -1 {
                p.blend += 64;
                if p.blend > 255 {
                    p.blend = 0;
                    p.frame_prev = -1;
                }
            }
            continue;
        }
        if obj.prev_speed == 0 {
            p.blend = 0;
            p.frame_prev = -1;
            p.frame_next = 5;
        }
        p.blend += obj.speed * 3 / 2;
        let mut n = 0;
        while p.blend >= 256 && n < 3 {
            p.blend -= 256;
            p.frame_prev = p.frame_next;
            p.frame_next = (p.frame_next + 1).rem_euclid(12);
            if p.frame_prev == 0 || p.frame_prev == 6 {
                step(&mut commands, obj, p.frame_prev);
            }
            n += 1;
        }
        n = 0;
        while p.blend < 0 && n < 3 {
            // Walking backwards plants a foot too (Mac `do_Person` l.106448,
            // 68k ~122773).
            if p.frame_next == 0 || p.frame_next == 6 {
                step(&mut commands, obj, p.frame_next);
            }
            p.blend += 256;
            p.frame_next = p.frame_prev;
            p.frame_prev = (p.frame_prev - 1).rem_euclid(12);
            n += 1;
        }
    }
}

/// Footstep sound from the surface under the person: the texture's
/// `.CubeTextureList` value is the sound id; water squares splash.
///
/// Any cube takes its top texture's sound (texture 0, Pavement, gives
/// stone), whatever its face type or slope (Mac `do_Person` @ 1013fbd0
/// l.116461). Snow and slush steps (sounds 0x32, 0x33) leave a footprint:
/// a type-8 Dust2 splat (size 0x82, on the person's level, subtractive)
/// half a step to the right of the heading when the planted frame is 6,
/// else to the left (Mac l.116470-116485, 68k `FUN_000e3a58` ~122789).
fn footstep(
    commands: &mut Commands,
    effects: &mut crate::effects::Effects,
    trig: &Trig,
    city: &City,
    data: &GameData,
    obj: &Obj,
    frame: i32,
) {
    let (sx, sy) = ((obj.pos.x >> 11) as usize, (obj.pos.y >> 11) as usize);
    if sx >= MAP_SIZE || sy >= MAP_SIZE {
        return;
    }
    let cube = &city.map.square(sx, sy).cubes[obj.level];
    let top = cube.faces[map::TOP];
    let sound = if top.kind == FaceType::Reflect {
        id::FOOTSTEP_WATER
    } else {
        data.texture_flags(top.texture)
            .map(|f| f as u16)
            .unwrap_or(id::FOOTSTEP_STONE)
    };
    let mut at = obj.posf();
    at.z -= 200.0;
    // The splash carries 15000, other steps 19000 (Mac `do_Person` l.106454).
    let range = if sound == id::FOOTSTEP_WATER { 15000.0 } else { 19000.0 };
    commands.spawn(Sound::once(sound, 3_000_000.0, range, SoundPos::At(at)));
    if sound == 0x32 || sound == 0x33 {
        let (s, c) = (trig.sin(obj.heading) >> 1, trig.cos(obj.heading) >> 1);
        let side = if frame == 6 { IVec3::new(c, -s, 0) } else { IVec3::new(-c, s, 0) };
        effects.add(
            8,
            crate::effects::Tex::Dust2,
            (obj.pos + side).as_vec3(),
            Vec3::ZERO,
            130.0,
            obj.level as u8,
            crate::effects::Blend::Subtract,
        );
    }
}

/// Player health regeneration, death and respawn (people.md §4.5).
#[allow(clippy::too_many_arguments)]
fn regen_and_respawn(
    mut commands: Commands,
    mut session: ResMut<PlayerSession>,
    data: Res<GameData>,
    city: Option<Res<City>>,
    starts: Option<Res<StartPoints>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: ResMut<Rng>,
    mut outcome: MessageWriter<crate::mission::LevelOutcome>,
    mut players: Query<(Entity, &mut Obj, Has<Person>), With<CameraTarget>>,
    mode: Res<GameMode>,
    mut triggers: ResMut<crate::mission::Triggers>,
) {
    // Multiplayer deaths and respawns: `multiplayer::deaths_and_respawns`.
    if mode.is_multiplayer() {
        return;
    }
    let (Some(city), Some(starts), Some(mut library)) = (city, starts, library) else {
        return;
    };
    let mut alive = false;
    for e__ in crate::sim::table_order(players.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, on_foot)) = players.get_mut(e__) else { continue };
        match obj.state {
            State::Dead | State::Removed => {
                if session.respawn_timer == 0 {
                    session.respawn_timer = RESPAWN_TICKS;
                }
                // The score survives death (§4.5; Mac 10100f50).
                session.saved_score = obj.score;
                if obj.state == State::Dead {
                    // The corpse stays; the camera moves to the new player.
                    commands.entity(e).remove::<CameraTarget>();
                    obj.player = None;
                } else {
                    // Blown apart (state 6): removed at once (people.md
                    // §7.2). Kept, it would stay the camera target and
                    // re-arm the respawn timer forever after the respawn.
                    commands.entity(e).despawn();
                }
            }
            _ => {
                alive = true;
                // +1 health every 250 ticks; while driving it heals the
                // stored on-foot health.
                session.regen += 1;
                if session.regen >= REGEN_TICKS {
                    session.regen = 0;
                    if on_foot {
                        if (1..100).contains(&obj.health) {
                            obj.health += 1;
                        }
                    } else if (1..100).contains(&session.foot_health) {
                        session.foot_health += 1;
                    }
                }
            }
        }
    }
    if alive || session.respawn_timer == 0 {
        return;
    }
    session.respawn_timer -= 1;
    if session.respawn_timer > 0 {
        return;
    }
    session.lives -= 1;
    session.multiplier = 1;
    if session.lives <= 0 {
        info!("Game over!");
        outcome.write(crate::mission::LevelOutcome::GameOver);
        return;
    }
    let pos = match session.respawn_square {
        Some((sx, sy, level)) => {
            let top = city.square_cubes(sx, sy)[level].top_at(0.5, 0.5);
            Vec3::new(
                (sx as f32 + 0.5) * 2048.0,
                (sy as f32 + 0.5) * 2048.0,
                crate::world::z_of_h(top),
            )
        }
        None => match random_start(&starts, &mut session, &mut rng).or(starts.level_start) {
            Some(p) => p,
            None => return,
        },
    };
    if let Some(e) = spawn_person(
        &mut commands,
        &data,
        &mut library,
        &mut meshes,
        0,
        pos.as_ivec3(),
        0,
        State::Player,
    ) {
        make_player(&mut commands, e, 0);
        // "score restored" (§4.5): the new object's +0x864 is the score
        // stored at death (Mac `amiga_levelloop` @ 10100f50).
        let score = session.saved_score;
        commands.queue(move |world: &mut World| {
            if let Some(mut o) = world.get_mut::<Obj>(e) {
                o.score = score;
            }
        });
        // Trigger type 0x3e (`Trigger_PlayerRespawned` 0xa69c2, called
        // right after the respawn placement).
        triggers.report(crate::mission::kind::RESPAWNED, 0, 0, 0);
    }
}

// Pedestrians are spawned with the traffic: a flag-0x20 square is one of
// `AddThing`'s choices (`traffic::spawn_ambient`, people.md §7.2).

/// Spawned by the ambient spawner (removable when off screen).
#[derive(Component)]
pub struct Ambient;

/// Remove untagged objects that leave every camera's window (Mac
/// `DoCollideStuff` @ 10067b50 l.25138, 68k `FUN_000dc5ac` ~117128, OS4
/// `FUN_010542b8`): objects without a script id (`+0x47c`, Mac `+0x968`)
/// in states 3, 5, 7, 10, 11 or 12 (drone, parked, dead, pursuit, barrier)
/// whose square is outside every player's window (inclusive edges) — not
/// only the spawner's, but also released actors, thrown-out drivers,
/// abandoned cars and bodies. State 6 goes at once. Idle (4), fleeing
/// (0x12) and script-order objects stay.
fn despawn_far(
    mut commands: Commands,
    cam: Res<GameCam>,
    views: Res<ViewCams>,
    q: Query<(Entity, &Obj), Without<CameraTarget>>,
) {
    let cams: &[GameCam] = if views.0.is_empty() { std::slice::from_ref(&*cam) } else { &views.0 };
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, obj)) = q.get(e__) else { continue };
        let sq = IVec2::new(obj.pos.x >> 11, obj.pos.y >> 11);
        if obj.player.is_some() {
            continue;
        }
        // State 6 is removed at once (blown apart, wrecked to nothing).
        if obj.state == State::Removed && obj.class != super::Class::Projectile {
            commands.entity(e).despawn();
            continue;
        }
        if obj.script_id != -1 {
            continue;
        }
        // Projectiles (and placed bombs) have their own lifetime; special
        // objects (barriers, cones) are state 0xc.
        if obj.class == super::Class::Projectile && obj.state != State::Barrier {
            continue;
        }
        let outside = |cam: &GameCam| {
            let (c, h) = (cam.square(), cam.window());
            (sq.x - c.x).abs() > h || (sq.y - c.y).abs() > h
        };
        let gone = matches!(
            obj.state,
            State::Wander
                | State::Parked
                | State::Dead
                | State::Removed
                | State::PursuitRoute
                | State::Pursuit
                | State::Barrier
        ) && cams.iter().all(outside);
        if gone {
            commands.entity(e).despawn();
        }
    }
}

/// People render scale (render units per file unit): 2/3 map units
/// horizontally and 1/2 vertically per people.md §2.3 (uniform port).
const HS: f32 = (2.0 / 3.0) / 256.0;
const VS: f32 = 0.5 / 256.0;

/// Standing pose: model Y vertical (down), the feet at `foot` on the
/// floor, model -Z forward, model +X the person's left. `dz`, `dy`: motion
/// offsets. `foot` is the model's own feet height ([`LoadedModel::foot`]:
/// 0 for the Mac people, whose origin is at the feet; 90, 92 or 100 for
/// the Amiga files, the shifts the Mac applied in data), so either set
/// stands on the floor as on the Mac (Mac `DrawObject` @ 10075880 draws
/// people at their origin; `docs/re/mac/gameplay_people.md` M3).
///
/// [`LoadedModel::foot`]: crate::objects::LoadedModel::foot
fn standing(v: &Vertex, dz: f32, dy: f32, foot: f32) -> [f32; 3] {
    let (x, y, z) = (v.x as f32, v.y as f32 + dy, v.z as f32 + dz);
    [x * HS, (foot - y) * VS, -z * HS]
}

/// Lying pose (pitch 0): `DrawObject` adds 0x5dc (1500) to the model Y
/// (now along the ground, forward) of a person with pitch 0 (Mac
/// `DrawObject` @ 10075880, OS4 `FUN_010c101c`). Measured from the feet
/// (Mac data), the body lies about centred on where the person stood.
fn lying(v: &Vertex, foot: f32) -> [f32; 3] {
    let (x, y, z) = (v.x as f32, v.y as f32 - foot, v.z as f32);
    [x * HS, -z * VS * 0.5, (y * 16.0 + 1500.0) / 3.0 / 2048.0]
}

/// Motion offset for limb `l` (1-based) from the blend state (people.md §3.3).
fn limb_offset(m: &Motion, p: &Person, l: usize, b: bool) -> f32 {
    if l == 0 || l > 14 {
        return 0.0;
    }
    let arr = |f: i32| -> f32 {
        let t = if b { &m.lift } else { &m.swing };
        t[f as usize][l - 1] as f32
    };
    let t = p.blend as f32 / 256.0;
    match (p.frame_next, p.frame_prev) {
        (-1, -1) => 0.0,
        (n, -1) => arr(n) * t,
        (-1, pr) => arr(pr) * (1.0 - t),
        (n, pr) => arr(pr) + (arr(n) - arr(pr)) * t,
    }
}

/// Pose every person's mesh. The mesh is rebuilt when the model changes
/// or the player number does (player n wears sand n, see
/// `ObjectLibrary::build_person_mesh`).
fn pose_people(
    motion: Option<Res<HumanMotion>>,
    library: Option<ResMut<ObjectLibrary>>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sands: Local<std::collections::HashMap<Entity, usize>>,
    mut q: Query<(Entity, &Obj, &mut Person, &mut Transform)>,
) {
    let (Some(motion), Some(mut library)) = (motion, library) else {
        return;
    };
    sands.retain(|e, _| q.contains(*e));
    for (e, obj, mut p, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        tf.rotation = heading_to_render(obj.heading as f32);
        let path = p.pose_model.unwrap_or(PEOPLE_MODELS[p.kind]);
        let Some(shown) = library.load(&data, &mut meshes, path) else {
            continue;
        };
        let sand = obj.player.map_or(0, usize::from);
        if shown != p.shown || sands.get(&e).copied().unwrap_or(0) != sand {
            // Different vertex layout or clothes: rebuild the whole mesh.
            sands.insert(e, sand);
            let foot = library.model(shown).foot as f32;
            let rebuilt =
                library.build_person_mesh(&library.model(shown).obj, sand, |v| standing(v, 0.0, 0.0, foot));
            if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                *mesh = rebuilt;
            }
            p.shown = shown;
        }
        let foot = library.model(shown).foot as f32;
        let model = &library.model(shown).obj;
        let positions = if obj.pitch == 0 {
            corner_positions(model, |v| lying(v, foot))
        } else {
            corner_positions(model, |v| {
                let l = v.limb as usize;
                standing(
                    v,
                    limb_offset(&motion.0, &p, l, false),
                    limb_offset(&motion.0, &p, l, true),
                    foot,
                )
            })
        };
        if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
            if let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                *pos = positions;
            }
        }
    }
}

/// Sounds are heard from the followed object; in split screen from every
/// player (each sound as loud as at the nearest one).
fn update_listener(
    mut listener: ResMut<Listener>,
    mut others: ResMut<crate::audio::OtherListeners>,
    mode: Res<GameMode>,
    q: Query<&Obj, With<CameraTarget>>,
) {
    if !mode.is_multiplayer() {
        if let Ok(obj) = q.single() {
            listener.0 = obj.posf();
        }
        return;
    }
    let mut players: Vec<(u8, Vec3)> = q.iter().filter_map(|o| Some((o.player?, o.posf()))).collect();
    players.sort_by_key(|p| p.0);
    if let Some(first) = players.first() {
        listener.0 = first.1;
    }
    others.0 = players.iter().skip(1).map(|p| p.1).collect();
}

#[cfg(test)]
#[path = "tests/people.rs"]
mod tests;
