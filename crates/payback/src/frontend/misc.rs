//! Sub-screens: text entry (`FUN_01012e38`), message box (`FUN_0101dc60`),
//! load/save (`FUN_0101f1a4` / `FUN_0101f738`), credits (`FUN_010206bc`), language chooser
//! (`FUN_0101267c`), control setup (`FUN_0102e75c`) and CD audio setup
//! (`FUN_0102dd30`). The replay list is in `replays.rs`.

use super::bindings::{self, COLUMNS, Controls, Table};
use super::progress::{self, Profile, SLOTS};
use super::ui::*;
use bevy::prelude::{GamepadAxis, GamepadButton, KeyCode};
use crate::hud::paint::text_len;

/// Characters accepted by text entry: letters, digits, space and
/// `! ( ) , - . : ; _` (mask 0x0fff7303 over 0x20..0x3b).
fn allowed(c: char) -> bool {
    let n = c as u32;
    c.is_ascii_alphabetic()
        || c == '_'
        || ((0x20..0x3c).contains(&n) && 0x0fff_7303u32 >> (n - 0x20) & 1 != 0)
}

pub struct TextEntry {
    prompt: String,
    buf: String,
    max: usize,
}

impl TextEntry {
    pub fn new(prompt: String, default: &str, max: usize) -> TextEntry {
        TextEntry {
            prompt,
            buf: default.chars().take(max).collect(),
            max,
        }
    }
}

impl Screen for TextEntry {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.enter {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Text(self.buf.clone()));
        }
        if input.erase {
            if self.buf.pop().is_some() {
                cx.sound(Sfx::Select);
            } else {
                cx.sound(Sfx::Refuse);
            }
        }
        for &c in &input.typed {
            if !allowed(c) {
                continue;
            }
            if self.buf.chars().count() >= self.max {
                cx.sound(Sfx::Refuse);
            } else {
                self.buf.push(c);
                cx.sound(Sfx::Select);
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, false);
        ui.wrapped(&self.prompt, 0x3c, 2);
        let x = ui.cx() - 7 * text_len(&self.buf) / 2;
        ui.text(&self.buf, x, 0x5f, 1);
        if ui.frame & 8 != 0 {
            let fe = ui.fe;
            ui.icon(&fe.cursor, x + 7 * text_len(&self.buf), 0x5f);
        }
        ui.centred(&cx.g(2119), 0x91, 2);
    }
}

/// A heading and a flashing "Continue".
pub struct Message {
    text: String,
}

impl Message {
    pub fn new(text: String) -> Message {
        Message { text }
    }
}

impl Screen for Message {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.fire || input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Done);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.heading(&self.text, 0x5f);
        ui.centred(&cx.g(2110), 0x91, ui.flash());
    }
}

/// Up/down over `n` lines with wrap.
fn pick(sel: &mut usize, n: usize, cx: &mut Ctx, input: &Input) {
    if input.up || input.down {
        *sel = (*sel as isize + if input.down { 1 } else { -1 }).rem_euclid(n as isize) as usize;
        cx.sound(Sfx::Move);
    }
}

/// The 14 slot lines and "Cancel": load at virtual y = 0x20 + 14i, Cancel
/// at 0xeb (`FUN_0101f1a4`, Mac `DoLoadGame` @ 10096950); save at
/// (0x19 + 11i)/200 of the height, Cancel at 0xb8/200 (`FUN_0101f738`, Mac
/// `DoSaveGame` @ 10096d80). An empty slot reads "Slot %d" (`FUN_01011da8`).
fn draw_slots(ui: &mut Ui, cx: &Ctx, names: &[Option<String>], sel: usize, save: bool) {
    for i in 0..=SLOTS {
        let y = match (save, i == SLOTS) {
            (false, false) => 0x20 + 14 * i as i32,
            (false, true) => 0xeb,
            (true, false) => ui.hy(0x19 + 11 * i as i32),
            (true, true) => ui.hy(0xb8),
        };
        let s = match names.get(i) {
            Some(Some(n)) => n.clone(),
            Some(None) => format!("Slot {}", i + 1),
            None => cx.g(2114),
        };
        let c = if i == sel { ui.flash() } else { 2 };
        ui.centred(&s, y, c);
    }
}

pub struct LoadGame {
    sel: usize,
    names: Vec<Option<String>>,
    done: bool,
}

