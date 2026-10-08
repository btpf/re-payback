//! Level 21 (city 2, level 1, "Born Slippy") mission script
//! (docs/re/missions_21.md).
//!
//! Update 7 68k: init `0xb9aaa`, event handler `0xc79cc`, unlock routine
//! `0xb538e`, race helper `0xb6e48` (OS4 `0x010ebbe8` / `0x010df934`).
//! Mission 1 starts by getting into car 50; finishing it opens phones for
//! M2, M3 and M6, and finishing those three opens M4, M5, M7 and M8. Every
//! mission runs once, pass or fail. The level ends at 2,000,000 points.

use crate::mission::api::{MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::{LevelScript, MissionState, Script, kind};

/// Global phrase "Kill frenzy!".
const KILL_FRENZY: u32 = 2029;

/// Race AI waypoints (`0xb6e48` tables): x, y, arrival radius of the type
/// 0x32 trigger, and the `+0x386` speed cap (minus 30 per grid slot).
pub(crate) const RACE_WAYPOINTS: [(i32, i32, i32, i32); 13] = [
    (0x2000, 0x30c00, 3000, 550), // (4,97)
    (0x2400, 0x34400, 2500, 300), // (4,104)
    (0x6800, 0x33800, 2500, 300), // (13,103)
    (0x6800, 0x2d000, 2000, 400), // (13,90)
    (0x6400, 0x2a400, 2500, 250), // (12,84)
    (0x1400, 0x29400, 1500, 250), // (2,82)
    (0x1c00, 0x27c00, 1500, 200), // (3,79)
    (0x5400, 0x27c00, 1500, 300), // (10,79)
    (0x6400, 0x26c00, 1500, 250), // (12,77)
    (0x5400, 0x23c00, 1500, 300), // (10,71)
    (0x6c00, 0x22400, 1500, 300), // (13,68)
    (0x6200, 0x20c00, 2000, 300), // (12,65)
    (0x1c00, 0x21400, 2000, 450), // (3,66)
];

/// Race checkpoints, armed one at a time per car: actor-coordinate trigger
/// type and its threshold. The last one is the finish line.
const RACE_CHECKPOINTS: [(u8, i32); 5] = [
    (kind::ACTOR_Y_GT, 0x33000),
    (kind::ACTOR_X_GT, 0x5000),
    (kind::ACTOR_Y_LE, 0x21800),
    (kind::ACTOR_X_LE, 0x3000),
    (kind::ACTOR_Y_GT, 0x24c00),
];

/// AI racers 11..15 on the grid (M3 ev 1 `0xc83ac`): x, y, colour.
const RACE_GRID: [(i32, i32, i32); 5] = [
    (0x2400, 0x23f32, 2), // (4,71)
    (0x1c00, 0x23730, 3), // (3,70)
    (0x2400, 0x22f38, 4), // (4,69)
    (0x1c00, 0x2272c, 5), // (3,68)
    (0x2400, 0x21f34, 6), // (4,67)
];

/// Aggression of AI racers 11..15.
const RACE_AGGRO: [i32; 5] = [100, 100, 100, 99, 99];

/// Laps per race.
const RACE_LAPS: i32 = 3;

/// M5 gang pedestrians 510..513: x, y, angle.
const GANG: [(i32, i32, i32); 4] = [
    (0x12c00, 0x2f400, 0xb40), // (37,94)
    (0x13400, 0x2f400, 0xb40), // (38,94)
    (0x12c00, 0x2cc00, 0),     // (37,89)
    (0x13400, 0x2cc00, 0),     // (38,89)
];

/// M6 robbers' van escape route: ev 616 + i arms ev 617 + i to this point.
const VAN_ROUTE: [(i32, i32); 8] = [
    (0x37400, 0x3e400), // (110,124)
    (0x37400, 0x3b400), // (110,118)
    (0x34400, 0x3b400), // (104,118)
    (0x34400, 0x2a400), // (104,84)
    (0x38c00, 0x2a400), // (113,84)
    (0x38c00, 0x25400), // (113,74)
    (0x3cc00, 0x25400), // (121,74)
    (0x3cc00, 0x21c00), // (121,67)
];

/// Pedestrian model of Nasty Nick and the van driver (`~-16` = 15).
const PED_NICK: i32 = -16;
/// Pedestrian model of the M5 gang (`~-12` = 11).
const PED_GANG: i32 = -12;

/// `SetFace*` face indices.
const FACE_TOP: usize = 0;
const FACE_E: usize = 2;
const FACE_W: usize = 4;

/// Bomb truck target: the rival garage at (123,67), floor 0.
const GARAGE_X: i32 = 0x3dc00;
const GARAGE_Y: i32 = 0x21c00;

/// Level 21 globals (68k `0x2fba2c..0x2fba50`, race state `0x2fb068..`).
#[derive(Default)]
pub struct Level21 {
    /// `0x2fba4c`: M1 bomb armed (kidnapper killed after giving the
    /// location), M4 van alerted, M5 gang attack started.
    alert: bool,
    /// `0x2fba44`: M5 first vehicle entry seen, M7 truck inside the zone.
    seen: bool,
    /// `0x2fba50`: M5 Nick in the player's car, M7 player in the truck.
    aboard: bool,
    /// `0x2fba48`: M5 gang members remaining.
    gang_left: i32,
    /// `0x2fba30..0x2fba3c`: M5 alive flags of gang peds 510..513.
    gang_alive: [bool; 4],
    /// `0x2fb068[5]`: waypoint index per AI racer.
    race_wp: [usize; 5],
    /// `0x2fb080[6]`: checkpoint progress per car (0 = the player's car 10).
    race_prog: [usize; 6],
    /// `0x2fb098[6]`: current lap per car.
    race_lap: [i32; 6],
    /// `0x2fb0b0[6]`: car has finished.
    race_done: [bool; 6],
}

impl LevelScript for Level21 {
    /// Level init `0xb9aaa`.
    fn init(&mut self, s: &mut Script) {
        // The first-phones flag is cleared before RefreshPhones, so the
        // generic intro phrase is never shown; phrase 1 is said directly.
        s.world.resource_mut::<MissionState>().first_phones = false;
        s.add_trigger(-1, kind::MONEY_GOAL, 2_000_000, 0, 0, 0);
        s.refresh_phones(); // all slots empty: adds nothing, says nothing
        s.create_actor(50, 0, 0x26400, 0x17000, 0, 0, 4); // (76,46)
        s.t(100, kind::ENTER_ACTOR, 50, 0, 0, 0);
        // Mac only (`InitScript_21` @ 10129d20): car 50 destroyed before
        // the player got in fails mission 1 instead of soft-locking the
        // level (68k `0xb9aaa` has no such trigger).
        s.t(50, kind::ACTOR_GONE, 50, 0, 0, 0);
        // Never referenced again: a static parked vehicle at (60,59).
        s.create_actor(1_000_000, 23, 0x1e2d0, 0x1db64, 0, 0x10e0, 0);
        s.show_message(0);
        s.say(2, 1);
    }

    /// Event handler `0xc79cc`: dispatch on the current mission slot.
    fn event(&mut self, s: &mut Script, ev: i32) {
        if ev == 0 {
            return;
        }
        match s.cur_slot() {
            1 => self.kidnapping(s, ev),
            2 => rocket_car(s, ev),
            3 => self.race_event(s, ev),
            4 => self.surveillance(s, ev),
            5 => self.nasty_nick(s, ev),
            6 => bank_van(s, ev),
            7 => self.bomb_truck(s, ev),
            8 => grenades(s, ev),
            _ => self.start(s, ev),
        }
    }
}

/// Unlock routine `0xb538e`: empty the finished slot (success or failure),
/// open the next phones, ring them and clear the current slot.
fn unlock(s: &mut Script) {
    let slot = s.cur_slot();
    s.consume_slot();
    if slot == 1 {
        s.set_phone(2, 200, 1, false);
        s.set_phone(3, 300, 2, false);
        s.set_phone(6, 600, 3, false);
    } else if matches!(slot, 2 | 3 | 6) && [2, 3, 6].iter().all(|&i| s.phone_event(i) == -1) {
        s.set_phone(4, 400, 5, false);
        s.set_phone(5, 500, 6, false);
        s.set_phone(7, 700, 7, false);
        s.set_phone(8, 800, 8, false);
    }
    s.refresh_phones();
    s.set_cur_slot(-1);
}

/// Common fail tail `0xc9c02`.
fn fail(s: &mut Script, phrase: u32) {
    s.say(2, phrase);
    s.show_message(MISSION_FAILED);
    unlock(s);
}

/// Common success path: the boss's line, the reward (times the multiplier
/// the mission ran at) and the tail `0xc9c12`.
fn win(s: &mut Script, phrase: u32, reward: i32) {
    s.say(2, phrase);
    s.add_money(reward);
    s.show_message(MISSION_COMPLETE);
    unlock(s);
}

/// Begin(speaker); Dialog_Say1(phrase); End: the player's own line.
fn say_self(s: &mut Script, speaker: i32, phrase: u32) {
    s.dialog_begin(speaker);
    s.say1(phrase);
    s.dialog_end();
}

impl Level21 {
    /// Mission starts `0xc9c22` (no mission running).
    fn start(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // M1 (`0xc9c76`): the player got into car 50.
            100 => {
                s.actor_release(50);
                s.say(2, 2);
                s.set_cur_slot(1);
                s.t(101, kind::PHONE, 4, 0, 0, 0);
                s.t(150, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(151, kind::COUNTDOWN, 30, 0, 0, 0);
            }
            // Car 50 destroyed before mission 1 (Mac `ProcessTrigger_21`
            // @ 1013b2b0): "You idiot!", mission failed, and the phones
            // of mission 1's unlock open. After ev 100 the trigger can't
            // fire: `Actor_Release(50)` cleared the id it watches.
            50 => {
                s.actor_release(50);
                s.set_cur_slot(1);
                s.kill_all_mt();
                s.say(2, 5);
                s.show_message(MISSION_FAILED);
                unlock(s);
            }
            // M2 rocket car (`0xc9d00`).
            200 => {
                s.kill_all_phones();
                s.say(0, 19);
                s.create_actor(201, 24, 0x1400, 0x3ec00, 0, 0x5a0, 4); // (2,125)
                s.set_cur_slot(2);
                s.t(201, kind::ENTER_ACTOR, 201, 0, 0, 0);
                s.t(250, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(252, kind::ACTOR_GONE, 201, 0, 0, 0);
            }
            // M3 race (`0xc9dac`).
            300 => {
                s.kill_all_phones();
                s.say(0, 28);
                s.create_actor(10, 27, 0x1c00, 0x21728, 0, 0, 1); // (3,66)
                s.actor_set_controls(10, false); // held until "Go!"
                s.t(1, kind::ENTER_ACTOR, 10, 0, 0, 0);
                s.t(10, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(10, kind::ACTOR_GONE, 10, 0, 0, 0);
                s.set_cur_slot(3);
            }
            // M4 police surveillance van (`0xc9e60`).
            400 => {
                s.kill_all_phones();
                s.say(0, 40);
                s.create_actor(400, 7, 0x3bc00, 0x2134, 0, 0, 6); // (119,4)
                s.actor_set_controls(400, false); // parked
                s.t(401, kind::ACTOR_GONE, 400, 0, 0, 0);
                s.t(420, kind::ACTOR_ATTACKED, 400, 0, 0, 0);
                s.t(421, kind::ACTOR_SEES_PLAYER, 400, 0, 0, 0);
                s.t(450, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(4);
                self.alert = false;
            }
            // M5 Nasty Nick (`0xc9f30`).
            500 => {
                s.kill_all_phones();
                s.say(0, 46);
                s.create_actor(500, PED_NICK, 0x29400, 0x2a400, 0, 0, 6); // (82,84)
                s.t(501, kind::NEAR_ACTOR, 500, 0, 0, 0);
                s.t(550, kind::ACTOR_GONE, 500, 0, 0, 0);
                s.t(551, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.set_cur_slot(5);
            }
            // M6 bank-robbery van (`0xc9fd0`).
            600 => {
                s.kill_all_phones();
                s.say(0, 57);
                s.create_actor(600, 7, 0x2f400, 0x3e400, 0, 0x5a0, 7); // (94,124)
                s.t(650, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(651, kind::ACTOR_GONE, 600, 0, 0, 0);
                s.t(601, kind::ENTER_ACTOR, 600, 0, 0, 0);
                s.t(616, kind::TIMER, 10, 0, 0, 0);
                s.actor_set_aggro(600, 100);
                s.set_cur_slot(6);
            }
            // M7 bomb truck (`0xca0a4`).
            700 => {
                s.kill_all_phones();
                s.say(0, 66);
                s.create_actor(700, 19, 0x1f400, 0x36400, 0, 0xb40, 7); // (62,108)
                s.t(750, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(751, kind::ACTOR_GONE, 700, 0, 0, 0);
                s.t(701, kind::ENTER_ACTOR, 700, 0, 0, 0);
                s.set_cur_slot(7);
            }
            // M8 grenade kill frenzy (`0xca14c`): 12 crates on a roof.
            800 => {
                s.kill_all_phones();
                s.say(0, 80);
                let mut e = 801;
                for x in 55..=58 {
                    for y in 96..=98 {
                        s.set_scenery(x, y, 1, 6, 0x2c, 3);
                        s.t(e, kind::SCENERY_GONE, x, y, 1, 0);
                        e += 1;
                    }
                }
                s.show_checkpoint_marker(true);
                s.t(850, kind::PLAYER_DOWN, 0, 0, 0, 0);
                s.t(830, kind::COUNTDOWN, 10, 0, 0, 0);
                s.set_cur_slot(8);
            }
            _ => {}
        }
    }

    /// M1 "The kidnapping" (slot 1, `0xc7a06..0xc80fe`).
    fn kidnapping(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Phone 4 answered.
            101 => {
                s.kill_mt(151);
                s.say(0, 6);
                s.set_scenery(86, 43, 0, 23, 0x85, 0); // briefcase
                s.t(102, kind::ON_FOOT_AT_POINT, 0x2b400, 0x15c00, 0, 0);
                self.alert = false;
            }
            // Picked up the briefcase.
            102 => {
                s.set_scenery(86, 43, 0, 0, 0x59, 0);
                s.say(2, 4);
                s.t(103, kind::ON_FOOT_AT_POINT, 0x1dc00, 0x10c00, 0, 0);
            }
            // Case left at the drop point (59,33); the kidnapper comes.
            103 => {
                s.set_scenery(59, 33, 0, 23, 0x9c, 0);
                s.say(2, 7);
                s.create_actor(101, 23, 0x17800, 0x9400, 0, 0x5a0, 0); // (47,18)
                s.t(104, kind::ACTOR_GOTO, 101, 0x1c460, 0xc400, 0);
                s.t(152, kind::ACTOR_GONE, 101, 0, 0, 0);
            }
            104 => s.t(105, kind::ACTOR_GOTO, 101, 0x1c460, 0xc400, 0),
            105 => s.t(106, kind::ACTOR_GOTO, 101, 0x1d400, 0x10c00, 0),
            // The kidnapper collected the case and gives the location.
            106 => {
                s.set_scenery(59, 33, 0, 0, 0x59, 0);
                s.kill_mt(152);
                s.t(120, kind::ACTOR_GONE, 101, 0, 0, 0);
                s.create_actor(102, 6, 0x19c00, 0x27400, 0, 0x10e0, 1); // (51,78)
                s.t(154, kind::ACTOR_GONE, 102, 0, 0, 0);
                s.t(107, kind::ENTER_ACTOR, 102, 0, 0, 0);
                s.say(1, 12);
                s.t(130, kind::ACTOR_GOTO, 101, 0x18400, 0x17400, 0);
            }
            // He escaped and becomes traffic.
            130 => s.actor_release(101),
            // Killed him after he gave the location: bomb clock.
            120 => {
                s.say(2, 9);
                s.t(153, kind::COUNTDOWN, 60, 0, 0, 0);
                self.alert = true;
            }
            // Got into the Fjord.
            107 => {
                if self.alert {
                    say_self(s, 1, 13);
                } else {
                    s.say(2, 14);
                }
                s.t(108, kind::GARAGE, 28, 110, 1, 1);
                s.t(109, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
            }
            109 => {
                s.kill_mt(108);
                s.t(110, kind::ENTER_ACTOR, 102, 0, 0, 0);
                s.say(2, 15);
            }
            110 => {
                s.t(108, kind::GARAGE, 28, 110, 1, 1);
                s.t(109, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
                s.say(2, 16);
            }
            // Delivered into the warehouse.
            108 => {
                // Id 100 is never used on this level (harmless).
                for id in 100..=102 {
                    s.actor_release(id);
                }
                s.kill_all_mt(); // also stops the bomb clock
                if self.alert {
                    win(s, 17, 300_000);
                } else {
                    win(s, 18, 150_000);
                }
            }
            150 => kidnapping_fail(s, 5),
            151 => kidnapping_fail(s, 3),
            152 => kidnapping_fail(s, 8),
            153 => {
                s.actor_kill(102);
                kidnapping_fail(s, 10);
            }
            154 => kidnapping_fail(s, 11),
            _ => {}
        }
    }

    /// M3 "The race" (slot 3, `0xc8384..0xc8642`): start sequence; every
    /// other event goes to the race helper.
    fn race_event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // In the car: the AI racers line up.
            1 => {
                s.say(2, 29);
                for (i, &(x, y, colour)) in RACE_GRID.iter().enumerate() {
                    s.create_actor(11 + i as i32, 27, x, y, 0, 0, colour);
                }
                for (i, &aggro) in RACE_AGGRO.iter().enumerate() {
                    s.actor_set_aggro(11 + i as i32, aggro);
                }
                s.set_race_lights(0);
                s.t(2, kind::TIMER, 2, 0, 0, 0);
            }
            2 => {
                s.set_race_lights(1);
                s.t(3, kind::TIMER, 1, 0, 0, 0);
            }
            3 => {
                s.set_race_lights(2);
                s.t(4, kind::TIMER, 1, 0, 0, 0);
            }
            4 => {
                s.say(2, 30);
                s.set_race_lights(3);
                s.actor_set_controls(10, true);
                self.race(s, -1);
                s.t(5, kind::TIMER, 1, 0, 0, 0);
            }
            5 => {
                s.set_race_lights(-1);
                s.set_lap(1, RACE_LAPS);
            }
            // Player dead or car 10 destroyed.
            10 => {
                s.kill_all_mt();
                s.set_race_lights(-1);
                s.set_lap(-1, RACE_LAPS);
                for id in 10..=15 {
                    s.actor_release(id);
                }
                fail(s, 37);
            }
            // Includes ev -1 (money goal reached mid-race), which restarts
            // the race triggers just as in the original; the level is
            // complete at that point anyway.
            _ => self.race(s, ev),
        }
    }

    /// `Race21(ev)` `0xb6e48`: -1 starts the race, 10000+r = AI racer r
    /// reached its waypoint, 20000+c = car c passed its next checkpoint.
    fn race(&mut self, s: &mut Script, ev: i32) {
        match ev {
            -1 => {
                for r in 0..5 {
                    self.race_wp[r] = 0;
                    arm_waypoint(s, r, 0);
                }
                for c in 0..6 {
                    self.race_prog[c] = 0;
                    self.race_lap[c] = 1;
                    self.race_done[c] = false;
                    arm_checkpoint(s, c, 0);
                }
            }
            10000..=10004 => {
                let r = (ev - 10000) as usize;
                self.race_wp[r] = (self.race_wp[r] + 1) % RACE_WAYPOINTS.len();
                arm_waypoint(s, r, self.race_wp[r]);
            }
            20000..=20005 => {
                let c = (ev - 20000) as usize;
                self.race_prog[c] += 1;
                if self.race_prog[c] >= RACE_CHECKPOINTS.len() {
                    self.race_prog[c] = 0;
                    self.race_lap[c] += 1;
                    if self.race_lap[c] <= RACE_LAPS {
                        if c == 0 {
                            s.set_lap(self.race_lap[0], RACE_LAPS);
                            // 38 "2 laps to go.", 39 "1 lap to go..."
                            s.say(2, 36 + self.race_lap[0] as u32);
                        }
                    } else {
                        self.race_done[c] = true;
                        if c == 0 {
                            self.race_finished(s);
                        }
                    }
                }
                if !self.race_done[c] && s.cur_slot() != -1 {
                    arm_checkpoint(s, c, self.race_prog[c]);
                }
            }
            _ => {}
        }
    }

    /// The player completed three laps: place = AI racers already home.
    /// 1st-3rd pay 255,000 / 170,000 / 85,000; 4th-6th fail.
    fn race_finished(&mut self, s: &mut Script) {
        s.kill_all_mt();
        for id in 10..=15 {
            s.actor_release(id);
        }
        let ahead = self.race_done[1..].iter().filter(|&&d| d).count() as i32;
        s.set_lap(-1, RACE_LAPS);
        s.say(2, 31 + ahead as u32);
        if ahead < 3 {
            s.add_money((3 - ahead) * 85_000);
            s.show_message(MISSION_COMPLETE);
        } else {
            s.show_message(MISSION_FAILED);
        }
        unlock(s);
    }

    /// M4 "Police surveillance van" (slot 4, `0xc8642..0xc87f2`).
    fn surveillance(&mut self, s: &mut Script, ev: i32) {
        match ev {
            420 => self.van_alerted(s, 421, 42),
            421 => self.van_alerted(s, 420, 43),
            401 => {
                s.kill_all_mt();
                s.actor_release(400);
                s.set_wanted_level(6);
                win(s, 41, 200_000);
            }
            450 | 451 => {
                s.kill_all_mt();
                s.actor_release(400);
                fail(s, if ev == 450 { 44 } else { 45 });
            }
            _ => {}
        }
    }

    /// M4 `0xc8708` / `0xc873e`: the van was attacked or saw the player
    /// (once): it flees and the player must catch it.
    fn van_alerted(&mut self, s: &mut Script, other: i32, phrase: u32) {
        if self.alert {
            return;
        }
        self.alert = true;
        s.set_wanted_level(6);
        s.kill_mt(other);
        s.say(2, phrase);
        s.actor_set_aggro(400, 100);
        s.t(451, kind::ACTOR_FLEES, 400, 0, 0, 0);
    }

    /// M5 "Nasty Nick escorts the mayor" (slot 5, `0xc87f2..0xc9080`).
    fn nasty_nick(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Gang ped ev-10 destroyed (`0xc87f2`); only 520..523 are armed.
            520..=529 => {
                self.gang_left -= 1;
                // 56 "That's one less scumbag..." .. 53 "they're all dead!"
                s.say(2, (53 + self.gang_left) as u32);
                if self.gang_left > 0 && !self.aboard && self.alert {
                    s.actor_attack(500, 90, self.first_alive_gang());
                }
                s.actor_release(ev - 10);
            }
            // Reached Nick: he follows; the limo and the gang appear.
            501 => {
                say_self(s, 2, 49);
                s.actor_follow(500, -1);
                s.t(510, kind::IN_ANY_VEHICLE, 0, 0, 0, 0);
                self.aboard = false;
                s.create_actor(502, 8, 0x19c00, 0x3d400, 0, 0xb40, 1); // (51,122)
                s.t(552, kind::ACTOR_KILLED, 502, 0, 0, 0);
                s.t(540, kind::TIMER, 10, 0, 0, 0);
                for (i, &(x, y, angle)) in GANG.iter().enumerate() {
                    s.create_actor(510 + i as i32, PED_GANG, x, y, 0, angle, 1);
                }
                for i in 0..GANG.len() {
                    self.gang_alive[i] = true;
                    s.t(520 + i as i32, kind::ACTOR_GONE, 510 + i as i32, 0, 0, 0);
                }
                self.gang_left = 4;
                self.alert = false;
                self.seen = false;
            }
            // The limo's route: (51,92), (39,92) by the gang, (31,92), (31,110).
            540 => s.t(502, kind::ACTOR_GOTO, 502, 0x19c00, 0x2e400, 0),
            502 => s.t(503, kind::ACTOR_GOTO, 502, 0x13c00, 0x2e400, 0),
            503 => {
                s.t(504, kind::ACTOR_GOTO, 502, 0xfc00, 0x2e400, 0);
                for id in 510..=513 {
                    s.actor_attack(id, 90, 502);
                }
                self.alert = true;
                if !self.aboard {
                    s.actor_attack(500, 90, 510);
                }
            }
            504 => {
                s.t(505, kind::ACTOR_GOTO, 502, 0xfc00, 0x37400, 0);
                s.set_face(FACE_E, 28, 110, 1, 0, 0, 0); // open the warehouse door
            }
            // Player got into a vehicle.
            510 => {
                s.t(511, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
                s.t(512, kind::ACTOR_BOARDS_PLAYER, 500, 0, 0, 0);
                // Quirk: trigger 510 is never re-armed, so "Wait for Nick."
                // can never be shown and Nick boards only once.
                if self.seen {
                    say_self(s, 2, 50);
                } else {
                    self.seen = true;
                }
            }
            // Nick boarded: tag the player's car 501; he can't die inside.
            512 => {
                s.set_player_id(501);
                s.kill_mt(550);
                self.aboard = true;
            }
            // Player left the vehicle.
            511 => {
                if self.aboard {
                    s.ped_exits_vehicle(500, 501, PED_NICK, 1);
                    s.actor_release(501);
                    s.t(550, kind::ACTOR_GONE, 500, 0, 0, 0);
                    self.aboard = false;
                    if self.gang_left > 0 && self.alert {
                        s.actor_attack(500, 90, self.first_alive_gang());
                    }
                } else {
                    s.kill_mt(512);
                }
            }
            // The limo reached the warehouse.
            505 => {
                s.kill_all_mt();
                s.set_face(FACE_E, 28, 110, 1, 1, 0x76, 1); // close the door
                if self.aboard {
                    s.ped_exits_vehicle(500, 501, PED_NICK, 1);
                    s.actor_release(501);
                }
                s.actor_follow(500, -2);
                // Ev 560 has no handler; Actor_Release right after it also
                // overrides the walk-in order, as in the original.
                s.t(560, kind::ACTOR_GOTO_VANISH, 500, 0xe420, 0x37400, 0);
                s.actor_release(500);
                s.actor_release(502);
                for id in 510..=513 {
                    s.actor_release(id);
                }
                win(s, 52, 200_000);
            }
            550 => self.nasty_nick_fail(s, 48),
            551 => self.nasty_nick_fail(s, 47),
            552 => self.nasty_nick_fail(s, 51),
            _ => {}
        }
    }

    /// First gang ped whose alive flag is set. Quirk (68k and OS4): the
    /// flags are never cleared, so this is always 510, even when dead.
    fn first_alive_gang(&self) -> i32 {
        self.gang_alive
            .iter()
            .position(|&a| a)
            .map_or(-1, |i| 510 + i as i32)
    }

    /// M5 failure tail (`0xc8e46..0xc9080`).
    fn nasty_nick_fail(&mut self, s: &mut Script, phrase: u32) {
        s.kill_all_mt();
        s.set_face(FACE_E, 28, 110, 1, 1, 0x76, 1);
        s.actor_follow(500, -2);
        s.actor_release(500);
        if self.aboard {
            s.actor_release(501);
        }
        s.actor_release(502);
        for id in 510..=513 {
            s.actor_release(id);
        }
        fail(s, phrase);
    }

    /// M7 "The bomb truck" (slot 7, `0xc958a..0xc9a6a`).
    fn bomb_truck(&mut self, s: &mut Script, ev: i32) {
        match ev {
            701 => {
                s.say(2, 69);
                s.t(740, kind::COUNTDOWN, 50, 0, 0, 0);
                s.t(702, kind::AT_POINT, GARAGE_X, GARAGE_Y, 0, 0);
                s.t(710, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
                self.seen = false;
                self.aboard = true;
            }
            // Reached the garage.
            702 => {
                self.seen = true;
                if self.aboard {
                    s.say(2, 74);
                    s.t(703, kind::PLAYER_OUTSIDE, GARAGE_X, GARAGE_Y, 0, 0);
                }
            }
            // Drove out of the zone again.
            703 => {
                self.seen = false;
                s.say(2, 75);
                s.t(702, kind::AT_POINT, GARAGE_X, GARAGE_Y, 0, 0);
            }
            // Left the truck.
            710 => {
                self.aboard = false;
                if self.seen {
                    s.kill_mt(703);
                    s.say(2, 79);
                } else {
                    s.kill_mt(702);
                    s.say(2, 70);
                    s.t(711, kind::ENTER_ACTOR, 700, 0, 0, 0);
                }
            }
            // Back in. Quirk: 710 is not re-armed, so a later bailout
            // still counts as "in the truck" (phrase 77 instead of 76).
            711 => {
                self.aboard = true;
                if self.seen {
                    s.say(2, 72);
                    s.t(703, kind::PLAYER_OUTSIDE, GARAGE_X, GARAGE_Y, 0, 0);
                } else {
                    s.say(2, 71);
                    s.t(702, kind::AT_POINT, GARAGE_X, GARAGE_Y, 0, 0);
                }
            }
            740 => self.truck_explodes(s),
            750 | 751 => {
                s.kill_all_mt();
                s.actor_release(700);
                fail(s, if ev == 750 { 67 } else { 68 });
            }
            _ => {}
        }
    }

    /// M7 ev 740 `0xc97d4`: the timer ran out and the truck blows up.
    fn truck_explodes(&mut self, s: &mut Script) {
        s.kill_all_mt();
        s.actor_kill(700);
        s.actor_release(700);
        if !self.seen {
            s.kill_all_mt();
            s.say(2, 78);
            s.show_message(MISSION_FAILED);
            unlock(s);
            return;
        }
        // The rival garage at (124..127, 64..68) is flattened.
        for x in 124..=126 {
            for y in 64..=68 {
                s.set_cube(x, y, 1, 0x200, 0x200, 0, 0, 0);
            }
        }
        for x in 124..=127 {
            for y in 64..=68 {
                s.set_cube(x, y, 2, 0x200, 0x200, 0, 0, 0);
                // Always true while the random table stays below 512.
                // Arguments drawn last first (Mac case 1013d194, 68k
                // 0xc98e6..0xc990e): z, y, x. OS4 draws x, y, z.
                if s.random(1) == 0 {
                    let ez = 0x4268 + s.random(1500);
                    let ey = y * 0x800 + s.random(0x800);
                    let ex = x * 0x800 + s.random(0x800);
                    s.explosion(ex, ey, ez, 3000);
                }
            }
        }
        for x in 124..=126 {
            s.set_face(FACE_TOP, x, 69, 0, 1, 0xd0, 1);
        }
        s.say(2, if self.aboard { 77 } else { 76 });
        s.add_money(200_000);
        s.show_message(MISSION_COMPLETE);
        unlock(s);
    }
}

/// M1 failure tail (`0xc7f00..0xc80fe`).
fn kidnapping_fail(s: &mut Script, phrase: u32) {
    s.set_scenery(59, 33, 0, 0, 0x59, 0);
    s.set_scenery(86, 43, 0, 0, 0x59, 0);
    for id in 100..=102 {
        s.actor_release(id);
    }
    s.kill_all_mt();
    fail(s, phrase);
}

/// Arm AI racer `r`'s trigger to waypoint `wp` and set its speed cap.
fn arm_waypoint(s: &mut Script, r: usize, wp: usize) {
    let (x, y, radius, max_speed) = RACE_WAYPOINTS[wp];
    let id = 11 + r as i32;
    s.t(10000 + r as i32, kind::ACTOR_GOTO_DIRECT, id, x, y, radius);
    s.actor_set_max_speed(id, max_speed - 30 * r as i32);
}

/// Arm car `c`'s (id 10+c) trigger for checkpoint `cp`.
fn arm_checkpoint(s: &mut Script, c: usize, cp: usize) {
    let (ty, value) = RACE_CHECKPOINTS[cp];
    s.t(20000 + c as i32, ty, 10 + c as i32, value, 0, 0);
}

/// M2 "Rocket car" (slot 2, `0xc80fe..0xc8384`).
fn rocket_car(s: &mut Script, ev: i32) {
    match ev {
        // Got in: alarm, 50 s to reach the garage at (107,3).
        201 => {
            s.set_wanted_level(5);
            s.say(2, 20);
            s.t(202, kind::GARAGE, 107, 3, 1, 1);
            s.t(210, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
            s.t(205, kind::RC_THROTTLE, 0, 0, 0, 0);
            s.t(251, kind::COUNTDOWN, 50, 0, 0, 0);
        }
        205 => say_self(s, 1, 21),
        210 => {
            s.say(2, 22);
            s.kill_mt(202);
            s.t(211, kind::ENTER_ACTOR, 201, 0, 0, 0);
        }
        211 => {
            s.say(2, 23);
            s.t(202, kind::GARAGE, 107, 3, 1, 1);
            s.t(210, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
        }
        202 => {
            s.kill_all_mt();
            s.actor_release(201);
            win(s, 27, 250_000);
        }
        250..=252 => {
            if ev == 251 {
                s.actor_kill(201); // the alarm bomb goes off
            }
            s.kill_all_mt();
            s.actor_release(201);
            fail(s, (ev - 250 + 24) as u32);
        }
        _ => {}
    }
}

/// M6 "Bank-robbery van hijack" (slot 6, `0xc9080..0xc958a`).
fn bank_van(s: &mut Script, ev: i32) {
    match ev {
        // Escape route step: drive to the next point.
        616..=623 => {
            let (x, y) = VAN_ROUTE[(ev - 616) as usize];
            s.t(ev + 1, kind::ACTOR_GOTO_15, 600, x, y, 0);
            if ev == 623 {
                s.actor_set_max_speed(600, 150);
            }
        }
        // At the rival garage: its west door opens.
        624 => {
            s.say(2, 60);
            s.set_face(FACE_W, 124, 67, 1, 0, 0, 0);
            s.t(625, kind::ACTOR_GOTO_15, 600, 0x3efd0, 0x21c00, 0);
        }
        // Hijacked. Quirk: the 10 s start timer 616 is not killed, so a
        // hijack before it fires still sends the van on its route.
        601 => {
            for e in 617..=625 {
                s.kill_mt(e);
            }
            s.say(2, 60); // sic: phrase 62 was probably meant
            s.t(602, kind::GARAGE, 107, 3, 1, 1);
            s.t(610, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
            s.ped_exits_vehicle(601, 600, PED_NICK, 1);
            s.actor_attack(601, 60, -1);
            s.set_wanted_level(5);
            s.actor_set_max_speed(600, 1_000_000);
        }
        610 => {
            s.kill_mt(602);
            s.say(2, 63);
            s.t(611, kind::ENTER_ACTOR, 600, 0, 0, 0);
            s.actor_attack(601, 100, -1);
        }
        611 => {
            s.say(2, 64);
            s.t(602, kind::GARAGE, 107, 3, 1, 1);
            s.t(610, kind::VEHICLE_CHANGED, 0, 0, 0, 0);
            s.actor_attack(601, 100, -1);
        }
        // Delivered to Fry's garage.
        602 => {
            s.kill_all_mt();
            // Quirk: closes the EAST face, but ev 624 opened the west one.
            s.set_face(FACE_E, 124, 67, 1, 1, 0x76, 1);
            s.actor_release(600);
            s.actor_release(601);
            win(s, 65, 200_000);
        }
        625 | 650 | 651 => {
            s.kill_all_mt();
            s.set_face(FACE_W, 124, 67, 1, 1, 0x76, 1);
            s.actor_release(600);
            s.actor_release(601);
            match ev {
                625 => fail(s, 61),
                651 => fail(s, 59),
                _ => {
                    say_self(s, 1, 58);
                    s.show_message(MISSION_FAILED);
                    unlock(s);
                }
            }
        }
        _ => {}
    }
}

/// M8 "Grenade kill frenzy" (slot 8, `0xc9a6a..0xc9c02`).
fn grenades(s: &mut Script, ev: i32) {
    match ev {
        // First crate opened: 60 s to make 15,000 points.
        801..=812 => {
            for e in 801..=812 {
                s.kill_mt(e);
            }
            s.show_checkpoint_marker(false);
            s.show_message(KILL_FRENZY);
            s.say(2, 84);
            s.t(820, kind::MONEY_GAIN, 15_000, 0, 0, 0);
            s.t(851, kind::COUNTDOWN, 60, 0, 0, 0);
        }
        // The phone self-destructs (cosmetic).
        830 => {
            // y offset drawn first (Mac case 1013d750, 68k; OS4 draws x
            // first).
            let y = 0x3d3ce + s.random(100);
            let x = 0x253ce + s.random(100);
            s.explosion(x, y, 20000, 1500);
            s.set_scenery(74, 122, 0, 0, 0x2c, 0);
        }
        820 => {
            s.kill_all_mt();
            win(s, 83, 150_000);
        }
        850 => {
            s.show_checkpoint_marker(false);
            s.kill_all_mt();
            fail(s, 81);
        }
        851 => {
            s.kill_all_mt();
            fail(s, 82);
        }
        _ => {}
    }
}
