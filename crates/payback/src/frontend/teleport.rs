//! "Teleport to..." (port addition, not in the original; the cheat menu,
//! `super::cheats`, docs/re/frontend_menus.md §12): pick a square on the
//! level's map and the player is moved there (`crate::cheats`,
//! `Cheat::TeleportTo`).
//!
//! The game camera glides over the cursor square and looks straight down
//! on it (`camera::CameraPreview`), with a flashing cross-hair in the
//! middle of the view where the player would land. In the bottom-left
//! corner, the level's minimap picture (`<map>.mini`, 64×64, one pixel per
//! 2×2 squares; the HUD's, `hud::load_minimap`) at twice its size shows the
//! cursor (a small cross-hair) with the HUD's blobs for the player (green)
//! and the objective (red).
//! Arrows move the cursor a square (held, it keeps moving), Enter
//! teleports, Esc goes back to the cheat menu.

use super::ui::*;
use crate::hud::icons::Icon;
use crate::hud::paint::Blend;
use bevy::math::IVec2;

/// Corner-map pixels per minimap pixel.
const SCALE: i32 = 2;

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
        // Test switch `--teleport-cursor <x>:<y>`: start the cursor there.
        let forced = crate::arg("--teleport-cursor").and_then(|v| {
            let (x, y) = v.split_once(':')?;
            Some(IVec2::new(x.parse().ok()?, y.parse().ok()?))
        });
        let cursor = forced.unwrap_or_else(|| player.map_or(IVec2::splat(SQUARES / 2), |p| p >> 11));
        TeleportPicker {
            mini,
            cursor: cursor.clamp(IVec2::ZERO, IVec2::splat(SQUARES - 1)),
            player,
            target,
            held: 0,
        }
    }

    /// Top-left of the corner map on the canvas (bottom left, above the
    /// hint lines).
    fn origin(ui: &Ui) -> IVec2 {
        let _ = ui;
        IVec2::new(10, H - 64 * SCALE - 34)
    }
}

/// A cross-hair at `c`: arms `len` long, two pixels thick, black outline.
fn cross(ui: &mut Ui, c: IVec2, len: i32, gap: i32, colour: u8) {
    for (ring, col) in [(1i32, 8u8), (0, colour)] {
        for d in -len..=len {
            if d.abs() < gap {
                continue;
            }
            for t in -1 - ring..=ring {
                ui.front.put(c.x + d, c.y + t, col, Blend::Plain);
                ui.front.put(c.x + t, c.y + d, col, Blend::Plain);
            }
        }
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

    /// The camera looks down on the cursor square (`camera::CameraPreview`).
    fn camera_focus(&self) -> Option<IVec2> {
        Some(self.cursor)
    }

    fn draw(&self, ui: &mut Ui, _cx: &Ctx) {
        let o = Self::origin(ui);
        let size = 64 * SCALE;
        ui.heading("TELEPORT", 6);
        // Where you would land: the middle of the view, which the camera
        // keeps over the cursor square.
        let blink = ui.flash() == 2;
        let colour = if blink { 0xB0 } else { 8 };
        cross(ui, IVec2::new(ui.cx(), H / 2), 14, 4, colour);
        // The corner map with the cursor, player and objective.
        ui.back.darken(o.x - 4, o.y - 4, o.x + size + 4, o.y + size + 4);
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
        cross(ui, o + (self.cursor * SCALE + IVec2::ONE * SCALE / 2) / 2, 7, 2, colour);
        let label = format!("Square {}, {}", self.cursor.x, self.cursor.y);
        ui.centred(&label, H - 26, 2);
        ui.centred("Arrows: move  Enter: go  Esc: back", H - 14, 2);
    }
}
