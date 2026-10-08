//! The window icon: the image the Amiga Workbench launcher showed for the
//! game (`Payback.info`, the OS 3.5 colour image of the title art), read
//! from the game data. The executable gets the same icon at build time
//! (`build.rs`).

use crate::data::GameData;
use bevy::{ecs::system::NonSendMarker, prelude::*, window::PrimaryWindow, winit::WINIT_WINDOWS};

/// Window icon size: the 66x30 art scaled 3x, centred.
const SIZE: u32 = 256;

pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_window_icon);
    }
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
    let image = match data.read("Payback.info").map_err(|e| e.to_string()).and_then(|b| {
        payback_formats::icon::workbench_image(&b).map_err(|e| e.to_string())
    }) {
        Ok(i) => i.square(SIZE),
        Err(e) => {
            warn!("window icon: {e}");
            *done = true;
            return;
        }
    };
    WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else { return };
        match winit::window::Icon::from_rgba(image.pixels.clone(), image.width, image.height) {
            Ok(icon) => window.set_window_icon(Some(icon)),
            Err(e) => warn!("window icon: {e}"),
        }
        *done = true;
    });
}
