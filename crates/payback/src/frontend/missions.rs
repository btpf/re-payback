//! The mission selector (port addition, not in the original; a play-testing
//! aid, docs/re/frontend_menus.md "Mission selector").
//!
//! "Mission select" in the cheat menu (F12) of a story level lists every
//! mission of the level (`mission::levels::missions`), whether or not its
//! phone is ringing yet. Choosing one starts it as if its phone had been
//! answered, after ending any mission in progress without a result
//! (`crate::cheats`, [`Cheat::Mission`]): with "Take me there" (left/right,
//! on by default) the player is first moved to the mission's phone (or the
//! car it starts in), else it starts where the player is. The replay
//! records it like the other cheats, so the game replays exactly.
//!
//! Debug: `--mission-menu [--menu-item <n>] [--mission-go]` opens it after
//! 3 s on line n and, with `--mission-go`, chooses that line.
//!
//! Drawn like the cheat menu (`super::cheats`): the frozen game behind the
//! pause menu's darkened panel, small-font lines with the selected one
//! flashing, the menu sounds, and the player's RC car turning behind the
//! panel the way the vehicle selection's models do.
//!
//! [`Cheat::Mission`]: crate::cheats::Cheat::Mission

use super::cheats::{SPIN, Size, TITLE_Y, model_size, panel, turning_model};
use super::ui::*;
use crate::data::GameData;
use crate::hud::Texts;
use crate::mission::levels::MissionInfo;
use crate::vehicles::{TYPES, kind};
use std::sync::atomic::{AtomicBool, Ordering};

/// Mission lines shown at once (the list scrolls).
const ROWS: usize = 7;

/// First line and spacing (virtual 200-line units).
const TOP: i32 = 0x3a;
const LINE: i32 = 11;

/// Characters that fit across the panel in the small font.
const WIDTH: usize = 30;

/// "Take me there": the player is moved to the mission's start (its phone,
/// or the car it starts in) before it starts. On by default; kept for the
/// session.
static TRAVEL: AtomicBool = AtomicBool::new(true);

pub struct MissionMenu {
    list: &'static [MissionInfo],
    /// The opening line of each mission's phone briefing, cut to the panel.
    briefs: Vec<String>,
    sel: usize,
    /// First line shown.
    top: usize,
    spin: i32,
    car: Size,
    /// `--mission-go`: start the selected mission on the first tick.
    go: bool,
}

impl MissionMenu {
    pub fn new(
        data: &GameData,
        texts: &Texts,
        list: &'static [MissionInfo],
        sel: usize,
    ) -> MissionMenu {
        let briefs = list
            .iter()
            .map(|m| first_words(&texts.level(m.brief)))
            .collect();
        let sel = sel.min(list.len().saturating_sub(1));
        MissionMenu {
            list,
            briefs,
            sel,
            top: sel.saturating_sub(ROWS - 1),
            spin: 0x2d0,
            car: model_size(data, TYPES[kind::RC_CAR].model).0,
            go: crate::args().any(|a| a == "--mission-go"),
        }
    }
}

/// The start of `text` on one line of the panel: printable characters, at
/// most [`WIDTH`], cut at a word with "..." when longer.
fn first_words(text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.chars().count() <= WIDTH {
        return clean;
    }
    let mut out = String::new();
    for word in clean.split(' ') {
        if out.chars().count() + word.chars().count() + 1 > WIDTH - 3 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out + "..."
}

impl Screen for MissionMenu {
    /// Keep the music going, as the pause menu does.
    fn cd_track(&self) -> i32 {
        0
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.spin = (self.spin + SPIN).rem_euclid(TURN);
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.left || input.right {
            TRAVEL.fetch_xor(true, Ordering::Relaxed);
            cx.sound(Sfx::Select);
            return Step::Stay;
        }
        if input.up || input.down {
            let n = self.list.len() as i32;
            let d = if input.down { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(n) as usize;
            if self.sel < self.top {
                self.top = self.sel;
            } else if self.sel >= self.top + ROWS {
                self.top = self.sel + 1 - ROWS;
            }
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        // Debug (`--mission-menu --mission-go`): choose the line at once.
        let go = std::mem::take(&mut self.go);
        if input.fire || go {
            cx.sound(Sfx::Select);
            cx.requests
                .push(Request::Cheat(crate::cheats::Cheat::Mission {
                    event: self.list[self.sel].event,
                    travel: TRAVEL.load(Ordering::Relaxed),
                }));
            cx.requests.push(Request::Resume);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        panel(ui);
        ui.heading("MISSIONS", TITLE_Y);
        for (row, i) in (self.top..self.list.len()).take(ROWS).enumerate() {
            let m = &self.list[i];
            let c = if i == self.sel { ui.flash() } else { 1 };
            ui.centred(
                &format!("{}. {}", i + 1, m.name),
                ui.hy(TOP + LINE * row as i32),
                c,
            );
        }
        // More lines above or below the window.
        if self.top > 0 {
            ui.centred("...", ui.hy(TOP - 7), 2);
        }
        if self.top + ROWS < self.list.len() {
            ui.centred("...", ui.hy(TOP + LINE * ROWS as i32 - 4), 2);
        }
        ui.centred(&self.briefs[self.sel], ui.hy(0x88), 2);
        let on = cx.g(if TRAVEL.load(Ordering::Relaxed) { 2081 } else { 2080 });
        ui.centred(&format!("Take me there: {on}"), ui.hy(0x92), 1);
        ui.centred("Enter: play  Left/Right: toggle", ui.hy(0x9c), 2);
    }

    /// The RC car turning behind the panel, nearer than the vehicle
    /// selection's models.
    fn model(&self) -> Option<ModelView> {
        Some(turning_model(TYPES[kind::RC_CAR].model, self.car, self.spin, 0, None, 0.6, -0.04))
    }
}
