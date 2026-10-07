//! Ambient traffic (68k `FUN_000d0cfc` spawner, `FUN_000cf5cc` square test,
//! `FUN_000a143e` junction choice, driver AI in `FUN_000e3a58`).
//!
//! Each map square is one one-way lane; flags bits 1/2/4/8 allow leaving
//! N/E/S/W and 0x10 means "straight on". Cars aim at square centres, choose
//! exits at random (no U-turns), brake for anything occupying the squares
//! ahead and for red lights, and honk when stuck.

use super::people::Ambient;
use super::police::Aggressiveness;
use super::{Class, Obj, Rng, State, Stick, angle_diff, heading_of};
use crate::audio::{Sound, SoundPos};
use crate::camera::{CameraTarget, GameCam};
use crate::level::LevelSpec;
use crate::mission::MissionState;
use crate::vehicles::{self, TYPES, Vehicle, kind as vk};
use crate::world::{City, z_of_h};
use bevy::prelude::*;
use payback_formats::map::{self, MAP_SIZE, Slope};

/// Exit directions: (flag bit, dx, dy, heading, direction code).
const EXITS: [(u8, i32, i32, i32, u8); 4] = [
    (0x01, 0, -1, 0xb40, 1),
    (0x02, 1, 0, 0x5a0, 2),
    (0x04, 0, 1, 0, 3),
    (0x08, -1, 0, 0x10e0, 4),
];

