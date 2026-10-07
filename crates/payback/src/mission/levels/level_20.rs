//! Level 20 (city 2, level 0): "Escape from jail" (docs/re/missions_20.md).
//!
//! A single scripted jailbreak, no phone missions: an accomplice opens the
//! player's cell, 15 guards patrol the prison and raise the alarm when they
//! see the player, two keys open a locked door and the weapon store, and
//! leaving the prison block sends the player to the gang's garage in the
//! East, where `LevelComplete()` ends the level. Any death or arrest before
//! that resets the prison. Init 68k `0xb98f4`, handler `0xc73b4`, helpers
//! `0xb5468` (rectangles), `0xb5692` (alarm), `0xb5920` (guards).

use crate::mission::kind::*;
use crate::mission::{LevelScript, Script};

/// Square centre in world units (`sq << 11 | 0x400`).
const fn sq(x: i32, y: i32) -> (i32, i32) {
    ((x << 11) | 0x400, (y << 11) | 0x400)
}

/// Rectangle of the cell the player starts in (squares 12–13 × 19–20).
const CELL: [i32; 4] = [0x6000, 0x9800, 0x7000, 0xa800];

/// Prison block: squares 0..60 × 0..60 (event 200000 when left).
const PRISON: [i32; 4] = [0, 0, 0x1e800, 0x1e800];

/// The gang's garage at square (107.5, 3.5).
const GARAGE: (i32, i32) = (0x35c00, 0x1c00);

/// The helipad (38.0, 25.0), where the alarm helicopter lands.
const HELIPAD: (i32, i32) = (0x13000, 0xc800);

const ACCOMPLICE: i32 = 1;
const BOAT: i32 = 1000;
const HELICOPTER: i32 = 1001;

/// Vehicle models (load order `0xd7874`).
const MODEL_HELICOPTER: i32 = 17;
const MODEL_BOAT: i32 = 23;
/// Ped models are passed as `~m`: 7 = PoliceMale1, 9 = PedMale3.
const MODEL_POLICE: i32 = !7;
const MODEL_ACCOMPLICE: i32 = !9;

/// Face indices of `Script::set_face` (`SetFaceE` / `SetFaceW`).
const FACE_E: usize = 2;
const FACE_W: usize = 4;

/// Mission events.
const EV_ESCAPED: i32 = 200000;
const EV_HELI_LANDED: i32 = 200001;
const EV_NEAR_GARAGE: i32 = 200002;
const EV_IN_GARAGE: i32 = 200003;
const EV_RECT: i32 = 100000;
const EV_RECT_LEFT: i32 = 100100;
/// Sight/attack events are the guard id + 10.
const SIGHT: i32 = 10;

/// Indoor rectangle in squares (inclusive) and the floor it applies to.
struct Rect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    floor: i32,
}

const fn rect(x0: i32, y0: i32, x1: i32, y1: i32, floor: i32) -> Rect {
    Rect {
        x0,
        y0,
        x1,
        y1,
        floor,
    }
}

/// Roof cut-away rectangles (68k tables x0 `0x11a358`, y0 `0x11a3b4`,
/// x1 `0x11a410`, y1 `0x11a46c`, floor `0x11a4c8`). Rect i sends event
/// 100000 + i.
const RECTS: [Rect; 23] = [
    rect(26, 16, 27, 17, 0),
    rect(30, 20, 31, 21, 0),
    rect(28, 7, 33, 12, 0),
    rect(28, 14, 33, 19, 0),
    rect(27, 22, 34, 41, 0),
    rect(21, 28, 40, 35, 0),
    rect(6, 13, 25, 20, 0),
    rect(12, 7, 19, 26, 0),
    rect(27, 22, 34, 41, 1),
    rect(21, 28, 40, 35, 1),
    rect(6, 13, 25, 20, 1),
    rect(12, 7, 19, 26, 1),
    rect(27, 22, 34, 41, 2),
    rect(21, 28, 40, 35, 2),
    rect(6, 13, 25, 20, 2),
    rect(12, 7, 19, 26, 2),
    rect(15, 27, 16, 27, 0),
    rect(20, 31, 20, 32, 0),
    rect(6, 28, 19, 41, 0),
    rect(20, 37, 25, 41, 0),
    rect(36, 37, 42, 41, 0),
    rect(43, 10, 45, 12, 0),
    rect(38, 15, 42, 19, 0),
];

