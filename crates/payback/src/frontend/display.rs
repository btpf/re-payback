//! Display options: renderer, resolution and window mode
//! (docs/re/frontend_menus.md §3, "Graphics options").
//!
//! The original chose the screen mode and the renderer (software or Warp3D
//! hardware, `DAT_0114c7d8`) in the external `PaybackSetup` launcher and
//! stored them in `S:Payback.config` (fields 7, 16 and 19). Here they are
//! menu items: the pause menu's "Graphics options" (`FUN_0101a7dc`, item 5)
//! and the front end's Graphics options screen, saved in `Saves/Payback.cfg`
//! (`hardware`, `resolution`, `window_mode`).
//!
//! The window follows [`Settings`] live; `--resolution <w>x<h>` and
//! `--window-mode windowed|borderless|fullscreen` win over the settings
//! (not saved).

use super::progress::Settings;
use super::ui::{Ctx, Sfx};
use bevy::{
    prelude::*,
    window::{
        Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, VideoMode, VideoModeSelection,
        WindowMode, WindowResolution,
    },
};

/// The window when nothing is configured (logical pixels).
pub const DEFAULT_SIZE: (u32, u32) = (1280, 800);

/// Common windowed sizes offered besides the monitor's video modes: PAL
/// 320×256 multiples, then the usual 16:10 / 16:9 / 4:3 sizes.
const COMMON: [(u32, u32); 12] = [
    (640, 512),
    (800, 600),
    (960, 768),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1280, 1024),
    (1366, 768),
    (1600, 900),
    (1680, 1050),
    (1920, 1080),
    (2560, 1440),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowChoice {
    #[default]
    Windowed,
    /// Borderless fullscreen on the window's monitor, at its desktop size.
    Borderless,
    /// Exclusive fullscreen in the video mode of `Settings::resolution`.
    Fullscreen,
}

impl WindowChoice {
    pub const ALL: [WindowChoice; 3] = [WindowChoice::Windowed, WindowChoice::Borderless, WindowChoice::Fullscreen];

    pub fn name(self) -> &'static str {
        match self {
            WindowChoice::Windowed => "windowed",
            WindowChoice::Borderless => "borderless",
            WindowChoice::Fullscreen => "fullscreen",
        }
    }

    pub fn parse(s: &str) -> Option<WindowChoice> {
        WindowChoice::ALL.into_iter().find(|c| c.name() == s.trim().to_ascii_lowercase())
    }

    /// Menu text (no phrase exists for it).
    pub fn label(self) -> &'static str {
        match self {
            WindowChoice::Windowed => "Windowed",
            WindowChoice::Borderless => "Windowed borderless",
            WindowChoice::Fullscreen => "Fullscreen",
        }
    }

    fn step(self, d: i32) -> WindowChoice {
        let i = WindowChoice::ALL.iter().position(|&c| c == self).unwrap_or(0) as i32;
        WindowChoice::ALL[(i + d).rem_euclid(3) as usize]
    }
}

/// `"1280x800"` → (1280, 800).
pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w >= 160 && h >= 128 && w <= 16384 && h <= 16384).then_some((w, h))
}

fn forced_size() -> Option<(u32, u32)> {
    crate::arg("--resolution").as_deref().and_then(parse_size)
}

fn forced_mode() -> Option<WindowChoice> {
    crate::arg("--window-mode").as_deref().and_then(WindowChoice::parse)
}

/// The window's monitor and size, refreshed every frame for the menus.
#[derive(Resource, Default, Clone, Debug)]
pub struct Display {
    /// Desktop size of the window's monitor (physical pixels).
    pub monitor: Option<(u32, u32)>,
    /// The monitor's video modes (physical sizes, deduplicated).
    pub modes: Vec<(u32, u32)>,
    /// The window's current physical size.
    pub current: (u32, u32),
}

