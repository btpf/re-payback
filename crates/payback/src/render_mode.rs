//! Software or hardware (Warp3D) rendering (docs/re/rendering.md,
//! "Hardware (Warp3D) renderer").
//!
//! The OS4 build carries both renderers and branches on `DAT_0114c7d8`
//! (config field 19, chosen in the launcher): the 8-bit chunky software
//! renderer with its `Alpha.256` / `Gouraud.256` tables, or Warp3D, which
//! draws bilinear-filtered true-colour textures, modulates them by the
//! Gouraud vertex colours and blends with real alpha / additive blend
//! modes. Hardware mode forces Gouraud shading on and refuses motion blur
//! (LoadConfig `FUN_0104199c`, pause menu `FUN_0101a7dc`).
//!
//! [`RenderMode`] follows `Settings::hardware`; `--hardware` / `--software`
//! win. Switching rebuilds the city (lighting, atlases, chunks) and the
//! materials follow the mode on the next frame.

use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Mirror of [`RenderMode`] for code without world access (the level
/// loader's lighting step calls `lighting::set_lighting` directly).
static HARDWARE: AtomicBool = AtomicBool::new(false);

/// Whether the hardware renderer is selected (see [`RenderMode`]).
pub fn is_hardware() -> bool {
    HARDWARE.load(Ordering::Relaxed)
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderMode {
    #[default]
    Software,
    Hardware,
}

impl RenderMode {
    pub fn hardware(self) -> bool {
        self == RenderMode::Hardware
    }

    /// `--hardware` / `--software` on the command line.
    fn forced() -> Option<RenderMode> {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--hardware") {
            Some(RenderMode::Hardware)
        } else if args.iter().any(|a| a == "--software") {
            Some(RenderMode::Software)
        } else {
            None
        }
    }

    fn from_settings(hardware: bool) -> RenderMode {
        if hardware { RenderMode::Hardware } else { RenderMode::Software }
    }
}

pub struct RenderModePlugin;

impl Plugin for RenderModePlugin {
    fn build(&self, app: &mut App) {
        let mode = RenderMode::forced().unwrap_or_default();
        HARDWARE.store(mode.hardware(), Ordering::Relaxed);
        app.insert_resource(mode).add_systems(PreUpdate, follow_settings);
    }
}

/// `Settings::hardware` drives the mode unless the command line forces it.
fn follow_settings(settings: Option<Res<crate::frontend::progress::Settings>>, mut mode: ResMut<RenderMode>) {
    let Some(settings) = settings else { return };
    if RenderMode::forced().is_some() || !settings.is_changed() {
        return;
    }
    let want = RenderMode::from_settings(settings.hardware);
    if *mode != want {
        *mode = want;
        HARDWARE.store(want.hardware(), Ordering::Relaxed);
    }
}
