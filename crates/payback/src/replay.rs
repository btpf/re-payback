//! Replays (docs/re/frontend_menus.md §8; OS4 load `FUN_010bc000`, save
//! `FUN_010bc3b0`, record word `FUN_01051a10`, playback `FUN_0104135c`).
//!
//! The original records one input word per player per frame, with the
//! number of 30 Hz ticks the frame took, plus the random seed at level
//! start; playback restores the seed and feeds the words back in place of
//! the controls. Here the simulation runs at a fixed 30 Hz, the original's
//! tick, so every game tick is one record with `ticks = 1` (durations are in
//! 30 Hz ticks, as in the original files).
//!
//! The random generator's whole state is its table index ([`Rng::seed`]),
//! which is the stored seed. For the replay to reproduce the game, every
//! system that consumes randomness must run in the same order each tick:
//! the fixed-step schedules use the single-threaded executor, which runs
//! systems in the (deterministic) topological order of the schedule.
//!
//! Debug / determinism check (`--map` required):
//! `--record <file> [--script-input] [--seed <n>] [--ticks <n>]` plays,
//! logs a hash of every object every 50 ticks, writes the replay at tick
//! `n` (default 500) and exits; `--play <file> [--ticks <n>]` plays it back
//! and logs the same hashes, then exits. Matching hashes = deterministic.
//! `--record ... --again` plays the recording back in the same session
//! after leaving the level, logging a second set of hashes.
//!
//! Multiplayer games record one word per player per tick (records
//! interleaved by player, as in the original).

use crate::level::{AppState, GameMode, LevelLoaded, LevelSpec};
use crate::sim::people::{PeopleThink, PlayerInput, PlayerInputs};
use crate::objects::ObjectLibrary;
use crate::sim::{Obj, Rng};
use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};
use bevy::prelude::*;
use payback_formats::replay::{Header, Input, NO_WEAPON_CHANGE, RandomSetup, Record, Replay, VERSION};
use std::path::PathBuf;

/// The recording buffer is 150000 bytes (one u16 per record); the
/// original silently stops recording when it is full.
pub const MAX_RECORDS: usize = 75_000;

/// Controller code of the keyboard (`Header::controllers`).
const KEYBOARD: i32 = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Record,
    Play,
}

/// The replay being recorded or played in the current level.
#[derive(Resource, Default)]
pub struct ReplayState {
    pub mode: Mode,
    pub replay: Option<Replay>,
    /// Next record to play.
    pub cursor: usize,
    /// Game ticks left of the current record (records may span 1..5).
    left: u8,
    /// Playback: each player's in/out bit in the previous record.
    held: [bool; 4],
    /// Game ticks since the level started.
    pub tick: u32,
    /// Playback ran out of records or was stopped with Esc.
    pub finished: bool,
}

impl ReplayState {
    pub fn playing(&self) -> bool {
        self.mode == Mode::Play
    }
}

/// The last game played, for the game menu's "View replay" / "Save
/// replay" (the original keeps it in the recording buffer).
#[derive(Resource, Default)]
pub struct LastReplay(pub Option<Replay>);

/// Set by the menus before entering a level: play this replay instead of
/// recording.
#[derive(Resource, Default)]
pub struct PlayRequest(pub Option<Replay>);

/// Random phone missions are on in this level (Rampage with the "Random
/// missions" setting, `Variants::random_missions`; the Mac always plays
/// them, `ProcessTrigger_Random`). Set at level start by [`begin`], before
/// `LevelLoaded`: from the settings when recording, from the replay header
/// when playing back, since the missions change the simulation.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RandomMissions(pub bool);

/// `--again`: re-enter the level once the menus have been entered.
#[derive(Resource)]
struct Again;