impl LoadGame {
    pub fn new() -> LoadGame {
        LoadGame {
            sel: 0,
            names: progress::slot_names(),
            done: false,
        }
    }
}

impl Screen for LoadGame {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if self.done {
            return Step::Pop(Outcome::Done);
        }
        pick(&mut self.sel, SLOTS + 1, cx, input);
        if input.back || (input.fire && self.sel == SLOTS) {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if !input.fire {
            return Step::Stay;
        }
        match progress::load_slot(self.sel) {
            Some((_, p)) => {
                cx.sound(Sfx::Select);
                *cx.profile = Profile { played: false, ..p };
                self.done = true;
                Step::Push(Box::new(Message::new(cx.g(2111))))
            }
            None => {
                cx.sound(Sfx::Refuse);
                Step::Push(Box::new(Message::new(cx.g(2112))))
            }
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2082));
        draw_slots(ui, cx, &self.names, self.sel, false);
    }
}

pub struct SaveGame {
    sel: usize,
    names: Vec<Option<String>>,
    state: SaveState,
}

#[derive(PartialEq, Eq)]
enum SaveState {
    Pick,
    Naming,
    Saved,
}

impl SaveGame {
    pub fn new() -> SaveGame {
        SaveGame {
            sel: 0,
            names: progress::slot_names(),
            state: SaveState::Pick,
        }
    }
}

impl Screen for SaveGame {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if self.state == SaveState::Saved {
            return Step::Pop(Outcome::Done);
        }
        pick(&mut self.sel, SLOTS + 1, cx, input);
        if input.back || (input.fire && self.sel == SLOTS) {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if !input.fire {
            return Step::Stay;
        }
        cx.sound(Sfx::Select);
        // A used slot offers its description; an empty one "<name> -
        // C<city>, L<level>" of the first level without a cup.
        let default = self.names[self.sel].clone().unwrap_or_else(|| cx.profile.slot_description());
        self.state = SaveState::Naming;
        Step::Push(Box::new(TextEntry::new(cx.g(2084), &default, 40)))
    }

    fn resume(&mut self, cx: &mut Ctx, outcome: Outcome) -> Step {
        match outcome {
            Outcome::Text(desc) if self.state == SaveState::Naming => {
                if let Err(e) = progress::save_slot(self.sel, &desc, cx.profile) {
                    bevy::log::warn!("saving slot {}: {e}", self.sel + 1);
                    cx.sound(Sfx::Refuse);
                    self.state = SaveState::Pick;
                    return Step::Stay;
                }
                self.names = progress::slot_names();
                self.state = SaveState::Saved;
                Step::Push(Box::new(Message::new(cx.g(2083))))
            }
            _ => {
                self.state = SaveState::Pick;
                Step::Stay
            }
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2089));
        draw_slots(ui, cx, &self.names, self.sel, true);
    }
}

/// The Mac's About box (Apple menu "About Payback…", `ShowAboutBox__11CPaybackAppFl`
/// @ 1004ea30, PPob 2000: the title, a caption and the `vers` 1 string),
/// shown here at the end of the main menu's credits ([`Credits::with_about`])
/// since the port has no menu bar.
pub const ABOUT: [&str; 4] = ["", " Payback", "$Macintosh version by Alex Curylo", "$1.0.0, \u{a9}2002 VPL"];

/// Scrolling credits: one phrase per line from `first` until `!`.
pub struct Credits {
    lines: Vec<String>,
    /// Lines added after the phrases ([`ABOUT`]).
    extra: Vec<String>,
    speed: i32,
    skippable: bool,
    stop_y: i32,
    hold: i32,
    y: i32,
    loaded: bool,
    first: u32,
}

/// Line markup (first character).
fn line_height(l: &str) -> i32 {
    match l.chars().next() {
        Some('?') => 84,
        Some('\u{a3}') => H / 2,
        Some(' ') => 16,
        _ => 12,
    }
}

impl Credits {
    pub fn new(first: u32, speed: i32, skippable: bool, stop_y: i32, hold: i32) -> Credits {
        Credits {
            lines: Vec::new(),
            extra: Vec::new(),
            speed,
            skippable,
            stop_y,
            hold,
            y: H,
            loaded: false,
            first,
        }
    }

    /// With the Mac About box's text at the end ([`ABOUT`]).
    pub fn with_about(mut self) -> Credits {
        self.extra = ABOUT.iter().map(|s| s.to_string()).collect();
        self
    }

