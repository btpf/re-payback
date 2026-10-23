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
use crate::cheats::{Cheat, Cheats, MAX_LIVES, MAX_WANTED, VEHICLES};
use crate::objects::{PAINT_COUNT, paintable};
use crate::data::GameData;
use crate::vehicles::TYPES;
use bevy::math::{EulerRot, Quat, Vec3};

/// The menu panel, as the pause menu's (virtual 50..269 × 26..229).
pub(super) fn panel(ui: &mut Ui) {
    let (x0, x1) = (ui.vx(50), ui.vx(269));
    ui.back.darken(x0, 26, x1, 229);
}

pub(super) const TITLE_Y: i32 = 34;

/// Menu lines.
const ITEMS: usize = 8;

const GOD: usize = 0;
const VEHICLE_GOD: usize = 1;
const WEAPONS: usize = 2;
const VEHICLE: usize = 3;
/// "Wanted level: n/6": left/right step it, Enter clears it.
const WANTED: usize = 4;
/// "Lives: n": left/right step it.
const LIVES: usize = 5;
/// "Teleport to target" (the objective arrow's, `arrow::Seek`).
const TELEPORT: usize = 6;
/// The "Mission select" line (`super::missions`).
const MISSIONS: usize = 7;

/// Line spacing of the items (virtual 200-line units).
const LINE: i32 = 11;

/// First item line (virtual 200-line units): eight items above the hint
/// and "Esc" lines.
const TOP: i32 = 0x37;

/// Main cheat menu: God mode (On/Off), Vehicle god mode (On/Off), All
/// weapons, Spawn vehicle, Wanted level (left/right, Enter clears), Lives
/// (left/right), Teleport to target (dimmed with no objective arrow) and
/// Mission select (dimmed where the level has no missions: Rampage, the
/// jailbreak).
pub struct CheatMenu {
    sel: usize,
    /// God mode as it will be after the queued cheats.
    god: bool,
    /// Vehicle god mode as it will be after the queued cheats.
    vehicle_god: bool,
    /// The wanted level as it will be after the queued cheats.
    wanted: i32,
    /// The lives as they will be after the queued cheats.
    lives: i32,
    /// There is an objective arrow to teleport to.
    target: bool,
    /// The level's missions (`mission::levels::missions`).
    missions: &'static [crate::mission::levels::MissionInfo],
}

impl CheatMenu {
    /// Showing the cheats as they will be once the queue is applied.
    pub fn new(cheats: &Cheats, target: bool, missions: &'static [crate::mission::levels::MissionInfo]) -> CheatMenu {
        CheatMenu {
            sel: 0,
            god: cheats.god_pending(),
            vehicle_god: cheats.vehicle_god_pending(),
            wanted: cheats.wanted_pending(),
            lives: cheats.lives_pending(),
            target,
            missions,
        }
    }

    /// Debug (`--cheat-menu`, on line `--menu-item <n>`).
    pub fn at(
        cheats: &Cheats,
        target: bool,
        missions: &'static [crate::mission::levels::MissionInfo],
        sel: usize,
    ) -> CheatMenu {
        CheatMenu { sel: sel.min(ITEMS - 1), ..CheatMenu::new(cheats, target, missions) }
    }

    fn label(&self, cx: &Ctx, i: usize) -> String {
        match i {
            GOD => format!("God mode: {}", cx.g(if self.god { 2081 } else { 2080 })),
            VEHICLE_GOD => format!("Vehicle god mode: {}", cx.g(if self.vehicle_god { 2081 } else { 2080 })),
            WEAPONS => "All weapons".into(),
            VEHICLE => "Spawn vehicle".into(),
            WANTED => format!("Wanted level: {}/{}", self.wanted, MAX_WANTED),
            LIVES => format!("Lives: {}", self.lives),
            TELEPORT => "Teleport to target".into(),
            _ => "Mission select".into(),
        }
    }

