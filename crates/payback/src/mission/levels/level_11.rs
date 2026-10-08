//! Level 11 (city 1, level 1, "Remote Control") mission script
//! (docs/re/missions_11.md). Init 68k `0xb96ec`, handler `0xc436a`, unlock
//! routine `0xb52c2`; OS4 `0x010eb188` / `0x010f16ec` / `0x010d9dc8`.
//!
//! Nine phone missions plus the "Kill frenzy" RC-truck bonus and two
//! visible-layer zones (roof cut-away inside the hotel and the kidnap house).

use std::ops::RangeInclusive;

use crate::mission::api::{MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::kind::*;
use crate::mission::{LevelScript, Script};

/// Global phrase "Kill frenzy!".
const KILL_FRENZY: u32 = 2029;

/// Centre of map square `n` in world units.
const fn sq(n: i32) -> i32 {
    (n << 11) | 0x400
}

/// Visible-layer zone A (hotel, squares (2,104)-(19,123)) and zone B
/// (M3 kidnap house, (53,109)-(60,115)): event base and rectangle.
const ZONE_A: (i32, [i32; 4]) = (100_000, [0x1000, 0x34000, 0x9800, 0x3d800]);
const ZONE_B: (i32, [i32; 4]) = (100_010, [0x1a800, 0x36800, 0x1e000, 0x39800]);

/// RC-truck bonus: actor id of the truck (also its "got in" event).
const RC_TRUCK: i32 = 100_020;

/// M1: the apartment block centre (80,112).
const BLOCK: (i32, i32) = (0x28400, 0x38400);
/// M2: bus route in squares (68k `0xb7fa0` / `0xb7fe4`).
const BUS_ROUTE: [(i32, i32); 17] = [
    (65, 125),
    (50, 125),
    (50, 118),
    (50, 110),
    (42, 110),
    (33, 110),
    (33, 120),
    (38, 120),
    (38, 116),
    (50, 116),
    (61, 116),
    (61, 109),
    (69, 109),
    (76, 109),
    (76, 115),
    (79, 115),
    (79, 125),
];
/// M3: our helipad (106,55).
const HELIPAD: (i32, i32) = (0x35000, 0x1b800);
/// M3 patrol pair: both ends and the two middle points.
const PAIR_ENDS: [(i32, i32); 2] = [(0x1ac00, 0x38c00), (0x1dc00, 0x38c00)];
const PAIR_MIDDLE: [(i32, i32); 2] = [(0x1c000, 0x38c00), (0x1c800, 0x38c00)];
/// M4: the hotel drop-off (20,109).
const HOTEL_STOP: (i32, i32) = (0xa400, 0x36c00);
/// M5: the car-park meeting point (72,120) and the briefcase spot (69,121).
const MEETING: (i32, i32) = (0x24400, 0x3c400);
const BRIEFCASE: (i32, i32) = (0x22c00, 0x3cc00);
/// M9: the wife's route in squares.
const WIFE_ROUTE: [(i32, i32); 6] = [(86, 67), (82, 67), (82, 71), (72, 71), (72, 59), (86, 59)];

/// Level 11 globals (missions_11.md §0). The mission state itself is the
/// shared current phone slot (`0x11a354`).
#[derive(Default)]
pub struct Level11 {
    /// `0x2fba08`: M1 "player near the block", M4 stage, M5 "boarded",
    /// M8 "alerted".
    b: i32,
    /// `0x2fb150`: M1 "bomb in range", M4 arrivals, M5 "killed",
    /// M8 "remaining".
    c: i32,
    /// `0x2fb9f8`: 1 = M3 succeeded, -1 = brother killed or M5 started.
    kidnap: i32,
    /// `0x2fba44`: M5 player at the meeting point.
    at_meeting: bool,
    /// `0x2fb130`: M2 bus waypoint.
    bus_wp: usize,
    /// `0x2fb134`: M3 arrivals of the guard pair.
    pair_arrived: i32,
    /// `0x2fb138`: M3 guards of the pair still alive.
    pair_alive: i32,
    /// `0x2fb13c`: M3 player at the helipad.
    at_pad: bool,
    /// `0x2fba04`: M3 alarm raised or brother grabbed.
    alarm: bool,
    /// `0x2fb140`: M9 waypoint of the wife's car.
    wife_wp: usize,
}

fn release(s: &mut Script, ids: RangeInclusive<i32>) {
    for id in ids {
        s.actor_release(id);
    }
}

fn attack(s: &mut Script, ids: RangeInclusive<i32>, mode: i32) {
    for id in ids {
        s.actor_attack(id, mode, -1);
    }
}

/// M5: open / close the garage door at (48,124) (east face of cube 1).
fn m5_door(s: &mut Script, open: bool) {
    if open {
        s.set_face(2, 0x30, 0x7c, 1, 0, 0, 0);
    } else {
        s.set_face(2, 0x30, 0x7c, 1, 1, 0x76, 1);
    }
}

impl LevelScript for Level11 {
    /// `Init11` (68k `0xb96ec`).
    fn init(&mut self, s: &mut Script) {
        s.set_intro_phrase(1);
        s.set_phone(1, 100, 10, false);
        s.set_phone(2, 200, 11, false);
        s.set_phone(3, 300, 12, false);
        s.set_phone(4, 400, 13, false);
        s.add_trigger(-1, MONEY_GOAL, 2_000_000, 0, 0, 0);
        s.add_lives(1);
        s.show_message(0);
        for (base, r) in [ZONE_A, ZONE_B] {
            for floor in 0..3 {
                s.add_trigger_ex(base + floor, IN_RECT, r[0], r[1], r[2], r[3], floor);
            }
        }
        s.create_actor(RC_TRUCK, 21, 0x2bc00, 0x25400, 0, 0xb40, 1);
        s.add_trigger(RC_TRUCK, ENTER_ACTOR_QUIET, RC_TRUCK, 0, 0, 0);
        s.add_trigger(100_030, ACTOR_GONE, RC_TRUCK, 0, 0, 0);
        self.kidnap = 0;
        s.refresh_phones();
    }

    /// `Handler11` (68k `0xc436a`).
    fn event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0 => {}
            100_000..=100_003 => zone(s, ZONE_A, ev),
            100_010..=100_013 => zone(s, ZONE_B, ev),
            100_020 | 100_021 | 100_022 | 100_025 | 100_030 | 100_031 => rc_bonus(s, ev),
            _ => match s.cur_slot() {
                1 => self.m1(s, ev),
                2 => self.m2(s, ev),
                3 => self.m3(s, ev),
                4 => self.m4(s, ev),
                5 => self.m5(s, ev),
                6 => self.m6(s, ev),
                7 => self.m7(s, ev),
                8 => self.m8(s, ev),
                9 => self.m9(s, ev),
                _ => self.phone(s, ev),
            },
        }
    }
}

