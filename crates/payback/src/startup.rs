//! Start-up loading behind the `Loading` panel (68k `FUN_000ec686`:
//! InitProgress `FUN_000cc0c2("startup.size")` at 0xed4fe right after the
//! fade, the loaders calling DrawProgress `FUN_000cc456`, the intro at
//! 0xeda52 after the cube textures, then CloseProgress inline at the end;
//! OS4 `FUN_0104feb0` with InitProgress `FUN_01042ff8`; docs/re/
//! frontend_menus.md §10, frontend_audio.md §6.3).
//!
//! Booting into the menus, the asset loads that used to run at `Startup`
//! run here one stage per frame while the front end shows the panel and
//! fills its 12 pieces ([`crate::level::Progress`], units = microseconds
//! of work, self-calibrating through `Saves/startup.size` like the level
//! loading bar with `map.size`). The intro plays at the end of loading,
//! before CloseProgress; the menus start once it has finished. Started
//! directly into a level (`--map`, `--play`) every stage runs at once in
//! `Startup` and no panel is shown. `--load-delay <ms>` (debug) idles
//! between stages.

use crate::data::GameData;
use crate::intro::{IntroFinished, PlayIntro};
use crate::level::Progress;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

/// Stages: palette tables, sounds, effect textures, object textures, menu
/// models. The original's loaders between InitProgress and the intro
/// include the paint textures (`FUN_000955e2`), effect textures
/// (`FUN_00038e38`), object textures (`FUN_000952c2`), the objects
/// (`FUN_000d7874`) and the cube textures (`FUN_00043344`).
const STAGES: usize = 5;

/// A start-up load in progress (present only while the panel shows).
#[derive(Resource)]
pub struct StartupLoad {
    stage: usize,
    /// Frames since the panel was requested; the first stage waits for the
    /// panel to be on screen (InitProgress draws before anything loads).
    frames: u32,
    pub progress: Progress,
    intro: IntroStep,
    delay: std::time::Duration,
    wait_until: Option<std::time::Instant>,
}

enum IntroStep {
    /// Not reached yet.
    Pending,
    /// Requested; waiting for [`IntroFinished`].
    Playing(MessageCursor<IntroFinished>),
}

impl StartupLoad {
    /// Filled pieces, 0..=12.
    pub fn pieces(&self) -> usize {
        self.progress.pieces()
    }

    /// The intro is (about to be) on screen.
    pub fn intro_playing(&self) -> bool {
        matches!(self.intro, IntroStep::Playing(_))
    }
}

pub struct StartupPlugin;

impl Plugin for StartupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, begin)
            .add_systems(Update, advance.before(crate::frontend::FrontendRun));
    }
}

/// Booting straight into a level or a replay (no menus first).
fn direct_start() -> bool {
    crate::arg("--map").is_some() || crate::arg("--play").is_some()
}

/// The start-up intro (`FUN_000ec686` plays it unconditionally; here not
/// with `--map`, `--screenshot` or `--no-intro`, unless `--intro`).
fn intro_wanted() -> bool {
    let skip = ["--map", "--screenshot", "--no-intro"]
        .iter()
        .any(|a| std::env::args().any(|x| x == *a));
    !skip || std::env::args().any(|x| x == "--intro")
}

fn begin(world: &mut World) {
    if direct_start() {
        // The menu models (stages 3, 4) load on first use then.
        for stage in 0..3 {
            run_stage(world, stage);
        }
        return;
    }
    let progress = Progress::init(world.resource::<GameData>(), "startup.size");
    let ms = crate::arg("--load-delay").and_then(|v| v.parse().ok()).unwrap_or(0);
    world.insert_resource(StartupLoad {
        stage: 0,
        frames: 0,
        progress,
        intro: IntroStep::Pending,
        delay: std::time::Duration::from_millis(ms),
        wait_until: None,
    });
}

fn run_stage(world: &mut World, stage: usize) {
    let result = match stage {
        // The palette tables of the final colour snap.
        0 => world.run_system_once(crate::camera::postfx::build_tables),
        1 => world.run_system_once(crate::audio::load_sounds),
        // Effect textures and batches.
        2 => {
            crate::effects::load(world);
            Ok(())
        }
        // Object textures (the menu models' library).
        3 => {
            crate::frontend::preload_textures(world);
            Ok(())
        }
        // The menu models.
        _ => {
            crate::frontend::preload_models(world);
            Ok(())
        }
    };
    if let Err(e) = result {
        error!("start-up stage {stage}: {e}");
    }
}

/// One stage per frame; then the intro; then CloseProgress.
fn advance(world: &mut World) {
    let Some(mut load) = world.remove_resource::<StartupLoad>() else {
        return;
    };
    load.frames += 1;
    let now = std::time::Instant::now();
    if load.frames < 3 || load.wait_until.is_some_and(|t| now < t) {
        world.insert_resource(load);
        return;
    }
    if load.stage < STAGES {
        load.wait_until = Some(now + load.delay);
        run_stage(world, load.stage);
        load.stage += 1;
        load.progress.draw(now.elapsed().as_micros() as u64);
        world.insert_resource(load);
        return;
    }
    match &mut load.intro {
        IntroStep::Pending if intro_wanted() => {
            let cursor = world.resource::<Messages<IntroFinished>>().get_cursor();
            world.write_message(PlayIntro);
            load.intro = IntroStep::Playing(cursor);
            world.insert_resource(load);
            return;
        }
        IntroStep::Playing(cursor) => {
            let messages = world.resource::<Messages<IntroFinished>>();
            if cursor.read(messages).count() == 0 {
                world.insert_resource(load);
                return;
            }
        }
        IntroStep::Pending => {}
    }
    // CloseProgress: the resource stays removed, so the menus start.
    load.progress.close();
}
