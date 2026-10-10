//! Player progress, save slots and the settings file
//! (docs/re/frontend_menus.md §4–6).
//!
//! Save slots keep the original 152-byte big-endian layout
//! (`FUN_0101182c` load, `FUN_01011a70` save; the Mac's `ReadSaveGame` @
//! 1008b560 / `WriteSaveGame` @ 1008b7e0 write the same bytes) as
//! `<saves>/PaybackSlot<n>.save`, n = 1..14, where `<saves>` is
//! [`saves_dir`]: `PAYBACK_SAVES`, else `Saves` next to the executable
//! (saves from the per-user data directory, `%APPDATA%\Payback` on
//! Windows, or an old `./Saves` are copied there on first use).
//!
//! A slot holds only the description, the player's name, and the best time
//! and cup of each of the six levels: no money, score, current level or
//! difficulty (none of the originals stores more). A file shorter than 152
//! bytes reads as an empty slot here (the original would `fread` what is
//! there and keep the rest of the old profile).
//!
//! Settings replace `S:Payback.config` (whose layout is PaybackSetup's
//! and mostly screen modes) with a text file, `<saves>/Payback.cfg`:
//! one `key = value` per line, `#` comments, unknown keys ignored.
//!
//! | key | value |
//! |---|---|
//! | `language` | phrase-file language token, e.g. `English` |
//! | `volume` | sound volume 0..100 |
//! | `fx` | lens-flare FX 0 off, 1 flares, 2 flares and artifacts |
//! | `motion_blur`, `gouraud`, `rotating_view` | 0 / 1 |
//! | `smooth_motion` | 0 / 1: render-only interpolation between game ticks (port-added, default 1; `frame.rs`) |
//! | `vsync` | 0 / 1: present with vsync (port-added, default 1) |
//! | `frame_limit` | frames a second, 0 = none (port-added; 30, 60, 120, 144) |
//! | `zoom_in_map` | 0 / 1: the level-start zoom-in map of software rendering (`hud::zoom`, default 1) |
//! | `view_size` | 3-D view size /256, 176..256 in steps of 16 (config field 12, `DAT_0114c310`; '-' / '=' in game, software rendering) |
//! | `minimap_3d` | 0 / 1: the rotating view's tilted minimap, also without the rotating view (port-added, default 0; forced on by the rotating view) |
//! | `hardware` | 0 software / 1 hardware renderer (config field 19, `DAT_0114c7d8`; forces `gouraud = 1`, `motion_blur = 0`) |
//! | `resolution` | window size in physical pixels, `<w>x<h>`; absent = the default window |
//! | `window_mode` | `windowed`, `borderless` (borderless fullscreen) or `fullscreen` (exclusive, video mode of `resolution`) |
//! | `cd_mode` | music mode 0 off, 1 radio (default), 2 continuous; 3 disabled is forced while no music files are found (`music.rs`) and otherwise reads as radio |
//! | `police` | Rampage police aggressiveness 0..5 (`DAT_0114c2ec`) |
//! | `city` | Rampage / multiplayer city (`DAT_0114c7c0`; 3.. = user maps) |
//! | `unlock_all` | 0 / 1: every city and level selectable (port-added; the profile is not changed) |
//! | `key.<action>` | single-player key, Bevy `KeyCode` name (actions: phrases 2457–2467, then `horn`, a port addition); empty = disabled |
//! | `pad.<action>`, `pad<n>.<action>` | gamepad 1 (single player and multiplayer) / gamepad n = 2..4 button, Bevy `GamepadButton` name |
//! | `mp<n>.key.<action>` | multiplayer key set n (`bindings.rs`) |

use bevy::prelude::*;
use std::path::PathBuf;

/// Target times in minutes, `{gold, silver}` per city and level
/// (`DAT_0117b6c8`, set in `FUN_010138dc`).
pub const TARGETS: [[[i32; 2]; 2]; 3] = [[[8, 12], [10, 14]], [[11, 15], [8, 12]], [[4, 7], [10, 15]]];

pub const SLOTS: usize = 14;
const SAVE_SIZE: usize = 152;

/// Name, best times and cups (`DAT_0117aeac`, `DAT_0117aeb0[6]`,
/// `DAT_0117aec8[6]`), indexed `city*2 + level`.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// Best time in minutes, -1 = none.
    pub best: [i32; 6],
    /// 0 none, 1 bronze, 2 silver, 3 gold.
    pub cups: [i32; 6],
    /// A level has been played since the game was started/loaded (the
    /// game menu's replay items need one).
    pub played: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: "Travis".into(),
            best: [-1; 6],
            cups: [0; 6],
            played: false,
        }
    }
}

impl Profile {
    /// Start a new game (`FUN_0102fc44` item 0).
    pub fn new_game(name: String) -> Profile {
        Profile {
            name,
            ..default()
        }
    }

