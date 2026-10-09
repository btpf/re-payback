//! Mission triggers (docs/re/script_api.md §3; 68k `Trigger_Update`
//! `0xa4a68`, `Trigger_QueueAdd` `0xa6842`, `Trigger_QueueKill` `0xa692a`).
//!
//! A trigger is "when condition X happens, send event E to the level
//! handler, then delete the trigger". Adds and kills are queued and applied
//! after evaluation, so handlers may change triggers freely.

use bevy::prelude::*;

/// Trigger type codes (script_api.md §3.2).
pub mod kind {
    pub const MONEY_GOAL: u8 = 0x00;
    pub const PHONE: u8 = 0x01;
    pub const AT_ACTOR_ON_FOOT: u8 = 0x02;
    pub const IN_VEHICLE_MODEL: u8 = 0x03;
    pub const GARAGE: u8 = 0x04;
    pub const COUNTDOWN: u8 = 0x05;
    pub const PLAYER_DOWN: u8 = 0x06;
    pub const SECRET_PHONE: u8 = 0x07;
    pub const ENTER_ACTOR: u8 = 0x08;
    pub const ACTOR_GONE: u8 = 0x09;
    pub const MONEY_GAIN: u8 = 0x0a;
    pub const IN_ANY_VEHICLE: u8 = 0x0b;
    pub const ACTOR_BOARDS_PLAYER: u8 = 0x0c;
    pub const NEAR_ACTOR: u8 = 0x0d;
    pub const VEHICLE_CHANGED: u8 = 0x0e;
    pub const AT_POINT: u8 = 0x0f;
    pub const STOPPED_AT_POINT: u8 = 0x10;
    pub const TIMER: u8 = 0x11;
    pub const ACTOR_GOTO_VANISH: u8 = 0x12;
    pub const ACTOR_FASTER: u8 = 0x13;
    pub const ACTOR_SLOWER: u8 = 0x14;
    pub const NEAR_POINT_12000: u8 = 0x15;
    pub const ACTOR_OUTSIDE: u8 = 0x16;
    pub const ACTOR_GOTO: u8 = 0x17;
    pub const ACTOR_NEAR_PLAYER: u8 = 0x18;
    pub const ACTOR_LOST: u8 = 0x19;
    pub const NEAR_POINT_10000: u8 = 0x1a;
    pub const ACTOR_SEES_PLAYER: u8 = 0x1b;
    pub const PLAYER_OUTSIDE: u8 = 0x1c;
    pub const ACTOR_NEAR_ACTOR: u8 = 0x1d;
    pub const PLAYER_X_LE: u8 = 0x1e;
    pub const PLAYER_X_GT: u8 = 0x1f;
    pub const PLAYER_Y_LE: u8 = 0x20;
    pub const PLAYER_Y_GT: u8 = 0x21;
    pub const WEAPON_FIRED: u8 = 0x22;
    pub const WEAPON_SELECTED: u8 = 0x23;
    pub const WEAPON_SWITCHED: u8 = 0x24;
    pub const SCENERY_GONE: u8 = 0x25;
    pub const OUT_OF_AMMO: u8 = 0x26;
    pub const HANDBRAKE: u8 = 0x27;
    pub const ACTOR_KILLED: u8 = 0x28;
    pub const COP_KILLED: u8 = 0x29;
    pub const CUBE_SMASHED: u8 = 0x2a;
    pub const ACTOR_IN_WATER: u8 = 0x2b;
    pub const ON_FOOT_AT_POINT: u8 = 0x2c;
    pub const ENTER_ACTOR_QUIET: u8 = 0x2d;
    pub const ACTOR_ATTACKED: u8 = 0x2e;
    pub const SCENERY_GONE_QUIET: u8 = 0x2f;
    pub const ACTOR_ATTACKED_QUIET: u8 = 0x30;
    pub const ACTOR_SEES_PLAYER_QUIET: u8 = 0x31;
    pub const ACTOR_GOTO_DIRECT: u8 = 0x32;
    pub const ACTOR_X_LE: u8 = 0x33;
    pub const ACTOR_X_GT: u8 = 0x34;
    pub const ACTOR_Y_LE: u8 = 0x35;
    pub const ACTOR_Y_GT: u8 = 0x36;
    pub const TIMER_TICKS: u8 = 0x37;
    pub const ON_FOOT_NEAR_POINT: u8 = 0x38;
    pub const IN_RECT: u8 = 0x39;
    pub const OUT_OF_RECT: u8 = 0x3a;
    pub const ACTOR_INSIDE: u8 = 0x3b;
    pub const NEAR_ACTOR_1000: u8 = 0x3c;
    pub const ACTOR_BOARDS_ACTOR: u8 = 0x3d;
    pub const RESPAWNED: u8 = 0x3e;
    pub const NEAR_POINT_6000: u8 = 0x3f;
    pub const AT_POINT_400: u8 = 0x40;
    pub const ON_FOOT_AT_POINT_QUIET: u8 = 0x41;
    pub const RC_THROTTLE: u8 = 0x42;
    pub const ACTOR_FLEES: u8 = 0x43;
    pub const ACTOR_GOTO_15: u8 = 0x44;
}

