//! The web build's entry points (docs/web.md).
//!
//! `web/index.html` loads the wasm module, fetches the data pack
//! (`data/payback.json` + `data/payback.pak`), hands it to
//! [`crate::vfs::vfs_load`] and calls [`start`]. The game then runs in the
//! page's `#payback` canvas, which it resizes to fill the page. Options
//! come from the URL query string instead of the command line
//! ([`query_args`]).

use crate::data::GameData;
use bevy::window::{Window, WindowMode};
use wasm_bindgen::prelude::*;

/// Starts the game (the data pack must be loaded). Returns at once; the
/// browser drives the frames.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    // Report a missing or broken data pack to the page, not just the log.
    GameData::locate().and_then(|root| GameData::load(&root)).map_err(|e| JsValue::from_str(&e))?;
    crate::run_game();
    Ok(())
}

/// The page's query string as command-line arguments: `?map=3&level=1&intro`
/// = `--map 3 --level 1 --intro` (a key without a value is a flag).
pub fn query_args() -> Vec<String> {
    let mut out = vec!["payback".to_string()];
    let search = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
    for pair in search.trim_start_matches('?').split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let dec = |s: &str| {
            js_sys::decode_uri_component(&s.replace('+', " ")).ok().and_then(|j| j.as_string()).unwrap_or_else(|| s.to_string())
        };
        out.push(format!("--{}", dec(k).trim_start_matches('-')));
        if !v.is_empty() {
            out.push(dec(v));
        }
    }
    out
}

/// The window is the page's canvas, sized by the page (the saved
/// resolution and window mode do not apply).
pub fn adapt_window(w: &mut Window) {
    w.canvas = Some("#payback".into());
    w.fit_canvas_to_parent = true;
    w.prevent_default_event_handling = true;
    w.mode = WindowMode::Windowed;
}
