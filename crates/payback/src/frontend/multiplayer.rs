//! Multiplayer setup (`FUN_0102b2c8`) and the match results
//! (`FUN_01027330`), docs/re/frontend_menus.md §7.
//!
//! Setup rows: "Player k: <device>" (0/2/4/6) each followed by the
//! player's keys (1/3/5/7: the key set, selectable for keyboard players;
//! the bindings for joysticks), City, Required kills 1..50, Play (needs
//! at least two players, else 2179), Cancel. Each device (keyboard key
//! set or joystick) can be used by one player only.

use super::rampage::unlocked;
use super::ui::*;
use super::progress::Settings;
use crate::multiplayer::{Device, JOYSTICK_HINT, KEY_SETS, MpTotals};

const ROWS: usize = 12;
const CITY: usize = 8;
const KILLS: usize = 9;
const PLAY: usize = 10;
const CANCEL: usize = 11;

/// The devices of the players taking part, in player order.
pub fn active_devices(settings: &Settings) -> Vec<Device> {
    settings.mp_devices.iter().copied().filter(|d| *d != Device::Off).collect()
}

/// "Joystick n" / "Keyboard" / "Off" (2166 / 2167 / 2168).
fn device_label(cx: &Ctx, d: Device) -> String {
    match d {
        Device::Off => cx.g(2168),
        Device::Keyboard(_) => cx.g(2167),
        Device::Joystick(n) => format!("{} {}", cx.g(2166), n + 1),
    }
}

/// The device order the left/right keys step through.
fn device_cycle() -> Vec<Device> {
    let mut v = vec![Device::Off, Device::Keyboard(0)];
    v.extend((0..4).map(Device::Joystick));
    v
}

pub struct MultiplayerSetup {
    sel: usize,
    city: usize,
    /// Thumbnail angle and flash phase, as in the Rampage setup.
    angle: i32,
    flash: u32,
    /// Frames left showing "(At least 2 players are required!)".
    refuse: u32,
}

impl MultiplayerSetup {
    /// Opening the setup starts new won/drawn/lost totals.
    pub fn new(cx: &mut Ctx) -> MultiplayerSetup {
        *cx.mp_totals = MpTotals::default();
        let mut city = (cx.settings.rampage_city as usize).min(2);
        while !unlocked(cx, city) {
            city -= 1;
        }
        MultiplayerSetup {
            sel: PLAY,
            city,
            angle: 0xe10,
            flash: 0,
            refuse: 0,
        }
    }

    /// Rows that can be selected: key-set rows only for keyboard players.
    fn selectable(&self, cx: &Ctx, row: usize) -> bool {
        row >= 8 || row % 2 == 0 || matches!(cx.settings.mp_devices[row / 2], Device::Keyboard(_))
    }

    fn used(cx: &Ctx, p: usize, d: Device) -> bool {
        cx.settings
            .mp_devices
            .iter()
            .enumerate()
            .any(|(i, &o)| i != p && o != Device::Off && o == d)
    }

    /// Next device for player `p` in direction `d`, skipping devices other
    /// players use; a keyboard takes the first free key set.
    fn step_device(cx: &mut Ctx, p: usize, d: i32) {
        let cycle = device_cycle();
        let cur = match cx.settings.mp_devices[p] {
            Device::Keyboard(_) => 1,
            dev => cycle.iter().position(|c| *c == dev).unwrap_or(0),
        };
        for k in 1..=cycle.len() as i32 {
            let cand = cycle[(cur as i32 + d * k).rem_euclid(cycle.len() as i32) as usize];
            let cand = match cand {
                Device::Keyboard(_) => {
                    match (0..KEY_SETS.len() as u8).map(Device::Keyboard).find(|&kb| !Self::used(cx, p, kb)) {
                        Some(kb) => kb,
                        None => continue,
                    }
                }
                c if Self::used(cx, p, c) => continue,
                c => c,
            };
            cx.settings.mp_devices[p] = cand;
            return;
        }
    }

    fn step_keys(cx: &mut Ctx, p: usize, d: i32) {
        let Device::Keyboard(cur) = cx.settings.mp_devices[p] else { return };
        let n = KEY_SETS.len() as i32;
        for k in 1..n {
            let cand = Device::Keyboard((cur as i32 + d * k).rem_euclid(n) as u8);
            if !Self::used(cx, p, cand) {
                cx.settings.mp_devices[p] = cand;
                return;
            }
        }
    }

