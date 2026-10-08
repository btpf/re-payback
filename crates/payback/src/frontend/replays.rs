//! Replay pick list (`FUN_0101fdcc`, "SELECT REPLAY") and the replay info
//! screen (`FUN_0101e618`, "VIEW REPLAY").

use super::misc::Message;
use super::ui::*;
use crate::mission::LevelClock;
use payback_formats::replay::Replay;
use std::path::PathBuf;

/// Load a replay the way `FUN_010bc000` reports it: Err(phrase) = 2115
/// "FILE NOT FOUND", 2116 "WRONG VERSION NUMBER", 2038 "MAP NOT FOUND".
/// Versions 10 / 0x3F2 (Amiga, the OS4 check) and 0x3F3 (Mac `LoadMovie`
/// @ 100fa7b0, which accepts only 0x3F3, returning 2 for an unknown user
/// map) are accepted; the shipped Amiga ones are version 3, refused as on
/// OS4.
pub fn load(cx: &Ctx, path: &std::path::Path) -> Result<Replay, u32> {
    let b = std::fs::read(path).map_err(|_| 2115u32)?;
    let version = b.get(..4).map(|v| i32::from_be_bytes([v[0], v[1], v[2], v[3]]));
    if !matches!(version, Some(10 | 0x3f2 | 0x3f3)) {
        return Err(2116);
    }
    let r = Replay::parse(&b).map_err(|_| 2116u32)?;
    let found = launch(cx.user_maps, &r).is_some_and(|(spec, _)| cx.data.read(spec.path("map")).is_ok());
    if !found {
        return Err(2038);
    }
    Ok(r)
}

/// The level of a replay, with `PROGDIR:UserMaps/<name>` found in the
/// user-map list (`GetCityNumFromFilename` @ 10084d20).
pub fn launch(
    user_maps: &[super::usermaps::UserMap],
    r: &Replay,
) -> Option<(crate::level::LevelSpec, crate::level::GameMode)> {
    crate::replay::launch_with(r, |name| {
        user_maps.iter().find(|m| m.name.eq_ignore_ascii_case(name)).map(|m| m.spec())
    })
}

/// The files in `Saves/Replays/` (ours) and the game's `Replays/`
/// (`FUN_01009b48`), sorted by name.
fn scan(cx: &Ctx) -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for dir in [crate::replay::saves_dir(), cx.data.root.join("Replays")] {
        let Ok(d) = std::fs::read_dir(&dir) else { continue };
        for e in d.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_file() && !files.iter().any(|(n, _)| *n == name) {
                files.push((name, e.path()));
            }
        }
    }
    files.sort_by_key(|(n, _)| n.to_lowercase());
    files.truncate(2000);
    files
}

pub struct ReplayList {
    files: Vec<(String, PathBuf)>,
    sel: usize,
    top: usize,
}

impl ReplayList {
    pub fn new(cx: &Ctx) -> ReplayList {
        ReplayList {
            files: scan(cx),
            sel: 0,
            top: 0,
        }
    }
}

impl Screen for ReplayList {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        let n = self.files.len() + 1;
        if input.up || input.down {
            let d = if input.down { 1 } else { -1 };
            self.sel = (self.sel as isize + d).rem_euclid(n as isize) as usize;
            cx.sound(Sfx::Move);
        }
        self.top = self.top.min(self.sel).max(self.sel.saturating_sub(13));
        if input.back || (input.fire && self.sel == n - 1) {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if !input.fire {
            return Step::Stay;
        }
        cx.sound(Sfx::Select);
        let (name, path) = &self.files[self.sel];
        match load(cx, path) {
            Ok(r) => Step::Push(Box::new(ReplayInfo::new(r, Some(name.clone())))),
            Err(phrase) => Step::Push(Box::new(Message::new(cx.g(phrase)))),
        }
    }