/// Group A patrol, guards 300000–300003 (`0x11a524` / `0x11a554`).
const ROUTE_A: [(i32, i32); 12] = [
    sq(14, 18),
    sq(6, 18),
    sq(6, 15),
    sq(14, 15),
    sq(14, 7),
    sq(17, 7),
    sq(17, 15),
    sq(25, 15),
    sq(25, 18),
    sq(17, 18),
    sq(17, 26),
    sq(14, 26),
];

/// Guard 300200 (`0x11a584` / `0x11a58c`). Quirk kept from the original
/// (also in OS4): x is `X << 11` without the `| 0x400` centre offset.
const ROUTE_200: [(i32, i32); 2] = [
    (16 << 11, (25 << 11) | 0x400),
    (16 << 11, (8 << 11) | 0x400),
];

/// Guards 300300–300301 (`0x11a594` / `0x11a5a4`).
const ROUTE_300: [(i32, i32); 4] = [sq(29, 33), sq(29, 30), sq(32, 30), sq(32, 33)];

/// Guard 300400 (`0x11a5b4` / `0x11a5bc`).
const ROUTE_400: [(i32, i32); 2] = [sq(33, 19), sq(33, 14)];

/// Guards 300500–300501 (`0x11a5c4` / `0x11a5ec`).
const ROUTE_500: [(i32, i32); 10] = [
    sq(42, 36),
    sq(42, 22),
    sq(36, 22),
    sq(36, 14),
    sq(35, 14),
    sq(35, 7),
    sq(35, 14),
    sq(36, 14),
    sq(36, 22),
    sq(42, 22),
];

/// Guard 300600 (`0x11a614` / `0x11a61c`).
const ROUTE_600: [(i32, i32); 2] = [sq(42, 11), sq(36, 11)];

/// Guard 300800 (`0x11a624` / `0x11a634`).
const ROUTE_800: [(i32, i32); 4] = [sq(10, 41), sq(6, 41), sq(6, 39), sq(10, 39)];

/// Where a guard stands or walks.
enum Post {
    /// Stands still at (x, y); no patrol trigger.
    Static(i32, i32),
    /// Starts at `route[start]` and walks the route cyclically.
    Patrol {
        route: &'static [(i32, i32)],
        start: usize,
    },
}

struct Guard {
    id: i32,
    floor: i32,
    angle: i32,
    post: Post,
    /// Group A ignores sighting while the player is in his cell.
    cell_aware: bool,
}

const fn patrol(
    id: i32,
    floor: i32,
    angle: i32,
    route: &'static [(i32, i32)],
    start: usize,
) -> Guard {
    Guard {
        id,
        floor,
        angle,
        post: Post::Patrol { route, start },
        cell_aware: false,
    }
}

const fn sentry(id: i32, angle: i32, x: i32, y: i32) -> Guard {
    Guard {
        id,
        floor: 0,
        angle,
        post: Post::Static(x, y),
        cell_aware: false,
    }
}

const fn group_a(id: i32, start: usize) -> Guard {
    Guard {
        cell_aware: true,
        ..patrol(id, 1, 0, &ROUTE_A, start)
    }
}

