//! People: the player on foot and pedestrians (docs/re/people.md).
//!
//! Movement, gravity, collision, the walk animation and the pedestrian AI
//! follow the 68k `FUN_000e3a58` people branch. All rates are per game tick.

use super::collide;
use super::combat::{Arsenal, Panic};
use super::{Class, Obj, Rng, State, Stick, Trig, angle_diff, heading_of, wrap};
use crate::audio::{Listener, Sound, SoundPos, id};
use crate::camera::{CameraTarget, GameCam};
use crate::data::GameData;
use crate::level::{AppState, LevelEntity, LevelLoaded, StartPoints};
use crate::objects::{ObjectLibrary, corner_positions};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use payback_formats::map::{self, FaceType, MAP_SIZE, Slope};
use payback_formats::{Motion, obj::Vertex};

/// Collision radius of every person: MaleNormal's max |X| / 3 (people.md §2.1).
pub const PERSON_RADIUS: i32 = 192;
/// Drowning depth (z) for people in water.
const DROWN_Z: i32 = 27000;
/// Ticks between player health regeneration steps.
const REGEN_TICKS: i32 = 250;
/// Respawn delay after death, ticks.
const RESPAWN_TICKS: i32 = 80;
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
    /// Wander target heading (< 0: walking straight, counting down).
    pub wander: i32,
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
    /// Script-forced respawn square (sx, sy, level).
    pub respawn_square: Option<(usize, usize, usize)>,
    /// Start points used recently (avoid repeats).
    pub recent_starts: Vec<usize>,
}

impl Default for PlayerSession {
    fn default() -> Self {
        PlayerSession {
            lives: 3,
            multiplier: 1,
            foot_health: 100,
            respawn_timer: 0,
            regen: 0,
            respawn_square: None,
            recent_starts: Vec::new(),
        }
    }
}

/// Held keys and latched presses, collected every frame and consumed by
/// the next fixed tick (so short presses aren't lost between ticks).
#[derive(Resource, Default)]
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

#[derive(Resource)]
pub struct HumanMotion(pub Motion);

/// The player has been spawned at level start.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerSpawned;

/// People decide what to do (controls and AI fill the stick).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeopleThink;

/// People move and animate; other systems order themselves after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeopleSet;

pub struct PeoplePlugin;