/// Input is read (recorded or played back) before anyone uses it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplayInput;

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        // Deterministic system order in the fixed-step schedules and at
        // level start (randomness is drawn there too).
        for label in [
            FixedFirst.intern(),
            FixedPreUpdate.intern(),
            FixedUpdate.intern(),
            FixedPostUpdate.intern(),
            FixedLast.intern(),
            OnEnter(AppState::Playing).intern(),
        ] {
            app.edit_schedule(label, |s| {
                s.set_executor(SingleThreadedExecutor::new());
            });
        }
        if let Some(r) = crate::arg("--play").and_then(|p| load_file(&PathBuf::from(&p)).ok()) {
            if let Some((spec, mode)) = launch_of(&r) {
                app.insert_resource(spec).insert_resource(mode);
            }
            app.insert_resource(PlayRequest(Some(r)));
        } else {
            app.init_resource::<PlayRequest>();
        }
        app.init_resource::<ReplayState>()
            .init_resource::<LastReplay>()
            .init_resource::<RandomMissions>()
            .add_systems(OnEnter(AppState::Playing), begin.before(LevelLoaded))
            .add_systems(OnExit(AppState::Playing), end)
            .configure_sets(FixedUpdate, ReplayInput.before(PeopleThink))
            .add_systems(
                FixedUpdate,
                (scripted_input, feed, debug_hash)
                    .chain()
                    .in_set(ReplayInput)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, stop_on_escape.run_if(in_state(AppState::Playing)))
            .add_systems(
                OnEnter(AppState::Frontend),
                |mut commands: Commands, again: Option<Res<Again>>, mut next: ResMut<NextState<AppState>>| {
                    if again.is_some() {
                        commands.remove_resource::<Again>();
                        next.set(AppState::Playing);
                    }
                },
            );
    }
}

/// Read a replay file.
pub fn load_file(path: &std::path::Path) -> Result<Replay, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Replay::parse(&b).map_err(|e| format!("{}: {e}", path.display()))
}

/// The level a replay was recorded on: `PROGDIR:Maps/<n>` or
/// `PROGDIR:UserMaps/<name>` (taken as `UserMaps/<name>` in the game
/// data), level within the city, and Rampage with its police
/// aggressiveness.
pub fn launch_of(r: &Replay) -> Option<(LevelSpec, GameMode)> {
    launch_with(r, |_| None)
}

/// [`launch_of`], finding user maps with `user_map(name)` first (the Mac's
/// `GetCityNumFromFilename` @ 10084d20 looks the path up in the user-map
/// list).
pub fn launch_with(r: &Replay, user_map: impl Fn(&str) -> Option<LevelSpec>) -> Option<(LevelSpec, GameMode)> {
    let level = (r.header.level & 1) as u8;
    let spec = match r.user_map() {
        Some(name) => LevelSpec {
            level,
            ..user_map(name).unwrap_or_else(|| LevelSpec { map: format!("UserMaps/{name}"), level })
        },
        // Older files of ours put a user map's path after `PROGDIR:Maps/`.
        None => LevelSpec {
            map: r.header.map.strip_prefix("PROGDIR:Maps/")?.to_string(),
            level,
        },
    };
    let mode = if r.header.players > 1 {
        GameMode::Multiplayer {
            players: r.header.players.clamp(2, 4) as u8,
            kills: r.header.required_kills.clamp(1, 50) as u8,
        }
    } else if r.header.rampage {
        GameMode::Rampage {
            police: r.header.police.clamp(0, 5) as u8,
        }
    } else {
        GameMode::Story
    };
    Some((spec, mode))
}

/// The map path stored in a replay (`GetCityFilename` @ 10084c90):
/// `PROGDIR:Maps/<n>` for a city, `PROGDIR:UserMaps/<name>` for a user map.
pub fn map_path(spec: &LevelSpec) -> String {
    match spec.city() {
        Some(c) => format!("PROGDIR:Maps/{c}"),
        None if spec.is_user_map() => {
            let name = spec.map.rsplit(['/', '\\']).next().unwrap_or(&spec.map);
            format!("PROGDIR:UserMaps/{name}")
        }
        None => format!("PROGDIR:Maps/{}", spec.map),
    }
}

