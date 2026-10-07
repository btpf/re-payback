//! The single-player mission system (docs/re/script_api.md).
//!
//! In Update 7 the six level scripts are compiled into the game as
//! event-driven state machines: the per-tick trigger evaluator sends event
//! numbers to the current level's handler, which reacts through the script
//! API. Here each level implements [`LevelScript`] and talks to the game
//! through [`Script`], whose methods carry the original API names.

pub mod api;
pub mod eval;
pub mod levels;
pub mod triggers;

pub use api::Script;
use bevy::prelude::*;
pub use triggers::{Triggers, kind};

/// Number of phone slots per level.
pub const PHONE_SLOTS: usize = 20;

#[derive(Clone, Copy, Debug)]
pub struct PhoneSlot {
    /// Event sent when answered; -1 = empty.
    pub event: i32,
    /// Map phone number (`GetPhoneCoords`).
    pub phone: i32,
    /// Hidden phone: no arrow, doesn't count as an available mission.
    pub hidden: bool,
}

impl Default for PhoneSlot {
    fn default() -> Self {
        PhoneSlot {
            event: -1,
            phone: 0,
            hidden: false,
        }
    }
}

/// Level-script state shared by all levels (68k globals around 0x2fb158).
#[derive(Resource)]
pub struct MissionState {
    pub city: u8,
    pub level: u8,
    pub phones: [PhoneSlot; PHONE_SLOTS],
    /// Slot of the mission in progress (`0x11a354`); -1 = none.
    pub cur_slot: i32,
    /// Next RefreshPhones is the level's first (`0x2fb220`).
    pub first_phones: bool,
    /// Level intro phrase (`0x2fb224`).
    pub intro_phrase: u32,
    /// Events added with the tracked variants (`0x2fb228`).
    pub tracked: Vec<i32>,
    /// Money goal shown in the pause menu; <= 0 = "Escape from jail".
    pub goal: i32,
    /// Script sound slots (`AddSound_script` refs 0..9).
    pub sounds: [Option<Entity>; 10],
    /// The level is over (complete or game over).
    pub finished: bool,
    /// Forced traffic vehicle type (`0x11a734`), -1 = random.
    pub forced_traffic: i32,
    /// Script id for dropped bombs (`0x11a77e`), -1 = none.
    pub projectile_id: i32,
}

impl Default for MissionState {
    fn default() -> Self {
        MissionState {
            city: 0,
            level: 0,
            phones: [PhoneSlot::default(); PHONE_SLOTS],
            cur_slot: -1,
            first_phones: true,
            intro_phrase: 0,
            tracked: Vec::new(),
            goal: 0,
            sounds: [None; 10],
            finished: false,
            forced_traffic: -1,
            projectile_id: -1,
        }
    }
}

/// A level's mission script: init once, then react to events.
pub trait LevelScript: Send + Sync + 'static {
    fn init(&mut self, s: &mut Script);
    fn event(&mut self, s: &mut Script, ev: i32);
    /// Called at level end (`Mission_Shutdown`).
    fn shutdown(&mut self, _s: &mut Script) {}
}

/// The running level script.
#[derive(Resource)]
pub struct ActiveLevel(pub Option<Box<dyn LevelScript>>);

/// Outcome signalled to the game loop.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelOutcome {
    /// `LevelComplete()` from the level script.
    Complete,
    /// The player ran out of lives.
    GameOver,
}

/// Game ticks since the level started (`DAT_0114c738`; the original counts
/// 30 Hz ticks, ours are 25 Hz). Paused with virtual time.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LevelClock(pub u32);

impl LevelClock {
    pub const TICKS_PER_SECOND: u32 = 25;

    /// Whole minutes elapsed (what the trophy table compares against).
    pub fn minutes(&self) -> u32 {
        self.0 / (Self::TICKS_PER_SECOND * 60)
    }
}

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Triggers>()
            .init_resource::<LevelClock>()
            .init_resource::<MissionState>()
            .insert_resource(ActiveLevel(None))
            .add_message::<LevelOutcome>()
            .add_systems(
                OnEnter(crate::level::AppState::Playing),
                start_level.after(crate::sim::people::PlayerSpawned),
            )
            .add_systems(OnExit(crate::level::AppState::Playing), reset_missions)
            .add_systems(
                FixedUpdate,
                eval::run_triggers.after(crate::sim::combat::CombatSet),
            )
            .add_systems(
                FixedUpdate,
                (|mut clock: ResMut<LevelClock>| clock.0 += 1)
                    .run_if(in_state(crate::level::AppState::Playing)),
            )
            .add_systems(Update, eval::flash_phones);
    }
}

/// `Mission_Init(city, level)` at level start (single player only).
fn start_level(world: &mut World) {
    let spec = world.resource::<crate::level::LevelSpec>().clone();
    let Ok(city) = spec.map.parse::<u8>() else {
        return;
    };
    if std::env::args().any(|a| a == "--no-script") {
        return;
    }
    let Some(mut script) = levels::for_level(city, spec.level) else {
        return;
    };
    {
        let mut st = world.resource_mut::<MissionState>();
        *st = MissionState {
            city,
            level: spec.level,
            ..default()
        };
    }
    script.init(&mut Script::new(world));
    world.resource_mut::<ActiveLevel>().0 = Some(script);
}

/// Clear the level script and every trigger when the level is left.
fn reset_missions(world: &mut World) {
    let sounds = world.resource_mut::<Triggers>().clear();
    for e in sounds {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    world.insert_resource(MissionState::default());
    world.insert_resource(LevelClock::default());
    world.resource_mut::<ActiveLevel>().0 = None;
}
