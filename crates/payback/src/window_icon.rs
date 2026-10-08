//! The window icon: the Mac release's application icon if it was available
//! at build time (embedded by `build.rs`), otherwise the image the Amiga
//! Workbench launcher showed (`Payback.info`), read from the game data.

use crate::data::GameData;
use bevy::{ecs::system::NonSendMarker, prelude::*, window::PrimaryWindow, winit::WINIT_WINDOWS};

/// Window icon embedded at build time: width, height (u32 LE), RGBA; empty
/// without game data at build time.
const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/window_icon.rgba"));

/// Fallback size for the 66x30 Amiga art (scaled 3x, centred).
const SIZE: u32 = 256;

pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_window_icon);
    }
}

/// The embedded icon, if any.
fn embedded() -> Option<(u32, u32, Vec<u8>)> {
    let w = u32::from_le_bytes(EMBEDDED.get(0..4)?.try_into().ok()?);
    let h = u32::from_le_bytes(EMBEDDED.get(4..8)?.try_into().ok()?);
    let px = EMBEDDED.get(8..)?;
    (px.len() == (w * h * 4) as usize).then(|| (w, h, px.to_vec()))
}

/// Set once, as soon as the primary window exists.
fn set_window_icon(
    data: Res<GameData>,
    window: Query<Entity, With<PrimaryWindow>>,
    mut done: Local<bool>,
    _main_thread: NonSendMarker,
) {
    if *done {
        return;
    }
    let Ok(entity) = window.single() else { return };
    let image = match embedded() {
        Some(i) => i,
        None => match data.read("Payback.info").map_err(|e| e.to_string()).and_then(|b| {
            payback_formats::icon::workbench_image(&b).map_err(|e| e.to_string())
        }) {
            Ok(i) => {
                let sq = i.square(SIZE);
                (sq.width, sq.height, sq.pixels)
            }
            Err(e) => {
                warn!("window icon: {e}");
                *done = true;
                return;
            }
        },
    };
    WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else { return };
        match winit::window::Icon::from_rgba(image.2.clone(), image.0, image.1) {
            Ok(icon) => window.set_window_icon(Some(icon)),
            Err(e) => warn!("window icon: {e}"),
        }
        *done = true;
    });
}