/// Saved replays (`Saves/Replays/`; the original writes
/// `PROGDIR:Replays/%s`, here kept out of the game data).
pub fn saves_dir() -> PathBuf {
    crate::frontend::progress::saves_dir().join("Replays")
}

/// A fresh seed for a new game: from `--seed`, else the clock.
fn new_seed() -> u16 {
    crate::arg("--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos() as u16 ^ d.as_secs() as u16)
    })
}

/// Level start: seed the generator from the replay to play, or pick a
/// seed and start recording.
fn begin(
    mut commands: Commands,
    mut st: ResMut<ReplayState>,
    mut req: ResMut<PlayRequest>,
    mut rng: ResMut<Rng>,
    mut inputs: ResMut<PlayerInputs>,
    spec: Res<LevelSpec>,
    mode: Res<GameMode>,
    devices: Res<crate::multiplayer::MpDevices>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mut random_missions: ResMut<RandomMissions>,
) {
    *inputs = PlayerInputs::default();
    *st = ReplayState::default();
    // Game state that outlives a level must start fresh too, or a replay
    // viewed after playing would diverge (traffic lights, pickup respawns).
    commands.insert_resource(crate::sim::traffic::LightPhase::default());
    commands.insert_resource(crate::sim::combat::PickupRespawns::default());
    if let Some(r) = req.0.take() {
        rng.seed(r.header.seed as u16);
        // Played back as recorded, whatever the setting is now.
        random_missions.0 = r.header.rampage && r.header.random_missions;
        info!(
            "replay: playing {} records on {} level {}, seed {}",
            r.records.len(),
            r.header.map,
            r.header.level,
            r.header.seed
        );
        st.mode = Mode::Play;
        st.replay = Some(r);
        return;
    }
    let seed = new_seed();
    rng.seed(seed);
    let (rampage, police) = match *mode {
        GameMode::Rampage { police } => (true, police as i32),
        _ => (false, 2),
    };
    random_missions.0 = rampage && settings.as_ref().is_none_or(|s| s.variants.random_missions);
    let (players, controllers, required_kills) = match *mode {
        GameMode::Multiplayer { players, kills } => {
            let mut c = [-1; 4];
            for (p, c) in c.iter_mut().enumerate().take(players as usize) {
                // Original controller codes: 0..3 joystick n, 12 keyboard.
                *c = match devices.0.get(p) {
                    Some(crate::multiplayer::Device::Joystick(n)) => *n as i32,
                    _ => KEYBOARD,
                };
            }
            (players as i32, c, kills as i32)
        }
        _ => (1, [KEYBOARD, -1, -1, -1], 5),
    };
    st.mode = Mode::Record;
    st.replay = Some(Replay {
        header: Header {
            version: VERSION,
            players,
            controllers,
            required_kills,
            duration: 0,
            seed: seed as i32,
            rampage,
            police,
            map: map_path(&spec),
            level: spec.level as i32,
            random_missions: random_missions.0,
            // Filled in by `random_setup` when the random missions start.
            random_setup: None,
        },
        records: Vec::new(),
    });
}

/// The random missions' outside inputs (money goal from the high-score
/// table, story progress from the profile): when playing back a replay that
/// recorded them, the recorded values; otherwise `computed`, which goes
/// into the recording. Older replays without them play with `computed`.
pub fn random_setup(world: &mut World, computed: RandomSetup) -> RandomSetup {
    let mut st = world.resource_mut::<ReplayState>();
    let mode = st.mode;
    match st.replay.as_mut() {
        Some(r) if mode == Mode::Play => r.header.random_setup.unwrap_or(computed),
        Some(r) if mode == Mode::Record => {
            r.header.random_setup = Some(computed);
            computed
        }
        _ => computed,
    }
}