/// Visible-layer zone events (68k `0xc45e2..0xc4a94`): entering the
/// rectangle on floor f draws f+1 layers; leaving it restores all three.
/// The triggers are untracked, so mission clean-ups never remove them.
fn zone(s: &mut Script, (base, r): (i32, [i32; 4]), ev: i32) {
    let f = ev - base;
    if f < 3 {
        s.set_visible_layers(f + 1);
        let others = (0..3).filter(|&g| g != f);
        for g in others.clone() {
            s.kill_trigger(base + g);
        }
        s.kill_trigger(base + 3);
        for g in others {
            s.add_trigger_ex(base + g, IN_RECT, r[0], r[1], r[2], r[3], g);
        }
        s.add_trigger(base + 3, OUT_OF_RECT, r[0], r[1], r[2], r[3]);
    } else {
        s.set_visible_layers(3);
        for g in 0..3 {
            s.kill_trigger(base + g);
        }
        for g in 0..3 {
            s.add_trigger_ex(base + g, IN_RECT, r[0], r[1], r[2], r[3], g);
        }
    }
}

/// "Kill frenzy" RC-truck bonus (68k `0xc4410..0xc45e0`): earn 15,000 in
/// 60 s with the truck's RC cars. Independent of the mission state; does
/// not unlock phones, but success still counts for the multiplier.
fn rc_bonus(s: &mut Script, ev: i32) {
    match ev {
        RC_TRUCK => {
            s.kill_trigger(100_030);
            s.add_trigger(100_031, ACTOR_GONE, RC_TRUCK, 0, 0, 0);
            s.say(2, 85);
            s.show_message(KILL_FRENZY);
            s.add_trigger(100_025, PLAYER_DOWN, 0, 0, 0, 0);
            s.add_trigger(100_021, MONEY_GAIN, 15_000, 0, 0, 0);
            s.add_trigger(100_022, COUNTDOWN, 60, 0, 0, 0);
        }
        100_021 => {
            s.say(2, 87);
            s.kill_trigger(100_025);
            s.kill_trigger(100_022);
            s.add_money(100_000);
            s.show_message(MISSION_COMPLETE);
        }
        100_022 => {
            s.say(2, 88);
            s.kill_trigger(100_025);
            s.kill_trigger(100_021);
            s.show_message(MISSION_FAILED);
        }
        100_025 => {
            s.say(2, 89);
            s.kill_trigger(100_021);
            s.kill_trigger(100_022);
            s.show_message(MISSION_FAILED);
        }
        100_030 => {
            s.actor_release(RC_TRUCK);
            s.kill_trigger(RC_TRUCK);
            s.say(2, 86);
        }
        100_031 => {
            // Quirk: 100031 stays armed after the bonus ended, so destroying
            // the truck later still plays phrase 90.
            s.actor_release(RC_TRUCK);
            s.kill_trigger(100_021);
            s.kill_trigger(100_022);
            s.kill_trigger(100_025);
            s.say(2, 90);
        }
        _ => {}
    }
}

impl Level11 {
    /// `Unlock11` (68k `0xb52c2`): empty the finished slot; M3 opens M5
    /// unless the brother died; slots 1-5 all empty open M6-M9.
    fn unlock(&mut self, s: &mut Script) {
        let slot = s.cur_slot();
        if (1..=9).contains(&slot) {
            s.clear_phone(slot as usize);
        }
        // The `else` is original: finishing M3 with M5 opened never opens
        // M6-M9 directly.
        if slot == 3 && self.kidnap != -1 {
            s.set_phone(5, 500, 12, false);
        } else if (1..=5).contains(&slot) && (1..=5).all(|i| s.phone_event(i) == -1) {
            s.set_phone(6, 600, 14, false);
            s.set_phone(7, 700, 15, false);
            s.set_phone(8, 800, 16, false);
            s.set_phone(9, 900, 17, false);
        }
        s.refresh_phones();
        s.set_cur_slot(-1);
    }

    /// `AddMoney(m); ShowMessage(2022); Unlock11()`.
    fn win(&mut self, s: &mut Script, money: i32) {
        s.add_money(money);
        s.show_message(MISSION_COMPLETE);
        self.unlock(s);
    }

    /// `ShowMessage(2028); Unlock11()` (68k tail `0xc6780`).
    fn failed(&mut self, s: &mut Script) {
        s.show_message(MISSION_FAILED);
        self.unlock(s);
    }

    /// `D(2, phrase); ShowMessage(2028); Unlock11()` (68k tail `0xc6770`).
    fn fail(&mut self, s: &mut Script, phrase: u32) {
        s.say(2, phrase);
        self.failed(s);
    }

    /// Phone dispatch while no mission runs (68k `0xc6d6e`). Every phone
    /// handler first silences the other phones and gives the briefing.
    fn phone(&mut self, s: &mut Script, ev: i32) {
        if !(100..=900).contains(&ev) || ev % 100 != 0 {
            return;
        }
        s.kill_all_phones();
        match ev {
            100 => self.start_m1(s),
            200 => self.start_m2(s),
            300 => {
                s.say(0, 14);
                self.m3(s, -1);
            }
            400 => self.start_m4(s),
            500 => self.start_m5(s),
            600 => self.start_m6(s),
            700 => self.start_m7(s),
            800 => self.start_m8(s),
            _ => self.m9(s, -1), // 900
        }
        s.set_cur_slot(ev / 100);
    }