    fn height(&self) -> i32 {
        self.lines.iter().map(|l| line_height(l)).sum()
    }
}

impl Screen for Credits {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if !self.loaded {
            self.loaded = true;
            let mut n = self.first;
            loop {
                let l = cx.g(n);
                if l.starts_with('!') || n > self.first + 400 {
                    break;
                }
                self.lines.push(l);
                n += 1;
            }
            self.lines.append(&mut self.extra);
        }
        if self.skippable && (input.fire || input.back) {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Done);
        }
        // Scroll until the last line reaches stop_y, then hold.
        if self.y + self.height() > self.stop_y + 12 {
            self.y -= self.speed;
        } else {
            self.hold -= 1;
            if self.hold <= 0 {
                return Step::Pop(Outcome::Done);
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, _cx: &Ctx) {
        ui.picture(0, true);
        let mut y = self.y;
        let (left, right) = (ui.vx(20), ui.vx(300));
        for l in &self.lines {
            let h = line_height(l);
            if y > -h && y < H {
                let mut chars = l.chars();
                let kind = chars.next();
                let rest: String = chars.collect();
                match kind {
                    Some(' ') => ui.heading(&rest, y),
                    Some('#') => ui.text(&rest, left, y, 2),
                    Some('@') => ui.right(&rest, right, y, 1),
                    Some('$') => ui.centred(&rest, y, 2),
                    Some('%') => ui.centred(&rest, y, 1),
                    Some('?') => {
                        let fe = ui.fe;
                        ui.icon(&fe.logo, ui.cx() - 83, y);
                    }
                    _ => {}
                }
            }
            y += h;
        }
    }
}

/// The on-disk language token's display name: the extracted file names
/// were decoded as code page 437, so map those characters back to their
/// Latin-1 bytes (e.g. `Espa±ol` -> `Español`).
pub fn language_name(token: &str) -> String {
    const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
    token
        .chars()
        .map(|c| match CP437.chars().position(|x| x == c) {
            Some(i) if !c.is_ascii() => char::from_u32(0x80 + i as u32).unwrap_or('?'),
            _ => c,
        })
        .collect()
}

/// "Please choose your language" (hard-coded English).
pub struct LanguageChooser {
    sel: usize,
    /// Eased list position ×16.
    pos: i32,
    first_run: bool,
}

impl LanguageChooser {
    pub fn new(cx: &Ctx) -> LanguageChooser {
        let sel = cx
            .languages
            .iter()
            .position(|l| *l == cx.language.0)
            .or_else(|| cx.languages.iter().position(|l| l == "English"))
            .unwrap_or(0);
        LanguageChooser {
            sel,
            pos: sel as i32 * 12 * 16,
            first_run: cx.settings.language.is_empty(),
        }
    }
}

