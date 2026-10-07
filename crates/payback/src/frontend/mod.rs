//! The front end: title and menus, level select, level results, pause
//! menu, saves and settings (docs/re/frontend_menus.md; main loop OS4
//! `FUN_010692bc`, front-end init `FUN_010138dc`).
//!
//! Each original screen is a modal loop; here screens form a stack whose
//! top is ticked at 30 Hz ([`ui::MENU_HZ`]) and drawn every frame into two
//! 256-line palette canvases like the HUD: the back layer (title picture,
//! darkened panels) on the game camera, and the front layer (text, icons)
//! on a second camera that also draws the spinning option model between
//! the two.
//!
//! Debug: `--frontend-screen <name>` opens a screen directly (main, game,
//! levelselect, complete, gameover, gamecomplete, name, load, save,
//! replays, credits, language, controls, cd, rampage, highscore,
//! replayinfo (with `--replay-file <name in Saves/Replays>`); `loading`
//! starts level 0.0
//! through the loading panel; with `--map`, `pause` opens the pause menu
//! after 3 s and `levelend` completes the level at once).

pub mod assets;
mod levels;
mod menus;
mod misc;
mod model;
mod multiplayer;
pub mod progress;
mod rampage;
mod replays;
pub mod ui;

use crate::audio::{FxVolume, Sound, SoundPos};
use crate::camera::GameCamera;
use crate::data::GameData;
use crate::hud::icons::HudIcons;
use crate::hud::paint::{CANVAS_H, Painter, blank, canvas_width};
use crate::hud::{Language, Texts};
use crate::level::{AppState, GameMode, LevelSpec};
use crate::rampage::ScoreHighWater;
use crate::replay::{LastReplay, PlayRequest, ReplayState};
use payback_formats::replay::Replay;
use crate::mission::{LevelClock, LevelOutcome, MissionState};
use assets::FrontendIcons;
use bevy::{
    ecs::system::SystemParam,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    window::PrimaryWindow,
};
use progress::{Profile, Settings};
use ui::*;

/// Seconds between the level's outcome and the result screen, so the
/// in-game "Level complete" / "Game over" message can be seen.
const END_DELAY: f32 = 4.0;

/// Phrase-file languages found at start-up (InitLanguages `FUN_0101345c`).
#[derive(Resource, Default)]
pub struct Languages(pub Vec<String>);

#[derive(Resource, Default)]
struct Frontend {
    stack: Vec<Box<dyn Screen>>,
    acc: f64,
    frame: u32,
    /// Keys gathered since the last tick.
    pending: Input,
    /// Waiting for the intro to finish before the menus start.
    wait_intro: bool,
    /// Level to load once the loading panel has been shown, and the
    /// frames it has been up.
    loading: Option<(Launch, u32)>,
    /// `--frontend-screen pause` has been opened.
    debug_paused: bool,
    /// Build the menu stack on the next update.
    needs_init: bool,
    /// The canvases are spawned (the `Layers` resource may lag a frame).
    layers_open: bool,
}

/// The two canvases and their nodes, alive while a menu is shown.
#[derive(Resource)]
struct Layers {
    back: Handle<Image>,
    front: Handle<Image>,
    back_px: Vec<u8>,
    front_px: Vec<u8>,
    w: usize,
    entities: Vec<Entity>,
}

#[derive(Component)]
struct LayerNode;

/// What to start once the loading panel is up.
#[derive(Clone)]
struct Launch {
    spec: LevelSpec,
    mode: GameMode,
    replay: Option<Replay>,
    /// After a replay, back to the game menu (else the main menu).
    back_to_game: bool,
    /// Multiplayer: the players' devices.
    devices: Option<Vec<crate::multiplayer::Device>>,
}

impl Launch {
    fn story(city: u8, level: u8) -> Launch {
        Launch {
            spec: LevelSpec {
                map: city.to_string(),
                level,
            },
            mode: GameMode::Story,
            replay: None,
            back_to_game: false,
            devices: None,
        }
    }
}

/// Where the menus pick up after a replay.
#[derive(Resource, Clone, Copy, Default)]
struct ReplayReturn {
    back_to_game: bool,
}