    /// Overall rating = lowest cup over the six levels, capped at 3.
    pub fn rating(&self) -> i32 {
        self.cups.iter().copied().min().unwrap_or(0).min(3)
    }

    /// Cup for a finished level and store the best time
    /// (`FUN_0102395c`): time <= gold -> 3, <= silver -> 2, else 1. Best
    /// time and cup are kept only if the time beats the old best.
    pub fn award(&mut self, city: usize, level: usize, minutes: i32) -> i32 {
        let [gold, silver] = TARGETS[city][level];
        let cup = if minutes <= gold {
            3
        } else if minutes <= silver {
            2
        } else {
            1
        };
        let i = city * 2 + level;
        if self.best[i] == -1 || minutes < self.best[i] {
            self.best[i] = minutes;
            self.cups[i] = cup;
        }
        cup
    }

    /// A level may be chosen only if the previous one has a cup
    /// (`FUN_01025544`). See [`level_open`] for the "Unlock all" setting.
    pub fn unlocked(&self, index: usize) -> bool {
        index == 0 || self.cups[index - 1] != 0
    }

    /// A built-in city may be chosen in Rampage and multiplayer once the
    /// previous city's last level has a cup (`DAT_0117aecc[(c-1)*2]`,
    /// `FUN_0102a428` / `FUN_0102b2c8`). See [`city_open`].
    pub fn city_unlocked(&self, city: usize) -> bool {
        city == 0 || self.cups.get(2 * city - 1).is_some_and(|&c| c != 0)
    }

    /// The level-select default (`FUN_01025544`): the first level without
    /// a cup; once every level has one, the first with the lowest cup
    /// (capped at gold), or the last level when all are gold.
    pub fn first_open(&self) -> usize {
        if let Some(i) = self.cups.iter().position(|&c| c == 0) {
            return i;
        }
        let min = self.cups.iter().map(|&c| c.min(3)).min().unwrap_or(3);
        if min >= 3 { 5 } else { self.cups.iter().position(|&c| c.min(3) == min).unwrap_or(5) }
    }

    /// The level named in a new slot's description `"%s - C%d, L%d"`
    /// (`FUN_0101f738`, Mac `DoSaveGame` @ 10096d80): the first level
    /// without a cup, or city 3 level 1 once all have one.
    pub fn save_level(&self) -> usize {
        self.cups.iter().position(|&c| c == 0).unwrap_or(4)
    }

    /// Default description of an empty slot.
    pub fn slot_description(&self) -> String {
        let i = self.save_level();
        format!("{} - C{}, L{}", self.name, i / 2 + 1, i % 2 + 1)
    }

    pub fn to_bytes(&self, description: &str) -> Vec<u8> {
        let mut b = Vec::with_capacity(SAVE_SIZE);
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&fixed(description));
        b.extend_from_slice(&fixed(&self.name));
        for i in 0..6 {
            b.extend_from_slice(&self.best[i].to_be_bytes());
            b.extend_from_slice(&self.cups[i].to_be_bytes());
        }
        b
    }

    /// (description, profile); the version field is not checked, as in
    /// the original.
    pub fn from_bytes(b: &[u8]) -> Option<(String, Profile)> {
        if b.len() < SAVE_SIZE {
            return None;
        }
        let i32_at = |o: usize| i32::from_be_bytes(b[o..o + 4].try_into().unwrap());
        let mut p = Profile {
            name: cstr(&b[0x36..0x68]),
            ..default()
        };
        // Garbage is tamed: a negative time is "none", cups are 0..3.
        for i in 0..6 {
            p.best[i] = i32_at(0x68 + 8 * i).max(-1);
            p.cups[i] = i32_at(0x68 + 8 * i + 4).clamp(0, 3);
        }
        Some((cstr(&b[4..0x36]), p))
    }
}

fn fixed(s: &str) -> [u8; 50] {
    let mut out = [0u8; 50];
    for (o, c) in out.iter_mut().take(49).zip(s.chars()) {
        *o = if (c as u32) < 256 { c as u8 } else { b'?' };
    }
    out
}

fn cstr(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
}

