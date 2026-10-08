//! Level 00 (city 0, level 0) mission script (docs/re/missions_00.md):
//! 68k init `0xb91dc`, handler `0xb9b6a`, unlock routine `0xb506e`.
//!
//! Twelve phone missions, unlocked in three waves by [`unlock_phones`]. The
//! level ends when the player reaches 1,500,000 points (type-0 trigger).
//! Trigger arguments the original leaves out are passed as -1, like the
//! original calls do.

use crate::mission::api::{MISSION_COMPLETE, MISSION_FAILED};
use crate::mission::kind::*;
use crate::mission::{LevelScript, Script};

/// Secret Info pickup at (37,0) cube 1: phrase 400 (`0x898`).
const SECRET_INFO: i32 = 0x898;

// Model ids (`CreateActor`): vehicles, people (`~m`), special objects.
const MUNDANEO: i32 = 0;
const PUG: i32 = 1;
const BUS: i32 = 3;
const VAN: i32 = 7;
const LIMO: i32 = 8;
const TANK: i32 = 11;
const TAXI: i32 = 13;
const PEDMALE2: i32 = -9;
const PEDMALE3: i32 = -10;
const PEDMALE4: i32 = -11;
const SHOTGUN_PED: i32 = -12;
const KKK_PED: i32 = -13;
const CONE: i32 = 1001;
const DUMMY: i32 = 1002;

// Scenery ids (`SetScenery`).
const CRATE: i32 = 6;
const INFO: i32 = 24;
const ALARM_BOX: i32 = 21;
const ALARM_BOX_FLASH: i32 = 22;

// Weapons as numbered by trigger type 0x22.
const GRENADE: i32 = 3;
const SHOTGUN: i32 = 5;

/// M1: shotgun dummies are actors 0x65..=0x67, grenade dummies 0x68..=0x6a.
const SHOTGUN_DUMMY: i32 = 0x65;
const GRENADE_DUMMY: i32 = 0x68;
/// M1: the white lines (player x / y).
const LINE_X: i32 = 0x5400;
const LINE_Y: i32 = 0x1ec00;

/// M9 time-trial limo.
const TRIAL_LIMO: i32 = 0x385;
/// M9 start/finish line (5,33).
const TRIAL_START: (i32, i32) = (0x2c00, 0x10c00);
/// M9 checkpoints: event `0x387 + i` fires at `TRIAL_CHECKPOINTS[i]`; the
/// last one (0x396) is the finish line.
const TRIAL_CHECKPOINTS: [(i32, i32); 16] = [
    (0x3bd0, 0x5c00),
    (0x16400, 0x5400),
    (0x17c00, 0x2400),
    (0x38c00, 0x2400),
    (0x39400, 0x13400),
    (0x31c00, 0x18c00),
    (0x2bc00, 0x17400),
    (0x16c00, 0x17400),
    (0x157c0, 0x19064),
    (0x4400, 0x19400),
    (0x3400, 0x25c00),
    (0x8400, 0x25c00),
    (0x8400, 0x15400),
    (0xbc00, 0x14400),
    (0xbc00, 0x11400),
    TRIAL_START,
];

/// M2 dope car.
const DOPE_CAR: i32 = 1;
/// M3 bus.
const DOPE_BUS: i32 = 0x12d;

/// M6 traitor.
const TRAITOR: i32 = 0x259;
/// M6 walk: (arrival event, x, y). The last leg (into the rival's house)
/// removes him on arrival.
const TRAITOR_WALK: [(i32, i32, i32); 14] = [
    (0x25a, 0x7400, 0x12c00),
    (0x25b, 0x7918, 0x13c04),
    (0x25c, 0x9c40, 0x13c04),
    (0x25d, 0x9c40, 0x15400),
    (0x25e, 0x9470, 0x15400),
    (0x25f, 0x9470, 0x17c00),
    (0x260, 0xa400, 0x18400),
    (0x261, 0xa400, 0x19c00),
    (0x262, 0x9400, 0x1a3ec),
    (0x26c, 0x9400, 0x1d4c0),
    (0x263, 0x9400, 0x20c00),
    (0x264, 0xa3de, 0x20c00),
    (0x265, 0xbc00, 0x20400),
    (0x266, 0xd1f4, 0x20c00),
];

/// M11: KKK members are actors `0x44d + i`; killed events `0x489 + i`,
/// escaped events `0x475 + i`.
const KKK: i32 = 0x44d;
const KKK_KILLED: i32 = 0x489;
const KKK_ESCAPED: i32 = 0x475;
/// M11 spawn (x, y, angle), all around (98,28).
const KKK_SPAWN: [(i32, i32, i32); 5] = [
    (0x31400, 0xe400, 0x226),
    (0x31018, 0xe400, 0x1068),
    (0x31400, 0xe018, 0xdac),
    (0x31400, 0xe7e8, 0x8fc),
    (0x317e8, 0xe400, 0x6a4),
];

/// M5 bomb van and the parking spot outside the rival's house (24,66).
const BOMB_VAN: i32 = 0x1f5;
const HOUSE_SPOT: (i32, i32) = (0xc400, 0x21400);

/// M10 actors: the boss's red limo, the two gunmen, the boss on foot.
const BOSS_LIMO: i32 = 0x3e9;
const GUNMAN_A: i32 = 0x3ea;
const GUNMAN_B: i32 = 0x3eb;
const BOSS: i32 = 0x3ec;
/// M10 limo route: (arrival event, x, y).
const BOSS_ROUTE: [(i32, i32, i32); 17] = [
    (0x3ea, 0x3ac00, 0x3ac00),
    (0x3eb, 0x39400, 0x3ac00),
    (0x3f7, 0x39400, 0x3c400),
    (0x3f8, 0x37400, 0x3c400),
    (0x3f9, 0x33400, 0x3c400),
    (0x3fa, 0x30400, 0x3c400),
    (0x3fb, 0x2d400, 0x3c400),
    (0x3ec, 0x2a400, 0x3c400),
    (0x3fc, 0x26c00, 0x3c400),
    (0x3fd, 0x26c00, 0x39400),
    (0x3ed, 0x26c00, 0x36400),
    (0x3ee, 0x26c00, 0x34008),
    (0x3ef, 0x26c00, 0x30400),
    (0x3fe, 0x26400, 0x2ec00),
    (0x3f0, 0x26400, 0x2bc00),
    (0x3f1, 0x26400, 0x29400),
    (0x3f2, 0x24c00, 0x29400),
];

/// M0 actors: Ron, Trey and the player's car (tagged by `SetPlayerId`).
const RON: i32 = 0x32;
const TREY: i32 = 0x33;
const GETAWAY_CAR: i32 = 5000;
/// M0 stop outside the bank (89,75).
const BANK: (i32, i32) = (0x2cc00, 0x25c00);

/// M7 white pug and its two guards.
const WHITE_PUG: i32 = 0x2bd;
const GUARD_A: i32 = 0x2be;
const GUARD_B: i32 = 0x2c0;

/// M19 tank.
const ARMY_TANK: i32 = 0x76c;

