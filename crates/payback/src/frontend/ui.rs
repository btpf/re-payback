//! Menu drawing and the screen interface.
//!
//! Layout follows the original's virtual 320×256 grid: x scales with
//! `xs = (W+8)*16/320` (`v*xs>>4`), y is 1:1 on our 256-line canvas, and
//! "n/200 of H" positions become `256*n/200`.

use super::assets::{FrontendIcons, Picture};
use super::progress::{Profile, Settings};
use crate::data::GameData;
use crate::hud::icons::{HudIcons, Icon};
use crate::hud::paint::{Blend, CANVAS_H, Painter, text_len};
use crate::hud::{Language, Texts};
use bevy::prelude::{GamepadButton, KeyCode};
use payback_formats::replay::Replay;

/// Menu ticks per second (the OS4 build's 30 Hz frame tick).
pub const MENU_HZ: f64 = 30.0;

/// One full turn in object angle units.
pub const TURN: i32 = 0x1680;

/// Menu keys pressed since the last tick (`FUN_0103f3d0`: left, right,
/// up, down, fire, Esc), plus typed characters for text entry. A gamepad
/// drives the same keys: d-pad or left stick, A (South) or Start = fire,
/// B (East) = back.
#[derive(Default, Clone, Debug)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// Enter or Space.
    pub fire: bool,
    /// Enter alone (text entry, where Space is typed).
    pub enter: bool,
    pub back: bool,
    pub typed: Vec<char>,
    pub erase: bool,
    /// Every key and gamepad button pressed (the control setup's
    /// redefine prompt).
    pub keys: Vec<KeyCode>,
    pub pad: Vec<GamepadButton>,
}

/// Menu sounds (Amiga `FUN_010053c4(id, period)`, Mac `PlaySound__Fii`
/// @ 100f1960): rate = NTSC clock / period, `1.0 / (2.79365e-7 · period)`
/// on the Mac (the Amiga used the PAL clock 3,546,895); played through
/// [`crate::audio::Sound::menu`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    /// (0x20, 0xeb) ChangeSelection.
    Move,
    /// (0x21, 0x10e) ChangeSelection2.
    Select,
    /// (0x2d, 0xa0) ChangeSelection3.
    Refuse,
}

impl Sfx {
    pub fn id_hz(self) -> (u16, f32) {
        let (id, period) = match self {
            Sfx::Move => (0x20, 0xeb),
            Sfx::Select => (0x21, 0x10e),
            Sfx::Refuse => (0x2d, 0xa0),
        };
        (id, 3_579_545.0 / period as f32)
    }
}

/// Things only the game loop can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    StartLevel { city: u8, level: u8 },
    /// Rampage on `city` with police aggressiveness 0..5.
    StartRampage { city: u8, police: u8 },
    /// Multiplayer deathmatch on `city` with the setup in the settings.
    StartMultiplayer { city: u8 },
    /// Play a replay; afterwards return to the game menu or the main menu.
    PlayReplay { replay: Box<Replay>, back_to_game: bool },
    Exit,
    PlayIntro,
    /// Leave the pause menu.
    Resume,
    /// Pause menu "Quit to title screen".
    QuitToTitle,
    /// Cheat menu (port addition): queue a cheat for the next game tick
    /// (`crate::cheats`).
    Cheat(crate::cheats::Cheat),
}

/// What a screen hands back to the one below it.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Cancel,
    Done,
    Level(u8, u8),
    /// Rampage setup "Play": city, police aggressiveness.
    Rampage(u8, u8),
    /// Multiplayer setup "Play" / results "Play again": city.
    Multiplayer(u8),
    /// Replay info "View Replay".
    Replay(Box<Replay>),
    Text(String),
}

pub enum Step {
    Stay,
    Push(Box<dyn Screen>),
    Pop(Outcome),
    Replace(Box<dyn Screen>),
}