    fn set_city(&mut self, cx: &mut Ctx, d: i32) {
        let mut c = self.city as i32;
        loop {
            c = (c + d).rem_euclid(3);
            if unlocked(cx, c as usize) {
                break;
            }
        }
        self.city = c as usize;
        cx.settings.rampage_city = self.city as u8;
    }

    /// Row y positions, n/200 of the screen (device rows at 30k/200).
    fn row_y(ui: &Ui, row: usize) -> i32 {
        match row {
            r if r < 8 => ui.hy(30 * (r as i32 / 2 + 1)) + if r % 2 == 1 { 12 } else { 0 },
            CITY => ui.hy(149),
            KILLS => ui.hy(161),
            PLAY => ui.hy(173),
            _ => ui.hy(185),
        }
    }
}

impl Screen for MultiplayerSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.angle = (self.angle + 0x30).rem_euclid(TURN);
        self.flash = (self.flash + 1) % 4;
        self.refuse = self.refuse.saturating_sub(1);
        if input.back {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return Step::Pop(Outcome::Cancel);
        }
        if input.up || input.down {
            cx.sound(Sfx::Move);
            let d = if input.down { 1 } else { -1 };
            loop {
                self.sel = (self.sel as i32 + d).rem_euclid(ROWS as i32) as usize;
                if self.selectable(cx, self.sel) {
                    break;
                }
            }
            return Step::Stay;
        }
        if input.left || input.right {
            let d = if input.right { 1 } else { -1 };
            match self.sel {
                r if r < 8 && r % 2 == 0 => Self::step_device(cx, r / 2, d),
                r if r < 8 => Self::step_keys(cx, r / 2, d),
                CITY => self.set_city(cx, d),
                KILLS => {
                    let k = cx.settings.required_kills as i32 + d;
                    cx.settings.required_kills = if k < 1 { 50 } else if k > 50 { 1 } else { k as u8 };
                }
                _ => return Step::Stay,
            }
            cx.sound(Sfx::Select);
            return Step::Stay;
        }
        if input.fire {
            match self.sel {
                PLAY => {
                    if active_devices(cx.settings).len() < 2 {
                        cx.sound(Sfx::Refuse);
                        self.refuse = 90;
                        return Step::Stay;
                    }
                    cx.sound(Sfx::Select);
                    cx.settings.save();
                    return Step::Pop(Outcome::Multiplayer(self.city as u8));
                }
                CANCEL => {
                    cx.sound(Sfx::Select);
                    cx.settings.save();
                    return Step::Pop(Outcome::Cancel);
                }
                _ => {}
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        let fe = ui.fe;
        ui.rotated(&fe.thumbs[self.city], ui.cx(), ui.cy(), 100, self.angle);
        ui.title(&cx.g(2135));
        let on = self.flash > 1;
        let colour = |row: usize| if row == self.sel && on { 2 } else { 1 };
        for p in 0..4 {
            let dev = cx.settings.mp_devices[p];
            let line = format!("{} {}: {}", cx.g(2160), p + 1, device_label(cx, dev));
            ui.centred(&line, Self::row_y(ui, 2 * p), colour(2 * p));
            let hint = match dev {
                Device::Off => continue,
                Device::Keyboard(k) => format!("{}. {}", k + 1, KEY_SETS[k as usize % KEY_SETS.len()].hint),
                Device::Joystick(_) => JOYSTICK_HINT.to_string(),
            };
            ui.centred(&hint, Self::row_y(ui, 2 * p + 1), if 2 * p + 1 == self.sel && on { 1 } else { 2 });
        }
        let city = format!("{} {}", cx.g(2124), cx.g(2170 + self.city as u32));
        let kills = format!("{} {}", cx.g(2329), cx.settings.required_kills);
        for (row, s) in [(CITY, city), (KILLS, kills), (PLAY, cx.g(2095)), (CANCEL, cx.g(2114))] {
            ui.centred(&s, Self::row_y(ui, row), colour(row));
        }
        // Arrows round a row that changes with left/right.
        if self.sel < PLAY && !on {
            let y = Self::row_y(ui, self.sel);
            let (l, r) = (ui.vx(-0x8c), ui.vx(0x7e));
            ui.icon(&fe.left_arrow, ui.cx() + l, y);
            ui.icon(&fe.right_arrow, ui.cx() + r, y);
        }
        if self.refuse > 0 {
            ui.centred(&cx.g(2179), H - 14, 1);
        }
    }
}

