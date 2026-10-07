//! Rampage setup (`FUN_0102a428`): city with its thumbnail spinning
//! behind the text, police aggressiveness, the top-5 table for that
//! city and aggressiveness, Play / Cancel.

use super::ui::*;
use crate::rampage::load_scores;
use payback_formats::scores::HighScores;

/// Rows: city, police aggressiveness, Play, Cancel.
const ROWS: usize = 4;

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
/// (`DAT_0117aecc[(c-1)*2]`).
pub fn unlocked(cx: &Ctx, city: usize) -> bool {
    city == 0 || cx.profile.cups[2 * city - 1] != 0
}

impl RampageSetup {
    pub fn new(cx: &Ctx) -> RampageSetup {
        let mut city = (cx.settings.rampage_city as usize).min(2);
        while !unlocked(cx, city) {
            city -= 1;
        }
        RampageSetup {
            sel: 2,
            city,
            police: cx.settings.police.min(5),
            scores: load_scores(cx.data, city),
            angle: 0xe10,
            flash: 0,
        }
    }

    fn set_city(&mut self, cx: &Ctx, d: i32) {
        let mut c = self.city as i32;
        loop {
            c = (c + d).rem_euclid(3);
            if unlocked(cx, c as usize) {
                break;
            }
        }
        self.city = c as usize;
        self.scores = load_scores(cx.data, self.city);
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
                _ => {}
            }
            cx.settings.rampage_city = self.city as u8;
            cx.settings.police = self.police;
            return Step::Stay;
        }
        if input.fire {
            cx.sound(Sfx::Select);
            match self.sel {
                2 => {
                    cx.settings.save();
                    return Step::Pop(Outcome::Rampage(self.city as u8, self.police));
                }
                3 => return Step::Pop(Outcome::Cancel),
                _ => {}
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        let fe = ui.fe;
        ui.rotated(&fe.thumbs[self.city], ui.cx(), ui.cy(), 100, self.angle);
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
        let city = format!("{} {}", cx.g(2124), cx.g(2170 + self.city as u32));
        let police = format!("{} {}", cx.g(2055), cx.g(2491 + self.police as u32));
        let rows = [(city, 0x82), (police, 0x91), (cx.g(2095), 0xa5), (cx.g(2114), 0xb4)];
        for (i, (s, y)) in rows.iter().enumerate() {
            let c = if i == self.sel && on { 2 } else { 1 };
            ui.centred(s, ui.hy(*y), c);
        }
        // Arrows round the selected city / police row, off while flashing.
        if self.sel < 2 && !on {
            let y = ui.hy(rows[self.sel].1);
            let (l, r) = (ui.vx(-0x8c), ui.vx(0x7e));
            ui.icon(&fe.left_arrow, ui.cx() + l, y);
            ui.icon(&fe.right_arrow, ui.cx() + r, y);
        }
    }
}
