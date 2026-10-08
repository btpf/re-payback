//! Ambient traffic (Mac `AddVehicles` @ 10059d40 + `AddThing` @ 10058350
//! spawner, `MoveUnit` @ 10081080 lane choice, driver in `do_Other`
//! @ 10146280; 68k `FUN_000d0cfc`, `FUN_000cf5cc`, `FUN_000a143e`,
//! `FUN_000e3a58`; OS4 `FUN_010485cc`, `FUN_01047af4`, `FUN_010cb7e4`).
//!
//! Each map square is one one-way lane; flags bits 1/2/4/8 allow leaving
//! N/E/S/W and 0x10 means "straight on". A traffic car (state 3) steers at
//! its target point, a square centre; once within reach of it, `MoveUnit`
//! picks the next square from that square's flags (no U-turn unless it is
//! the only way). Everything else (look-ahead, red lights, cruise speeds,
//! impatience and the horn) is the driver shared with police and script
//! cars, `vehicle_ai::controller`.
//!
//! The spawner also places pedestrians (flag 0x20 squares) and, at wanted
//! >= 5, roadblocks (Mac `AddPolice` @ 10058ba0, 68k `FUN_000cfe0c`).

use super::boarding::AiDriver;
use super::people::{self, Ambient, Person};
use super::police::Aggressiveness;
use super::vehicle_ai::{self, Driver, Env, Nav};
use super::{Obj, Rng, State, Stick, Trig};
use crate::audio::Sound;
use crate::camera::{CameraTarget, GameCam, ViewCams};
use crate::data::GameData;
use crate::level::{AppState, GameMode, LevelSpec};
use crate::mission::MissionState;
use crate::objects::ObjectLibrary;
use crate::vehicles::{self, TYPES, Vehicle, kind as vk};
use crate::world::{City, z_of_h};
use bevy::prelude::*;
use payback_formats::map::{self, MAP_SIZE, Slope};

/// Spawn exits in `AddThing` order: (flag bit, dx, dy, heading, direction
/// code).
const EXITS: [(u8, i32, i32, i32, u8); 4] = [
    (0x01, 0, -1, 0xb40, 1),
    (0x02, 1, 0, 0x5a0, 2),
    (0x04, 0, 1, 0, 3),
    (0x08, -1, 0, 0x10e0, 4),
];

/// A traffic driver (state 3). The steering target is [`Nav::aim`].
#[derive(Component, Clone, Debug)]
pub struct Traffic {
    /// Lane direction code 1..4 (N, E, S, W) for "straight on"
    /// (`+0x423`, Mac `+0x90d`); 0 for a car that never had a lane, which
    /// `MoveUnit` treats as west.
    pub dir: u8,
    /// Driving skill 0..99 (`+0x346`).
    pub aggro: i32,
}

/// An object counted in the spawner's budget (`+0x934`, Mac
/// `IncExtraObjs` @ 10051d30): placed by the map's object list or created
/// by the level script.
#[derive(Component)]
pub struct Persistent;

/// Global traffic-light phase 0..5 (`DAT_0034c084`), advancing every 61
/// ticks; UK sequence.
#[derive(Resource, Default, Clone, Copy)]
pub struct LightPhase {
    pub phase: i32,
    /// Ticks into the phase, 0..60 (`DAT_0034c098`); pedestrian-crossing
    /// beacons show their flash model from 25 on.
    pub sub: i32,
}

/// The traffic update chain (vehicle AI runs after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrafficSet;

pub struct TrafficPlugin;

impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightPhase>()
            .init_resource::<SpawnTurn>()
            .add_systems(OnEnter(AppState::Playing), |mut t: ResMut<SpawnTurn>| {
                *t = SpawnTurn::default();
            })
            .add_systems(
                FixedUpdate,
                (advance_lights, spawn_ambient, drive_traffic)
                    .chain()
                    .in_set(TrafficSet)
                    .after(super::people::PeopleThink)
                    .before(super::driving::DrivingSet),
            );
    }
}

