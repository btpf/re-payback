//! Player progress, save slots and the settings file
//! (docs/re/frontend_menus.md §4–6).
//!
//! Save slots keep the original 152-byte big-endian layout
//! (`FUN_0101182c` load, `FUN_01011a70` save) as
//! `Saves/PaybackSlot<n>.save`, n = 1..14, relative to the working
//! directory.
//!
//! Settings replace `S:Payback.config` (whose layout is PaybackSetup's
//! and mostly screen modes) with a text file, `Saves/Payback.cfg`:
//! one `key = value` per line, `#` comments, unknown keys ignored.
//!
//! | key | value |
//! |---|---|
//! | `language` | phrase-file language token, e.g. `English` |
//! | `volume` | sound volume 0..100 |
//! | `fx` | lens-flare FX 0 off, 1 flares, 2 flares and artifacts |
//! | `motion_blur`, `gouraud`, `rotating_view` | 0 / 1 |
//! | `cd_mode` | CD playback 0 off, 1 radio, 2 continuous, 3 disabled |
//! | `police` | Rampage police aggressiveness 0..5 (`DAT_0114c2ec`) |
//! | `city` | Rampage / multiplayer city (`DAT_0114c7c0`) |
//! | `key.<action>` | Bevy `KeyCode` name per action (phrases 2457–2467) |

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

    /// A level may be chosen only if the previous one has a cup.
    pub fn unlocked(&self, index: usize) -> bool {
        index == 0 || self.cups[index - 1] != 0
    }

    /// First level without a cup (the level-select default), or the last.
    pub fn first_open(&self) -> usize {
        self.cups.iter().position(|&c| c == 0).unwrap_or(5)
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
        for i in 0..6 {
            p.best[i] = i32_at(0x68 + 8 * i);
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

pub fn saves_dir() -> PathBuf {
    PathBuf::from("Saves")
}

fn slot_path(slot: usize) -> PathBuf {
    saves_dir().join(format!("PaybackSlot{}.save", slot + 1))
}

/// Load slot 0..13.
pub fn load_slot(slot: usize) -> Option<(String, Profile)> {
    std::fs::read(slot_path(slot)).ok().and_then(|b| Profile::from_bytes(&b))
}

pub fn save_slot(slot: usize, description: &str, profile: &Profile) -> std::io::Result<()> {
    std::fs::create_dir_all(saves_dir())?;
    std::fs::write(slot_path(slot), profile.to_bytes(description))
}

/// Slot descriptions (`FUN_01011da8`); `None` = empty slot.
pub fn slot_names() -> Vec<Option<String>> {
    (0..SLOTS).map(|s| load_slot(s).map(|(d, _)| d)).collect()
}

/// Control actions in phrase order 2457..2467.
pub const ACTIONS: [&str; 11] = [
    "accelerate",
    "reverse",
    "left",
    "right",
    "shoot",
    "shoot_alt",
    "in_out",
    "brake",
    "next_weapon",
    "prev_weapon",
    "repeat_message",
];

/// The keyboard bindings the game reads (`sim::people::read_keyboard`),
/// in [`ACTIONS`] order; empty = not bound.
const DEFAULT_KEYS: [&str; 11] = [
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Space",
    "ControlLeft",
    "Enter",
    "ShiftLeft",
    "",
    "",
    "",
];

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
    pub cd_mode: u8,
    /// Rampage police aggressiveness 0..5 (config field 13).
    pub police: u8,
    /// Rampage and multiplayer city (config field 11).
    pub rampage_city: u8,
    pub keys: [String; 11],
    /// Multiplayer setup: each player's device (config: 4 × {device, key
    /// set}; here the key set is part of the device).
    pub mp_devices: [crate::multiplayer::Device; 4],
    /// Multiplayer required kills 1..50 (`DAT_0114c2e8`, config field 10).
    pub required_kills: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: String::new(),
            volume: 100,
            fx: 2,
            motion_blur: false,
            gouraud: true,
            rotating_view: false,
            cd_mode: 3,
            police: 2,
            rampage_city: 0,
            keys: DEFAULT_KEYS.map(String::from),
            mp_devices: {
                use crate::multiplayer::Device;
                [Device::Keyboard(0), Device::Keyboard(1), Device::Off, Device::Off]
            },
            required_kills: 5,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        saves_dir().join("Payback.cfg")
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(Self::path()) else {
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
                "cd_mode" => s.cd_mode = num(3).unwrap_or(s.cd_mode),
                "police" => s.police = num(5).unwrap_or(s.police),
                "city" => s.rampage_city = num(2).unwrap_or(s.rampage_city),
                "required_kills" => s.required_kills = num(50).unwrap_or(s.required_kills).max(1),
                _ => {
                    if let Some(p) = k.strip_prefix("mp.player").and_then(|p| p.parse::<usize>().ok()) {
                        if let (Some(slot), Some(d)) =
                            (s.mp_devices.get_mut(p.wrapping_sub(1)), crate::multiplayer::Device::parse(v))
                        {
                            *slot = d;
                        }
                    }
                    if let Some(a) = k.strip_prefix("key.") {
                        if let Some(i) = ACTIONS.iter().position(|x| *x == a) {
                            s.keys[i] = v.to_string();
                        }
                    }
                }
            }
        }
        s
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
        t += &format!("cd_mode = {}\n", self.cd_mode);
        t += &format!("police = {}\n", self.police);
        t += &format!("city = {}\n", self.rampage_city);
        t += &format!("required_kills = {}\n", self.required_kills);
        for (i, d) in self.mp_devices.iter().enumerate() {
            t += &format!("mp.player{} = {}\n", i + 1, d.name());
        }
        for (a, k) in ACTIONS.iter().zip(&self.keys) {
            t += &format!("key.{a} = {k}\n");
        }
        if let Err(e) = std::fs::create_dir_all(saves_dir()).and_then(|_| std::fs::write(Self::path(), t)) {
            warn!("saving settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
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
}
