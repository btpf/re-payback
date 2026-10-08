//! Level 01 (city 0, level 1, "Gangster Tripping") mission script
//! (docs/re/missions_01.md): init 68k `0xb9324`, event handler `0xbed0c`,
//! unlock routine `0xb5198`, shutdown extras in `Mission_Shutdown` `0xca322`.
//!
//! Nine phone missions. The first group {M0, M1, M2, M6} unlocks the second
//! group {M3, M4, M5, M7}; M8 sits on a hidden phone from the start. Every
//! mission ends (success or failure) through [`unlock`], so none can be
//! retried. The level is won by the 2,000,000 money goal.

use crate::mission::api::{MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::kind::*;
use crate::mission::{LevelScript, Script};

/// Dialogue speakers (`Dialog_Begin` icon): briefing on the phone, the
/// booby-trapped car's security system, the boss on the mobile.
const PHONE: i32 = 0;
const LIPS: i32 = 1;
const BOSS: i32 = 2;

/// Booby-trapped diblo at (117,88): getting in arms a 10 s countdown.
const TRAP_CAR: i32 = 0x835;
const TRAP_ENTERED: i32 = 0x834;
const TRAP_INFO: i32 = 0x836;
/// Secret Info pickup placed by the map at (37,0) cube 1.
const SECRET_INFO: i32 = 0x898;

/// Mission 0: the killer's escape route, one leg per arrival event 4..=9
/// (the last leg, event 10 → 11, makes him vanish).
const M0_ROUTE: [(i32, i32); 6] = [
    (0x2400, 0xf400),
    (0x3c00, 0xf400),
    (0x3c00, 0x103c4),
    (0x9400, 0x103c4),
    (0x9400, 0x11c00),
    (0xa410, 0x11c00),
];

/// Mission 2: Quik-e-mart window cells (cube 1), events `0xd2 + i`. The
/// first seven are west faces of x = 0x5a, the rest south faces of y = 0x5c.
const M2_WINDOWS: [(i32, i32); 11] = [
    (0x5a, 0x53),
    (0x5a, 0x54),
    (0x5a, 0x55),
    (0x5a, 0x57),
    (0x5a, 0x58),
    (0x5a, 0x5a),
    (0x5a, 0x5b),
    (0x5b, 0x5c),
    (0x5c, 0x5c),
    (0x5d, 0x5c),
    (0x5e, 0x5c),
];
const M2_WEST_WINDOWS: usize = 7;

/// Mission 3: the three cars to dump in the sea.
const M3_CARS: i32 = 0x12d;
const M3_GET_IN: i32 = 0x137;
const M3_IN_SEA: i32 = 0x141;
const M3_WRECKED: i32 = 0x14b;
const M3_LEFT_CAR: i32 = 0x168;

/// Mission 4: the courier and his legs; `(arrival event, next event, x, y)`.
const M4_COURIER: i32 = 0x191;
const M4_ROUTE: [(i32, i32, i32, i32); 4] = [
    (0x193, 0x194, 0x2ac00, 0xcc00),
    (0x194, 0x195, 0x29c00, 0xa400),
    (0x196, 0x197, 0x28c00, 0x6400),
    (0x197, 0x198, 0x2dc00, 0x5c00),
];

/// Mission 5: the pickup's endless patrol; arrival event `0x1f5 + i` arms
/// `M5_PATROL[i]` = (next event, x, y).
const M5_PICKUP: i32 = 0x1f5;
const M5_DRIVER: i32 = 0x1f6;
const M5_PATROL: [(i32, i32, i32); 12] = [
    (0x1f6, 0x2bc00, 0x17400),
    (0x1f7, 0x21400, 0x17400),
    (0x1f8, 0x16c00, 0x17400),
    (0x1f9, 0x16c00, 0x1ac00),
    (0x1fa, 0x1ac00, 0x1ac00),
    (0x1fb, 0x1ac00, 0x23c00),
    (0x1fc, 0x15400, 0x23c00),
    (0x1fd, 0x15400, 0x28400),
    (0x1fe, 0x1dc00, 0x28400),
    (0x1ff, 0x26c00, 0x28400),
    (0x200, 0x26c00, 0x2f400),
    (0x1f5, 0x2bc00, 0x2f400),
];

/// Mission 6: the ice-cream van.
const M6_VAN: i32 = 0x259;

/// Mission 7: the four gangsters `0x2bd + i`: (x, y, angle).
const M7_GANG: i32 = 0x2bd;
const M7_SPAWNS: [(i32, i32, i32); 4] = [
    (0x2fc00, 0x28c00, 0x5a0),
    (0x33c00, 0x2ac00, 0x5a0),
    (0x34400, 0x2ac00, 0x10e0),
    (0x38c00, 0x28c00, 0x10e0),
];

/// Mission 8: the bus, the prisoner currently boarding/unloading, and the
/// safe house he walks into.
const M8_BUS: i32 = 0x321;
const M8_PRISONER: i32 = 0x322;
const M8_HOUSE: (i32, i32) = (0x1cad4, 0x7c00);

/// The level globals (68k addresses in missions_01.md).
#[derive(Default)]
pub struct Level01 {
    /// Per-mission flag / counter "A" (`0x2fba1c`).
    a: i32,
    /// Counter "B" (`0x2fba08`): cars / gangsters resolved.
    b: i32,
    /// Counter "C" (`0x2fb148`): cars dumped / gang alerted.
    c: i32,
    /// Per-actor "still active" flags (`0x2fba30[4]`, missions 3 and 7).
    active: [bool; 4],
    /// Mission 8: prisoners still alive (`0x2fba48`).
    alive: i32,
    /// Mission 8 phase (`0x2fba4c`): 0 none, 1 boarding, 2 unloading.
    phase: i32,
}

impl LevelScript for Level01 {
    /// `Init01` (68k `0xb9324`).
    fn init(&mut self, s: &mut Script) {
        s.set_intro_phrase(2);
        s.set_phone(0, 1, 8, false);
        s.set_phone(1, 100, 9, false);
        s.set_phone(2, 200, 18, false);
        s.set_phone(6, 600, 20, false);
        s.set_phone(8, 800, 16, true);
        s.add_trigger(-1, MONEY_GOAL, 2_000_000, 0, 0, 0);
        s.add_lives(2);
        // Four traffic cones (special object 1).
        for (id, x, y) in [
            (100_000, 0xb400, 0x2e000),
            (100_001, 0xb400, 0x2f800),
            (100_002, 0x15400, 0x2e000),
            (100_003, 0x15400, 0x2f800),
        ] {
            s.create_actor(id, 1001, x, y, 0, 0, 0);
        }
        // Squares x 26..30, y 61..70: cube level 1 flattened. The original
        // uses counter B as the loop variable; every mission resets it.
        for sx in 0x1a..=0x1e {
            for sy in 0x3d..=0x46 {
                s.set_cube(sx, sy, 1, 0x200, 0x200, 0, 0, 0);
            }
        }
        s.refresh_phones();
        s.show_message(14);
        s.create_actor(TRAP_CAR, 2, 0x3ac00, 0x2c400, 0, 0, 6);
        s.add_trigger(TRAP_ENTERED, ENTER_ACTOR_QUIET, TRAP_CAR, 0, 0, 0);
        s.set_scenery(0x75, 0x57, 0, 24, 0, 0);
        s.add_trigger(TRAP_INFO, SCENERY_GONE_QUIET, 0x75, 0x57, 0, 0);
        s.add_trigger(SECRET_INFO, SCENERY_GONE_QUIET, 0x25, 0, 1, 0);
    }

    /// `Handler01` (68k `0xbed0c`): global events first, then the running
    /// mission (= its phone slot), else a phone being answered.
    fn event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0 => return,
            TRAP_CAR => {
                s.actor_kill(TRAP_CAR);
                return;
            }
            TRAP_ENTERED => {
                s.say(LIPS, 42);
                // Tracked: any mission ending with KillAllMissionTriggers
                // while it runs defuses the car (original behaviour).
                s.t(TRAP_CAR, COUNTDOWN, 10, 0, 0, 0);
                return;
            }
            TRAP_INFO => {
                s.hint(51);
                return;
            }
            SECRET_INFO => {
                s.hint(400);
                return;
            }
            _ => {}
        }
        match s.cur_slot() {
            0 => self.m0_harry(s, ev),
            1 => self.m1_cops(s, ev),
            2 => self.m2_ram_raid(s, ev),
            3 => self.m3_dump_cars(s, ev),
            4 => self.m4_briefcase(s, ev),
            5 => self.m5_pickup(s, ev),
            6 => self.m6_ice_cream_van(s, ev),
            7 => self.m7_gang(s, ev),
            8 => self.m8_prison_bus(s, ev),
            _ => self.answer_phone(s, ev),
        }
    }

    /// Level-01 part of `Mission_Shutdown` (68k `0xca322`).
    fn shutdown(&mut self, s: &mut Script) {
        s.actor_release(TRAP_CAR);
        s.kill_mt(TRAP_ENTERED);
        s.kill_mt(TRAP_CAR);
    }
}

