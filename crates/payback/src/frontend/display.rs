//! Display options: renderer, resolution, window mode and the Mac/Amiga
//! presentation and content choices (docs/re/frontend_menus.md §3,
//! "Graphics options"; docs/re/mac_version.md, "Decisions").
//!
//! The original chose the screen mode and the renderer (software or Warp3D
//! hardware, `DAT_0114c7d8`) in the external `PaybackSetup` launcher and
//! stored them in `S:Payback.config` (fields 7, 16 and 19). Here they are
//! menu items on the Graphics options screen ([`GraphicsSetup`]), reached
//! from the front end's main menu and from the pause menu's "Graphics
//! options" (`FUN_0101a7dc`, item 5), saved in `Saves/Payback.cfg`
//! (`hardware`, `resolution`, `window_mode`).
//!
//! The Mac release configured the window, size, renderer, rotating view,
//! Gouraud, motion blur and FX in its "Payback Setup" dialog
//! (`CPaybackPreferences::EditPreferences` @ 10050c60). The port has no
//! such dialog: all of it is on the front end's Graphics options screen
//! ([`SETUP_ITEMS`]), with the Mac/Amiga variants, the HUD resolution and
//! the framing. The pause menu keeps the original's two sub-items
//! ([`ITEMS`]) and adds a line that opens the same screen in game.
//!
//! The window follows [`Settings`] live; `--resolution <w>x<h>` and
//! `--window-mode windowed|borderless|fullscreen` win over the settings
//! (not saved).

use super::progress::Settings;
use super::ui::{Ctx, Sfx};
use bevy::{
    prelude::*,
    window::{
        Monitor, MonitorSelection, OnMonitor, PresentMode, PrimaryMonitor, PrimaryWindow, VideoMode,
        VideoModeSelection, WindowMode, WindowResolution,
    },
};

/// The window when nothing is configured (logical pixels).
pub const DEFAULT_SIZE: (u32, u32) = (1280, 800);

/// The Mac's three sizes (MENU 135 "Size", `CAmigaWindow::CAmigaWindow`
/// @ 10102400; 640×480 is its default, `ClearPreference` @ 100508c0),
/// offered first.
pub const MAC_SIZES: [(u32, u32); 3] = [(640, 480), (800, 600), (1024, 768)];