/// How the last level ended, for the result screen.
#[derive(Resource, Clone, Copy, Debug)]
struct LevelResult {
    /// None: a replay finished or was stopped.
    outcome: Option<LevelOutcome>,
    minutes: i32,
    city: Option<(usize, usize)>,
    /// Rampage: (city, police aggressiveness, score `DAT_0114c420`).
    rampage: Option<(usize, u8, i32)>,
    back_to_game: bool,
    /// Multiplayer: (result, players); result = winner, -1 draw,
    /// -2 aborted (`FUN_01027330`).
    mp: Option<(i32, usize)>,
}

/// Outcome seen, waiting for [`END_DELAY`].
#[derive(Resource, Default)]
struct PendingEnd(Option<(LevelResult, f32)>);

pub struct FrontendPlugin;

impl Plugin for FrontendPlugin {
    fn build(&self, app: &mut App) {
        let settings = Settings::load();
        let language = if settings.language.is_empty() {
            Language::default()
        } else {
            Language(settings.language.clone())
        };
        app.insert_resource(settings)
            .insert_resource(language)
            .init_resource::<Profile>()
            .init_resource::<Frontend>()
            .init_resource::<PendingEnd>()
            .init_resource::<ReplayReturn>()
            .init_resource::<model::CurrentModel>()
            .init_resource::<model::ModelCache>()
            .add_systems(Startup, setup)
            .add_systems(PostStartup, default_ui_camera)
            .add_systems(OnEnter(AppState::Frontend), enter_frontend)
            .add_systems(OnExit(AppState::Frontend), close_layers)
            .add_systems(OnExit(AppState::Playing), |mut pending: ResMut<PendingEnd>, mut time: ResMut<Time<Virtual>>| {
                pending.0 = None;
                time.unpause();
            })
            .add_systems(
                Update,
                (debug_level_end, watch_level_end)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, (run, model::sync_model).chain())
            // The pause menu's FX option drives the lens flares (`--fx` wins).
            .add_systems(Update, |settings: Res<Settings>, mut mode: ResMut<crate::effects::FxMode>| {
                if settings.is_changed() && crate::arg("--fx").is_none() && mode.0 != settings.fx {
                    mode.0 = settings.fx;
                }
            });
    }
}

