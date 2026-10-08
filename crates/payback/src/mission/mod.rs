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
    /// Door side toggle of `Ped_ExitVehicle` (68k `0x34c1b0`, OS4
    /// `DAT_0114c628`; true = 1). The main loop sets it to 1 right before
    /// each frame's trigger update (68k `FUN_000ec686` ~127806, OS4
    /// ~49717) and at level start, and every exit flips it first, so the
    /// first alternating exit of a frame is on the driver's door side.
    pub exit_toggle: bool,
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
            exit_toggle: true,
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

/// Random missions in Rampage (Mac only, [`levels::random`]), set at level
/// start by the replay module from the settings or the replay header.
pub use crate::replay::RandomMissions;

/// Outcome signalled to the game loop.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelOutcome {
    /// `LevelComplete()` from the level script.
    Complete,
    /// The player ran out of lives.
    GameOver,
    /// Multiplayer: a player reached the required kills.
    MatchOver,
}

/// Game ticks since the level started (`DAT_0114c738`; 30 Hz ticks, 1800
/// per minute in the trophy check, as in the port). Paused with virtual time.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LevelClock(pub u32);

impl LevelClock {
    pub const TICKS_PER_SECOND: u32 = 30;

    /// Whole minutes elapsed (what the trophy table compares against).
    pub fn minutes(&self) -> u32 {
        self.0 / (Self::TICKS_PER_SECOND * 60)
    }
}

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Triggers>()
            .init_resource::<RandomMissions>()
            .init_resource::<LevelClock>()
            .init_resource::<MissionState>()
            .insert_resource(ActiveLevel(None))
            .add_message::<LevelOutcome>()
            .add_systems(
                OnEnter(crate::level::AppState::Playing),
                // After the level phrases are loaded (`LevelLoaded`), or
                // the intro line and first message come out empty.
                start_level
                    .after(crate::sim::people::PlayerSpawned)
                    .after(crate::level::LevelLoaded),
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
            .init_resource::<eval::FlashedPhones>()
            .add_systems(
                FixedUpdate,
                eval::flash_phones
                    .after(eval::run_triggers)
                    .run_if(in_state(crate::level::AppState::Playing)),
            );
    }
}

/// `Mission_Init(city, level)` at level start (single-player story), or
/// the Random missions in Rampage.
fn start_level(world: &mut World) {
    let spec = world.resource::<crate::level::LevelSpec>().clone();
    if std::env::args().any(|a| a == "--no-script") {
        return;
    }
    // Multiplayer runs the city with no level script (and no level phrases
    // in the original's LoadMap); so does Rampage, apart from the Mac's
    // Random missions.
    match *world.resource::<crate::level::GameMode>() {
        crate::level::GameMode::Story => {}
        crate::level::GameMode::Rampage { police } => {
            if world.get_resource::<RandomMissions>().is_none_or(|r| r.0) {
                start_random(world, &spec, police);
            }
            return;
        }
        _ => return,
    }
    let Ok(city) = spec.map.parse::<u8>() else {
        return;
    };
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

/// `InitScript_Random` (Mac @ 10129e50), called from `amiga_startlevel`
/// @ 100fff40 in Rampage when the map has phones (`GetNumAltPhones() > 0`).
/// The trigger loop then runs in Rampage too (`amiga_levelloop` @
/// 10100f50 gates it on the phone count instead of the Rampage flag).
fn start_random(world: &mut World, spec: &crate::level::LevelSpec, police: u8) {
    if Script::new(world).alt_phones().is_empty() {
        return;
    }
    let p = police.min(5) as usize;
    let user_maps = world.get_resource::<crate::frontend::usermaps::UserMaps>();
    let city = user_maps.and_then(|m| crate::frontend::usermaps::city_of(&m.0, spec));
    let user = city
        .and_then(|c| c.checked_sub(crate::frontend::usermaps::BUILT_IN))
        .and_then(|i| user_maps.and_then(|m| m.0.get(i)));
    let data = world.resource::<crate::data::GameData>();
    // `GetHighestScore` @ 100a05a0: the top of the Rampage table for P.
    let scores = match (user, city) {
        (Some(m), _) => Some(m.load_scores(data)),
        (None, Some(c)) => Some(crate::rampage::load_scores(data, c)),
        _ => None,
    };
    let goal = scores.map_or(0, |s| s.tables[p][0].score);
    // `GetJustCityName` @ 10084e40: phrases 1840-1842 (Mac), else the
    // user map's name.
    let texts = world.resource::<crate::hud::Texts>();
    let city_name = match (user, city) {
        (Some(m), _) => m.name.clone(),
        (None, Some(c)) => {
            let n = texts.global(1840 + c as u32);
            if n.is_empty() { texts.global(2170 + c as u32) } else { n }
        }
        _ => spec.map.clone(),
    };
    // `GetHighestLevelCompleted` @ 1009b580: story levels done in order.
    let progress = world
        .get_resource::<crate::frontend::progress::Profile>()
        .map_or(0, |p| p.cups.iter().position(|&c| c == 0).unwrap_or(6) as i32);
    *world.resource_mut::<MissionState>() = MissionState {
        city: 3,
        ..default()
    };
    let mut script = levels::random::Random::new(police as i32, goal, city_name, progress);
    script.init(&mut Script::new(world));
    world.resource_mut::<ActiveLevel>().0 = Some(Box::new(script));
}

/// Clear the level script and every trigger when the level is left.
fn reset_missions(world: &mut World) {
    world.resource_mut::<eval::FlashedPhones>().0.clear();
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
