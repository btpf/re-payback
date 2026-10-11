//! Sub-screens: text entry (`FUN_01012e38`), message box (`FUN_0101dc60`),
//! load/save (`FUN_0101f1a4` / `FUN_0101f738`), credits (`FUN_010206bc`), language chooser
//! (`FUN_0101267c`), control setup (`FUN_0102e75c`) and CD audio setup
//! (`FUN_0102dd30`). The replay list is in `replays.rs`.

use super::bindings::{self, COLUMNS, Controls, Table};
use super::progress::{self, Profile, SLOTS};
use super::ui::*;
use bevy::prelude::{GamepadButton, KeyCode};
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

/// Gamepad control name in the label style of the settings and the first
/// connected pad ("A", "RT", "LS Left"; "Cross", "R2"; "B", "ZR" ...,
/// `bindings::pad_label`). The original named PSX/CD32 buttons through
/// phrases 1500–1505 (Select, Start, L1, R1, L2, R2); today's pads print
/// their own names, so the port uses those.
pub fn pad_name(cx: &Ctx, b: Option<bindings::PadInput>) -> String {
    let first = (0..4).find_map(bindings::pad_info);
    let style = cx.settings.controls.opts.labels.resolve(first.as_ref().map(|(n, v)| (n.as_str(), *v)));
    pad_name_in(style, b)
}

/// [`pad_name`] in a resolved label style; "-?-" when unbound.
pub fn pad_name_in(style: bindings::LabelStyle, b: Option<bindings::PadInput>) -> String {
    b.map_or_else(|| "-?-".into(), |b| bindings::pad_label(b, style))
}

/// The label style for gamepad number `n` (its own pad, else the first
/// connected one).
fn pad_style(c: &Controls, n: usize) -> bindings::LabelStyle {
    let info = bindings::pad_info(n).or_else(|| (0..4).find_map(bindings::pad_info));
    c.opts.labels.resolve(info.as_ref().map(|(name, v)| (name.as_str(), *v)))
}

/// Control setup (`FUN_0102e75c`): one column of 11 bindings per device
/// table (`bindings.rs`), scrolled sideways, then Done and Cancel. A
/// twelfth row, Horn, is a port addition (`bindings::HORN`).
///
/// The selection is `column*11 + action`, then Done (`0x79` in the
/// original's 11 columns) and Cancel (`0x7a`, the start). Up from an
/// action's first row goes to Cancel, down from its last row to Done; up
/// from Done goes to the first column's last row; down from Cancel wraps
/// to the first row. Left/right change the column (wrapping). Fire on an
/// action shows the redefine prompt (2468, 2119) and waits for a key
/// (`FUN_01040790`): Esc cancels, 0 disables, the digits, `-`, `=` and
/// F1 are refused (Mac; the Amiga refused `P` instead); pad columns take
/// a gamepad button, trigger or stick end. Edits go to a copy that Done
/// stores (and saves); Cancel drops them. The Mac (`DoControlSetup`
/// @ 100a1f80) has 5 columns (keyboard, devices 1..4) and spins the
/// `controlsetup` model behind the screen.
///
/// Port additions for gamepads (docs/controls.md): a "Gamepad options"
/// line between the last row and Done opens [`PAD_ITEMS`] (stick
/// deadzone, trigger threshold, button labels, left stick forward/back,
/// reset to defaults, the connected pads); a pad's Start cancels the redefine prompt (so Start
/// cannot be bound: it pauses), X / Square (West) or Backspace clears the
/// selected binding; pad bindings show the pad's own button names.
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
    /// The gamepad options page is up, on this item.
    pad_page: Option<usize>,
}

const ROWS: usize = bindings::NUM_ACTIONS;
/// Row pitch in virtual pixels: the original's 10 for its 11 rows, 9 so
/// that the twelfth (Horn) fits above "Gamepad options".
const ROW_PITCH: i32 = 9;
/// "Gamepad options" (port addition), between the last row and Done.
const PAD_OPTIONS: usize = COLUMNS * ROWS;
const DONE: usize = PAD_OPTIONS + 1;
const CANCEL: usize = DONE + 1;