impl Display {
    /// Sizes the Resolution item cycles through: the monitor's video modes,
    /// the common sizes that fit on it, the current size and the saved one,
    /// ascending.
    pub fn resolutions(&self, saved: Option<(u32, u32)>) -> Vec<(u32, u32)> {
        let fits = |&(w, h): &(u32, u32)| self.monitor.is_none_or(|(mw, mh)| w <= mw && h <= mh);
        let mut v: Vec<(u32, u32)> = self.modes.clone();
        v.extend(COMMON.iter().copied().filter(fits));
        v.push(self.current);
        v.extend(saved);
        v.retain(|&(w, h)| w > 0 && h > 0);
        v.sort_by_key(|&(w, h)| (w as u64 * h as u64, w));
        v.dedup();
        v
    }

    /// The size the Resolution item shows.
    pub fn shown(&self, s: &Settings) -> (u32, u32) {
        match s.window_mode {
            WindowChoice::Borderless => self.monitor.unwrap_or(self.current),
            _ => s.resolution.unwrap_or(self.current),
        }
    }
}

/// One line of the graphics options (pause menu item 5 sub-items and the
/// front end's Graphics options screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// "Gouraud shading: " 1601 (`DAT_0114c2fc`).
    Gouraud,
    /// "Rotating view: " 1602 (`DAT_0114c7dc`).
    Rotating,
    /// "Hardware acceleration: " 1603 (`DAT_0114c7d8`; unused in the
    /// original's menus, set by PaybackSetup).
    Hardware,
    /// "Resolution: " (no phrase).
    Resolution,
    /// "Window mode: " (no phrase).
    WindowMode,
}

pub const ITEMS: [Item; 5] = [Item::Gouraud, Item::Rotating, Item::Hardware, Item::Resolution, Item::WindowMode];

impl Item {
    pub fn label(self, cx: &Ctx) -> String {
        let s = &*cx.settings;
        let on_off = |b: bool| cx.g(if b { 2081 } else { 2080 });
        match self {
            Item::Gouraud => format!("{}{}", cx.g(1601), on_off(s.gouraud)),
            Item::Rotating => format!("{}{}", cx.g(1602), on_off(s.rotating_view)),
            Item::Hardware => format!("{}{}", cx.g(1603), on_off(s.hardware)),
            Item::Resolution => {
                let (w, h) = cx.display.shown(s);
                format!("Resolution: {w}x{h}")
            }
            Item::WindowMode => format!("Window mode: {}", s.window_mode.label()),
        }
    }

    /// Drawn dimmed (`FUN_0100cc78`): Gouraud under hardware rendering
    /// (`FUN_0101a7dc`: `DAT_0114c2f8 == 0 || DAT_0114c7d8 != 0`), the
    /// resolution in borderless fullscreen (it is the desktop's).
    pub fn dimmed(self, s: &Settings) -> bool {
        match self {
            Item::Gouraud => s.hardware,
            Item::Resolution => s.window_mode == WindowChoice::Borderless,
            _ => false,
        }
    }

    /// Can be selected: under hardware rendering the pause menu starts the
    /// sub-selection on "Rotating view" and moving up from it leaves the
    /// sub-items (`local_5c = DAT_0114c7d8 != 0`), so Gouraud is skipped;
    /// the same goes for the resolution in borderless fullscreen.
    pub fn selectable(self, s: &Settings) -> bool {
        !self.dimmed(s)
    }

    /// Fire (d = 1) or left/right (d = ±1) on the item, with the select or
    /// refuse sound.
    pub fn activate(self, cx: &mut Ctx, d: i32) {
        let display = cx.display.clone();
        let s = &mut *cx.settings;
        let ok = match self {
            _ if self.dimmed(s) => false,
            Item::Gouraud => {
                s.gouraud = !s.gouraud;
                true
            }
            Item::Rotating => {
                s.rotating_view = !s.rotating_view;
                true
            }
            Item::Hardware => {
                s.hardware = !s.hardware;
                s.apply_hardware();
                true
            }
            Item::Resolution => {
                let list = display.resolutions(s.resolution);
                let now = display.shown(s);
                let i = list.iter().position(|&r| r == now).unwrap_or(0) as i32;
                let next = list[(i + d).rem_euclid(list.len().max(1) as i32) as usize];
                s.resolution = Some(next);
                true
            }
            Item::WindowMode => {
                s.window_mode = s.window_mode.step(d);
                true
            }
        };
        cx.sound(if ok { Sfx::Select } else { Sfx::Refuse });
    }
}

