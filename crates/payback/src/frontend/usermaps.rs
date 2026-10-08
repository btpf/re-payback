//! User maps (docs/re/frontend_menus.md §11): InitUserMap (`FUN_0100a1d4`)
//! lists `PROGDIR:UserMaps/#?.map` and AddUserMap (`FUN_01009e88`) keeps
//! every file but `test.map` (max 2000), with the path
//! `PROGDIR:UserMaps/<name>` (`DAT_01170808`) and the display name
//! `<name>` (`DAT_01172748`). The Rampage and multiplayer city rows offer
//! them after the three built-in cities as cities 3.. (`FUN_01009ff8`;
//! preview `FUN_01027164` loads `<map>.thumb`), and their high scores are
//! `<map>.scores` (`FUN_01029b3c`).
//!
//! Directories searched, in order (a name found twice is kept once):
//! `UserMaps/` in the game data, next to the executable, and in the
//! working directory. Lists are sorted by name (the original keeps the
//! directory order).
//!
//! High scores: read from `Saves/UserMaps/<name>.scores`, else next to the
//! map; written next to the map when it is outside the game data,
//! otherwise to `Saves/UserMaps/`, so the game data stays untouched.

use super::assets::Picture;
use super::rampage::unlocked;
use super::ui::Ctx;
use crate::data::GameData;
use crate::level::LevelSpec;
use bevy::prelude::*;
use payback_formats::scores::HighScores;
use std::path::{Path, PathBuf};

/// Built-in cities before the user maps.
pub const BUILT_IN: usize = 3;
const MAX: usize = 2000;

pub struct UserMap {
    /// Display name: the file name without `.map`.
    pub name: String,
    /// The `.map` file, relative to the game data or absolute.
    pub path: PathBuf,
    /// The map lives in the game data.
    in_data: bool,
    /// `<map>.thumb` (256×256), black if missing.
    pub thumb: Picture,
}

/// Every user map found at start-up.
#[derive(Resource, Default)]
pub struct UserMaps(pub Vec<UserMap>);

impl UserMaps {
    pub fn scan(data: &GameData) -> UserMaps {
        let mut dirs: Vec<(PathBuf, bool)> = vec![(data.root.join("UserMaps"), true)];
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            dirs.push((exe.join("UserMaps"), false));
        }
        dirs.push((PathBuf::from("UserMaps"), false));
        let mut seen_dirs: Vec<PathBuf> = Vec::new();
        let mut maps: Vec<UserMap> = Vec::new();
        for (dir, in_data) in dirs {
            let Ok(canon) = dir.canonicalize() else { continue };
            if seen_dirs.contains(&canon) {
                continue;
            }
            seen_dirs.push(canon);
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut found: Vec<String> = entries
                .flatten()
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .filter(|n| n.to_lowercase().ends_with(".map") && !n.eq_ignore_ascii_case("test.map"))
                .collect();
            found.sort_by_key(|n| n.to_lowercase());
            for file in found {
                let name = file[..file.len() - 4].to_string();
                if name.is_empty() || maps.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                if maps.len() >= MAX {
                    warn!("AddUserMap(): Too many user maps...");
                    break;
                }
                let path = if in_data {
                    Path::new("UserMaps").join(&file)
                } else {
                    std::path::absolute(dir.join(&file)).unwrap_or_else(|_| dir.join(&file))
                };
                let thumb = Picture::load_or_black(data, &path.with_extension("thumb"), 256, 256);
                maps.push(UserMap {
                    name,
                    path,
                    in_data,
                    thumb,
                });
            }
        }
        maps.sort_by_key(|m| m.name.to_lowercase());
        if !maps.is_empty() {
            info!("user maps: {}", maps.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", "));
        }
        UserMaps(maps)
    }
}

impl UserMap {
    pub fn spec(&self) -> LevelSpec {
        LevelSpec::user_map(&self.path)
    }

    fn saved_scores(&self) -> PathBuf {
        super::progress::saves_dir().join("UserMaps").join(format!("{}.scores", self.name))
    }