impl Screen for LanguageChooser {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        let n = cx.languages.len().max(1);
        pick(&mut self.sel, n, cx, input);
        let target = self.sel as i32 * 12 * 16;
        self.pos = (target + 8 * self.pos) / 9;
        if input.back && !self.first_run {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.fire {
            cx.sound(Sfx::Select);
            if let Some(l) = cx.languages.get(self.sel) {
                cx.settings.language = l.clone();
                if cx.language.0 != *l {
                    cx.language.0 = l.clone();
                }
                cx.settings.save();
            }
            return Step::Pop(Outcome::Done);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title("Please choose your language");
        let cy = ui.cy();
        for (i, l) in cx.languages.iter().enumerate() {
            let y = cy - 5 + i as i32 * 12 - self.pos / 16;
            if !(20..H - 10).contains(&y) {
                continue;
            }
            let c = if i == self.sel { ui.flash() } else { 2 };
            ui.centred(&language_name(l), y, c);
        }
    }
}

/// Key name for a binding (`FUN_0103c690`: "-?-" when disabled, phrases
/// 1520.. for the special keys). Character keys show what they print on
/// the keyboard layout once they have been pressed (`bindings::key_label`),
/// else their US name.
pub fn key_name(cx: &Ctx, key: Option<KeyCode>) -> String {
    let Some(key) = key else {
        return "-?-".into();
    };
    if let Some(label) = bindings::key_label(key) {
        return label;
    }
    let id = bindings::key_id(key);
    let phrase = match id.as_str() {
        "Space" => 1521,
        "Enter" => 1520,
        "Delete" => 1523,
        "NumpadEnter" => 1524,
        "ControlLeft" | "ControlRight" => 1525,
        "ShiftLeft" => 1526,
        "AltLeft" => 1527,
        "ShiftRight" => 1529,
        "AltRight" => 1530,
        "Tab" => 1532,
        "Backspace" => 1533,
        "NumpadMultiply" => 1534,
        "PageUp" => return "PgUp".into(),
        "PageDown" => return "PgDn".into(),
        k => {
            let short = k.strip_prefix("Arrow").or(k.strip_prefix("Key")).or(k.strip_prefix("Digit"));
            return short.unwrap_or(k).to_string();
        }
    };
    cx.g(phrase)
}

/// Gamepad control name (phrases 1500–1505 for Select, Start, L1/R1/L2/R2;
/// axes as "LX+", "RY-", "LZ+" ...).
pub fn pad_name(cx: &Ctx, b: Option<bindings::PadInput>) -> String {
    use GamepadButton::*;
    let b = match b {
        None => return "-?-".into(),
        Some(bindings::PadInput::Axis(a, p)) => {
            let name = match a {
                GamepadAxis::LeftStickX => "LX",
                GamepadAxis::LeftStickY => "LY",
                GamepadAxis::RightStickX => "RX",
                GamepadAxis::RightStickY => "RY",
                GamepadAxis::LeftZ => "LZ",
                GamepadAxis::RightZ => "RZ",
                _ => "Axis",
            };
            return format!("{name}{}", if p { '+' } else { '-' });
        }
        Some(bindings::PadInput::Button(b)) => b,
    };
    let phrase = match b {
        Select => 1500,
        Start => 1501,
        LeftTrigger => 1502,
        RightTrigger => 1503,
        LeftTrigger2 => 1504,
        RightTrigger2 => 1505,
        South => return "A".into(),
        East => return "B".into(),
        West => return "X".into(),
        North => return "Y".into(),
        LeftThumb => return "L3".into(),
        RightThumb => return "R3".into(),
        DPadUp => return "Up".into(),
        DPadDown => return "Down".into(),
        DPadLeft => return "Left".into(),
        DPadRight => return "Right".into(),
        b => return bindings::pad_id(bindings::PadInput::Button(b)),
    };
    cx.g(phrase)
}

/// Control setup (`FUN_0102e75c`): one column of 11 bindings per device
/// table (`bindings.rs`), scrolled sideways, then Done and Cancel.
///
/// The selection is `column*11 + action`, then Done (`0x79` in the
/// original's 11 columns) and Cancel (`0x7a`, the start). Up from an
/// action's first row goes to Cancel, down from its last row to Done; up
/// from Done goes to the first column's last row; down from Cancel wraps
/// to the first row. Left/right change the column (wrapping). Fire on an
/// action shows the redefine prompt (2468, 2119) and waits for a key
/// (`FUN_01040790`): Esc cancels, 0 disables, the digits, `-`, `=` and
/// F1 are refused (Mac; the Amiga refused `P` instead); pad columns take
/// a gamepad button. Edits go to a copy that Done stores (and saves);
/// Cancel drops them. The Mac (`DoControlSetup` @ 100a1f80) has 5 columns
/// (keyboard, devices 1..4) and spins the `controlsetup` model behind
/// the screen.
pub struct ControlSetup {
    sel: usize,
    controls: Controls,
    /// Column scroll in virtual pixels, eased `p = (7p + target) >> 3`.
    scroll: i32,
    /// The redefine prompt is up.
    waiting: bool,
    /// Flash phase 0..3 (`uVar18`).
    flash: u32,
    /// The Mac's spinning `controlsetup` model.
    obj: Bobbing,
}

const ROWS: usize = 11;
const DONE: usize = COLUMNS * ROWS;
const CANCEL: usize = DONE + 1;

impl ControlSetup {
    pub fn new(cx: &Ctx) -> ControlSetup {
        ControlSetup {
            sel: CANCEL,
            controls: cx.settings.controls.clone(),
            scroll: -1000,
            waiting: false,
            flash: 0,
            obj: Bobbing::new("options/controlsetup.obj"),
        }
    }

    /// `--frontend-screen controls-redefine`: the prompt for the
    /// single-player keyboard's "accelerate".
    pub fn redefining(cx: &Ctx) -> ControlSetup {
        ControlSetup {
            sel: 0,
            waiting: true,
            scroll: 0,
            ..ControlSetup::new(cx)
        }
    }

