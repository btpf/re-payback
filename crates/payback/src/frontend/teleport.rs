//! "Teleport to..." (port addition, not in the original; the cheat menu,
//! `super::cheats`, docs/re/frontend_menus.md §12): pick a square on the
//! level's map and the player is moved there (`crate::cheats`,
//! `Cheat::TeleportTo`).
//!
//! The map is the level's minimap picture (`<map>.mini`, 64×64, one pixel
//! per 2×2 squares; the HUD's, `hud::load_minimap`) drawn three times its
//! size over the pause menu's darkened panel, with the HUD's blobs for the
//! player (green) and the objective (red) and a flashing cross-hair cursor.
//! Arrows move the cursor a square (held, it keeps moving), Enter
//! teleports, Esc goes back to the cheat menu.

use super::ui::*;
use crate::hud::icons::Icon;
use crate::hud::paint::Blend;
use bevy::math::IVec2;

/// Map pixels per minimap pixel.
const SCALE: i32 = 3;

/// Squares on a side.
const SQUARES: i32 = 128;

/// Ticks a direction is held before the cursor repeats.
const REPEAT_AFTER: u32 = 6;

pub struct TeleportPicker {
    mini: Option<Icon>,
    /// Cursor, in squares.
    cursor: IVec2,
    /// The player and the objective, in object units (`HudData`).
    player: Option<IVec2>,
    target: Option<IVec2>,
    held: u32,
}

impl TeleportPicker {
    pub fn new(mini: Option<Icon>, player: Option<IVec2>, target: Option<IVec2>) -> TeleportPicker {
        let cursor = player.map_or(IVec2::splat(SQUARES / 2), |p| p >> 11);
        TeleportPicker {
            mini,
            cursor: cursor.clamp(IVec2::ZERO, IVec2::splat(SQUARES - 1)),
            player,
            target,
            held: 0,
        }
    }

    /// Top-left of the map on the canvas.
    fn origin(ui: &Ui) -> IVec2 {
        IVec2::new(ui.cx() - 32 * SCALE, 30)
    }
}

impl Screen for TeleportPicker {
    fn cd_track(&self) -> i32 {
        0
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.fire {
            cx.sound(Sfx::Select);
            let c = self.cursor;
            cx.requests.push(Request::Cheat(crate::cheats::Cheat::TeleportTo(c.x as u8, c.y as u8)));
            cx.requests.push(Request::Resume);
            return Step::Stay;
        }
        let pressed = IVec2::new(
            input.right as i32 - input.left as i32,
            input.down as i32 - input.up as i32,
        );
        let step = if pressed != IVec2::ZERO {
            self.held = 0;
            pressed
        } else if input.held != IVec2::ZERO {
            self.held += 1;
            if self.held > REPEAT_AFTER { input.held } else { IVec2::ZERO }
        } else {
            self.held = 0;
            IVec2::ZERO
        };
        if step != IVec2::ZERO {
            self.cursor = (self.cursor + step).clamp(IVec2::ZERO, IVec2::splat(SQUARES - 1));
            if pressed != IVec2::ZERO {
                cx.sound(Sfx::Move);
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, _cx: &Ctx) {
        let o = Self::origin(ui);
        let size = 64 * SCALE;
        ui.back.darken(o.x - 8, 8, o.x + size + 8, 252);
        ui.heading("TELEPORT", 6);
        if let Some(m) = &self.mini {
            for y in 0..m.h as i32 {
                for x in 0..m.w as i32 {
                    let c = m.px[(y * m.w as i32 + x) as usize];
                    if c == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            ui.front.put(o.x + x * SCALE + dx, o.y + y * SCALE + dy, c, Blend::Plain);
                        }
                    }
                }
            }
        }
        // Object units to map pixels: >> 12 is a minimap pixel.
        let at = |p: IVec2| o + IVec2::new(p.x * SCALE >> 12, p.y * SCALE >> 12);
        let blink = ui.flash() == 2;
        let blobs = &ui.fonts.blobs;
        let mut icons = Vec::new();
        if let Some(t) = self.target.filter(|t| t.x >= 0) {
            icons.push((blobs.get(if blink { 3 } else { 2 }).cloned(), at(t)));
        }
        if let Some(p) = self.player {
            icons.push((blobs.get(if blink { 1 } else { 0 }).cloned(), at(p)));
        }
        for (icon, p) in icons {
            if let Some(icon) = icon {
                ui.icon(&icon, p.x - icon.w as i32 / 2, p.y - icon.h as i32 / 2);
            }
        }
        // Cross-hair at the cursor square's centre: a red cross two pixels
        // thick with a black outline, flashing.
        let c = o + (self.cursor * SCALE + IVec2::ONE * SCALE / 2) / 2;
        let colour = if blink { 0xB0 } else { 8 };
        for (ring, col) in [(1i32, 8u8), (0, colour)] {
            for d in -10i32..=10 {
                if d.abs() < 3 {
                    continue;
                }
                for t in -1 - ring..=ring {
                    ui.front.put(c.x + d, c.y + t, col, Blend::Plain);
                    ui.front.put(c.x + t, c.y + d, col, Blend::Plain);
                }
            }
        }
        let label = format!("Square {}, {}", self.cursor.x, self.cursor.y);
        ui.centred(&label, o.y + size + 4, 2);
        ui.centred("Arrows: move  Enter: go  Esc: back", o.y + size + 16, 2);
    }
}