/// Game state visible to screens.
pub struct Ctx<'a> {
    pub data: &'a GameData,
    pub texts: &'a Texts,
    pub profile: &'a mut Profile,
    pub settings: &'a mut Settings,
    pub language: &'a mut Language,
    /// Languages found in `Phrases/` (`DAT_0117aee4`).
    pub languages: &'a [String],
    /// Pause menu: the level's money goal (<= 0 = escape) and ticks.
    pub goal: i32,
    pub level_ticks: u32,
    /// Story level (the goal line is hidden in rampage and multiplayer).
    pub story: bool,
    /// The last game played (game menu "View replay" / "Save replay").
    pub last_replay: Option<&'a Replay>,
    /// Multiplayer won/drawn/lost totals.
    pub mp_totals: &'a mut crate::multiplayer::MpTotals,
    /// User maps, cities 3.. (`DAT_01170808`).
    pub user_maps: &'a [super::usermaps::UserMap],
    /// Hardware (Warp3D) rendering is active (`DAT_0114c7d8`,
    /// `RenderMode`): full-brightness title, Motion blur refused.
    pub hardware: bool,
    /// The window's monitor and size (graphics options).
    pub display: &'a super::display::Display,
    /// Gamepads connected (the Mac offers the multiplayer devices 0..3
    /// only when HID input exists, `ControlMethodExists` @ 100f8420).
    pub pads: usize,
    pub sfx: Vec<Sfx>,
    pub requests: Vec<Request>,
}

impl Ctx<'_> {
    pub fn g(&self, n: u32) -> String {
        self.texts.global(n)
    }

    pub fn sound(&mut self, s: Sfx) {
        self.sfx.push(s);
    }
}

/// A rotating option model (`FUN_010c0c1c` instance): position in the
/// original's camera space (x right, y down, z depth) and angles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelView {
    pub path: &'static str,
    pub pos: [i32; 3],
    pub rot: [i32; 3],
}

/// The spinning option object of the main/game/pause menus.
#[derive(Clone, Debug)]
pub struct OptionObj {
    pub view: ModelView,
    /// Slide direction: -1 left, +1 right, 0 still.
    dir: i32,
    next: &'static str,
    speed: i32,
    swap_at: i32,
}

impl OptionObj {
    pub fn new(path: &'static str, y: i32, z: i32) -> OptionObj {
        OptionObj {
            view: ModelView {
                path,
                pos: [0, y, z],
                rot: [0; 3],
            },
            dir: 0,
            next: path,
            speed: 0x5dc,
            swap_at: 0x1194,
        }
    }

    /// Slower slide (pause menu: 0x2ee per frame, swap at 0x8ca).
    pub fn slow(mut self) -> OptionObj {
        self.speed = 0x2ee;
        self.swap_at = 0x8ca;
        self
    }

    /// [3] -= 0x20, [4] += 0x40, [5] -= 0x10 per frame, then the slide.
    pub fn tick(&mut self) {
        let r = &mut self.view.rot;
        r[0] = (r[0] - 0x20).rem_euclid(TURN);
        r[1] = (r[1] + 0x40).rem_euclid(TURN);
        r[2] = (r[2] - 0x10).rem_euclid(TURN);
        // Moving right (+1) slides the object off to the left, then the
        // new model comes in from the right.
        if self.dir != 0 {
            let x = &mut self.view.pos[0];
            *x -= self.dir * self.speed;
            if x.abs() > self.swap_at {
                self.view.path = self.next;
                *x = self.dir * self.swap_at;
            } else if *x == 0 {
                self.dir = 0;
            }
        }
    }

    /// Start sliding to `model` (direction +1 = selection moved right).
    pub fn change(&mut self, model: &'static str, dir: i32) {
        self.view.pos[0] = 0;
        self.dir = dir;
        self.next = model;
    }
}

/// The model the Mac spins behind its control setup (`controlsetup`,
/// `DoControlSetup` @ 100a1f80) and music setup (`cd`, `DoCDAudioSetup` @
/// 100a1500); the Amiga screens have none. `CreateObject(model, 0, 600, 0)`
/// with z = 6000, the option objects' spin, y halved every frame, and z
/// bobbing: z += v, then v += 2 while z <= 6000, else v -= 2 (v from 100).
#[derive(Clone, Copy, Debug)]
pub struct Bobbing {
    pub view: ModelView,
    v: i32,
}

