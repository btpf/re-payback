//! Single-player Rampage (docs/re/frontend_menus.md §7,
//! combat_police.md §7): free play on a city with no level script, 3
//! lives, score from kills, and a top-5 table per police aggressiveness.
//!
//! High scores are `Maps/<n>.scores` in the original (OS4 load
//! `FUN_01029b3c`, save `FUN_0102a0fc`). They are written to
//! `Saves/<n>.scores` here so the game data stays untouched; loading
//! falls back to the game data's `Maps/<n>.scores`, then to the defaults.

use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::level::{AppState, GameMode};
use crate::sim::Obj;
use bevy::prelude::*;
use payback_formats::scores::HighScores;
use std::path::PathBuf;

/// Highest score seen this level (`DAT_0114c420`, the HUD high-water
/// mark): the score is kept by the controlled object, which is gone after
/// the last death.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ScoreHighWater(pub i32);

pub struct RampagePlugin;

impl Plugin for RampagePlugin {
    fn build(&self, app: &mut App) {
        if let Some(p) = crate::arg("--rampage").and_then(|v| v.parse::<u8>().ok()) {
            app.insert_resource(GameMode::Rampage { police: p.min(5) });
        }
        app.init_resource::<ScoreHighWater>()
            .add_systems(OnEnter(AppState::Playing), |mut hw: ResMut<ScoreHighWater>| hw.0 = 0)
            .add_systems(
                FixedPostUpdate,
                track_score.run_if(in_state(AppState::Playing)),
            );
    }
}

fn track_score(mut hw: ResMut<ScoreHighWater>, player: Query<&Obj, With<CameraTarget>>) {
    for o in &player {
        hw.0 = hw.0.max(o.score);
    }
}

fn saved_path(city: usize) -> PathBuf {
    crate::frontend::progress::saves_dir().join(format!("{city}.scores"))
}

/// The table for `city`: our save, else the game's file, else defaults
/// (also when the checksum is wrong).
pub fn load_scores(data: &GameData, city: usize) -> HighScores {
    crate::frontend::progress::read_file(&saved_path(city))
        .ok()
        .and_then(|b| HighScores::parse(&b))
        .or_else(|| data.read(format!("Maps/{city}.scores")).ok().and_then(|b| HighScores::parse(&b)))
        .unwrap_or_default()
}

pub fn save_scores(city: usize, scores: &HighScores) {
    let path = saved_path(city);
    let res = crate::frontend::progress::write_file(&path, &scores.write());
    if let Err(e) = res {
        warn!("{}: {e}", path.display());
    }
}