fn setup(mut commands: Commands, data: Res<GameData>, mut fe: ResMut<Frontend>) {
    commands.insert_resource(FrontendIcons::load(&data));
    // Phrases/Global.<language>.phrases, keeping the middle token.
    let mut langs: Vec<String> = std::fs::read_dir(data.root.join("Phrases"))
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let mid = n.strip_prefix("Global.")?.strip_suffix(".phrases")?;
                    Some(mid.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    langs.sort_by_key(|l| l.to_lowercase());
    commands.insert_resource(Languages(langs));
    // Same condition as the intro's start-up play.
    let skip = ["--map", "--screenshot", "--no-intro"]
        .iter()
        .any(|a| std::env::args().any(|x| x == *a));
    fe.wait_intro = !skip || std::env::args().any(|x| x == "--intro");
}

/// Keep the HUD and intro overlay on the game camera when the menu camera
/// (higher order) exists.
fn default_ui_camera(mut commands: Commands, cams: Query<Entity, With<GameCamera>>) {
    for e in &cams {
        commands.entity(e).insert(IsDefaultUiCamera);
    }
}

fn debug_screen(name: &str, cx: &mut Ctx) -> Option<Box<dyn Screen>> {
    Some(match name {
        "main" => Box::new(menus::MainMenu::running()),
        "game" => Box::new(menus::GameMenu::new()),
        "levelselect" => Box::new(levels::LevelSelect::new(cx.profile)),
        "complete" => Box::new(levels::LevelComplete::new(cx.profile, 0, 0, 9)),
        "gameover" => Box::new(levels::GameOver::new(13)),
        "gamecomplete" => Box::new(levels::GameComplete::new()),
        "name" => Box::new(misc::TextEntry::new(cx.g(2088), "Travis", 25)),
        "load" => Box::new(misc::LoadGame::new()),
        "save" => Box::new(misc::SaveGame::new()),
        "replays" => Box::new(replays::ReplayList::new(cx)),
        "rampage" => Box::new(rampage::RampageSetup::new(cx)),
        "multiplayer" => Box::new(multiplayer::MultiplayerSetup::new(cx)),
        "mpresults" => Box::new(multiplayer::MpResults::new(cx, 1, 3)),
        "highscore" => Box::new(misc::TextEntry::new(cx.g(1611), "Travis", 25)),
        "replayinfo" => {
            let name = crate::arg("--replay-file")?;
            let r = crate::replay::load_file(&crate::replay::saves_dir().join(&name)).ok()?;
            Box::new(replays::ReplayInfo::new(r, Some(name)))
        }
        "credits" => Box::new(misc::Credits::new(2200, 2, true, 0x2e, 100)),
        "language" => Box::new(misc::LanguageChooser::new(cx)),
        "controls" => Box::new(misc::ControlSetup::new()),
        "cd" => Box::new(misc::CdSetup::new()),
        _ => return None,
    })
}

/// Entering the menus (also at boot, before Startup): the stack is built
/// by the first `run`, once assets and phrases are loaded.
fn enter_frontend(mut fe: ResMut<Frontend>) {
    fe.stack.clear();
    fe.loading = None;
    fe.needs_init = true;
}

fn init_stack(fe: &mut Frontend, cx: &mut Ctx, result: Option<LevelResult>) {
    if let Some(r) = result {
        // After a level the main menu skips straight into the game menu
        // (`DAT_0118ff28`), with the result screen first.
        let Some(outcome) = r.outcome else {
            // After a replay: back where it was started from.
            if r.back_to_game {
                fe.stack.push(Box::new(menus::MainMenu::running()));
                fe.stack.push(Box::new(menus::GameMenu::new()));
            } else {
                fe.stack.push(Box::new(menus::MainMenu::new()));
            }
            return;
        };
        cx.profile.played = true;
        fe.stack.push(Box::new(menus::MainMenu::running()));
        if let Some((result, players)) = r.mp {
            fe.stack.push(Box::new(menus::GameMenu::new()));
            if let Some((c, _)) = r.city {
                cx.settings.rampage_city = c as u8;
            }
            fe.stack.push(Box::new(multiplayer::MpResults::new(cx, result, players)));
            return;
        }
        fe.stack.push(Box::new(match r.rampage {
            Some((c, p, score)) => menus::GameMenu::after_rampage(c, p, score),
            None => menus::GameMenu::new(),
        }));
        match (outcome, r.city) {
            (LevelOutcome::Complete, Some((c, l))) if r.rampage.is_none() => fe
                .stack
                .push(Box::new(levels::LevelComplete::new(cx.profile, c, l, r.minutes))),
            (LevelOutcome::GameOver, _) => fe.stack.push(Box::new(levels::GameOver::new(r.minutes))),
            _ => {}
        }
        return;
    }
    fe.stack.push(Box::new(menus::MainMenu::new()));
    let debug = crate::arg("--frontend-screen");
    if let Some(s) = debug.as_deref().and_then(|n| debug_screen(n, cx)) {
        fe.stack.push(s);
    } else if debug.as_deref() == Some("loading") {
        fe.loading = Some((Launch::story(0, 0), 0));
    } else if cx.settings.language.is_empty() {
        // No language configured: ask first (InitLanguages).
        fe.stack.push(Box::new(misc::LanguageChooser::new(cx)));
    }
}

fn open_layers(commands: &mut Commands, images: &mut Assets<Image>, game_cam: Entity) {
    let w = 320;
    let back = images.add(blank(w));
    let front = images.add(blank(w));
    let menu_cam = model::spawn_camera(commands);
    let node = |image: Handle<Image>, cam: Entity, z: i32| {
        (
            ImageNode::new(image),
            // 1 px overscan so no clear colour shows at the edges.
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(-1.0),
                right: Val::Px(-1.0),
                top: Val::Px(-1.0),
                bottom: Val::Px(-1.0),
                ..default()
            },
            UiTargetCamera(cam),
            GlobalZIndex(z),
            LayerNode,
        )
    };
    let b = commands.spawn(node(back.clone(), game_cam, 10)).id();
    let f = commands.spawn(node(front.clone(), menu_cam, 10)).id();
    commands.insert_resource(Layers {
        back,
        front,
        back_px: vec![0; w * CANVAS_H * 4],
        front_px: vec![0; w * CANVAS_H * 4],
        w,
        entities: vec![b, f, menu_cam],
    });
}