/// The 15 prison guards (`Guards_Event` 68k `0xb5920`), in the original
/// creation / alarm order. All are model PoliceMale1, colour 0.
const GUARDS: [Guard; 15] = [
    group_a(300000, 0),
    group_a(300001, 3),
    group_a(300002, 6),
    group_a(300003, 9),
    sentry(300100, 0, 0xcc00, 0x8000),
    sentry(300101, 0xb40, 0xcc00, 0x9000),
    patrol(300200, 0, 0xb40, &ROUTE_200, 0),
    patrol(300300, 1, 0, &ROUTE_300, 0),
    patrol(300301, 1, 0, &ROUTE_300, 2),
    patrol(300400, 0, 0xb40, &ROUTE_400, 0),
    patrol(300500, 1, 0, &ROUTE_500, 0),
    patrol(300501, 1, 0, &ROUTE_500, 5),
    patrol(300600, 0, 0xb40, &ROUTE_600, 0),
    sentry(300700, 0x10e0, 0x17400, 0x5c00),
    patrol(300800, 0, 0xb40, &ROUTE_800, 0),
];

/// Key / locked-door state (`0x2fba10`, `0x2fba14`).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Key {
    /// Not found, door not tried.
    #[default]
    Missing,
    /// Picked up.
    Found,
    /// Door tried without the key: picking it up re-arms the door trigger.
    DoorTried,
}

/// Level 20 globals (68k `0x2fb028..0x2fba50`).
#[derive(Default)]
pub struct Level20 {
    /// Alarm raised (`0x2fb9fc`).
    alarm: bool,
    /// Level finished (`0x2fba0c`).
    escaped: bool,
    /// Player inside his cell (`0x2fba00`).
    in_cell: bool,
    key1: Key,
    key2: Key,
    /// Weapon store opened (`0x2fba50`).
    store_open: bool,
    /// Current indoor rectangle (`0x2fb028`).
    cur_rect: Option<usize>,
    /// Next waypoint index per guard (`0x2fb030..0x2fb064`); unused for
    /// static guards.
    route_state: [usize; GUARDS.len()],
}

impl LevelScript for Level20 {
    /// `Init` 68k `0xb98f4`.
    fn init(&mut self, s: &mut Script) {
        // "The pigs did a raid on your house and put you away for life. ..."
        s.set_intro_phrase(1);
        self.alarm = false;
        self.escaped = false;
        // Goal -1: no trigger, the pause menu shows "Goal: Escape from jail".
        s.add_trigger(-1, MONEY_GOAL, -1, -1, -1, -1);
        self.rects_init(s);
        // No phone slots: only the first-call intro (speaker 2, phrase 1).
        s.refresh_phones();
        s.show_message(0);

        s.t(30, SCENERY_GONE_QUIET, 8, 40, 0, -1);
        s.t(31, ON_FOOT_AT_POINT_QUIET, 0x11000, 0x8c00, 0, -1);
        self.key1 = Key::Missing;
        s.t(40, SCENERY_GONE_QUIET, 37, 38, 0, -1);
        s.t(41, ON_FOOT_AT_POINT_QUIET, 0x11000, 0x4c00, 0, -1);
        self.key2 = Key::Missing;
        s.t(50, NEAR_POINT_6000, HELIPAD.0, HELIPAD.1, 0, -1);
        s.t(51, NEAR_POINT_6000, 0x16000, 0x5c00, 0, -1);

        s.create_actor(ACCOMPLICE, MODEL_ACCOMPLICE, 0x7400, 0xc400, 1, 0, 0);
        s.t(1, ACTOR_GOTO, ACCOMPLICE, 29000, 0xa400, -1);
        s.t(20, RESPAWNED, -1, -1, -1, -1);
        s.t(10, OUT_OF_RECT, CELL[0], CELL[1], CELL[2], CELL[3]);
        self.in_cell = true;
        // Every (re)spawn is inside the cell.
        s.set_respawn_point(12, 20, 1);
        self.guards_init(s);
    }