fn advance_lights(mut lights: ResMut<LightPhase>) {
    lights.sub += 1;
    if lights.sub > 60 {
        lights.sub -= 61;
        lights.phase = (lights.phase + 1) % 6;
    }
}

fn flags_at(city: &City, sq: IVec2, level: usize) -> u8 {
    if !(0..MAP_SIZE as i32).contains(&sq.x) || !(0..MAP_SIZE as i32).contains(&sq.y) {
        return 0;
    }
    city.map.square(sq.x as usize, sq.y as usize).cubes[level].flags
}

/// Hand a vehicle (back) to the traffic driver (state 3): only the state
/// changes. The car keeps its skill, its lane direction and its steering
/// target ([`Nav::aim`]); the lane code takes over once it gets there
/// (`MoveUnit` @ 10081080).
pub fn resume(aggro: i32, dir: u8) -> Traffic {
    Traffic { dir, aggro }
}

fn centre(sq: IVec2) -> IVec2 {
    IVec2::new((sq.x << 11) | 0x400, (sq.y << 11) | 0x400)
}

/// Cube level holding h-depth `zh` in square (sx, sy): the first cube whose
/// span `[base - height1, base]` contains it, counting down from the plane
/// (Mac `GetCubeNum` @ 100ae750, 68k `FUN_00092070`). None above them all.
fn cube_num(city: &City, sx: usize, sy: usize, zh: i32) -> Option<usize> {
    let mut base = super::collide::PLANE;
    for (i, c) in city.map.square(sx, sy).cubes.iter().enumerate() {
        let top = base - c.height1 as i32;
        if zh <= base && top <= zh {
            return Some(i);
        }
        base = top;
    }
    None
}

/// Lane choice for state 3 (Mac `MoveUnit` @ 10081080; 68k `FUN_000a143e`,
/// OS4 `FUN_010cb7e4`). Once the target point is within
/// `r = max(speed·1000/yaw, front)` of the car on both axes (the Amiga used
/// `5·speed²/yaw`; equal at speed 200), pick the next square from the
/// target square's flags:
/// - the cube level is the one under a point 0x200 above the car (the cube
///   holding `(z − 0x200) >> 4`, minus one; level 2 above every cube), and
///   a level without flow bits borrows them from the level above, then
///   below;
/// - the exit opposite the car's heading quadrant is dropped, unless it is
///   the only one (so a dead end turns round);
/// - `random(n)` over the remaining exits N, E, S, W and 0x10 "straight on"
///   (in the stored direction), and the target moves to that neighbour's
///   centre. No flow bits at all: the target stays put.
pub fn move_unit(
    city: &City,
    obj: &Obj,
    front: i32,
    yaw: i32,
    aim: &mut IVec2,
    dir: &mut u8,
    rng: &mut Rng,
) {
    let r = (obj.speed * 1000 / yaw.max(1)).max(front);
    let (x, y) = (obj.pos.x, obj.pos.y);
    let on_map = |v: i32| (0..MAP_SIZE as i32).contains(&(v >> 11));
    if !on_map(x) || !on_map(y) {
        return;
    }
    if !(x - r <= aim.x && aim.x <= x + r && y - r <= aim.y && aim.y <= y + r) {
        return;
    }
    // (The original reads past the map for a target off its side.)
    if !on_map(aim.x) || !on_map(aim.y) {
        return;
    }
    let (sx, sy) = (aim.x >> 11, aim.y >> 11);
    let level = cube_num(city, sx as usize, sy as usize, (obj.pos.z - 0x200) >> 4)
        .map_or(2, |n| n.saturating_sub(1));
    let cubes = &city.map.square(sx as usize, sy as usize).cubes;
    let mut f = cubes[level].flags & 0x1f;
    if f == 0 && level < 2 {
        f = cubes[level + 1].flags & 0x1f;
    }
    if f == 0 && level > 0 {
        f = cubes[level - 1].flags & 0x1f;
    }
    // Facing E drops W, S drops N... (quadrants centred on the axes).
    let mask = match obj.heading {
        0x2d0..0x870 => 0xf7,
        0x870..0xe10 => 0xfb,
        0xe10..0x13b0 => 0xfd,
        _ => 0xfe,
    };
    if f & mask != 0 {
        f &= mask;
    }
    let n = f.count_ones() as i32;
    if n == 0 {
        return;
    }
    let neighbour = |d: u8| match d {
        1 => IVec2::new(sx * 0x800 + 0x400, sy * 0x800 - 0x400),
        2 => IVec2::new(sx * 0x800 + 0xc00, sy * 0x800 + 0x400),
        3 => IVec2::new(sx * 0x800 + 0x400, sy * 0x800 + 0xc00),
        _ => IVec2::new(sx * 0x800 - 0x400, sy * 0x800 + 0x400),
    };
    let mut k = rng.rand(n);
    for (bit, d) in [(1u8, 1u8), (2, 2), (4, 3), (8, 4)] {
        if f & bit != 0 {
            k -= 1;
            if k == -1 {
                *aim = neighbour(d);
                *dir = d;
                return;
            }
        }
    }
    *aim = neighbour(*dir);
}

