//! Control bindings: the original's key tables `DAT_012a63fc[11][11]`
//! (config field 14; docs/re/frontend_menus.md §6), redefined in the
//! control setup (`FUN_0102e75c`, `misc::ControlSetup`).
//!
//! Each table holds one binding per action, in phrase order 2457..2467
//! ([`ACTIONS`]); `None` = disabled (the original's 0, "-?-"). The Amiga
//! has 11 tables: single-player keyboard / PSX / CD32 pad, then a PSX and
//! a CD32 table per multiplayer player. The Mac (`DoControlSetup` @
//! 100a1f80, `UpdateInputStatus` @ 100f8490) drops PSX and CD32 and has 5:
//! the keyboard and one table per input device 1..4, where device 1 serves
//! single player too and a multiplayer player uses the table of the device
//! they picked. Here the Mac's 5 columns come first, then the multiplayer
//! keyboard key sets (fixed on both originals, redefinable here):
//!
//! | Column | Table |
//! |---|---|
//! | 0 | single player, keyboard ([`Controls::keys`], read by `sim::people::read_keyboard`) |
//! | 1 | single player, gamepad 1 ([`Controls::pads`]`[0]`, the first connected pad; also multiplayer "Joystick 1") |
//! | 2..4 | multiplayer gamepad 2..4 ([`Controls::pads`]`[1..4]`) |
//! | 5..8 | multiplayer keyboard key set 1..4 ([`Controls::mp_keys`]) |
//!
//! Single player reads the keyboard and gamepad 1 together, as the Mac
//! polls device 1 every frame next to the keyboard.
//!
//! Fixed, as in the original: digits 0..7 pick a weapon, Esc pauses. A
//! gamepad's left stick and d-pad always steer and accelerate/reverse
//! (the joystick directions); its table adds buttons. In single player,
//! Ctrl, Shift and Enter bindings accept either key of the pair
//! (left/right, main/numpad), so the defaults behave exactly as the
//! hard-wired keys did before.

use bevy::prelude::*;

/// Actions in phrase order 2457..2467.
pub const ACTIONS: [&str; 11] = [
    "accelerate",
    "reverse",
    "left",
    "right",
    "shoot",
    "shoot_alt",
    "in_out",
    "brake",
    "next_weapon",
    "prev_weapon",
    "repeat_message",
];

pub const ACCELERATE: usize = 0;
pub const REVERSE: usize = 1;
pub const LEFT: usize = 2;
pub const RIGHT: usize = 3;
pub const SHOOT: usize = 4;
pub const SHOOT_ALT: usize = 5;
pub const IN_OUT: usize = 6;
pub const BRAKE: usize = 7;
pub const NEXT_WEAPON: usize = 8;
pub const PREV_WEAPON: usize = 9;
pub const REPEAT_MESSAGE: usize = 10;

pub type KeyTable = [Option<KeyCode>; 11];
pub type PadTable = [Option<GamepadButton>; 11];

/// Columns of the control setup screen.
pub const COLUMNS: usize = 9;

/// First multiplayer key-set column.
pub const KEY_SET_COLUMN: usize = 5;

/// Single-player keyboard: the keys `read_keyboard` has always read.
const SP_KEYS: KeyTable = [
    Some(KeyCode::ArrowUp),
    Some(KeyCode::ArrowDown),
    Some(KeyCode::ArrowLeft),
    Some(KeyCode::ArrowRight),
    Some(KeyCode::Space),
    Some(KeyCode::ControlLeft),
    Some(KeyCode::Enter),
    Some(KeyCode::ShiftLeft),
    None,
    None,
    // Repeat message: M, the original's default (raw key 0x37).
    Some(KeyCode::KeyM),
];

/// A gamepad: up/down/left/right also come from the stick and d-pad.
const PAD: PadTable = [
    Some(GamepadButton::RightTrigger2),
    Some(GamepadButton::LeftTrigger2),
    Some(GamepadButton::DPadLeft),
    Some(GamepadButton::DPadRight),
    Some(GamepadButton::South),
    None,
    Some(GamepadButton::North),
    Some(GamepadButton::East),
    Some(GamepadButton::RightTrigger),
    Some(GamepadButton::LeftTrigger),
    None,
];

/// Multiplayer keyboard key sets, so that up to four players can share a
/// keyboard.
const MP_KEYS: [KeyTable; 4] = {
    use KeyCode::*;
    [
        [
            Some(ArrowUp),
            Some(ArrowDown),
            Some(ArrowLeft),
            Some(ArrowRight),
            Some(ControlRight),
            None,
            Some(Enter),
            Some(ShiftRight),
            Some(PageDown),
            Some(Delete),
            None,
        ],
        [
            Some(KeyW),
            Some(KeyS),
            Some(KeyA),
            Some(KeyD),
            Some(ControlLeft),
            None,
            Some(Tab),
            Some(ShiftLeft),
            Some(KeyE),
            Some(KeyQ),
            None,
        ],
        [
            Some(Numpad8),
            Some(Numpad5),
            Some(Numpad4),
            Some(Numpad6),
            Some(Numpad0),
            None,
            Some(NumpadEnter),
            Some(NumpadDecimal),
            Some(NumpadAdd),
            Some(NumpadSubtract),
            None,
        ],
        [
            Some(KeyI),
            Some(KeyK),
            Some(KeyJ),
            Some(KeyL),
            Some(Space),
            None,
            Some(KeyH),
            Some(KeyN),
            Some(KeyO),
            Some(KeyU),
            None,
        ],
    ]
};