    /// Handler 68k `0xc73b4`.
    fn event(&mut self, s: &mut Script, ev: i32) {
        match ev {
            300000..=399999 => self.guards_event(s, ev),
            100000..=199999 => self.rects_event(s, ev),
            // The accomplice reaches the cell door.
            1 => {
                s.say(1, 4); // "Psst! Over here!"
                s.t(2, NEAR_ACTOR_1000, ACCOMPLICE, -1, -1, -1);
            }
            // Player next to the accomplice: remove the cell bars.
            2 => {
                s.say(1, 5);
                s.set_face(FACE_E, 13, 20, 2, 0, 0, 0);
                s.set_face(FACE_W, 14, 20, 2, 0, 0, 0);
                s.t(3, ACTOR_GOTO, ACCOMPLICE, 0x7400, 0x9400, -1);
            }
            3 => s.t(4, ACTOR_GOTO, ACCOMPLICE, 0x30d4, 0x9664, -1),
            4 => s.actor_release(ACCOMPLICE),
            10 => {
                self.in_cell = false;
                s.t_ex(11, IN_RECT, CELL[0], CELL[1], CELL[2], CELL[3], 1);
            }
            11 => {
                self.in_cell = true;
                s.t(10, OUT_OF_RECT, CELL[0], CELL[1], CELL[2], CELL[3]);
            }
            20 => self.respawned(s),
            // Key 1 picked up.
            30 => {
                think(s, 6); // "This could come in handy."
                if self.key1 == Key::DoorTried {
                    s.t(31, ON_FOOT_AT_POINT_QUIET, 0x11000, 0x8c00, 0, -1);
                }
                self.key1 = Key::Found;
            }
            // Locked door between squares 33 and 34 at y = 17.
            31 => {
                if self.key1 == Key::Found {
                    think(s, 8); // "Great! The key fits!"
                    s.set_face(FACE_W, 34, 17, 1, 0, 0, 0);
                    s.set_face(FACE_E, 33, 17, 1, 0, 0, 0);
                } else {
                    self.key1 = Key::DoorTried;
                    think(s, 7); // "Damn! It's locked. ..."
                }
            }
            // Key 2 picked up.
            40 => {
                think(s, 9); // "I wonder what this key is for?"
                if self.key2 == Key::DoorTried {
                    s.t(41, ON_FOOT_AT_POINT_QUIET, 0x11000, 0x4c00, 0, -1);
                }
                self.key2 = Key::Found;
            }
            // Weapon store door between squares 33 and 34 at y = 9.
            41 => {
                if self.key2 == Key::Found {
                    think(s, 11); // "It fits! ..."
                    s.set_face(FACE_W, 34, 9, 1, 0, 0, 0);
                    s.set_face(FACE_E, 33, 9, 1, 0, 0, 0);
                    self.store_open = true;
                } else {
                    self.key2 = Key::DoorTried;
                    think(s, 10); // "This must be the weapon store. ..."
                }
            }
            50 if !self.alarm => think(s, 12), // "Hmm..a helipad. ..."
            51 if !self.alarm && !self.store_open => think(s, 13), // "It's too heavily guarded here. ..."
            EV_ESCAPED => self.escaped_prison(s),
            EV_HELI_LANDED => s.heli_take_off(HELICOPTER, false),
            EV_NEAR_GARAGE => {
                s.set_face(FACE_E, 107, 3, 1, 0, 0, 0);
                s.say(2, 14); // "You made it - come in!"
                s.t(EV_IN_GARAGE, AT_POINT_400, GARAGE.0, GARAGE.1, 0, -1);
            }
            EV_IN_GARAGE => {
                s.say(2, 15); // "I knew you could do it! ..."
                s.set_respawn_point(-1, 0, 0);
                self.escaped = true;
                s.level_complete();
            }
            _ => {}
        }
    }
}

/// `Dialog_Begin(1); Dialog_Say1(n); Dialog_End()`: the player thinks aloud.
fn think(s: &mut Script, n: u32) {
    s.dialog_begin(1);
    s.say1(n);
    s.dialog_end();
}

/// Event 200000 when the player leaves the prison block.
fn arm_escape(s: &mut Script) {
    let [x0, y0, x1, y1] = PRISON;
    s.t(EV_ESCAPED, OUT_OF_RECT, x0, y0, x1, y1);
}