/// A traffic driver.
#[derive(Component, Clone, Debug)]
pub struct Traffic {
    /// Target square.
    pub target: IVec2,
    /// Current direction code 1..4 (N, E, S, W).
    pub dir: u8,
    /// Driving aggression 0..99 (`+0x346`).
    pub aggro: i32,
    /// Impatience / horn timer (`+0x450`).
    pub horn: i32,
    pub horn_sound: Option<Entity>,
}

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
        app.init_resource::<LightPhase>().add_systems(
            FixedUpdate,
            (advance_lights, spawn_traffic, drive_traffic)
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

/// Hand a vehicle (back) to the traffic driver: aim at the next square in
/// the lane direction nearest its heading.
pub fn resume(obj: &Obj, aggro: i32) -> Traffic {
    let (_, dx, dy, _, dir) = *EXITS
        .iter()
        .min_by_key(|x| angle_diff(obj.heading, x.3).abs())
        .unwrap_or(&EXITS[2]);
    Traffic {
        target: IVec2::new((obj.pos.x >> 11) + dx, (obj.pos.y >> 11) + dy),
        dir,
        aggro,
        horn: 0,
        horn_sound: None,
    }
}

fn centre(sq: IVec2) -> IVec2 {
    IVec2::new((sq.x << 11) | 0x400, (sq.y << 11) | 0x400)
}

/// Pick a vehicle type for traffic (up to 20 tries; police more likely at
/// wanted >= 4; forced type from scripts).
fn pick_type(
    rng: &mut Rng,
    wanted: i32,
    city: i32,
    forced: i32,
    existing: &[usize],
) -> Option<usize> {
    if forced >= 0 && !existing.contains(&(forced as usize)) {
        return Some(forced as usize);
    }
    for _ in 0..20 {
        if wanted >= 4 && rng.rand(10 - wanted.min(9)) == 0 {
            return Some(vk::POLICE);
        }
        let t = rng.rand(28) as usize;
        let ty = &TYPES[t];
        if rng.rand(256) < ty.weight && (ty.city == -1 || ty.city == city) {
            return Some(t);
        }
    }
    None
}

/// Spawn traffic on lane squares at the edge of the view window.
fn spawn_traffic(world: &mut World) {
    let objects = world.query::<&Obj>().iter(world).count();
    if objects >= 20 || !world.contains_resource::<City>() {
        return;
    }
    let Some(cam) = world.get_resource::<GameCam>().copied() else {
        return;
    };
    let (player_vel, wanted) = world
        .query_filtered::<&Obj, With<CameraTarget>>()
        .iter(world)
        .next()
        .map(|o| (o.vel, o.wanted))
        .unwrap_or((IVec3::ZERO, 0));
    let mut occupied = vec![false; MAP_SIZE * MAP_SIZE];
    for o in world.query::<&Obj>().iter(world) {
        let (x, y) = (o.pos.x >> 11, o.pos.y >> 11);
        if (0..MAP_SIZE as i32).contains(&x) && (0..MAP_SIZE as i32).contains(&y) {
            occupied[x as usize * MAP_SIZE + y as usize] = true;
        }
    }
    let existing: Vec<usize> = world
        .query::<&Vehicle>()
        .iter(world)
        .map(|v| v.kind)
        .collect();
    let forced = world.resource::<MissionState>().forced_traffic;
    let city_no: i32 = world.resource::<LevelSpec>().map.parse().unwrap_or(-1);
    let aggr = world.get_resource::<Aggressiveness>().map_or(2, |a| a.0);
    let c = cam.square();
    let h = cam.window();
    let spawns = world.resource_scope(|world, city: Mut<City>| {
        let mut rng = world.resource_mut::<Rng>();
        let mut spawns = Vec::new();
        for _ in 0..2 {
            let along = rng.rand(2 * h + 1) - h;
            let v = player_vel;
            let edge = if v.x.abs() >= v.y.abs() && v.x != 0 {
                if v.x > 0 { 1 } else { 3 }
            } else if v.y != 0 {
                if v.y > 0 { 2 } else { 0 }
            } else {
                rng.rand(3)
            };
            let sq = match edge {
                0 => IVec2::new(c.x + along, c.y - h),
                1 => IVec2::new(c.x + h, c.y + along),
                2 => IVec2::new(c.x + along, c.y + h),
                _ => IVec2::new(c.x - h, c.y + along),
            };
            if !(1..MAP_SIZE as i32 - 1).contains(&sq.x)
                || !(1..MAP_SIZE as i32 - 1).contains(&sq.y)
            {
                continue;
            }
            let busy = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dy)| occupied[(sq.x + dx) as usize * MAP_SIZE + (sq.y + dy) as usize]);
            if busy {
                continue;
            }
            for level in 0..3 {
                let cube = &city.map.square(sq.x as usize, sq.y as usize).cubes[level];
                let top = cube.faces[map::TOP];
                if cube.slope == Slope::Off || !top.visible() || top.kind == map::FaceType::Reflect
                {
                    continue;
                }
                // Exits whose neighbour continues the same flow.
                let exits: Vec<_> = EXITS
                    .iter()
                    .filter(|(bit, dx, dy, ..)| {
                        cube.flags & bit != 0
                            && flags_at(&city, sq + IVec2::new(*dx, *dy), level) & bit != 0
                    })
                    .copied()
                    .collect();
                if exits.is_empty() {
                    continue;
                }
                let (_, dx, dy, heading, dir) = exits[rng.rand(exits.len() as i32) as usize];
                let Some(kind) = pick_type(&mut rng, wanted, city_no, forced, &existing) else {
                    continue;
                };
                let top_h = city.square_cubes(sq.x as usize, sq.y as usize)[level].top_at(0.5, 0.5);
                let pos = centre(sq).extend(z_of_h(top_h) as i32 - 0x200);
                // The white van's drivers skip the -30.
                let aggro = if kind == 7 {
                    (rng.rand(90) + aggr * 8).min(99)
                } else {
                    (rng.rand(90) + aggr * 8 - 30).clamp(0, 99)
                };
                spawns.push((
                    kind,
                    pos,
                    heading,
                    dir,
                    sq + IVec2::new(dx, dy),
                    aggro,
                    level,
                ));
                break;
            }
        }
        spawns
    });
    for (kind, pos, heading, dir, target, aggro, level) in spawns {
        if let Some(e) = vehicles::spawn_vehicle_world(world, kind as i32, pos, heading, 0) {
            if let Some(mut o) = world.get_mut::<Obj>(e) {
                o.state = State::Wander;
                o.level = level;
            }
            world.entity_mut(e).insert((
                Traffic {
                    target,
                    dir,
                    aggro,
                    horn: 0,
                    horn_sound: None,
                },
                Ambient,
            ));
        }
    }
}

/// Cruise speed (straight, steering) and the minimum below which a
/// driver accelerates, by aggression.
fn cruise(aggro: i32, police_chase: bool) -> (i32, i32, i32, i32) {
    if police_chase {
        (730, 500, 240, 120)
    } else if aggro <= 50 {
        (150, 80, 100, 50)
    } else if aggro <= 90 {
        (200, 100, 140, 70)
    } else if aggro <= 94 {
        (230, 150, 180, 90)
    } else {
        (330, 220, 200, 100)
    }
}