#[cfg(test)]
thread_local! {
    /// Tests point the saves at a scratch directory.
    static TEST_SAVES: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Run `f` with the saves in `dir` (tests).
#[cfg(test)]
pub fn with_saves_dir<R>(dir: &std::path::Path, f: impl FnOnce() -> R) -> R {
    TEST_SAVES.with(|t| *t.borrow_mut() = Some(dir.to_path_buf()));
    let r = f();
    TEST_SAVES.with(|t| *t.borrow_mut() = None);
    r
}

/// Where saves, settings, scores and replays live: `PAYBACK_SAVES` if
/// set, else `Saves` next to the executable, as the original kept them
/// next to the game (`PROGDIR:Saves`; `S:Payback.config`). If that folder
/// can't be created (a read-only install folder), the per-user data
/// directory ([`user_data_dir`]), else `Saves` in the working directory.
/// The first time the folder next to the executable is missing or empty,
/// the saves of an earlier build are copied into it
/// ([`migrate_legacy_saves`]).
pub fn saves_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(d) = TEST_SAVES.with(|t| t.borrow().clone()) {
        return d;
    }
    // Web build: the saves live in localStorage (`crate::vfs`).
    #[cfg(target_arch = "wasm32")]
    return PathBuf::from(crate::vfs::SAVES);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(d) = std::env::var_os("PAYBACK_SAVES").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("Saves")));
            if let Some(d) = exe {
                migrate_legacy_saves(&d);
                match std::fs::create_dir_all(&d) {
                    Ok(()) => return d,
                    Err(e) => eprintln!("saves: can't create {}: {e}", d.display()),
                }
            }
            user_data_dir().filter(|d| std::fs::create_dir_all(d).is_ok()).unwrap_or_else(|| PathBuf::from("Saves"))
        })
        .clone()
    }
}

/// The per-user data directory: `%APPDATA%\Payback` on Windows,
/// `~/Library/Application Support/Payback` on macOS,
/// `$XDG_DATA_HOME/payback` (default `~/.local/share/payback`) elsewhere.
/// Earlier builds kept the saves there; now only a fallback and a
/// migration source.
#[cfg(not(target_arch = "wasm32"))]
pub fn user_data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA").map(|d| d.join("Payback"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Payback"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("payback"))
    }
}

/// A directory with something in it.
#[cfg(not(target_arch = "wasm32"))]
fn has_files(d: &std::path::Path) -> bool {
    std::fs::read_dir(d).is_ok_and(|mut d| d.next().is_some())
}

/// First run with the saves next to the executable (`to` missing or
/// empty): copy in what an earlier build saved, from the per-user data
/// directory ([`user_data_dir`], where the previous builds kept them) or
/// else `Saves` in the working directory (older builds). Only the first of
/// these that holds anything is copied; the originals are left in place.
#[cfg(not(target_arch = "wasm32"))]
fn migrate_legacy_saves(to: &std::path::Path) {
    if has_files(to) {
        return;
    }
    let same = |a: &std::path::Path| match (std::fs::canonicalize(a), std::fs::canonicalize(to)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    let Some(from) = [user_data_dir(), Some(PathBuf::from("Saves"))]
        .into_iter()
        .flatten()
        .find(|d| has_files(d) && !same(d))
    else {
        return;
    };
    // Usually runs before the log is set up (the settings load at
    // startup), so it goes to stderr as well.
    let msg = match copy_tree(&from, to) {
        Ok(n) => format!("saves: migrated {n} files from {} to {}", from.display(), to.display()),
        Err(e) => format!("saves: copying {} to {}: {e}", from.display(), to.display()),
    };
    eprintln!("{msg}");
    info!("{msg}");
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut n = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            n += copy_tree(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
            n += 1;
        }
    }
    Ok(n)
}

fn slot_path(slot: usize) -> PathBuf {
    saves_dir().join(format!("PaybackSlot{}.save", slot + 1))
}

/// Write a save file (slot, settings, scores, replay, ...) through a
/// temporary file and a rename, so a crash never leaves a half-written save
/// behind. All save IO goes through here and [`read_file`]: files natively,
/// localStorage in the web build (`crate::vfs`).
pub fn write_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        crate::vfs::create_dir_all(dir)?;
    }
    #[cfg(target_arch = "wasm32")]
    return crate::vfs::write(path, bytes);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        crate::vfs::write(&tmp, bytes)?;
        crate::vfs::rename(&tmp, path)
    }
}

/// Read a save file written by [`write_file`].
pub fn read_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    crate::vfs::read(path)
}

/// Load slot 0..13 (`FUN_0101182c`); `None` = missing or too short.
pub fn load_slot(slot: usize) -> Option<(String, Profile)> {
    read_file(&slot_path(slot)).ok().and_then(|b| Profile::from_bytes(&b))
}

/// Write slot 0..13 (`FUN_01011a70`), replacing what was there.
pub fn save_slot(slot: usize, description: &str, profile: &Profile) -> std::io::Result<()> {
    write_file(&slot_path(slot), &profile.to_bytes(description))
}

/// Slot descriptions (`FUN_01011da8`); `None` = empty slot ("Slot %d").
pub fn slot_names() -> Vec<Option<String>> {
    (0..SLOTS).map(|s| load_slot(s).map(|(d, _)| d)).collect()
}

/// A story level may be chosen: the profile's rule, or "Unlock all".
pub fn level_open(profile: &Profile, settings: &Settings, index: usize) -> bool {
    settings.unlock_all || profile.unlocked(index)
}

