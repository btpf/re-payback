//! Payback reimplementation (Bevy).
//!
//! Usage: `payback [--data <dir>] [--mac-data <dir>] [--map <n>] [--level <0|1>] [--free]
//!                 [--rampage <police 0..5>] [--multiplayer <2..4> [--kills <n>]]
//!                 [--screenshot <out.png>] [--resolution <w>x<h>]
//!                 [--window-mode windowed|borderless|fullscreen]
//!                 [--record <file> | --play <file>] [--ticks <n>]
//!                 [--autoplay [--autoplay-ticks <n>]] [--fast <n>]`
//!
//! Also: `--mac-only` (ignore the Amiga data), `--hardware` / `--software`
//! (render mode), `--perspective original|experimental` (`camera::Chase`),
//! `--seed <n>`, `--intro` / `--no-intro`, `--pad-debug`,
//! `--cheat-menu`; the rest are test switches read where they are used.
//!
//! The game data is searched for (env, `--data`, `data/` next to the
//! executable, `payback-data.cfg`, `extracted/` above the executable or the
//! working directory, else a message to run `payback-extract`; `locate.rs`).
//! `PAYBACK_NO_DIALOG=1` disables the message boxes.
//! `--record`/`--play` are the replay determinism check (see `replay.rs`).
//! `--autoplay` plays the story level's missions (see `autoplay.rs`);
//! `--fast n` runs the game n times faster.
//! `--multiplayer` starts a split-screen match on `--map` (key bindings in
//! `multiplayer.rs`).
//!
//! Controls: arrows move the player (keys in the control setup). F6 toggles the free camera
//! (WASD pan, Q/E rotate, R/F tilt, wheel or Z/X zoom).

mod arrow;
mod audio;
mod autoplay;
mod camera;
mod cheats;
mod city;
mod data;
mod debris;
mod effects;
mod frame;
mod frontend;
mod hud;
mod intro;
mod level;
mod lighting;
#[cfg(not(target_arch = "wasm32"))]
mod locate;
mod mapedit;
mod mission;
mod music;
mod multiplayer;
mod objects;
mod parked;
mod perf;
mod rampage;
mod render_mode;
mod replay;
mod sim;
mod startup;
mod vehicles;
mod vfs;
mod water;
mod weather;
#[cfg(target_arch = "wasm32")]
mod web;
mod window_icon;
mod world;

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use data::GameData;

pub fn arg(name: &str) -> Option<String> {
    args().skip_while(|a| a != name).nth(1)
}

/// The command line; in the web build the page's query string instead
/// (`?map=3&level=1&intro` = `--map 3 --level 1 --intro`, `web.rs`).
pub fn args() -> impl Iterator<Item = String> {
    static ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    ARGS.get_or_init(|| {
        #[cfg(target_arch = "wasm32")]
        return web::query_args();
        #[cfg(not(target_arch = "wasm32"))]
        std::env::args().collect()
    })
    .clone()
    .into_iter()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> AppExit {
    run_game()
}

/// Web build: nothing runs until the page has loaded the game data and
/// calls `web::start` (`web/index.html`).
#[cfg(target_arch = "wasm32")]
fn main() {}

fn run_game() -> AppExit {
    let loaded = GameData::locate().and_then(|root| GameData::load(&root));
    let data = match loaded {
        Ok(d) => d,
        Err(e) => {
            // Shown in a message box too: the game may run without a console.
            #[cfg(not(target_arch = "wasm32"))]
            locate::report_error(&e);
            #[cfg(target_arch = "wasm32")]
            eprintln!("{e}");
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // Saved resolution and window mode (`--resolution`, `--window-mode`).
            primary_window: Some({
                #[allow(unused_mut)]
                let mut w = frontend::display::startup_window("Payback");
                #[cfg(target_arch = "wasm32")]
                web::adapt_window(&mut w);
                w
            }),
            ..default()
        }))
        .insert_resource(data)
                .insert_resource(level::LevelSpec {
            map: arg("--map").unwrap_or_else(|| "0".into()),
            level: arg("--level").and_then(|v| v.parse().ok()).unwrap_or(0),
        })
        // One game tick is 1/30 s, as on the Mac (`amiga_levelloop`
        // @ 10100f50: `TickCount()` halved) and OS4 (`FUN_0110761c`); the 68k
        // Update 7 runs 1/25 s (docs/re/people.md §1.1). Per-tick rules are
        // the same in all builds.
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(ClearColor(Color::srgb(0.35, 0.55, 0.85)))
        .add_plugins((
            level::LevelPlugin,
            objects::SceneryPlugin,
            mapedit::MapEditPlugin,
            camera::CameraPlugin,
            sim::SimPlugin,
            (audio::AudioPlugin, music::MusicPlugin),
            water::WaterPlugin,
            (effects::EffectsPlugin, weather::WeatherPlugin, debris::DebrisPlugin),
            hud::HudPlugin,
            frontend::FrontendPlugin,
            mission::MissionPlugin,
            (intro::IntroPlugin, startup::StartupPlugin, frame::FramePlugin),
            (parked::ParkedPlugin, window_icon::WindowIconPlugin, render_mode::RenderModePlugin),
            (rampage::RampagePlugin, replay::ReplayPlugin, multiplayer::MultiplayerPlugin),
            (autoplay::AutoplayPlugin, perf::PerfPlugin, arrow::ArrowPlugin, cheats::CheatsPlugin),
        ))
                .add_systems(Update, screenshot_and_exit)
        .add_systems(Startup, data::log_data_roots)
        // `--map` starts a level directly; otherwise the menus come first.
        .add_systems(Startup, |mut next: ResMut<NextState<level::AppState>>| {
            if arg("--map").is_some() || arg("--play").is_some() {
                next.set(level::AppState::Playing);
            }
        })
        .run()
}

/// With `--screenshot <path>`, capture a frame once rendering has settled
/// (6 s, or `--screenshot-delay <secs>`) and exit. Used for automated visual checks.
fn screenshot_and_exit(mut commands: Commands, time: Res<Time<Real>>, mut taken: Local<bool>) {
    let Some(path) = arg("--screenshot") else {
        return;
    };
    // Render pipelines compile asynchronously; give them time to be ready
    // (`--screenshot-delay <secs>` waits longer).
    let delay = arg("--screenshot-delay").and_then(|s| s.parse::<f32>().ok()).unwrap_or(6.0);
    if !*taken && time.elapsed_secs() > delay {
        *taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
}
