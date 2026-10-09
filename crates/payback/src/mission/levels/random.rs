//! Random missions in Rampage (Mac only; docs/re/missions_random.md).
//!
//! The Mac release runs a small script in Rampage on any map with phones
//! (`amiga_startlevel` @ 100fff40 calls `InitScript_Random` @ 10129e50 when
//! `GetNumAltPhones() > 0`; city index 3 routes `ProcessTrigger` @ 1013e2c0
//! to `ProcessTrigger_Random` @ 1012a0a0). A random map phone rings; when
//! it is answered one of seven jobs is picked at random. Timers, rewards
//! and wanted levels scale with the police aggressiveness `P` (0..5,
//! `_DAT_10188fe4`). The text is global phrases 1400-1470 and 1800-1842,
//! which only the Mac phrase files have. Neither Amiga build has any of
//! this.
//!
//! Positions are squares (`GetSafePos` returns cube 0). The case bodies
//! are in the jump-table re-export of `analysis/ghidra_macx`.

use crate::mission::api::{Fmt, MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::{LevelScript, Script, kind};
use crate::sim::Obj;
use crate::sim::vehicle_ai::RouteGraph;
use crate::vehicles::TYPES;
use crate::world::{City, z_of_h};
use bevy::prelude::*;
use payback_formats::map::{FaceType, MAP_SIZE, Slope};

/// "ANSWER THE PHONE" and the welcome line (`%s` = map name).
pub const ANSWER_THE_PHONE: u32 = 1400;
const WELCOME: u32 = 1401;
/// Success lines 1404-1406 (`%d` = thousands), failure lines 1407-1409.
const PAID: u32 = 1404;
const FAILED: u32 = 1407;
/// "Nice one - only %d left!" .. 1425.
const LEFT: u32 = 1422;
/// Location names 1830 + [`point_location`].
const LOCATION: u32 = 1830;
/// "Things are a bit too quiet ..." announce.
const POINTS_TITLE: u32 = 2029;

/// The Random-mission script state (`_DAT_1017d324..d338`, `cbbc`).
pub struct Random {
    /// Police aggressiveness P (0..5).
    police: i32,
    /// Top Rampage score for P (`GetHighestScore` @ 100a05a0): the money
    /// goal.
    goal: i32,
    /// `GetJustCityName` @ 10084e40 of the map.
    city_name: String,
    /// `GetHighestLevelCompleted` @ 1009b580: story levels with a cup
    /// (0..6), which unlocks vehicles for the car jobs.
    progress: i32,
    /// Time limit in seconds, 0 = none (`d334`).
    time: i32,
    /// Gang size / cop count at the start (`d328`).
    count: i32,
    /// Still to kill (`cbbc`).
    left: i32,
    /// Steal job mode 0..2, gang job timed (`d330`).
    mode: i32,
    /// The gang has been set on the player (`d32c`).
    roused: bool,
    /// Checkpoint reached (`d324`).
    checkpoint: usize,
    /// Vehicle type of the job (`d338`).
    vehicle: i32,
    /// Job position (`d098`) and drop-off (`d090`), squares.
    pos: IVec2,
    dest: IVec2,
    /// Race checkpoints (`d094`).
    points: [IVec2; 5],
}

impl Random {
    pub fn new(police: i32, goal: i32, city_name: String, progress: i32) -> Random {
        Random {
            police,
            goal,
            city_name,
            progress,
            time: 0,
            count: 0,
            left: 0,
            mode: 0,
            roused: false,
            checkpoint: 0,
            vehicle: 0,
            pos: IVec2::ZERO,
            dest: IVec2::ZERO,
            points: [IVec2::ZERO; 5],
        }
    }
}

/// Square centre in object units.
fn centre(p: IVec2) -> IVec2 {
    IVec2::new(p.x * 0x800 + 0x400, p.y * 0x800 + 0x400)
}

/// `GetPointLocation` @ 10129f50: the map split at squares 43 and 86 into
/// N 0, NE 1, E 2, SE 3, S 4, SW 5, W 6, NW 7 and centre 8.
pub fn point_location(p: IVec2) -> u32 {
    let (x, y) = (p.x, p.y);
    if x < 43 {
        if y < 43 {
            7
        } else if y > 85 {
            5
        } else {
            6
        }
    } else if x > 85 {
        if y < 43 {
            1
        } else if y > 85 {
            3
        } else {
            2
        }
    } else if y < 43 {
        0
    } else if y < 86 {
        8
    } else {
        4
    }
}

/// `vehicle+0x8134 · vehicle+0x813c · 2` (max speed × acceleration × 2),
/// the divisor of `EstimateTime`; at least 1.
fn speed_factor(vehicle: i32) -> i32 {
    TYPES
        .get(vehicle as usize)
        .map_or(1, |t| (t.max_speed * t.accel * 2).max(1))
}

/// `EstimateTime(p, vehicle)` @ 10057c60: seconds from the player to square
/// `p`: the Manhattan distance >> 12 on foot (`vehicle` None), else divided
/// by the vehicle's [`speed_factor`].
fn estimate_time(s: &mut Script, p: IVec2, vehicle: Option<i32>) -> i32 {
    let me = s
        .player()
        .and_then(|e| s.world.get::<Obj>(e))
        .map_or(IVec2::ZERO, |o| o.pos.truncate());
    let c = centre(p);
    let d = (c.x - me.x).abs() + (c.y - me.y).abs();
    match vehicle {
        None => d >> 12,
        Some(v) => d / speed_factor(v),
    }
}

/// `EstimateTime2(a, b, vehicle)` @ 10057d60: seconds between two squares,
/// Manhattan squares >> 1 on foot, else ·0x800 / [`speed_factor`].
fn estimate_time2(a: IVec2, b: IVec2, vehicle: Option<i32>) -> i32 {
    let d = (a.x - b.x).abs() + (a.y - b.y).abs();
    match vehicle {
        None => d >> 1,
        Some(v) => d * 0x800 / speed_factor(v),
    }
}

/// `GetSafePos` @ 100b42b0: a random square 1..126 whose ground cube is
/// there (slope not Off) with a top face that isn't off or water
/// (Reflect), nothing in cubes 1-2, and a line of sight from the nearest
/// route node. After 4000 tries any such square will do, after 5000 any
/// square at all. Always cube 0.
fn safe_pos(s: &mut Script) -> IVec2 {
    let mut tries = 0;
    loop {
        let x = s.random(0x7e) + 1;
        let y = s.random(0x7e) + 1;
        if let Some(ground_z) = open_ground(s.world, x as usize, y as usize) {
            if tries > 4000 {
                return IVec2::new(x, y);
            }
            let c = centre(IVec2::new(x, y));
            let at = IVec3::new(c.x, c.y, ground_z);
            let node = s
                .world
                .get_resource::<RouteGraph>()
                .and_then(|r| r.nearest(at).map(|n| (r.pos(n), r.0.nodes[n].cube)));
            let Some((node, cube)) = node else {
                return IVec2::new(x, y);
            };
            // The node end at the height of the node's own cube
            // (`GetNodeCubeNum` @ 100f1450; map 1 has nodes on cube 1).
            let (nx, ny) = (
                ((node.x >> 11) as usize).min(MAP_SIZE - 1),
                ((node.y >> 11) as usize).min(MAP_SIZE - 1),
            );
            let node_z = cube_z(s.world, nx, ny, cube as usize);
            let seen = s.world.get_resource::<City>().is_some_and(|city| {
                crate::sim::collide::line_of_sight(city, node.extend(node_z), at)
            });
            if seen {
                return IVec2::new(x, y);
            }
        }
        tries += 1;
        if tries > 4999 {
            return IVec2::new(x, y);
        }
    }
}

/// Floor z of square (x, y) if it is open ground (see [`safe_pos`]).
fn open_ground(world: &World, x: usize, y: usize) -> Option<i32> {
    let city = world.get_resource::<City>()?;
    let cubes = &city.map.square(x, y).cubes;
    let top = cubes[0].faces[0].kind;
    let ok = cubes[0].slope != Slope::Off
        && !matches!(top, FaceType::Off | FaceType::Reflect)
        && cubes[1].slope == Slope::Off
        && cubes[2].slope == Slope::Off;
    ok.then(|| open_ground_z(world, x, y))
}

fn open_ground_z(world: &World, x: usize, y: usize) -> i32 {
    cube_z(world, x, y, 0)
}

/// The top of cube `cube` (0..2) at the centre of square (x, y)
/// (`GetCubeHeight` @ 100ae7d0).
fn cube_z(world: &World, x: usize, y: usize, cube: usize) -> i32 {
    world.get_resource::<City>().map_or(0x4dc0, |c| {
        z_of_h(c.square_cubes(x, y)[cube.min(2)].top_at(0.5, 0.5)) as i32
    })
}

/// `GetRandomVehicleNum` @ 10057e50 / `_Pursue` @ 100580c0: a random
/// vehicle type 0..24 or 27 (never the RC car or the boat; the pursue
/// variant also never a police car, tank or helicopter). Tanks, helicopters,
/// Pug Racers, RC trucks, rocket cars and the Vapour GT are barred or made
/// rare until enough story levels are done.
fn random_vehicle(s: &mut Script, progress: i32, pursue: bool) -> i32 {
    loop {
        let mut v = s.random(0x1a);
        if v == 0x19 {
            v = 0x1b;
        }
        let barred = if pursue {
            matches!(v, 0x16 | 0x17 | 0xb | 0x11 | 9)
        } else {
            matches!(v, 0x16 | 0x17)
        };
        if barred {
            continue;
        }
        let reject = match progress {
            0 | 1 => matches!(v, 0xb | 0x18 | 0x15 | 0x11 | 0x12 | 0x1b),
            2 => matches!(v, 0xb | 0x18 | 0x15 | 0x12 | 0x1b) || v == 0x11 && s.random(5) < 4,
            3 => matches!(v, 0xb | 0x18 | 0x15 | 0x1b) || v == 0x11 && s.random(5) < 4,
            4 => matches!(v, 0xb | 0x18 | 0x1b) || matches!(v, 0x15 | 0x11) && s.random(5) < 4,
            5 => matches!(v, 0xb | 0x18) || matches!(v, 0x15 | 0x11) && s.random(5) < 4,
            _ => v == 0xb || matches!(v, 0x15 | 0x11 | 0x18) && s.random(5) < 4,
        };
        if !reject {
            return v;
        }
    }
}

/// Phrase of a vehicle's name (`vehicle+0x8198`, 2330..2355).
fn vehicle_name(vehicle: i32) -> u32 {
    TYPES.get(vehicle as usize).map_or(2330, |t| t.name_phrase)
}

impl Random {
    fn location(&self, s: &Script, p: IVec2) -> Fmt {
        Fmt::S(s.global_phrase(LOCATION + point_location(p)))
    }

    /// Speaker 0 (the phone) says a formatted brief.
    fn brief(s: &mut Script, n: u32, args: &[Fmt]) {
        s.dialog_begin(0);
        s.say2_fmt(n, args);
        s.dialog_end();
    }

    /// Speaker 2 (the mobile) says a formatted line.
    fn mobile(s: &mut Script, n: u32, args: &[Fmt]) {
        s.dialog_begin(2);
        s.say2_fmt(n, args);
        s.dialog_end();
    }

    /// The success tail: `KillAllMissionTriggers`, "Here's %d grand"
    /// (random of three), the score in thousands, "Mission complete!",
    /// a new phone. `amount` is drawn before the phrase.
    fn win(&mut self, s: &mut Script, amount: i32) {
        s.kill_all_mt();
        self.paid(s, amount);
        s.add_money(amount * 1000);
        s.show_message(MISSION_COMPLETE);
        self.finish(s);
    }

    fn paid(&mut self, s: &mut Script, amount: i32) {
        let n = PAID + s.random(3) as u32;
        Self::mobile(s, n, &[Fmt::D(amount)]);
    }

    /// The failure tail: an insult (random of three), "Mission failed!",
    /// a new phone. `release` is the job's actor, freed before the message.
    fn fail(&mut self, s: &mut Script, release: &[i32]) {
        s.kill_all_mt();
        let n = FAILED + s.random(3) as u32;
        Self::mobile(s, n, &[]);
        for &id in release {
            s.actor_release(id);
        }
        s.show_message(MISSION_FAILED);
        self.finish(s);
    }

    /// `FinishMission_Random` @ 1012a020: ring another random phone.
    fn finish(&mut self, s: &mut Script) {
        s.add_trigger(100, kind::PHONE, -1, -1, -1, -1);
        s.set_cur_slot(0);
    }

    /// The phone was answered: pick a job (`Random(7) + 1`).
    fn answer(&mut self, s: &mut Script) {
        let slot = s.random(7) + 1;
        s.set_cur_slot(slot);
        let p = self.police;
        match slot {
            1 => self.steal_car(s),
            // 2: kill a gang.
            2 => {
                self.pos = safe_pos(s);
                self.roused = false;
                let loc = point_location(self.pos);
                self.count = s.random(p + 1) + 2;
                self.left = self.count;
                self.mode = if p >= 5 {
                    1
                } else if p < 3 {
                    0
                } else {
                    s.random(2)
                };
                s.dialog_begin(0);
                let place = Fmt::S(s.global_phrase(LOCATION + loc));
                if self.mode == 1 {
                    let r = s.random(20);
                    let e = estimate_time(s, self.pos, None);
                    self.time = if p < 4 {
                        self.count * 8 + ((e + r) * 3 >> 1)
                    } else {
                        self.count * 4 + e + r
                    };
                    s.say2_fmt(1421, &[place, Fmt::D(self.time)]);
                } else {
                    self.time = 0;
                    s.say2_fmt(1420, &[place]);
                }
                s.dialog_end();
                for i in 0..self.count {
                    let angle = s.random(0x1680);
                    let dy = s.random(0x800);
                    let dx = s.random(0x800);
                    let c = self.pos * 0x800;
                    s.create_actor(i + 0xc9, -12, c.x + dx - 0x400, c.y + dy - 0x400, 0, angle, 0);
                    s.t(0xc9, kind::ACTOR_NEAR_PLAYER, i + 0xc9, -1, -1, -1);
                    s.t(0xca, kind::ACTOR_KILLED, i + 0xc9, -1, -1, -1);
                }
                s.t(0xfa, kind::PLAYER_DOWN, -1, -1, -1, -1);
                if self.time != 0 {
                    s.t(0xfc, kind::COUNTDOWN, self.time, -1, -1, -1);
                }
            }
            // 3: a five-checkpoint race against the clock.
            3 => {
                for i in 0..5 {
                    self.points[i] = safe_pos(s);
                }
                let q = self.points;
                let legs = estimate_time2(q[1], q[2], None)
                    + estimate_time2(q[0], q[1], None)
                    + estimate_time2(q[2], q[3], None)
                    + estimate_time2(q[3], q[4], None);
                let r = s.random(0x14);
                self.time = if p < 4 { r + legs } else { (r + legs) * 2 / 3 };
                let place = self.location(s, q[0]);
                Self::brief(s, 1430, &[place]);
                self.checkpoint = 0;
                s.t(0x15e, kind::PLAYER_DOWN, -1, -1, -1, -1);
                let c = centre(q[0]);
                s.t(0x12d, kind::AT_POINT, c.x, c.y, 0, -1);
                s.show_checkpoint_marker(true);
            }
            // 4: kill a number of cops in time.
            4 => {
                self.count = p + s.random(2) + 1;
                self.left = self.count;
                self.time = self.count * 10 + s.random(10) + 0xf;
                Self::brief(s, 1440, &[Fmt::D(self.left), Fmt::D(self.time)]);
                s.set_wanted_level(6);
                for _ in 0..self.left {
                    s.t(0x191, kind::COP_KILLED, -1, -1, -1, -1);
                }
                s.t(0x1c2, kind::PLAYER_DOWN, -1, -1, -1, -1);
                s.t(0x1c4, kind::COUNTDOWN, self.time, -1, -1, -1);
            }
            // 5: destroy the police vehicle that is watching.
            5 => {
                self.vehicle = random_vehicle(s, self.progress, true);
                self.pos = safe_pos(s);
                let r = s.random(10);
                let e = estimate_time(s, self.pos, None);
                self.time = if p < 4 {
                    (e + r + 0x28) * 3 >> 1
                } else {
                    e + r + 0x28
                };
                let place = self.location(s, self.pos);
                let name = Fmt::S(s.global_phrase(vehicle_name(self.vehicle)));
                Self::brief(s, 1450, &[place, name]);
                let colour = s.random(8);
                let angle = s.random(0x1680);
                let c = centre(self.pos);
                s.create_actor(0x1f5, self.vehicle, c.x, c.y, 0, angle, colour);
                s.actor_set_controls(0x1f5, false);
                s.t(0x1f5, kind::ACTOR_KILLED, 0x1f5, -1, -1, -1);
                s.t(0x1f6, kind::NEAR_POINT_12000, c.x, c.y, 0, -1);
                s.t(0x226, kind::PLAYER_DOWN, -1, -1, -1, -1);
            }
            // 6: survive at wanted level 6.
            6 => {
                self.time = s.random(10) + 0x32;
                Self::brief(s, 1460, &[]);
                s.show_message(1461);
                s.set_wanted_level(6);
                s.t(0x28a, kind::PLAYER_DOWN, -1, -1, -1, -1);
                s.t(0x259, kind::COUNTDOWN, self.time, -1, -1, -1);
                s.t(0x262, kind::TIMER, 1, -1, -1, -1);
            }
            // 7: earn points against the clock.
            _ => {
                let a = s.random(0x1e) + 0x14;
                self.time = if p < 4 { a * 2 } else { a * 3 >> 1 };
                Self::brief(s, 1470, &[Fmt::D(a), Fmt::D(self.time)]);
                s.show_message(POINTS_TITLE);
                s.t(0x2bd, kind::MONEY_GAIN, a * 1000, -1, -1, -1);
                s.t(0x2ee, kind::PLAYER_DOWN, -1, -1, -1, -1);
                s.t(0x2ef, kind::COUNTDOWN, self.time, -1, -1, -1);
            }
        }
    }

    /// Job 1: steal a car (id 101) and take it to a drop-off. Mode 0 has
    /// no clock, mode 1 a clock to reach the car, mode 2 one clock for the
    /// whole job.
    fn steal_car(&mut self, s: &mut Script) {
        let p = self.police;
        self.vehicle = random_vehicle(s, self.progress, false);
        self.mode = if p < 3 { s.random(3) } else { s.random(2) + 1 };
        self.pos = safe_pos(s);
        self.dest = safe_pos(s);
        let place = self.location(s, self.pos);
        let name = Fmt::S(s.global_phrase(vehicle_name(self.vehicle) - 0x212));
        s.dialog_begin(0);
        match self.mode {
            1 => {
                let r = s.random(0x14);
                let e = estimate_time(s, self.pos, None);
                self.time = if p < 4 { (e + r) * 3 >> 1 } else { e + r };
                s.say2_fmt(1411, &[name, place, Fmt::D(self.time)]);
            }
            0 => {
                self.time = 0;
                s.say2_fmt(1410, &[name, place]);
            }
            _ => {
                let e2 = estimate_time2(self.pos, self.dest, Some(self.vehicle));
                let e = estimate_time(s, self.pos, None);
                let r = s.random(0x14);
                self.time = if p < 4 { (r + e + e2) * 3 >> 1 } else { r + e + e2 };
                s.say2_fmt(1412, &[name, place, Fmt::D(self.time)]);
            }
        }
        s.dialog_end();
        let colour = s.random(8);
        let angle = s.random(0x1680);
        let c = centre(self.pos);
        s.create_actor(0x65, self.vehicle, c.x, c.y, 0, angle, colour);
        s.t(0x96, kind::PLAYER_DOWN, -1, -1, -1, -1);
        s.t(0x97, kind::ACTOR_GONE, 0x65, -1, -1, -1);
        s.t(0x65, kind::ENTER_ACTOR, 0x65, -1, -1, -1);
        if self.time != 0 {
            s.t(0x98, kind::COUNTDOWN, self.time, -1, -1, -1);
        }
    }

    /// Head for the drop-off: the marker, the area trigger and "get back
    /// in" when the player leaves the car.
    fn to_drop_off(&self, s: &mut Script) {
        let c = centre(self.dest);
        s.t(0x66, kind::AT_POINT, c.x, c.y, 0, -1);
        s.t(0x8c, kind::VEHICLE_CHANGED, -1, -1, -1, -1);
    }

    /// Job 1 events (`ProcessTrigger_Random` slot 1).
    fn steal_event(&mut self, s: &mut Script, ev: i32) {
        let p = self.police;
        match ev {
            // In the car.
            0x65 => {
                s.show_checkpoint_marker(true);
                let place = self.location(s, self.dest);
                s.dialog_begin(2);
                if self.mode == 2 {
                    s.say2_fmt(1414, &[place]);
                } else {
                    s.kill_mt(0x98);
                    if s.random(2) == 0 {
                        let r = s.random(0x14);
                        let e = estimate_time(s, self.dest, Some(self.vehicle));
                        self.time = if p < 4 { (e + r) * 3 >> 1 } else { e + r };
                        s.say2_fmt(1415, &[place, Fmt::D(self.time)]);
                        s.t(0x98, kind::COUNTDOWN, self.time, -1, -1, -1);
                    } else {
                        s.say2_fmt(1414, &[place]);
                    }
                }
                s.dialog_end();
                match p {
                    1 => {
                        if s.random(3) == 0 {
                            let w = s.random(2) + 4;
                            s.set_wanted_level(w);
                        }
                    }
                    2 => {
                        if s.random(2) == 0 {
                            s.set_wanted_level(5);
                        }
                    }
                    3 => s.set_wanted_level(5),
                    4 => {
                        let w = s.random(2) + 5;
                        s.set_wanted_level(w);
                    }
                    5 => s.set_wanted_level(6),
                    _ => {}
                }
                self.to_drop_off(s);
            }
            // Delivered.
            0x66 => {
                s.show_checkpoint_marker(false);
                s.kill_all_mt();
                let amount = s.random(0x14) + 0x32;
                self.paid(s, amount);
                s.actor_release(0x65);
                s.add_money(amount * 1000);
                s.show_message(MISSION_COMPLETE);
                self.finish(s);
            }
            // Out of the car: "Oi! Get back in!"
            0x8c => {
                s.kill_mt(0x66);
                s.show_checkpoint_marker(false);
                Self::mobile(s, 1417, &[]);
                s.t(0x8d, kind::ENTER_ACTOR, 0x65, -1, -1, -1);
            }
            // Back in.
            0x8d => {
                s.show_checkpoint_marker(true);
                Self::mobile(s, 1418, &[]);
                self.to_drop_off(s);
            }
            // Down, car destroyed or out of time.
            0x96..=0x98 => {
                s.show_checkpoint_marker(false);
                self.fail(s, &[0x65]);
            }
            _ => {}
        }
    }

    /// Job 2 events: the gang (ids 201..).
    fn gang_event(&mut self, s: &mut Script, ev: i32) {
        let gang: Vec<i32> = (0..self.count).map(|i| i + 0xc9).collect();
        match ev {
            // The player came near one of them, or killed one: they all
            // attack.
            0xc9 | 0xca => {
                if !self.roused {
                    for &id in &gang {
                        s.actor_attack(id, self.police * 0xf + 0x14, -1);
                    }
                    self.roused = true;
                }
                if ev == 0xc9 {
                    s.kill_mt(0xc9);
                    return;
                }
                self.left -= 1;
                if self.left < 1 {
                    s.kill_all_mt();
                    let amount = self.count * 0x14 + s.random(0x14);
                    self.paid(s, amount);
                    for &id in &gang {
                        s.actor_release(id);
                    }
                    s.add_money(amount * 1000);
                    s.show_message(MISSION_COMPLETE);
                    self.finish(s);
                } else {
                    let n = LEFT + s.random(4) as u32;
                    Self::mobile(s, n, &[Fmt::D(self.left)]);
                }
            }
            0xfa | 0xfc => self.fail(s, &gang),
            _ => {}
        }
    }

    /// Job 3 events: the checkpoint race.
    fn race_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // At the start: "Go!", the clock runs.
            0x12d => {
                Self::mobile(s, 1431, &[]);
                self.checkpoint = 1;
                let c = centre(self.points[1]);
                s.t(0x12e, kind::AT_POINT, c.x, c.y, 0, -1);
                s.t(0x15f, kind::COUNTDOWN, self.time, -1, -1, -1);
            }
            0x12e => {
                self.checkpoint += 1;
                if self.checkpoint < 5 {
                    let c = centre(self.points[self.checkpoint]);
                    s.t(0x12e, kind::AT_POINT, c.x, c.y, 0, -1);
                } else {
                    s.show_checkpoint_marker(false);
                    let amount = s.random(0x1e) + 0x46;
                    self.win(s, amount);
                }
            }
            0x15e | 0x15f => {
                s.show_checkpoint_marker(false);
                self.fail(s, &[]);
            }
            _ => {}
        }
    }

    /// Job 4 events: cop kills.
    fn cops_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x191 => {
                self.left -= 1;
                if self.left < 1 {
                    let amount = self.count * 0x23 + s.random(0x14);
                    self.win(s, amount);
                } else {
                    let n = LEFT + s.random(4) as u32;
                    Self::mobile(s, n, &[Fmt::D(self.left)]);
                }
            }
            0x1c2 | 0x1c4 => self.fail(s, &[]),
            _ => {}
        }
    }

    /// Job 5 events: the watching vehicle (id 501).
    fn watchers_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Close: they drive off.
            0x1f6 => {
                Self::mobile(s, 1451, &[]);
                s.actor_set_aggro(0x1f5, 100);
                s.t(0x228, kind::ACTOR_FLEES, 0x1f5, -1, -1, -1);
                if s.random(2) == 0 {
                    s.t(0x227, kind::COUNTDOWN, self.time, -1, -1, -1);
                }
            }
            // Destroyed.
            0x1f5 => {
                s.kill_all_mt();
                let amount = s.random(0x14) + 0x46;
                self.paid(s, amount);
                s.actor_release(0x1f5);
                s.add_money(amount * 1000);
                s.show_message(MISSION_COMPLETE);
                self.finish(s);
            }
            // Down, out of time, or got away ("You idiot - you let them
            // get away!").
            0x226..=0x228 => {
                s.kill_all_mt();
                if ev == 0x228 {
                    Self::mobile(s, 1452, &[]);
                } else {
                    let n = FAILED + s.random(3) as u32;
                    Self::mobile(s, n, &[]);
                }
                s.actor_release(0x1f5);
                s.show_message(MISSION_FAILED);
                self.finish(s);
            }
            _ => {}
        }
    }

    /// Job 6 events: survival.
    fn survival_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Every second the wanted level is pushed back to 6.
            0x262 => {
                s.set_wanted_level(6);
                s.t(0x262, kind::TIMER, 1, -1, -1, -1);
            }
            0x259 => {
                let amount = self.time;
                s.kill_all_mt();
                self.paid(s, amount);
                s.add_money(amount * 1000);
                s.show_message(MISSION_COMPLETE);
                self.finish(s);
            }
            0x28a => self.fail(s, &[]),
            _ => {}
        }
    }

    /// Job 7 events: points.
    fn points_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x2bd => {
                let amount = s.random(0x14) + 0x1e;
                self.win(s, amount);
            }
            0x2ee | 0x2ef => self.fail(s, &[]),
            _ => {}
        }
    }
}