impl Plugin for PeoplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerSession>()
            .init_resource::<PlayerInput>()
            .add_message::<Panic>()
            .add_systems(Startup, load_motion)
            .add_systems(
                OnEnter(AppState::Playing),
                spawn_player.in_set(LevelLoaded).in_set(PlayerSpawned),
            )
            .add_systems(OnExit(AppState::Playing), |mut commands: Commands| {
                commands.insert_resource(PlayerSession::default());
                commands.insert_resource(PlayerInput::default());
            })
            .add_systems(PreUpdate, read_keyboard)
            .add_systems(
                FixedUpdate,
                (
                    (player_stick, panic_peds, ped_ai, super::ai::people_orders)
                        .chain()
                        .in_set(PeopleThink),
                    (
                        move_people,
                        animate_people,
                        regen_and_respawn,
                        spawn_peds,
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
    let mesh = meshes.add(library.build_mesh(&library.model(model).obj, |v| standing(v, 0.0, 0.0)));
    let mut obj = Obj::new(Class::Person, model, pos, heading);
    obj.state = state;
    let person = Person {
        frame_next: 0,
        frame_prev: 11,
        blend: 0,
        stun: 0,
        flee: 0,
        wander: -1,
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

fn spawn_player(
    mut commands: Commands,
    data: Res<GameData>,
    starts: Option<Res<StartPoints>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(starts), Some(mut library)) = (starts, library) else {
        return;
    };
    let Some(pos) = starts.level_start.or_else(|| starts.list.first().copied()) else {
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
        make_player(&mut commands, e);
    }
}

/// Turn a freshly spawned person into the local player.
fn make_player(commands: &mut Commands, e: Entity) {
    commands
        .entity(e)
        .insert((CameraTarget, Arsenal::default()));
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_mut::<Person>(e) {
            p.follows_weapon = true;
        }
        if let Some(mut o) = world.get_mut::<Obj>(e) {
            o.player = Some(0);
        }
    });
}

fn read_keyboard(keys: Res<ButtonInput<KeyCode>>, mut input: ResMut<PlayerInput>) {
    let held = |k: &[KeyCode]| keys.any_pressed(k.iter().copied());
    input.left = held(&[KeyCode::ArrowLeft]);
    input.right = held(&[KeyCode::ArrowRight]);
    input.accel = held(&[KeyCode::ArrowUp]);
    input.reverse = held(&[KeyCode::ArrowDown]);
    input.fire = held(&[KeyCode::Space, KeyCode::ControlRight, KeyCode::ControlLeft]);
    input.brake = held(&[KeyCode::ShiftRight, KeyCode::ShiftLeft]);
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        input.use_pressed = true;
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
}

/// Player controls -> virtual stick (people.md §4.1).
fn player_stick(mut input: ResMut<PlayerInput>, mut q: Query<(&Obj, &mut Stick)>) {
    for (obj, mut stick) in &mut q {
        if obj.state != State::Player {
            continue;
        }
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
    input.use_pressed = false;
}

/// Civilians within 10000 units of a shot or blast scream and flee for
/// 100 ticks (68k `FUN_000ccabe`, OS4 `FUN_010435f4` / `FUN_01043788`).
fn panic_peds(
    mut commands: Commands,
    mut events: MessageReader<Panic>,
    mut rng: ResMut<Rng>,
    mut q: Query<(&mut Obj, &mut Person)>,
    models: Option<Res<ObjectLibrary>>,
) {
    let Some(models) = models else {
        events.clear();
        return;
    };
    for ev in events.read() {
        for (mut o, mut p) in &mut q {
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
    mut rng: ResMut<Rng>,
    targets: Query<&Obj, Without<Person>>,
    mut q: Query<(&mut Obj, &mut Person, &mut Stick)>,
) {
    let Some(city) = city else { return };
    for (mut obj, mut p, mut stick) in &mut q {
        // The player's stick comes from `player_stick`.
        if obj.state == State::Player {
            continue;
        }
        *stick = Stick::default();
        match obj.state {
            State::Wander => {
                let mut t = p.wander;
                if t < 0 {
                    t -= 1;
                    let on_pavement = pavement(&city, obj.pos, obj.level);
                    if t < -50 && (p.hit_wall || !on_pavement) {
                        t = wrap(obj.heading + 0x5a0 + rng.rand(0xb40));
                    }
                }
                if t >= 0 {
                    // Snap the goal to the 90° grid and ease toward it.
                    let snapped = wrap(((t + 0x168) / 0x5a0) * 0x5a0);
                    let d = angle_diff(t, snapped);
                    t = wrap(t + d.clamp(-16, 16));
                    let err = angle_diff(obj.heading, t);
                    stick.steer = if err > 0 {
                        -1
                    } else if err < 0 {
                        1
                    } else {
                        0
                    };
                    if err.abs() < 200 {
                        t = -1;
                    }
                }
                if t < 0 && obj.speed < 0x30 {
                    stick.throttle = -1;
                }
                p.wander = t;
            }
            State::Flee => {
                p.flee -= 1;
                if p.flee <= 0 {
                    obj.state = State::Wander;
                    p.wander = -1;
                    continue;
                }
                let from = p
                    .target
                    .and_then(|e| targets.get(e).ok())
                    .map(|o| o.pos)
                    .unwrap_or(obj.pos);
                let away = heading_of(obj.pos.x - from.x, obj.pos.y - from.y);
                let err = angle_diff(obj.heading, away);
                stick.steer = if err > 0 {
                    -1
                } else if err < 0 {
                    1
                } else {
                    0
                };
                if obj.speed < 0x18 {
                    stick.throttle = -1;
                }
            }
            _ => {}
        }
    }
}

/// Whether the cube at `level` under `pos` lets pedestrians walk (0x20).
fn pavement(city: &City, pos: IVec3, level: usize) -> bool {
    let (sx, sy) = ((pos.x >> 11) as usize, (pos.y >> 11) as usize);
    sx < MAP_SIZE && sy < MAP_SIZE && city.map.square(sx, sy).cubes[level].flags & 0x20 != 0
}

/// Movement, gravity, collision and water for every person (people.md §4).
#[allow(clippy::type_complexity)]
fn move_people(
    mut commands: Commands,
    city: Option<Res<City>>,
    trig: Res<Trig>,
    mut q: Query<(Entity, &mut Obj, &mut Person, &Stick)>,
) {
    let Some(city) = city else { return };
    for (e, mut obj, mut p, stick) in &mut q {
        if obj.state == State::Removed {
            continue;
        }
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
        obj.vel.x = trig.sin(obj.heading) * obj.speed >> 8;
        obj.vel.y = trig.cos(obj.heading) * obj.speed >> 8;

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
        let mut next = obj.pos;
        next.x += obj.vel.x;
        next.y += obj.vel.y;

        // Walls, then the floor at the new position.
        let (resolved, hit) = collide::resolve_walls(&city, next, obj.prev_pos, PERSON_RADIUS);
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

        // Swimming and drowning.
        if obj.in_water && next.z >= obj.floor_z {
            if next.z < DROWN_Z {
                next.z += 24;
            } else if obj.state != State::Dead {
                obj.state = State::Removed;
                commands.entity(e).despawn();
                continue;
            }
        }

        obj.pos = next;
        obj.prev_pos = next;
        if obj.health < 1 && obj.state != State::Dead {
            obj.state = State::Dead;
        }
    }
}

/// Walk-cycle frame advance and footsteps (people.md §3.4).
fn animate_people(
    mut commands: Commands,
    city: Option<Res<City>>,
    data: Res<GameData>,
    mut q: Query<(&Obj, &mut Person)>,
) {
    let Some(city) = city else { return };
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
                footstep(&mut commands, &city, &data, obj);
            }
            n += 1;
        }
        n = 0;
        while p.blend < 0 && n < 3 {
            p.blend += 256;
            p.frame_next = p.frame_prev;
            p.frame_prev = (p.frame_prev - 1).rem_euclid(12);
            n += 1;
        }
    }
}

/// Footstep sound from the surface under the person: the texture's
/// `.CubeTextureList` value is the sound id; water squares splash.
fn footstep(commands: &mut Commands, city: &City, data: &GameData, obj: &Obj) {
    let (sx, sy) = ((obj.pos.x >> 11) as usize, (obj.pos.y >> 11) as usize);
    if sx >= MAP_SIZE || sy >= MAP_SIZE {
        return;
    }
    let cube = &city.map.square(sx, sy).cubes[obj.level];
    let top = cube.faces[map::TOP];
    let sound = if top.kind == FaceType::Reflect {
        id::FOOTSTEP_WATER
    } else if top.visible() && cube.slope != Slope::Off {
        data.cube_textures
            .entries
            .get(top.texture as usize)
            .map(|e| e.flags as u16)
            .unwrap_or(id::FOOTSTEP_STONE)
    } else {
        id::FOOTSTEP_STONE
    };
    let mut at = obj.posf();
    at.z -= 200.0;
    commands.spawn(Sound::once(sound, 3_000_000.0, 19000.0, SoundPos::At(at)));
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
) {
    let (Some(city), Some(starts), Some(mut library)) = (city, starts, library) else {
        return;
    };
    let mut alive = false;
    for (e, mut obj, on_foot) in &mut players {
        match obj.state {
            State::Dead | State::Removed => {
                if session.respawn_timer == 0 {
                    session.respawn_timer = RESPAWN_TICKS;
                }
                if obj.state == State::Dead {
                    // The corpse stays; the camera moves to the new player.
                    commands.entity(e).remove::<CameraTarget>();
                    obj.player = None;
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
        None if !starts.list.is_empty() => {
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
            starts.list[i]
        }
        None => match starts.level_start {
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
        make_player(&mut commands, e);
    }
}

/// Spawn pedestrians just outside the view (people.md §7.2): two attempts
/// per tick on the window edge ahead of the player's motion; flag-0x20
/// squares accept a pedestrian 1 time in 5.
#[allow(clippy::too_many_arguments)]
fn spawn_peds(
    mut commands: Commands,
    data: Res<GameData>,
    city: Option<Res<City>>,
    cam: Res<GameCam>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: ResMut<Rng>,
    objects: Query<&Obj>,
    player: Query<&Obj, With<CameraTarget>>,
) {
    let (Some(city), Some(mut library)) = (city, library) else {
        return;
    };
    let count = objects.iter().count();
    // Ambient budget: min(placed + 11, 20) objects.
    if count >= 20 {
        return;
    }
    let mut occupied = vec![false; MAP_SIZE * MAP_SIZE];
    for o in &objects {
        let (sx, sy) = ((o.pos.x >> 11) as usize, (o.pos.y >> 11) as usize);
        if sx < MAP_SIZE && sy < MAP_SIZE {
            occupied[sx * MAP_SIZE + sy] = true;
        }
    }
    let vel = player.single().map(|o| o.vel).unwrap_or(IVec3::ZERO);
    let c = cam.square();
    let h = cam.window();
    for _ in 0..2 {
        let along = rng.rand(2 * h + 1) - h;
        let edge = if vel.x.abs() >= vel.y.abs() && vel.x != 0 {
            if vel.x > 0 { 1 } else { 3 }
        } else if vel.y != 0 {
            if vel.y > 0 { 2 } else { 0 }
        } else {
            rng.rand(3)
        };
        let (sx, sy) = match edge {
            0 => (c.x + along, c.y - h),
            1 => (c.x + h, c.y + along),
            2 => (c.x + along, c.y + h),
            _ => (c.x - h, c.y + along),
        };
        if !(1..MAP_SIZE as i32 - 1).contains(&sx) || !(1..MAP_SIZE as i32 - 1).contains(&sy) {
            continue;
        }
        let (sx, sy) = (sx as usize, sy as usize);
        let near_busy = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|(dx, dy)| {
                occupied[(sx as i32 + dx) as usize * MAP_SIZE + (sy as i32 + dy) as usize]
            });
        if near_busy {
            continue;
        }
        for level in 0..3 {
            let cube = &city.map.square(sx, sy).cubes[level];
            if cube.slope == Slope::Off || !cube.faces[map::TOP].visible() {
                continue;
            }
            if cube.flags & 0x20 == 0 || rng.rand(5) != 0 {
                continue;
            }
            let top = city.square_cubes(sx, sy)[level].top_at(0.5, 0.5);
            let pos = IVec3::new(
                (sx as i32) * 2048 + 0x400,
                (sy as i32) * 2048 + 0x400,
                crate::world::z_of_h(top) as i32 - 0x200,
            );
            // PedMale1-4 (table indices 6, 8, 9, 10).
            let kind = [6, 8, 9, 10][rng.rand(4) as usize];
            let heading = rng.rand(0x167f);
            if let Some(e) = spawn_person(
                &mut commands,
                &data,
                &mut library,
                &mut meshes,
                kind,
                pos,
                heading,
                State::Wander,
            ) {
                commands.entity(e).insert(Ambient);
                occupied[sx * MAP_SIZE + sy] = true;
            }
            break;
        }
    }
}

/// Spawned by the ambient spawner (removable when off screen).
#[derive(Component)]
pub struct Ambient;

/// Remove ambient objects that leave every camera's window.
fn despawn_far(mut commands: Commands, cam: Res<GameCam>, q: Query<(Entity, &Obj), With<Ambient>>) {
    let c = cam.square();
    let h = cam.window();
    for (e, obj) in &q {
        let sq = IVec2::new(obj.pos.x >> 11, obj.pos.y >> 11);
        if obj.script_id != -1 {
            continue;
        }
        let gone = matches!(
            obj.state,
            State::Wander | State::Parked | State::Dead | State::Flee | State::Removed
        ) && ((sq.x - c.x).abs() > h || (sq.y - c.y).abs() > h);
        if gone {
            commands.entity(e).despawn();
        }
    }
}

/// People render scale (render units per file unit): 2/3 map units
/// horizontally and 1/2 vertically per people.md §2.3 (uniform port).
const HS: f32 = (2.0 / 3.0) / 256.0;
const VS: f32 = 0.5 / 256.0;

/// Standing pose: model Y vertical (feet at +90 on the floor), model -Z
/// forward, model +X the person's left. `dz`, `dy`: motion offsets.
fn standing(v: &Vertex, dz: f32, dy: f32) -> [f32; 3] {
    let (x, y, z) = (v.x as f32, v.y as f32 + dy, v.z as f32 + dz);
    [x * HS, (90.0 - y) * VS, -z * HS]
}

/// Lying pose (pitch 0): the body extends forward from the origin.
fn lying(v: &Vertex) -> [f32; 3] {
    let (x, y, z) = (v.x as f32, v.y as f32, v.z as f32);
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

fn pose_people(
    motion: Option<Res<HumanMotion>>,
    library: Option<ResMut<ObjectLibrary>>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&Obj, &mut Person, &mut Transform)>,
) {
    let (Some(motion), Some(mut library)) = (motion, library) else {
        return;
    };
    for (obj, mut p, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        tf.rotation = heading_to_render(obj.heading as f32);
        let path = p.pose_model.unwrap_or(PEOPLE_MODELS[p.kind]);
        let Some(shown) = library.load(&data, &mut meshes, path) else {
            continue;
        };
        if shown != p.shown {
            // Different vertex layout: rebuild the whole mesh.
            let rebuilt = library.build_mesh(&library.model(shown).obj, |v| standing(v, 0.0, 0.0));
            if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                *mesh = rebuilt;
            }
            p.shown = shown;
        }
        let model = &library.model(shown).obj;
        let positions = if obj.pitch == 0 {
            corner_positions(model, lying)
        } else {
            corner_positions(model, |v| {
                let l = v.limb as usize;
                standing(
                    v,
                    limb_offset(&motion.0, &p, l, false),
                    limb_offset(&motion.0, &p, l, true),
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

/// Sounds are heard from the followed object.
fn update_listener(mut listener: ResMut<Listener>, q: Query<&Obj, With<CameraTarget>>) {
    if let Ok(obj) = q.single() {
        listener.0 = obj.posf();
    }
}