/// Which home cities' traffic models (`+0x48d4` 0/1/2) a user map allows:
/// the three booleans of its `.vehicles` file (OS4 `FUN_0106dbbc`; read
/// through `FUN_0106d958/64/70`). No file, or a version other than 1,
/// allows none.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficCities(pub [bool; 3]);

/// Pick a vehicle type for traffic (OS4 `FUN_01047af4` ~37770, 68k
/// `FUN_000cf5cc`): a script-forced type (`DAT_0011a734`) unless one is
/// already out; otherwise draw a model (the police car with chance
/// `1/(10 − wanted)` at wanted >= 4, not in multiplayer; else `random(28)`)
/// and keep it if `random(256) < weight` (`+0x4934`) and its home city
/// (`+0x48d4`) is -1, the current city (cities 0..2), or, on a user map
/// (city >= 3), one the map's `.vehicles` allows. A draw failing the weight
/// test after the 20th gives up; one passing the weight but not the city
/// test does not count against that, and an accepted draw after the 19th
/// is discarded too.
fn pick_type(
    rng: &mut Rng,
    wanted: i32,
    multiplayer: bool,
    city: i32,
    allowed: TrafficCities,
    forced: i32,
    existing: &[usize],
) -> Option<usize> {
    if forced >= 0 && !existing.contains(&(forced as usize)) {
        return Some(forced as usize);
    }
    let mut tries = 0;
    loop {
        tries += 1;
        let t = if !multiplayer && wanted >= 4 && rng.rand(10 - wanted.min(9)) == 0 {
            vk::POLICE
        } else {
            rng.rand(28) as usize
        };
        let ty = &TYPES[t];
        if rng.rand(256) < ty.weight {
            let home = ty.city;
            let ok = home == -1
                || if city <= 2 {
                    home == city
                } else {
                    (0..3).contains(&home) && allowed.0[home as usize]
                };
            if ok {
                return (tries < 20).then_some(t);
            }
        } else if tries > 19 {
            return None;
        }
    }
}

/// The spawner's player rotation in multiplayer (Mac `_DAT_1017c450`,
/// "spawned" flag `_DAT_1017c454`; 68k `DAT_0034c06c` / `DAT_0034c068`).
#[derive(Resource, Default, Clone, Copy)]
pub struct SpawnTurn {
    player: usize,
    spawned: bool,
}

/// The occupancy grid while spawning (updated as objects are added, as
/// `NewObj` @ 100575b0 calls `AddCollideObj`).
struct Grid(Vec<bool>);

impl Grid {
    fn new() -> Grid {
        Grid(vec![false; MAP_SIZE * MAP_SIZE * 3])
    }

    fn add(&mut self, cells: &[(IVec2, usize, Entity)]) {
        for (q, l, _) in cells {
            self.0[(q.x as usize * MAP_SIZE + q.y as usize) * 3 + *l] = true;
        }
    }