/// A built-in city may be chosen (Rampage, multiplayer): the profile's
/// rule, or "Unlock all".
pub fn city_open(profile: &Profile, settings: &Settings, city: usize) -> bool {
    settings.unlock_all || profile.city_unlocked(city)
}

/// Debug (`--autoplay-save <n>`, with `--autoplay`): when the story level
/// is completed, award its cup as Level Complete does (`FUN_0102395c`)
/// and write the profile to slot n (1..14) with the empty-slot
/// description, so a scripted run can check the save end to end.
pub fn autoplay_save(
    mut outcomes: MessageReader<crate::mission::LevelOutcome>,
    clock: Res<crate::mission::LevelClock>,
    spec: Res<crate::level::LevelSpec>,
    mode: Res<crate::level::GameMode>,
    mut profile: ResMut<Profile>,
) {
    let Some(slot) = crate::arg("--autoplay-save").and_then(|v| v.parse::<usize>().ok()) else {
        outcomes.clear();
        return;
    };
    for o in outcomes.read() {
        let (crate::mission::LevelOutcome::Complete, Some(city), crate::level::GameMode::Story) =
            (o, spec.city(), &*mode)
        else {
            continue;
        };
        let (city, level) = (city.min(2) as usize, spec.level.min(1) as usize);
        let cup = profile.award(city, level, clock.minutes() as i32);
        let desc = profile.slot_description();
        let slot = slot.clamp(1, SLOTS) - 1;
        match save_slot(slot, &desc, &profile) {
            Ok(()) => info!("autoplay-save: cup {cup}, slot {} \"{desc}\": {:?}", slot + 1, *profile),
            Err(e) => warn!("autoplay-save: slot {}: {e}", slot + 1),
        }
    }
}

/// Debug (`--load-slot <n>`): load slot n (1..14) into the profile as
/// "Load saved game" does, and log what the level select will show.
pub fn debug_load(profile: &mut Profile) {
    let Some(n) = crate::arg("--load-slot").and_then(|v| v.parse::<usize>().ok()) else {
        return;
    };
    match load_slot(n.clamp(1, SLOTS) - 1) {
        Some((desc, p)) => {
            *profile = Profile { played: false, ..p };
            let i = profile.first_open();
            info!("load-slot {n}: \"{desc}\": {profile:?}; level select on city {} level {}", i / 2, i % 2);
        }
        None => warn!("load-slot {n}: empty"),
    }
}

/// Stored options (pause menu, language, sound, controls).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Empty = ask at start (`DAT_012a63d4`).
    pub language: String,
    pub volume: u8,
    pub fx: u8,
    pub motion_blur: bool,
    pub gouraud: bool,
    pub rotating_view: bool,
    /// "3D minimap" (port-added, default off): the rotating view's tilted
    /// minimap, also without the rotating view (`hud::minimap_3d`). The
    /// rotating view always uses it, as in the original; see
    /// [`Settings::minimap_3d_on`].
    pub minimap_3d: bool,
    /// Hardware (Warp3D) rendering, config field 19 (`DAT_0114c7d8`).
    pub hardware: bool,
    /// Window size in physical pixels (PaybackSetup's screen mode); None =
    /// leave the window as created.
    pub resolution: Option<(u32, u32)>,
    pub window_mode: super::display::WindowChoice,
    pub cd_mode: u8,
    /// Rampage police aggressiveness 0..5 (config field 13).
    pub police: u8,
    /// Rampage and multiplayer city (config field 11).
    pub rampage_city: u8,
    /// Key and gamepad bindings (`DAT_012a63fc`, `bindings.rs`).
    pub controls: super::bindings::Controls,
    /// Multiplayer setup: each player's device (config: 4 × {device, key
    /// set}; here the key set is part of the device).
    pub mp_devices: [crate::multiplayer::Device; 4],
    /// Multiplayer required kills 1..50 (`DAT_0114c2e8`, config field 10).
    pub required_kills: u8,
    /// Mac-or-Amiga content variants (decisions in `docs/re/mac_version.md`).
    pub variants: Variants,
    /// Hi-res HUD at >= 640x400 (both builds switch at W > 639 && H > 399);
    /// false = always the lo-res 320x256 layout.
    pub hires_hud: bool,
    /// "Unlock all levels" (port-added): every story level and built-in
    /// city is selectable whatever the profile's cups; the profile itself
    /// is not changed, so turning it off restores the original gating.
    pub unlock_all: bool,
    /// Fixed original framing (HFOV ~64 deg, the 320x256 look on 4:3);
    /// false (the default) = widen with the window aspect.
    pub original_aspect: bool,
    /// "Smooth motion" (port-added, default on): objects and cameras drawn
    /// between the last two game ticks (`crate::frame`); off = the
    /// original's one new picture per tick.
    pub smooth_motion: bool,
    /// Present with vsync (port-added; the Mac requests none).
    pub vsync: bool,
    /// Frame limit in frames a second, 0 = none (`crate::frame::FRAME_LIMITS`).
    pub frame_limit: u16,
    /// "Zoom-in map" (port option, default on): the level-start zoom of
    /// software rendering (`hud::zoom`).
    pub zoom_in_map: bool,
    /// 3-D view size in 1/256 of the window, 0xB0..0x100 (config field 12,
    /// `DAT_0114c310`; '-' and '=' in game, `camera::view_size`).
    pub view_size: u16,
}

