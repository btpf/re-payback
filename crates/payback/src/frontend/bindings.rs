//! Control bindings: the original's key tables `DAT_012a63fc[11][11]`
//! (config field 14; docs/re/frontend_menus.md §6), redefined in the
//! control setup (`FUN_0102e75c`, `misc::ControlSetup`).
//!
//! Each table holds one binding per action, in phrase order 2457..2467
//! ([`ACTIONS`]); `None` = disabled (the original's 0, "-?-"). The
//! original has 11 tables: single-player keyboard / PSX / CD32 pad, then
//! a PSX and a CD32 table per multiplayer player. Here there are 10:
//!
//! | Column | Table |
//! |---|---|
//! | 0 | single player, keyboard ([`Controls::keys`], read by `sim::people::read_keyboard`) |
//! | 1 | single player, gamepad ([`Controls::pad`], the first connected pad) |
//! | 2..5 | multiplayer keyboard key set 1..4 ([`Controls::mp_keys`]) |
//! | 6..9 | multiplayer gamepad of player 1..4 ([`Controls::mp_pad`]) |
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

pub type KeyTable = [Option<KeyCode>; 11];
pub type PadTable = [Option<GamepadButton>; 11];

/// Columns of the control setup screen.
pub const COLUMNS: usize = 10;

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
    None,
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
    pub pad: PadTable,
    pub mp_keys: [KeyTable; 4],
    pub mp_pad: [PadTable; 4],
}

impl Default for Controls {
    fn default() -> Self {
        Controls {
            keys: SP_KEYS,
            pad: PAD,
            mp_keys: MP_KEYS,
            mp_pad: [PAD; 4],
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
            1 => Table::Pad(self.pad),
            2..=5 => Table::Keys(self.mp_keys[c - 2]),
            _ => Table::Pad(self.mp_pad[(c - 6).min(3)]),
        }
    }

    /// Column `c` is a keyboard table.
    pub fn is_keyboard(c: usize) -> bool {
        matches!(c, 0 | 2..=5)
    }

    pub fn set_key(&mut self, c: usize, action: usize, key: Option<KeyCode>) {
        match c {
            0 => self.keys[action] = key,
            2..=5 => self.mp_keys[c - 2][action] = key,
            _ => {}
        }
    }

    pub fn set_pad(&mut self, c: usize, action: usize, b: Option<GamepadButton>) {
        match c {
            1 => self.pad[action] = b,
            6..=9 => self.mp_pad[c - 6][action] = b,
            _ => {}
        }
    }

    /// Settings-file lines (`key.<action>`, `pad.<action>`,
    /// `mp<n>.key.<action>`, `mp<n>.pad.<action>`).
    pub fn write(&self, t: &mut String) {
        let mut table = |prefix: &str, names: [String; 11]| {
            for (a, n) in ACTIONS.iter().zip(names) {
                t.push_str(&format!("{prefix}{a} = {n}\n"));
            }
        };
        table("key.", self.keys.map(|k| k.map(key_id).unwrap_or_default()));
        table("pad.", self.pad.map(|b| b.map(pad_id).unwrap_or_default()));
        for p in 0..4 {
            table(&format!("mp{}.key.", p + 1), self.mp_keys[p].map(|k| k.map(key_id).unwrap_or_default()));
        }
        for p in 0..4 {
            table(&format!("mp{}.pad.", p + 1), self.mp_pad[p].map(|b| b.map(pad_id).unwrap_or_default()));
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
        match (kind, player) {
            ("key", None) => self.keys[a] = parse_key(v),
            ("key", Some(p)) => self.mp_keys[p][a] = parse_key(v),
            ("pad", None) => self.pad[a] = parse_pad(v),
            ("pad", Some(p)) => self.mp_pad[p][a] = parse_pad(v),
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

/// Keys the redefine prompt refuses (`FUN_01040790`): the digits pick
/// weapons, `-`, `=` and `P` are game keys too. 0 disables (handled by
/// the caller), Esc cancels.
pub fn refused_key(k: KeyCode) -> bool {
    use KeyCode::*;
    matches!(
        k,
        Digit1 | Digit2 | Digit3 | Digit4 | Digit5 | Digit6 | Digit7 | Digit8 | Digit9 | Minus | Equal | KeyP
    )
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
        c.set_key(3, SHOOT, None);
        c.set_pad(7, BRAKE, Some(GamepadButton::West));
        let mut t = String::new();
        c.write(&mut t);
        let mut d = Controls {
            keys: [None; 11],
            pad: [None; 11],
            mp_keys: [[None; 11]; 4],
            mp_pad: [[None; 11]; 4],
        };
        for line in t.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(d.read(k.trim(), v.trim()), "{line}");
        }
        assert_eq!(c, d);
        assert!(!d.read("key.fly", "KeyF"));
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
