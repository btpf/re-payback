//! Payback reimplementation (Bevy).
//!
//! Usage: `payback [--data <dir>] [--map <n>] [--level <0|1>] [--free]
//!                 [--rampage <police 0..5>] [--multiplayer <2..4> [--kills <n>]]
//!                 [--screenshot <out.png>] [--resolution <w>x<h>]
//!                 [--window-mode windowed|borderless|fullscreen]
//!                 [--record <file> | --play <file>] [--ticks <n>]
//!                 [--autoplay [--autoplay-ticks <n>]] [--fast <n>]`
//!
//! `--record`/`--play` are the replay determinism check (see `replay.rs`).
//! `--autoplay` plays the story level's missions (see `autoplay.rs`);
//! `--fast n` runs the game n times faster.
//! `--multiplayer` starts a split-screen match on `--map` (key bindings in
//! `multiplayer.rs`).
//!
//! Controls: arrows/WASD move the player. F2 toggles the free camera
//! (WASD pan, Q/E rotate, R/F tilt, wheel or Z/X zoom).

mod audio;
mod autoplay;
mod camera;
mod city;
mod data;
mod debris;
mod effects;
mod frontend;
mod hud;
mod intro;
mod level;
mod lighting;
mod mapedit;
mod mission;
mod music;
mod multiplayer;
mod objects;
mod parked;
mod rampage;
mod render_mode;
mod replay;
mod sim;
mod startup;
mod vehicles;
mod water;
mod weather;
mod window_icon;
mod world;

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use data::GameData;

pub fn arg(name: &str) -> Option<String> {
    std::env::args().skip_while(|a| a != name).nth(1)
}

fn main() -> AppExit {
    let loaded = GameData::locate().and_then(|root| GameData::load(&root));
    let data = match loaded {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // Saved resolution and window mode (`--resolution`, `--window-mode`).
            primary_window: Some(frontend::display::startup_window("Payback")),
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
            (intro::IntroPlugin, startup::StartupPlugin),
            (parked::ParkedPlugin, window_icon::WindowIconPlugin, render_mode::RenderModePlugin),
            (rampage::RampagePlugin, replay::ReplayPlugin, multiplayer::MultiplayerPlugin),
            autoplay::AutoplayPlugin,
        ))
                .add_systems(Update, screenshot_and_exit)
        // `--map` starts a level directly; otherwise the menus come first.
        .add_systems(Startup, |mut next: ResMut<NextState<level::AppState>>| {
            if arg("--map").is_some() || arg("--play").is_some() {
                next.set(level::AppState::Playing);
            }
        })
        .run()
}

/// With `--screenshot <path>`, capture a frame once rendering has settled
/// and exit. Used for automated visual checks.
fn screenshot_and_exit(mut commands: Commands, time: Res<Time<Real>>, mut taken: Local<bool>) {
    let Some(path) = arg("--screenshot") else {
        return;
    };
    // Render pipelines compile asynchronously; give them time to be ready.
    if !*taken && time.elapsed_secs() > 6.0 {
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