impl Bobbing {
    pub fn new(path: &'static str) -> Bobbing {
        Bobbing {
            view: ModelView {
                path,
                pos: [0, 600, 6000],
                rot: [0; 3],
            },
            v: 100,
        }
    }

    pub fn tick(&mut self) {
        let r = &mut self.view.rot;
        r[0] = (r[0] - 0x20).rem_euclid(TURN);
        r[1] = (r[1] + 0x40).rem_euclid(TURN);
        r[2] = (r[2] - 0x10).rem_euclid(TURN);
        let p = &mut self.view.pos;
        p[1] >>= 1;
        p[2] += self.v;
        self.v += if p[2] < 0x1771 { 2 } else { -2 };
    }
}

/// Drawing target for one frame: the back layer (behind the 3D model)
/// and the front layer (over it).
pub struct Ui<'a> {
    pub back: Painter<'a>,
    pub front: Painter<'a>,
    pub fonts: &'a HudIcons,
    pub fe: &'a FrontendIcons,
    pub w: i32,
    pub frame: u32,
    /// Hardware rendering: [`Ui::picture`] stretches `TitleScreen.w3d`
    /// at full brightness.
    pub hardware: bool,
}

pub const H: i32 = CANVAS_H as i32;

impl Ui<'_> {
    pub fn cx(&self) -> i32 {
        self.w / 2
    }

    pub fn cy(&self) -> i32 {
        H / 2
    }

    /// Virtual x to canvas x.
    pub fn vx(&self, x: i32) -> i32 {
        x * ((self.w + 8) * 16 / 320) >> 4
    }

    /// `H*n/200`.
    pub fn hy(&self, n: i32) -> i32 {
        H * n / 200
    }

    /// Flashing colour of a selected line (4-frame cycle).
    pub fn flash(&self) -> u8 {
        if self.frame % 4 > 1 { 2 } else { 1 }
    }

    /// Small-font text: colour 1 = red "You", 2 = blue "Them"
    /// (`FUN_0100bb94`), 7 px pitch.
    pub fn text(&mut self, s: &str, x: i32, y: i32, colour: u8) {
        let font = if colour == 2 { &self.fonts.them } else { &self.fonts.you };
        self.front.text(font, s, x, y, 7, Blend::Plain);
    }

    /// Dimmed small-font text (`FUN_0100cc78`: each glyph pixel through
    /// the 50 % blend with colour 0, i.e. half brightness), centred. Used
    /// for an option that cannot be changed.
    pub fn centred_dim(&mut self, s: &str, y: i32, colour: u8) {
        let x = self.cx() - 7 * text_len(s) / 2;
        self.text_dim(s, x, y, colour);
    }

    /// [`Ui::centred_dim`] from `x`.
    pub fn text_dim(&mut self, s: &str, mut x: i32, y: i32, colour: u8) {
        let font = if colour == 2 { &self.fonts.them } else { &self.fonts.you };
        let pal = self.front.pal;
        let mut chars = s.chars();
        while let Some(mut c) = chars.next() {
            if c == '^' {
                match chars.next() {
                    Some(n) => c = n,
                    None => break,
                }
            }
            let b = if (c as u32) < 256 { c as u8 } else { b'?' };
            if let Some(g) = font.glyph(b).filter(|_| b != b' ') {
                for iy in 0..g.h {
                    for ix in 0..g.w {
                        let p = g.px[iy * g.w + ix] as usize;
                        if p != 0 {
                            let rgb = [pal[p * 3] / 2, pal[p * 3 + 1] / 2, pal[p * 3 + 2] / 2];
                            self.front.put_rgb(x + ix as i32, y + iy as i32, rgb);
                        }
                    }
                }
            }
            x += 7;
        }
    }

    pub fn centred(&mut self, s: &str, y: i32, colour: u8) {
        let x = self.cx() - 7 * text_len(s) / 2;
        self.text(s, x, y, colour);
    }

    /// Right-aligned at `x`.
    pub fn right(&mut self, s: &str, x: i32, y: i32, colour: u8) {
        self.text(s, x - 7 * text_len(s), y, colour);
    }

    /// Wide title text (`FUN_0100ba18`: Them font, 10 px pitch), centred.
    pub fn heading(&mut self, s: &str, y: i32) {
        let x = self.cx() - 10 * text_len(s) / 2;
        self.front.text(&self.fonts.them, s, x, y, 10, Blend::Plain);
    }

    /// Screen title at H/40.
    pub fn title(&mut self, s: &str) {
        self.heading(s, H / 40);
    }

    /// Centred text word-wrapped to the screen width (`FUN_0100bd24`,
    /// blue, 12 px lines). Returns the y after the last line.
    pub fn wrapped(&mut self, s: &str, mut y: i32, colour: u8) -> i32 {
        let max = ((self.w - 20) / 7) as usize;
        let mut line = String::new();
        for word in s.split_whitespace() {
            if !line.is_empty() && line.len() + word.len() + 1 > max {
                self.centred(&line, y, colour);
                y += 12;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            self.centred(&line, y, colour);
            y += 12;
        }
        y
    }

    pub fn icon(&mut self, icon: &Icon, x: i32, y: i32) {
        self.front.icon(icon, x, y, Blend::Plain);
    }

    /// The left/right arrows around the item label.
    pub fn arrows(&mut self, y: i32) {
        let d = self.vx(0x60);
        let fe = self.fe;
        self.icon(&fe.left_arrow, self.cx() - d - 14, y);
        self.icon(&fe.right_arrow, self.cx() + d, y);
    }

    /// Fill the back layer with the title picture, scaled to the canvas
    /// (`FUN_0100d9c0`) and shifted down by `dy` (`FUN_0100ab78`).
    /// `dark` = the darkened sub-screen copy (`DAT_0117b6b4`, the 50 %
    /// blend with black).
    ///
    /// Hardware rendering draws `TitleScreen.w3d` stretched over the whole
    /// screen instead, at full brightness on every screen (the dimmed copy
    /// is never built there).
    pub fn picture(&mut self, dy: i32, dark: bool) {
        if let Some(rgb) = self.fe.title_w3d.as_deref().filter(|_| self.hardware) {
            for y in 0..H {
                let sy = y - dy;
                for x in 0..self.w {
                    let c = if (0..H).contains(&sy) {
                        let (tx, ty) = (x as usize * 256 / self.w as usize, sy as usize * 256 / H as usize);
                        let i = (ty * 256 + tx) * 3;
                        [rgb[i], rgb[i + 1], rgb[i + 2]]
                    } else {
                        [0, 0, 0]
                    };
                    self.back.put_rgb(x, y, c);
                }
            }
            return;
        }
        let pic: &Picture = &self.fe.title;
        let pal = self.back.pal;
        for y in 0..H {
            let sy = y - dy;
            for x in 0..self.w {
                let rgb = if (0..pic.h as i32).contains(&sy) {
                    let sx = x as usize * pic.w / self.w as usize;
                    let c = pic.px[sy as usize * pic.w + sx] as usize;
                    let mut rgb = [pal[c * 3], pal[c * 3 + 1], pal[c * 3 + 2]];
                    if dark {
                        rgb = rgb.map(|v| v / 2);
                    }
                    rgb
                } else {
                    [0, 0, 0]
                };
                self.back.put_rgb(x, y, rgb);
            }
        }
    }

    /// A palette picture scaled to a `size`×`size` square centred at
    /// (x, y) (the textured quads of `FUN_010c3e10`).
    pub fn square(&mut self, pic: &Picture, x: i32, y: i32, size: i32) {
        if size <= 0 {
            return;
        }
        let pal = self.front.pal;
        for dy in 0..size {
            let sy = dy as usize * pic.h / size as usize;
            for dx in 0..size {
                let sx = dx as usize * pic.w / size as usize;
                let c = pic.px[sy * pic.w + sx] as usize;
                self.front.put_rgb(
                    x - size / 2 + dx,
                    y - size / 2 + dy,
                    [pal[c * 3], pal[c * 3 + 1], pal[c * 3 + 2]],
                );
            }
        }
    }
}