/// Move the sub-selection `sel` (index into [`ITEMS`]) by `d`, skipping
/// items that cannot be selected.
pub fn step_item(sel: usize, d: i32, s: &Settings) -> usize {
    let n = ITEMS.len() as i32;
    let mut i = sel as i32;
    for _ in 0..n {
        i = (i + d).rem_euclid(n);
        if ITEMS[i as usize].selectable(s) {
            break;
        }
    }
    i as usize
}

/// First selectable sub-item.
pub fn first_item(s: &Settings) -> usize {
    ITEMS.iter().position(|i| i.selectable(s)).unwrap_or(0)
}

/// The front end's Graphics options screen (main menu item 8; the
/// original set these in PaybackSetup). Laid out like CD Audio Setup
/// (`FUN_0102dd30`): title, the items from y = 60/200 14 px apart, "Done"
/// 2148 at 170/200. Up/down select, fire or right steps an item forward,
/// left back; Esc or Done saves and returns.
pub struct GraphicsSetup {
    /// Index into [`ITEMS`], or `ITEMS.len()` = Done.
    sel: usize,
}

impl GraphicsSetup {
    pub fn new(s: &Settings) -> GraphicsSetup {
        GraphicsSetup { sel: first_item(s) }
    }
}

impl super::ui::Screen for GraphicsSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &super::ui::Input) -> super::ui::Step {
        let n = ITEMS.len() + 1;
        if input.up || input.down {
            let d = if input.down { 1 } else { -1 };
            let mut i = self.sel as i32;
            for _ in 0..n {
                i = (i + d).rem_euclid(n as i32);
                if ITEMS.get(i as usize).is_none_or(|it| it.selectable(cx.settings)) {
                    break;
                }
            }
            self.sel = i as usize;
            cx.sound(Sfx::Move);
        }
        let done = self.sel == ITEMS.len();
        if input.back || (input.fire && done) {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return super::ui::Step::Pop(super::ui::Outcome::Done);
        }
        if let Some(&item) = ITEMS.get(self.sel) {
            if input.fire || input.right {
                item.activate(cx, 1);
            } else if input.left {
                item.activate(cx, -1);
            }
        }
        super::ui::Step::Stay
    }

    fn draw(&self, ui: &mut super::ui::Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(1600));
        for (i, item) in ITEMS.iter().enumerate() {
            let y = ui.hy(60) + 14 * i as i32;
            let c = if i == self.sel { ui.flash() } else { 2 };
            if item.dimmed(cx.settings) {
                ui.centred_dim(&item.label(cx), y, 2);
            } else {
                ui.centred(&item.label(cx), y, c);
            }
        }
        let c = if self.sel == ITEMS.len() { ui.flash() } else { 2 };
        ui.centred(&cx.g(2148), ui.hy(170), c);
    }
}

/// The primary window at start-up, from the saved settings (or the debug
/// flags). Sizes are physical; the window is created assuming a scale
/// factor of 1 and [`apply`] corrects it once the monitor is known.
/// Exclusive fullscreen starts in the monitor's current mode and switches
/// to the saved video mode when the monitors are listed.
pub fn startup_window(title: &str) -> Window {
    let s = Settings::load();
    let (w, h) = forced_size().or(s.resolution).unwrap_or(DEFAULT_SIZE);
    let mode = match forced_mode().unwrap_or(s.window_mode) {
        WindowChoice::Windowed => WindowMode::Windowed,
        WindowChoice::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        WindowChoice::Fullscreen => WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current),
    };
    Window {
        title: title.into(),
        resolution: WindowResolution::new(w, h),
        mode,
        ..default()
    }
}

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Display>().add_systems(PreUpdate, (scan, apply).chain());
    }
}

