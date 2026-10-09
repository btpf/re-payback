//! The cheat menu and its vehicle selection (port addition, not in the
//! original; `crate::cheats`, docs/re/frontend_menus.md "Cheat menu").
//!
//! Opened with F12 in a single-player level (not in multiplayer or while a
//! replay plays). Drawn like the pause menu (`FUN_0101a7dc`): the frozen
//! game behind a darkened panel (no `options.obj` behind it: it reads
//! "OPTIONS"), small-font lines with the selected one flashing, the menu sounds, and the game paused while it is open. The
//! vehicle selection spins the chosen vehicle's model behind the panel the
//! way the option objects spin behind the menus, sliding it out and the
//! next one in on left/right (`OptionObj`'s 3 + 3 frame slide); up/down
//! change its paint colour (`objects::PAINTS`) when the model has one.
//!
//! The menus only queue the cheats ([`Request::Cheat`]); the game applies
//! them in its next fixed tick and records them in the replay.

use super::ui::*;
use crate::cheats::{Cheat, Cheats, MAX_WANTED, VEHICLES};
use crate::objects::{PAINT_COUNT, paintable};
use crate::data::GameData;
use crate::vehicles::TYPES;
use bevy::math::{EulerRot, Quat, Vec3};

/// The menu panel, as the pause menu's (virtual 50..269 × 26..229).
fn panel(ui: &mut Ui) {
    let (x0, x1) = (ui.vx(50), ui.vx(269));
    ui.back.darken(x0, 26, x1, 229);
}

const TITLE_Y: i32 = 34;

/// Menu lines.
const ITEMS: usize = 7;

/// Line spacing of the items (virtual 200-line units).
const LINE: i32 = 12;

/// First item line (virtual 200-line units): seven items and the wanted
/// level above the "Esc" line at 0x9a.
const TOP: i32 = 0x3a;

/// Main cheat menu: God mode (On/Off), Vehicle god mode (On/Off), All
/// weapons, Spawn vehicle, Clear wanted, Wanted up, Wanted down, and the
/// wanted level below them.
pub struct CheatMenu {
    sel: usize,
    /// God mode as it will be after the queued cheats.
    god: bool,
    /// Vehicle god mode as it will be after the queued cheats.
    vehicle_god: bool,
    /// The wanted level as it will be after the queued cheats.
    wanted: i32,
}

impl CheatMenu {
    /// Showing the cheats as they will be once the queue is applied.
    pub fn new(cheats: &Cheats) -> CheatMenu {
        CheatMenu {
            sel: 0,
            god: cheats.god_pending(),
            vehicle_god: cheats.vehicle_god_pending(),
            wanted: cheats.wanted_pending(),
        }
    }

    /// Debug (`--cheat-menu`, on line `--menu-item <n>`).
    pub fn at(cheats: &Cheats, sel: usize) -> CheatMenu {
        CheatMenu { sel: sel.min(ITEMS - 1), ..CheatMenu::new(cheats) }
    }

    fn label(&self, cx: &Ctx, i: usize) -> String {
        match i {
            0 => format!("God mode: {}", cx.g(if self.god { 2081 } else { 2080 })),
            1 => format!("Vehicle god mode: {}", cx.g(if self.vehicle_god { 2081 } else { 2080 })),
            2 => "All weapons".into(),
            3 => "Spawn vehicle".into(),
            4 => "Clear wanted".into(),
            5 => "Wanted up".into(),
            _ => "Wanted down".into(),
        }
    }

    /// Queue a wanted-level change (the menu stays open, showing the
    /// level it will have).
    fn wanted(&mut self, cx: &mut Ctx, c: Cheat) {
        self.wanted = match c {
            Cheat::Wanted(d) => (self.wanted + d).clamp(0, MAX_WANTED),
            _ => 0,
        };
        cx.requests.push(Request::Cheat(c));
    }
}