fn close_layers(
    mut commands: Commands,
    layers: Option<Res<Layers>>,
    mut current: ResMut<model::CurrentModel>,
    mut fe: ResMut<Frontend>,
) {
    if let Some(l) = layers {
        for &e in &l.entities {
            commands.entity(e).despawn();
        }
        commands.remove_resource::<Layers>();
    }
    current.0 = None;
    current.1 = None;
    fe.stack.clear();
    fe.loading = None;
    fe.layers_open = false;
}

/// `--frontend-screen levelend` with `--map`: complete the level at once.
fn debug_level_end(mut out: MessageWriter<LevelOutcome>, mut done: Local<bool>) {
    if !*done && crate::arg("--frontend-screen").as_deref() == Some("levelend") {
        *done = true;
        out.write(LevelOutcome::Complete);
    }
}

/// Remember the level's outcome and leave for the result screen after
/// [`END_DELAY`] (main loop `DAT_0114c730`: 3 = complete, 2 = game over).
/// A replay ends when its records run out or Esc is pressed, with no
/// result screen (`FUN_0104135c`).
#[allow(clippy::too_many_arguments)]
fn watch_level_end(
    mut commands: Commands,
    mut outcomes: MessageReader<LevelOutcome>,
    clock: Res<LevelClock>,
    spec: Res<LevelSpec>,
    mode: Res<GameMode>,
    high: Res<ScoreHighWater>,
    replay: Res<ReplayState>,
    back: Res<ReplayReturn>,
    time: Res<Time<Real>>,
    game: Res<crate::multiplayer::Match>,
    mut pending: ResMut<PendingEnd>,
    mut next: ResMut<NextState<AppState>>,
) {
    // The determinism check keeps the level running to its last tick.
    if crate::arg("--record").is_some() || crate::arg("--play").is_some() {
        outcomes.clear();
        return;
    }
    if replay.playing() {
        outcomes.clear();
        if replay.finished {
            commands.insert_resource(LevelResult {
                outcome: None,
                minutes: 0,
                city: None,
                rampage: None,
                back_to_game: back.back_to_game,
                mp: None,
            });
            next.set(AppState::Frontend);
        }
        return;
    }
    for &outcome in outcomes.read() {
        if pending.0.is_none() {
            let city = spec
                .map
                .parse::<usize>()
                .ok()
                .filter(|&c| c < 3)
                .map(|c| (c, spec.level.min(1) as usize));
            let rampage = match *mode {
                GameMode::Rampage { police } => city.map(|(c, _)| (c, police, high.0)),
                _ => None,
            };
            let mp = mode
                .is_multiplayer()
                .then(|| (game.result.unwrap_or(-2), mode.players()));
            let r = LevelResult {
                outcome: Some(outcome),
                minutes: clock.minutes() as i32,
                city,
                rampage,
                back_to_game: false,
                mp,
            };
            pending.0 = Some((r, time.elapsed_secs()));
        }
    }
    if let Some((r, at)) = pending.0 {
        if time.elapsed_secs() - at >= END_DELAY {
            pending.0 = None;
            commands.insert_resource(r);
            next.set(AppState::Frontend);
        }
    }
}

fn gather_input(keys: &ButtonInput<KeyCode>, typed: &mut MessageReader<KeyboardInput>, input: &mut Input) {
    input.left |= keys.just_pressed(KeyCode::ArrowLeft);
    input.right |= keys.just_pressed(KeyCode::ArrowRight);
    input.up |= keys.just_pressed(KeyCode::ArrowUp);
    input.down |= keys.just_pressed(KeyCode::ArrowDown);
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    input.enter |= enter;
    input.fire |= enter || keys.just_pressed(KeyCode::Space);
    input.back |= keys.just_pressed(KeyCode::Escape);
    input.erase |= keys.just_pressed(KeyCode::Backspace);
    for ev in typed.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if let Key::Character(s) = &ev.logical_key {
            input.typed.extend(s.chars());
        } else if ev.logical_key == Key::Space {
            input.typed.push(' ');
        }
    }
}