fn scan(
    mut display: ResMut<Display>,
    windows: Query<(&Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    primary: Query<&Monitor, With<PrimaryMonitor>>,
) {
    let Ok((window, on)) = windows.single() else {
        return;
    };
    let monitor = on.and_then(|m| monitors.get(m.0).ok()).or_else(|| primary.single().ok());
    let current = (window.resolution.physical_width(), window.resolution.physical_height());
    let (size, modes) = match monitor {
        Some(m) => {
            let mut modes: Vec<(u32, u32)> = m.video_modes.iter().map(|v| (v.physical_size.x, v.physical_size.y)).collect();
            modes.sort();
            modes.dedup();
            (Some((m.physical_width, m.physical_height)), modes)
        }
        None => (None, Vec::new()),
    };
    if display.current != current || display.monitor != size || display.modes != modes {
        display.current = current;
        display.monitor = size;
        display.modes = modes;
    }
}

/// What was last applied to the window.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Applied {
    size: Option<(u32, u32)>,
    mode: WindowChoice,
}

/// Bring the window in line with the settings when they change (and once
/// at start, when the monitor is known).
fn apply(
    settings: Option<Res<Settings>>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    primary: Query<&Monitor, With<PrimaryMonitor>>,
    mut last: Local<Option<Applied>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let want = Applied {
        size: forced_size().or(settings.resolution),
        mode: forced_mode().unwrap_or(settings.window_mode),
    };
    if *last == Some(want) {
        return;
    }
    let Ok((mut window, on)) = windows.single_mut() else {
        return;
    };
    let Some(monitor) = on.and_then(|m| monitors.get(m.0).ok()).or_else(|| primary.single().ok()) else {
        // Not listed yet (first frames).
        return;
    };
    *last = Some(want);
    let mode = match want.mode {
        WindowChoice::Windowed => WindowMode::Windowed,
        WindowChoice::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        WindowChoice::Fullscreen => {
            let size = want.size.unwrap_or((monitor.physical_width, monitor.physical_height));
            match video_mode(monitor, size) {
                Some(v) => WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Specific(v)),
                None => {
                    warn!("no {}x{} video mode; using the desktop mode", size.0, size.1);
                    WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
                }
            }
        }
    };
    if window.mode != mode {
        window.mode = mode;
    }
    if want.mode == WindowChoice::Windowed {
        if let Some((w, h)) = want.size {
            if (window.resolution.physical_width(), window.resolution.physical_height()) != (w, h) {
                window.resolution.set_physical_resolution(w, h);
            }
        }
    }
}

/// The monitor's video mode of that size with the highest refresh rate
/// and bit depth.
fn video_mode(monitor: &Monitor, (w, h): (u32, u32)) -> Option<VideoMode> {
    monitor
        .video_modes
        .iter()
        .filter(|v| v.physical_size == UVec2::new(w, h))
        .max_by_key(|v| (v.refresh_rate_millihertz, v.bit_depth))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_modes() {
        assert_eq!(parse_size("1280x800"), Some((1280, 800)));
        assert_eq!(parse_size(" 640 X 512 "), Some((640, 512)));
        assert_eq!(parse_size("12x8"), None);
        assert_eq!(WindowChoice::parse("Borderless"), Some(WindowChoice::Borderless));
        assert_eq!(WindowChoice::Fullscreen.step(1), WindowChoice::Windowed);
        let d = Display { monitor: Some((1920, 1080)), modes: vec![(1920, 1080), (800, 600)], current: (1000, 700) };
        let r = d.resolutions(Some((1280, 800)));
        assert_eq!(r.first(), Some(&(640, 512)));
        assert!(r.contains(&(1000, 700)) && r.contains(&(1920, 1080)));
        assert!(!r.contains(&(2560, 1440)));
    }

    #[test]
    fn hardware_skips_gouraud() {
        let mut s = Settings::default();
        assert_eq!(first_item(&s), 0);
        s.hardware = true;
        assert_eq!(first_item(&s), 1);
        assert_eq!(step_item(1, -1, &s), 4);
        assert_eq!(step_item(4, 1, &s), 1);
    }
}
