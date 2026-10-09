//! Rampage setup (`FUN_0102a428`, Mac `DoRampageSetup` @ 1009ebc0): city
//! with its thumbnail spinning behind the text, police aggressiveness
//! ("Difficulty:" in the Mac phrases, 2055), the top-5 table for that city
//! and aggressiveness, Play / Cancel.
//!
//! Added row: "Random missions" (`Variants::random_missions`, decision 8 in
//! docs/re/mac_version.md): the Mac's random phone missions in Rampage
//! (`ProcessTrigger_Random`), on by default. Play and Cancel move down
//! one row to make room. The choice changes the simulation, so replays
//! record it (`crate::replay::RandomMissions`).

use super::ui::*;
use super::usermaps::{city_name, city_thumb, initial_city, load_scores, step_city};
use payback_formats::scores::HighScores;

/// Rows: city, police aggressiveness, random missions, Play, Cancel.
const ROWS: usize = 5;
const RANDOM: usize = 2;
const PLAY: usize = 3;
const CANCEL: usize = 4;

pub struct RampageSetup {
    sel: usize,
    city: usize,
    police: u8,
    scores: HighScores,
    /// Thumbnail angle (`+0x30` per frame from 0xe10).
    angle: i32,
    /// Flash phase 0..3 (`local_a0`).
    flash: u32,
}

/// A city may be chosen once the previous city's last level has a cup
/// (`DAT_0117aecc[(c-1)*2]`), or always with "Unlock all".
pub fn unlocked(cx: &Ctx, city: usize) -> bool {
    super::progress::city_open(cx.profile, cx.settings, city)
}

impl RampageSetup {
    pub fn new(cx: &Ctx) -> RampageSetup {
        let city = initial_city(cx);
        RampageSetup {
            sel: PLAY,
            city,
            police: cx.settings.police.min(5),
            scores: load_scores(cx, city),
            angle: 0xe10,
            flash: 0,
        }
    }

    /// The next city: built-in ones once unlocked, then the user maps.
    fn set_city(&mut self, cx: &Ctx, d: i32) {
        self.city = step_city(cx, self.city, d);
        self.scores = load_scores(cx, self.city);
    }
}

impl Screen for RampageSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.angle = (self.angle + 0x30).rem_euclid(TURN);
        self.flash = (self.flash + 1) % 4;
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.up || input.down {
            cx.sound(Sfx::Move);
            let d = if input.down { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(ROWS as i32) as usize;
            return Step::Stay;
        }
        if input.left || input.right {
            cx.sound(Sfx::Select);
            let d = if input.right { 1 } else { -1 };
            match self.sel {
                0 => self.set_city(cx, d),
                1 => self.police = (self.police as i32 + d).rem_euclid(6) as u8,
                RANDOM => cx.settings.variants.random_missions ^= true,
                _ => {}
            }
            cx.settings.rampage_city = self.city as u8;
            cx.settings.police = self.police;
            return Step::Stay;
        }
        if input.fire {
            cx.sound(Sfx::Select);
            match self.sel {
                RANDOM => cx.settings.variants.random_missions ^= true,
                PLAY => {
                    cx.settings.save();
                    return Step::Pop(Outcome::Rampage(self.city as u8, self.police));
                }
                CANCEL => return Step::Pop(Outcome::Cancel),
                _ => {}
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        let fe = ui.fe;
        ui.rotated(city_thumb(fe, cx, self.city), ui.cx(), ui.cy(), 100, self.angle);
        ui.title(&cx.g(2196));
        ui.centred(&cx.g(1610), H / 5, 2);
        // "%d: %s" padded with dots to 35 characters, then the score.
        // The rank shown on a line goes up only when the line above it
        // differs from the one above that (as the original counts).
        let table = &self.scores.tables[self.police as usize];
        let mut rank = 1;
        let mut prev = -1;
        for (i, e) in table.iter().enumerate() {
            let mut line = format!("{rank}: {}", e.name);
            while line.chars().count() < 0x23 {
                line.push('.');
            }
            line += &e.score.to_string();
            ui.centred(&line, ui.hy(0x37 + 12 * i as i32), 2);
            if e.score != prev {
                rank = i + 2;
            }
            prev = e.score;
        }
        let on = self.flash > 1;
        let city = format!("{} {}", cx.g(2124), city_name(cx, self.city));
        let police = format!("{} {}", cx.g(2055), cx.g(2491 + self.police as u32));
        // No phrase for the added row.
        let on_off = cx.g(if cx.settings.variants.random_missions { 2081 } else { 2080 });
        let random = format!("Random missions: {on_off}");
        let rows = [(city, 0x82), (police, 0x91), (random, 0xa0), (cx.g(2095), 0xb0), (cx.g(2114), 0xbf)];
        for (i, (s, y)) in rows.iter().enumerate() {
            let c = if i == self.sel && on { 2 } else { 1 };
            ui.centred(s, ui.hy(*y), c);
        }
        // Arrows round the selected city / police / missions row, off while
        // flashing.
        if self.sel < PLAY && !on {
            let y = ui.hy(rows[self.sel].1);
            let (l, r) = (ui.vx(-0x8c), ui.vx(0x7e));
            ui.icon(&fe.left_arrow, ui.cx() + l, y);
            ui.icon(&fe.right_arrow, ui.cx() + r, y);
        }
    }
}