impl Level01 {
    /// Phone events, handled only while no mission runs: brief and start.
    fn answer_phone(&mut self, s: &mut Script, ev: i32) {
        match ev {
            1 => {
                brief(s, 0);
                s.t(2, NEAR_POINT_10000, 7000, 0x5c00, 0, 0);
                s.t(3, TIMER, 100, 0, 0, 0);
                s.t(0x32, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(0);
                self.a = 0;
            }
            100 => {
                brief(s, 6);
                s.set_wanted_level(5);
                // Four independent triggers on the same event, one per cop.
                for _ in 0..4 {
                    s.t(0x65, COP_KILLED, 0, 0, 0, 0);
                }
                s.t(0x8c, COUNTDOWN, 180, 0, 0, 0);
                s.t(0x96, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(1);
                self.a = 0;
            }
            200 => {
                brief(s, 15);
                for &(sx, sy) in &M2_WINDOWS[..M2_WEST_WINDOWS] {
                    s.set_face(4, sx, sy, 1, 3, 0x1da, 0);
                }
                for &(sx, sy) in &M2_WINDOWS[M2_WEST_WINDOWS..] {
                    s.set_face(3, sx, sy, 1, 3, 0x1db, 0);
                }
                // The loot point does not require breaking a window first.
                s.t(0xc9, AT_POINT, 0x2e400, 0x2b400, 0, 0);
                for (ev, &(sx, sy)) in (0xd2..).zip(&M2_WINDOWS) {
                    s.t(ev, CUBE_SMASHED, sx, sy, 1, 0);
                }
                s.t(0xfa, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(2);
                self.a = 0;
            }
            300 => {
                brief(s, 21);
                s.create_actor(M3_CARS, 12, 0x3dc00, 0x3400, 0, 0xa0, 1);
                s.create_actor(M3_CARS + 1, 10, 0x3e418, 0x3400, 0, 0, 4);
                s.create_actor(M3_CARS + 2, 4, 0x3ebe8, 0x3400, 0, 0x15e0, 0);
                self.active[..3].fill(true);
                for i in 0..3 {
                    s.t(M3_GET_IN + i, ENTER_ACTOR, M3_CARS + i, 0, 0, 0);
                    s.t(M3_IN_SEA + i, ACTOR_IN_WATER, M3_CARS + i, 0, 0, 0);
                    s.t(M3_WRECKED + i, ACTOR_GONE, M3_CARS + i, 0, 0, 0);
                }
                s.t(0x154, COUNTDOWN, 200, 0, 0, 0);
                // No death trigger in this mission (original).
                s.set_cur_slot(3);
                self.c = 0;
                self.b = 0;
                self.a = 0;
            }
            400 => {
                s.kill_all_phones();
                s.dialog_begin(PHONE);
                s.say2(31);
                s.say1(32);
                s.say2(33);
                s.dialog_end();
                s.set_scenery(0x28, 0x3f, 0, 23, 0x59, 0);
                s.t(0x191, ON_FOOT_AT_POINT, 0x14400, 0x1fc00, 0, 0);
                s.t(0x1c2, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(4);
            }
            500 => {
                brief(s, 43);
                s.create_actor(M5_PICKUP, 15, 0x2bc00, 0x2f400, 0, 0xb40, 4);
                s.t(0x1f5, ACTOR_GOTO, M5_PICKUP, 0x2bc00, 0x23280, 0);
                s.t(0x21c, ACTOR_ATTACKED, M5_PICKUP, 0, 0, 0);
                s.t(0x226, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(5);
            }
            600 => {
                brief(s, 46);
                s.create_actor(M6_VAN, 14, 0x1f400, 0x9400, 0, 0xb40, 7);
                s.t(0x259, ENTER_ACTOR, M6_VAN, 0, 0, 0);
                s.t(0x28a, ACTOR_GONE, M6_VAN, 0, 0, 0);
                s.t(0x28b, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_scenery(0x3f, 0x12, 0, 24, 0, 0);
                s.t(0x2a8, SCENERY_GONE_QUIET, 0x3f, 0x12, 0, 0);
                s.set_cur_slot(6);
            }
            700 => {
                brief(s, 52);
                s.set_scenery(0x5f, 0x5d, 0, 24, 0, 0);
                s.t(0x30c, SCENERY_GONE_QUIET, 0x5f, 0x5d, 0, 0);
                for (id, &(x, y, angle)) in (M7_GANG..).zip(&M7_SPAWNS) {
                    s.create_actor(id, -12, x, y, 0, angle, 0);
                }
                for i in 0..4 {
                    s.t(M7_GANG + i, ACTOR_SEES_PLAYER_QUIET, M7_GANG + i, 0, 0, 0);
                }
                for i in 0..4 {
                    s.t(0x2c7 + i, ACTOR_ATTACKED_QUIET, M7_GANG + i, 0, 0, 0);
                }
                s.t(0x2d1, ACTOR_GONE, M7_GANG, 0, 0, 0);
                s.t(0x2d2, ACTOR_KILLED, M7_GANG + 1, 0, 0, 0);
                s.t(0x2d3, ACTOR_KILLED, M7_GANG + 2, 0, 0, 0);
                s.t(0x2d4, ACTOR_GONE, M7_GANG + 3, 0, 0, 0);
                s.t(0x2ee, PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(7);
                self.c = 0;
                self.b = 0;
                self.active = [true; 4];
            }
            800 => {
                brief(s, 69);
                s.create_actor(M8_BUS, 3, 0x28400, 0x1dc00, 0, 0, 0);
                s.t(0x321, ENTER_ACTOR, M8_BUS, 0, 0, 0);
                s.t(0x352, PLAYER_DOWN, 0, 0, 0, 0);
                s.t(0x353, ACTOR_GONE, M8_BUS, 0, 0, 0);
                s.set_cur_slot(8);
            }
            _ => {}
        }
    }

    /// Mission 0 "Harry" (68k ≈`0xc0f1c`, state block ≈`0xbee40`): avenge
    /// Harry, whose garage blows up; kill the killer before he escapes.
    fn m0_harry(&mut self, s: &mut Script, ev: i32) {
        match ev {
            2 | 3 => {
                if self.a == 0 {
                    s.kill_mt(if ev == 2 { 3 } else { 2 });
                    self.a = 1;
                    garage_explodes(s);
                }
            }
            4..=9 => {
                let (x, y) = M0_ROUTE[(ev - 4) as usize];
                s.t(ev + 1, ACTOR_GOTO, 1, x, y, 0);
            }
            10 => s.t(11, ACTOR_GOTO_VANISH, 1, 0xa410, 0x12400, 0),
            11 | 0x1e | 0x32 => {
                s.kill_all_mt();
                s.actor_release(1);
                match ev {
                    11 => {
                        s.say(BOSS, 5);
                        fail(s);
                    }
                    0x1e => {
                        s.say(BOSS, 4);
                        win(s, 100_000);
                    }
                    _ => {
                        s.say(BOSS, 1);
                        fail(s);
                    }
                }
            }
            _ => {}
        }
    }

    /// Mission 1 "Kill 4 cops in 3 minutes" (68k ≈`0xc0fa0`, state block
    /// ≈`0xbf040`).
    fn m1_cops(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x65 => {
                s.say(BOSS, 7 + self.a as u32);
                self.a += 1;
                if self.a == 4 {
                    s.kill_all_mt();
                    win(s, 200_000);
                }
            }
            0x8c | 0x96 => {
                s.kill_all_mt();
                s.say(BOSS, if ev == 0x8c { 12 } else { 13 });
                fail(s);
            }
            _ => {}
        }
    }

    /// Mission 2 "Ram-raid the Quik-e-mart" (68k ≈`0xc108e`, state block
    /// ≈`0xbf130`): smash a window, grab the loot, deliver it to Jon's
    /// garage within 60 s. `a` = alarm ringing.
    fn m2_ram_raid(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0xd2..=0xdc => {
                if self.a != 0 {
                    return;
                }
                self.a = 1;
                for e in 0xd2..=0xdc {
                    s.kill_mt(e);
                }
                s.say(BOSS, 16);
                s.set_wanted_level(5);
                // Sample 0x30 = "Alarm"; the original's range 20000 has no
                // equivalent in `add_sound`.
                s.add_sound(0, 0x30, true, 100_000_000.0, 16000.0, 0x2e400, 0x2b400);
                alarm_boxes(s, 22);
            }
            0xc9 => {
                s.say(BOSS, 17);
                s.t(0xca, GARAGE, 0x0c, 0x74, 1, 3);
                s.t(0xcb, COUNTDOWN, 60, 0, 0, 0);
            }
            0xca | 0xcb | 0xfa => {
                s.kill_all_mt();
                if self.a == 1 {
                    s.delete_sound(0);
                    alarm_boxes(s, 21);
                }
                // The windows stay glass (no restore in the original).
                match ev {
                    0xca => {
                        s.say(BOSS, 19);
                        win(s, 150_000);
                    }
                    0xcb => {
                        s.say(BOSS, 20);
                        fail(s);
                    }
                    _ => {
                        s.say(BOSS, 18);
                        fail(s);
                    }
                }
            }
            _ => {}
        }
    }

    /// Mission 3 "Dump the cars in the sea" (68k ≈`0xc1364`, state block
    /// ≈`0xbfdd0`): `a` = player in a mission car, `b` = cars resolved,
    /// `c` = cars dumped (75,000 each).
    fn m3_dump_cars(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x137..=0x139 => {
                if self.a != 0 {
                    return;
                }
                for e in [0x137, 0x138, 0x139, M3_LEFT_CAR] {
                    s.kill_mt(e);
                }
                self.a = 1;
                s.say(BOSS, 22);
                s.t(M3_LEFT_CAR, VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            M3_LEFT_CAR => {
                self.m3_rearm_get_in(s);
                self.a = 0;
            }
            0x141..=0x143 => {
                s.kill_mt(M3_LEFT_CAR);
                let i = (ev - M3_IN_SEA) as usize;
                if !self.active[i] {
                    return;
                }
                self.b += 1;
                self.c += 1;
                self.active[i] = false;
                if self.b < 3 {
                    // Quirk (68k, OS4 and CD): `a` stays 1 here, so getting
                    // into the next car is ignored (no phrase 22, no 0x168).
                    self.m3_rearm_get_in(s);
                    s.say(BOSS, 23);
                    return;
                }
                m3_finish(s, self.c);
            }
            0x14b..=0x14d => {
                s.kill_mt(M3_LEFT_CAR);
                let i = (ev - M3_WRECKED) as usize;
                if !self.active[i] {
                    return;
                }
                self.b += 1;
                self.active[i] = false;
                if self.b < 3 {
                    self.m3_rearm_get_in(s);
                    s.say(BOSS, 25);
                    return;
                }
                if self.c > 0 {
                    m3_finish(s, self.c);
                } else {
                    s.kill_all_mt();
                    m3_release_cars(s);
                    s.say(BOSS, 26);
                    fail(s);
                }
            }
            0x154 => {
                s.kill_all_mt();
                m3_release_cars(s);
                s.say(BOSS, 28);
                // Partial pay for the cars dumped, but still a failure.
                if self.c > 0 {
                    s.add_money(self.c * 75_000);
                }
                fail(s);
            }
            _ => {}
        }
    }

    /// Re-arm "get into car i" for every car still in play.
    fn m3_rearm_get_in(&self, s: &mut Script) {
        for i in 0..3 {
            if self.active[i as usize] {
                s.t(M3_GET_IN + i, ENTER_ACTOR, M3_CARS + i, 0, 0, 0);
            }
        }
    }

    /// Mission 4 "The briefcase" (68k ≈`0xc1538`, state block ≈`0xc02a0`):
    /// carry it to the park, then tail the courier unseen.
    fn m4_briefcase(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x191 => {
                s.set_scenery(0x28, 0x3f, 0, 0, 0x59, 0);
                s.say(BOSS, 35);
                s.t(0x192, ON_FOOT_AT_POINT, 0x29df4, 0xa400, 0, 0);
                s.show_checkpoint_marker(true);
            }
            0x192 => {
                s.set_scenery(0x53, 0x14, 0, 23, 0xb2, 0);
                s.show_checkpoint_marker(false);
                s.say(BOSS, 36);
                s.create_actor(M4_COURIER, -10, 0x2ec00, 0xdc00, 0, 0, 0);
                // Stays armed while tailing him too: being seen then fails.
                s.t(0x1c3, ACTOR_SEES_PLAYER, M4_COURIER, 0, 0, 0);
                s.t(0x1c4, ACTOR_GONE, M4_COURIER, 0, 0, 0);
                s.t(0x193, ACTOR_GOTO, M4_COURIER, 0x2ec00, 0xcc00, 0);
            }
            0x195 => {
                s.set_scenery(0x53, 0x14, 0, 0, 0xb2, 0);
                s.say(BOSS, 39);
                s.t(0x196, ACTOR_GOTO, M4_COURIER, 0x28c00, 0x8c00, 0);
                s.t(0x1c5, ACTOR_LOST, M4_COURIER, 0, 0, 0);
            }
            0x198 => s.t(0x199, ACTOR_GOTO_VANISH, M4_COURIER, 0x2dc00, 0x445c, 0),
            0x199 => {
                s.actor_release(M4_COURIER);
                s.kill_all_mt();
                s.say(BOSS, 41);
                win(s, 200_000);
            }
            0x1c2 => {
                s.show_checkpoint_marker(false);
                s.kill_all_mt();
                s.set_scenery(0x28, 0x3f, 0, 0, 0x59, 0);
                s.set_scenery(0x53, 0x14, 0, 0, 0x59, 0);
                s.actor_release(M4_COURIER);
                s.say(BOSS, 34);
                fail(s);
            }
            0x1c3..=0x1c5 => {
                s.kill_all_mt();
                s.set_scenery(0x53, 0x14, 0, 0, 0x59, 0);
                s.actor_release(M4_COURIER);
                s.say(BOSS, [37, 38, 40][(ev - 0x1c3) as usize]);
                fail(s);
            }
            _ => {
                if let Some(&(_, next, x, y)) = M4_ROUTE.iter().find(|r| r.0 == ev) {
                    s.t(next, ACTOR_GOTO, M4_COURIER, x, y, 0);
                }
            }
        }
    }

    /// Mission 5 "Wipe out the pickup driver" (68k ≈`0xc15d0`, state block
    /// ≈`0xc06a0`): the pickup patrols until attacked, then the driver
    /// bails out and fights. No time limit.
    fn m5_pickup(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x1f5..=0x200 => {
                let (next, x, y) = M5_PATROL[(ev - 0x1f5) as usize];
                s.t(next, ACTOR_GOTO, M5_PICKUP, x, y, 0);
            }
            0x21c => {
                s.kill_all_mt();
                s.ped_exits_vehicle(M5_DRIVER, M5_PICKUP, -12, -1);
                s.actor_release(M5_PICKUP);
                s.actor_attack(M5_DRIVER, 0x50, -1);
                s.t(0x21d, ACTOR_GONE, M5_DRIVER, 0, 0, 0);
                s.t(0x226, PLAYER_DOWN, 0, 0, 0, 0);
            }
            0x21d => {
                s.kill_all_mt();
                s.actor_release(M5_DRIVER);
                s.say(BOSS, 45);
                win(s, 300_000);
            }
            0x226 => {
                s.kill_all_mt();
                s.actor_release(M5_PICKUP);
                s.actor_release(M5_DRIVER);
                s.say(BOSS, 44);
                fail(s);
            }
            _ => {}
        }
    }

    /// Mission 6 "Booby-trapped ice-cream van" (68k ≈`0xc167c`, state block
    /// ≈`0xc09f0`): reach Ray's garage within 55 s without leaving the van.
    fn m6_ice_cream_van(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x2a8 => s.hint(59),
            0x259 => {
                s.say(BOSS, 47);
                s.t(0x25a, GARAGE, 0x6d, 0x25, 1, 2);
                s.t(0x280, COUNTDOWN, 55, 0, 0, 0);
                s.t(0x281, VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            0x25a | 0x280 | 0x281 | 0x28a | 0x28b => {
                s.kill_all_mt();
                // The van blows up (with the player in it on time-out).
                let (blow_up, phrase) = match ev {
                    0x25a => (false, 49),
                    0x280 => (true, 48),
                    0x281 => (true, 50),
                    0x28a => (false, 29),
                    _ => (false, 30),
                };
                if blow_up {
                    s.actor_kill(M6_VAN);
                } else {
                    s.actor_release(M6_VAN);
                }
                s.say(BOSS, phrase);
                s.set_scenery(0x3f, 0x12, 0, 0, 0, 0);
                if ev == 0x25a {
                    win(s, 200_000);
                } else {
                    fail(s);
                }
            }
            _ => {}
        }
    }

    /// Mission 7 "Wasteland gang deal" (68k ≈`0xc1804`, state block
    /// ≈`0xc0b90`): `b` = gangsters killed, `c` = gang alerted.
    fn m7_gang(&mut self, s: &mut Script, ev: i32) {
        match ev {
            M7_GANG..=0x2d4 => {
                // Part 1: a gangster was killed.
                if ev >= 0x2d1 {
                    self.m7_killed(s, (ev - 0x2d1) as usize);
                }
                // Part 2: any sighting, attack or kill alerts the gang.
                if self.c == 0 {
                    self.c = 1;
                    for j in self.m7_active() {
                        s.actor_attack(M7_GANG + j, 0x46, -1);
                    }
                }
            }
            0x2da | 0x2ee => {
                s.kill_all_mt();
                for j in self.m7_active() {
                    s.actor_release(M7_GANG + j);
                }
                s.set_scenery(0x5f, 0x5d, 0, 0, 0, 0);
                s.say(BOSS, if ev == 0x2da { 56 } else { 57 });
                // Quirk (68k, OS4 and CD): these failure endings show
                // "Mission complete!", with its sound and multiplier bonus.
                s.show_message(MISSION_COMPLETE);
                unlock(s);
            }
            0x30c => s.hint(58),
            _ => {}
        }
    }

    /// Gangster `i` killed: 50k each, 100k and success for the fourth; when
    /// three are down the last one flees.
    fn m7_killed(&mut self, s: &mut Script, i: usize) {
        if !self.active[i] {
            return;
        }
        self.active[i] = false;
        self.b += 1;
        s.actor_release(M7_GANG + i as i32);
        s.dialog_begin(BOSS);
        if self.b < 4 {
            s.add_money(50_000);
            s.say2(if self.b < 3 { 53 } else { 54 });
            s.dialog_end();
        } else {
            s.kill_all_mt();
            s.set_scenery(0x5f, 0x5d, 0, 0, 0, 0);
            s.add_money(100_000);
            s.say2(55);
            s.dialog_end();
            s.show_message(MISSION_COMPLETE);
            unlock(s);
        }
        if self.b == 3 {
            for j in self.m7_active() {
                s.t(0x2da, ACTOR_LOST, M7_GANG + j, 0, 0, 0);
                s.actor_attack(M7_GANG + j, -1, -1);
            }
        }
    }

    /// Indices of the gangsters still in play.
    fn m7_active(&self) -> impl Iterator<Item = i32> + use<> {
        let active = self.active;
        (0..4).filter(move |&j| active[j as usize])
    }

    /// Mission 8 "Prison break bus" (68k ≈`0xc1760`, state block
    /// ≈`0xbf680`): drive the bus to the prison, pick up five prisoners one
    /// at a time, unload them at the safe house; 30,000 per survivor.
    fn m8_prison_bus(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x321 => {
                s.say(BOSS, 72);
                // Squares (50..53, 104..105): the prison wall is breached.
                for sx in 0x32..=0x35 {
                    for sy in 0x68..=0x69 {
                        s.set_cube(sx, sy, 1, 0x200, 0x200, 0, 0, 0);
                    }
                }
                s.t(0x322, COUNTDOWN, 40, 0, 0, 0);
                s.t(0x323, NEAR_POINT_10000, 0x19000, 0x34800, 0, 0);
                s.t(0x325, VEHICLE_CHANGED, 0, 0, 0, 0);
                self.a = 5;
                self.alive = 5;
                self.phase = 0;
            }
            0x322 => {
                s.actor_release(M8_BUS);
                s.kill_all_mt();
                s.say(BOSS, 73);
                fail(s);
            }
            0x323 => {
                s.kill_mt(0x322);
                s.say(BOSS, 74);
                m8_spawn_prisoner(s);
                self.phase = 1;
            }
            0x324 => {
                s.kill_mt(0x328);
                self.m8_next_boarding(s);
            }
            0x32a => {
                s.kill_mt(0x329);
                self.m8_next_unloading(s);
            }
            0x328 | 0x329 => {
                s.actor_release(M8_PRISONER);
                self.alive -= 1;
                if self.alive > 0 {
                    s.kill_mt(if ev == 0x328 { 0x324 } else { 0x32a });
                    s.say(BOSS, 78);
                    if ev == 0x328 {
                        self.m8_next_boarding(s);
                    } else {
                        self.m8_next_unloading(s);
                    }
                } else {
                    s.actor_release(M8_BUS);
                    s.kill_all_mt();
                    s.say(BOSS, 80);
                    fail(s);
                }
            }
            0x327 => {
                if self.phase == 2 {
                    return;
                }
                s.say(BOSS, 79);
                self.a = self.alive;
                m8_unload_prisoner(s);
                self.phase = 2;
            }
            0x325 => {
                match self.phase {
                    1 => {
                        s.kill_mt(0x324);
                        s.kill_mt(0x328);
                    }
                    2 => {
                        s.kill_mt(0x32a);
                        s.kill_mt(0x329);
                    }
                    // Phase 0: the safe-house trigger 0x327 stays armed.
                    _ => {}
                }
                s.t(0x326, ENTER_ACTOR, M8_BUS, 0, 0, 0);
                s.say(BOSS, 75);
            }
            0x326 => {
                match self.phase {
                    1 => {
                        s.kill_mt(0x324);
                        s.t(0x328, ACTOR_GONE, M8_PRISONER, 0, 0, 0);
                        s.t(0x324, ACTOR_BOARDS_PLAYER, M8_PRISONER, 0, 0, 0);
                    }
                    2 => {
                        s.kill_mt(0x32a);
                        m8_prisoner_triggers(s);
                    }
                    _ => {}
                }
                s.t(0x325, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.say(BOSS, 76);
            }
            0x352 | 0x353 => {
                s.actor_release(M8_BUS);
                s.actor_release(M8_PRISONER);
                s.kill_all_mt();
                s.say(BOSS, if ev == 0x352 { 70 } else { 71 });
                fail(s);
            }
            _ => {}
        }
    }

    /// A prisoner got on (or died): next one, or off to the safe house.
    fn m8_next_boarding(&mut self, s: &mut Script) {
        s.set_wanted_level(5);
        self.a -= 1;
        if self.a < 1 {
            s.t(0x327, AT_POINT, 0x1c000, 0x8000, 0, 0);
            s.say(BOSS, 77);
            self.phase = 0;
        } else {
            m8_spawn_prisoner(s);
        }
    }

    /// A prisoner is inside (or died): next one, or pay per survivor.
    fn m8_next_unloading(&mut self, s: &mut Script) {
        self.a -= 1;
        if self.a < 1 {
            s.actor_release(M8_BUS);
            s.kill_all_mt();
            // 81 (5 alive) .. 85 (1 alive).
            s.say(BOSS, (0x56 - self.alive) as u32);
            s.add_money(self.alive * 30_000);
            s.show_message(MISSION_COMPLETE);
            unlock(s);
            // After the unlock in the original too.
            s.set_wanted_level(3);
        } else {
            m8_unload_prisoner(s);
        }
    }
}

/// Briefing on the phone: other phones stop ringing, the boss speaks.
fn brief(s: &mut Script, phrase: u32) {
    s.kill_all_phones();
    s.dialog_begin(PHONE);
    s.say2(phrase);
    s.dialog_end();
}

/// `SUCCESS(money)`: pay (× multiplier), "Mission complete!", unlock.
fn win(s: &mut Script, money: i32) {
    s.add_money(money);
    s.show_message(MISSION_COMPLETE);
    unlock(s);
}

/// `FAIL`: "Mission failed!", unlock.
fn fail(s: &mut Script) {
    s.show_message(MISSION_FAILED);
    unlock(s);
}

/// `Unlock01` (68k `0xb5198`): clear the finished slot; once slots 0, 1, 2
/// and 6 are all done, open the second group (slots 3, 4, 5, 7).
fn unlock(s: &mut Script) {
    let slot = s.cur_slot();
    s.consume_slot();
    if matches!(slot, 0 | 1 | 2 | 6) && [0, 1, 2, 6].iter().all(|&i| s.phone_event(i) == -1) {
        s.set_phone(3, 300, 6, false);
        s.set_phone(4, 400, 7, false);
        s.set_phone(5, 500, 19, false);
        s.set_phone(7, 700, 21, false);
    }
    s.refresh_phones();
    s.set_cur_slot(-1);
}

/// Mission 0 helper "garage explodes" (68k `0xb6c46`): explosions over
/// squares (2..3, 10..12), the building removed, scorched ground, then the
/// killer appears and runs.
fn garage_explodes(s: &mut Script) {
    for gx in [0x1000, 0x1800] {
        for gy in [0x5000, 0x5800, 0x6000] {
            if s.random(2) == 0 {
                // 68k evaluates the Randoms in the order z, y, x.
                let z = 17000 + s.random(1500);
                let y = gy + s.random(0x800);
                let x = gx + s.random(0x800);
                s.explosion(x, y, z, 3000);
            }
        }
    }
    for sx in 2..=3 {
        for sy in 10..=12 {
            for cube in 1..=2 {
                s.set_cube(sx, sy, cube, 0x200, 0x200, 0, 0, 0);
            }
        }
    }
    s.set_cube(1, 0x0b, 1, 0x200, 0x200, 0, 0, 0);
    s.set_face(0, 2, 0x0b, 0, 1, 0x14c, 1);
    s.set_face(0, 3, 0x0b, 0, 1, 0x14e, 1);
    s.set_face(0, 3, 0x0d, 0, 1, 0x0d4, 1);
    s.set_face(0, 2, 0x0d, 0, 1, 0x0d0, 1);
    for sy in 10..=13 {
        s.set_face(0, 4, sy, 0, 1, 0xda, 1);
    }
    s.say(BOSS, 3);
    s.create_actor(1, -9, 0x2400, 0x6c00, 0, 0, 0);
    s.t(4, ACTOR_GOTO, 1, 0x2400, 0xbc00, 0);
    s.t(0x1e, ACTOR_KILLED, 1, 0, 0, 0);
}

/// Mission 2 alarm boxes (cube 2): scenery 22 flashing, 21 still.
fn alarm_boxes(s: &mut Script, scenery: i32) {
    for (sx, sy, rot) in [
        (0x59, 0x52, 0xbf),
        (0x59, 0x56, 0xbf),
        (0x59, 0x5c, 0xbf),
        (0x5a, 0x5d, 0),
        (0x5f, 0x5d, 0),
    ] {
        s.set_cube(sx, sy, 2, 0x40, 0x150, scenery, rot, 1);
    }
}

fn m3_release_cars(s: &mut Script) {
    for i in 0..3 {
        s.actor_release(M3_CARS + i);
    }
}

/// `Finish3`: success paid per car dumped.
fn m3_finish(s: &mut Script, dumped: i32) {
    s.kill_all_mt();
    m3_release_cars(s);
    s.say(BOSS, 24);
    win(s, dumped * 75_000);
}

/// A prisoner comes out of the breached wall and boards the player's bus.
fn m8_spawn_prisoner(s: &mut Script) {
    s.create_actor(M8_PRISONER, -10, 110_000, 0x34800, 0, 0, 0);
    s.t(0x324, ACTOR_BOARDS_PLAYER, M8_PRISONER, 0, 0, 0);
    s.t(0x328, ACTOR_GONE, M8_PRISONER, 0, 0, 0);
}

/// A prisoner gets out of the player's vehicle and walks into the house.
fn m8_unload_prisoner(s: &mut Script) {
    s.ped_exits_vehicle(M8_PRISONER, -1, -10, -1);
    m8_prisoner_triggers(s);
}

fn m8_prisoner_triggers(s: &mut Script) {
    s.t(0x329, ACTOR_GONE, M8_PRISONER, 0, 0, 0);
    s.t(
        0x32a,
        ACTOR_GOTO_VANISH,
        M8_PRISONER,
        M8_HOUSE.0,
        M8_HOUSE.1,
        0,
    );
}