/// The gamepad options page.
const PAD_ITEMS: usize = 7;
const PAD_DEADZONE: usize = 0;
const PAD_TRIGGER: usize = 1;
const PAD_LABELS: usize = 2;
const PAD_STICK_DRIVE: usize = 3;
const PAD_RESET: usize = 4;
const PAD_RESET_KEYS: usize = 5;
const PAD_BACK: usize = 6;

impl ControlSetup {
    pub fn new(cx: &Ctx) -> ControlSetup {
        ControlSetup {
            sel: CANCEL,
            controls: cx.settings.controls.clone(),
            scroll: -1000,
            waiting: false,
            flash: 0,
            obj: Bobbing::new("options/controlsetup.obj"),
            pad_page: None,
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

    /// `--frontend-screen controls-pad`: gamepad 1's column, on "shoot";
    /// `controls-pad-options`: the gamepad options page.
    pub fn debug(cx: &Ctx, options: bool) -> ControlSetup {
        let sel = Controls::pad_column(0) * ROWS + bindings::SHOOT;
        ControlSetup {
            sel: if options { PAD_OPTIONS } else { sel },
            scroll: if options { 0 } else { -100 * Controls::pad_column(0) as i32 },
            pad_page: options.then_some(PAD_DEADZONE),
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
        if input.pad.contains(&GamepadButton::Start) {
            cx.sound(Sfx::Select);
            self.waiting = false;
            return;
        }
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

    /// The gamepad options page.
    fn pad_tick(&mut self, item: usize, cx: &mut Ctx, input: &Input) -> Option<usize> {
        if input.back {
            cx.sound(Sfx::Select);
            return None;
        }
        if input.up || input.down {
            cx.sound(Sfx::Move);
            let d = if input.down { 1 } else { PAD_ITEMS - 1 };
            return Some((item + d) % PAD_ITEMS);
        }
        let o = &mut self.controls.opts;
        let step = if input.right || (input.fire && item <= PAD_STICK_DRIVE) {
            1
        } else if input.left {
            -1
        } else {
            0
        };
        if step != 0 {
            let wrap = input.fire && !input.right;
            let adjust = |v: u8, (lo, hi): (u8, u8)| -> u8 {
                let n = v as i32 + 5 * step;
                if wrap && n > hi as i32 { lo } else { n.clamp(lo as i32, hi as i32) as u8 }
            };
            match item {
                PAD_DEADZONE => o.deadzone = adjust(o.deadzone, bindings::DEADZONE_RANGE),
                PAD_TRIGGER => o.trigger = adjust(o.trigger, bindings::TRIGGER_RANGE),
                PAD_LABELS => {
                    let all = bindings::LabelStyle::ALL;
                    let i = all.iter().position(|l| *l == o.labels).unwrap_or(0) as i32;
                    o.labels = all[(i + step).rem_euclid(all.len() as i32) as usize];
                }
                PAD_STICK_DRIVE => o.stick_drive = !o.stick_drive,
                _ => {}
            }
            if item <= PAD_STICK_DRIVE {
                cx.sound(Sfx::Move);
                return Some(item);
            }
        }
        if input.fire {
            cx.sound(Sfx::Select);
            match item {
                PAD_RESET => self.controls.reset_pads(),
                PAD_RESET_KEYS => self.controls.reset_keys(),
                PAD_BACK => return None,
                _ => {}
            }
        }
        Some(item)
    }

    fn pad_draw(&self, item: usize, ui: &mut Ui, lit: bool) {
        ui.title("Gamepad options");
        let o = &self.controls.opts;
        let lines = [
            format!("Stick deadzone: {} percent", o.deadzone),
            format!("Trigger threshold: {} percent", o.trigger),
            format!("Button labels: {}", o.labels.name()),
            format!("Left stick forward/back: {}", if o.stick_drive { "On" } else { "Off" }),
            "Reset gamepad buttons".to_string(),
            "Reset keyboard keys".to_string(),
            "Back".to_string(),
        ];
        for (i, l) in lines.iter().enumerate() {
            let colour = if i == item && lit { 2 } else { 1 };
            ui.centred(l, ui.hy(48 + 12 * i as i32), colour);
        }
        let mut y = ui.hy(136);
        for n in 0..4 {
            let name = match bindings::pad_info(n) {
                Some((name, _)) => name.chars().take(30).collect::<String>(),
                None => "-".into(),
            };
            ui.centred_dim(&format!("Gamepad {}: {name}", n + 1), y, 1);
            y += ui.hy(10);
        }
        // How to drive it with a pad, in the first pad's names.
        let s = pad_style(&self.controls, 0);
        let b = |b: GamepadButton| bindings::pad_label(bindings::PadInput::Button(b), s);
        if item == PAD_STICK_DRIVE {
            let note = if o.stick_drive {
                "Stick and d-pad up/down drive and walk"
            } else {
                "Up/down only in menus; the stick steers"
            };
            ui.centred_dim(note, ui.hy(176), 1);
        }
        let hint = format!("{}: change   {}: back", b(GamepadButton::South), b(GamepadButton::East));
        ui.centred_dim(&hint, ui.hy(184), 1);
    }
}

impl Screen for ControlSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.flash = (self.flash + 1) % 4;
        self.obj.tick();
        let target = if self.sel >= PAD_OPTIONS { 0 } else { -100 * self.column() as i32 };
        self.scroll = (7 * self.scroll + target) >> 3;
        if self.waiting {
            self.redefine(cx, input);
            return Step::Stay;
        }
        if let Some(item) = self.pad_page {
            self.pad_page = self.pad_tick(item, cx, input);
            return Step::Stay;
        }
        if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        }
        // Clear the selected binding (port addition: a pad's X / Square,
        // or Backspace).
        let clear = input.erase || input.pad.contains(&GamepadButton::West);
        if input.up {
            cx.sound(Sfx::Select);
            self.sel = match self.sel {
                PAD_OPTIONS => ROWS - 1,
                DONE => PAD_OPTIONS,
                CANCEL => DONE,
                s if s % ROWS == 0 => CANCEL,
                s => s - 1,
            };
        } else if input.down {
            cx.sound(Sfx::Select);
            self.sel = match self.sel {
                PAD_OPTIONS => DONE,
                DONE => CANCEL,
                CANCEL => 0,
                s if s % ROWS == ROWS - 1 => PAD_OPTIONS,
                s => s + 1,
            };
        } else if (input.left || input.right) && self.sel < PAD_OPTIONS {
            cx.sound(Sfx::Select);
            let d = if input.right { ROWS } else { PAD_OPTIONS - ROWS };
            self.sel = (self.sel + d) % PAD_OPTIONS;
        } else if clear && self.sel < PAD_OPTIONS {
            cx.sound(Sfx::Select);
            let (col, action) = (self.column(), self.sel % ROWS);
            self.controls.set_key(col, action, None);
            self.controls.set_pad(col, action, None);
        } else if input.fire {
            cx.sound(Sfx::Select);
            match self.sel {
                PAD_OPTIONS => self.pad_page = Some(PAD_DEADZONE),
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
        let lit = self.flash < 2;
        if self.waiting {
            ui.wrapped(&cx.g(2468), ui.hy(80), 2);
            ui.centred(&cx.g(2119), ui.hy(110) - 5, 2);
            if !Controls::is_keyboard(self.column()) {
                ui.centred("(Start cancels, 0 clears.)", ui.hy(122) - 5, 2);
            }
            return;
        }
        if let Some(item) = self.pad_page {
            self.pad_draw(item, ui, lit);
            return;
        }
        ui.title(&cx.g(2469));
        // Action names right-aligned at x = 130.
        let label_x = ui.vx(130);
        for i in 0..ROWS {
            let label = bindings::action_phrase(i).map_or_else(|| "Horn".to_string(), |p| cx.g(p));
            ui.right(&label, label_x, ui.hy(50 + ROW_PITCH * i as i32), 2);
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
                    Table::Pad(t) => pad_name_in(pad_style(&self.controls, col - 1), t[i]),
                };
                let colour = if self.sel == col * ROWS + i && lit { 2 } else { 1 };
                put(ui, &name, ui.hy(50 + ROW_PITCH * i as i32), colour);
            }
        }
        let items = [(PAD_OPTIONS, "Gamepad options".to_string(), 160), (DONE, cx.g(2148), 170), (CANCEL, cx.g(2192), 180)];
        for (item, text, y) in items {
            let colour = if self.sel == item && lit { 2 } else { 1 };
            ui.centred(&text, ui.hy(y), colour);
        }
        // On the Horn row: what it does (no phrase; port addition), in
        // place of the pad hint.
        if self.sel < PAD_OPTIONS && self.sel % ROWS == bindings::HORN {
            ui.centred_dim("Horn: works in vehicles only", ui.hy(191), 1);
        } else if cx.pads > 0 {
            // With a pad connected: how to drive the screen with it.
            let s = pad_style(&self.controls, 0);
            let b = |b: GamepadButton| bindings::pad_label(bindings::PadInput::Button(b), s);
            let hint = format!(
                "{}: change   {}: clear   {}: back",
                b(GamepadButton::South),
                b(GamepadButton::West),
                b(GamepadButton::East)
            );
            ui.centred_dim(&hint, ui.hy(191), 1);
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
/// Added rows: "One-shot cut" (`Variants::oneshot_cut`, the Mac's cut-off of
/// one-shot sounds; docs/re/mac_version.md decision 2) and "Music volume"
/// (`Settings::music_volume`, 0..100 in tens; left/right step it, fire
/// steps up and wraps 100 to 0 like the pause menu's volume).
pub struct CdSetup {
    sel: usize,
    obj: Bobbing,
}

const CD_MODE: usize = 2;
const CD_CUT: usize = 3;
const CD_VOL: usize = 4;
const CD_DONE: usize = 5;

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
        if self.sel == CD_VOL && (input.left || input.right) {
            let v = &mut cx.settings.music_volume;
            *v = if input.right { (*v + 10).min(100) } else { v.saturating_sub(10) };
            cx.sound(Sfx::Move);
            return Step::Stay;
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
                CD_VOL => {
                    let v = &mut cx.settings.music_volume;
                    *v = if *v >= 100 { 0 } else { *v + 10 };
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
        let music = cx.settings.music_volume;
        let x = ui.vx(30);
        let lines = [
            (cx.g(2138), ui.hy(50)),
            (cx.g(2139), ui.hy(61)),
            (cx.g(2142 + crate::music::effective_mode(cx.settings.cd_mode) as u32), ui.hy(80)),
            // No phrase: the colon in line with the Mac labels'.
            (format!("{:>12} : {on_off}", "One-shot cut"), ui.hy(91)),
            (format!("{:>12} : {music}", "Music volume"), ui.hy(102)),
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
                ui.centred(l, ui.hy(120) + 11 * i as i32, 2);
            }
        }
        let c = if self.sel == CD_DONE { ui.flash() } else { 1 };
        ui.centred(&cx.g(2148), ui.hy(170), c);
    }

    fn model(&self) -> Option<ModelView> {
        Some(self.obj.view)
    }
}

#[cfg(test)]
mod tests {
    use super::super::progress::{self, Settings, testing};
    use super::*;
    use bevy::prelude::GamepadAxis;
    use bindings::PadInput;

    fn pad(b: GamepadButton) -> Input {
        let mut i = Input { pad: vec![b], ..Default::default() };
        // As `bindings::pad_menu_input` maps them.
        match b {
            GamepadButton::South => (i.fire, i.enter) = (true, true),
            GamepadButton::Start => (i.fire, i.enter, i.start) = (true, true, true),
            GamepadButton::East => i.back = true,
            GamepadButton::DPadUp => i.up = true,
            GamepadButton::DPadDown => i.down = true,
            GamepadButton::DPadLeft => i.left = true,
            GamepadButton::DPadRight => i.right = true,
            _ => {}
        }
        i
    }

    /// Drive the control setup with a pad only: rebind, clear, cancel a
    /// prompt with Start, change the gamepad options, Done; the saved
    /// Payback.cfg loads back the same controls.
    #[test]
    fn control_setup_by_pad() {
        use GamepadButton::*;
        let dir = testing::scratch("pad_setup");
        progress::with_saves_dir(&dir, || {
            let mut env = testing::Env::new();
            let mut cx = env.cx();
            let mut s = ControlSetup::new(&cx);
            let press = |s: &mut ControlSetup, cx: &mut Ctx, b| s.tick(cx, &pad(b));
            // Cancel -> first row, right to gamepad 1's column, down to shoot.
            press(&mut s, &mut cx, DPadDown);
            press(&mut s, &mut cx, DPadRight);
            for _ in 0..bindings::SHOOT {
                press(&mut s, &mut cx, DPadDown);
            }
            assert_eq!(s.sel, Controls::pad_column(0) * ROWS + bindings::SHOOT);
            // A opens the prompt; RB binds.
            press(&mut s, &mut cx, South);
            assert!(s.waiting);
            press(&mut s, &mut cx, RightTrigger);
            assert!(!s.waiting);
            assert_eq!(s.controls.pads[0][bindings::SHOOT], Some(PadInput::Button(RightTrigger)));
            // A stick end binds too; Start cancels the prompt.
            press(&mut s, &mut cx, South);
            s.tick(&mut cx, &Input { pad_axes: vec![PadInput::Axis(GamepadAxis::RightStickX, true)], ..Default::default() });
            assert_eq!(s.controls.pads[0][bindings::SHOOT], Some(PadInput::Axis(GamepadAxis::RightStickX, true)));
            press(&mut s, &mut cx, South);
            press(&mut s, &mut cx, Start);
            assert!(!s.waiting);
            assert_eq!(s.controls.pads[0][bindings::SHOOT], Some(PadInput::Axis(GamepadAxis::RightStickX, true)));
            // X clears the next row.
            press(&mut s, &mut cx, DPadDown);
            press(&mut s, &mut cx, West);
            assert_eq!(s.controls.pads[0][bindings::SHOOT_ALT], None);
            // Down to "Gamepad options": deadzone -5, trigger +5, labels.
            while s.sel != PAD_OPTIONS {
                press(&mut s, &mut cx, DPadDown);
            }
            press(&mut s, &mut cx, South);
            assert_eq!(s.pad_page, Some(PAD_DEADZONE));
            press(&mut s, &mut cx, DPadLeft);
            press(&mut s, &mut cx, DPadDown);
            press(&mut s, &mut cx, DPadRight);
            press(&mut s, &mut cx, DPadDown);
            press(&mut s, &mut cx, DPadRight);
            press(&mut s, &mut cx, East);
            assert_eq!(s.pad_page, None);
            let o = s.controls.opts;
            assert_eq!((o.deadzone, o.trigger, o.labels), (45, 35, bindings::LabelStyle::Xbox));
            // B on the bindings page would cancel; Done keeps and saves.
            press(&mut s, &mut cx, DPadDown);
            assert_eq!(s.sel, DONE);
            assert!(matches!(press(&mut s, &mut cx, South), Step::Pop(Outcome::Done)));
            let saved = cx.settings.controls.clone();
            assert_eq!(saved.pads[0][bindings::SHOOT], Some(PadInput::Axis(GamepadAxis::RightStickX, true)));
            assert_eq!(Settings::load().controls, saved);
            // Reset gamepad buttons from the options page.
            let mut s = ControlSetup::debug(&cx, true);
            for _ in 0..PAD_RESET {
                press(&mut s, &mut cx, DPadDown);
            }
            press(&mut s, &mut cx, South);
            assert_eq!(s.controls.pads, Controls::default().pads);
            assert_eq!(s.controls.opts, saved.opts, "options kept");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