/// Level globals (68k `0x2fba18..0x2fba50`). The original shares them
/// between missions; only one mission runs at a time.
#[derive(Default)]
pub struct Level00 {
    /// `0x2fba18`: M1 player is behind the white line.
    behind: bool,
    /// `0x2fba1c`: general counter: M1 valid shotgun hits, M9 bonus flag,
    /// M4 taxis delivered, M5 van moved, M10/M7 kills, M11 resolved KKK,
    /// M0 passengers aboard / proximity guard.
    count: i32,
    /// `0x2fba20`: M1 dummies destroyed from in front of the line.
    invalid: i32,
    /// `0x2fba24`: M1 shotgun pulls / grenade throws.
    shots: i32,
    /// `0x2fba28`: M1 valid grenade hits.
    grenade_hits: i32,
    /// `0x2fba30..0x2fba40`: M11 KKK member still in play.
    kkk_alive: [bool; 5],
    /// `0x2fba44` ("A"): M11 the KKK flee; M0 player near the pick-up /
    /// bank.
    flag_a: bool,
    /// `0x2fba48`: M11 KKK members not killed.
    kkk_left: i32,
    /// `0x2fba4c` ("B"): M7 guards alerted; M0 bank robbed (alarm on).
    flag_b: bool,
    /// `0x2fba50` ("C"): M0 the player is in the getaway car.
    flag_c: bool,
}

impl LevelScript for Level00 {
    /// Level setup (68k `0xb91dc`).
    fn init(&mut self, s: &mut Script) {
        s.set_intro_phrase(106);
        s.set_phone(1, 100, 2, false);
        s.set_phone(9, 900, 3, false);
        s.set_phone(7, 700, 1, true);
        s.set_phone(19, 1900, 10, true);
        s.add_trigger(-1, MONEY_GOAL, 1_500_000, -1, -1, -1);
        s.add_trigger(SECRET_INFO, SCENERY_GONE_QUIET, 0x25, 0, 1, -1);
        s.add_lives(2);
        // Four cones, never referenced again.
        for (id, x, y) in [
            (100_000, 0xb400, 0x2e000),
            (100_001, 0xb400, 0x2f800),
            (100_002, 0x15400, 0x2e000),
            (100_003, 0x15400, 0x2f800),
        ] {
            s.create_actor(id, CONE, x, y, 0, 0, 0);
        }
        s.refresh_phones();
        s.show_message(7);
    }

    /// Event handler (68k `0xb9b6a`): event 0 is ignored, 0x898 is the
    /// secret, everything else goes to the running mission (`curSlot`) or,
    /// with none running, to the phones.
    fn event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            0 => {}
            SECRET_INFO => s.hint(400),
            _ => match s.cur_slot() {
                0 => self.m0_getaway(s, ev),
                1 => self.m1_range(s, ev),
                2 => m2_dope_car(s, ev),
                3 => m3_bus(s, ev),
                4 => self.m4_taxis(s, ev),
                5 => self.m5_van_bomb(s, ev),
                6 => m6_tail(s, ev),
                7 => self.m7_pug(s, ev),
                9 => self.m9_time_trial(s, ev),
                10 => self.m10_escort(s, ev),
                11 => self.m11_kkk(s, ev),
                19 => m19_tank(s, ev),
                _ => self.answer_phone(s, ev),
            },
        }
    }
}

/// `AddMissionTrigger` with the omitted trailing arguments set to -1.
fn mt(s: &mut Script, ev: i32, ty: u8, args: &[i32]) {
    let arg = |i: usize| args.get(i).copied().unwrap_or(-1);
    s.t(ev, ty, arg(0), arg(1), arg(2), arg(3));
}

/// Phone briefing: stop every phone, then speaker 0 (phone) says `n`.
fn brief(s: &mut Script, n: u32) {
    s.kill_all_phones();
    s.say(0, n);
}

/// "Mission failed!" and the unlock routine.
fn failed(s: &mut Script) {
    s.show_message(MISSION_FAILED);
    unlock_phones(s);
}

/// FAIL(n, speaker): `Say(speaker, n)`, "Mission failed!", unlock.
fn fail(s: &mut Script, speaker: i32, n: u32) {
    s.say(speaker, n);
    failed(s);
}

/// WIN: "Mission complete!" (multiplier +1) and the unlock routine.
fn win(s: &mut Script) {
    s.show_message(MISSION_COMPLETE);
    unlock_phones(s);
}

/// `UnlockPhones()` (68k `0xb506e`): the answered mission is gone for good
/// (success or failure); finishing a whole wave opens the next one.
fn unlock_phones(s: &mut Script) {
    s.consume_slot();
    let done = |s: &mut Script, slots: &[usize]| slots.iter().all(|&i| s.phone_event(i) == -1);
    // (slot, event, phone); the hidden flags are never touched.
    let unlocked: &[(usize, i32, i32)] = match s.cur_slot() {
        1 | 9 if done(s, &[1, 9]) => &[(4, 400, 4), (2, 200, 5)],
        4 | 2 if done(s, &[4, 2]) => &[(11, 1100, 11), (3, 300, 12), (6, 600, 13)],
        11 | 3 | 6 if done(s, &[11, 3, 6]) => &[(5, 500, 14), (10, 1000, 17), (0, 20, 15)],
        _ => &[],
    };
    for &(slot, event, phone) in unlocked {
        s.set_phone(slot, event, phone, false);
    }
    s.refresh_phones();
    s.set_cur_slot(-1);
}