impl LevelScript for Random {
    /// `InitScript_Random` @ 10129e50.
    fn init(&mut self, s: &mut Script) {
        s.set_cur_slot(0);
        // The goal is set even when it is 0 (no table yet); the trigger is
        // added only for a positive goal (`SetTrigger` type 0).
        s.add_trigger(-1, kind::MONEY_GOAL, self.goal, -1, -1, -1);
        s.add_trigger(100, kind::PHONE, -1, -1, -1, -1);
        s.show_message(ANSWER_THE_PHONE);
        let name = Fmt::S(self.city_name.clone());
        Self::mobile(s, WELCOME, &[name]);
    }

    /// `ProcessTrigger_Random` @ 1012a0a0: dispatch on the job in progress
    /// (0 = waiting for the phone).
    fn event(&mut self, s: &mut Script, ev: i32) {
        match s.cur_slot() {
            0 => {
                if ev == 100 {
                    self.answer(s);
                }
            }
            1 => self.steal_event(s, ev),
            2 => self.gang_event(s, ev),
            3 => self.race_event(s, ev),
            4 => self.cops_event(s, ev),
            5 => self.watchers_event(s, ev),
            6 => self.survival_event(s, ev),
            7 => self.points_event(s, ev),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations() {
        let at = |x, y| point_location(IVec2::new(x, y));
        assert_eq!(at(10, 10), 7);
        assert_eq!(at(64, 10), 0);
        assert_eq!(at(100, 10), 1);
        assert_eq!(at(100, 64), 2);
        assert_eq!(at(100, 100), 3);
        assert_eq!(at(64, 100), 4);
        assert_eq!(at(10, 100), 5);
        assert_eq!(at(10, 64), 6);
        assert_eq!(at(64, 64), 8);
        assert_eq!(at(42, 85), 6);
        assert_eq!(at(43, 86), 4);
    }

    #[test]
    fn estimates() {
        assert_eq!(estimate_time2(IVec2::new(0, 0), IVec2::new(10, 4), None), 7);
        // Mundaneo: 280 · 5 · 2 = 2800.
        assert_eq!(
            estimate_time2(IVec2::new(0, 0), IVec2::new(10, 4), Some(0)),
            14 * 0x800 / 2800
        );
    }
}