/// Level end: keep the recording as the last game.
fn end(mut st: ResMut<ReplayState>, mut last: ResMut<LastReplay>) {
    if st.mode == Mode::Record {
        if let Some(mut r) = st.replay.take() {
            let players = r.header.players.max(1) as usize;
            r.header.duration = r.records.iter().step_by(players).map(|r| r.input.ticks as i32).sum();
            last.0 = Some(r);
        }
    }
    *st = ReplayState::default();
}

/// The player's controls as a record word (`FUN_01051a10`): weapon = the
/// HUD slot chosen this tick (0..7) or 10.
fn to_input(i: &PlayerInput) -> Input {
    Input {
        left: i.left,
        right: i.right,
        accelerate: i.accel,
        reverse: i.reverse,
        shoot: i.fire,
        use_vehicle: i.use_pressed,
        brake: i.brake,
        weapon: i.weapon_pressed.filter(|&w| w < 8).unwrap_or(NO_WEAPON_CHANGE),
        ticks: 1,
    }
}

/// Record this tick's controls, or replace them with the recorded ones
/// (one record per player per frame).
fn feed(mut st: ResMut<ReplayState>, mut inputs: ResMut<PlayerInputs>) {
    let st = &mut *st;
    st.tick += 1;
    let players = st.replay.as_ref().map_or(1, |r| r.header.players.clamp(1, 4) as usize);
    let inputs = &mut inputs.0[..players];
    match st.mode {
        Mode::Off => {}
        Mode::Record => {
            let Some(r) = &mut st.replay else { return };
            if r.records.len() + players <= MAX_RECORDS {
                for input in inputs.iter() {
                    r.records.push(Record {
                        input: to_input(input),
                        comment: None,
                    });
                }
            }
        }
        Mode::Play => {
            let Some(r) = &st.replay else { return };
            if st.left == 0 {
                let Some(frame) = r.records.get(st.cursor..st.cursor + players) else {
                    if !st.finished {
                        info!("replay: finished after {} ticks", st.tick - 1);
                    }
                    st.finished = true;
                    inputs.fill(PlayerInput::default());
                    return;
                };
                st.cursor += players;
                st.left = frame[0].input.ticks.max(1);
                for ((input, rec), held) in inputs.iter_mut().zip(frame).zip(&mut st.held) {
                    let i = rec.input;
                    // Bit 9 is the in/out key's held state in the
                    // originals' files (`FUN_01051a10`, Mac `ReadInput` @
                    // 10064bf0; `GetInOutVehicles` @ 10066140 takes the
                    // edge): a press is a rising edge. Ours hold one-tick
                    // presses, which read the same.
                    let use_pressed = i.use_vehicle && !*held;
                    *held = i.use_vehicle;
                    *input = PlayerInput {
                        left: i.left,
                        right: i.right,
                        accel: i.accelerate,
                        reverse: i.reverse,
                        fire: i.shoot,
                        brake: i.brake,
                        use_pressed,
                        weapon_pressed: (i.weapon < 8).then_some(i.weapon),
                    };
                }
            } else {
                // Later ticks of a multi-tick record: held keys only.
                for input in inputs.iter_mut() {
                    input.use_pressed = false;
                    input.weapon_pressed = None;
                }
            }
            st.left -= 1;
        }
    }
}

/// Esc or P stops playback (instead of opening the pause menu; Mac
/// `ReadPlaybackInput` @ 10064ad0 ends on status 5, set by both keys, as
/// OS4 `FUN_0103f3d0` does).
fn stop_on_escape(keys: Res<ButtonInput<KeyCode>>, mut st: ResMut<ReplayState>) {
    if st.playing() && keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyP]) {
        st.finished = true;
    }
}

