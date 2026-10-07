//! Sub-screens: text entry (`FUN_01012e38`), message box (`FUN_0101dc60`),
//! load/save (`FUN_0101f1a4` / `FUN_0101f738`), credits (`FUN_010206bc`), language chooser
//! (`FUN_0101267c`), control setup (`FUN_0102e75c`) and CD audio setup
//! (`FUN_0102dd30`). The replay list is in `replays.rs`.

use super::progress::{self, Profile, SLOTS};
use super::ui::*;
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

/// The 14 slot lines at y = 0x20 + 14i and "Cancel" at 0xeb.
fn draw_slots(ui: &mut Ui, cx: &Ctx, names: &[Option<String>], sel: usize) {
    for i in 0..=SLOTS {
        let (s, y) = match names.get(i) {
            Some(Some(n)) => (n.clone(), 0x20 + 14 * i as i32),
            Some(None) => (format!("Slot {}", i + 1), 0x20 + 14 * i as i32),
            None => (cx.g(2114), 0xeb),
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
        draw_slots(ui, cx, &self.names, self.sel);
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
        // Empty slot: "<name> - C<city>, L<level>" of the first level
        // without a cup.
        let i = cx.profile.first_open();
        let default = self.names[self.sel]
            .clone()
            .unwrap_or_else(|| format!("{} - C{}, L{}", cx.profile.name, i / 2 + 1, i % 2 + 1));
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
        draw_slots(ui, cx, &self.names, self.sel);
    }
}

/// Scrolling credits: one phrase per line from `first` until `!`.
pub struct Credits {
    lines: Vec<String>,
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
            speed,
            skippable,
            stop_y,
            hold,
            y: H,
            loaded: false,
            first,
        }
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

/// Key name for a binding (phrases 1520.. for the special keys).
fn key_name(cx: &Ctx, key: &str) -> String {
    let phrase = match key {
        "" => return "-".into(),
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
        k => {
            let short = k.strip_prefix("Arrow").or(k.strip_prefix("Key")).or(k.strip_prefix("Digit"));
            return short.unwrap_or(k).to_string();
        }
    };
    cx.g(phrase)
}

/// Control setup: shows the single-player keyboard bindings (redefining
/// keys is not implemented).
pub struct ControlSetup;

impl ControlSetup {
    pub fn new() -> ControlSetup {
        ControlSetup
    }
}

impl Screen for ControlSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if input.fire || input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Done);
        }
        if input.left || input.right || input.up || input.down {
            cx.sound(Sfx::Refuse);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2469));
        ui.centred(&format!("{} - {}", cx.g(2455), cx.g(2167)), ui.hy(30), 1);
        let (l, r) = (ui.vx(40), ui.vx(280));
        for i in 0..11 {
            let y = ui.hy(50 + 10 * i);
            ui.text(&cx.g(2457 + i as u32), l, y, 2);
            ui.right(&key_name(cx, &cx.settings.keys[i as usize]), r, y, 1);
        }
        ui.centred(&cx.g(2148), ui.hy(180), ui.flash());
    }
}

/// CD audio setup: no CD audio here, so device and unit stay unknown;
/// only the playback mode is stored.
pub struct CdSetup {
    sel: usize,
}

impl CdSetup {
    pub fn new() -> CdSetup {
        CdSetup { sel: 2 }
    }
}

impl Screen for CdSetup {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        pick(&mut self.sel, 4, cx, input);
        if input.back || (input.fire && self.sel == 3) {
            cx.sound(Sfx::Select);
            cx.settings.save();
            return Step::Pop(Outcome::Done);
        }
        if input.fire {
            if self.sel == 2 {
                cx.settings.cd_mode = (cx.settings.cd_mode + 1) % 4;
                cx.sound(Sfx::Select);
            } else {
                cx.sound(Sfx::Refuse);
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        ui.title(&cx.g(2131));
        let lines = [
            (cx.g(2138), H / 4),
            (cx.g(2139), H / 4 + 12),
            (cx.g(2142 + cx.settings.cd_mode as u32), ui.hy(80)),
            (cx.g(2148), ui.hy(170)),
        ];
        for (i, (s, y)) in lines.iter().enumerate() {
            let c = if i == self.sel { ui.flash() } else { 2 };
            ui.centred(s, *y, c);
        }
    }
}