    pub fn load_scores(&self, data: &GameData) -> HighScores {
        std::fs::read(self.saved_scores())
            .ok()
            .and_then(|b| HighScores::parse(&b))
            .or_else(|| data.read(self.path.with_extension("scores")).ok().and_then(|b| HighScores::parse(&b)))
            .unwrap_or_default()
    }

    pub fn save_scores(&self, scores: &HighScores) {
        let path = if self.in_data {
            self.saved_scores()
        } else {
            self.path.with_extension("scores")
        };
        let res = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, scores.write()));
        if let Err(e) = res {
            warn!("{}: {e}", path.display());
        }
    }
}

/// City numbers: 0..2 built in, then the user maps.
pub fn city_count(cx: &Ctx) -> usize {
    BUILT_IN + cx.user_maps.len()
}

fn user<'a>(cx: &Ctx<'a>, city: usize) -> Option<&'a UserMap> {
    city.checked_sub(BUILT_IN).and_then(|i| cx.user_maps.get(i))
}

/// The city row's name: 2170..2172, a user map's name, or "Not found!"
/// (2039).
pub fn city_name(cx: &Ctx, city: usize) -> String {
    match city {
        0..BUILT_IN => cx.g(2170 + city as u32),
        _ => user(cx, city).map_or_else(|| cx.g(2039), |m| m.name.clone()),
    }
}

/// The city preview (`FUN_01027164`).
pub fn city_thumb<'a>(fe: &'a super::assets::FrontendIcons, cx: &Ctx<'a>, city: usize) -> &'a Picture {
    match user(cx, city) {
        Some(m) => &m.thumb,
        None => &fe.thumbs[city.min(BUILT_IN - 1)],
    }
}

/// Built-in cities need the previous city's last level done; user maps
/// are always offered.
pub fn available(cx: &Ctx, city: usize) -> bool {
    if city < BUILT_IN {
        unlocked(cx, city)
    } else {
        user(cx, city).is_some()
    }
}

/// The configured city (`DAT_0114c7c0`), or the nearest available one
/// below it (a missing user map gives city 0, as the setup screens do).
pub fn initial_city(cx: &Ctx) -> usize {
    let mut c = cx.settings.rampage_city as usize;
    if c >= BUILT_IN && user(cx, c).is_none() {
        c = 0;
    }
    while c > 0 && !available(cx, c) {
        c -= 1;
    }
    c
}

/// The next available city in direction `d`, wrapping.
pub fn step_city(cx: &Ctx, city: usize, d: i32) -> usize {
    let n = city_count(cx) as i32;
    let mut c = city as i32;
    for _ in 0..n {
        c = (c + d).rem_euclid(n);
        if available(cx, c as usize) {
            break;
        }
    }
    c as usize
}

/// What to load for a city number.
pub fn city_spec(user_maps: &[UserMap], city: usize, level: u8) -> LevelSpec {
    match city.checked_sub(BUILT_IN).and_then(|i| user_maps.get(i)) {
        Some(m) => m.spec(),
        None => LevelSpec {
            map: city.min(BUILT_IN - 1).to_string(),
            level,
        },
    }
}

/// The city number of a level spec.
pub fn city_of(user_maps: &[UserMap], spec: &LevelSpec) -> Option<usize> {
    if let Some(c) = spec.city() {
        return Some(c as usize).filter(|&c| c < BUILT_IN);
    }
    user_maps
        .iter()
        .position(|m| m.spec() == spec.clone().with_level(0))
        .map(|i| i + BUILT_IN)
}

/// High scores of a city (Rampage).
pub fn load_scores(cx: &Ctx, city: usize) -> HighScores {
    match user(cx, city) {
        Some(m) => m.load_scores(cx.data),
        None => crate::rampage::load_scores(cx.data, city),
    }
}

pub fn save_scores(cx: &Ctx, city: usize, scores: &HighScores) {
    match user(cx, city) {
        Some(m) => m.save_scores(scores),
        None => crate::rampage::save_scores(city, scores),
    }
}

trait WithLevel {
    fn with_level(self, level: u8) -> Self;
}

impl WithLevel for LevelSpec {
    fn with_level(mut self, level: u8) -> Self {
        self.level = level;
        self
    }
}
