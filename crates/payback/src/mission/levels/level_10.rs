//! Level 10 (city 1 "Los Francos", level 0 "Going For Gold") mission script
//! (docs/re/missions_10.md). Update 7: init 68k `0xb953c`, handler
//! `0xc1a92`, helpers `0xb73d0` (race), `0xb7844` (M500), `0xb7c40` (M600),
//! unlock routine `0xb5230`.

use crate::mission::api::{MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::kind::*;
use crate::mission::{LevelScript, Script};
use std::ops::RangeInclusive;

/// `SetFaceN/E/S` face indices for [`Script::set_face`].
const FACE_N: usize = 1;
const FACE_E: usize = 2;
const FACE_S: usize = 3;

/// Hotel block (M700) and a second building: ambience rectangles.
const R1: [i32; 4] = [0x1000, 0x34000, 0x9800, 0x3d800];
const R2: [i32; 4] = [0x1a800, 0x36800, 0x1e000, 0x38800];

/// Race opponents' AI waypoints (squares) and arrival radii.
pub(crate) const AI_X: [i32; 14] = [
    106, 106, 116, 123, 123, 121, 105, 106, 110, 115, 117, 120, 120, 112,
];
pub(crate) const AI_Y: [i32; 14] = [5, 22, 23, 24, 35, 39, 39, 31, 29, 28, 17, 15, 5, 4];
const AI_SP: [i32; 14] = [
    2500, 3000, 1500, 3000, 4000, 2500, 3000, 3500, 2500, 3000, 3500, 3500, 2500, 3000,
];
/// Race checkpoints: actor coordinate tests (one lap = 8 checkpoints).
const CP_TYPE: [u8; 8] = [
    ACTOR_Y_GT, ACTOR_X_GT, ACTOR_Y_GT, ACTOR_X_LE, ACTOR_Y_LE, ACTOR_X_GT, ACTOR_Y_LE, ACTOR_X_LE,
];
const CP_VAL: [i32; 8] = [
    0xa800, 0x3b800, 0x12800, 0x37000, 0x10000, 0x39000, 0x3800, 0x36400,
];

/// M100 fixed route links: on event, add tracked trigger
/// `(event, type, a, b, c, d)`. Henchman 0x65, contacts 0x66 / 0x67.
const M100_ROUTE: [(i32, i32, u8, i32, i32, i32, i32); 18] = [
    (0x6d, 0x6e, ACTOR_GOTO, 0x65, 0x1e400, 0x27c00, -1),
    (0x6e, 0x82, ACTOR_GOTO, 0x65, 0x1bd50, 0x27c00, -1),
    (0x82, 0x6f, ACTOR_GOTO, 0x65, 0x18c00, 0x27c00, -1),
    (0x6f, 0x70, ACTOR_GOTO, 0x65, 0x188f8, 0x2b368, -1),
    (0x70, 0x71, TIMER, 2, -1, -1, -1),
    (0x83, 0x72, ACTOR_GOTO, 0x65, 0x18c00, 0x2e310, -1),
    (0x72, 0x79, ACTOR_GOTO, 0x65, 0x1c138, 0x2e310, -1),
    (0x79, 0x73, ACTOR_GOTO, 0x65, 0x20aa8, 0x2e630, -1),
    (0x73, 0x74, TIMER, 2, -1, -1, -1),
    (0xa1, 0x75, ACTOR_GOTO, 0x65, 0x29c00, 0x2e400, -1),
    (0x75, 0x76, ACTOR_GOTO, 0x65, 0x29c00, 0x2fc00, -1),
    (0x76, 0x77, ACTOR_GOTO, 0x65, 0x2ac00, 0x2fc00, -1),
    (0x77, 0x78, ACTOR_GOTO_VANISH, 0x65, 0x2ac00, 0x2f508, 1),
    (0xaa, 0xab, ACTOR_GOTO, 0x66, 0x17000, 0x29c00, -1),
    (0xab, 0xac, ACTOR_GOTO, 0x66, 0x15c00, 0x29c00, -1),
    (0xac, 0xad, ACTOR_GOTO_VANISH, 0x66, 0x15c00, 0x2bb38, 1),
    (0xb4, 0xb5, ACTOR_GOTO, 0x67, 0x21c00, 0x32400, -1),
    (0xb5, 0xb6, ACTOR_GOTO_VANISH, 0x67, 0x21c00, 0x323d0, 1),
];

/// M500 convoy route (squares), index 0..12.
const M500_X: [i32; 13] = [50, 50, 61, 61, 72, 72, 72, 77, 82, 82, 82, 77, 77];
const M500_Y: [i32; 13] = [91, 80, 80, 72, 72, 59, 47, 47, 47, 40, 35, 35, 37];

/// M600 route of the tail and Ike (squares), index 0..10.
const M600_X: [i32; 11] = [11, 11, 16, 20, 21, 21, 24, 24, 34, 34, 40];
const M600_Y: [i32; 11] = [32, 35, 35, 35, 34, 27, 27, 24, 24, 22, 22];

/// Centre of map square `n` in world units.
fn sq(n: i32) -> i32 {
    n << 11 | 0x400
}

/// Race state (`0x2fb0c0..0x2fb108`): opponents' waypoint index, and per
/// car (0 = the player's car 10) checkpoint, lap and finished flag.
#[derive(Default)]
struct Race {
    wp: [usize; 5],
    cp: [usize; 6],
    lap: [i32; 6],
    fin: [bool; 6],
}

/// Level 10 globals (missions_10.md §0).
#[derive(Default)]
pub struct Level10 {
    /// `0x2fba08`: per-mission counter `S` (M100 meetings, enemies left).
    count: i32,
    /// `0x2fb14c` as M100 pay factor `P`.
    pay: i32,
    /// `0x2fb14c` as M200 "poison warning shown" flag.
    warned: bool,
    /// `0x2fba4c`: M200 gas sound ref 1 playing.
    gas_sound: bool,
    /// `0x2fba30` / `0x2fba34`: M400 henchmen A / B alive.
    hench_a: bool,
    hench_b: bool,
    race: Race,
    /// `0x2fb118`: M500 waypoint counters of 0x1f5..0x1f7.
    convoy: [usize; 3],
    /// `0x2fb128`: M600 waypoint counters of the tail (0x259) and Ike (0x25a).
    tail: [usize; 2],
}

/// `UnlockPhones10()` (`0xb5230`): clear the answered slot; once the four
/// stage-1 missions are all done, open phones 5..8; ring the phones.
fn unlock_phones(s: &mut Script) {
    let cur = s.cur_slot();
    s.consume_slot();
    // Signed compare as in the original (cur is never -1 here).
    if cur < 4 && (0..4).all(|i| s.phone_event(i) == -1) {
        s.set_phone(4, 400, 5, false);
        s.set_phone(5, 500, 6, false);
        s.set_phone(6, 600, 7, false);
        s.set_phone(7, 700, 8, false);
    }
    s.refresh_phones();
    s.set_cur_slot(-1);
}

/// Mission success tail (§1.2).
fn succeed(s: &mut Script, phrase: u32, money: i32) {
    s.say(2, phrase);
    s.add_money(money);
    s.show_message(MISSION_COMPLETE);
    unlock_phones(s);
}

/// Mission failure tail (§1.2).
fn fail(s: &mut Script, phrase: u32) {
    s.say(2, phrase);
    s.show_message(MISSION_FAILED);
    unlock_phones(s);
}

fn release(s: &mut Script, ids: RangeInclusive<i32>) {
    for id in ids {
        s.actor_release(id);
    }
}

fn add_rect(s: &mut Script, ev: i32, r: [i32; 4], floor: i32) {
    s.add_trigger_ex(ev, IN_RECT, r[0], r[1], r[2], r[3], floor);
}

/// Ambience rectangles (`0xc1a92` top, all states): events `base+0..2` =
/// entered the building on floor 0..2, `base+3` = left it. Returns whether
/// `ev` was one of them.
fn ambience(s: &mut Script, ev: i32) -> bool {
    let (base, r) = match ev {
        100000..=100003 => (100000, R1),
        100010..=100013 => (100010, R2),
        _ => return false,
    };
    let f = ev - base;
    if f == 3 {
        s.set_visible_layers(3);
        for g in 0..3 {
            s.kill_trigger(base + g);
        }
        for g in 0..3 {
            add_rect(s, base + g, r, g);
        }
    } else {
        // The roof (floor 2) uses the outdoor value.
        s.set_visible_layers(f + 1);
        let others = (0..3).filter(|&g| g != f);
        for g in others.clone() {
            s.kill_trigger(base + g);
        }
        s.kill_trigger(base + 3);
        for g in others {
            add_rect(s, base + g, r, g);
        }
        s.add_trigger(base + 3, OUT_OF_RECT, r[0], r[1], r[2], r[3]);
    }
    true
}

impl LevelScript for Level10 {
    /// Level init (`0xb953c`).
    fn init(&mut self, s: &mut Script) {
        s.set_intro_phrase(0);
        s.set_phone(0, 1, 1, false);
        s.set_phone(1, 100, 2, false);
        s.set_phone(2, 200, 3, false);
        s.set_phone(3, 300, 4, false);
        s.add_trigger(-1, MONEY_GOAL, 2_000_000, -1, -1, -1);
        s.add_lives(1);
        s.refresh_phones();
        s.show_message(1);
        // Free helicopter; id 1 is never referenced again.
        s.create_actor(1, 17, 0x35000, 0x1b800, 0, 0, 1);
        for f in 0..3 {
            add_rect(s, 100000 + f, R1, f);
        }
        for f in 0..3 {
            add_rect(s, 100010 + f, R2, f);
        }
    }

    /// Event handler (`0xc1a92`).
    fn event(&mut self, s: &mut Script, ev: i32) {
        if ambience(s, ev) || ev == 0 {
            return;
        }
        match s.cur_slot() {
            0 => self.race_event(s, ev),
            1 => self.m100(s, ev),
            2 => self.m200(s, ev),
            3 => self.m300(s, ev),
            4 => self.m400(s, ev),
            5 => self.m500(s, ev),
            6 => self.m600(s, ev),
            7 => self.m700(s, ev),
            _ => match ev {
                1 => self.start_race(s),
                100 => self.start_m100(s),
                200 => self.start_m200(s),
                300 => self.start_m300(s),
                400 => self.start_m400(s),
                500 => self.start_m500(s),
                600 => self.start_m600(s),
                700 => self.start_m700(s),
                _ => {}
            },
        }
    }
}

impl Level10 {
    // ---- Mission "1": street race (phone 1, slot 0) ------------------------

    /// Street race briefing (`0xc3c30`): get into the provided Pug Racer.
    fn start_race(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 2);
        s.create_actor(10, 18, 0x3ac00, 0x2400, 0, 0x10e0, 1);
        s.actor_set_controls(10, false);
        s.t(1, ENTER_ACTOR, 10, -1, -1, -1);
        s.t(10, PLAYER_DOWN, -1, -1, -1, -1);
        s.t(10, ACTOR_GONE, 10, -1, -1, -1);
        s.set_cur_slot(0);
    }

    /// Street race, slot-0 events (`0xc1fe4..0xc2298`): grid, start
    /// lights, failure; everything else goes to [`Self::race`].
    fn race_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            1 => {
                s.say(2, 3);
                s.create_actor(11, 18, 0x37400, 0x1c00, 0, 0x10e0, 2);
                s.create_actor(12, 18, 0x37c00, 0x2400, 0, 0x10e0, 3);
                s.create_actor(13, 18, 0x38c00, 0x1c00, 0, 0x10e0, 4);
                s.create_actor(14, 18, 0x39400, 0x2400, 0, 0x10e0, 5);
                s.create_actor(15, 18, 0x3a400, 0x1c00, 0, 0x10e0, 6);
                for (id, aggro) in [(11, 100), (12, 100), (13, 99), (14, 98), (15, 95)] {
                    s.actor_set_aggro(id, aggro);
                }
                s.set_race_lights(0);
                s.t(2, TIMER, 5, -1, -1, -1);
            }
            2 => {
                s.set_race_lights(1);
                s.t(3, TIMER, 1, -1, -1, -1);
            }
            3 => {
                s.set_race_lights(2);
                s.t(4, TIMER, 1, -1, -1, -1);
            }
            4 => {
                s.say(2, 4);
                s.set_race_lights(3);
                s.actor_set_controls(10, true);
                self.race(s, -1);
                s.t(5, TIMER, 1, -1, -1, -1);
            }
            5 => {
                s.set_race_lights(-1);
                s.set_lap(1, 3);
            }
            10 => {
                s.kill_all_mt();
                s.set_race_lights(-1);
                s.set_lap(-1, 3);
                release(s, 10..=15);
                fail(s, 11);
            }
            // Includes the money-goal event -1, which re-arms the race
            // (harmless: the level is already complete).
            _ => self.race(s, ev),
        }
    }

    /// `Race10(ev)` (`0xb73d0`): -1 arms AI waypoints and checkpoints;
    /// 10000+i = opponent 11+i reached its waypoint; 20000+i = car 10+i
    /// passed its next checkpoint.
    fn race(&mut self, s: &mut Script, ev: i32) {
        let r = &mut self.race;
        match ev {
            -1 => {
                for i in 0..5 {
                    r.wp[i] = 0;
                    let i = i as i32;
                    s.t(
                        10000 + i,
                        ACTOR_GOTO_DIRECT,
                        11 + i,
                        sq(AI_X[0]),
                        sq(AI_Y[0]),
                        AI_SP[0],
                    );
                }
                for i in 0..6 {
                    r.cp[i] = 0;
                    r.lap[i] = 1;
                    r.fin[i] = false;
                    let i = i as i32;
                    s.t(20000 + i, CP_TYPE[0], 10 + i, CP_VAL[0], -1, -1);
                }
            }
            10000..=10004 => {
                let i = (ev - 10000) as usize;
                r.wp[i] = (r.wp[i] + 1) % AI_X.len();
                let w = r.wp[i];
                s.t(
                    ev,
                    ACTOR_GOTO_DIRECT,
                    11 + i as i32,
                    sq(AI_X[w]),
                    sq(AI_Y[w]),
                    AI_SP[w],
                );
            }
            20000..=20005 => {
                let i = (ev - 20000) as usize;
                r.cp[i] += 1;
                if r.cp[i] >= CP_TYPE.len() {
                    r.cp[i] = 0;
                    r.lap[i] += 1;
                    if r.lap[i] < 4 {
                        if i == 0 {
                            s.set_lap(r.lap[0], 3);
                            // 12 "2 laps left", 13 "1 lap left".
                            s.say(2, 10 + r.lap[0] as u32);
                        }
                    } else {
                        r.fin[i] = true;
                        if i == 0 {
                            // The player finished: place = opponents already in.
                            s.kill_all_mt();
                            for j in 0..6 {
                                s.actor_release(10 + j);
                            }
                            let n = r.fin[1..].iter().filter(|&&f| f).count() as i32;
                            s.set_lap(-1, 3);
                            s.say(2, 5 + n as u32);
                            if n < 3 {
                                s.add_money((3 - n) * 85000);
                                s.show_message(MISSION_COMPLETE);
                            } else {
                                s.show_message(MISSION_FAILED);
                            }
                            unlock_phones(s);
                        }
                    }
                }
                if !r.fin[i] && s.cur_slot() != -1 {
                    let c = r.cp[i];
                    s.t(ev, CP_TYPE[c], 10 + i as i32, CP_VAL[c], -1, -1);
                }
            }
            _ => {}
        }
    }

    // ---- Mission 100: follow the henchman (phone 2, slot 1) ----------------

    /// M100 briefing (`0xc3ce4`): follow Kenny's henchman to two meetings.
    fn start_m100(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 14);
        s.create_actor(0x65, !11, 0x1e400, 0x25400, 0, 0xb40, 0);
        s.create_actor(0x66, !10, 0x18818, 0x2b400, 0, 0x870, 0);
        s.create_actor(0x67, !10, 0x20c00, 0x2e7e8, 0, 0xe10, 0);
        for id in 0x65..=0x67 {
            s.t(id, ACTOR_GONE, id, -1, -1, -1);
        }
        s.t(0x6d, TIMER, 20, -1, -1, -1);
        s.t(0x8c, ACTOR_SEES_PLAYER, 0x65, -1, -1, -1);
        s.t(0x96, PLAYER_DOWN, -1, -1, -1, -1);
        self.count = 0;
        self.pay = 3;
        s.set_cur_slot(1);
    }

    /// Stops the henchman's route. Literal range 0x6d..0x79: 0x82, 0x83,
    /// 0xa1 and the contacts' routes survive and may keep walking.
    fn m100_stop_route(s: &mut Script) {
        for ev in 0x6d..=0x79 {
            s.kill_mt(ev);
        }
    }

    /// The henchman makes a break for it after a contact died too early.
    fn m100_break(&mut self, s: &mut Script, other_contact: i32, pay: i32) {
        Self::m100_stop_route(s);
        s.actor_attack(0x65, -1, -1);
        s.actor_release(other_contact);
        self.pay = pay;
        s.say(2, 17);
        self.count = 2;
        s.t(0x69, ACTOR_LOST, 0x65, -1, -1, -1);
    }

    /// M100 events (`0xc229c..0xc2ae8`).
    fn m100(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x71 => {
                self.count = 1;
                s.t(0x83, ACTOR_GOTO, 0x65, 0x18c00, 0x2bc00, -1);
                s.say(2, 21);
                s.t(0xaa, ACTOR_GOTO, 0x66, 0x17000, 0x2b400, -1);
            }
            0x74 => {
                self.count = 2;
                s.t(0xa1, ACTOR_GOTO, 0x65, 0x253bc, 0x2e400, -1);
                s.say(2, 22);
                s.t(0xb4, ACTOR_GOTO, 0x67, 0x20c00, 0x32400, -1);
            }
            // A contact walked away: only a message.
            0xad => {
                s.actor_release(0x66);
                s.say(2, 26);
            }
            0xb6 => {
                s.actor_release(0x67);
                s.say(2, 26);
            }
            0x66 => {
                s.actor_release(0x66);
                if self.count > 0 {
                    s.say(2, 18);
                } else {
                    self.m100_break(s, 0x67, 1);
                }
            }
            0x67 => {
                s.actor_release(0x67);
                if self.count >= 2 {
                    s.say(2, 19);
                } else {
                    let pay = if self.count == 0 { 1 } else { 2 };
                    self.m100_break(s, 0x66, pay);
                }
            }
            0x8c => {
                Self::m100_stop_route(s);
                s.kill_mt(0xad);
                s.kill_mt(0xb6);
                s.actor_attack(0x65, 0x50, -1);
                s.actor_release(0x66);
                s.actor_release(0x67);
                s.say(2, 24);
                if self.count <= 1 {
                    self.pay = 1;
                    self.count = 2;
                }
                s.t(0x69, ACTOR_LOST, 0x65, -1, -1, -1);
            }
            0x65 => {
                s.kill_all_mt();
                release(s, 0x65..=0x67);
                if self.count <= 1 {
                    fail(s, 15);
                } else {
                    succeed(s, 16, self.pay * 85000);
                }
            }
            0x69 => {
                s.kill_all_mt();
                s.actor_release(0x65);
                fail(s, 20);
            }
            0x78 | 0x96 => {
                s.kill_all_mt();
                release(s, 0x65..=0x67);
                fail(s, if ev == 0x78 { 23 } else { 25 });
            }
            _ => {
                if let Some(&(_, next, ty, a, b, c, d)) = M100_ROUTE.iter().find(|r| r.0 == ev) {
                    s.t(next, ty, a, b, c, d);
                }
            }
        }
    }

    // ---- Mission 200: Cartman's shipment (phone 3, slot 2) -----------------

    /// M200 briefing (`0xc3e34`): steal the truck from the central car park.
    fn start_m200(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 27);
        s.create_actor(0xc9, 19, 0x18400, 0x15400, 0, 0x2d0, 0);
        s.t(0xc9, ENTER_ACTOR, 0xc9, -1, -1, -1);
        s.t(0xfa, ACTOR_GONE, 0xc9, -1, -1, -1);
        s.t(0xfa, PLAYER_DOWN, -1, -1, -1, -1);
        self.gas_sound = false;
        s.set_cur_slot(2);
    }

    /// Closes the garage's east face again.
    fn m200_close_garage(s: &mut Script) {
        s.set_face(FACE_E, 0x4f, 0x43, 1, 1, 0x76, 1);
    }

    fn m200_stop_gas(&mut self, s: &mut Script) {
        // The flag is not cleared, as in the original.
        if self.gas_sound {
            s.delete_sound(1);
        }
    }

    /// M200 events (`0xc2aec..0xc3224`).
    fn m200(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0xc9 => {
                s.set_wanted_level(6);
                s.say(2, 29);
                s.t(0xca, GARAGE, 0x4f, 0x43, 1, 1);
            }
            0xca => {
                s.say(2, 30);
                s.t(0xcb, TIMER, 3, -1, -1, -1);
            }
            0xcb => {
                s.create_actor(0xc9, 19, 0x27ac4, 0x21c00, 0, 0x10e0, 0);
                s.create_actor(0xca, !11, 0x27eac, 0x21822, 0, 0x5a0, 0);
                s.create_actor(0xcb, !11, 0x27eac, 0x21ef8, 0, 0x5a0, 0);
                s.set_face(FACE_E, 0x4f, 0x43, 1, 0, 0, 0);
                s.add_sound(
                    1,
                    0x3a,
                    true,
                    0x3d0900 as f32,
                    0x206c as f32,
                    0x27cb8,
                    0x21c00,
                    20000,
                );
                s.t(0xcc, TIMER_TICKS, 5, -1, -1, -1);
                s.t(0xcd, ON_FOOT_NEAR_POINT, 0x27c00, 0x21c00, 0, -1);
                s.t(0xd5, ACTOR_GONE, 0xca, -1, -1, -1);
                s.t(0xdf, ACTOR_GONE, 0xcb, -1, -1, -1);
                s.t(0xce, TIMER, 2, -1, -1, -1);
                s.say(2, 31);
                self.count = 2;
                self.warned = false;
                self.gas_sound = true;
            }
            0xcc => {
                // Gas puff; Random calls in argument order (OS4 0x010d13f4).
                let x = 0x27d2c + s.random(200);
                let y = 0x21b9c + s.random(200);
                let z = 20000 - s.random(200);
                let vx = 0x1e + s.random(80);
                let vy = s.random(80) - 0x28;
                let vz = -s.random(100);
                let life = 400 + s.random(500);
                // TODO(api): the 8th AddGas argument (1) has no parameter
                // in `Script::add_effect`.
                s.add_effect(x, y, z, vx, vy, vz, life);
                s.t(0xcc, TIMER_TICKS, 5, -1, -1, -1);
            }
            0xcd => {
                if !self.warned {
                    self.warned = true;
                    s.say(2, 35);
                }
                s.add_health(-1);
                s.t(0xe6, TIMER_TICKS, 5, -1, -1, -1);
            }
            0xe6 => s.t(0xcd, ON_FOOT_NEAR_POINT, 0x27c00, 0x21c00, 0, -1),
            0xce => {
                s.say(2, 32);
                s.t(0xd2, ACTOR_GOTO, 0xca, 0x29810, 0x20f58, -1);
                let t = s.random(2) + 2;
                s.t(0xd3, TIMER, t, -1, -1, -1);
                s.t(0xdc, ACTOR_GOTO, 0xcb, 0x29810, 0x226c8, -1);
                let t = s.random(2) + 2;
                s.t(0xdd, TIMER, t, -1, -1, -1);
            }
            // Each mutant alternates attack and flee (2-3 s each) forever.
            0xd3 | 0xd4 | 0xdd | 0xde => {
                let (walk, id) = if ev <= 0xd4 {
                    (0xd2, 0xca)
                } else {
                    (0xdc, 0xcb)
                };
                let attack = ev == 0xd3 || ev == 0xdd;
                s.kill_mt(walk);
                s.actor_attack(id, if attack { 0x3c } else { -1 }, -1);
                let next = if attack { ev + 1 } else { ev - 1 };
                let t = s.random(2) + 2;
                s.t(next, TIMER, t, -1, -1, -1);
            }
            0xd5 | 0xdf => {
                self.count -= 1;
                if self.count == 0 {
                    Self::m200_close_garage(s);
                    s.kill_all_mt();
                    self.m200_stop_gas(s);
                    release(s, 0xc9..=0xcb);
                    succeed(s, 34, 250000);
                } else {
                    s.say(2, 33);
                }
            }
            0xfa => {
                Self::m200_close_garage(s);
                s.kill_all_mt();
                self.m200_stop_gas(s);
                release(s, 0xc9..=0xcb);
                fail(s, 28);
            }
            // 0xd2 / 0xdc (mutant reached its walk target) have no case.
            _ => {}
        }
    }

    // ---- Mission 300: rival gang meeting (phone 4, slot 3) -----------------

    /// M300 briefing (`0xc3ee6`): meet a rival gang (an ambush).
    fn start_m300(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 36);
        s.create_actor(0x12d, !9, 0x3400, 0x12c00, 0, 0x5a0, 0);
        s.create_actor(0x12e, !11, 0x2400, 0x11c00, 0, 0x5a0, 0);
        s.create_actor(0x12f, !11, 0x2400, 0x13c00, 0, 0x5a0, 0);
        s.t(0x12d, NEAR_ACTOR, 0x12d, -1, -1, -1);
        s.t(0x137, ACTOR_KILLED, 0x12d, -1, -1, -1);
        s.t(0x138, ACTOR_GONE, 0x12e, -1, -1, -1);
        s.t(0x139, ACTOR_GONE, 0x12f, -1, -1, -1);
        s.t(0x15e, PLAYER_DOWN, -1, -1, -1, -1);
        self.count = 3;
        s.set_cur_slot(3);
    }

    /// M300 events (`0xc3228..0xc34ae`).
    fn m300(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x12d => {
                // The ambush warning uses speaker 0, as in the original.
                s.say(0, 38);
                s.t(0x12e, TIMER, 2, -1, -1, -1);
            }
            0x12e => {
                s.actor_attack(0x12d, -1, -1);
                s.actor_attack(0x12e, 0x46, -1);
                s.actor_attack(0x12f, 0x46, -1);
            }
            0x137..=0x139 => {
                s.actor_release(0x12d + (ev - 0x137));
                self.count -= 1;
                if self.count == 0 {
                    s.kill_all_mt();
                    succeed(s, 39, 250000);
                } else {
                    s.say(2, if self.count == 1 { 41 } else { 40 });
                }
            }
            0x15e => {
                s.kill_all_mt();
                release(s, 0x12d..=0x12f);
                fail(s, 37);
            }
            _ => {}
        }
    }

    // ---- Mission 400: help Kyle in a gunfight (phone 5, slot 4) ------------

    /// M400 briefing (`0xc4008`): reach Kyle within 30 s and protect him.
    fn start_m400(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 42);
        s.create_actor(0x191, !15, 0x1c908, 0x13400, 0, 5000, 0);
        s.create_actor(0x192, !11, 0x18400, 0x15400, 0, 0x708, 0);
        s.create_actor(0x193, !11, 0x19c00, 0x16c00, 0, 0x7d0, 0);
        s.t(0x191, NEAR_POINT_10000, 0x1c908, 0x13400, 0, -1);
        s.t(0x19a, ACTOR_GONE, 0x192, -1, -1, -1);
        s.t(0x19b, ACTOR_GONE, 0x193, -1, -1, -1);
        s.t(0x1c2, PLAYER_DOWN, -1, -1, -1, -1);
        s.t(0x1c3, ACTOR_GONE, 0x191, -1, -1, -1);
        s.t(0x1c4, TIMER, 30, -1, -1, -1);
        self.hench_a = true;
        self.hench_b = true;
        s.set_cur_slot(4);
    }

    /// M400 events (`0xc34b2..0xc3768`).
    fn m400(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x191 => {
                s.kill_mt(0x1c4);
                s.say(2, 43);
                if self.hench_a {
                    s.actor_attack(0x191, 0x5a, 0x192);
                } else if self.hench_b {
                    s.actor_attack(0x191, 0x5a, 0x193);
                }
                s.actor_attack(0x192, 0x3c, 0x191);
                s.actor_attack(0x193, 0x3c, 0x191);
            }
            0x19a | 0x19b => {
                let (dead, other) = if ev == 0x19a {
                    (0x192, 0x193)
                } else {
                    (0x193, 0x192)
                };
                s.kill_mt(0x1c4);
                s.actor_release(dead);
                if ev == 0x19a {
                    self.hench_a = false;
                } else {
                    self.hench_b = false;
                }
                let other_alive = if ev == 0x19a {
                    self.hench_b
                } else {
                    self.hench_a
                };
                if other_alive {
                    // Kyle keeps his old target.
                    s.actor_attack(other, 0x3c, -1);
                    s.say(2, 44);
                } else {
                    s.actor_release(0x191);
                    s.kill_all_mt();
                    succeed(s, 45, 250000);
                }
            }
            0x1c2..=0x1c4 => {
                s.kill_all_mt();
                release(s, 0x191..=0x193);
                fail(s, 46 + (ev - 0x1c2) as u32);
            }
            _ => {}
        }
    }

    // ---- Mission 500: hijack the police van (phone 6, slot 5) --------------

    /// M500 briefing (`0xc415a`): hijack the police van carrying Stan.
    fn start_m500(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 49);
        s.create_actor(0x1f5, 9, 0x1ac00, 0x2dc00, 0, 0x10e0, 0);
        s.create_actor(0x1f6, 20, 0x1bc00, 0x2dc00, 0, 0x10e0, 0);
        s.create_actor(0x1f7, 9, 0x1cc00, 0x2dc00, 0, 0x10e0, 0);
        self.m500(s, -1);
        s.set_cur_slot(5);
    }

    /// Releases the convoy, closes the police-station door, ends M500.
    fn m500_end(s: &mut Script) {
        release(s, 0x1f5..=0x1f7);
        s.kill_all_mt();
        s.set_face(FACE_N, 0x4d, 0x25, 1, 1, 0x76, 1);
    }

    /// `Mission500(ev)` (`0xb7844`). Also receives the money-goal event -1,
    /// which re-arms the mission (harmless, as in the original).
    fn m500(&mut self, s: &mut Script, ev: i32) {
        match ev {
            -1 => {
                for i in 0..3 {
                    self.convoy[i] = 0;
                    let id = 0x1f5 + i as i32;
                    s.t(id, ACTOR_GOTO, id, sq(M500_X[0]), sq(M500_Y[0]), -1);
                }
                s.t(0x226, PLAYER_DOWN, -1, -1, -1, -1);
                s.t(0x226, ACTOR_GONE, 0x1f6, -1, -1, -1);
                s.t(0x1fe, ENTER_ACTOR, 0x1f6, -1, -1, -1);
            }
            0x1fe => {
                for id in 0x1f5..=0x1f7 {
                    s.kill_mt(id);
                }
                s.set_wanted_level(6);
                s.say(2, 51);
                s.t(0x208, GARAGE, 0x4f, 0x43, 1, 1);
            }
            0x208 => {
                Self::m500_end(s);
                succeed(s, 52, 250000);
            }
            0x226 => {
                Self::m500_end(s);
                fail(s, 50);
            }
            0x1f5..=0x1f7 => {
                let i = (ev - 0x1f5) as usize;
                self.convoy[i] += 1;
                let target = match (self.convoy[i], i) {
                    (12, 1) => {
                        // Open the police-station door (face off).
                        s.set_face(FACE_N, 0x4d, 0x25, 1, 0, 0, 0);
                        Some((sq(M500_X[12]), sq(M500_Y[12])))
                    }
                    (12, 0) => Some((0x25c00, 0x11c00)),
                    (12, _) => Some((0x27400, 0x11c00)),
                    (13, 1) => {
                        // The van is inside the station: too late.
                        Self::m500_end(s);
                        fail(s, 53);
                        None
                    }
                    (n, _) if n < 12 => Some((sq(M500_X[n]), sq(M500_Y[n]))),
                    _ => None,
                };
                if let Some((x, y)) = target {
                    s.t(ev, ACTOR_GOTO, ev, x, y, -1);
                }
            }
            _ => {}
        }
    }

    // ---- Mission 600: kill Ike's tail (phone 7, slot 6) --------------------

    /// M600 briefing (`0xc41f2`): kill the man tailing Ike.
    fn start_m600(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 54);
        s.create_actor(0x259, !11, 0x2400, 0x10400, 0, 0x5a0, 0);
        s.create_actor(0x25a, !9, 0x5c00, 0x10400, 0, 0, 0);
        self.m600(s, -1);
        s.set_cur_slot(6);
    }

    fn m600_fail(s: &mut Script, phrase: u32) {
        s.kill_all_mt();
        release(s, 0x259..=0x25a);
        fail(s, phrase);
    }

    /// `Mission600(ev)` (`0xb7c40`). Also receives the money-goal event -1
    /// (re-arms the mission, harmless).
    fn m600(&mut self, s: &mut Script, ev: i32) {
        match ev {
            -1 => {
                self.tail = [0, 1];
                for i in 0..2 {
                    let id = 0x259 + i as i32;
                    let n = self.tail[i];
                    s.t(id, ACTOR_GOTO, id, sq(M600_X[n]), sq(M600_Y[n]), -1);
                }
                s.t(0x26c, ACTOR_GONE, 0x259, -1, -1, -1);
                s.t(0x280, ACTOR_SEES_PLAYER_QUIET, 0x259, -1, -1, -1);
                s.t(0x28a, PLAYER_DOWN, -1, -1, -1, -1);
                // Arrow on Ike ("Ike will be targetted").
                s.t(0x28b, ACTOR_KILLED, 0x25a, -1, -1, -1);
            }
            0x26c => {
                s.kill_all_mt();
                release(s, 0x259..=0x25a);
                succeed(s, 56, 225000);
            }
            0x280 => {
                // Permanent switch to attack.
                s.actor_attack(0x259, 0x5a, -1);
                s.say(2, 59);
            }
            0x28a => Self::m600_fail(s, 55),
            0x28b => Self::m600_fail(s, 57),
            0x259 | 0x25a => {
                let i = (ev - 0x259) as usize;
                // The tail only advances while it is behind Ike; otherwise
                // it stays idle for the rest of the mission (literal code,
                // possibly unintended).
                if i == 1 || self.tail[0] < self.tail[1] {
                    self.tail[i] += 1;
                    match self.tail[i] {
                        11 => {
                            if i == 1 {
                                // Ike vanishes on arrival.
                                s.t(0x25a, ACTOR_GOTO_VANISH, 0x25a, 0x14400, 0xab20, -1);
                            }
                        }
                        12 => {
                            if i == 1 {
                                Self::m600_fail(s, 58);
                            }
                        }
                        n if n < 11 => s.t(ev, ACTOR_GOTO, ev, sq(M600_X[n]), sq(M600_Y[n]), -1),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // ---- Mission 700: kidnapped brother (phone 8, slot 7) ------------------

    /// M700 briefing (`0xc4266`): rescue Kyle's brother from the hotel
    /// penthouse.
    fn start_m700(&mut self, s: &mut Script) {
        s.kill_all_phones();
        s.say(0, 60);
        s.create_actor(0x2bd, !9, 0x8c00, 0x35400, 1, 0x10e0, 0);
        s.create_actor(0x2be, !11, 0x8400, 0x36800, 1, 0, 0);
        s.create_actor(0x2bf, !11, 0x8400, 0x35400, 1, 0x10e0, 0);
        // Door target: special object 3 (bullet.obj).
        s.create_actor(0x2c0, 1003, 0x8c00, 0x38000, 1, 0, 0);
        s.t(0x2bd, AT_POINT, 0x8800, 0x38c00, 0, -1);
        s.t(0x2ee, PLAYER_DOWN, -1, -1, -1, -1);
        self.count = 2;
        s.set_cur_slot(7);
    }

    /// M700 events (`0xc378a..0xc3bcc`).
    fn m700(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x2bd => {
                s.say(2, 61);
                s.t_ex(0x2be, IN_RECT, 0x6000, 0x38000, 0x9000, 0x38800, 1);
                s.t(0x2c6, ACTOR_KILLED, 0x2c0, -1, -1, -1);
            }
            0x2be => {
                // Deaths are only watched from here on: a kidnapper killed
                // earlier is never counted (literal behaviour).
                s.say(2, 62);
                s.t(0x2ef, ACTOR_GONE, 0x2bd, -1, -1, -1);
                s.t(0x2d1, ACTOR_GONE, 0x2be, -1, -1, -1);
                s.t(0x2d2, ACTOR_GONE, 0x2bf, -1, -1, -1);
            }
            0x2c6 => {
                // Remove the wall between squares (17,112) and (17,111).
                s.set_face(FACE_N, 0x11, 0x70, 2, 0, 0, 0);
                s.set_face(FACE_S, 0x11, 0x6f, 2, 0, 0, 0);
                s.say(2, 63);
                s.actor_attack(0x2be, 0x46, -1);
                s.t(0x2c7, PLAYER_Y_LE, 0x36000, -1, -1, -1);
            }
            0x2c7 => {
                s.say(2, 65);
                s.actor_attack(0x2bf, 0x46, -1);
            }
            0x2d1 | 0x2d2 => {
                self.count -= 1;
                if self.count == 0 {
                    s.kill_all_mt();
                    release(s, 0x2bd..=0x2c0);
                    succeed(s, 68, 250000);
                } else {
                    s.say(2, 67);
                }
            }
            0x2ee | 0x2ef => {
                s.kill_all_mt();
                release(s, 0x2bd..=0x2c0);
                fail(s, if ev == 0x2ee { 64 } else { 66 });
            }
            _ => {}
        }
    }
}