    /// The square and its four neighbours are free at `level`.
    fn free5(&self, sq: IVec2, level: usize) -> bool {
        [(0, 0), (-1, 0), (0, -1), (1, 0), (0, 1)].iter().all(|(dx, dy)| {
            !self.0[((sq.x + dx) as usize * MAP_SIZE + (sq.y + dy) as usize) * 3 + level]
        })
    }
}

/// Mark the squares of a just-created object.
fn mark(world: &mut World, grid: &mut Grid, e: Entity) {
    let Some(o) = world.get::<Obj>(e).cloned() else {
        return;
    };
    let cells = vehicle_ai::occupancy(
        std::iter::once((e, &o)),
        world.resource::<ObjectLibrary>(),
        world.resource::<Trig>(),
    );
    grid.add(&cells);
}

/// Every object creation draws `random(100)` for the skill (Mac
/// `CreateObject` @ 10075630, 68k `FUN_000a2b80`).
fn create_draw(world: &mut World) {
    world.resource_mut::<Rng>().rand(100);
}

/// Spawn a person of `kind` (state 3) straight into the world.
fn spawn_person_world(
    world: &mut World,
    kind: usize,
    pos: IVec3,
    heading: i32,
    level: usize,
) -> Option<Entity> {
    // A system state, not `resource_scope`: taking a resource out and back
    // marks it newly added (docs/research.md "Performance").
    let mut st: bevy::ecs::system::SystemState<(
        Commands,
        Res<GameData>,
        ResMut<ObjectLibrary>,
        ResMut<Assets<Mesh>>,
    )> = bevy::ecs::system::SystemState::new(world);
    let e = {
        let Ok((mut commands, data, mut library, mut meshes)) = st.get_mut(world) else {
            return None;
        };
        people::spawn_person(
            &mut commands,
            &data,
            &mut library,
            &mut meshes,
            kind,
            pos,
            heading,
            State::Wander,
        )
    };
    st.apply(world);
    world.flush();
    if let Some(mut o) = e.and_then(|e| world.get_mut::<Obj>(e)) {
        o.level = level;
    }
    e
}

/// Ground under the centre of square `sq` at `level`, as an object z.
fn ground_z(city: &City, sq: IVec2, level: usize) -> i32 {
    let top = city.square_cubes(sq.x as usize, sq.y as usize)[level].top_at(0.5, 0.5);
    z_of_h(top) as i32
}