    fn column(&self) -> usize {
        self.sel / ROWS
    }

    /// Read the new binding for the selected action.
    fn redefine(&mut self, cx: &mut Ctx, input: &Input) {
        let (col, action) = (self.column(), self.sel % ROWS);
        let keyboard = Controls::is_keyboard(col);
        for &k in &input.keys {
            if k == KeyCode::Escape {
                cx.sound(Sfx::Select);
                self.waiting = false;
                return;
            }
            if k == KeyCode::Digit0 {
                cx.sound(Sfx::Select);
                self.controls.set_key(col, action, None);
                self.controls.set_pad(col, action, None);
                self.waiting = false;
                return;
            }
            if keyboard && !bindings::refused_key(k) {
                cx.sound(Sfx::Select);
                self.controls.set_key(col, action, Some(k));
                self.waiting = false;
                return;
            }
            cx.sound(Sfx::Refuse);
        }
        if !keyboard {
            let pressed = input.pad.first().map(|&b| bindings::PadInput::Button(b));
            if let Some(b) = pressed.or_else(|| input.pad_axes.first().copied()) {
                cx.sound(Sfx::Select);
                self.controls.set_pad(col, action, Some(b));
                self.waiting = false;
            }
        }
    }
}

impl Screen for ControlSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.flash = (self.flash + 1) % 4;
        self.obj.tick();
        let target = if self.sel >= DONE { 0 } else { -100 * self.column() as i32 };
        self.scroll = (7 * self.scroll + target) >> 3;
        if self.waiting {
            self.redefine(cx, input);
            return Step::Stay;
        }
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        if input.up {
            cx.sound(Sfx::Select);
            self.sel = match self.sel {
                DONE => ROWS - 1,
                CANCEL => DONE,
                s if s % ROWS == 0 => CANCEL,
                s => s - 1,
            };
        } else if input.down {
            cx.sound(Sfx::Select);
            self.sel = match self.sel {
                DONE => CANCEL,
                CANCEL => 0,
                s if s % ROWS == ROWS - 1 => DONE,
                s => s + 1,
            };
        } else if (input.left || input.right) && self.sel < DONE {
            cx.sound(Sfx::Select);
            let d = if input.right { ROWS } else { DONE - ROWS };
            self.sel = (self.sel + d) % DONE;
        } else if input.fire {
            cx.sound(Sfx::Select);
            match self.sel {
                DONE => {
                    cx.settings.controls = self.controls.clone();
                    cx.settings.save();
                    return Step::Pop(Outcome::Done);
                }
                CANCEL => return Step::Pop(Outcome::Cancel),
                _ => self.waiting = true,
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        if self.waiting {
            ui.wrapped(&cx.g(2468), ui.hy(80), 2);
            ui.centred(&cx.g(2119), ui.hy(110) - 5, 2);
            return;
        }
        ui.title(&cx.g(2469));
        let lit = self.flash < 2;
        // Action names right-aligned at x = 130.
        let label_x = ui.vx(130);
        for i in 0..ROWS {
            ui.right(&cx.g(2457 + i as u32), label_x, ui.hy(50 + 10 * i as i32), 2);
        }
        // The columns, in a view from x = 135 to the right edge with the
        // selected one at 90 in it.
        let x0 = ui.vx(0x87);
        for col in 0..COLUMNS {
            let centre = x0 + ui.vx(90 + self.scroll + 100 * col as i32);
            let put = |ui: &mut Ui, s: &str, y: i32, colour: u8| {
                let w = 7 * text_len(s);
                let x = centre - w / 2;
                if x >= x0 && x + w <= ui.w {
                    ui.text(s, x, y, colour);
                }
            };
            // Mac titles (`DoControlSetup`): "Single player" over Keyboard
            // and device 1, "Multiplayer" over devices 2..4; then the key
            // sets.
            let (head, device) = match col {
                0 => (2455, cx.g(2167)),
                1 => (2455, format!("{} 1", cx.g(2166))),
                c if c < bindings::KEY_SET_COLUMN => (2456, format!("{} {c}", cx.g(2166))),
                c => (2456, format!("{} {}", cx.g(2167), c - bindings::KEY_SET_COLUMN + 1)),
            };
            put(ui, &cx.g(head), ui.hy(30), 2);
            put(ui, &device, H / 5, 2);
            for i in 0..ROWS {
                let name = match self.controls.column(col) {
                    Table::Keys(t) => key_name(cx, t[i]),
                    Table::Pad(t) => pad_name(cx, t[i]),
                };
                let colour = if self.sel == col * ROWS + i && lit { 2 } else { 1 };
                put(ui, &name, ui.hy(50 + 10 * i as i32), colour);
            }
        }
        for (item, phrase, y) in [(DONE, 2148, 170), (CANCEL, 2192, 180)] {
            let colour = if self.sel == item && lit { 2 } else { 1 };
            ui.centred(&cx.g(phrase), ui.hy(y), colour);
        }
    }

    fn model(&self) -> Option<ModelView> {
        Some(self.obj.view)
    }
}

/// CD audio setup, "Music setup" on the Mac (`DoCDAudioSetup` @ 100a1500,
/// OS4 `FUN_0102dd30`): no CD drive here, so device and unit stay unknown
/// (the Mac's `CD_Init` ignores them too); only the music mode is stored.
/// The tracks are ripped files (`crate::music`); the mode shows Disabled,
/// dimmed and skipped, while none are found, as both originals do when the
/// CD is missing. Labels are drawn from x = 30/320 (the Mac's labels are
/// indented so the colons line up), Done centred; the Mac spins the `cd`
/// model behind the screen and starts on Done.
///
/// Added row: "One-shot cut" (`Variants::oneshot_cut`, the Mac's cut-off of
/// one-shot sounds; docs/re/mac_version.md decision 2).
pub struct CdSetup {
    sel: usize,
    obj: Bobbing,
}

const CD_MODE: usize = 2;
const CD_CUT: usize = 3;
const CD_DONE: usize = 4;

impl CdSetup {
    pub fn new() -> CdSetup {
        CdSetup {
            sel: CD_DONE,
            obj: Bobbing::new("options/cd.obj"),
        }
    }