    fn resume(&mut self, _cx: &mut Ctx, outcome: Outcome) -> Step {
        match outcome {
            Outcome::Replay(r) => Step::Pop(Outcome::Replay(r)),
            _ => Step::Stay,
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2113));
        let n = self.files.len();
        for (row, i) in (self.top..n).take(14).enumerate() {
            let c = if i == self.sel { ui.flash() } else { 2 };
            ui.centred(&self.files[i].0, 0x20 + 14 * row as i32, c);
        }
        let c = if self.sel == n { ui.flash() } else { 2 };
        ui.centred(&cx.g(2114), 0xeb, c);
    }
}

/// "VIEW REPLAY": name, duration, average frame rate, city, level (or
/// police aggressiveness / number of players), then View Replay / Cancel.
pub struct ReplayInfo {
    replay: Option<Replay>,
    name: Option<String>,
    sel: usize,
    flash: u32,
}

impl ReplayInfo {
    pub fn new(replay: Replay, name: Option<String>) -> ReplayInfo {
        ReplayInfo {
            replay: Some(replay),
            name,
            sel: 0,
            flash: 0,
        }
    }

    fn lines(&self, cx: &Ctx) -> Vec<(String, i32, u8)> {
        let Some(r) = &self.replay else { return Vec::new() };
        let h = &r.header;
        let hz = LevelClock::TICKS_PER_SECOND as i32;
        let ticks = h.duration.max(1);
        let secs = ticks / hz;
        let mut out = Vec::new();
        if let Some(n) = &self.name {
            out.push((format!("{} {n}", cx.g(2126)), 0x37, 2));
        }
        out.push((
            format!("{} {}:{:02}:{:02}", cx.g(2122), secs / 3600, secs / 60 % 60, secs % 60),
            0x49,
            1,
        ));
        // Frame rate = words·rate / (players·ticks).
        let fps = r.records.len() as i64 * hz as i64 / (h.players.max(1) as i64 * ticks as i64);
        out.push((format!("{} {fps} {}", cx.g(2123), cx.g(2149)), 0x57, 1));
        let city = r.city().map(|c| c as i32).unwrap_or(-1);
        // `GetCityName` @ 10084f10: 2170.., a user map's name, else 2039.
        let city_name = match r.user_map() {
            _ if (0..3).contains(&city) => cx.g(2170 + city as u32),
            Some(name) => name.to_string(),
            None => cx.g(2039),
        };
        out.push((format!("{} {city_name}", cx.g(2124)), 0x69, 2));
        let last = if h.players >= 2 {
            format!("{} {}", cx.g(2199), h.players)
        } else if h.rampage {
            format!("{} {}", cx.g(2055), cx.g(2491 + h.police.clamp(0, 5) as u32))
        } else {
            let l = (city.max(0) * 2 + (h.level & 1)) as u32;
            format!("{} {}", cx.g(2125), cx.g(2180 + l))
        };
        out.push((last, 0x77, 2));
        out
    }
}

impl Screen for ReplayInfo {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.flash = (self.flash + 1) % 4;
        if input.left || input.right {
            cx.sound(Sfx::Select);
            self.sel ^= 1;
        }
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.fire {
            cx.sound(Sfx::Select);
            return match (self.sel, self.replay.take()) {
                (0, Some(r)) => Step::Pop(Outcome::Replay(Box::new(r))),
                _ => Step::Pop(Outcome::Cancel),
            };
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.heading(&cx.g(2127), 0x14);
        for (s, y, c) in self.lines(cx) {
            ui.centred(&s, ui.hy(y), c);
        }
        for i in 0..2 {
            let s = cx.g(2120 + i as u32);
            let x = ui.w / 4 + i * ui.cx() - 7 * crate::hud::paint::text_len(&s) / 2;
            let c = if i as usize == self.sel && self.flash < 2 { 2 } else { 1 };
            ui.text(&s, x, ui.hy(0xaa), c);
        }
    }
}