/// Per-tick spawner (Mac `AddVehicles` @ 10059d40, 68k `FUN_000d0cfc`, OS4
/// `FUN_010485cc`). It runs while the object table holds at most
/// `min(extra + 11, 20)` objects (the Amiga: `extra + 8`; extra =
/// [`Persistent`] objects, plus two per player in multiplayer) and makes
/// `2·ticks` passes. Each pass picks the player (multiplayer: the next one
/// after a pass that spawned something) and the edge of that player's view
/// window ahead of its motion: the dominant axis of its velocity (y on a
/// tie), or with no motion a random one of N, E, S (`random(3)`, never W).
/// Then for each cube level 0..2: `AddThing` at a random square of that
/// edge, and at wanted >= 5 (single player) `(wanted − 4)·2 + 1` roadblock
/// tries on the same edge and level. The budget is re-checked before each
/// try.
fn spawn_ambient(world: &mut World) {
    if !world.contains_resource::<City>() || !world.contains_resource::<ObjectLibrary>() {
        return;
    }
    let Some(cam) = world.get_resource::<GameCam>().copied() else {
        return;
    };
    let mode = *world.resource::<GameMode>();
    let mp = mode.is_multiplayer();
    let views = world.resource::<ViewCams>().0.clone();
    let cams = if views.is_empty() { vec![cam] } else { views };
    let players: Vec<(Option<u8>, IVec3, i32)> = world
        .query_filtered::<&Obj, With<CameraTarget>>()
        .iter(world)
        .map(|o| (o.player, o.vel, o.wanted))
        .collect();
    // The player in slot p (single player: the one camera target).
    let player = |p: usize| {
        if mp {
            players.iter().find(|x| x.0 == Some(p as u8)).copied()
        } else {
            players.first().copied()
        }
    };
    let extra = world.query_filtered::<(), With<Persistent>>().iter(world).count() as i32
        + if mp { 2 * mode.players() as i32 } else { 0 };
    let cap = (extra + 11).min(20);
    let mut count = world.query::<&Obj>().iter(world).count() as i32;
    if count - 1 >= cap {
        return;
    }
    let mut grid = Grid::new();
    {
        let mut q = world.query::<(Entity, &Obj)>();
        let cells = vehicle_ai::occupancy(
            q.iter(world),
            world.resource::<ObjectLibrary>(),
            world.resource::<Trig>(),
        );
        grid.add(&cells);
    }
    let mut turn = *world.resource::<SpawnTurn>();
    let window = |c: &GameCam| {
        let sq = c.square().clamp(IVec2::ZERO, IVec2::splat(MAP_SIZE as i32 - 1));
        (sq, c.window())
    };
    for _ in 0..2 {
        if !mp {
            turn.player = 0;
        } else {
            let n = cams.len().max(1);
            if turn.spawned {
                turn.player = (turn.player + 1) % 4;
            }
            turn.spawned = false;
            while turn.player >= n {
                turn.player = (turn.player + 1) % 4;
            }
        }
        // No player object (between lives): no spawning this pass.
        let Some((_, v, _)) = player(turn.player) else {
            continue;
        };
        let (c, half) = window(&cams[turn.player.min(cams.len() - 1)]);
        let width = half * 2 + 1;
        let edge = if v.y.abs() < v.x.abs() {
            if v.x > 0 { 1 } else { 3 }
        } else if v.x == 0 && v.y == 0 {
            world.resource_mut::<Rng>().rand(3)
        } else if v.y > 0 {
            2
        } else {
            0
        };
        let at = |along: i32| match edge {
            0 => IVec2::new(along + c.x - half, c.y - half),
            1 => IVec2::new(c.x + half, along + c.y - half),
            2 => IVec2::new(along + c.x - half, c.y + half),
            _ => IVec2::new(c.x - half, along + c.y - half),
        };
        let others: Vec<(IVec2, i32)> = if mp {
            (0..cams.len())
                .filter(|&i| i != turn.player && player(i).is_some())
                .map(|i| window(&cams[i]))
                .collect()
        } else {
            Vec::new()
        };
        for level in 0..3 {
            if count - 1 >= cap {
                continue;
            }
            let along = world.resource_mut::<Rng>().rand(width);
            if add_thing(world, &mut grid, at(along), level, &others) {
                count = world.query::<&Obj>().iter(world).count() as i32;
                if mp {
                    turn.spawned = true;
                }
            }
            let wanted = player(0).map_or(0, |p| p.2);
            if !mp && wanted > 4 {
                for _ in 0..=(wanted - 4) * 2 {
                    if count - 1 < cap {
                        let along = world.resource_mut::<Rng>().rand(width);
                        add_police(world, &mut grid, at(along), level);
                        count = world.query::<&Obj>().iter(world).count() as i32;
                    }
                }
            }
        }
    }
    *world.resource_mut::<SpawnTurn>() = turn;
}