/// Every binding table.
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    pub keys: KeyTable,
    /// Gamepad n's buttons (the Mac's device tables 1..4).
    pub pads: [PadTable; 4],
    pub mp_keys: [KeyTable; 4],
}

impl Default for Controls {
    fn default() -> Self {
        Controls {
            keys: SP_KEYS,
            pads: [PAD; 4],
            mp_keys: MP_KEYS,
        }
    }
}

/// One column's table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Table {
    Keys(KeyTable),
    Pad(PadTable),
}

impl Controls {
    pub fn column(&self, c: usize) -> Table {
        match c {
            0 => Table::Keys(self.keys),
            1..KEY_SET_COLUMN => Table::Pad(self.pads[c - 1]),
            _ => Table::Keys(self.mp_keys[(c - KEY_SET_COLUMN).min(3)]),
        }
    }

    /// The control-setup column of gamepad `n` (0..3).
    pub fn pad_column(n: usize) -> usize {
        1 + n.min(3)
    }

    /// The control-setup column of multiplayer key set `k` (0..3).
    pub fn key_set_column(k: usize) -> usize {
        KEY_SET_COLUMN + k.min(3)
    }

    /// Column `c` is a keyboard table.
    pub fn is_keyboard(c: usize) -> bool {
        c == 0 || c >= KEY_SET_COLUMN
    }

    pub fn set_key(&mut self, c: usize, action: usize, key: Option<KeyCode>) {
        match c {
            0 => self.keys[action] = key,
            KEY_SET_COLUMN..COLUMNS => self.mp_keys[c - KEY_SET_COLUMN][action] = key,
            _ => {}
        }
    }

    pub fn set_pad(&mut self, c: usize, action: usize, b: Option<GamepadButton>) {
        if (1..KEY_SET_COLUMN).contains(&c) {
            self.pads[c - 1][action] = b;
        }
    }

    /// Settings-file lines (`key.<action>`, `pad.<action>` = gamepad 1,
    /// `pad<n>.<action>` = gamepad n, `mp<n>.key.<action>` = key set n).
    pub fn write(&self, t: &mut String) {
        let mut table = |prefix: &str, names: [String; 11]| {
            for (a, n) in ACTIONS.iter().zip(names) {
                t.push_str(&format!("{prefix}{a} = {n}\n"));
            }
        };
        table("key.", self.keys.map(|k| k.map(key_id).unwrap_or_default()));
        for (n, pad) in self.pads.iter().enumerate() {
            let prefix = if n == 0 { "pad.".to_string() } else { format!("pad{}.", n + 1) };
            table(&prefix, pad.map(|b| b.map(pad_id).unwrap_or_default()));
        }
        for p in 0..4 {
            table(&format!("mp{}.key.", p + 1), self.mp_keys[p].map(|k| k.map(key_id).unwrap_or_default()));
        }
    }

    /// One settings-file line; false if the key is not a binding.
    pub fn read(&mut self, k: &str, v: &str) -> bool {
        let (player, rest) = match k.strip_prefix("mp").and_then(|r| r.split_once('.')) {
            Some((n, rest)) => match n.parse::<usize>() {
                Ok(n @ 1..=4) => (Some(n - 1), rest),
                _ => return false,
            },
            None => (None, k),
        };
        let Some((kind, action)) = rest.split_once('.') else {
            return false;
        };
        let Some(a) = ACTIONS.iter().position(|x| *x == action) else {
            return false;
        };
        let pad = match kind.strip_prefix("pad") {
            Some("") => Some(0),
            Some(n) => match n.parse::<usize>() {
                Ok(n @ 2..=4) => Some(n - 1),
                _ => return false,
            },
            None => None,
        };
        match (kind, player, pad) {
            ("key", None, _) => self.keys[a] = parse_key(v),
            ("key", Some(p), _) => self.mp_keys[p][a] = parse_key(v),
            (_, None, Some(n)) => self.pads[n][a] = parse_pad(v),
            // Older files: per-player multiplayer pads, now per gamepad.
            (_, Some(_), Some(0)) => {}
            _ => return false,
        }
        true
    }
}

/// Keys pressed for a single-player binding: Ctrl, Shift and Enter count
/// on both sides (multiplayer key sets share the keyboard, so they use
/// the exact key).
pub fn key_variants(k: KeyCode) -> [KeyCode; 2] {
    use KeyCode::*;
    match k {
        ControlLeft | ControlRight => [ControlLeft, ControlRight],
        ShiftLeft | ShiftRight => [ShiftLeft, ShiftRight],
        Enter | NumpadEnter => [Enter, NumpadEnter],
        k => [k, k],
    }
}