    /// The line can't be chosen now.
    fn dimmed(&self, i: usize) -> bool {
        (i == MISSIONS && self.missions.is_empty()) || (i == TELEPORT && !self.target)
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
        let step = if input.right { 1 } else if input.left { -1 } else { 0 };
        if step != 0 {
            match self.sel {
                GOD | VEHICLE_GOD => {}
                WANTED => {
                    cx.sound(Sfx::Move);
                    self.wanted(cx, Cheat::Wanted(step));
                    return Step::Stay;
                }
                LIVES => {
                    cx.sound(Sfx::Move);
                    self.lives = (self.lives + step).clamp(0, MAX_LIVES);
                    cx.requests.push(Request::Cheat(Cheat::Lives(step)));
                    return Step::Stay;
                }
                _ => return Step::Stay,
            }
        }
        if step == 0 && !input.fire {
            return Step::Stay;
        }
        if self.dimmed(self.sel) {
            cx.sound(Sfx::Refuse);
            return Step::Stay;
        }
        cx.sound(Sfx::Select);
        match self.sel {
            GOD => {
                self.god = !self.god;
                cx.requests.push(Request::Cheat(Cheat::God(self.god)));
            }
            VEHICLE_GOD => {
                self.vehicle_god = !self.vehicle_god;
                cx.requests.push(Request::Cheat(Cheat::VehicleGod(self.vehicle_god)));
            }
            WEAPONS => {
                cx.requests.push(Request::Cheat(Cheat::AllWeapons));
                cx.requests.push(Request::Resume);
            }
            VEHICLE => return Step::Push(Box::new(VehicleSelect::new(cx.data, 0))),
            WANTED => self.wanted(cx, Cheat::ClearWanted),
            LIVES => {}
            TELEPORT => {
                cx.requests.push(Request::Cheat(Cheat::Teleport));
                cx.requests.push(Request::Resume);
            }
            _ => {
                return Step::Push(Box::new(super::missions::MissionMenu::new(cx.data, cx.texts, self.missions, 0)));
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        panel(ui);
        ui.heading("CHEATS", TITLE_Y);
        for i in 0..ITEMS {
            let y = ui.hy(TOP + LINE * i as i32);
            if self.dimmed(i) {
                ui.centred_dim(&self.label(cx, i), y, 1);
            } else {
                let c = if i == self.sel { ui.flash() } else { 1 };
                ui.centred(&self.label(cx, i), y, c);
            }
        }
        if matches!(self.sel, WANTED | LIVES) {
            let hint = if self.sel == WANTED { "Left/Right: change  Enter: clear" } else { "Left/Right: change" };
            ui.centred(hint, ui.hy(0x90), 2);
        }
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
pub(super) const SPIN: i32 = 0x30;

/// A vehicle model's bounding box centre and half diagonal, in model units.
#[derive(Clone, Copy, Debug)]
pub(super) struct Size {
    centre: Vec3,
    radius: f32,
}

/// The bounding box of `Objects/<model>` and whether it has paintable
/// faces (`objects::paintable`).
pub(super) fn model_size(data: &GameData, model: &str) -> (Size, bool) {
    let obj = data
        .read(format!("Objects/{model}"))
        .ok()
        .and_then(|b| payback_formats::Obj::parse(&b).ok());
    let paint = obj.as_ref().is_some_and(paintable);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in obj.iter().flat_map(|o| o.vertices.iter()) {
        let p = Vec3::new(v.x as f32, v.y as f32, v.z as f32);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if lo.x > hi.x {
        return (Size { centre: Vec3::ZERO, radius: 100.0 }, paint);
    }
    (Size { centre: (lo + hi) / 2.0, radius: ((hi - lo) / 2.0).length().max(1.0) }, paint)
}

/// A vehicle model turning on its vertical axis by `spin`, seen from above
/// at [`ELEVATION`], its bounding box centred below the menu title,
/// `slide` quarters of its distance to the side; `near` scales its
/// distance (1: the vehicle selection's) and `drop` its offset below the
/// screen centre, in model distances (the vehicle selection's: 0.09).
pub(super) fn turning_model(
    path: &'static str,
    size: Size,
    spin: i32,
    slide: i32,
    paint: Option<u8>,
    near: f32,
    drop: f32,
) -> ModelView {
    let z = size.radius * MODEL_UNIT * DISTANCE * near;
    let a = |v: i32| v as f32 / TURN as f32 * std::f32::consts::TAU;
    // The model's vertical (z) is the render z axis: spin about it,
    // then tip it back so the roof faces up and towards the camera.
    let q = Quat::from_rotation_x(-(std::f32::consts::FRAC_PI_2 - ELEVATION)) * Quat::from_rotation_z(a(spin));
    let (rz, ry, rx) = q.to_euler(EulerRot::ZYX);
    let unit = |r: f32| (r / std::f32::consts::TAU * TURN as f32).round() as i32;
    // `frontend::model::rotation`: Rz(-[5]) · Ry(-[4]) · Rx([3]).
    let rot = [unit(rx).rem_euclid(TURN), unit(-ry).rem_euclid(TURN), unit(-rz).rem_euclid(TURN)];
    let target = Vec3::new(slide as f32 * z / 4.0, drop * z, z);
    let pos = target - flip(q * flip(size.centre * MODEL_UNIT));
    ModelView {
        path,
        pos: [pos.x.round() as i32, pos.y.round() as i32, pos.z.round() as i32],
        rot,
        paint,
    }
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
                let (size, paint) = model_size(data, TYPES[k].model);
                (size, if paint { PAINT_COUNT as u8 } else { 1 })
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
        let size = *self.sizes.get(self.shown)?;
        let paint = (self.paints[self.shown] > 1).then_some(self.colours[self.shown]);
        Some(turning_model(TYPES[VEHICLES[self.shown]].model, size, self.spin, self.slide, paint, 1.0, 0.09))
    }
}