    // ---- M1: apartment bomb ---------------------------------------------

    /// M1 setup, phone 10 (68k `0xc6dca`): pick up the big bomb from a crate
    /// at (107,81).
    fn start_m1(&mut self, s: &mut Script) {
        s.say(0, 2);
        s.set_scenery(0x6b, 0x51, 0, 6, 0x2c, 7);
        s.t(0x65, SCENERY_GONE, 0x6b, 0x51, 0, 0);
        s.t(0x96, PLAYER_DOWN, 0, 0, 0, 0);
        s.set_projectile_id(0x65);
        self.b = 0;
    }

    /// M1 events (68k `0xc4ac2`, table `0xc4ad6`): place the bomb next to
    /// the block at (80,112) and detonate it within range.
    fn m1(&mut self, s: &mut Script, ev: i32) {
        let (bx, by) = BLOCK;
        match ev {
            0x65 => {
                s.say(2, 3);
                // Weapon 8 = remote bomb placed (INFERRED, doc §8.1).
                s.t(0x66, WEAPON_FIRED, 8, 0, 0, 0);
                s.t(0x67, AT_POINT, bx, by, 0, 0);
            }
            0x67 => {
                self.b = 1;
                s.say(2, 6);
                s.t(0x6e, PLAYER_OUTSIDE, bx, by, 0, 0);
            }
            0x6e => {
                self.b = 0;
                s.say(2, 47);
                s.t(0x67, AT_POINT, bx, by, 0, 0);
            }
            0x66 => {
                if self.b == 1 {
                    self.c = 1;
                    s.say(2, 5);
                    s.kill_mt(0x6e);
                    s.t(0x68, ACTOR_OUTSIDE, 0x65, bx, by, 0);
                } else {
                    self.c = 0;
                    s.say(2, 7);
                    s.kill_mt(0x67);
                    s.t(0x69, ACTOR_INSIDE, 0x65, bx, by, 0);
                }
                s.t(0x6a, ACTOR_GONE, 0x65, 0, 0, 0);
            }
            0x68 => {
                s.say(2, 7);
                s.t(0x69, ACTOR_INSIDE, 0x65, bx, by, 0);
                self.c = 0;
            }
            0x69 => {
                s.say(2, 5);
                s.t(0x68, ACTOR_OUTSIDE, 0x65, bx, by, 0);
                self.c = 1;
            }
            0x6a => {
                if self.c == 1 {
                    // The original reuses `b`/`c` as the loop counters;
                    // every later reader re-initialises them first.
                    for x in 78..=82 {
                        for y in 110..=113 {
                            if s.random(2) == 0 {
                                let ex = x * 0x800 + s.random(0x800);
                                let ey = y * 0x800 + s.random(0x800);
                                let ez = 17000 + s.random(1500);
                                s.explosion(ex, ey, ez, 3000);
                            }
                            s.set_cube(x, y, 1, 0x200, 0x200, 0, 0, 0);
                        }
                    }
                    for x in 78..=83 {
                        s.set_face(0, x, 114, 0, 1, 0x1dd, 1);
                    }
                    for y in 110..=113 {
                        s.set_face(0, 83, y, 0, 1, 0x1dd, 1);
                    }
                    s.kill_all_mt();
                    s.say(2, 9);
                    self.win(s, 220_000);
                } else {
                    s.kill_all_mt();
                    self.fail(s, 8);
                    s.set_scenery(0x6b, 0x51, 0, 0, 0, 0);
                }
                s.set_projectile_id(-1);
            }
            0x96 => {
                s.kill_all_mt();
                self.fail(s, 4);
                s.set_scenery(0x6b, 0x51, 0, 0, 0, 0);
                s.set_projectile_id(-1);
            }
            _ => {}
        }
    }

    // ---- M2: drug bus ---------------------------------------------------

    /// M2 phone 11 (68k `0xc6e64`): a bazooka crate at (115,108).
    fn start_m2(&mut self, s: &mut Script) {
        s.say(0, 10);
        s.set_scenery(0x73, 0x6c, 0, 6, 0x2c, 1);
        self.m2(s, -1);
    }