/// One live trigger.
#[derive(Clone, Debug)]
pub struct Trigger {
    pub event: i32,
    pub kind: u8,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub e: i32,
    /// The actor this trigger watches (one trigger per matching actor).
    pub actor: Option<Entity>,
    /// Order triggers (0x0c, 0x12, 0x17, 0x3d, 0x44): every actor given
    /// the order; one trigger fires on the first completion.
    pub group: Vec<Entity>,
    /// Resolved through the script id `a` (Mac `GetObject` in
    /// `CheckForTriggers`): with no actor carrying the id yet, or the
    /// bound one gone or re-tagged, the trigger waits dormant and binds to
    /// the first actor that has the id.
    pub by_id: bool,
    /// Ticks left (timers) or a value captured at add time (money, vehicle).
    pub counter: i64,
    /// Looping ring sound for phones; stopped when the trigger goes.
    pub sound: Option<Entity>,
}

impl Trigger {
    pub fn new(event: i32, kind: u8, args: [i32; 5]) -> Trigger {
        Trigger {
            event,
            kind,
            a: args[0],
            b: args[1],
            c: args[2],
            d: args[3],
            e: args[4],
            actor: None,
            group: Vec::new(),
            by_id: false,
            counter: 0,
            sound: None,
        }
    }

    /// Shows a HUD direction marker (script_api.md §3.2 "Arrow" column).
    pub fn has_arrow(&self) -> bool {
        use kind::*;
        matches!(
            self.kind,
            PHONE
                | AT_ACTOR_ON_FOOT
                | GARAGE
                | ENTER_ACTOR
                | NEAR_ACTOR
                | AT_POINT
                | STOPPED_AT_POINT
                | NEAR_POINT_10000
                | ACTOR_SEES_PLAYER
                | SCENERY_GONE
                | ACTOR_KILLED
                | ON_FOOT_AT_POINT
                | ACTOR_ATTACKED
                | NEAR_ACTOR_1000
                | AT_POINT_400
        )
    }
}

#[derive(Clone, Debug)]
enum Pending {
    Add(Trigger),
    Kill(i32),
}

/// The trigger table (500 entries in the original).
#[derive(Resource, Default)]
pub struct Triggers {
    pub list: Vec<Trigger>,
    pending: Vec<Pending>,
    /// Edge events reported by game code (weapon fired, actor killed, ...),
    /// consumed by the next evaluation: (trigger kind, a, b, c).
    pub reports: Vec<(u8, i32, i32, i32)>,
    /// Actor-specific reports: (trigger kind, actor).
    pub actor_reports: Vec<(u8, Entity)>,
}

pub const MAX_TRIGGERS: usize = 500;

impl Triggers {
    pub fn queue_add(&mut self, t: Trigger) {
        self.pending.push(Pending::Add(t));
    }

    /// Kill every trigger with this event number.
    pub fn queue_kill(&mut self, event: i32) {
        self.pending.push(Pending::Kill(event));
    }

    pub fn report(&mut self, kind: u8, a: i32, b: i32, c: i32) {
        self.reports.push((kind, a, b, c));
    }

    pub fn report_actor(&mut self, kind: u8, actor: Entity) {
        self.actor_reports.push((kind, actor));
    }

    /// Apply queued adds and kills in order. Returns the ring sounds of
    /// removed triggers (to stop).
    pub fn apply_pending(&mut self) -> Vec<Entity> {
        let mut stopped = Vec::new();
        for p in std::mem::take(&mut self.pending) {
            match p {
                Pending::Add(t) => {
                    if self.list.len() < MAX_TRIGGERS {
                        self.list.push(t);
                    } else {
                        warn!("AddTrigger(): Too many triggers");
                    }
                }
                Pending::Kill(ev) => {
                    // `ProcessToDoList` @ 100f4880: `RemoveTrigger` @
                    // 100f4560 moves the last trigger into the freed index
                    // (68k `Trigger_Remove` `0xa45c6`), and the index is
                    // tested again.
                    let mut i = 0;
                    while i < self.list.len() {
                        if self.list[i].event == ev {
                            stopped.extend(self.list.swap_remove(i).sound);
                        } else {
                            i += 1;
                        }
                    }
                }
            }
        }
        stopped
    }

    pub fn clear(&mut self) -> Vec<Entity> {
        self.pending.clear();
        self.reports.clear();
        self.actor_reports.clear();
        self.list.drain(..).filter_map(|t| t.sound).collect()
    }

    /// Ticks left on the first visible countdown (HUD clock).
    pub fn countdown_seconds(&self) -> Option<i32> {
        self.list
            .iter()
            .find(|t| t.kind == kind::COUNTDOWN)
            .map(|t| (t.counter / 30) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_kill_removes_all_with_event() {
        let mut t = Triggers::default();
        t.queue_add(Trigger::new(5, kind::TIMER, [1, 0, 0, 0, 0]));
        t.queue_add(Trigger::new(5, kind::PLAYER_DOWN, [0; 5]));
        t.queue_add(Trigger::new(6, kind::PLAYER_DOWN, [0; 5]));
        t.apply_pending();
        assert_eq!(t.list.len(), 3);
        t.queue_kill(5);
        t.apply_pending();
        assert_eq!(t.list.len(), 1);
        assert_eq!(t.list[0].event, 6);
    }

    #[test]
    fn kill_moves_the_last_trigger_in() {
        let mut t = Triggers::default();
        for ev in [1, 2, 3, 4] {
            t.queue_add(Trigger::new(ev, kind::TIMER, [1, 0, 0, 0, 0]));
        }
        t.apply_pending();
        t.queue_kill(1);
        t.apply_pending();
        let order: Vec<i32> = t.list.iter().map(|t| t.event).collect();
        assert_eq!(order, [4, 2, 3]);
    }
}