/// One spawn try at square `sq`, cube level `level` (Mac `AddThing`
/// @ 10058350, 68k `FUN_000cf5cc`, OS4 `FUN_01047af4`). In multiplayer the
/// square must lie outside the other players' windows. The square must be
/// inside 1..126, its cube exist (slope byte `+8` != 0) with a top face that
/// is neither Off nor Reflect (`+9` 0 / 4), and it and its four neighbours
/// be free at that level. Its choices are the flow exits N, E, S, W whose
/// neighbour continues the same flow, plus one for a pavement (flag 0x20);
/// `random(n)` picks one. A pavement pick spawns a pedestrian 1 time in 5;
/// an exit spawns a traffic car facing it (`pick_type`).
fn add_thing(
    world: &mut World,
    grid: &mut Grid,
    sq: IVec2,
    level: usize,
    others: &[(IVec2, i32)],
) -> bool {
    if others
        .iter()
        .any(|&(c, h)| c.x - h < sq.x && sq.x < c.x + h && c.y - h < sq.y && sq.y < c.y + h)
    {
        return false;
    }
    if !(1..MAP_SIZE as i32 - 1).contains(&sq.x) || !(1..MAP_SIZE as i32 - 1).contains(&sq.y) {
        return false;
    }
    let (flags, exits) = {
        let city = world.resource::<City>();
        let cube = &city.map.square(sq.x as usize, sq.y as usize).cubes[level];
        let top = cube.faces[map::TOP];
        if cube.slope == Slope::Off || !top.visible() || top.kind == map::FaceType::Reflect {
            return false;
        }
        let exits: Vec<_> = EXITS
            .iter()
            .filter(|(bit, dx, dy, ..)| {
                cube.flags & bit != 0
                    && flags_at(city, sq + IVec2::new(*dx, *dy), level) & bit != 0
            })
            .copied()
            .collect();
        (cube.flags, exits)
    };
    if !grid.free5(sq, level) {
        return false;
    }
    let n = exits.len() as i32 + i32::from(flags & 0x20 != 0);
    if n == 0 {
        return false;
    }
    let k = world.resource_mut::<Rng>().rand(n) as usize;
    let Some(&(_, _, _, heading, dir)) = exits.get(k) else {
        // The pavement: a pedestrian 1 time in 5 (PedMale1-4, table indices
        // 6, 8, 9, 10), facing anywhere, its wander anchor on the square's
        // centre (Mac `+0x7d4/+0x7d8`).
        let (kind, heading) = {
            let mut rng = world.resource_mut::<Rng>();
            if rng.rand(5) != 0 {
                return false;
            }
            let r = rng.rand(4) as usize;
            let kind = if r > 0 { r + 7 } else { 6 };
            (kind, rng.rand(0x167f))
        };
        let z = ground_z(world.resource::<City>(), sq, level) - 0x200;
        let pos = centre(sq).extend(z);
        create_draw(world);
        let Some(e) = spawn_person_world(world, kind, pos, heading, level) else {
            return false;
        };
        world.entity_mut(e).insert(Ambient);
        if let Some(mut p) = world.get_mut::<Person>(e) {
            p.anchor = pos.truncate();
        }
        mark(world, grid, e);
        return true;
    };
    let wanted = world
        .query_filtered::<&Obj, With<CameraTarget>>()
        .iter(world)
        .next()
        .map_or(0, |o| o.wanted);
    let existing: Vec<usize> = world.query::<&Vehicle>().iter(world).map(|v| v.kind).collect();
    let forced = world.resource::<MissionState>().forced_traffic;
    // `DAT_0114c694`: 0..2, or 3 and up for a user map.
    let city_no: i32 = world.resource::<LevelSpec>().city().map_or(3, i32::from);
    let allowed = world.get_resource::<TrafficCities>().copied().unwrap_or_default();
    let multiplayer = world.resource::<GameMode>().is_multiplayer();
    let aggr = world.get_resource::<Aggressiveness>().map_or(2, |a| a.0);
    let picked = pick_type(
        &mut world.resource_mut::<Rng>(),
        wanted,
        multiplayer,
        city_no,
        allowed,
        forced,
        &existing,
    );
    let Some(kind) = picked else {
        return false;
    };
    let z = ground_z(world.resource::<City>(), sq, level) - 0x200;
    let pos = centre(sq).extend(z);
    create_draw(world);
    // Paint (`AddThing` tail): Shine/Cardef pair `random(8)`; white vans'
    // drivers skip the -30.
    let (colour, aggro) = {
        let mut rng = world.resource_mut::<Rng>();
        let colour = rng.rand(8);
        let aggro = if kind == 7 && colour == 7 {
            (rng.rand(90) + aggr * 8).min(99)
        } else {
            (rng.rand(90) + aggr * 8 - 30).clamp(0, 99)
        };
        (colour, aggro)
    };
    let Some(e) = vehicles::spawn_vehicle_world(world, kind as i32, pos, heading, colour) else {
        return false;
    };
    if let Some(mut o) = world.get_mut::<Obj>(e) {
        o.state = State::Wander;
        o.level = level;
        // The Mac starts new traffic at speed min(maxF, 100); the Amiga
        // (`FUN_000cf5cc`) left it 0. Only the speed: grip brings the
        // velocity up.
        o.speed = TYPES[kind].max_speed.min(100);
    }
    // The steering target starts on the spawn square's centre, so the lane
    // code picks the first square at once (`CreateObject` @ 10075630).
    world
        .entity_mut(e)
        .insert((Traffic { dir, aggro }, Nav::at(pos), Ambient, AiDriver));
    mark(world, grid, e);
    true
}