/// Everything the menu loop touches.
#[derive(SystemParam)]
struct Env<'w, 's> {
    commands: Commands<'w, 's>,
    time: Res<'w, Time<Real>>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    typed: MessageReader<'w, 's, KeyboardInput>,
    data: Res<'w, GameData>,
    texts: Res<'w, Texts>,
    profile: ResMut<'w, Profile>,
    settings: ResMut<'w, Settings>,
    language: ResMut<'w, Language>,
    languages: Res<'w, Languages>,
    mission: Res<'w, MissionState>,
    clock: Res<'w, LevelClock>,
    state: Res<'w, State<AppState>>,
    next: ResMut<'w, NextState<AppState>>,
    virtual_time: ResMut<'w, Time<Virtual>>,
    pending_end: Res<'w, PendingEnd>,
    result: Option<Res<'w, LevelResult>>,
    fx: ResMut<'w, FxVolume>,
    spec: ResMut<'w, LevelSpec>,
    mode: ResMut<'w, GameMode>,
    high: Res<'w, ScoreHighWater>,
    play_request: ResMut<'w, PlayRequest>,
    last_replay: Res<'w, LastReplay>,
    replay: Res<'w, ReplayState>,
    replay_return: ResMut<'w, ReplayReturn>,
    mp_totals: ResMut<'w, crate::multiplayer::MpTotals>,
    mp_devices: ResMut<'w, crate::multiplayer::MpDevices>,
    intro_req: MessageWriter<'w, crate::intro::PlayIntro>,
    intro_done: MessageReader<'w, 's, crate::intro::IntroFinished>,
    intro: Option<Res<'w, crate::intro::Intro>>,
    exit: MessageWriter<'w, AppExit>,
    images: ResMut<'w, Assets<Image>>,
    layers: Option<ResMut<'w, Layers>>,
    nodes: Query<'w, 's, (&'static mut ImageNode, &'static mut Visibility), With<LayerNode>>,
    icons: Option<Res<'w, HudIcons>>,
    fe_icons: Option<Res<'w, FrontendIcons>>,
    window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    /// The camera the menu's back layer is drawn on: the game camera, or
    /// the full-window overlay in split screen.
    game_cam: Query<'w, 's, Entity, (With<IsDefaultUiCamera>, With<Camera3d>)>,
    current: ResMut<'w, model::CurrentModel>,
}

