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

/// Menu ticks per second (the OS4 build's 30 Hz frame tick).
pub const MENU_HZ: f64 = 30.0;

/// One full turn in object angle units.
pub const TURN: i32 = 0x1680;

/// Menu keys pressed since the last tick (`FUN_0103f3d0`: left, right,
/// up, down, fire, Esc), plus typed characters for text entry.
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
}

/// Menu sounds (`FUN_010053c4(id, period)`, played on Paula at full
/// volume; rate = PAL clock / period).
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
        (id, 3_546_895.0 / period as f32)
    }
}

/// Things only the game loop can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    StartLevel { city: u8, level: u8 },
    Exit,
    PlayIntro,
    /// Leave the pause menu.
    Resume,
    /// Pause menu "Quit to title screen".
    QuitToTitle,
}

/// What a screen hands back to the one below it.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Cancel,
    Done,
    Level(u8, u8),
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

/// Drawing target for one frame: the back layer (behind the 3D model)
/// and the front layer (over it).
pub struct Ui<'a> {
    pub back: Painter<'a>,
    pub front: Painter<'a>,
    pub fonts: &'a HudIcons,
    pub fe: &'a FrontendIcons,
    pub w: i32,
    pub frame: u32,
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
    pub fn picture(&mut self, dy: i32, dark: bool) {
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
    /// Screen brightness 0..1 (palette fades, `FUN_0104138c`).
    fn brightness(&self) -> f32 {
        1.0
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