/// Common windowed sizes offered besides the monitor's video modes: PAL
/// 320×256 multiples, then the usual 16:10 / 16:9 / 4:3 sizes.
const COMMON: [(u32, u32); 13] = [
    (640, 480),
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
    /// Sizes the Resolution item cycles through: the Mac sizes first
    /// ([`MAC_SIZES`]), then the monitor's video modes, the common sizes
    /// that fit on it, the current size and the saved one, ascending.
    pub fn resolutions(&self, saved: Option<(u32, u32)>) -> Vec<(u32, u32)> {
        let fits = |&(w, h): &(u32, u32)| self.monitor.is_none_or(|(mw, mh)| w <= mw && h <= mh);
        let mut v: Vec<(u32, u32)> = self.modes.clone();
        v.extend(COMMON.iter().copied().filter(fits));
        v.push(self.current);
        v.extend(saved);
        v.retain(|&(w, h)| w > 0 && h > 0);
        v.sort_by_key(|&(w, h)| (!MAC_SIZES.contains(&(w, h)), w as u64 * h as u64, w));
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
    /// "Perspective: " original / experimental
    /// (`Settings::experimental_perspective`, port-added; no phrase).
    Perspective,
    /// "3D minimap: " on / off (`Settings::minimap_3d`, port-added; no
    /// phrase): the rotating view's tilted minimap, also without it.
    Minimap3d,
    /// "Hardware acceleration: " 1603 (`DAT_0114c7d8`; unused in the
    /// original's menus, set by PaybackSetup).
    Hardware,
    /// "Resolution: " (no phrase).
    Resolution,
    /// "Window mode: " (no phrase).
    WindowMode,
    /// "Motion blur: " 2053 (`DAT_0114c6c8`; pause menu item 3, the Mac
    /// dialog's `mb!!`).
    MotionBlur,
    /// "Lens effects: " 2052 + 2060.. (pause menu item 2, the Mac dialog's
    /// "FX:" popup `le!!`).
    Fx,
    /// "HUD: " hi-res / lo-res (`Settings::hires_hud`; no phrase).
    HiresHud,
    /// "Aspect: " original / free (`Settings::original_aspect`).
    Aspect,
    /// "Textures: " Mac / Amiga look (`Variants::textures`).
    Textures,
    /// "Blood: " Mac / Amiga (`Variants::blood`).
    Blood,
    /// "Menu props: " Mac / Amiga (`Variants::menu_props`).
    MenuProps,
    /// "Title screens: " Mac / Amiga (`Variants::title`).
    Title,
    /// "Intro: " Mac film / Amiga (`Variants::intro`).
    Intro,
    /// "Scenery: " Mac / Amiga (`Variants::scenery`).
    Scenery,
    /// "German gas: " on / off (`Variants::gas`).
    Gas,
    /// "Smooth motion: " on / off (`Settings::smooth_motion`, port-added;
    /// `crate::frame`).
    SmoothMotion,
    /// "Vsync: " on / off (`Settings::vsync`, port-added).
    Vsync,
    /// "Frame limit: " off / 30 / 60 / 120 / 144 (`Settings::frame_limit`,
    /// port-added; 30 is the original's in-game rate).
    FrameLimit,
    /// "Zoom-in map: " on / off (`Settings::zoom_in_map`): the software
    /// renderer's level-start zoom (`hud::zoom`); dimmed under hardware,
    /// which skips it as the original does.
    ZoomInMap,
}

/// The pause menu's graphics sub-items (item 5), as in the original
/// (`FUN_0101a7dc`, `local_5c`). Sub-selection `ITEMS.len()` is the added
/// line [`MORE`], which opens [`GraphicsSetup::in_game`].
pub const ITEMS: [Item; 2] = [Item::Gouraud, Item::Rotating];

/// The pause menu's added line under [`ITEMS`] (no phrase).
pub const MORE: &str = "More graphics options";

/// The front end's Graphics options screen: the Mac setup dialog's
/// controls in its order (display, then rendering), then the HUD and
/// framing, then the Mac/Amiga content choices. ("Unlock all levels" is
/// on the main menu, `menus::MainMenu`.)
pub const SETUP_ITEMS: [Item; 22] = [
    Item::Resolution,
    Item::WindowMode,
    Item::Vsync,
    Item::FrameLimit,
    Item::SmoothMotion,
    Item::Hardware,
    Item::Perspective,
    Item::Rotating,
    Item::Minimap3d,
    Item::Gouraud,
    Item::MotionBlur,
    Item::Fx,
    Item::HiresHud,
    Item::Aspect,
    Item::ZoomInMap,
    Item::Textures,
    Item::Blood,
    Item::Scenery,
    Item::MenuProps,
    Item::Title,
    Item::Intro,
    Item::Gas,
];

/// The Mac intro film (`GetPaybackMovie` @ 100fcbc0, played by
/// `PlayAnimation` @ 100fb1d0).
pub const MAC_FILM: &str = "Videos/payback_high_mpeg1.mpg";

impl Item {
    pub fn label(self, cx: &Ctx) -> String {
        let s = &*cx.settings;
        let on_off = |b: bool| cx.g(if b { 2081 } else { 2080 });
        match self {
            Item::Gouraud => format!("{}{}", cx.g(1601), on_off(s.gouraud)),
            Item::Rotating => format!("{}{}", cx.g(1602), on_off(s.rotating_view)),
            Item::Minimap3d => format!("3D minimap: {}", on_off(s.minimap_3d_on())),
            Item::Perspective => {
                format!("Perspective: {}", if s.experimental_perspective { "Experimental" } else { "Original" })
            }
            Item::Hardware => format!("{}{}", cx.g(1603), on_off(s.hardware)),
            Item::Resolution => {
                let (w, h) = cx.display.shown(s);
                format!("Resolution: {w}x{h}")
            }
            Item::WindowMode => format!("Window mode: {}", s.window_mode.label()),
            // Hardware: always "Off" (drawn dimmed), as in the pause menu.
            Item::MotionBlur => format!("{}{}", cx.g(2053), on_off(s.motion_blur && !s.hardware)),
            Item::Fx => format!("{}{}", cx.g(2052), cx.g(2060 + s.fx.min(2) as u32)),
            Item::HiresHud => format!("HUD: {}", if s.hires_hud { "Hi-res" } else { "Lo-res" }),
            Item::Aspect => format!("Aspect: {}", if s.original_aspect { "Original" } else { "Free" }),
            Item::Gas => format!("German gas: {}", on_off(s.variants.gas)),
            Item::SmoothMotion => format!("Smooth motion: {}", on_off(s.smooth_motion)),
            Item::Vsync => format!("Vsync: {}", on_off(s.vsync)),
            Item::ZoomInMap => format!("Zoom-in map: {}", on_off(s.zoom_in_map && !s.hardware)),
            Item::FrameLimit => match s.frame_limit {
                0 => format!("Frame limit: {}", on_off(false)),
                n => format!("Frame limit: {n} fps"),
            },
            _ => {
                // A Mac choice reads "Amiga" when the Mac data is missing,
                // an Amiga one "Mac" when the Amiga data is.
                let mac = self.variant(s).unwrap_or(false) && !self.needs_mac(cx.data)
                    || self.needs_amiga(cx.data);
                let (name, m, a) = match self {
                    Item::Textures => ("Textures", "Mac", "Amiga look"),
                    Item::Blood => ("Blood", "Mac", "Amiga"),
                    Item::MenuProps => ("Menu props", "Mac", "Amiga"),
                    Item::Title => ("Title screens", "Mac", "Amiga"),
                    Item::Intro => ("Intro", "Mac film", "Amiga"),
                    _ => ("Scenery", "Mac", "Amiga"),
                };
                format!("{name}: {}", if mac { m } else { a })
            }
        }
    }

    /// The Mac/Amiga variant flag behind the item, if it is one.
    fn variant(self, s: &Settings) -> Option<bool> {
        let v = &s.variants;
        Some(match self {
            Item::Textures => v.textures,
            Item::Blood => v.blood,
            Item::MenuProps => v.menu_props,
            Item::Title => v.title,
            Item::Intro => v.intro,
            Item::Scenery => v.scenery,
            _ => return None,
        })
    }

    fn variant_mut(self, s: &mut Settings) -> Option<&mut bool> {
        let v = &mut s.variants;
        Some(match self {
            Item::Textures => &mut v.textures,
            Item::Blood => &mut v.blood,
            Item::MenuProps => &mut v.menu_props,
            Item::Title => &mut v.title,
            Item::Intro => &mut v.intro,
            Item::Scenery => &mut v.scenery,
            Item::Gas => &mut v.gas,
            _ => return None,
        })
    }

    /// A Mac-only choice without the Mac data (`GameData::mac`, or the film
    /// for the intro): greyed out and showing the Amiga content.
    pub fn needs_mac(self, data: &crate::data::GameData) -> bool {
        match self {
            Item::Intro => data.mac_path(MAC_FILM).is_none(),
            Item::Textures | Item::Blood | Item::MenuProps | Item::Title | Item::Scenery => data.mac.is_none(),
            _ => false,
        }
    }

    /// A Mac/Amiga choice without the Amiga data (running from the Mac
    /// data alone, `GameData::amiga`): greyed out and showing the Mac
    /// content, mirroring [`Item::needs_mac`].
    pub fn needs_amiga(self, data: &crate::data::GameData) -> bool {
        !data.amiga
            && matches!(self, Item::Textures | Item::Blood | Item::MenuProps | Item::Title | Item::Intro | Item::Scenery)
    }

    /// Dimmed for any of these reasons ([`Item::dimmed`],
    /// [`Item::needs_mac`], [`Item::needs_amiga`]).
    pub fn locked(self, cx: &Ctx) -> bool {
        self.dimmed(cx.settings) || self.needs_mac(cx.data) || self.needs_amiga(cx.data)
    }

    /// In a level the setting only takes effect at the next level load:
    /// the scenery models (`objects::set_model_variants`) are built at
    /// load and swapping them would reshape the level's objects. The
    /// others apply at once (the cube textures through
    /// `lighting::follow_textures`, the blood through
    /// `effects::sync_blood`); the title, intro and menu props show in
    /// the menus.
    pub fn next_level(self) -> bool {
        self == Item::Scenery
    }

    /// Debug name (`--graphics-toggle`): the settings-file key.
    pub fn from_name(name: &str) -> Option<Item> {
        Some(match name.trim() {
            "gouraud" => Item::Gouraud,
            "rotating_view" | "rotating" => Item::Rotating,
            "minimap_3d" | "minimap" => Item::Minimap3d,
            "perspective" => Item::Perspective,
            "hardware" => Item::Hardware,
            "resolution" => Item::Resolution,
            "window_mode" => Item::WindowMode,
            "motion_blur" => Item::MotionBlur,
            "fx" => Item::Fx,
            "hires_hud" | "hud" => Item::HiresHud,
            "original_aspect" | "aspect" => Item::Aspect,
            "textures" => Item::Textures,
            "blood" => Item::Blood,
            "menu_props" => Item::MenuProps,
            "title" => Item::Title,
            "intro" => Item::Intro,
            "scenery" => Item::Scenery,
            "gas" => Item::Gas,
            "smooth_motion" | "smooth" => Item::SmoothMotion,
            "vsync" => Item::Vsync,
            "zoom_in_map" | "zoom" => Item::ZoomInMap,
            "frame_limit" => Item::FrameLimit,
            _ => return None,
        })
    }

    /// Drawn dimmed (`FUN_0100cc78`): Gouraud under hardware rendering
    /// (`FUN_0101a7dc`: `DAT_0114c2f8 == 0 || DAT_0114c7d8 != 0`), the
    /// resolution in borderless fullscreen (it is the desktop's).
    pub fn dimmed(self, s: &Settings) -> bool {
        match self {
            // Motion blur too (`FUN_0101a7dc` case 3 refuses it).
            Item::Gouraud | Item::MotionBlur | Item::ZoomInMap => s.hardware,
            // Web build: the canvas fills the page; the browser owns the
            // size, the mode and vsync.
            #[cfg(target_arch = "wasm32")]
            Item::WindowMode | Item::Vsync => true,
            Item::Resolution => cfg!(target_arch = "wasm32") || s.window_mode == WindowChoice::Borderless,
            // Forced on by the rotating view (`Settings::minimap_3d_on`).
            Item::Minimap3d => s.rotating_view,
            // The experimental perspective replaces the rotating view.
            Item::Rotating => s.experimental_perspective,
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
        let locked = self.locked(cx);
        let s = &mut *cx.settings;
        let ok = match self {
            _ if locked => false,
            Item::Gouraud => {
                s.gouraud = !s.gouraud;
                true
            }
            Item::Rotating => {
                s.rotating_view = !s.rotating_view;
                true
            }
            Item::Minimap3d => {
                s.minimap_3d = !s.minimap_3d;
                true
            }
            Item::Perspective => {
                s.experimental_perspective = !s.experimental_perspective;
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
            Item::MotionBlur => {
                s.motion_blur = !s.motion_blur;
                true
            }
            Item::Fx => {
                s.fx = (s.fx as i32 + d).rem_euclid(3) as u8;
                true
            }
            Item::HiresHud => {
                s.hires_hud = !s.hires_hud;
                true
            }
            Item::Aspect => {
                s.original_aspect = !s.original_aspect;
                true
            }
            Item::SmoothMotion => {
                s.smooth_motion = !s.smooth_motion;
                true
            }
            Item::Vsync => {
                s.vsync = !s.vsync;
                true
            }
            Item::ZoomInMap => {
                s.zoom_in_map = !s.zoom_in_map;
                true
            }
            Item::FrameLimit => {
                let l = &crate::frame::FRAME_LIMITS;
                let i = l.iter().position(|&n| n == s.frame_limit).unwrap_or(0) as i32;
                s.frame_limit = l[(i + d).rem_euclid(l.len() as i32) as usize];
                true
            }
            _ => match self.variant_mut(s) {
                Some(v) => {
                    *v = !*v;
                    true
                }
                None => false,
            },
        };
        cx.sound(if ok { Sfx::Select } else { Sfx::Refuse });
    }
}

/// Move the sub-selection `sel` (index into [`ITEMS`], or `ITEMS.len()`
/// for [`MORE`]) by `d`, skipping items that cannot be selected.
pub fn step_item(sel: usize, d: i32, s: &Settings) -> usize {
    let n = ITEMS.len() as i32 + 1;
    let mut i = sel as i32;
    for _ in 0..n {
        i = (i + d).rem_euclid(n);
        if ITEMS.get(i as usize).is_none_or(|it| it.selectable(s)) {
            break;
        }
    }
    i as usize
}

/// First selectable sub-item.
pub fn first_item(s: &Settings) -> usize {
    ITEMS.iter().position(|i| i.selectable(s)).unwrap_or(0)
}

/// The Graphics options screen: main menu item 8 and, in a level, the
/// pause menu's "More graphics options" (the original set these in
/// PaybackSetup, the Mac in its setup dialog). Laid out like CD Audio
/// Setup (`FUN_0102dd30`): title, the items, "Done" 2148 at 170/200. The
/// [`SETUP_ITEMS`] list is 12 px a line from 36/200 and scrolls when it
/// does not fit above Done (dots mark hidden lines). Up/down select, fire
/// or right steps an item forward, left back; Esc or Done saves and
/// returns.
///
/// In a level it is drawn like the pause menu: over the frozen game
/// picture, darkened (FadeDarken3, also under hardware rendering), with
/// `options.obj` turning behind, and the music kept going. Settings apply
/// live; [`Item::next_level`] ones read "(next level)".
pub struct GraphicsSetup {
    /// Index into [`SETUP_ITEMS`], or `SETUP_ITEMS.len()` = Done.
    sel: usize,
    /// First item shown.
    top: usize,
    /// Opened from the pause menu.
    in_game: bool,
    /// Background `options.obj` angle (as the pause menu's).
    back: i32,
    /// Debug (`--graphics-toggle`): items stepped on the first tick.
    toggle: Vec<Item>,
}

/// Line spacing of the item list.
const STEP: i32 = 12;

/// First line and number of item lines that fit above Done.
fn list_rows() -> (i32, usize) {
    use super::ui::H;
    let y0 = H * 36 / 200;
    (y0, ((H * 170 / 200 - 4 - y0) / STEP) as usize)
}

impl GraphicsSetup {
    pub fn new(s: &Settings) -> GraphicsSetup {
        // The Mac-only items come last, so the first one that is not
        // dimmed is never locked for want of the Mac data.
        let sel = SETUP_ITEMS.iter().position(|i| !i.dimmed(s)).unwrap_or(0);
        GraphicsSetup { sel, top: 0, in_game: false, back: 0, toggle: Vec::new() }
    }

    /// From the pause menu.
    pub fn in_game(s: &Settings) -> GraphicsSetup {
        GraphicsSetup { in_game: true, ..GraphicsSetup::new(s) }
    }

    /// Debug (`--pause-graphics`): on line `--menu-item <n>`, stepping the
    /// `--graphics-toggle <keys>` items (comma-separated) once.
    pub fn debug(s: &Settings) -> GraphicsSetup {
        let mut g = GraphicsSetup::in_game(s);
        if let Some(i) = crate::arg("--menu-item").and_then(|v| v.parse::<usize>().ok()) {
            g.sel = i.min(SETUP_ITEMS.len());
        }
        g.toggle = crate::arg("--graphics-toggle")
            .map(|v| v.split(',').filter_map(Item::from_name).collect())
            .unwrap_or_default();
        g
    }

    fn label(&self, item: Item, cx: &Ctx) -> String {
        let l = item.label(cx);
        if self.in_game && item.next_level() { format!("{l} (next level)") } else { l }
    }
}

impl super::ui::Screen for GraphicsSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &super::ui::Input) -> super::ui::Step {
        for item in std::mem::take(&mut self.toggle) {
            item.activate(cx, 1);
        }
        self.back = (self.back + 0x40).rem_euclid(super::ui::TURN);
        let n = SETUP_ITEMS.len() + 1;
        if input.up || input.down {
            let d = if input.down { 1 } else { -1 };
            let mut i = self.sel as i32;
            for _ in 0..n {
                i = (i + d).rem_euclid(n as i32);
                if SETUP_ITEMS.get(i as usize).is_none_or(|it| !it.locked(cx)) {
                    break;
                }
            }
            self.sel = i as usize;
            cx.sound(Sfx::Move);
        }
        // Keep the selection on screen (Done shows the end of the list).
        let rows = list_rows().1;
        let last = self.sel.min(SETUP_ITEMS.len() - 1);
        if last < self.top {
            self.top = last;
        } else if last >= self.top + rows {
            self.top = last + 1 - rows;
        }
        let done = self.sel == SETUP_ITEMS.len();
        if input.back || (input.fire && done) {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return super::ui::Step::Pop(super::ui::Outcome::Done);
        }
        if let Some(&item) = SETUP_ITEMS.get(self.sel) {
            if input.fire || input.right {
                item.activate(cx, 1);
            } else if input.left {
                item.activate(cx, -1);
            }
        }
        super::ui::Step::Stay
    }

    fn draw(&self, ui: &mut super::ui::Ui, cx: &Ctx) {
        if !self.in_game {
            ui.picture(0, true);
        } else {
            // The pause menu's FadeDarken3 panel, widened for the list
            // (under hardware rendering too, as the pause menu).
            let (x0, x1) = (ui.vx(20), ui.vx(299));
            ui.back.darken(x0, 2, x1, 229);
        }
        ui.title(&cx.g(1600));
        let (y0, rows) = list_rows();
        let end = (self.top + rows).min(SETUP_ITEMS.len());
        for (row, i) in (self.top..end).enumerate() {
            let item = SETUP_ITEMS[i];
            let y = y0 + STEP * row as i32;
            if item.locked(cx) {
                ui.centred_dim(&self.label(item, cx), y, 2);
            } else {
                let c = if i == self.sel { ui.flash() } else { 2 };
                ui.centred(&self.label(item, cx), y, c);
            }
        }
        if self.top > 0 {
            ui.centred_dim("...", y0 - STEP, 2);
        }
        if end < SETUP_ITEMS.len() {
            ui.centred_dim("...", y0 + STEP * rows as i32 - 4, 2);
        }
        let c = if self.sel == SETUP_ITEMS.len() { ui.flash() } else { 2 };
        ui.centred(&cx.g(2148), ui.hy(170), c);
    }

    /// In a level: the pause menu's `options.obj` behind.
    fn back_model(&self) -> Option<super::ui::ModelView> {
        self.in_game.then_some(super::ui::ModelView {
            path: "options/options.obj",
            pos: [0, -2000, 10000],
            rot: [self.back, 0, 0],
            paint: None,
        })
    }

    /// In a level the music keeps going (`CD_CheckForTrackEnd(0)`, as the
    /// pause menu); in the menus track 13.
    fn cd_track(&self) -> i32 {
        if self.in_game { 0 } else { 13 }
    }
}

/// The primary window at start-up, from the saved settings (or the debug
/// flags). Sizes are physical; the window is created assuming a scale
/// factor of 1 and [`apply`] corrects it once the monitor is known.
/// Exclusive fullscreen needs the monitor's video modes, which are only
/// listed once the window exists: the window is created hidden and
/// windowed, and [`apply`] shows it directly in the chosen video mode (the
/// Mac opens in the chosen screen mode, `UDrawSprocket::GetDSpContext`
/// @ 10157090), with no switch through the desktop mode.
pub fn startup_window(title: &str) -> Window {
    let s = Settings::load();
    let (w, h) = forced_size().or(s.resolution).unwrap_or(DEFAULT_SIZE);
    let (mode, visible) = match forced_mode().unwrap_or(s.window_mode) {
        WindowChoice::Windowed => (WindowMode::Windowed, true),
        WindowChoice::Borderless => (WindowMode::BorderlessFullscreen(MonitorSelection::Current), true),
        WindowChoice::Fullscreen => (WindowMode::Windowed, false),
    };
    Window {
        title: title.into(),
        resolution: WindowResolution::new(w, h),
        mode,
        visible,
        present_mode: if s.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync },
        ..default()
    }
}