/// Results menu items: City, Play again, View replay, Save replay, End game.
const RESULT_ITEMS: [u32; 5] = [2124, 2198, 2097, 2098, 2099];

pub struct MpResults {
    /// Winner, -1 draw, -2 aborted.
    result: i32,
    players: usize,
    totals: MpTotals,
    sel: usize,
    city: usize,
    /// Waiting for the "Save replay" file name.
    naming: bool,
}

impl MpResults {
    pub fn new(cx: &Ctx, result: i32, players: usize) -> MpResults {
        MpResults {
            result,
            players: players.clamp(1, 4),
            totals: *cx.mp_totals,
            sel: 1,
            city: (cx.settings.rampage_city as usize).min(2),
            naming: false,
        }
    }
}

impl Screen for MpResults {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.up || input.down {
            cx.sound(Sfx::Move);
            let d = if input.down { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(RESULT_ITEMS.len() as i32) as usize;
            return Step::Stay;
        }
        if (input.left || input.right) && self.sel == 0 {
            cx.sound(Sfx::Select);
            let d = if input.right { 1 } else { -1 };
            let mut c = self.city as i32;
            loop {
                c = (c + d).rem_euclid(3);
                if unlocked(cx, c as usize) {
                    break;
                }
            }
            self.city = c as usize;
            cx.settings.rampage_city = self.city as u8;
            return Step::Stay;
        }
        if input.back {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return Step::Pop(Outcome::Cancel);
        }
        if !input.fire {
            return Step::Stay;
        }
        match self.sel {
            1 => {
                cx.sound(Sfx::Select);
                cx.settings.save();
                Step::Pop(Outcome::Multiplayer(self.city as u8))
            }
            2 if cx.last_replay.is_some() => {
                cx.sound(Sfx::Select);
                let r = cx.last_replay.cloned().expect("checked");
                Step::Pop(Outcome::Replay(Box::new(r)))
            }
            3 if cx.last_replay.is_some() => {
                cx.sound(Sfx::Select);
                self.naming = true;
                Step::Push(super::menus::replay_name_entry(cx))
            }
            4 => {
                cx.sound(Sfx::Select);
                cx.settings.save();
                Step::Pop(Outcome::Cancel)
            }
            _ => {
                cx.sound(Sfx::Refuse);
                Step::Stay
            }
        }
    }

    fn resume(&mut self, cx: &mut Ctx, outcome: Outcome) -> Step {
        match (std::mem::take(&mut self.naming), outcome) {
            (true, Outcome::Text(name)) => super::menus::save_last_replay(cx, &name),
            _ => Step::Stay,
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2129));
        let header = match self.result {
            -1 => cx.g(2128),
            -2 => cx.g(2174),
            p if p >= 0 => cx.g(2173).replace("%d", &(p + 1).to_string()),
            _ => String::new(),
        };
        ui.centred(&header, ui.hy(30), 2);
        // Player | Won | Drawn | Lost.
        let cols = [ui.cx() - ui.vx(110), ui.cx() - ui.vx(10), ui.cx() + ui.vx(40), ui.cx() + ui.vx(90)];
        let y0 = ui.hy(50);
        ui.text(&cx.g(2160), cols[0], y0, 1);
        for (i, n) in [2161, 2163, 2162].into_iter().enumerate() {
            ui.text(&cx.g(n), cols[i + 1], y0, 1);
        }
        let t = &self.totals;
        for p in 0..self.players {
            let y = y0 + 14 * (p as i32 + 1);
            ui.text(&format!("{} {}", cx.g(2160), p + 1), cols[0], y, 2);
            for (i, v) in [t.won[p], t.drawn[p], t.lost[p]].into_iter().enumerate() {
                ui.text(&v.to_string(), cols[i + 1], y, 2);
            }
        }
        for (i, &n) in RESULT_ITEMS.iter().enumerate() {
            let s = if i == 0 {
                format!("{} {}", cx.g(n), cx.g(2170 + self.city as u32))
            } else {
                cx.g(n)
            };
            let c = if i == self.sel { ui.flash() } else { 1 };
            ui.centred(&s, ui.hy(125 + 14 * i as i32), c);
        }
        if self.sel == 0 {
            ui.arrows(ui.hy(125));
        }
    }
}