/// A roadblock try at square `sq`, level `level` (Mac `AddPolice`
/// @ 10058ba0, 68k `FUN_000cfe0c`, OS4 `FUN_01047134`): inside 1..126, the
/// cube exists, it and its four neighbours are free, and it has flag 0x40.
/// The run of 0x40 squares through it across the flow gets alternately a
/// parked police car (state 5, +500 along the run) and a barrier (state
/// 0xc, +600) with a shotgun cop (state 3, +0x600, facing anywhere). N/S
/// flow: the run goes along x, heading 0x5a0; E/W: along y, heading 0.
fn add_police(world: &mut World, grid: &mut Grid, sq: IVec2, level: usize) {
    if !(1..MAP_SIZE as i32 - 1).contains(&sq.x) || !(1..MAP_SIZE as i32 - 1).contains(&sq.y) {
        return;
    }
    let (run, step) = {
        let city = world.resource::<City>();
        let cube = &city.map.square(sq.x as usize, sq.y as usize).cubes[level];
        let f = cube.flags;
        if cube.slope == Slope::Off || !grid.free5(sq, level) || f & 0x40 == 0 {
            return;
        }
        let step = if f & 0x05 != 0 {
            IVec2::X
        } else if f & 0x0a != 0 {
            IVec2::Y
        } else {
            return;
        };
        let mut a = sq;
        while flags_at(city, a - step, level) & 0x40 != 0 {
            a -= step;
        }
        let mut b = sq;
        while flags_at(city, b + step, level) & 0x40 != 0 {
            b += step;
        }
        ((a, b), step)
    };
    let heading = if step == IVec2::X { 0x5a0 } else { 0 };
    info!("roadblock {}..{} level {level}", run.0, run.1);
    let mut q = run.0;
    let mut car = true;
    loop {
        let base = IVec2::new(q.x << 11, q.y << 11);
        let at = |world: &World, off: i32| {
            let mid = base + IVec2::splat(0x400);
            let p = if step == IVec2::X {
                IVec2::new(base.x + off, mid.y)
            } else {
                IVec2::new(mid.x, base.y + off)
            };
            p.extend(ground_z(world.resource::<City>(), q, level))
        };
        if car {
            let pos = at(world, 500);
            create_draw(world);
            if let Some(e) =
                vehicles::spawn_vehicle_world(world, vk::POLICE as i32, pos, heading, 0)
            {
                if let Some(mut o) = world.get_mut::<Obj>(e) {
                    o.level = level;
                }
                world
                    .entity_mut(e)
                    .insert((Ambient, super::pursuit::Roadblock, Nav::at(pos)));
                mark(world, grid, e);
            }
        } else {
            let pos = at(world, 600);
            create_draw(world);
            if let Some(e) = vehicles::spawn_prop_world(world, 0, pos, heading) {
                if let Some(mut o) = world.get_mut::<Obj>(e) {
                    o.level = level;
                }
                world.entity_mut(e).insert(super::pursuit::Roadblock);
                mark(world, grid, e);
            }
            let h = world.resource_mut::<Rng>().rand(0x167f);
            let pos = at(world, 0x600);
            create_draw(world);
            if let Some(cop) = super::pursuit::spawn_cop_world(world, pos, h, level) {
                world.entity_mut(cop).insert(super::pursuit::Roadblock);
                mark(world, grid, cop);
            }
        }
        car = !car;
        if q == run.1 {
            break;
        }
        q += step;
    }
}