/// The menu loop: input, ticks, requests, drawing.
fn run(mut fe: ResMut<Frontend>, mut env: Env) {
    let env = &mut env;
    if env.fx.0 != env.settings.volume {
        env.fx.0 = env.settings.volume;
    }
    if env.intro_done.read().count() > 0 {
        fe.wait_intro = false;
    }
    let intro_on = env.intro.is_some() || fe.wait_intro;
    let playing = *env.state.get() == AppState::Playing;

    // Esc in a level opens the pause menu (`FUN_0101a7dc`).
    let debug_pause = playing
        && !fe.debug_paused
        && crate::arg("--frontend-screen").as_deref() == Some("pause")
        && env.time.elapsed_secs() > 3.0;
    if playing
        && fe.stack.is_empty()
        && env.pending_end.0.is_none()
        && !intro_on
        && !env.replay.playing()
        && (env.keys.just_pressed(KeyCode::Escape) || debug_pause)
    {
        fe.debug_paused = true;
        env.virtual_time.pause();
        fe.stack.push(Box::new(menus::PauseMenu::new()));
        fe.pending = Input::default();
        fe.acc = 0.0;
        env.typed.clear();
        return;
    }

    let mut cx = Ctx {
        data: &env.data,
        texts: &env.texts,
        profile: &mut env.profile,
        settings: &mut env.settings,
        language: &mut env.language,
        languages: &env.languages.0,
        goal: env.mission.goal,
        level_ticks: env.clock.0,
        story: matches!(*env.mode, GameMode::Story),
        last_replay: env.last_replay.0.as_ref(),
        mp_totals: &mut env.mp_totals,
        sfx: Vec::new(),
        requests: Vec::new(),
    };
    if fe.needs_init && !playing {
        fe.needs_init = false;
        let result = env.result.as_deref().copied();
        if result.is_some() {
            env.commands.remove_resource::<LevelResult>();
        }
        init_stack(&mut fe, &mut cx, result);
    }
    if fe.stack.is_empty() && fe.loading.is_none() {
        env.typed.clear();
        return;
    }
    if !fe.layers_open {
        if let Ok(cam) = env.game_cam.single() {
            fe.layers_open = true;
            open_layers(&mut env.commands, &mut env.images, cam);
        }
    }

    let mut pending = std::mem::take(&mut fe.pending);
    gather_input(&env.keys, &mut env.typed, &mut pending);

    // Ticks at 30 Hz of real time (virtual time is paused in the pause
    // menu).
    if intro_on || fe.loading.is_some() {
        fe.acc = 0.0;
        pending = Input::default();
    } else {
        fe.acc = (fe.acc + env.time.delta_secs_f64() * MENU_HZ).min(4.0);
        while fe.acc >= 1.0 {
            fe.acc -= 1.0;
            fe.frame = fe.frame.wrapping_add(1);
            let input = std::mem::take(&mut pending);
            let Some(top) = fe.stack.last_mut() else { break };
            let mut step = top.tick(&mut cx, &input);
            loop {
                step = match step {
                    Step::Stay => break,
                    Step::Push(s) => {
                        fe.stack.push(s);
                        break;
                    }
                    Step::Replace(s) => {
                        fe.stack.pop();
                        fe.stack.push(s);
                        break;
                    }
                    Step::Pop(o) => {
                        fe.stack.pop();
                        match fe.stack.last_mut() {
                            Some(top) => top.resume(&mut cx, o),
                            None => break,
                        }
                    }
                };
            }
            if !cx.requests.is_empty() {
                break;
            }
        }
    }
    fe.pending = pending;

    for s in std::mem::take(&mut cx.sfx) {
        let (id, hz) = s.id_hz();
        env.commands.spawn(Sound::once(id, 255.0, hz, SoundPos::Global));
    }
    let mut close = false;
    for r in std::mem::take(&mut cx.requests) {
        match r {
            Request::StartLevel { city, level } => fe.loading = Some((Launch::story(city, level), 0)),
            Request::StartRampage { city, police } => {
                let mut l = Launch::story(city, 0);
                l.mode = GameMode::Rampage { police };
                fe.loading = Some((l, 0));
            }
            Request::StartMultiplayer { city } => {
                let devices = multiplayer::active_devices(cx.settings);
                let mut l = Launch::story(city, 0);
                l.mode = GameMode::Multiplayer {
                    players: devices.len() as u8,
                    kills: cx.settings.required_kills.clamp(1, 50),
                };
                l.devices = Some(devices);
                fe.loading = Some((l, 0));
            }
            Request::PlayReplay { replay, back_to_game } => {
                if let Some((spec, mode)) = crate::replay::launch_of(&replay) {
                    let replay = Some(*replay);
                    fe.loading = Some((Launch { spec, mode, replay, back_to_game, devices: None }, 0));
                }
            }
            Request::Exit => {
                env.exit.write(AppExit::Success);
            }
            Request::PlayIntro => {
                env.intro_req.write(crate::intro::PlayIntro);
                fe.wait_intro = true;
            }
            Request::Resume => {
                env.virtual_time.unpause();
                close = true;
            }
            Request::QuitToTitle => {
                env.virtual_time.unpause();
                close = true;
                env.next.set(AppState::Frontend);
                // A multiplayer match quit from the pause menu: "Match
                // aborted." on the results screen.
                if env.mode.is_multiplayer() && !env.replay.playing() {
                    env.commands.insert_resource(LevelResult {
                        outcome: Some(LevelOutcome::MatchOver),
                        minutes: 0,
                        city: None,
                        rampage: None,
                        back_to_game: false,
                        mp: Some((-2, env.mode.players())),
                    });
                }
                // A rampage quit early still goes through the high-score
                // check at the top of the game menu (`FUN_0102ca08`).
                if let GameMode::Rampage { police } = *env.mode {
                    if !env.replay.playing() {
                        let city = env.spec.map.parse::<usize>().ok().filter(|&c| c < 3);
                        env.commands.insert_resource(LevelResult {
                            outcome: Some(LevelOutcome::MatchOver),
                            minutes: 0,
                            city: city.map(|c| (c, 0)),
                            rampage: city.map(|c| (c, police, env.high.0)),
                            back_to_game: false,
                            mp: None,
                        });
                    }
                }
            }
        }
    }
    if close {
        fe.stack.clear();
        fe.layers_open = false;
        env.current.0 = None;
        env.current.1 = None;
        if let Some(l) = &env.layers {
            for &e in &l.entities {
                env.commands.entity(e).despawn();
            }
        }
        env.commands.remove_resource::<Layers>();
        return;
    }

    // Loading panel shown for a couple of frames, then the level loads
    // (the last frame stays on screen while `load_level` runs).
    if let Some((launch, n)) = &mut fe.loading {
        *n += 1;
        if *n == 3 {
            *env.spec = launch.spec.clone();
            *env.mode = launch.mode;
            if let Some(d) = &launch.devices {
                env.mp_devices.0 = d.clone();
            }
            env.replay_return.back_to_game = launch.back_to_game;
            if launch.replay.is_none() {
                cx.profile.played = true;
            }
            env.play_request.0 = launch.replay.take();
            env.next.set(AppState::Playing);
        }
    }

    // Draw.
    let (Some(layers), Some(icons), Some(fe_icons)) =
        (env.layers.as_deref_mut(), env.icons.as_deref(), env.fe_icons.as_deref())
    else {
        return;
    };
    let aspect = env
        .window
        .single()
        .map(|w| w.width() / w.height().max(1.0))
        .unwrap_or(1.25);
    let w = canvas_width(aspect);
    if layers.w != w {
        layers.w = w;
        layers.back_px = vec![0; w * CANVAS_H * 4];
        layers.front_px = vec![0; w * CANVAS_H * 4];
    }
    layers.back_px.fill(0);
    layers.front_px.fill(0);
    let top = fe.stack.last();
    let loading = fe.loading.is_some();
    {
        let mut ui = Ui {
            back: Painter {
                px: &mut layers.back_px,
                w,
                pal: &env.data.palette,
            },
            front: Painter {
                px: &mut layers.front_px,
                w,
                pal: &env.data.palette,
            },
            fonts: icons,
            fe: fe_icons,
            w: w as i32,
            frame: fe.frame,
        };
        match top {
            _ if loading => draw_loading(&mut ui),
            Some(top) => top.draw(&mut ui, &cx),
            None => {}
        }
    }
    for (handle, px) in [(&layers.back, &layers.back_px), (&layers.front, &layers.front_px)] {
        if let Some(mut img) = env.images.get_mut(handle) {
            if img.width() as usize != w {
                *img = blank(w);
            }
            if let Some(d) = img.data.as_mut() {
                d.copy_from_slice(px);
            }
        }
    }
    let shown = !intro_on;
    let b = if loading { 1.0 } else { top.map_or(1.0, |t| t.brightness()) };
    for (mut node, mut vis) in &mut env.nodes {
        node.color = Color::srgb(b, b, b);
        *vis = if shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    let top = top.filter(|_| shown && !loading);
    env.current.0 = top.and_then(|t| t.model());
    env.current.1 = top.and_then(|t| t.back_model());
}

/// InitProgress (`FUN_01042ff8`): clear a box round the centre and draw
/// `Loading` with its empty segments. The load itself is synchronous, so
/// no `LoadingPiece` is filled in.
fn draw_loading(ui: &mut Ui) {
    let (cx, cy) = ((ui.w - 1) / 2, (H - 1) / 2);
    for y in cy - 0x16..=cy + 0x17 {
        for x in cx - 0x36..=cx + 0x35 {
            ui.front.put_rgb(x, y, [0, 0, 0]);
        }
    }
    for y in 0..H {
        for x in 0..ui.w {
            ui.back.put_rgb(x, y, [0, 0, 0]);
        }
    }
    let fe = ui.fe;
    ui.icon(&fe.loading, cx - 0x2e, cy - 0x11);
}

