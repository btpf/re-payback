//! Payback reimplementation (Bevy).
//!
//! Usage: `payback [--data <dir>] [--map <n>] [--level <0|1>] [--free]
//!                 [--screenshot <out.png>]`
//!
//! Controls: arrows/WASD move the player. F2 toggles the free camera
//! (WASD pan, Q/E rotate, R/F tilt, wheel or Z/X zoom).

mod audio;
mod camera;
mod city;
mod data;
mod effects;
mod frontend;
mod hud;
mod intro;
mod level;
mod mapedit;
mod mission;
mod objects;
mod sim;
mod vehicles;
mod water;
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
            primary_window: Some(Window {
                title: "Payback".into(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(data)
                .insert_resource(level::LevelSpec {
            map: arg("--map").unwrap_or_else(|| "0".into()),
            level: arg("--level").and_then(|v| v.parse().ok()).unwrap_or(0),
        })
        // One game tick is 1/25 s on the Amiga (the OS4 port uses 1/30 s;
        // docs/re/people.md).
        .insert_resource(Time::<Fixed>::from_hz(25.0))
        .insert_resource(ClearColor(Color::srgb(0.35, 0.55, 0.85)))
        .add_plugins((
            level::LevelPlugin,
            objects::SceneryPlugin,
            mapedit::MapEditPlugin,
            camera::CameraPlugin,
            sim::SimPlugin,
            audio::AudioPlugin,
            water::WaterPlugin,
            effects::EffectsPlugin,
            hud::HudPlugin,
            frontend::FrontendPlugin,
            mission::MissionPlugin,
            intro::IntroPlugin,
        ))
                .add_systems(Update, screenshot_and_exit)
        // `--map` starts a level directly; otherwise the menus come first.
        .add_systems(Startup, |mut next: ResMut<NextState<level::AppState>>| {
            if arg("--map").is_some() {
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