    fn disabled() -> bool {
        crate::music::effective_mode(0) == crate::music::mode::DISABLED
    }
}

impl Screen for CdSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.obj.tick();
        if input.up || input.down {
            // Up/down skip the mode line while it is Disabled.
            pick(&mut self.sel, CD_DONE + 1, cx, input);
            if self.sel == CD_MODE && Self::disabled() {
                self.sel = if input.down { CD_CUT } else { 1 };
            }
        }
        if input.back || (input.fire && self.sel == CD_DONE) {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return Step::Pop(Outcome::Done);
        }
        if input.fire {
            match self.sel {
                CD_MODE => {
                    cx.settings.cd_mode = crate::music::next_mode(cx.settings.cd_mode);
                    cx.sound(Sfx::Select);
                }
                CD_CUT => {
                    let v = &mut cx.settings.variants;
                    v.oneshot_cut = !v.oneshot_cut;
                    cx.sound(Sfx::Select);
                }
                _ => cx.sound(Sfx::Refuse),
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2131));
        let on_off = cx.g(if cx.settings.variants.oneshot_cut { 2081 } else { 2080 });
        let x = ui.vx(30);
        let lines = [
            (cx.g(2138), ui.hy(50)),
            (cx.g(2139), ui.hy(61)),
            (cx.g(2142 + crate::music::effective_mode(cx.settings.cd_mode) as u32), ui.hy(80)),
            // No phrase: the colon in line with the Mac labels'.
            (format!("{:>12} : {on_off}", "One-shot cut"), ui.hy(91)),
        ];
        // Colour 1, the selected line flashing 2 (Mac `DisplayTextAlpha`,
        // dimmed Disabled with `DisplayTextAlpha3`; OS4 `FUN_0100bb94` /
        // `FUN_0100cc78`).
        for (i, (s, y)) in lines.iter().enumerate() {
            if i == CD_MODE && Self::disabled() {
                ui.text_dim(s, x, *y, 1);
            } else {
                let c = if i == self.sel { ui.flash() } else { 1 };
                ui.text(s, x, *y, c);
            }
        }
        // No tracks: say where they go (`music::NO_MUSIC_HINT`).
        if !crate::music::available() {
            for (i, l) in crate::music::NO_MUSIC_HINT.iter().enumerate() {
                ui.centred(l, ui.hy(112) + 11 * i as i32, 2);
            }
        }
        let c = if self.sel == CD_DONE { ui.flash() } else { 1 };
        ui.centred(&cx.g(2148), ui.hy(170), c);
    }

    fn model(&self) -> Option<ModelView> {
        Some(self.obj.view)
    }
}