/// `--script-input`: a fixed pattern of controls instead of the keyboard
/// (walk, turn, shoot, try a vehicle, switch weapon), for the determinism
/// check.
fn scripted_input(st: Res<ReplayState>, mut inputs: ResMut<PlayerInputs>) {
    if st.mode != Mode::Record || !std::env::args().any(|a| a == "--script-input") {
        return;
    }
    let players = st.replay.as_ref().map_or(1, |r| r.header.players.clamp(1, 4) as u32);
    for (p, input) in inputs.0.iter_mut().enumerate().take(players as usize) {
        // Other players run the same pattern, shifted.
        let t = st.tick + 1 + 37 * p as u32;
        *input = PlayerInput {
            accel: t % 200 < 150,
            left: (20..35).contains(&(t % 100)),
            right: (60..70).contains(&(t % 120)),
            fire: t % 50 < 5,
            brake: t % 300 > 290,
            reverse: false,
            use_pressed: t % 250 == 0,
            weapon_pressed: (t % 400 == 100).then_some(1),
        };
    }
}

/// Order-independent hash of every object's state (position, velocity,
/// heading, health, state, model) and the generator index. Models are
/// hashed by file name: library indices depend on when the renderer first
/// asked for a model (e.g. pose and damage variants), not on the game.
pub fn world_hash<'a>(objs: impl Iterator<Item = &'a Obj>, rng: u16, model_id: impl Fn(usize) -> i32) -> (u64, usize) {
    let mut v: Vec<[i32; 10]> = objs
        .map(|o| {
            [
                o.pos.x,
                o.pos.y,
                o.pos.z,
                o.vel.x,
                o.vel.y,
                o.vel.z,
                o.heading,
                o.health,
                o.state as i32,
                model_id(o.model),
            ]
        })
        .collect();
    v.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |x: i32| {
        for b in x.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for o in &v {
        o.iter().for_each(|&x| mix(x));
    }
    mix(rng as i32);
    (h, v.len())
}

/// With `--record` / `--play`: log the world hash every 50 ticks; at
/// `--ticks` write the recording (or stop the playback) and exit.
fn debug_hash(
    st: Res<ReplayState>,
    rng: Res<Rng>,
    objs: Query<&Obj>,
    library: Option<Res<ObjectLibrary>>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let record = crate::arg("--record");
    if record.is_none() && crate::arg("--play").is_none() {
        return;
    }
    let end = crate::arg("--ticks").and_then(|v| v.parse().ok()).unwrap_or(500u32);
    // `feed` has counted this tick; the hash is of the state the input is
    // about to act on, i.e. after `st.tick - 1` complete ticks.
    let done = st.tick - 1;
    if done % 50 == 0 || done == end {
        let path = |m: usize| library.as_ref().map_or("", |l| l.model(m).info.path.as_str()).to_string();
        let model_id = |m: usize| path(m).bytes().fold(0i32, |h, b| h.wrapping_mul(31).wrapping_add(b as i32));
        let (h, n) = world_hash(objs.iter(), rng.index(), model_id);
        info!("replay hash: tick {done} objects {n} rng {} hash {h:016x}", rng.index());
        // `--dump <prefix>`: every object, for diffing two runs.
        if let Some(prefix) = crate::arg("--dump") {
            let mut lines: Vec<String> = objs
                .iter()
                .map(|o| format!("{} {:?}", path(o.model), Obj { model: 0, ..o.clone() }))
                .collect();
            lines.sort();
            let _ = std::fs::write(format!("{prefix}.{done}"), lines.join("\n"));
        }
    }
    if done == end {
        if let (Some(path), Some(r)) = (record, &st.replay) {
            let mut r = r.clone();
            // The record of this tick is not part of the run.
            let players = r.header.players.max(1) as usize;
            r.records.truncate(done as usize * players);
            r.header.duration = (r.records.len() / players) as i32;
            match std::fs::write(&path, r.write()) {
                Ok(()) => info!("replay: wrote {} records to {path}", r.records.len()),
                Err(e) => error!("replay: {path}: {e}"),
            }
            // `--again`: leave the level and play the recording back in
            // this session (checks nothing leaks from one level to the
            // next).
            if st.mode == Mode::Record && std::env::args().any(|a| a == "--again") {
                info!("replay: playing back in the same session");
                commands.insert_resource(PlayRequest(Some(r)));
                commands.insert_resource(Again);
                next.set(AppState::Frontend);
                return;
            }
        }
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_round_trip() {
        let p = PlayerInput {
            left: true,
            fire: true,
            use_pressed: true,
            weapon_pressed: Some(3),
            ..default()
        };
        let i = to_input(&p);
        assert_eq!(Input::from_word(i.to_word()), i);
        assert_eq!(i.weapon, 3);
        assert_eq!(to_input(&PlayerInput::default()).weapon, NO_WEAPON_CHANGE);
    }

    #[test]
    fn launch() {
        let r = Replay {
            header: Header {
                version: VERSION,
                players: 1,
                controllers: [KEYBOARD, -1, -1, -1],
                required_kills: 5,
                duration: 0,
                seed: 9,
                rampage: true,
                police: 4,
                map: "PROGDIR:Maps/2".into(),
                level: 1,
                random_missions: true,
                random_setup: None,
            },
            records: vec![],
        };
        let (spec, mode) = launch_of(&r).unwrap();
        assert_eq!((spec.map.as_str(), spec.level), ("2", 1));
        assert_eq!(mode, GameMode::Rampage { police: 4 });
    }

    /// Recording stores the random missions' goal and progress; playback
    /// uses the stored ones whatever the files say now; an older replay
    /// without them plays with the current values.
    #[test]
    fn random_setup_recorded() {
        let mut header = Header {
            version: VERSION,
            players: 1,
            controllers: [KEYBOARD, -1, -1, -1],
            required_kills: 5,
            duration: 0,
            seed: 9,
            rampage: true,
            police: 2,
            map: "PROGDIR:Maps/1".into(),
            level: 0,
            random_missions: true,
            random_setup: None,
        };
        let now = RandomSetup { goal: 900_000, progress: 6 };
        let mut world = World::new();
        world.insert_resource(ReplayState {
            mode: Mode::Record,
            replay: Some(Replay { header: header.clone(), records: vec![] }),
            ..default()
        });
        assert_eq!(random_setup(&mut world, now), now);
        let st = world.resource::<ReplayState>();
        assert_eq!(st.replay.as_ref().unwrap().header.random_setup, Some(now));

        let then = RandomSetup { goal: 1_500_000, progress: 0 };
        header.random_setup = Some(then);
        world.insert_resource(ReplayState {
            mode: Mode::Play,
            replay: Some(Replay { header: header.clone(), records: vec![] }),
            ..default()
        });
        assert_eq!(random_setup(&mut world, now), then);

        header.random_setup = None;
        world.insert_resource(ReplayState {
            mode: Mode::Play,
            replay: Some(Replay { header, records: vec![] }),
            ..default()
        });
        assert_eq!(random_setup(&mut world, now), now);
    }

    #[test]
    fn map_paths() {
        let city = LevelSpec { map: "1".into(), level: 0 };
        assert_eq!(map_path(&city), "PROGDIR:Maps/1");
        let user = LevelSpec::user_map(std::path::Path::new("UserMaps/House Park.map"));
        assert_eq!(map_path(&user), "PROGDIR:UserMaps/House Park");
        let mut r = Replay {
            header: Header {
                version: VERSION,
                players: 1,
                controllers: [KEYBOARD, -1, -1, -1],
                required_kills: 5,
                duration: 0,
                seed: 9,
                rampage: false,
                police: 2,
                map: map_path(&user),
                level: 1,
                random_missions: false,
                random_setup: None,
            },
            records: vec![],
        };
        let (spec, _) = launch_of(&r).unwrap();
        assert_eq!((spec.map.as_str(), spec.level), ("UserMaps/House Park", 1));
        r.header.map = "PROGDIR:Maps/UserMaps/Old".into();
        assert_eq!(launch_of(&r).unwrap().0.map, "UserMaps/Old");
    }
}