#[allow(clippy::type_complexity)]
fn drive_traffic(
    mut commands: Commands,
    city: Option<Res<City>>,
    lights: Res<LightPhase>,
    mut rng: ResMut<Rng>,
    others: Query<(Entity, &Obj)>,
    mut q: Query<(Entity, &Obj, &Vehicle, &mut Traffic, &mut Stick)>,
) {
    let Some(city) = city else { return };
    // Occupancy of other objects by square and level.
    let mut occ: Vec<(IVec2, usize, Entity)> = Vec::new();
    for (e, o) in &others {
        if o.class != Class::Projectile {
            occ.push((IVec2::new(o.pos.x >> 11, o.pos.y >> 11), o.level, e));
        }
    }
    for (e, obj, veh, mut tr, mut stick) in &mut q {
        if obj.state != State::Wander {
            continue;
        }
        let t = &TYPES[veh.kind];
        let pos = obj.pos.truncate();
        // Junction: close enough to the target square's centre.
        let r = (obj.speed * obj.speed * 5 / t.turn.max(1)).max(512);
        let goal = centre(tr.target);
        if (goal - pos).abs().max_element() <= r {
            let f = flags_at(&city, tr.target, obj.level) & 0x1f;
            // No U-turns.
            let back = match tr.dir {
                1 => 0x04,
                2 => 0x08,
                3 => 0x01,
                _ => 0x02,
            };
            let mut choices: Vec<u8> = [0x01, 0x02, 0x04, 0x08, 0x10]
                .into_iter()
                .filter(|b| f & b != 0 && *b != back)
                .collect();
            if choices.is_empty() {
                choices.push(0x10);
            }
            let pick = choices[rng.rand(choices.len() as i32) as usize];
            let dir = if pick == 0x10 {
                tr.dir
            } else {
                EXITS.iter().find(|x| x.0 == pick).map_or(tr.dir, |x| x.4)
            };
            let (_, dx, dy, _, _) = EXITS[(dir.clamp(1, 4) - 1) as usize];
            tr.dir = dir;
            tr.target += IVec2::new(dx, dy);
        }
        let goal = centre(tr.target);
        let want = heading_of(goal.x - pos.x, goal.y - pos.y);
        let err = angle_diff(obj.heading, want);
        let steering = err.abs() > t.turn / 2;
        stick.steer = if !steering {
            0
        } else if err > 0 {
            -1
        } else {
            1
        };

        // Look ahead for obstacles and red lights.
        let mut blocked = false;
        let reach = (0x800).max(512 + obj.speed * obj.speed * 4 / t.brake.max(1) + 0x400);
        let dir = (goal - pos).as_vec2().normalize_or_zero();
        let mut d = 0x800;
        let mut last_sq = IVec2::new(pos.x >> 11, pos.y >> 11);
        while d <= reach {
            let p = pos + (dir * d as f32).as_ivec2();
            let sq = IVec2::new(p.x >> 11, p.y >> 11);
            last_sq = sq;
            if !(0..MAP_SIZE as i32).contains(&sq.x) || !(0..MAP_SIZE as i32).contains(&sq.y) {
                blocked = true;
                break;
            }
            if occ
                .iter()
                .any(|(s, l, oe)| *s == sq && *l == obj.level && *oe != e)
            {
                blocked = true;
                break;
            }
            d += 0x800;
        }
        if !blocked
            && tr.aggro < 70
            && (0..MAP_SIZE as i32).contains(&last_sq.x)
            && (0..MAP_SIZE as i32).contains(&last_sq.y)
        {
            let special = city
                .map
                .square(last_sq.x as usize, last_sq.y as usize)
                .cubes[obj.level]
                .special as i32;
            if (1..=6).contains(&special) {
                let mut v = special + lights.phase;
                if v > 6 {
                    v -= 6;
                }
                blocked = matches!(v, 1 | 2 | 3 | 6);
            }
        }

        // Impatience: honk and nudge forward now and then.
        if blocked {
            tr.horn -= 1;
            if tr.horn <= 0 {
                let (base, range) = match tr.aggro {
                    91.. => (20, 20),
                    61..=90 => (50, 50),
                    _ => (100, 50),
                };
                tr.horn = rng.rand(range) + base;
                if tr.aggro > 30 && tr.horn_sound.is_none() {
                    let hz = (16_000_000 / t.mass.max(1)).clamp(3000, 20000) as f32;
                    let s = commands
                        .spawn(Sound::once(
                            t.horn,
                            20_000_000.0,
                            hz,
                            SoundPos::At(obj.posf()),
                        ))
                        .id();
                    tr.horn_sound = Some(s);
                }
                blocked = false;
            }
        } else {
            tr.horn_sound = None;
        }

        let (v_straight, lo_straight, v_turn, lo_turn) = cruise(tr.aggro, false);
        let (v, lo) = if steering {
            (v_turn, lo_turn)
        } else {
            (v_straight, lo_straight)
        };
        stick.throttle = if blocked {
            if obj.speed > t.brake / 2 { 1 } else { 0 }
        } else if obj.speed < v.max(lo) - t.accel {
            -1
        } else if obj.speed > v + t.brake {
            1
        } else {
            0
        };
    }
}