impl Ui<'_> {
    /// A palette picture on a square of corner radius `r` centred at
    /// (x, y), rotated by `angle` (`FUN_010c3e10(tex, x, y, r, angle)`:
    /// corners at `angle + k·0x5a0`).
    pub fn rotated(&mut self, pic: &Picture, x: i32, y: i32, r: i32, angle: i32) {
        let a = angle as f32 * std::f32::consts::TAU / TURN as f32;
        // Corner k sits at angle a + k·90°, so the edges are at a ± 45°.
        let (s, c) = (a - std::f32::consts::FRAC_PI_4).sin_cos();
        let half = r as f32 / std::f32::consts::SQRT_2;
        let pal = self.front.pal;
        for py in y - r..=y + r {
            for px in x - r..=x + r {
                let (dx, dy) = ((px - x) as f32, (py - y) as f32);
                // Into the square's own axes, -half..half.
                let u = dx * c + dy * s;
                let v = -dx * s + dy * c;
                if u.abs() >= half || v.abs() >= half {
                    continue;
                }
                let sx = ((u + half) / (2.0 * half) * pic.w as f32) as usize;
                let sy = ((v + half) / (2.0 * half) * pic.h as f32) as usize;
                let col = pic.px[sy.min(pic.h - 1) * pic.w + sx.min(pic.w - 1)] as usize;
                self.front.put_rgb(px, py, [pal[col * 3], pal[col * 3 + 1], pal[col * 3 + 2]]);
            }
        }
    }
}