/// Patrol trigger: `guard` walks to `(x, y)`; the event is the guard id.
fn walk_to(s: &mut Script, guard: i32, (x, y): (i32, i32)) {
    s.t(guard, ACTOR_GOTO, guard, x, y, -1);
}

/// Sight and attack triggers, both sending the guard id + 10.
fn watch(s: &mut Script, guard: i32) {
    s.t(guard + SIGHT, ACTOR_SEES_PLAYER_QUIET, guard, -1, -1, -1);
    s.t(guard + SIGHT, ACTOR_ATTACKED_QUIET, guard, -1, -1, -1);
}

impl Level20 {
    // ---- Indoor rectangles (`Rects_Event` 68k `0xb5468`) ----------------

    /// Arm the "entered rect i" trigger (untracked, type 0x39).
    fn arm_rect(s: &mut Script, i: usize) {
        let r = &RECTS[i];
        s.add_trigger_ex(
            EV_RECT + i as i32,
            IN_RECT,
            r.x0 << 11,
            r.y0 << 11,
            (r.x1 + 1) * 0x800,
            (r.y1 + 1) * 0x800,
            r.floor,
        );
    }

    /// `Rects_Event(-1)`. Visible layers are not set here (engine default 3).
    fn rects_init(&mut self, s: &mut Script) {
        for i in 0..RECTS.len() {
            Self::arm_rect(s, i);
        }
        self.cur_rect = None;
    }

    /// `Rects_Event(ev)`: entering rect i shows layers `0..=floor` and
    /// watches for leaving it (half-square margin); leaving shows all.
    /// Other events in 100000–199999 are ignored.
    fn rects_event(&mut self, s: &mut Script, ev: i32) {
        let entered = usize::try_from(ev - EV_RECT)
            .ok()
            .filter(|&i| i < RECTS.len());
        if entered.is_none() && ev != EV_RECT_LEFT {
            return;
        }
        if let Some(cur) = self.cur_rect {
            Self::arm_rect(s, cur);
        }
        self.cur_rect = entered;
        let Some(i) = entered else {
            s.set_visible_layers(3);
            return;
        };
        let r = &RECTS[i];
        s.set_visible_layers(r.floor + 1);
        s.kill_trigger(EV_RECT_LEFT);
        s.add_trigger(
            EV_RECT_LEFT,
            OUT_OF_RECT,
            r.x0 * 0x800 - 0x400,
            r.y0 * 0x800 - 0x400,
            r.x1 * 0x800 + 0xc00,
            r.y1 * 0x800 + 0xc00,
        );
    }

    // ---- Guards (`Guards_Event` 68k `0xb5920`) --------------------------

    /// Create every guard at its post with its patrol, sight and attack
    /// triggers, and reset the patrol states.
    fn spawn_guards(&mut self, s: &mut Script) {
        for (g, state) in GUARDS.iter().zip(&mut self.route_state) {
            match g.post {
                Post::Static(x, y) => {
                    s.create_actor(g.id, MODEL_POLICE, x, y, g.floor, g.angle, 0);
                }
                Post::Patrol { route, start } => {
                    let (x, y) = route[start];
                    s.create_actor(g.id, MODEL_POLICE, x, y, g.floor, g.angle, 0);
                    *state = start + 1;
                    walk_to(s, g.id, route[*state]);
                }
            }
            watch(s, g.id);
        }
    }

    /// `Guards_Event(-1)`: escape trigger, the boat and the guards.
    fn guards_init(&mut self, s: &mut Script) {
        arm_escape(s);
        s.create_actor(BOAT, MODEL_BOAT, 0x17c00, 0x6400, 0, 0x5a0, 0);
        self.spawn_guards(s);
    }