/// Frames to wait for the monitor list before showing a hidden fullscreen
/// window in the desktop mode anyway.
const MONITOR_WAIT: u32 = 60;

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<Display>().add_systems(PreUpdate, (scan, apply).chain());
        // Web build: the page sizes the canvas (`web::adapt_window`).
        #[cfg(target_arch = "wasm32")]
        app.init_resource::<Display>().add_systems(PreUpdate, scan);
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
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Applied {
    size: Option<(u32, u32)>,
    mode: WindowChoice,
}

/// Bring the window in line with the settings when they change (and once
/// at start, when the monitor is known).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn apply(
    settings: Option<Res<Settings>>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    primary: Query<&Monitor, With<PrimaryMonitor>>,
    mut last: Local<Option<Applied>>,
    mut waited: Local<u32>,
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
        *waited += 1;
        if !window.visible && *waited > MONITOR_WAIT {
            warn!("no monitor listed; fullscreen in the desktop mode");
            window.mode = WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current);
            window.visible = true;
        }
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
    if !window.visible {
        window.visible = true;
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
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
        assert_eq!(&r[..3], &MAC_SIZES);
        assert!(r[3..].windows(2).all(|p| p[0].0 as u64 * p[0].1 as u64 <= p[1].0 as u64 * p[1].1 as u64));
        assert!(r.contains(&(1000, 700)) && r.contains(&(1920, 1080)));
        assert!(!r.contains(&(2560, 1440)));
    }

    #[test]
    fn hardware_skips_gouraud() {
        let mut s = Settings { hardware: false, ..Settings::default() };
        assert_eq!(first_item(&s), 0);
        s.hardware = true;
        assert_eq!(first_item(&s), 1);
        // Gouraud is skipped: up from Rotating view wraps to the added line.
        assert_eq!(step_item(1, -1, &s), ITEMS.len());
        assert_eq!(step_item(ITEMS.len(), 1, &s), 1);
        s.hardware = false;
        assert_eq!(step_item(ITEMS.len(), 1, &s), 0);
    }
}