impl Screen for CheatMenu {
    /// Keep the music going, as the pause menu does.
    fn cd_track(&self) -> i32 {
        0
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.back {
            cx.sound(Sfx::Select);
            cx.requests.push(Request::Resume);
            return Step::Stay;
        }
        if input.up || input.down {
            let d = if input.down { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(ITEMS as i32) as usize;
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if (self.sel <= 1 && (input.left || input.right)) || input.fire {
            cx.sound(Sfx::Select);
            match self.sel {
                0 => {
                    self.god = !self.god;
                    cx.requests.push(Request::Cheat(Cheat::God(self.god)));
                }
                1 => {
                    self.vehicle_god = !self.vehicle_god;
                    cx.requests.push(Request::Cheat(Cheat::VehicleGod(self.vehicle_god)));
                }
                2 => {
                    cx.requests.push(Request::Cheat(Cheat::AllWeapons));
                    cx.requests.push(Request::Resume);
                }
                3 => return Step::Push(Box::new(VehicleSelect::new(cx.data, 0))),
                4 => self.wanted(cx, Cheat::ClearWanted),
                5 => self.wanted(cx, Cheat::Wanted(1)),
                _ => self.wanted(cx, Cheat::Wanted(-1)),
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        panel(ui);
        ui.heading("CHEATS", TITLE_Y);
        for i in 0..ITEMS {
            let c = if i == self.sel { ui.flash() } else { 1 };
            ui.centred(&self.label(cx, i), ui.hy(TOP + LINE * i as i32), c);
        }
        let w = format!("Wanted level: {}/{}", self.wanted, MAX_WANTED);
        ui.centred(&w, ui.hy(TOP + LINE * ITEMS as i32), 2);
        ui.centred("Esc: back to the game", ui.hy(0x9a), 2);
    }
}

/// Camera-space units per model unit, as the other menu models.
const MODEL_UNIT: f32 = super::model::MODEL_UNIT;

/// Distance of the model, in model bounding radii.
const DISTANCE: f32 = 4.4;

/// Elevation of the view onto the turning model.
const ELEVATION: f32 = 0.42;

/// Spin per menu tick (a turn in 120 ticks, 4 s).
const SPIN: i32 = 0x30;

/// A vehicle model's bounding box centre and half diagonal, in model units.
#[derive(Clone, Copy, Debug)]
struct Size {
    centre: Vec3,
    radius: f32,
}

/// Choose a vehicle: its name over the rotating model, left/right to
/// change (wrapping), up/down to change its colour, Enter to spawn it, Esc
/// back to the cheat menu.
pub struct VehicleSelect {
    /// Index into [`VEHICLES`] chosen.
    sel: usize,
    /// Index shown (changes when the model has slid out).
    shown: usize,
    /// Slide position in quarters of the model distance, and direction.
    slide: i32,
    dir: i32,
    spin: i32,
    sizes: Vec<Size>,
    /// Paint colours per vehicle: [`PAINT_COUNT`] when the model has
    /// paintable faces (`objects::paintable`), else 1 (fixed livery).
    paints: Vec<u8>,
    /// Colour chosen per vehicle (`objects::PAINTS` index).
    colours: Vec<u8>,
}

impl VehicleSelect {
    /// On vehicle `VEHICLES[i]`.
    pub fn new(data: &GameData, i: usize) -> VehicleSelect {
        let sel = i.min(VEHICLES.len() - 1);
        let (sizes, paints) = VEHICLES
            .iter()
            .map(|&k| {
                let obj = data
                    .read(format!("Objects/{}", TYPES[k].model))
                    .ok()
                    .and_then(|b| payback_formats::Obj::parse(&b).ok());
                let paints = if obj.as_ref().is_some_and(paintable) { PAINT_COUNT as u8 } else { 1 };
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for v in obj.iter().flat_map(|o| o.vertices.iter()) {
                    let p = Vec3::new(v.x as f32, v.y as f32, v.z as f32);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                if lo.x > hi.x {
                    return (Size { centre: Vec3::ZERO, radius: 100.0 }, paints);
                }
                (Size { centre: (lo + hi) / 2.0, radius: ((hi - lo) / 2.0).length().max(1.0) }, paints)
            })
            .unzip();
        VehicleSelect {
            sel,
            shown: sel,
            slide: 0,
            dir: 0,
            spin: 0x2d0,
            sizes,
            paints,
            colours: vec![0; VEHICLES.len()],
        }
    }

    /// Debug (`--cheat-vehicle <type> [--cheat-colour <paint>]`): on
    /// [`TYPES`] index `kind`, in that colour if it has it.
    pub fn of_kind(data: &GameData, kind: usize) -> VehicleSelect {
        let mut s = VehicleSelect::new(data, VEHICLES.iter().position(|&k| k == kind).unwrap_or(0));
        if let Some(c) = crate::arg("--cheat-colour").and_then(|v| v.parse::<u8>().ok()) {
            s.colours[s.sel] = c.min(s.paints[s.sel] - 1);
        }
        s
    }
}

/// Original camera space (x right, y down, z depth) <-> render axes
/// (`frontend::model::to_render`, without the scale).
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.y, -v.z)
}

impl Screen for VehicleSelect {
    fn cd_track(&self) -> i32 {
        0
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.spin = (self.spin + SPIN).rem_euclid(TURN);
        // `OptionObj::tick`: out 3 steps, swap, in 3 steps.
        if self.dir != 0 {
            self.slide -= self.dir;
            if self.slide.abs() > 3 {
                self.shown = self.sel;
                self.slide = self.dir * 3;
            } else if self.slide == 0 {
                self.dir = 0;
            }
        }
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.left || input.right {
            let d = if input.right { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(VEHICLES.len() as i32) as usize;
            self.slide = 0;
            self.dir = d;
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if (input.up || input.down) && self.paints[self.sel] > 1 {
            let d = if input.down { 1 } else { -1 };
            let n = self.paints[self.sel] as i32;
            self.colours[self.sel] = (self.colours[self.sel] as i32 + d).rem_euclid(n) as u8;
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if input.fire {
            cx.sound(Sfx::Select);
            cx.requests.push(Request::Cheat(Cheat::Vehicle(VEHICLES[self.sel], self.colours[self.sel])));
            cx.requests.push(Request::Resume);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        panel(ui);
        ui.heading("SPAWN VEHICLE", TITLE_Y);
        let y = ui.hy(0x46);
        ui.centred(&cx.g(TYPES[VEHICLES[self.sel]].name_phrase), y, 1);
        ui.arrows(y);
        let n = format!("{}/{}", self.sel + 1, VEHICLES.len());
        ui.centred(&n, ui.hy(0x46) + 12, 2);
        let c = format!("Colour {}/{}", self.colours[self.sel] + 1, self.paints[self.sel]);
        ui.centred(&c, ui.hy(0x46) + 24, 2);
        ui.centred("Up/Down: colour", ui.hy(0x9a) - 12, 2);
        ui.centred("Enter: spawn   Esc: back", ui.hy(0x9a), 2);
    }

    /// The vehicle turning on its vertical axis, seen from above at
    /// [`ELEVATION`], its bounding box centred below the name.
    fn model(&self) -> Option<ModelView> {
        let size = self.sizes.get(self.shown)?;
        let z = size.radius * MODEL_UNIT * DISTANCE;
        let a = |v: i32| v as f32 / TURN as f32 * std::f32::consts::TAU;
        // The model's vertical (z) is the render z axis: spin about it,
        // then tip it back so the roof faces up and towards the camera.
        let q = Quat::from_rotation_x(-(std::f32::consts::FRAC_PI_2 - ELEVATION)) * Quat::from_rotation_z(a(self.spin));
        let (rz, ry, rx) = q.to_euler(EulerRot::ZYX);
        let unit = |r: f32| (r / std::f32::consts::TAU * TURN as f32).round() as i32;
        // `frontend::model::rotation`: Rz(-[5]) · Ry(-[4]) · Rx([3]).
        let rot = [unit(rx).rem_euclid(TURN), unit(-ry).rem_euclid(TURN), unit(-rz).rem_euclid(TURN)];
        let target = Vec3::new(self.slide as f32 * z / 4.0, 0.09 * z, z);
        let pos = target - flip(q * flip(size.centre * MODEL_UNIT));
        Some(ModelView {
            path: TYPES[VEHICLES[self.shown]].model,
            pos: [pos.x.round() as i32, pos.y.round() as i32, pos.z.round() as i32],
            rot,
            paint: (self.paints[self.shown] > 1).then_some(self.colours[self.shown]),
        })
    }
}