    /// M2 helper (68k `0xb8028`): blow up the circling bus within 75 s.
    fn m2(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0xc9 => {
                self.bus_wp = if self.bus_wp + 1 > 16 {
                    0
                } else {
                    self.bus_wp + 1
                };
                bus_goto(s, self.bus_wp);
            }
            0xca => {
                s.kill_all_mt();
                s.actor_release(0xc9);
                s.say(2, 12);
                self.win(s, 200_000);
                // Original order: the wanted level is set after the unlock.
                s.set_wanted_level(5);
            }
            0xfa => {
                s.kill_all_mt();
                s.actor_release(0xc9);
                self.fail(s, 11);
            }
            0xfb => {
                // The bazooka crate is not removed on failure (unlike M1).
                s.kill_all_mt();
                s.actor_release(0xc9);
                self.fail(s, 13);
            }
            // Original `default:` = setup; it would rebuild the mission on
            // any unlisted event (none is generated in practice).
            _ => {
                s.create_actor(0xc9, 3, 0x27c00, 0x3ec00, 0, 0x10e0, 6);
                self.bus_wp = 0;
                bus_goto(s, 0);
                s.t(0xca, ACTOR_KILLED, 0xc9, 0, 0, 0);
                s.t(0xfa, PLAYER_DOWN, 0, 0, 0, 0);
                s.t(0xfb, COUNTDOWN, 75, 0, 0, 0);
            }
        }
    }

    // ---- M3: kidnap the rival's brother ---------------------------------

    /// M3 helper (68k `0xb82ca`, phone `0xc6eba`): sneak into the guarded
    /// house at (53..60,109..115), grab the brother and fly him to the
    /// helipad at (106,55).
    fn m3(&mut self, s: &mut Script, ev: i32) {
        let (px, py) = HELIPAD;
        match ev {
            0x12d..=0x130 => {
                let old = self.pair_arrived;
                self.pair_arrived += 1;
                if old == 1 || self.pair_alive == 1 {
                    if ev <= 0x12e {
                        pair_goto(s, 0x12f, PAIR_ENDS);
                    } else {
                        pair_goto(s, 0x12d, PAIR_MIDDLE);
                    }
                    self.pair_arrived = 0;
                }
            }
            0x137 => s.t(0x138, ACTOR_GOTO, 0x12f, 0x1c400, 0x36c00, 0),
            0x138 => s.t(0x137, ACTOR_GOTO, 0x12f, 0x1c400, 0x38400, 0),
            0x141 => s.t(0x142, ACTOR_GOTO, 0x130, 0x1ac00, 0x37400, 0),
            0x142 => s.t(0x143, ACTOR_GOTO, 0x130, 0x1ac00, 0x38400, 0),
            0x143 => s.t(0x144, ACTOR_GOTO, 0x130, 0x1bc00, 0x38400, 0),
            0x144 => s.t(0x141, ACTOR_GOTO, 0x130, 0x1bc00, 0x37400, 0),
            0x15f..=0x162 => {
                let k = ev - 0x15f;
                if !self.alarm {
                    s.say(2, 15);
                    s.actor_attack(0x12d + k, 0x50, -1);
                    s.t(0x168 + k, TIMER, 3, 0, 0, 0);
                }
            }
            0x154 => {
                s.kill_mt(0x168);
                let (x, y) = PAIR_ENDS[1];
                s.t(0x130, ACTOR_GOTO, 0x12e, x, y, 0);
                self.pair_alive -= 1;
            }
            0x155 => {
                s.kill_mt(0x169);
                let (x, y) = PAIR_ENDS[0];
                s.t(0x12f, ACTOR_GOTO, 0x12d, x, y, 0);
                self.pair_alive -= 1;
            }
            0x156 => s.kill_mt(0x16a),
            0x157 => s.kill_mt(0x16b),
            0x168..=0x16b | 0x16d => self.m3_alarm(s),
            0x14a => {
                s.dialog_begin(1);
                s.say1(19);
                s.say2(20);
                s.dialog_end();
                s.say(2, 21);
                s.actor_follow(0x131, -1);
                if self.alarm {
                    attack(s, 0x12d..=0x130, 0);
                } else {
                    self.alarm = true;
                }
                s.t(0x14b, ENTER_ACTOR, 0x132, 0, 0, 0);
            }
            0x14b => {
                s.say(2, 23);
                s.t(0x14c, ACTOR_BOARDS_PLAYER, 0x131, 0, 0, 0);
            }
            0x14c => {
                release(s, 0x12d..=0x130);
                s.say(2, 24);
                s.t(0x14d, AT_POINT, px, py, 0, 0);
                s.t(0x14e, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.create_actor(0x133, -12, 0x35c00, 0x1c400, 0, 0x10e0, 0);
                s.create_actor(0x134, -12, 0x35c00, 0x1c018, 0, 0x10e0, 0);
                self.at_pad = false;
            }
            0x14d => {
                self.at_pad = true;
                s.say(2, 29);
                s.t(0x14f, PLAYER_OUTSIDE, px, py, 0, 0);
            }
            0x14f => {
                self.at_pad = false;
                s.t(0x14d, AT_POINT, px, py, 0, 0);
            }
            0x14e => {
                if self.at_pad {
                    s.say(2, 26);
                    s.ped_exits_vehicle(0x131, 0x132, -9, 1);
                    s.actor_follow(0x131, -1);
                    s.t(0x150, NEAR_ACTOR_1000, 0x134, 0, 0, 0);
                } else {
                    s.say(2, 25);
                    s.kill_mt(0x14d);
                    s.t(0x151, ENTER_ACTOR, 0x132, 0, 0, 0);
                }
            }
            0x151 => {
                s.say(2, 28);
                s.t(0x14d, AT_POINT, px, py, 0, 0);
                s.t(0x14e, VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            0x150 => {
                s.kill_all_mt();
                s.actor_follow(0x131, 0x133);
                release(s, 0x131..=0x134);
                // Quirk: both builds say 26 again; phrase 27 is never used.
                s.say(2, 26);
                s.add_money(350_000);
                s.show_message(MISSION_COMPLETE);
                self.kidnap = 1;
                self.unlock(s);
            }
            0x15e => {
                // Death leaves `kidnap` at 0: M5 is still offered.
                s.kill_all_mt();
                s.say(2, 17);
                release(s, 0x12d..=0x134);
                self.failed(s);
            }
            0x164 => {
                s.kill_all_mt();
                s.say(2, 18);
                release(s, 0x12d..=0x134);
                s.show_message(MISSION_FAILED);
                self.kidnap = -1;
                self.unlock(s);
            }
            0x165 => {
                s.kill_all_mt();
                s.say(2, 22);
                release(s, 0x12d..=0x134);
                self.failed(s);
            }
            // Original `default:` = setup (68k `0xb8be6`); it would rebuild
            // the mission on any unlisted event (none is generated).
            _ => self.m3_setup(s),
        }
    }

    fn m3_setup(&mut self, s: &mut Script) {
        // Open a gap at (56,114..115) (INFERRED: the sneak entrance).
        s.set_cube(0x38, 0x72, 1, 0x200, 0x200, 0, 0, 0);
        s.set_cube(0x38, 0x73, 1, 0x200, 0x200, 0, 0, 0);
        s.set_face(0, 0x38, 0x73, 0, 1, 0x20e, 1);
        s.set_face(0, 0x39, 0x73, 0, 1, 0x20b, 1);
        s.t(0x15e, PLAYER_DOWN, 0, 0, 0, 0);
        // Guards A and B patrol between the ends and the middle.
        let [(ax, ay), (bx, by)] = PAIR_ENDS;
        s.create_actor(0x12d, -12, ax, ay, 0, 0, 0);
        s.create_actor(0x12e, -12, bx, by, 0, 0, 0);
        pair_goto(s, 0x12d, PAIR_MIDDLE);
        s.t(0x15f, ACTOR_SEES_PLAYER_QUIET, 0x12d, 0, 0, 0);
        s.t(0x160, ACTOR_SEES_PLAYER_QUIET, 0x12e, 0, 0, 0);
        s.t(0x154, ACTOR_GONE, 0x12d, 0, 0, 0);
        s.t(0x155, ACTOR_GONE, 0x12e, 0, 0, 0);
        self.pair_alive = 2;
        self.pair_arrived = 0;
        // Guard C walks (56,109) <-> (56,112).
        s.create_actor(0x12f, -12, 0x1c400, 0x36c00, 0, 0, 0);
        s.t(0x137, ACTOR_GOTO, 0x12f, 0x1c400, 0x38400, 0);
        s.t(0x161, ACTOR_SEES_PLAYER_QUIET, 0x12f, 0, 0, 0);
        s.t(0x156, ACTOR_GONE, 0x12f, 0, 0, 0);
        // Guard D walks a square upstairs.
        s.create_actor(0x130, -12, 0x1bc00, 0x38400, 1, 0, 0);
        s.t(0x141, ACTOR_GOTO, 0x130, 0x1bc00, 0x37400, 0);
        s.t(0x162, ACTOR_SEES_PLAYER_QUIET, 0x130, 0, 0, 0);
        s.t(0x157, ACTOR_GONE, 0x130, 0, 0, 0);
        // The brother, floor 1.
        s.create_actor(0x131, -9, 0x1b400, 0x37c00, 1, 0xe10, 0);
        s.t(0x16d, ACTOR_SEES_PLAYER_QUIET, 0x131, 0, 0, 0);
        s.t(0x164, ACTOR_GONE, 0x131, 0, 0, 0);
        s.t(0x14a, NEAR_ACTOR_1000, 0x131, 0, 0, 0);
        // The helicopter on the roof.
        s.create_actor(0x132, 17, 0x1b800, 0x37800, 2, 0, 1);
        s.t(0x165, ACTOR_GONE, 0x132, 0, 0, 0);
        self.alarm = false;
    }

    /// M3 alarm (68k `0xb824e`): every guard attacks.
    fn m3_alarm(&mut self, s: &mut Script) {
        if !self.alarm {
            self.alarm = true;
            s.say(2, 16);
            attack(s, 0x12d..=0x130, 0x50);
        }
    }

    // ---- M4: taxi for the boss' wife ------------------------------------

    /// M4 phone 13 (68k `0xc6ee8`): fetch the stolen taxi at (79,62).
    fn start_m4(&mut self, s: &mut Script) {
        s.say(0, 48);
        s.create_actor(0x191, 13, 0x27c00, 0x1f400, 0, 0xb40, 1);
        s.t(0x191, ENTER_ACTOR, 0x191, 0, 0, 0);
        s.t(0x1c5, ACTOR_GONE, 0x191, 0, 0, 0);
        s.t(0x1c2, PLAYER_DOWN, 0, 0, 0, 0);
    }

    /// M4 events (68k `0xc503e`, table `0xc5056`): pick up the wife and her
    /// "instructor", drop them at the hotel and see them in.
    fn m4(&mut self, s: &mut Script, ev: i32) {
        let (hx, hy) = HOTEL_STOP;
        match ev {
            0x191 => {
                s.say(2, 50);
                s.create_actor(0x192, -10, 0xac00, 0x16400, 0, 0x5a0, 0);
                s.t(0x192, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.t(0x1c3, ACTOR_GONE, 0x192, 0, 0, 0);
                s.t(0x195, NEAR_ACTOR, 0x192, 0, 0, 0);
                self.b = 0;
            }
            0x192 => {
                s.kill_mt(0x195 + self.b);
                s.say(2, 52);
                s.t(0x193, ENTER_ACTOR, 0x191, 0, 0, 0);
            }
            0x193 => {
                s.dialog_begin(2);
                match self.b {
                    0 => {
                        s.say2(53);
                        s.t(0x195, NEAR_ACTOR, 0x192, 0, 0, 0);
                    }
                    1 => {
                        s.say2(54);
                        s.t(0x196, ACTOR_BOARDS_PLAYER, 0x192, 0, 0, 0);
                    }
                    2 => {
                        s.say2(57);
                        s.t(0x197, NEAR_ACTOR, 0x193, 0, 0, 0);
                    }
                    3 => {
                        s.say2(58);
                        s.t(0x198, ACTOR_BOARDS_PLAYER, 0x193, 0, 0, 0);
                    }
                    4 => {
                        s.say2(60);
                        s.t(0x199, STOPPED_AT_POINT, hx, hy, 0, 0);
                    }
                    _ => {}
                }
                s.dialog_end();
                s.t(0x192, VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            0x195 => {
                self.b = 1;
                s.say(2, 54);
                s.t(0x196, ACTOR_BOARDS_PLAYER, 0x192, 0, 0, 0);
            }
            0x196 => {
                self.b = 2;
                s.say(1, 55);
                s.create_actor(0x193, -7, 0x2400, 0x27400, 0, 0x5a0, 0);
                s.t(0x1c4, ACTOR_GONE, 0x193, 0, 0, 0);
                s.t(0x197, NEAR_ACTOR, 0x193, 0, 0, 0);
            }
            0x197 => {
                self.b = 3;
                s.say(1, 58);
                s.t(0x198, ACTOR_BOARDS_PLAYER, 0x193, 0, 0, 0);
            }
            0x198 => {
                self.b = 4;
                s.say(1, 59);
                s.t(0x199, STOPPED_AT_POINT, hx, hy, 0, 0);
            }
            0x199 => {
                s.kill_mt(0x192);
                s.say(1, 61);
                s.ped_exits_vehicle(0x192, 0x191, -10, 0);
                s.ped_exits_vehicle(0x193, 0x191, -7, 1);
                self.c = 0;
                s.t(0x19a, ACTOR_GOTO, 0x192, 0x9858, 0x37018, 0);
                s.t(0x19a, ACTOR_GOTO, 0x193, 0x9a4c, 0x372d0, 0);
            }
            0x19a..=0x19d => {
                let old = self.c;
                self.c += 1;
                if old != 1 {
                    return;
                }
                self.c = 0;
                let next = ev + 1;
                match ev {
                    0x19a => {
                        s.t(next, ACTOR_GOTO, 0x192, 0x9858, 0x38c00, 0);
                        s.t(next, ACTOR_GOTO, 0x193, 0x9a4c, 0x38c00, 0);
                    }
                    0x19b => {
                        s.t(next, ACTOR_GOTO, 0x192, 0x5c00, 0x38a40, 0);
                        s.t(next, ACTOR_GOTO, 0x193, 0x5c00, 0x38e28, 0);
                    }
                    0x19c => {
                        s.t(next, ACTOR_GOTO, 0x192, 0x55f0, 0x35400, 0);
                        s.t(next, ACTOR_GOTO, 0x193, 0x5800, 0x35400, 0);
                    }
                    _ => {
                        // Update 7: the instructor follows 1 s later (0x19f).
                        s.t(0x19e, ACTOR_GOTO_VANISH, 0x192, 0x4c64, 0x35400, 0);
                        s.t(0x19f, TIMER, 1, 0, 0, 0);
                    }
                }
            }
            0x19f => s.t(0x19e, ACTOR_GOTO_VANISH, 0x193, 0x4c64, 0x35400, 0),
            0x19e => {
                let old = self.c;
                self.c += 1;
                if old == 1 {
                    s.kill_all_mt();
                    s.actor_release(0x191);
                    s.say(2, 62);
                    self.win(s, 150_000);
                }
            }
            0x1c2 | 0x1c5 => {
                s.kill_all_mt();
                release(s, 0x191..=0x193);
                self.fail(s, 49);
            }
            0x1c3 => {
                s.kill_all_mt();
                release(s, 0x191..=0x193);
                self.fail(s, 51);
            }
            0x1c4 => {
                // Quirk: killing the instructor counts as success and pays
                // more than the normal ending.
                s.kill_all_mt();
                release(s, 0x191..=0x193);
                s.say(2, 56);
                self.win(s, 200_000);
            }
            _ => {}
        }
    }

    // ---- M5: ransom handover --------------------------------------------

    /// M5 phone 12 after M3 (68k `0xc6f96`): drive the Mundaneo with the
    /// brother in the boot to the car park and make the trade.
    fn start_m5(&mut self, s: &mut Script) {
        s.say(0, if self.kidnap == 1 { 30 } else { 31 });
        s.create_actor(0x1f5, 0, 0x39c00, 0x3dc00, 0, 0x10e0, 1);
        s.t(0x226, PLAYER_DOWN, 0, 0, 0, 0);
        s.t(0x227, ACTOR_GONE, 0x1f5, 0, 0, 0);
        s.t(0x1f5, ENTER_ACTOR, 0x1f5, 0, 0, 0);
        self.kidnap = -1;
    }

    /// M5 events (68k `0xc57dc`, table `0xc57f4`). 150,000 at pickup; killing
    /// all three pays another 150,000; letting them escape fails but keeps
    /// the first payment.
    fn m5(&mut self, s: &mut Script, ev: i32) {
        let (mx, my) = MEETING;
        match ev {
            0x1f5 => {
                s.say(2, 34);
                s.create_actor(0x1f6, 6, 0x21c00, 0x3d400, 0, 0xb40, 4);
                s.create_actor(0x1f7, -12, 0x21c00, 0x3c400, 0, 0x5a0, 4);
                s.create_actor(0x1f8, -12, 0x21c00, 0x3cc00, 0, 0x5a0, 4);
                s.t(0x1f6, AT_POINT, mx, my, 0, 0);
                s.t(0x1f8, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.t(0x230, ACTOR_GONE, 0x1f6, 0, 0, 0);
                s.t(0x231, ACTOR_GONE, 0x1f7, 0, 0, 0);
                s.t(0x232, ACTOR_GONE, 0x1f8, 0, 0, 0);
                self.at_meeting = false;
            }
            0x1f6 => {
                self.at_meeting = true;
                s.say(2, 35);
                s.t(0x1f7, PLAYER_OUTSIDE, mx, my, 0, 0);
            }
            0x1f7 => {
                self.at_meeting = false;
                s.say(2, 36);
                s.t(0x1f6, AT_POINT, mx, my, 0, 0);
            }
            0x1f8 => {
                if self.at_meeting {
                    s.kill_mt(0x1f7);
                    s.ped_exits_vehicle(0x1f9, 0x1f5, -9, 1);
                    s.actor_follow(0x1f9, -1);
                    s.kill_mt(0x227);
                    s.actor_release(0x1f5);
                    s.say(2, 39);
                    let (bx, by) = BRIEFCASE;
                    s.t(0x1fa, ACTOR_GOTO, 0x1f8, bx, by, 0);
                } else {
                    s.kill_mt(0x1f6);
                    s.say(2, 37);
                    s.t(0x1f9, ENTER_ACTOR, 0x1f5, 0, 0, 0);
                }
            }
            0x1f9 => {
                s.say(2, 38);
                s.t(0x1f6, AT_POINT, mx, my, 0, 0);
                s.t(0x1f8, VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            0x1fa => {
                s.set_scenery(0x45, 0x79, 0, 23, 0xb2, 0);
                s.t(0x1fb, ACTOR_GOTO, 0x1f8, 0x21c00, 0x3cc00, 0);
                s.kill_mt(0x230);
                s.kill_mt(0x231);
                s.kill_mt(0x232);
            }
            0x1fb => {
                s.say(2, 41);
                let (bx, by) = BRIEFCASE;
                s.t(0x1fc, ON_FOOT_AT_POINT, bx, by, 0, 0);
            }
            0x1fc => {
                s.say(2, 42);
                s.set_scenery(0x45, 0x79, 0, 0, 0x59, 0);
                s.actor_follow(0x1f9, -2);
                // All three run for their car (0x3d: walk to its door).
                s.t(0x1fd, ACTOR_BOARDS_ACTOR, 0x1f7, 0x1f6, 0, 0);
                s.t(0x1fe, ACTOR_BOARDS_ACTOR, 0x1f8, 0x1f6, 0, 0);
                s.t(0x1ff, ACTOR_BOARDS_ACTOR, 0x1f9, 0x1f6, 0, 0);
                s.add_money(150_000);
                s.t(0x21d, ACTOR_GONE, 0x1f6, 0, 0, 0);
                for id in 0x1f7..=0x1f9 {
                    s.t(0x21c, ACTOR_GONE, id, 0, 0, 0);
                }
                self.b = 0;
                self.c = 0;
            }
            0x1fd..=0x1ff => {
                self.b += 1;
                if self.b + self.c == 3 {
                    m5_escape(s);
                }
            }
            0x200 => s.t(0x201, ACTOR_GOTO, 0x1f6, 0x1dc00, 0x3ec00, 0),
            0x201 => s.t(0x202, ACTOR_GOTO, 0x1f6, 0x19400, 0x3ec00, 0),
            0x202 => s.t(0x203, ACTOR_GOTO, 0x1f6, 0x19400, 0x3e400, 0),
            0x203 => {
                m5_door(s, true);
                s.t(0x204, ACTOR_GOTO, 0x1f6, 0x178f4, 0x3e400, 0);
            }
            0x204 => {
                m5_door(s, false);
                s.actor_release(0x1f6);
                s.kill_all_mt();
                self.fail(s, 46);
            }
            0x21c => {
                s.actor_set_aggro(0x1f6, 100);
                s.actor_attack(0x1f7, 0x50, -1);
                s.actor_attack(0x1f8, 0x50, -1);
                self.c += 1;
                if self.b + self.c == 3 {
                    if self.b == 0 {
                        self.m5_bonus(s);
                    } else {
                        s.say(2, 71);
                        m5_escape(s);
                    }
                } else {
                    s.say(2, 44);
                }
            }
            0x21d => {
                self.c += self.b;
                self.b = 0;
                attack(s, 0x1f7..=0x1f9, 0x50);
                if self.c == 3 {
                    self.m5_bonus(s);
                } else {
                    s.say(2, 44);
                }
            }
            0x226 => {
                s.kill_all_mt();
                m5_door(s, false);
                release(s, 0x1f5..=0x1f9);
                self.fail(s, 32);
            }
            0x227 => {
                s.kill_all_mt();
                release(s, 0x1f5..=0x1f9);
                self.fail(s, 33);
            }
            0x230..=0x232 => {
                s.kill_all_mt();
                release(s, 0x1f5..=0x1f9);
                self.fail(s, 40);
            }
            _ => {}
        }
    }

    /// M5: all three dealt with.
    fn m5_bonus(&mut self, s: &mut Script) {
        s.kill_all_mt();
        m5_door(s, false);
        release(s, 0x1f6..=0x1f9);
        s.say(2, 43);
        self.win(s, 150_000);
    }

    // ---- M6: dumped car -------------------------------------------------

    /// M6 phone 14 (68k `0xc7058`): find the Vapour at (2,23) within 90 s.
    fn start_m6(&mut self, s: &mut Script) {
        s.say(0, 63);
        s.create_actor(0x259, 4, 0x1400, 0xbc00, 0, 0xb40, 1);
        s.t(0x28a, PLAYER_DOWN, 0, 0, 0, 0);
        s.t(0x28b, ACTOR_GONE, 0x259, 0, 0, 0);
        s.t(0x259, ENTER_ACTOR, 0x259, 0, 0, 0);
        s.t(0x28c, COUNTDOWN, 90, 0, 0, 0);
    }

    /// M6 events (68k `0xc6302`, table `0xc631a`): deliver the car to
    /// Kenny's garage at (63,116) with wanted level 6.
    fn m6(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x259 => {
                s.say(2, 67);
                s.set_wanted_level(6);
                s.kill_mt(0x28c);
                s.t(0x25a, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.t(0x262, GARAGE, 0x3f, 0x74, 1, 3);
            }
            0x25a => {
                s.say(2, 68);
                s.kill_mt(0x262);
                s.t(0x25b, ENTER_ACTOR, 0x259, 0, 0, 0);
            }
            0x25b => {
                s.say(2, 69);
                s.t(0x25a, VEHICLE_CHANGED, 0, 0, 0, 0);
                s.t(0x262, GARAGE, 0x3f, 0x74, 1, 3);
            }
            0x262 => {
                s.kill_all_mt();
                s.actor_release(0x259);
                s.say(2, 70);
                s.set_wanted_level(0);
                self.win(s, 225_000);
            }
            0x28a..=0x28c => {
                s.kill_all_mt();
                s.actor_release(0x259);
                self.fail(s, [64, 65, 66][(ev - 0x28a) as usize]);
            }
            _ => {}
        }
    }

    // ---- M7: love rival -------------------------------------------------

    /// M7 phone 15 (68k `0xc711e`): the rival at (74,8).
    fn start_m7(&mut self, s: &mut Script) {
        s.say(0, 72);
        s.create_actor(0x2bd, -12, 0x25400, 0x4400, 0, 0, 1);
        s.t(0x2bd, NEAR_ACTOR, 0x2bd, 0, 0, 0);
        s.t(0x2d0, ACTOR_KILLED, 0x2bd, 0, 0, 0);
        s.t(0x2ee, PLAYER_DOWN, 0, 0, 0, 0);
    }

    /// M7 events (68k `0xc6526`, table `0xc653e`): he shoots for 2 s, runs
    /// on, three times, then vanishes into (71,13).
    fn m7(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x2bd => {
                s.say(2, 73);
                s.actor_attack(0x2bd, 0x5a, -1);
                s.t(0x2be, TIMER, 2, 0, 0, 0);
            }
            0x2be => {
                s.actor_attack(0x2bd, 0, 0);
                s.t(0x2bf, ACTOR_GOTO, 0x2bd, 0x25400, 0x7400, 0);
            }
            0x2bf => {
                s.actor_attack(0x2bd, 0x5a, -1);
                s.t(0x2c0, TIMER, 2, 0, 0, 0);
            }
            0x2c0 => {
                s.actor_attack(0x2bd, 0, 0);
                s.t(0x2c1, ACTOR_GOTO, 0x2bd, 0x23c00, 0x7400, 0);
            }
            0x2c1 => {
                s.actor_attack(0x2bd, 0x5a, -1);
                s.t(0x2c2, TIMER, 2, 0, 0, 0);
            }
            0x2c2 => {
                s.actor_attack(0x2bd, 0, 0);
                s.t(0x2c3, ACTOR_GOTO_VANISH, 0x2bd, 0x23c00, 0x6c00, 0);
            }
            0x2c3 => {
                s.kill_all_mt();
                s.actor_release(0x2bd);
                self.fail(s, 74);
            }
            0x2d0 => {
                s.kill_all_mt();
                s.actor_release(0x2bd);
                s.say(2, 76);
                s.set_wanted_level(0);
                self.win(s, 200_000);
            }
            0x2ee => {
                s.kill_all_mt();
                s.actor_release(0x2bd);
                self.fail(s, 75);
            }
            _ => {}
        }
    }

    // ---- M8: rival gang -------------------------------------------------

    /// M8 phone 16 (68k `0xc71c2`): four gangsters at (17..19,77..78).
    fn start_m8(&mut self, s: &mut Script) {
        s.say(0, 77);
        s.create_actor(0x321, -12, 0x9c00, 0x26c00, 0, 0x870, 1);
        s.create_actor(0x322, -12, 0x9c00, 0x27400, 0, 0x5a0, 1);
        s.create_actor(0x323, -12, 0x8c00, 0x26c00, 0, 0xe10, 1);
        s.create_actor(0x324, -12, 0x8c00, 0x27400, 0, 0x10e0, 1);
        for id in 0x321..=0x324 {
            s.t(0x33e, ACTOR_KILLED, id, 0, 0, 0);
        }
        for id in 0x321..=0x324 {
            s.t(0x321, ACTOR_NEAR_PLAYER, id, 0, 0, 0);
        }
        s.t(0x352, PLAYER_DOWN, 0, 0, 0, 0);
        self.b = 0;
        self.c = 4;
    }

    /// M8 events (68k `0xc6790`, table `0xc67a8`): once alerted, each
    /// gangster alternates attacking and fleeing every 1-4 s.
    fn m8(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x321 => {
                if self.b == 0 {
                    self.m8_alert(s);
                }
            }
            0x32b..=0x32e => {
                let k = ev - 0x32b;
                s.actor_attack(0x321 + k, -1, -1);
                let secs = s.random(4) + 1;
                s.t(0x335 + k, TIMER, secs, 0, 0, 0);
            }
            0x335..=0x338 => {
                let k = ev - 0x335;
                s.actor_attack(0x321 + k, 0x50, -1);
                let secs = s.random(4) + 1;
                s.t(0x32b + k, TIMER, secs, 0, 0, 0);
            }
            0x33e => {
                if self.b == 0 {
                    self.m8_alert(s);
                }
                let old = self.c;
                self.c -= 1;
                if old == 1 {
                    s.kill_all_mt();
                    release(s, 0x321..=0x324);
                    s.say(2, 80);
                    self.win(s, 250_000);
                } else {
                    s.say(2, 79);
                }
            }
            0x352 => {
                s.kill_all_mt();
                release(s, 0x321..=0x324);
                self.fail(s, 78);
            }
            _ => {}
        }
    }

    fn m8_alert(&mut self, s: &mut Script) {
        s.say(2, 81);
        self.b = 1;
        attack(s, 0x321..=0x324, 0x50);
        for k in 0..4 {
            let secs = s.random(4) + 1;
            s.t(0x32b + k, TIMER, secs, 0, 0, 0);
        }
    }

    // ---- M9: RC car on the boss' wife -----------------------------------

    /// M9 helper (68k `0xb8f4e`, phone `0xc7392`): from the RC truck at
    /// (11,125), blow up the wife's Hot Rod circling (72..86,59..71).
    fn m9(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0x385 => {
                s.say(2, 83);
                self.wife_wp = 1;
                s.create_actor(0x386, 16, 0x2b400, 0x21c00, 0, 0x10e0, 6);
                wife_goto(s, 1);
                s.t(0x3a2, ACTOR_KILLED, 0x386, 0, 0, 0);
            }
            0x386 => {
                self.wife_wp = if self.wife_wp + 1 > 5 {
                    0
                } else {
                    self.wife_wp + 1
                };
                wife_goto(s, self.wife_wp);
            }
            0x3a2 => {
                s.kill_all_mt();
                s.say(2, 84);
                s.actor_release(0x385);
                s.actor_release(0x386);
                self.win(s, 175_000);
            }
            0x3b6 => {
                s.kill_all_mt();
                s.say(2, 17);
                s.actor_release(0x385);
                s.actor_release(0x386);
                self.failed(s);
            }
            // Original `default:` = setup; it would rebuild the mission on
            // any unlisted event (none is generated in practice).
            _ => {
                s.say(0, 82);
                s.create_actor(0x385, 21, 0x5c00, 0x3ec00, 0, 0x5a0, 1);
                s.t(0x385, ENTER_ACTOR, 0x385, 0, 0, 0);
                s.t(0x3b6, ACTOR_GONE, 0x385, 0, 0, 0);
                s.t(0x3b6, PLAYER_DOWN, 0, 0, 0, 0);
            }
        }
    }
}

/// M2: send the bus to waypoint `wp`.
fn bus_goto(s: &mut Script, wp: usize) {
    let (x, y) = BUS_ROUTE[wp];
    s.t(0xc9, ACTOR_GOTO, 0xc9, sq(x), sq(y), 0);
}

/// M3: send guards A (0x12d) and B (0x12e) to `points`, with events `ev`
/// and `ev + 1`.
fn pair_goto(s: &mut Script, ev: i32, points: [(i32, i32); 2]) {
    for (k, (x, y)) in (0..).zip(points) {
        s.t(ev + k, ACTOR_GOTO, 0x12d + k, x, y, 0);
    }
}

/// M5: the gangsters' car drives off towards the garage at (48,124).
fn m5_escape(s: &mut Script) {
    s.t(0x200, ACTOR_GOTO, 0x1f6, 0x21c00, 0x3ec00, 0);
}

/// M9: send the wife's car to waypoint `wp`.
fn wife_goto(s: &mut Script, wp: usize) {
    let (x, y) = WIFE_ROUTE[wp];
    s.t(0x386, ACTOR_GOTO, 0x386, sq(x), sq(y), 0);
}