/// `h:mm` as the original prints minutes (`"%d:%d"` / `"%d:0%d"`).
pub fn hmm(minutes: i32) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// A front-end screen. The original runs each as a modal loop; here the
/// top of a stack is ticked at [`MENU_HZ`].
pub trait Screen: Send + Sync + 'static {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step;
    fn draw(&self, ui: &mut Ui, cx: &Ctx);
    /// A pushed screen returned.
    fn resume(&mut self, _cx: &mut Ctx, _outcome: Outcome) -> Step {
        Step::Stay
    }
    fn model(&self) -> Option<ModelView> {
        None
    }
    /// A second model drawn behind [`Screen::model`] (the pause menu's
    /// `options.obj`).
    fn back_model(&self) -> Option<ModelView> {
        None
    }
    /// Screen brightness 0..1 (palette fades, `FUN_0104138c`).
    fn brightness(&self) -> f32 {
        1.0
    }
    /// The CD track this screen keeps playing (`CD_CheckForTrackEnd(n)`
    /// in its loop, Mac @ 100fb000; `crate::music`): 13 in the menus.
    fn cd_track(&self) -> i32 {
        13
    }
}

/// The 26-frame title slide (`FUN_0100ab78(H*i/25)` with a palette fade):
/// `i` runs -25..0 sliding in, 0..25 sliding out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slide {
    pub i: i32,
    pub out: bool,
}

impl Slide {
    pub fn slide_in() -> Slide {
        Slide { i: -25, out: false }
    }

    pub fn slide_out() -> Slide {
        Slide { i: 0, out: true }
    }

    /// Advance; true when finished.
    pub fn tick(&mut self) -> bool {
        if self.out {
            self.i += 1;
            self.i > 25
        } else {
            self.i += 1;
            self.i > 0
        }
    }

    pub fn dy(&self) -> i32 {
        H * self.i.min(25) / 25
    }

    pub fn brightness(&self) -> f32 {
        1.0 - self.i.abs().min(25) as f32 / 25.0
    }
}