/// A binding is held; `pair` = accept both keys of a Ctrl/Shift/Enter pair.
pub fn key_held(keys: &ButtonInput<KeyCode>, k: Option<KeyCode>, pair: bool) -> bool {
    k.is_some_and(|k| if pair { keys.any_pressed(key_variants(k)) } else { keys.pressed(k) })
}

pub fn key_just(keys: &ButtonInput<KeyCode>, k: Option<KeyCode>, pair: bool) -> bool {
    k.is_some_and(|k| if pair { keys.any_just_pressed(key_variants(k)) } else { keys.just_pressed(k) })
}

pub fn pad_held(pad: &Gamepad, b: Option<GamepadButton>) -> bool {
    b.is_some_and(|b| pad.pressed(b))
}

pub fn pad_just(pad: &Gamepad, b: Option<GamepadButton>) -> bool {
    b.is_some_and(|b| pad.just_pressed(b))
}

/// Keys the redefine prompt refuses: the digits pick weapons, `-` and `=`
/// are game keys too, and F1 is kept for "Press F1 to continue" (Mac
/// `DoControlSetup` @ 100a1f80 refuses key code 0x7A and allows `P`, which
/// the Amiga's `FUN_01040790` refused). F12 opens the cheat menu (port
/// addition, `crate::cheats`). 0 disables (handled by the caller), Esc
/// cancels.
pub fn refused_key(k: KeyCode) -> bool {
    use KeyCode::*;
    matches!(
        k,
        Digit1 | Digit2 | Digit3 | Digit4 | Digit5 | Digit6 | Digit7 | Digit8 | Digit9 | Minus | Equal | F1 | F12
    )
}

/// The fixed weapon key for HUD slot `d` (0..7): Digit0..Digit7.
pub fn digit_key(d: u8) -> KeyCode {
    use KeyCode::*;
    [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7][d.min(7) as usize]
}

/// Keys that can be bound, by settings-file name.
const KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP,
        KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ, Digit0, Digit1, Digit2, Digit3,
        Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
        Space, Enter, Tab, Backspace, Delete, Insert, Home, End, PageUp, PageDown, Escape, ShiftLeft,
        ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, SuperLeft, SuperRight, CapsLock,
        Minus, Equal, BracketLeft, BracketRight, Backslash, Semicolon, Quote, Backquote, Comma,
        Period, Slash, IntlBackslash, ContextMenu, Numpad0, Numpad1, Numpad2, Numpad3, Numpad4,
        Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadSubtract, NumpadMultiply,
        NumpadDivide, NumpadDecimal, NumpadEnter, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    ]
};

const PADS: &[GamepadButton] = {
    use GamepadButton::*;
    &[
        South, East, North, West, C, Z, LeftTrigger, LeftTrigger2, RightTrigger, RightTrigger2,
        Select, Start, Mode, LeftThumb, RightThumb, DPadUp, DPadDown, DPadLeft, DPadRight,
    ]
};

/// Settings-file name of a key (Bevy's `KeyCode` variant name).
pub fn key_id(k: KeyCode) -> String {
    format!("{k:?}")
}

pub fn pad_id(b: GamepadButton) -> String {
    format!("{b:?}")
}

pub fn parse_key(s: &str) -> Option<KeyCode> {
    KEYS.iter().copied().find(|k| key_id(*k) == s)
}

pub fn parse_pad(s: &str) -> Option<GamepadButton> {
    PADS.iter().copied().find(|b| pad_id(*b) == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let mut c = Controls::default();
        c.set_key(0, NEXT_WEAPON, Some(KeyCode::KeyM));
        c.set_key(6, SHOOT, None);
        c.set_pad(3, BRAKE, Some(GamepadButton::West));
        c.set_pad(1, NEXT_WEAPON, None);
        let mut t = String::new();
        c.write(&mut t);
        let mut d = Controls {
            keys: [None; 11],
            pads: [[None; 11]; 4],
            mp_keys: [[None; 11]; 4],
        };
        for line in t.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(d.read(k.trim(), v.trim()), "{line}");
        }
        assert_eq!(c, d);
        assert_eq!(d.pads[2][BRAKE], Some(GamepadButton::West));
        assert_eq!(d.mp_keys[1][SHOOT], None);
        assert!(!d.read("key.fly", "KeyF"));
        assert!(!d.read("pad5.brake", "West"));
        assert!(d.read("mp2.pad.brake", "East"), "old per-player pad lines are skipped");
        assert_eq!(c, d);
    }

    #[test]
    fn mp_key_sets_distinct() {
        let mut all: Vec<KeyCode> = MP_KEYS.iter().flatten().flatten().copied().collect();
        let n = all.len();
        all.sort_by_key(|k| key_id(*k));
        all.dedup();
        assert_eq!(all.len(), n, "a key is in two sets");
    }

    #[test]
    fn names_parse() {
        for &k in KEYS {
            assert_eq!(parse_key(&key_id(k)), Some(k));
        }
        for &b in PADS {
            assert_eq!(parse_pad(&pad_id(b)), Some(b));
        }
    }
}