impl Level00 {
    /// Phone answered (68k `0xbe33e`): all phones stop ringing, the mission
    /// is briefed over the phone (speaker 0) and becomes the current slot.
    fn answer_phone(&mut self, s: &mut Script, ev: i32) {
        let slot = match ev {
            20 => {
                brief(s, 3);
                mt(s, 0x33, IN_ANY_VEHICLE, &[]);
                mt(s, 0x4b, PLAYER_DOWN, &[]);
                0
            }
            100 => {
                brief(s, 0);
                mt(s, 0x65, AT_POINT, &[0x5c00, 0x1e400, 0]);
                mt(s, 0x96, PLAYER_DOWN, &[]);
                self.invalid = 0;
                1
            }
            200 => {
                brief(s, 15);
                s.create_actor(DOPE_CAR, MUNDANEO, 0x1c00, 0x1ac00, 0, 0xb40, 6);
                mt(s, 0xc9, ENTER_ACTOR, &[DOPE_CAR]);
                mt(s, 0xfa, ACTOR_GONE, &[DOPE_CAR]);
                2
            }
            300 => {
                brief(s, 27);
                s.create_actor(DOPE_BUS, BUS, 0x24400, 0x32400, 0, 0x5a0, 6);
                mt(s, 0x12e, ENTER_ACTOR, &[DOPE_BUS]);
                mt(s, 0x12f, ACTOR_GONE, &[DOPE_BUS]);
                3
            }
            400 => {
                brief(s, 34);
                mt(s, 0x191, IN_VEHICLE_MODEL, &[TAXI]);
                mt(s, 0x1c2, PLAYER_DOWN, &[]);
                s.set_forced_traffic(TAXI);
                self.count = 0;
                4
            }
            500 => {
                brief(s, 39);
                s.create_actor(BOMB_VAN, VAN, 0x37c00, 0x16c00, 0, 0xb40, 4);
                mt(s, 0x1f5, ENTER_ACTOR, &[BOMB_VAN]);
                mt(s, 0x226, ACTOR_GONE, &[BOMB_VAN]);
                5
            }
            600 => {
                brief(s, 52);
                s.create_actor(TRAITOR, PEDMALE2, 0x59d8, 0x12400, 0, 0, 0);
                mt(s, 0x259, NEAR_POINT_10000, &[0x59d8, 0x12400, 0]);
                mt(s, 0x28a, PLAYER_DOWN, &[]);
                mt(s, 0x28c, ACTOR_GONE, &[TRAITOR]);
                mt(s, 0x28b, ACTOR_SEES_PLAYER, &[TRAITOR]);
                s.set_scenery(20, 39, 0, INFO, 0, 0);
                mt(s, 0x2a8, SCENERY_GONE_QUIET, &[20, 39, 0]);
                6
            }
            700 => {
                brief(s, 58);
                s.create_actor(WHITE_PUG, PUG, 0x33068, 0x6400, 0, 0xfa0, 7);
                s.create_actor(GUARD_A, SHOTGUN_PED, 0x32400, 0x6400, 0, 0x6e0, 0);
                s.create_actor(GUARD_B, SHOTGUN_PED, 0x33c20, 0x6400, 0, 0xfa0, 0);
                mt(s, 0x2bd, ACTOR_SEES_PLAYER, &[GUARD_A]);
                mt(s, 0x2bd, ACTOR_SEES_PLAYER, &[GUARD_B]);
                mt(s, 0x2be, ENTER_ACTOR, &[WHITE_PUG]);
                mt(s, 0x2da, ACTOR_GONE, &[GUARD_A]);
                mt(s, 0x2da, ACTOR_GONE, &[GUARD_B]);
                mt(s, 0x2ee, PLAYER_DOWN, &[]);
                mt(s, 0x2ef, ACTOR_GONE, &[WHITE_PUG]);
                self.flag_b = false;
                self.count = 0;
                7
            }
            900 => {
                brief(s, 86);
                s.create_actor(TRIAL_LIMO, LIMO, 0x1c00, 0x15400, 0, 0xb40, 1);
                mt(s, 0x3b6, PLAYER_DOWN, &[]);
                mt(s, 0x3b6, ACTOR_GONE, &[TRIAL_LIMO]);
                mt(s, 0x385, ENTER_ACTOR, &[TRIAL_LIMO]);
                s.set_scenery(1, 42, 0, INFO, 0, 0);
                mt(s, 0x3d4, SCENERY_GONE_QUIET, &[1, 42, 0]);
                9
            }
            1000 => {
                brief(s, 90);
                s.create_actor(BOSS_LIMO, LIMO, 0x3bc00, 0x3bc00, 0, 0x5a0, 4);
                mt(s, 0x3e9, NEAR_ACTOR, &[BOSS_LIMO]);
                mt(s, 0x41a, PLAYER_DOWN, &[]);
                mt(s, 0x41b, ACTOR_GONE, &[BOSS_LIMO]);
                10
            }
            1100 => {
                brief(s, 98);
                for (i, (x, y, angle)) in (0..).zip(KKK_SPAWN) {
                    s.create_actor(KKK + i, KKK_PED, x, y, 0, angle, 0);
                }
                for i in 0..5 {
                    mt(s, KKK_KILLED + i, ACTOR_KILLED, &[KKK + i]);
                }
                mt(s, 0x47e, PLAYER_DOWN, &[]);
                mt(s, 0x44d, NEAR_POINT_10000, &[0x31400, 0xe400, 0]);
                self.count = 0;
                self.kkk_left = 5;
                self.kkk_alive = [true; 5];
                self.flag_a = false;
                11
            }
            1900 => {
                brief(s, 20);
                s.create_actor(ARMY_TANK, TANK, 0x36400, 0x25400, 0, 0xb40, 6);
                mt(s, 0x76d, ENTER_ACTOR, &[ARMY_TANK]);
                mt(s, 0x78a, PLAYER_DOWN, &[]);
                mt(s, 0x794, ACTOR_GONE, &[ARMY_TANK]);
                19
            }
            _ => return,
        };
        s.set_cur_slot(slot);
    }