/// Mac-or-Amiga content choices (true = Mac; a Mac choice only applies when
/// the Mac data is present). Render and audio only, except
/// `random_missions`, which changes the simulation and is recorded in replays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variants {
    /// Mac cube textures; false = the "Amiga look".
    pub textures: bool,
    /// Mac `Blood` day and night; false = Amiga `Blood` + `Blood.dark` at night.
    pub blood: bool,
    /// Mac menu props (PC, gamepad, floppy, remodelled cup/world/CD).
    pub menu_props: bool,
    /// Mac title screens.
    pub title: bool,
    /// Mac MPEG intro film; false = the Amiga FLC.
    pub intro: bool,
    /// Mac scenery heights (shorter phones, traffic lights, tree crowns).
    pub scenery: bool,
    /// German: `EffectTextures/Gas` in the blood slot (Mac `DoSetLanguage`).
    pub gas: bool,
    /// Mac one-shot cut-off at ~65 % of the sample (slot clock).
    pub oneshot_cut: bool,
    /// Random phone missions in Rampage (Mac `ProcessTrigger_Random`).
    pub random_missions: bool,
}

impl Default for Variants {
    fn default() -> Self {
        Variants {
            textures: true,
            blood: true,
            menu_props: true,
            title: true,
            intro: true,
            scenery: true,
            gas: false,
            oneshot_cut: false,
            random_missions: true,
        }
    }
}