    /// `Guards_Event(-2)` (68k `0xb5ff2`): the full prison reset after a
    /// respawn. Keys, opened doors, the rectangle state and consumed
    /// one-shot triggers are not reset; the accomplice is removed and not
    /// re-created.
    fn guards_reset(&mut self, s: &mut Script) {
        s.kill_mt(EV_ESCAPED);
        s.kill_mt(EV_NEAR_GARAGE);
        s.kill_mt(EV_IN_GARAGE);
        arm_escape(s);
        s.remove_all_actors();
        s.kill_mt(EV_HELI_LANDED);
        s.create_actor(BOAT, MODEL_BOAT, 0x17c00, 0x6400, 0, 0x5a0, 0);
        // The original kills G + 10 twice; the second kill has no effect.
        for g in &GUARDS {
            s.kill_mt(g.id);
            s.kill_mt(g.id + SIGHT);
        }
        self.spawn_guards(s);
    }

    /// `Guards_Event(ev)` for 300000–399999: patrol arrivals and sightings.
    /// Ignored once the alarm is on.
    fn guards_event(&mut self, s: &mut Script, ev: i32) {
        if self.alarm {
            return;
        }
        if let Some(i) = GUARDS.iter().position(|g| g.id == ev) {
            // Static guards have no patrol trigger.
            if let Post::Patrol { route, .. } = GUARDS[i].post {
                let st = &mut self.route_state[i];
                *st = (*st + 1) % route.len();
                walk_to(s, ev, route[*st]);
            }
        } else if let Some(g) = GUARDS.iter().find(|g| g.id + SIGHT == ev) {
            if g.cell_aware && self.in_cell {
                // Quirk: only the sight trigger is re-armed. The attack
                // trigger with the same event was consumed if it fired, so
                // hitting a group-A guard from the cell is ignored once.
                s.t(ev, ACTOR_SEES_PLAYER_QUIET, g.id, -1, -1, -1);
            } else {
                self.raise_alarm(s);
            }
        }
    }

    /// `Alarm()` 68k `0xb5692`: helicopter to the helipad, every guard
    /// attacks, wanted level 6 and the looping siren.
    fn raise_alarm(&mut self, s: &mut Script) {
        if self.alarm {
            return;
        }
        s.create_actor(HELICOPTER, MODEL_HELICOPTER, 0x29c00, 0x19400, 0, 0, 4);
        s.heli_take_off(HELICOPTER, true);
        s.t(
            EV_HELI_LANDED,
            ACTOR_GOTO,
            HELICOPTER,
            HELIPAD.0,
            HELIPAD.1,
            -1,
        );
        for g in &GUARDS {
            if matches!(g.post, Post::Patrol { .. }) {
                s.kill_mt(g.id);
            }
            // The original kills G + 10 twice; the second kill has no effect.
            s.kill_mt(g.id + SIGHT);
            s.actor_attack(g.id, 0x50, -1);
        }
        self.alarm = true;
        // "You tripped the alarm! ..."
        s.say(2, 3);
        s.set_wanted_level(6);
        // TODO(api): the original passes an audible range of 19000 as the
        // 8th AddSound argument; `Script::add_sound` has no range parameter.
        s.add_sound(
            0,
            0x40,
            true,
            1_000_000_000.0,
            0x2b11 as f32,
            0xc800,
            0xc800,
        );
    }

    /// Event 20: the player respawned (in the cell) after a death/arrest.
    fn respawned(&mut self, s: &mut Script) {
        s.t(20, RESPAWNED, -1, -1, -1, -1);
        if self.alarm {
            s.delete_sound(0);
            self.alarm = false;
        }
        if !self.escaped {
            self.guards_reset(s);
        }
    }

    /// Event 200000: the player left the prison block (squares 0..60).
    /// The guards are released, and the player is sent to the garage. If
    /// the alarm was off, phrases 3 and 2 are queued back to back.
    fn escaped_prison(&mut self, s: &mut Script) {
        self.raise_alarm(s);
        for g in &GUARDS {
            s.actor_release(g.id);
        }
        s.set_wanted_level(6);
        // "The guards have noticed you're gone ... Drive to our garage in
        // the East of the city, but watch out for the law!"
        s.say(2, 2);
        s.t(EV_NEAR_GARAGE, NEAR_POINT_10000, GARAGE.0, GARAGE.1, 0, -1);
    }
}