    /// M1, weapons tutorial at the shooting range (68k `0xb9bd2`): shotgun
    /// dummies, then grenade dummies, each worth 20,000 when destroyed from
    /// behind the line; +25,000 and "Mission complete" if all three shotgun
    /// hits were valid.
    fn m1_range(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Arrived at the range: crate with a shotgun.
            0x65 => {
                s.say(2, 112);
                s.set_scenery(14, 60, 0, CRATE, 0xde, 5);
                mt(s, 0x66, SCENERY_GONE, &[14, 60, 0]);
                s.show_checkpoint_marker(true);
            }
            // Shotgun collected.
            0x66 => {
                s.say(2, 113);
                for (i, y) in (0..).zip([0x1dc00, 0x1e400, 0x1ec00]) {
                    s.create_actor_fx(SHOTGUN_DUMMY + i, DUMMY, 0x4400, y, 0, 0x5a0, 0, SHOTGUN);
                }
                self.behind = true;
                mt(s, 0x67, PLAYER_X_LE, &[LINE_X]);
                for i in 0..3 {
                    mt(s, 0x6f + i, ACTOR_KILLED, &[SHOTGUN_DUMMY + i]);
                }
                self.count = 0;
                mt(s, 0x72, WEAPON_FIRED, &[SHOTGUN]);
                self.shots = 0;
            }
            // Crossed the line / back behind it.
            0x67 => {
                s.dialog_flush();
                s.say(2, 114);
                self.behind = false;
                mt(s, 0x68, PLAYER_X_GT, &[LINE_X]);
            }
            0x68 => {
                s.dialog_flush();
                s.say(2, 115);
                self.behind = true;
                mt(s, 0x67, PLAYER_X_LE, &[LINE_X]);
            }
            // Shotgun dummy destroyed.
            0x6f..=0x71 => {
                s.dialog_flush();
                s.actor_release(SHOTGUN_DUMMY + ev - 0x6f);
                s.dialog_begin(2);
                if self.score_dummy(s) {
                    self.count += 1;
                }
                if self.count + self.invalid == 3 {
                    s.say2(118);
                    open_grenade_crate(s);
                    // Quirk: 0x68 is not killed, so crossing back later
                    // still says phrase 115 and re-arms 0x67.
                    s.kill_mt(0x67);
                    s.kill_mt(0x72);
                    self.invalid = 0;
                }
                s.dialog_end();
            }
            // Shotgun fired (two shots per pull): the 10th pull ends the stage.
            0x72 => {
                mt(s, 0x72, WEAPON_FIRED, &[SHOTGUN]);
                let pulls = self.shots;
                self.shots += 1;
                if pulls == 9 {
                    s.dialog_begin(2);
                    s.say2(120);
                    s.say2(118);
                    s.dialog_end();
                    open_grenade_crate(s);
                    for id in SHOTGUN_DUMMY..SHOTGUN_DUMMY + 3 {
                        s.actor_release(id);
                    }
                    // Quirks: 0x72 stays armed, so later shotgun shots keep
                    // counting into the grenade-throw counter, and `invalid`
                    // is not reset, so earlier invalid hits shorten the
                    // grenade stage.
                    for ev in [0x67, 0x68, 0x6f, 0x70, 0x71] {
                        s.kill_mt(ev);
                    }
                }
            }
            // Grenades collected.
            0x73 => {
                s.say(2, 119);
                for (i, x) in (0..).zip([0x5400, 0x5c00, 0x6400]) {
                    s.create_actor_fx(GRENADE_DUMMY + i, DUMMY, x, 0x1fc00, 0, 0xb40, 0, GRENADE);
                }
                self.behind = true;
                mt(s, 0x74, PLAYER_Y_GT, &[LINE_Y]);
                for i in 0..3 {
                    mt(s, 0x7c + i, ACTOR_KILLED, &[GRENADE_DUMMY + i]);
                }
                self.grenade_hits = 0;
                mt(s, 0x76, WEAPON_FIRED, &[GRENADE]);
                self.shots = 0;
            }
            0x74 => {
                s.dialog_flush();
                s.say(2, 114);
                self.behind = false;
                mt(s, 0x75, PLAYER_Y_LE, &[LINE_Y]);
            }
            0x75 => {
                s.dialog_flush();
                s.say(2, 115);
                self.behind = true;
                mt(s, 0x74, PLAYER_Y_GT, &[LINE_Y]);
            }
            // Grenade thrown: the 6th throw fails at once.
            0x76 => {
                mt(s, 0x76, WEAPON_FIRED, &[GRENADE]);
                let throws = self.shots;
                self.shots += 1;
                if throws == 5 {
                    s.show_checkpoint_marker(false);
                    s.kill_all_mt();
                    s.dialog_begin(2);
                    s.say2(120);
                    s.say2(121);
                    s.dialog_end();
                    for id in GRENADE_DUMMY..GRENADE_DUMMY + 3 {
                        s.actor_release(id);
                    }
                    s.set_scenery(14, 61, 0, 0, 0, 0);
                    failed(s);
                }
            }
            // Grenade dummy destroyed.
            0x7c..=0x7e => {
                s.dialog_flush();
                s.actor_release(GRENADE_DUMMY + ev - 0x7c);
                s.dialog_begin(2);
                if self.score_dummy(s) {
                    self.grenade_hits += 1;
                }
                if self.grenade_hits + self.invalid == 3 {
                    s.kill_all_mt();
                    s.show_checkpoint_marker(false);
                    s.set_scenery(14, 61, 0, 0, 0, 0);
                    // Only the shotgun stage decides: grenade misses don't
                    // matter.
                    if self.count == 3 {
                        s.say2(123);
                        s.dialog_end();
                        s.add_money(25_000);
                        win(s);
                    } else {
                        s.say2(122);
                        s.dialog_end();
                        failed(s);
                    }
                } else {
                    s.dialog_end();
                }
            }
            // Player dead or arrested.
            0x96 => {
                s.show_checkpoint_marker(false);
                s.set_scenery(14, 61, 0, 0, 0, 0);
                s.kill_all_mt();
                s.say(2, 2);
                for id in SHOTGUN_DUMMY..GRENADE_DUMMY + 3 {
                    s.actor_release(id);
                }
                failed(s);
            }
            // Info at (14,61).
            0xb4 => s.hint(69),
            _ => {}
        }
    }

    /// M1: a dummy destroyed (inside an open dialogue). From behind the line
    /// it pays 20,000 and counts; otherwise it is an invalid hit.
    fn score_dummy(&mut self, s: &mut Script) -> bool {
        if self.behind {
            s.say2(116);
            s.add_money(20_000);
        } else {
            s.say2(117);
            self.invalid += 1;
        }
        self.behind
    }

    /// M9, limo time trial (68k `0xbb9cc`): 15 checkpoints in 110 s;
    /// 100,000, or 125,000 within 80 s. Too slow = retry from the start.
    fn m9_time_trial(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // In the limo.
            0x385 => {
                s.say(2, 87);
                mt(s, 0x3b8, VEHICLE_CHANGED, &[]);
                mt(s, 0x386, AT_POINT, &[TRIAL_START.0, TRIAL_START.1, 0]);
                s.show_checkpoint_marker(true);
            }
            // Start line crossed.
            0x386 => {
                s.say(2, 30);
                // Never added; a no-op in the original too.
                s.kill_mt(0x3ac);
                trial_checkpoint(s, 0);
                mt(s, 0x3b7, COUNTDOWN, &[110]);
                mt(s, 0x3a2, TIMER, &[80]);
                self.count = 1;
            }
            0x387..=0x395 => trial_checkpoint(s, (ev - 0x387 + 1) as usize),
            // Finish.
            0x396 => {
                s.kill_all_mt();
                s.show_checkpoint_marker(false);
                s.actor_release(TRIAL_LIMO);
                s.dialog_begin(2);
                if self.count == 1 {
                    s.say2(89);
                    s.add_money(125_000);
                } else {
                    s.say2(33);
                    s.add_money(100_000);
                }
                s.dialog_end();
                win(s);
            }
            // 80 s gone: no bonus.
            0x3a2 => self.count = 0,
            // 110 s gone: back to the start line. The marker stays on, and
            // the Info trigger 0x3d4 is gone while its pickup stays.
            0x3b7 => {
                s.kill_all_mt();
                s.say(2, 88);
                mt(s, 0x386, AT_POINT, &[TRIAL_START.0, TRIAL_START.1, 0]);
                mt(s, 0x3b6, PLAYER_DOWN, &[]);
                mt(s, 0x3b6, ACTOR_GONE, &[TRIAL_LIMO]);
                mt(s, 0x3b8, VEHICLE_CHANGED, &[]);
            }
            // Dead / limo destroyed, or left the limo.
            0x3b6 | 0x3b8 => {
                s.kill_all_mt();
                s.show_checkpoint_marker(false);
                s.set_scenery(1, 42, 0, 0, 0, 0);
                s.actor_release(TRIAL_LIMO);
                fail(s, 2, if ev == 0x3b6 { 2 } else { 32 });
            }
            0x3d4 => s.hint(70),
            _ => {}
        }
    }

    /// M4, two taxis to Rob's garage (68k `0xbc61a`): 75,000. Traffic is
    /// forced to spawn taxis while the player needs one.
    fn m4_taxis(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Driving a taxi.
            0x191 => {
                s.kill_all_mt();
                s.say(2, 35);
                s.set_forced_traffic(-1);
                mt(s, 0x1c2, PLAYER_DOWN, &[]);
                mt(s, 0x192, VEHICLE_CHANGED, &[]);
                mt(s, 0x193, GARAGE, &[22, 43, 1, 0]);
            }
            // Left the taxi.
            0x192 => {
                s.kill_all_mt();
                s.say(2, 36);
                s.set_forced_traffic(TAXI);
                mt(s, 0x191, IN_VEHICLE_MODEL, &[TAXI]);
                mt(s, 0x1c2, PLAYER_DOWN, &[]);
            }
            // Taxi delivered.
            0x193 => {
                s.kill_all_mt();
                s.dialog_begin(1);
                let delivered = self.count;
                self.count += 1;
                if delivered == 0 {
                    s.say2(37);
                    s.dialog_end();
                    mt(s, 0x191, IN_VEHICLE_MODEL, &[TAXI]);
                    mt(s, 0x1c2, PLAYER_DOWN, &[]);
                    s.set_forced_traffic(TAXI);
                } else {
                    s.say2(38);
                    s.dialog_end();
                    s.add_money(75_000);
                    win(s);
                }
            }
            0x1c2 => {
                s.kill_all_mt();
                s.say(2, 2);
                s.set_forced_traffic(-1);
                failed(s);
            }
            _ => {}
        }
    }

    /// M5, van bomb outside the rival's house (68k `0xbca08`): park, get
    /// away within 10 s; 100,000 if the van was not moved off the spot.
    fn m5_van_bomb(&mut self, s: &mut Script, ev: i32) {
        let (hx, hy) = HOUSE_SPOT;
        match ev {
            // In the van.
            0x1f5 => {
                s.kill_all_mt();
                mt(s, 0x226, PLAYER_DOWN, &[]);
                mt(s, 0x226, ACTOR_GONE, &[BOMB_VAN]);
                mt(s, 0x1f6, VEHICLE_CHANGED, &[]);
                s.say(2, 40);
                mt(s, 0x1f7, AT_POINT, &[hx, hy, 0]);
            }
            // Left the van.
            0x1f6 => {
                s.kill_all_mt();
                mt(s, 0x226, PLAYER_DOWN, &[]);
                mt(s, 0x226, ACTOR_GONE, &[BOMB_VAN]);
                s.say(2, 41);
                mt(s, 0x1f5, ENTER_ACTOR, &[BOMB_VAN]);
            }
            // Near the house (0x1f6 stays armed).
            0x1f7 => {
                mt(s, 0x1f8, STOPPED_AT_POINT, &[hx, hy, 0]);
                s.say(2, 14);
            }
            // Stopped at the house: the fuse is lit.
            0x1f8 => {
                s.kill_all_mt();
                mt(s, 0x226, PLAYER_DOWN, &[]);
                mt(s, 0x226, ACTOR_GONE, &[BOMB_VAN]);
                mt(s, 0x1fa, ACTOR_OUTSIDE, &[BOMB_VAN, hx, hy]);
                s.say(2, 42);
                mt(s, 0x1f9, COUNTDOWN, &[10]);
                self.count = 0;
            }
            // Van moved away from the spot.
            0x1fa => self.count = 1,
            // The bomb goes off.
            0x1f9 => {
                s.kill_all_mt();
                s.actor_kill(BOMB_VAN);
                if self.count == 0 {
                    for x in 26..=30 {
                        for y in 61..=70 {
                            if s.random(2) == 0 {
                                // RNG call order of the 68k build: z, y, x.
                                let z = s.random(0x5dc) + 0x4268;
                                let dy = s.random(0x800);
                                let dx = s.random(0x800);
                                s.explosion(x * 0x800 + dx, y * 0x800 + dy, z, 0xbb8);
                            }
                        }
                    }
                    // Wreck the house.
                    for x in 26..=30 {
                        for y in 61..=70 {
                            s.set_cube(x, y, 1, 0x200, 0x200, 0, 0, 0);
                        }
                    }
                    s.say(2, 44);
                    s.add_money(100_000);
                    win(s);
                } else {
                    s.say(2, 43);
                    s.actor_release(BOMB_VAN);
                    failed(s);
                }
            }
            // Dead, or the van destroyed.
            0x226 => {
                s.kill_all_mt();
                s.actor_release(BOMB_VAN);
                fail(s, 2, 19);
            }
            _ => {}
        }
    }

    /// M10, escort the boss's red limo (68k `0xbb308`): an ambush on the
    /// way, the boss walks in at the end; 125,000.
    fn m10_escort(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Caught up with the limo: it sets off, gunmen appear.
            0x3e9 => {
                s.say(2, 91);
                self.count = 0;
                s.create_actor(GUNMAN_A, SHOTGUN_PED, 0x29800, 0x339ec, 0, 0x10e0, 0);
                s.create_actor(GUNMAN_B, SHOTGUN_PED, 0x29800, 0x33e14, 0, 0x10e0, 0);
                mt(s, 0x410, ACTOR_GONE, &[GUNMAN_A]);
                mt(s, 0x411, ACTOR_GONE, &[GUNMAN_B]);
                mt(s, 0x412, ACTOR_NEAR_ACTOR, &[GUNMAN_A, BOSS_LIMO]);
                mt(s, 0x412, ACTOR_NEAR_ACTOR, &[GUNMAN_B, BOSS_LIMO]);
                boss_route(s, 0);
                mt(s, 0x41c, ACTOR_LOST, &[BOSS_LIMO]);
                mt(s, 0x415, ACTOR_FASTER, &[BOSS_LIMO, 0x50]);
            }
            // Limo arrived: the boss gets out and walks in.
            0x3f2 => {
                s.ped_exits_vehicle(BOSS, BOSS_LIMO, PEDMALE3, -1);
                for ev in [0x41b, 0x413, 0x414, 0x415] {
                    s.kill_mt(ev);
                }
                // Phrase 92 (limo destroyed) now also covers the boss.
                mt(s, 0x41b, ACTOR_GONE, &[BOSS]);
                mt(s, 0x3f3, ACTOR_GOTO_VANISH, &[BOSS, 0x25c00, 0x274e8]);
            }
            0x3f3 => {
                release_escort(s);
                s.kill_all_mt();
                s.say(2, 94);
                s.add_money(125_000);
                win(s);
            }
            // A gunman killed: the other one turns on the player.
            0x410 => {
                self.count += 1;
                s.actor_attack(GUNMAN_B, 0x50, -1);
                s.say(2, 62);
            }
            0x411 => {
                self.count += 1;
                s.actor_attack(GUNMAN_A, 0x50, -1);
                s.say(2, 63);
            }
            // A gunman is close to the limo.
            0x412 => {
                s.actor_attack(GUNMAN_A, 0x50, BOSS_LIMO);
                s.actor_attack(GUNMAN_B, 0x50, BOSS_LIMO);
            }
            // Limo-stopped watchdog: moving -> wait for a stop.
            0x414 | 0x415 => {
                if ev == 0x414 {
                    s.kill_mt(0x416);
                }
                mt(s, 0x413, ACTOR_SLOWER, &[BOSS_LIMO, 1]);
            }
            // Stopped: call for help after 2 s unless it moves again.
            0x413 => {
                mt(s, 0x416, TIMER, &[2]);
                mt(s, 0x414, ACTOR_FASTER, &[BOSS_LIMO, 0x50]);
            }
            0x416 => {
                s.say(2, 96);
                s.kill_mt(0x414);
                mt(s, 0x417, ACTOR_FASTER, &[BOSS_LIMO, 0x50]);
            }
            0x417 => {
                s.dialog_flush();
                s.say(2, 97);
                mt(s, 0x413, ACTOR_SLOWER, &[BOSS_LIMO, 1]);
            }
            // Dead, limo (or boss) destroyed, limo lost.
            0x41a..=0x41c => {
                release_escort(s);
                s.kill_all_mt();
                let n = match ev {
                    0x41a => 19,
                    0x41b => 92,
                    _ => 95,
                };
                fail(s, 2, n);
            }
            // Waypoint reached: drive to the next one.
            _ => {
                // (0x3f2, the last one, is handled above.)
                let legs = &BOSS_ROUTE[..BOSS_ROUTE.len() - 1];
                let Some(i) = legs.iter().position(|w| w.0 == ev) else {
                    return;
                };
                boss_route(s, i + 1);
                // Ambush unless both gunmen are dead.
                if ev == 0x3ee && self.count <= 1 {
                    s.actor_attack(GUNMAN_A, 0x41, BOSS_LIMO);
                    s.actor_attack(GUNMAN_B, 0x41, BOSS_LIMO);
                    s.say(2, 93);
                }
            }
        }
    }

    /// M11, KKK meeting in central park (68k `0xba86a`): 20,000 per kill
    /// once all five are killed or escaped; none killed is a failure.
    fn m11_kkk(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // Player approached.
            0x44d => {
                self.kkk_flee(s);
                s.say(2, 107);
            }
            // Member i killed.
            0x489..=0x48d => {
                let i = ev - KKK_KILLED;
                self.kkk_alive[i as usize] = false;
                s.actor_release_idle(KKK + i);
                s.kill_mt(KKK_ESCAPED + i);
                self.kkk_flee(s);
                self.kkk_left -= 1;
                self.count += 1;
                if self.count <= 4 {
                    // 108 (four left) .. 111 (one left).
                    s.say(2, (112 - self.kkk_left) as u32);
                } else {
                    s.kill_all_mt();
                    s.say(2, (99 + self.kkk_left) as u32);
                    s.add_money(20_000 * (5 - self.kkk_left));
                    win(s);
                }
            }
            // Member i escaped.
            0x475..=0x479 => {
                let i = ev - KKK_ESCAPED;
                self.kkk_alive[i as usize] = false;
                s.actor_release_idle(KKK + i);
                s.kill_mt(KKK_KILLED + i);
                s.say(2, 105);
                self.count += 1;
                if self.count > 4 {
                    s.kill_all_mt();
                    // 99 (all killed) .. 104 (none).
                    s.say(2, (99 + self.kkk_left) as u32);
                    if self.kkk_left <= 4 {
                        s.add_money(20_000 * (5 - self.kkk_left));
                        win(s);
                    } else {
                        failed(s);
                    }
                }
            }
            0x47e => {
                for (i, _) in (0..).zip(self.kkk_alive).filter(|(_, alive)| *alive) {
                    s.actor_release_idle(KKK + i);
                }
                s.kill_all_mt();
                fail(s, 2, 2);
            }
            _ => {}
        }
    }

    /// M11 `Flee()`: the members still in play flee from the player; the
    /// first call also arms their "escaped" triggers.
    fn kkk_flee(&mut self, s: &mut Script) {
        for (i, _) in (0..).zip(self.kkk_alive).filter(|(_, alive)| *alive) {
            s.actor_attack(KKK + i, -1, -1);
            if !self.flag_a {
                mt(s, KKK_ESCAPED + i, ACTOR_LOST, &[KKK + i]);
            }
        }
        self.flag_a = true;
    }

    /// M0, getaway driver for the bank job (68k `0xbcf88`): steal a car,
    /// pick up Ron and Trey, wait at the bank, then Harry's garage within
    /// 50 s with wanted level 5; 150,000. `flag_a` = near the pick-up/bank,
    /// `flag_b` = robbed (alarm on), `flag_c` = in the car.
    fn m0_getaway(&mut self, s: &mut Script, ev: i32) {
        let (bx, by) = BANK;
        match ev {
            // In a vehicle: it becomes the getaway car.
            0x33 => {
                s.kill_all_mt();
                s.set_player_id(GETAWAY_CAR);
                mt(s, 0x4e, PLAYER_DOWN, &[]);
                s.create_actor(RON, PEDMALE3, 0x32c00, 0x13400, 0, 0, 0);
                s.create_actor(TREY, PEDMALE4, 0x33400, 0x13400, 0, 0, 0);
                mt(s, 0x4c, ACTOR_GONE, &[RON]);
                mt(s, 0x4d, ACTOR_GONE, &[TREY]);
                mt(s, 0x34, NEAR_ACTOR, &[RON]);
                mt(s, 0x34, NEAR_ACTOR, &[TREY]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                mt(s, 0x28, VEHICLE_CHANGED, &[]);
                s.say(2, 4);
                self.count = 1;
            }
            // Left the car before the pick-up.
            0x28 => {
                s.kill_all_mt();
                s.say(1, 10);
                mt(s, 0x4b, PLAYER_DOWN, &[]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                mt(s, 0x29, ENTER_ACTOR, &[GETAWAY_CAR]);
            }
            // Back in: only Ron's proximity trigger is re-added.
            0x29 => {
                s.kill_all_mt();
                s.say(1, 4);
                mt(s, 0x4b, PLAYER_DOWN, &[]);
                mt(s, 0x4c, ACTOR_GONE, &[RON]);
                mt(s, 0x4d, ACTOR_GONE, &[TREY]);
                mt(s, 0x34, NEAR_ACTOR, &[RON]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                mt(s, 0x28, VEHICLE_CHANGED, &[]);
            }
            // At the pick-up: they walk to the car.
            0x34 => {
                if self.count == 1 {
                    s.kill_all_mt();
                    s.say(1, 9);
                    mt(s, 0x4e, PLAYER_DOWN, &[]);
                    mt(s, 0x4c, ACTOR_GONE, &[RON]);
                    mt(s, 0x4d, ACTOR_GONE, &[TREY]);
                    board(s, 0x35, 0x36);
                    mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                    mt(s, 0x2a, VEHICLE_CHANGED, &[]);
                    mt(s, 0x1e, ACTOR_LOST, &[RON]);
                    self.count = 0;
                    self.flag_c = true;
                    self.flag_a = true;
                }
            }
            // Drove away from the pick-up.
            0x1e => {
                s.kill_mt(0x35);
                s.kill_mt(0x36);
                mt(s, 0x1f, NEAR_ACTOR, &[RON]);
                mt(s, 0x1f, NEAR_ACTOR, &[TREY]);
                s.say(1, 49);
                self.flag_a = false;
            }
            // Back at the pick-up. The second 0x1f trigger stays armed.
            0x1f => {
                if !self.flag_a {
                    self.flag_a = true;
                    if self.flag_c {
                        board(s, 0x35, 0x36);
                        s.say(1, 50);
                    } else {
                        s.say(1, 51);
                    }
                    mt(s, 0x1e, ACTOR_LOST, &[RON]);
                }
            }
            // Left the car at the pick-up.
            0x2a => {
                self.flag_c = false;
                s.say(1, 10);
                s.kill_mt(0x35);
                s.kill_mt(0x36);
                mt(s, 0x2b, ENTER_ACTOR, &[GETAWAY_CAR]);
            }
            0x2b => {
                if !self.flag_c {
                    self.flag_c = true;
                    s.say(1, 4);
                    if self.flag_a {
                        board(s, 0x35, 0x36);
                    }
                    mt(s, 0x2a, VEHICLE_CHANGED, &[]);
                }
            }
            // One of them boarded.
            0x35 | 0x36 => {
                let aboard = self.count;
                self.count += 1;
                if aboard == 1 {
                    s.kill_all_mt();
                    s.say(1, 8);
                    mt(s, 0x4e, PLAYER_DOWN, &[]);
                    mt(s, 0x38, VEHICLE_CHANGED, &[]);
                    mt(s, 0x37, AT_POINT, &[bx, by, 0]);
                    mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                }
            }
            // Near the bank.
            0x37 => {
                s.kill_all_mt();
                s.say(1, 14);
                mt(s, 0x4e, PLAYER_DOWN, &[]);
                mt(s, 0x38, VEHICLE_CHANGED, &[]);
                mt(s, 0x3a, STOPPED_AT_POINT, &[bx, by, 0]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
            }
            // Left the car on the way to the bank.
            0x38 => {
                s.kill_all_mt();
                s.say(1, 10);
                mt(s, 0x4b, PLAYER_DOWN, &[]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                mt(s, 0x39, ENTER_ACTOR, &[GETAWAY_CAR]);
            }
            // Back in (the car-destroyed trigger is not re-added).
            0x39 => {
                s.kill_all_mt();
                s.say(1, 13);
                mt(s, 0x4b, PLAYER_DOWN, &[]);
                mt(s, 0x38, VEHICLE_CHANGED, &[]);
                mt(s, 0x37, AT_POINT, &[bx, by, 0]);
            }
            // Stopped at the bank: Ron and Trey get out and walk in.
            0x3a => {
                s.kill_all_mt();
                s.say(1, 12);
                s.ped_exits_vehicle(RON, -1, PEDMALE3, -1);
                s.ped_exits_vehicle(TREY, -1, PEDMALE4, -1);
                mt(s, 0x4e, PLAYER_DOWN, &[]);
                mt(s, 0x4e, ACTOR_GONE, &[RON]);
                mt(s, 0x4e, ACTOR_GONE, &[TREY]);
                mt(s, 0x3d, VEHICLE_CHANGED, &[]);
                mt(s, 0x3b, ACTOR_GOTO_VANISH, &[RON, 0x2a5bc, 0x26400]);
                mt(s, 0x3c, ACTOR_GOTO_VANISH, &[TREY, 0x2a5bc, 0x25c00]);
                mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                mt(s, 0x2e, PLAYER_OUTSIDE, &[bx, by]);
                self.count = 0;
                self.flag_c = true;
                self.flag_b = false;
                self.flag_a = true;
            }
            // Both inside: 10 s robbery.
            0x3b | 0x3c => {
                let inside = self.count;
                self.count += 1;
                if inside == 1 {
                    s.kill_mt(0x4e);
                    mt(s, 0x4e, PLAYER_DOWN, &[]);
                    mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                    mt(s, 0x3e, TIMER, &[10]);
                }
            }
            // Robbery done: alarm on, they come out.
            0x3e => {
                self.flag_b = true;
                s.set_cube(85, 74, 2, 0x40, 0x150, ALARM_BOX_FLASH, 0x3f, 1);
                s.set_cube(85, 77, 2, 0x40, 0x150, ALARM_BOX_FLASH, 0x3f, 1);
                // TODO(api): `add_sound` has no range argument (20000).
                s.add_sound(0, 0x30, true, 100_000_000.0, 16000.0, 0x2a800, 0x26000, 20000);
                s.create_actor(RON, PEDMALE3, 0x2a84c, 0x26400, 0, 0, 0);
                s.create_actor(TREY, PEDMALE4, 0x2a84c, 0x25c00, 0, 0, 0);
                mt(s, 0x4e, ACTOR_GONE, &[RON]);
                mt(s, 0x4e, ACTOR_GONE, &[TREY]);
                if self.flag_c && self.flag_a {
                    board(s, 0x41, 0x42);
                }
                self.count = 0;
            }
            // Drove off from the bank.
            0x2e => {
                self.flag_a = false;
                s.kill_mt(0x41);
                s.kill_mt(0x42);
                s.say(2, 46);
                mt(s, 0x2f, AT_POINT, &[bx, by, 0]);
            }
            0x2f => {
                if !self.flag_a {
                    self.flag_a = true;
                    if self.flag_c && self.flag_b {
                        board(s, 0x41, 0x42);
                    }
                    mt(s, 0x2e, PLAYER_OUTSIDE, &[bx, by]);
                    s.say(2, 47);
                }
            }
            // Left the car at the bank.
            0x3d => {
                s.say(2, 10);
                mt(s, 0x3f, ENTER_ACTOR, &[GETAWAY_CAR]);
                s.kill_mt(0x41);
                s.kill_mt(0x42);
                self.flag_c = false;
            }
            0x3f => {
                if !self.flag_c {
                    s.say(2, 45);
                    mt(s, 0x3d, VEHICLE_CHANGED, &[]);
                    self.flag_c = true;
                    if self.flag_a && self.flag_b {
                        board(s, 0x41, 0x42);
                    }
                }
            }
            // Both back aboard: to Harry's garage. The re-armed 0x3d can
            // still lead to 0x3d/0x3f if the player gets out.
            0x41 | 0x42 => {
                let aboard = self.count;
                self.count += 1;
                if aboard == 1 {
                    s.kill_all_mt();
                    s.say(1, 24);
                    s.set_wanted_level(5);
                    mt(s, 0x3d, VEHICLE_CHANGED, &[]);
                    mt(s, 0x4e, ACTOR_GONE, &[GETAWAY_CAR]);
                    mt(s, 0x43, GARAGE, &[3, 11, 1, 1]);
                    mt(s, 0x4e, COUNTDOWN, &[50]);
                    mt(s, 0x44, TIMER, &[40]);
                    mt(s, 0x4e, PLAYER_DOWN, &[]);
                }
            }
            0x44 => s.say(1, 26),
            // Delivered.
            0x43 => {
                s.kill_all_mt();
                self.restore_alarm(s);
                s.say(1, 25);
                s.set_wanted_level(0);
                s.add_money(150_000);
                win(s);
            }
            // Dead, car destroyed, out of time, Ron/Trey killed.
            0x4b | 0x4e => {
                s.kill_all_mt();
                self.restore_alarm(s);
                release_getaway(s);
                fail(s, 2, 19);
            }
            0x4c | 0x4d => {
                s.kill_all_mt();
                release_getaway(s);
                fail(s, 2, if ev == 0x4c { 5 } else { 6 });
            }
            _ => {}
        }
    }

    /// M0: switch the bank alarm off again if it is on.
    fn restore_alarm(&mut self, s: &mut Script) {
        if self.flag_b {
            s.set_cube(85, 74, 2, 0x40, 0x150, ALARM_BOX, 0x3f, 1);
            s.set_cube(85, 77, 2, 0x40, 0x150, ALARM_BOX, 0x3f, 1);
            s.delete_sound(0);
            self.flag_b = false;
        }
    }

    /// M7, steal the white pug from its guards (68k `0xbc2dc`, hidden phone
    /// 1): Rob's garage, 150,000; +25,000 for killing both guards.
    fn m7_pug(&mut self, s: &mut Script, ev: i32) {
        match ev {
            // A guard spotted the player.
            0x2bd => {
                if !self.flag_b {
                    self.flag_b = true;
                    s.kill_mt(0x2bd);
                    s.say(2, 64);
                    s.actor_attack(GUARD_A, 0x1e, -1);
                    s.actor_attack(GUARD_B, 0x1e, -1);
                }
            }
            // In the pug.
            0x2be => {
                if !self.flag_b {
                    self.flag_b = true;
                    s.kill_mt(0x2bd);
                    s.actor_attack(GUARD_A, 0x3c, -1);
                    s.actor_attack(GUARD_B, 0x3c, -1);
                }
                mt(s, 0x2bf, GARAGE, &[22, 43, 1, 0]);
                mt(s, 0x2c0, VEHICLE_CHANGED, &[]);
                s.say(2, 65);
            }
            // Left the pug.
            0x2c0 => {
                s.kill_mt(0x2bf);
                mt(s, 0x2c1, ENTER_ACTOR, &[WHITE_PUG]);
                s.say(2, 66);
            }
            0x2c1 => {
                mt(s, 0x2bf, GARAGE, &[22, 43, 1, 0]);
                mt(s, 0x2c0, VEHICLE_CHANGED, &[]);
                s.say(2, 67);
            }
            // A guard killed.
            0x2da => {
                self.count += 1;
                if !self.flag_b {
                    self.flag_b = true;
                    s.kill_mt(0x2bd);
                    s.say(2, 64);
                    s.actor_attack(GUARD_A, 0x28, -1);
                    s.actor_attack(GUARD_B, 0x28, -1);
                }
                // 60 "One down, one to go!" / 61 "... extra twenty five grand".
                s.say(2, (59 + self.count) as u32);
                if self.count == 2 {
                    s.add_money(25_000);
                }
            }
            // Delivered.
            0x2bf => {
                s.kill_all_mt();
                release_pug(s);
                s.say(1, 68);
                s.add_money(150_000);
                win(s);
            }
            // Dead, or the pug destroyed.
            0x2ee | 0x2ef => {
                s.kill_all_mt();
                release_pug(s);
                fail(s, 2, if ev == 0x2ee { 19 } else { 59 });
            }
            _ => {}
        }
    }
}

/// M1: the second crate (grenades) at (14,59) and an Info at (14,61).
fn open_grenade_crate(s: &mut Script) {
    s.set_scenery(14, 59, 0, CRATE, 0x2c, 3);
    mt(s, 0x73, SCENERY_GONE, &[14, 59, 0]);
    s.set_scenery(14, 61, 0, INFO, 0, 0);
    mt(s, 0xb4, SCENERY_GONE_QUIET, &[14, 61, 0]);
}

/// M9: arm checkpoint `i` (event `0x387 + i`).
fn trial_checkpoint(s: &mut Script, i: usize) {
    let (x, y) = TRIAL_CHECKPOINTS[i];
    mt(s, 0x387 + i as i32, AT_POINT, &[x, y, 0]);
}

/// M2, dope car to Harry's garage (68k `0xbc7fe`): 25 s, 100,000. The
/// countdown keeps running while the player is out of the car.
fn m2_dope_car(s: &mut Script, ev: i32) {
    match ev {
        // In the car.
        0xc9 => {
            s.kill_all_mt();
            mt(s, 0xfa, ACTOR_GONE, &[DOPE_CAR]);
            mt(s, 0xfb, VEHICLE_CHANGED, &[]);
            s.say(2, 16);
            mt(s, 0xd2, COUNTDOWN, &[25]);
            mt(s, 0xca, GARAGE, &[3, 11, 1, 1]);
        }
        // Delivered.
        0xca => {
            s.kill_all_mt();
            s.actor_release(DOPE_CAR);
            s.say(1, 17);
            s.add_money(100_000);
            win(s);
        }
        // Left the car: raw (untracked) kill of the garage trigger.
        0xfb => {
            s.kill_trigger(0xca);
            s.say(2, 10);
            mt(s, 0xcb, ENTER_ACTOR, &[DOPE_CAR]);
        }
        0xcb => {
            mt(s, 0xfb, VEHICLE_CHANGED, &[]);
            s.say(2, 11);
            mt(s, 0xca, GARAGE, &[3, 11, 1, 1]);
        }
        // Time out, or the car destroyed.
        0xd2 | 0xfa => {
            s.kill_all_mt();
            s.actor_release(DOPE_CAR);
            if ev == 0xd2 {
                fail(s, 1, 18);
            } else {
                fail(s, 2, 19);
            }
        }
        _ => {}
    }
}

/// M3, bus to Jon's garage over the bridge (68k `0xbe160`): 50 s, 125,000.
fn m3_bus(s: &mut Script, ev: i32) {
    match ev {
        // In the bus.
        0x12e => {
            s.kill_all_mt();
            s.say(2, 28);
            mt(s, 0x12f, ACTOR_GONE, &[DOPE_BUS]);
            mt(s, 0x130, COUNTDOWN, &[50]);
            mt(s, 0x131, GARAGE, &[12, 116, 1, 3]);
            mt(s, 0x133, VEHICLE_CHANGED, &[]);
        }
        // Delivered.
        0x131 => {
            s.kill_all_mt();
            s.say(2, 29);
            s.add_money(125_000);
            win(s);
        }
        // Left the bus.
        0x133 => {
            s.say(2, 31);
            mt(s, 0x135, ENTER_ACTOR, &[DOPE_BUS]);
            s.kill_mt(0x131);
        }
        // Back in. Original bug, kept: the garage is re-armed at Harry's
        // (3,11) side 1 instead of Jon's (12,116) side 3.
        0x135 => {
            s.say(2, 28);
            mt(s, 0x131, GARAGE, &[3, 11, 1, 1]);
            mt(s, 0x133, VEHICLE_CHANGED, &[]);
        }
        // Bus destroyed, or time out.
        0x12f | 0x130 => {
            s.kill_all_mt();
            s.actor_release(DOPE_BUS);
            fail(s, 2, if ev == 0x12f { 19 } else { 18 });
        }
        _ => {}
    }
}

/// M6, tail the traitor to a rival's house (68k `0xbbefa`): 100,000; fails
/// if he spots the player, dies or gets more than 33000 away.
fn m6_tail(s: &mut Script, ev: i32) {
    match ev {
        // Player close: he sets off.
        0x259 => {
            s.say(2, 53);
            traitor_step(s, 0);
            mt(s, 0x28d, ACTOR_LOST, &[TRAITOR]);
        }
        // Went into the rival's house.
        0x266 => {
            s.kill_all_mt();
            s.say(2, 57);
            s.add_money(100_000);
            s.show_message(MISSION_COMPLETE);
            s.set_scenery(20, 39, 0, 0, 0, 0);
            unlock_phones(s);
        }
        // Dead, spotted, killed, lost.
        0x28a..=0x28d => {
            s.kill_all_mt();
            s.actor_release_idle(TRAITOR);
            s.say(2, [2, 54, 55, 56][(ev - 0x28a) as usize]);
            s.show_message(MISSION_FAILED);
            s.set_scenery(20, 39, 0, 0, 0, 0);
            unlock_phones(s);
        }
        0x2a8 => s.hint(71),
        // Walk leg finished: next leg.
        _ => {
            // (0x266, the last one, is handled above.)
            let legs = &TRAITOR_WALK[..TRAITOR_WALK.len() - 1];
            if let Some(i) = legs.iter().position(|w| w.0 == ev) {
                traitor_step(s, i + 1);
            }
        }
    }
}

/// M6: send the traitor on walk leg `i`; the last leg removes him on
/// arrival.
fn traitor_step(s: &mut Script, i: usize) {
    let (ev, x, y) = TRAITOR_WALK[i];
    let ty = if i + 1 == TRAITOR_WALK.len() {
        ACTOR_GOTO_VANISH
    } else {
        ACTOR_GOTO
    };
    mt(s, ev, ty, &[TRAITOR, x, y]);
}

/// M10: send the limo to route waypoint `i`.
fn boss_route(s: &mut Script, i: usize) {
    let (ev, x, y) = BOSS_ROUTE[i];
    mt(s, ev, ACTOR_GOTO, &[BOSS_LIMO, x, y]);
}

/// M10: release the limo and both gunmen.
fn release_escort(s: &mut Script) {
    for id in [BOSS_LIMO, GUNMAN_A, GUNMAN_B] {
        s.actor_release(id);
    }
}

/// M0: Ron and Trey walk to the player's car and board it (events
/// `ron_ev`, `trey_ev`).
fn board(s: &mut Script, ron_ev: i32, trey_ev: i32) {
    mt(s, ron_ev, ACTOR_BOARDS_PLAYER, &[RON]);
    mt(s, trey_ev, ACTOR_BOARDS_PLAYER, &[TREY]);
}

/// M0: release Ron, Trey and the getaway car.
fn release_getaway(s: &mut Script) {
    for id in [RON, TREY, GETAWAY_CAR] {
        s.actor_release(id);
    }
}

/// M7: release the pug; the guards go back to wandering.
fn release_pug(s: &mut Script) {
    s.actor_release(WHITE_PUG);
    s.actor_release_idle(GUARD_A);
    s.actor_release_idle(GUARD_B);
}

/// M19, tank kill frenzy (68k `0xbce1c`, hidden phone 10): +15,000 points
/// in 60 s (the text promises 20,000) pays 50,000. Once the player is in
/// the tank, destroying it no longer fails the mission.
fn m19_tank(s: &mut Script, ev: i32) {
    match ev {
        // In the tank.
        0x76d => {
            s.kill_all_mt();
            s.actor_release(ARMY_TANK);
            mt(s, 0x78a, PLAYER_DOWN, &[]);
            s.say(2, 21);
            s.show_message(2029);
            mt(s, 0x776, COUNTDOWN, &[60]);
            mt(s, 0x780, MONEY_GAIN, &[15_000]);
        }
        0x780 => {
            s.kill_all_mt();
            s.say(2, 22);
            s.add_money(50_000);
            win(s);
        }
        0x776 => {
            s.kill_all_mt();
            fail(s, 2, 18);
        }
        0x78a => {
            s.kill_all_mt();
            fail(s, 2, 19);
        }
        // Tank destroyed before the player got in.
        0x794 => {
            s.kill_all_mt();
            s.actor_release(ARMY_TANK);
            fail(s, 2, 23);
        }
        _ => {}
    }
}