impl Variants {
    /// (config key, field) pairs, for load and save.
    fn fields(&mut self) -> [(&'static str, &mut bool); 9] {
        [
            ("mac.textures", &mut self.textures),
            ("mac.blood", &mut self.blood),
            ("mac.menu_props", &mut self.menu_props),
            ("mac.title", &mut self.title),
            ("mac.intro", &mut self.intro),
            ("mac.scenery", &mut self.scenery),
            ("gas", &mut self.gas),
            ("oneshot_cut", &mut self.oneshot_cut),
            ("random_missions", &mut self.random_missions),
        ]
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: String::new(),
            // The Mac default (`ClearPreference` @ 100508c0); the Amiga's is 100.
            volume: 50,
            fx: 2,
            motion_blur: false,
            gouraud: true,
            rotating_view: false,
            minimap_3d: false,
            // Port default (user choice); the original starts in software.
            hardware: true,
            resolution: None,
            window_mode: default(),
            // Radio, the Mac default; 3 (Disabled) is forced while no music
            // is found (`crate::music`).
            cd_mode: 1,
            police: 2,
            rampage_city: 0,
            controls: default(),
            mp_devices: {
                use crate::multiplayer::Device;
                // The Mac default (`ClearPreference` @ 100508c0): player 1
                // Keyboard, player 2 device 1 (Amiga: joystick 0, keyboard).
                [Device::Keyboard(0), Device::Joystick(0), Device::Off, Device::Off]
            },
            required_kills: 5,
            variants: default(),
            hires_hud: true,
            unlock_all: false,
            // Port default (user choice): widen with the window.
            original_aspect: false,
            smooth_motion: true,
            vsync: true,
            frame_limit: 0,
            view_size: 0x100,
            zoom_in_map: true,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        saves_dir().join("Payback.cfg")
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = read_file(&Self::path()).map(|b| String::from_utf8_lossy(&b).into_owned()) else {
            return s;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            let num = |max: u8| v.parse::<u8>().ok().map(|n| n.min(max));
            match k {
                "language" => s.language = v.to_string(),
                "volume" => s.volume = num(100).unwrap_or(s.volume),
                "fx" => s.fx = num(2).unwrap_or(s.fx),
                "motion_blur" => s.motion_blur = v == "1",
                "gouraud" => s.gouraud = v == "1",
                "rotating_view" => s.rotating_view = v == "1",
                "minimap_3d" => s.minimap_3d = v == "1",
                "hardware" => s.hardware = v == "1",
                "resolution" => s.resolution = super::display::parse_size(v).or(s.resolution),
                "window_mode" => s.window_mode = super::display::WindowChoice::parse(v).unwrap_or(s.window_mode),
                "cd_mode" => s.cd_mode = num(3).unwrap_or(s.cd_mode),
                "police" => s.police = num(5).unwrap_or(s.police),
                // >= 3: a user map (checked against the list when used).
                "city" => s.rampage_city = num(u8::MAX).unwrap_or(s.rampage_city),
                "required_kills" => s.required_kills = num(50).unwrap_or(s.required_kills).max(1),
                "hires_hud" => s.hires_hud = v == "1",
                "original_aspect" => s.original_aspect = v == "1",
                "unlock_all" => s.unlock_all = v == "1",
                "smooth_motion" => s.smooth_motion = v == "1",
                "zoom_in_map" => s.zoom_in_map = v == "1",
                "view_size" => {
                    let n = v.parse::<u16>().ok().map(|n| n.clamp(0xB0, 0x100) & !0xF);
                    s.view_size = n.unwrap_or(s.view_size);
                }
                "vsync" => s.vsync = v == "1",
                "frame_limit" => {
                    let n = v.parse::<u16>().ok().filter(|n| crate::frame::FRAME_LIMITS.contains(n));
                    s.frame_limit = n.unwrap_or(s.frame_limit);
                }
                _ => {
                    if let Some((_, f)) = s.variants.fields().into_iter().find(|(n, _)| *n == k) {
                        *f = v == "1";
                        continue;
                    }
                    if let Some(p) = k.strip_prefix("mp.player").and_then(|p| p.parse::<usize>().ok()) {
                        if let (Some(slot), Some(d)) =
                            (s.mp_devices.get_mut(p.wrapping_sub(1)), crate::multiplayer::Device::parse(v))
                        {
                            *slot = d;
                        }
                    }
                    s.controls.read(k, v);
                }
            }
        }
        s.apply_hardware();
        s
    }

    /// Hardware rendering forces Gouraud shading on (LoadConfig
    /// `FUN_0104199c`: `DAT_0114c7d8 != 0` -> `DAT_0114c2fc = 1`) and has
    /// no motion blur (the pause menu refuses it, `FUN_0101a7dc` case 3).
    pub fn apply_hardware(&mut self) {
        if self.hardware {
            self.gouraud = true;
            self.motion_blur = false;
        }
    }

    /// The 3D minimap is drawn: chosen, or forced by the rotating view (the
    /// original draws the tilted minimap whenever the view rotates,
    /// `DisplayMinimap` @ 10091630).
    pub fn minimap_3d_on(&self) -> bool {
        self.minimap_3d || self.rotating_view
    }

    pub fn save(&self) {
        let mut t = String::from("# Payback settings (see crates/payback/src/frontend/progress.rs)\n");
        let b = |v: bool| if v { 1 } else { 0 };
        t += &format!("language = {}\n", self.language);
        t += &format!("volume = {}\n", self.volume);
        t += &format!("fx = {}\n", self.fx);
        t += &format!("motion_blur = {}\n", b(self.motion_blur));
        t += &format!("gouraud = {}\n", b(self.gouraud));
        t += &format!("rotating_view = {}\n", b(self.rotating_view));
        t += &format!("minimap_3d = {}\n", b(self.minimap_3d));
        t += &format!("hardware = {}\n", b(self.hardware));
        if let Some((w, h)) = self.resolution {
            t += &format!("resolution = {w}x{h}\n");
        }
        t += &format!("window_mode = {}\n", self.window_mode.name());
        t += &format!("cd_mode = {}\n", self.cd_mode);
        t += &format!("police = {}\n", self.police);
        t += &format!("city = {}\n", self.rampage_city);
        t += &format!("required_kills = {}\n", self.required_kills);
        t += &format!("hires_hud = {}\n", b(self.hires_hud));
        t += &format!("original_aspect = {}\n", b(self.original_aspect));
        t += &format!("unlock_all = {}\n", b(self.unlock_all));
        t += &format!("smooth_motion = {}\n", b(self.smooth_motion));
        t += &format!("vsync = {}\n", b(self.vsync));
        t += &format!("frame_limit = {}\n", self.frame_limit);
        t += &format!("view_size = {}\n", self.view_size);
        t += &format!("zoom_in_map = {}\n", b(self.zoom_in_map));
        for (k, v) in self.variants.clone().fields() {
            t += &format!("{k} = {}\n", b(*v));
        }
        for (i, d) in self.mp_devices.iter().enumerate() {
            t += &format!("mp.player{} = {}\n", i + 1, d.name());
        }
        self.controls.write(&mut t);
        if let Err(e) = write_file(&Self::path(), t.as_bytes()) {
            warn!("saving settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::scratch;
    use super::*;

    #[test]
    fn save_roundtrip() {
        let mut p = Profile::new_game("Travis".into());
        p.best[3] = 9;
        p.cups[3] = 2;
        let b = p.to_bytes("Travis - C2, L1");
        assert_eq!(b.len(), SAVE_SIZE);
        assert_eq!(&b[0..4], &[0, 0, 0, 1]);
        assert_eq!(b[0x36], b'T');
        let (d, q) = Profile::from_bytes(&b).unwrap();
        assert_eq!(d, "Travis - C2, L1");
        assert_eq!(q, p);
    }

    #[test]
    fn cups() {
        let mut p = Profile::default();
        assert_eq!(p.award(0, 0, 8), 3);
        assert_eq!(p.award(0, 0, 12), 2);
        assert_eq!(p.cups[0], 3, "worse time keeps the old cup");
        assert_eq!(p.award(2, 0, 20), 1);
        assert_eq!(p.rating(), 0);
    }

    /// The byte layout of `FUN_01011a70` / Mac `WriteSaveGame`: per city,
    /// best[2c], cup[2c], best[2c+1], cup[2c+1], big-endian.
    #[test]
    fn save_layout() {
        let mut p = Profile::new_game("Ann".into());
        p.best = [7, 13, -1, 9, 4, 30];
        p.cups = [3, 2, 0, 1, 3, 1];
        let b = p.to_bytes("desc");
        let at = |o: usize| i32::from_be_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!(&b[4..9], b"desc\0");
        assert_eq!(&b[0x36..0x3a], b"Ann\0");
        for i in 0..6 {
            assert_eq!(at(0x68 + 8 * i), p.best[i]);
            assert_eq!(at(0x6c + 8 * i), p.cups[i]);
        }
        // Descriptions are cut to 49 bytes plus the NUL.
        let long = "x".repeat(60);
        let (d, _) = Profile::from_bytes(&p.to_bytes(&long)).unwrap();
        assert_eq!(d.len(), 49);
    }

    #[test]
    fn short_and_corrupt_files() {
        let mut p = Profile::new_game("Bo".into());
        p.cups[0] = 2;
        p.best[0] = 11;
        let mut b = p.to_bytes("ok");
        assert!(Profile::from_bytes(&b[..SAVE_SIZE - 1]).is_none());
        assert!(Profile::from_bytes(&[]).is_none());
        // Trailing bytes are ignored; the version is not checked.
        b[3] = 7;
        b.extend_from_slice(&[0xAA; 10]);
        assert_eq!(Profile::from_bytes(&b).unwrap().1, p);
        // Garbage values are tamed; strings without a NUL end at the field.
        let mut g = vec![0xFFu8; SAVE_SIZE];
        g[0x6c..0x70].copy_from_slice(&99i32.to_be_bytes());
        let (d, q) = Profile::from_bytes(&g).unwrap();
        assert_eq!(d.chars().count(), 50);
        assert_eq!(q.best, [-1; 6]);
        assert_eq!(q.cups, [3, 0, 0, 0, 0, 0]);
    }

    /// Level-select default (`FUN_01025544`) and the save description
    /// (`FUN_0101f738`).
    #[test]
    fn defaults() {
        let mut p = Profile::new_game("Cy".into());
        assert_eq!((p.first_open(), p.slot_description()), (0, "Cy - C1, L1".to_string()));
        p.cups = [3, 1, 2, 0, 0, 0];
        assert_eq!((p.first_open(), p.slot_description()), (3, "Cy - C2, L2".to_string()));
        p.cups = [3, 2, 3, 1, 1, 2];
        assert_eq!(p.first_open(), 3, "first lowest cup");
        assert_eq!(p.slot_description(), "Cy - C3, L1", "all done: C3, L1 (both builds)");
        p.cups = [2, 3, 3, 3, 3, 3];
        assert_eq!(p.first_open(), 0);
        p.cups = [3; 6];
        assert_eq!(p.first_open(), 5);
    }

    #[test]
    fn unlock_all_gating() {
        let p = Profile::new_game("Di".into());
        let mut s = Settings::default();
        assert!(level_open(&p, &s, 0) && !level_open(&p, &s, 1) && !level_open(&p, &s, 5));
        assert!(city_open(&p, &s, 0) && !city_open(&p, &s, 1) && !city_open(&p, &s, 2));
        s.unlock_all = true;
        assert!((0..6).all(|i| level_open(&p, &s, i)));
        assert!((0..3).all(|c| city_open(&p, &s, c)));
        assert_eq!(p, Profile::new_game("Di".into()), "the profile is not changed");
        // Progress: city 1 opens with city 0's last level.
        let mut q = p.clone();
        q.cups[0] = 1;
        assert!(q.unlocked(1) && !q.city_unlocked(1));
        q.cups[1] = 1;
        assert!(q.city_unlocked(1) && !q.city_unlocked(2));
    }

    #[test]
    fn slot_files() {
        let dir = scratch("slots");
        with_saves_dir(&dir, || {
            assert_eq!(saves_dir(), dir);
            assert_eq!(slot_names(), vec![None; SLOTS]);
            assert!(load_slot(0).is_none());
            let mut p = Profile::new_game("Ed".into());
            p.award(0, 0, 9);
            save_slot(2, "first", &p).unwrap();
            assert!(dir.join("PaybackSlot3.save").is_file());
            assert!(!dir.join("PaybackSlot3.save.tmp").exists());
            assert_eq!(std::fs::read(dir.join("PaybackSlot3.save")).unwrap().len(), SAVE_SIZE);
            let names = slot_names();
            assert_eq!(names[2].as_deref(), Some("first"));
            assert_eq!(names.iter().filter(|n| n.is_some()).count(), 1);
            // Overwrite.
            p.award(0, 1, 20);
            save_slot(2, "second", &p).unwrap();
            assert_eq!(load_slot(2), Some(("second".to_string(), p.clone())));
            // A truncated file is an empty slot.
            std::fs::write(dir.join("PaybackSlot5.save"), [0u8; 40]).unwrap();
            assert!(load_slot(4).is_none() && slot_names()[4].is_none());
            // Slot 14 is the last one.
            save_slot(13, "last", &p).unwrap();
            assert!(dir.join("PaybackSlot14.save").is_file());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_file() {
        let dir = scratch("settings");
        with_saves_dir(&dir, || {
            assert_eq!(Settings::load(), Settings::default(), "no file: defaults");
            Settings::default().save();
            assert_eq!(Settings::load(), Settings::default());
            let mut s = Settings::default();
            s.unlock_all = true;
            s.language = "Deutsch".into();
            s.volume = 70;
            s.police = 4;
            s.rampage_city = 2;
            s.required_kills = 12;
            s.variants.textures = false;
            s.resolution = Some((800, 600));
            s.smooth_motion = false;
            s.vsync = false;
            s.frame_limit = 60;
            s.save();
            let text = std::fs::read_to_string(dir.join("Payback.cfg")).unwrap();
            assert!(text.contains("unlock_all = 1\n"));
            assert_eq!(Settings::load(), s);
            // Unknown keys and junk lines are ignored, bad numbers keep the default.
            std::fs::write(dir.join("Payback.cfg"), "# x\nfoo = 1\nnonsense\nvolume = loud\nunlock_all = 1\n").unwrap();
            let l = Settings::load();
            assert!(l.unlock_all);
            assert_eq!(l.volume, Settings::default().volume);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rampage high scores persist (the Mac's `SaveHiscores` @ 1009eaa0
    /// bug, which never persists them, is not copied).
    #[test]
    fn high_scores_persist() {
        let dir = scratch("scores");
        let data = testing::data();
        with_saves_dir(&dir, || {
            let mut t = crate::rampage::load_scores(&data, 1);
            assert_eq!(t, payback_formats::scores::HighScores::default());
            assert!(t.insert(2, "Fay", 9_999_999).is_some());
            crate::rampage::save_scores(1, &t);
            assert!(dir.join("1.scores").is_file());
            let back = crate::rampage::load_scores(&data, 1);
            assert_eq!(back, t);
            assert_eq!(back.tables[2][0].name, "Fay");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A menu context without the game data or a window, for screen tests.
#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::data::GameData;
    use crate::frontend::ui::Ctx;
    use crate::hud::{Language, Texts};

    /// A fresh scratch directory for one test.
    pub fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("payback_saves_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    pub fn data() -> GameData {
        GameData {
            root: PathBuf::from("no-game-data"),
            amiga: true,
            mac: None,
            palette: [0; 768],
            cube_textures: payback_formats::CubeTextureList { entries: Vec::new() },
            mac_cube_textures: true,
            custom_textures: Default::default(),
        }
    }

    pub struct Env {
        pub data: GameData,
        pub texts: Texts,
        pub profile: Profile,
        pub settings: Settings,
        pub language: Language,
        pub mp: crate::multiplayer::MpTotals,
        pub display: super::super::display::Display,
    }

    impl Env {
        pub fn new() -> Env {
            Env {
                data: data(),
                texts: Texts::default(),
                profile: Profile::default(),
                settings: Settings::default(),
                language: Language(String::new()),
                mp: Default::default(),
                display: Default::default(),
            }
        }

        pub fn cx(&mut self) -> Ctx<'_> {
            Ctx {
                data: &self.data,
                texts: &self.texts,
                profile: &mut self.profile,
                settings: &mut self.settings,
                language: &mut self.language,
                languages: &[],
                goal: 0,
                level_ticks: 0,
                story: false,
                last_replay: None,
                mp_totals: &mut self.mp,
                user_maps: &[],
                hardware: false,
                display: &self.display,
                pads: 0,
                sfx: Vec::new(),
                requests: Vec::new(),
            }
        }
    }
}