/// Drive the traffic cars (state 3): the lane choice, then the shared
/// driver with the car's skill and an unclamped aim (`do_Other`
/// @ 10146280).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn drive_traffic(
    mut commands: Commands,
    city: Option<Res<City>>,
    library: Option<Res<ObjectLibrary>>,
    lights: Res<LightPhase>,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    mut sounds: Query<&mut Sound>,
    mut set: ParamSet<(
        Query<(Entity, &Obj)>,
        Query<(
            Entity,
            &mut Obj,
            &Vehicle,
            &mut Traffic,
            &mut Stick,
            &super::driving::Drive,
            Option<&mut Nav>,
            Option<&super::ai::Ai>,
        )>,
    )>,
) {
    let (Some(city), Some(library)) = (city, library) else {
        return;
    };
    let occ = vehicle_ai::occupancy(set.p0().iter(), &library, &trig);
    let env = Env {
        city: &city,
        lights: lights.phase,
        occ: &occ,
    };
    let order = crate::sim::table_order(set.p1().iter().map(|(e, o, ..)| (e, o)));
    let mut q = set.p1();
    for e__ in order {
        let Ok((e, mut obj, veh, mut tr, mut stick, drive, nav, ai)) = q.get_mut(e__) else {
            continue;
        };
        if obj.state != State::Wander || obj.burn > 0 {
            continue;
        }
        let Some(mut nav) = nav else {
            commands.entity(e).insert(Nav::at(obj.pos));
            continue;
        };
        let t = &TYPES[veh.kind];
        let front = library.model(obj.model).info.max_y;
        move_unit(&city, &obj, front, t.turn, &mut nav.aim, &mut tr.dir, &mut rng);
        let driver = Driver {
            skill: tr.aggro,
            police: t.police,
            direct: false,
            clamp: false,
        };
        let aim = nav.aim;
        let s = vehicle_ai::controller(
            e, &obj, t, front, drive, driver, aim, &env, &mut nav, &mut rng, &trig,
        );
        vehicle_ai::horn(&mut commands, &mut sounds, &mut nav, e, veh, &obj, s.fire);
        *stick = s;
        // Script speed cap (`+0x386`, Mac `+0x868`).
        if let Some(ai) = ai {
            if obj.speed > ai.max_speed {
                obj.speed = ai.max_speed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng() -> Rng {
        let mut s: u32 = 0x1234_5678;
        let table = (0..65536)
            .map(|_| {
                s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
                ((s >> 16) & 0x7fff) as i32 >> 6
            })
            .collect();
        Rng { table, idx: 0 }
    }

    /// Home-city models (ice-cream van 0, police van 1, truck 2) on a user
    /// map only with the `.vehicles` flag; on a built-in city only at home.
    #[test]
    fn user_map_city_flags() {
        let homes = |city: i32, allowed: [bool; 3]| {
            let mut r = rng();
            let mut seen = [false; 3];
            for _ in 0..20000 {
                if let Some(t) = pick_type(&mut r, 0, false, city, TrafficCities(allowed), -1, &[]) {
                    if let Ok(h) = usize::try_from(TYPES[t].city) {
                        seen[h] = true;
                    }
                }
            }
            seen
        };
        assert_eq!(homes(3, [false; 3]), [false; 3]);
        assert_eq!(homes(3, [true, false, true]), [true, false, true]);
        assert_eq!(homes(3, [false, true, false]), [false, true, false]);
        assert_eq!(homes(1, [false; 3]), [false, true, false]);
        assert_eq!(homes(0, [false, true, true]), [true, false, false]);
    }
}
