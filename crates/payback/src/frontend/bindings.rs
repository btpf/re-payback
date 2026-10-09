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
//!
//! Modern gamepads (port addition, docs/controls.md): the default pad
//! table ([`PAD`]) follows today's Xbox / PlayStation / Switch layouts
//! (positional, as Bevy names the buttons: `South` is A / Cross / B).
//! The game's input is digital (the original polled switches), so
//! analogue controls count as pressed past a threshold: the stick
//! deadzone ([`PadOptions::deadzone`]: the left stick, stick-end bindings
//! and menu navigation) and the trigger threshold ([`PadOptions::trigger`]:
//! LT/RT, whether they report as buttons or as Z axes). Start pauses
//! (fixed, like Esc). Pads are read into the same per-tick `PlayerInput`
//! as the keyboard, so replays are unaffected.

use bevy::input::gamepad::{ButtonSettings, GamepadConnectionEvent, RawGamepadEvent};
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
pub type PadTable = [Option<PadInput>; 11];

/// A gamepad control a binding can use: a button, or an axis pushed to
/// one end (`positive`). The Mac binds any HID element, axes included, and
/// an axis counts as pressed near its recorded extreme (`IsHIDControlPressed`
/// @ 10158300: within ±50 of it on the scaled range, about 60 % of the way
/// out). Here the stick deadzone / trigger threshold decide ([`PadOptions`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadInput {
    Button(GamepadButton),
    Axis(GamepadAxis, bool),
}

/// Button names shown in the control setup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelStyle {
    /// From the pad's USB vendor / name (Sony: PlayStation, Nintendo:
    /// Switch, anything else: Xbox).
    #[default]
    Auto,
    Xbox,
    PlayStation,
    Nintendo,
}

impl LabelStyle {
    pub const ALL: [LabelStyle; 4] = [LabelStyle::Auto, LabelStyle::Xbox, LabelStyle::PlayStation, LabelStyle::Nintendo];

    pub fn id(self) -> &'static str {
        match self {
            LabelStyle::Auto => "auto",
            LabelStyle::Xbox => "xbox",
            LabelStyle::PlayStation => "playstation",
            LabelStyle::Nintendo => "nintendo",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LabelStyle::Auto => "Auto",
            LabelStyle::Xbox => "Xbox",
            LabelStyle::PlayStation => "PlayStation",
            LabelStyle::Nintendo => "Switch",
        }
    }

    fn parse(s: &str) -> Option<LabelStyle> {
        LabelStyle::ALL.into_iter().find(|l| l.id() == s)
    }

    /// The style for a pad (`name`, USB vendor id), Auto resolved.
    pub fn resolve(self, pad: Option<(&str, Option<u16>)>) -> LabelStyle {
        if self != LabelStyle::Auto {
            return self;
        }
        let Some((name, vendor)) = pad else {
            return LabelStyle::Xbox;
        };
        let n = name.to_ascii_lowercase();
        let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
        if vendor == Some(0x054c) || has(&["dualsense", "dualshock", "ps3", "ps4", "ps5", "playstation"]) {
            LabelStyle::PlayStation
        } else if vendor == Some(0x057e) || has(&["nintendo", "switch", "joy-con", "pro controller"]) {
            LabelStyle::Nintendo
        } else {
            LabelStyle::Xbox
        }
    }
}

/// Gamepad options (port addition), shared by every pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadOptions {
    /// Stick deadzone in percent: how far the left stick (or a bound
    /// stick end) must be pushed to count.
    pub deadzone: u8,
    /// Trigger threshold in percent: how far LT/RT must be pulled to count.
    pub trigger: u8,
    pub labels: LabelStyle,
    /// Settings-file layout of the pad tables: 1 = files from before the
    /// modern defaults (no `pad.layout`), [`PAD_LAYOUT`] = now.
    pub layout: u8,
}

/// Current pad-table layout (`pad.layout`).
pub const PAD_LAYOUT: u8 = 2;

pub const DEADZONE_RANGE: (u8, u8) = (10, 90);
pub const TRIGGER_RANGE: (u8, u8) = (5, 90);

impl Default for PadOptions {
    fn default() -> Self {
        PadOptions {
            deadzone: 50,
            trigger: 30,
            labels: LabelStyle::Auto,
            layout: PAD_LAYOUT,
        }
    }
}

impl PadOptions {
    pub fn stick(&self) -> f32 {
        self.deadzone.clamp(DEADZONE_RANGE.0, DEADZONE_RANGE.1) as f32 / 100.0
    }

    pub fn trigger(&self) -> f32 {
        self.trigger.clamp(TRIGGER_RANGE.0, TRIGGER_RANGE.1) as f32 / 100.0
    }

    /// Threshold of an axis end: the triggers' Z axes use the trigger
    /// threshold, the sticks the deadzone.
    pub fn axis_threshold(&self, a: GamepadAxis) -> f32 {
        if matches!(a, GamepadAxis::LeftZ | GamepadAxis::RightZ) { self.trigger() } else { self.stick() }
    }

    /// The fixed joystick directions (left, right, up, down): the left
    /// stick past the deadzone, or the d-pad. Opposite directions held
    /// together cancel (a d-pad can't press both; seen when a release
    /// event is lost, docs/controls.md).
    pub fn directions(&self, stick: Vec2, dpad: Vec2) -> [bool; 4] {
        let t = self.stick();
        let [l, r, u, d] = [
            stick.x <= -t || dpad.x < 0.0,
            stick.x >= t || dpad.x > 0.0,
            stick.y >= t || dpad.y > 0.0,
            stick.y <= -t || dpad.y < 0.0,
        ];
        [l && !r, r && !l, u && !d, d && !u]
    }

    /// [`Self::directions`] of a gamepad's left stick and d-pad buttons
    /// (opposite d-pad buttons cancel before the stick is added).
    pub fn pad_directions(&self, g: &Gamepad) -> [bool; 4] {
        use GamepadButton::*;
        let b = |n, p| g.pressed(p) as i32 as f32 - g.pressed(n) as i32 as f32;
        self.directions(g.left_stick(), Vec2::new(b(DPadLeft, DPadRight), b(DPadDown, DPadUp)))
    }
}

/// Apply the trigger threshold to every pad's LT/RT buttons (Bevy's
/// default press point is 75 %), so `Gamepad::pressed` and the bindings
/// agree with the option.
pub fn apply_pad_options(
    settings: Option<Res<super::progress::Settings>>,
    mut pads: Query<&mut GamepadSettings, With<Gamepad>>,
) {
    let t = settings.map(|s| s.controls.opts).unwrap_or_default().trigger();
    for mut gs in &mut pads {
        for b in [GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2] {
            if gs.get_button_settings(b).press_threshold() != t {
                let release = (t - 0.05).max(0.01);
                if let Ok(bs) = ButtonSettings::new(t, release) {
                    gs.button_settings.insert(b, bs);
                }
            }
        }
    }
}

/// The axes a binding can use, in bit order for [`PadAxes`].
pub const AXES: [GamepadAxis; 6] = [
    GamepadAxis::LeftStickX,
    GamepadAxis::LeftStickY,
    GamepadAxis::RightStickX,
    GamepadAxis::RightStickY,
    GamepadAxis::LeftZ,
    GamepadAxis::RightZ,
];

/// Bit of an axis end in [`PadAxes`].
fn axis_bit(axis: GamepadAxis, positive: bool) -> Option<u16> {
    AXES.iter().position(|&a| a == axis).map(|i| 1 << (2 * i + positive as usize))
}

/// Bit of the joystick direction `i` (left, right, up, down) in
/// [`PadTrack::armed`], after the axis-end bits.
const DIR_BITS: u32 = 2 * AXES.len() as u32;

/// Frames after a pad connects in which whatever is held is taken as its
/// rest state (about half a second).
const GRACE_FRAMES: u32 = 30;

/// One gamepad's tracked state ([`track_pad_axes`]).
#[derive(Clone, Copy, Debug, Default)]
struct PadTrack {
    /// Axis ends pressed the frame before and this frame (bits of
    /// [`axis_bit`]), already guarded.
    prev: u16,
    now: u16,
    /// The joystick directions (left, right, up, down), guarded.
    dirs: [bool; 4],
    /// Axis ends and directions seen released since the pad connected:
    /// one held from the start (a stick resting off centre, a trigger
    /// resting at -1, a button reported held; anything held during the
    /// first [`GRACE_FRAMES`] frames, when the platform reports the
    /// initial state) counts only once it has been let go.
    armed: u32,
    /// Frames since the pad connected, up to [`GRACE_FRAMES`].
    frames: u32,
    /// LeftZ / RightZ seen below zero: a trigger reported on the full
    /// -1..1 range (rest at -1), read as (v + 1) / 2.
    signed: [bool; 2],
}

/// Every gamepad's axis ends and joystick directions, this frame and the
/// frame before, so that axis bindings have presses (edges) like buttons;
/// with the guards against pads that misreport their rest
/// ([`PadTrack`], docs/controls.md).
#[derive(Resource, Default)]
pub struct PadAxes(bevy::platform::collections::HashMap<Entity, PadTrack>);

impl PadAxes {
    /// The axis ends newly pushed on gamepad `e` this frame. A trigger
    /// that reports as a button as well as a Z axis gives its button only
    /// (the redefine prompt takes the button).
    pub fn just_pressed(&self, e: Entity, g: &Gamepad) -> Vec<PadInput> {
        let t = self.0.get(&e).copied().unwrap_or_default();
        let new = t.now & !t.prev;
        let has_button = |a| match a {
            GamepadAxis::LeftZ => g.get(GamepadButton::LeftTrigger2).is_some_and(|v| v > 0.0),
            GamepadAxis::RightZ => g.get(GamepadButton::RightTrigger2).is_some_and(|v| v > 0.0),
            _ => false,
        };
        AXES.iter()
            .flat_map(|&a| [(a, false), (a, true)])
            .filter(|&(a, p)| axis_bit(a, p).is_some_and(|b| new & b != 0) && !has_button(a))
            .map(|(a, p)| PadInput::Axis(a, p))
            .collect()
    }

    /// Gamepad `e` with its axis edges and directions.
    pub fn state<'a>(&self, e: Entity, g: &'a Gamepad) -> PadState<'a> {
        let t = self.0.get(&e).copied().unwrap_or_default();
        PadState { g, prev: t.prev, now: t.now, dirs: t.dirs }
    }
}

/// An axis value as the bindings read it: triggers reported on -1..1
/// moved to 0..1 (`signed` remembers the range once seen).
fn axis_value(a: GamepadAxis, v: f32, signed: &mut [bool; 2]) -> f32 {
    let i = match a {
        GamepadAxis::LeftZ => 0,
        GamepadAxis::RightZ => 1,
        _ => return v,
    };
    signed[i] |= v < -0.05;
    if signed[i] { (v + 1.0) / 2.0 } else { v }
}

/// Update [`PadAxes`] (`PreUpdate`, after the gamepad input).
pub fn track_pad_axes(
    mut axes: ResMut<PadAxes>,
    pads: Query<(Entity, &Gamepad)>,
    settings: Option<Res<super::progress::Settings>>,
) {
    let opts = settings.map(|s| s.controls.opts).unwrap_or_default();
    axes.0.retain(|e, _| pads.contains(*e));
    for (e, g) in &pads {
        let t = axes.0.entry(e).or_default();
        let mut raw = 0u32;
        for (i, &a) in AXES.iter().enumerate() {
            let v = axis_value(a, g.get(a).unwrap_or(0.0), &mut t.signed);
            let th = opts.axis_threshold(a);
            if v <= -th {
                raw |= 1 << (2 * i);
            }
            if v >= th {
                raw |= 1 << (2 * i + 1);
            }
        }
        for (i, d) in opts.pad_directions(g).into_iter().enumerate() {
            raw |= (d as u32) << (DIR_BITS + i as u32);
        }
        if t.frames < GRACE_FRAMES {
            t.frames += 1;
            t.armed = !raw;
        } else {
            t.armed |= !raw;
        }
        let held = raw & t.armed;
        t.prev = t.now;
        t.now = held as u16;
        t.dirs = std::array::from_fn(|i| held & (1 << (DIR_BITS + i as u32)) != 0);
    }
}

/// A gamepad as the bindings read it ([`PadAxes::state`]).
pub struct PadState<'a> {
    pub g: &'a Gamepad,
    prev: u16,
    now: u16,
    dirs: [bool; 4],
}

impl PadState<'_> {
    /// The joystick directions (left, right, up, down): left stick past
    /// the deadzone or d-pad, opposite directions cancelling, a direction
    /// held since the pad connected ignored until released.
    pub fn directions(&self) -> [bool; 4] {
        self.dirs
    }
}

/// Columns of the control setup screen.
pub const COLUMNS: usize = 9;

/// First multiplayer key-set column.
pub const KEY_SET_COLUMN: usize = 5;

/// Single-player keyboard: the Mac default map (`ClearPreference`
/// @ 100508c0: 7E up, 7D down, 7B left, 7C right, 31 Space, 7F unbound,
/// 24 Return, 06 Z, 72 Help, 75 Forward-Delete, 2E M; the Amiga map key for
/// key, except that the Amiga's shoot-alt slot 0x00 fired on the backtick
/// key). Help is Insert on PC keyboards, Forward-Delete is Delete.
const SP_KEYS: KeyTable = [
    Some(KeyCode::ArrowUp),
    Some(KeyCode::ArrowDown),
    Some(KeyCode::ArrowLeft),
    Some(KeyCode::ArrowRight),
    Some(KeyCode::Space),
    None,
    Some(KeyCode::Enter),
    Some(KeyCode::KeyZ),
    Some(KeyCode::Insert),
    Some(KeyCode::Delete),
    Some(KeyCode::KeyM),
];

/// Fixed next-weapon keys besides the binding (Mac `UpdateInputStatus`
/// @ 100f8490 key table `0x1018f760`: Help, F3, F10, keypad +), single
/// player only (the multiplayer key sets use the keypad).
pub const NEXT_WEAPON_ALIASES: [KeyCode; 3] = [KeyCode::F3, KeyCode::F10, KeyCode::NumpadAdd];

/// Fixed previous-weapon keys (Forward-Delete, F2, F9, keypad −).
pub const PREV_WEAPON_ALIASES: [KeyCode; 4] = [KeyCode::Delete, KeyCode::F2, KeyCode::F9, KeyCode::NumpadSubtract];

/// A gamepad (layout 2, docs/controls.md); up/down/left/right also come
/// from the left stick and d-pad. RT accelerate, LT reverse, A fire (X
/// too), Y in/out, B brake, RB/LB next/previous weapon, Back repeats the
/// last message (Xbox names; PlayStation R2 L2 Cross Square Triangle
/// Circle R1 L1 Share, Switch ZR ZL B Y X A R L −).
pub const PAD: PadTable = {
    use GamepadButton::*;
    use PadInput::Button as B;
    [
        Some(B(RightTrigger2)),
        Some(B(LeftTrigger2)),
        Some(B(DPadLeft)),
        Some(B(DPadRight)),
        Some(B(South)),
        Some(B(West)),
        Some(B(North)),
        Some(B(East)),
        Some(B(RightTrigger)),
        Some(B(LeftTrigger)),
        Some(B(Select)),
    ]
};

/// The pad default before layout 2 (no shoot-alt or repeat-message
/// button); tables still equal to it move to [`PAD`].
const PAD_V1: PadTable = {
    let mut t = PAD;
    t[SHOOT_ALT] = None;
    t[REPEAT_MESSAGE] = None;
    t
};

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
    pub opts: PadOptions,
}

impl Default for Controls {
    fn default() -> Self {
        Controls {
            keys: SP_KEYS,
            pads: [PAD; 4],
            mp_keys: MP_KEYS,
            opts: PadOptions::default(),
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

    pub fn set_pad(&mut self, c: usize, action: usize, b: Option<PadInput>) {
        if (1..KEY_SET_COLUMN).contains(&c) {
            self.pads[c - 1][action] = b;
        }
    }

    /// Every gamepad table back to the default (options kept).
    pub fn reset_pads(&mut self) {
        self.pads = [PAD; 4];
    }

    /// Every keyboard table back to the default.
    pub fn reset_keys(&mut self) {
        self.keys = SP_KEYS;
        self.mp_keys = MP_KEYS;
    }

    /// Settings files from before layout 2: pad tables still on the old
    /// default take the new one (edited tables are kept).
    pub fn upgrade(&mut self) {
        if self.opts.layout < PAD_LAYOUT {
            for t in &mut self.pads {
                if *t == PAD_V1 {
                    *t = PAD;
                }
            }
            self.opts.layout = PAD_LAYOUT;
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
        // After the pad tables: `pad.layout` marks a file as current.
        let o = &self.opts;
        t.push_str(&format!("pad.deadzone = {}\npad.trigger = {}\n", o.deadzone, o.trigger));
        t.push_str(&format!("pad.labels = {}\npad.layout = {}\n", o.labels.id(), o.layout.max(PAD_LAYOUT)));
    }

    /// One settings-file line; false if the key is not a binding.
    pub fn read(&mut self, k: &str, v: &str) -> bool {
        let o = &mut self.opts;
        match k {
            "pad.deadzone" => {
                o.deadzone = v.parse::<u8>().map_or(o.deadzone, |n| n.clamp(DEADZONE_RANGE.0, DEADZONE_RANGE.1));
                return true;
            }
            "pad.trigger" => {
                o.trigger = v.parse::<u8>().map_or(o.trigger, |n| n.clamp(TRIGGER_RANGE.0, TRIGGER_RANGE.1));
                return true;
            }
            "pad.labels" => {
                o.labels = LabelStyle::parse(v).unwrap_or_default();
                return true;
            }
            "pad.layout" => {
                o.layout = v.parse().unwrap_or(1);
                return true;
            }
            _ => {}
        }
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
            (_, None, Some(n)) => {
                self.pads[n][a] = parse_pad(v);
                // Written before `pad.layout`: a file without that is older.
                self.opts.layout = 1;
            }
            // Older files: per-player multiplayer pads, now per gamepad.
            (_, Some(_), Some(0)) => {}
            _ => return false,
        }
        true
    }
}

/// One gamepad number's device (see [`PadSlots`]).
#[derive(Clone, Debug, PartialEq)]
struct PadSlot {
    entity: Entity,
    /// Name, USB vendor and product id, to recognise the device when it
    /// comes back as another entity.
    id: (String, Option<u16>, Option<u16>),
    connected: bool,
}

/// Gamepad numbers 1..4 (`Device::Joystick(0..3)`, the control setup's
/// pad columns), kept per device: the Mac's HID devices map to fixed
/// joystick numbers (`UInputSprocket`, `HIDBuildDeviceList`), so a pad
/// that is unplugged keeps its number free until it (or the same model)
/// comes back, instead of the others moving up. A new pad takes the first
/// number never used, else the first one whose pad is gone.
///
/// Hot-plugging (port addition): a pad that disconnects while connected
/// sets [`PadSlots::lost`], which pauses a running level
/// (`frontend::run_menus`), and single player moves on to the next
/// connected pad.
#[derive(Resource, Default, Debug)]
pub struct PadSlots {
    slots: [Option<PadSlot>; 4],
    /// A connected pad went away (cleared by [`PadSlots::take_lost`]).
    lost: bool,
}

/// Name and USB vendor of the pad on each number, for the control setup
/// (button label style, the connected-pads list).
static PAD_INFO: std::sync::Mutex<[Option<(String, Option<u16>)>; 4]> =
    std::sync::Mutex::new([None, None, None, None]);

/// The connected pad on number `n` (0..3): name and USB vendor.
pub fn pad_info(n: usize) -> Option<(String, Option<u16>)> {
    PAD_INFO.lock().ok()?.get(n)?.clone()
}

impl PadSlots {
    /// Gamepad number `n`'s entity, while it is connected.
    pub fn get(&self, n: usize) -> Option<Entity> {
        self.slots.get(n)?.as_ref().filter(|s| s.connected).map(|s| s.entity)
    }

    /// The lowest-numbered connected gamepad (single player).
    pub fn first(&self) -> Option<Entity> {
        (0..4).find_map(|n| self.get(n))
    }

    /// A connected pad was unplugged since the last call.
    pub fn take_lost(&mut self) -> bool {
        std::mem::take(&mut self.lost)
    }

    /// Number the connected pads (`(entity, name, vendor, product)`, in
    /// spawn order, the order Bevy saw them connect).
    fn update(&mut self, mut pads: Vec<(Entity, (String, Option<u16>, Option<u16>))>) {
        pads.sort_by_key(|p| p.0.index_u32());
        for (n, s) in self.slots.iter_mut().enumerate() {
            let Some(s) = s else { continue };
            let now = pads.iter().any(|p| p.0 == s.entity);
            if s.connected && !now {
                info!("Gamepad {} disconnected: {}", n + 1, s.id.0);
                self.lost = true;
            }
            s.connected = now;
        }
        for (e, id) in pads {
            if self.slots.iter().flatten().any(|s| s.entity == e) {
                continue;
            }
            let back = self.slots.iter().position(|s| s.as_ref().is_some_and(|s| !s.connected && s.id == id));
            let fresh = || self.slots.iter().position(Option::is_none);
            let stale = || self.slots.iter().position(|s| s.as_ref().is_some_and(|s| !s.connected));
            if let Some(n) = back.or_else(fresh).or_else(stale) {
                info!("Gamepad {} connected: {} (vendor {:04x?}, product {:04x?})", n + 1, id.0, id.1, id.2);
                self.slots[n] = Some(PadSlot { entity: e, id, connected: true });
            }
        }
        if let Ok(mut info) = PAD_INFO.lock() {
            for (i, s) in self.slots.iter().enumerate() {
                info[i] = s.as_ref().filter(|s| s.connected).map(|s| (s.id.0.clone(), s.id.1));
            }
        }
    }
}

/// Keep [`PadSlots`] in step with the connected gamepads.
pub fn assign_pads(mut slots: ResMut<PadSlots>, pads: Query<(Entity, &Gamepad, Option<&Name>)>) {
    let list: Vec<_> = pads
        .iter()
        .map(|(e, g, n)| (e, (n.map_or_else(String::new, |n| n.as_str().to_string()), g.vendor_id(), g.product_id())))
        .collect();
    let changed = list.len() != slots.slots.iter().flatten().filter(|s| s.connected).count()
        || list.iter().any(|(e, _)| !slots.slots.iter().flatten().any(|s| s.connected && s.entity == *e));
    if changed {
        slots.update(list);
    }
}

/// Settings files from before the modern pad defaults: untouched pad
/// tables take them (`Startup`; [`Controls::upgrade`]).
pub fn upgrade_pad_layout(mut settings: ResMut<super::progress::Settings>) {
    if settings.controls.opts.layout < PAD_LAYOUT {
        settings.controls.upgrade();
    }
}

/// Name of a gamepad control in a label style (resolved, not Auto), as
/// printed on today's pads: "A", "RT", "LS Left", "Cross", "ZR" ...
pub fn pad_label(b: PadInput, style: LabelStyle) -> String {
    use GamepadButton::*;
    use LabelStyle::*;
    let pick = |x: &str, p: &str, n: &str| -> String {
        match style {
            PlayStation => p,
            Nintendo => n,
            _ => x,
        }
        .to_string()
    };
    match b {
        PadInput::Button(b) => match b {
            South => pick("A", "Cross", "B"),
            East => pick("B", "Circle", "A"),
            West => pick("X", "Square", "Y"),
            North => pick("Y", "Triangle", "X"),
            LeftTrigger => pick("LB", "L1", "L"),
            RightTrigger => pick("RB", "R1", "R"),
            LeftTrigger2 => pick("LT", "L2", "ZL"),
            RightTrigger2 => pick("RT", "R2", "ZR"),
            Select => pick("Back", "Share", "Minus"),
            Start => pick("Start", "Options", "Plus"),
            Mode => pick("Guide", "PS", "Home"),
            LeftThumb => pick("LS Click", "L3", "LS Click"),
            RightThumb => pick("RS Click", "R3", "RS Click"),
            DPadUp => "D-pad Up".into(),
            DPadDown => "D-pad Down".into(),
            DPadLeft => "D-pad Left".into(),
            DPadRight => "D-pad Right".into(),
            b => pad_id(PadInput::Button(b)),
        },
        PadInput::Axis(a, p) => {
            let stick = |s: &str, neg: &str, pos: &str| format!("{s} {}", if p { pos } else { neg });
            match a {
                GamepadAxis::LeftStickX => stick("LS", "Left", "Right"),
                GamepadAxis::LeftStickY => stick("LS", "Down", "Up"),
                GamepadAxis::RightStickX => stick("RS", "Left", "Right"),
                GamepadAxis::RightStickY => stick("RS", "Down", "Up"),
                GamepadAxis::LeftZ if p => pick("LT", "L2", "ZL"),
                GamepadAxis::RightZ if p => pick("RT", "R2", "ZR"),
                a => pad_id(PadInput::Axis(a, p)),
            }
        }
    }
}

/// Menu auto-repeat of a held direction: the first repeat after
/// [`REPEAT_DELAY`] s, then every [`REPEAT_RATE`] s (port addition; the
/// original's menus step once per press).
pub const REPEAT_DELAY: f32 = 0.4;
pub const REPEAT_RATE: f32 = 0.1;

/// A pad's held menu direction (x: −1 left, +1 right; y: +1 up).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NavRepeat {
    dir: IVec2,
    next: f32,
    /// Components held when the menu opened (or the pad came): ignored
    /// until released, so a direction stuck on can't run the menu away.
    block: IVec2,
    /// Last menu frame, to notice a menu that opens again.
    last: f32,
}

/// A menu not read for this long has closed and opened again.
const NAV_GAP: f32 = 0.5;

impl NavRepeat {
    /// A new entry at time `now` with `dir` held: that direction is
    /// blocked until released.
    pub fn opened(dir: IVec2, now: f32) -> Self {
        NavRepeat { dir: IVec2::ZERO, next: now, block: dir, last: now }
    }

    /// The directions pressed at time `now` with `dir` held: newly pushed
    /// components at once, the whole direction again while held.
    pub fn step(&mut self, dir: IVec2, now: f32) -> IVec2 {
        if now - self.last > NAV_GAP {
            *self = Self::opened(dir, now);
        }
        self.last = now;
        // A blocked component frees once it goes back to centre (or flips).
        self.block = IVec2::new(
            if dir.x == self.block.x { self.block.x } else { 0 },
            if dir.y == self.block.y { self.block.y } else { 0 },
        );
        let dir = IVec2::new(
            if self.block.x != 0 { 0 } else { dir.x },
            if self.block.y != 0 { 0 } else { dir.y },
        );
        let new = IVec2::new(
            if dir.x != self.dir.x { dir.x } else { 0 },
            if dir.y != self.dir.y { dir.y } else { 0 },
        );
        self.dir = dir;
        if new != IVec2::ZERO {
            self.next = now + REPEAT_DELAY;
            new
        } else if dir != IVec2::ZERO && now >= self.next {
            self.next = now + REPEAT_RATE;
            dir
        } else {
            IVec2::ZERO
        }
    }
}

/// Every connected pad into the menu keys: d-pad or left stick (past the
/// deadzone, auto-repeating; [`PadState::directions`], so a direction
/// held since the pad came is ignored until released, and one held when
/// the menu opens too, [`NavRepeat`]), A (South) or Start = fire, B (East) = back,
/// Start also sets `start` (the pause menu resumes on it).
pub fn pad_menu_input<'a>(
    pads: impl Iterator<Item = (Entity, &'a Gamepad)>,
    axes: &PadAxes,
    nav: &mut Vec<(Entity, NavRepeat)>,
    now: f32,
    input: &mut super::ui::Input,
) {
    let mut seen = Vec::new();
    for (e, pad) in pads {
        input.pad.extend(pad.get_just_pressed().copied());
        input.pad_axes.extend(axes.just_pressed(e, pad));
        let ok = pad.any_just_pressed([GamepadButton::South, GamepadButton::Start]);
        input.fire |= ok;
        input.enter |= ok;
        input.back |= pad.just_pressed(GamepadButton::East);
        input.start |= pad.just_pressed(GamepadButton::Start);
        let [l, r, u, d] = axes.state(e, pad).directions();
        let dir = IVec2::new(r as i32 - l as i32, u as i32 - d as i32);
        let mut rep = nav.iter().find(|p| p.0 == e).map_or_else(|| NavRepeat::opened(dir, now), |p| p.1);
        let press = rep.step(dir, now);
        if press != IVec2::ZERO {
            pad_log(format_args!(
                "menu nav {press} from pad {e}: left stick {:.3?}, d-pad {:.3?}",
                pad.left_stick(),
                pad.dpad()
            ));
        }
        input.left |= press.x < 0;
        input.right |= press.x > 0;
        input.up |= press.y > 0;
        input.down |= press.y < 0;
        seen.push((e, rep));
    }
    *nav = seen;
}

/// `--pad-sim "<secs>:<control>[/<hold secs>],..."` (debug): a virtual
/// gamepad that connects at start and presses each control (a
/// [`pad_id`] name such as `DPadDown`, `South`, `LeftStickY-`; or
/// `disconnect` / `connect`) at the given real time for 0.15 s or the
/// hold time. `--pad-sim-name` names it (default "Xbox Controller
/// (virtual)"), `--pad-sim-vendor <hex>` sets its USB vendor id.
/// `PreUpdate`, before the input systems.
pub fn simulate_pad(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut state: Local<Option<(Entity, Vec<(f32, Option<PadInput>, bool)>)>>,
    mut connect: MessageWriter<GamepadConnectionEvent>,
    mut raw: MessageWriter<RawGamepadEvent>,
) {
    use bevy::input::gamepad::{GamepadConnection, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent};
    let Some(script) = crate::arg("--pad-sim") else { return };
    let connection = || GamepadConnection::Connected {
        name: crate::arg("--pad-sim-name").unwrap_or_else(|| "Xbox Controller (virtual)".into()),
        vendor_id: crate::arg("--pad-sim-vendor").and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok()),
        product_id: Some(1),
    };
    let (e, events) = state.get_or_insert_with(|| {
        let e = commands.spawn_empty().id();
        connect.write(GamepadConnectionEvent::new(e, connection()));
        let mut events = Vec::new();
        for item in script.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let Some((t, what)) = item.split_once(':') else { continue };
            let (what, hold) = what.split_once('/').unwrap_or((what, "0.15"));
            let (Ok(t), Ok(hold)) = (t.parse::<f32>(), hold.parse::<f32>()) else { continue };
            match what {
                "disconnect" => events.push((t, None, false)),
                "connect" => events.push((t, None, true)),
                w => {
                    if let Some(b) = parse_pad(w) {
                        events.push((t, Some(b), true));
                        events.push((t + hold, Some(b), false));
                    } else {
                        warn!("--pad-sim: unknown control {w}");
                    }
                }
            }
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        (e, events)
    });
    let now = time.elapsed_secs();
    while events.first().is_some_and(|ev| ev.0 <= now) {
        let (_, what, on) = events.remove(0);
        match what {
            None if on => {
                connect.write(GamepadConnectionEvent::new(*e, connection()));
            }
            None => {
                connect.write(GamepadConnectionEvent::new(*e, GamepadConnection::Disconnected));
            }
            Some(PadInput::Button(b)) => {
                raw.write(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(*e, b, on as u8 as f32)));
            }
            Some(PadInput::Axis(a, p)) => {
                let v = if !on { 0.0 } else if p { 1.0 } else { -1.0 };
                raw.write(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(*e, a, v)));
            }
        }
    }
}

/// Gamepad diagnostics (port addition, docs/controls.md): on with
/// `--pad-debug`, `PAYBACK_PAD_DEBUG=1`, or a file named `pad-debug` next
/// to the exe (for launches without a console, such as a Steam shortcut).
/// Logs to `payback.log` next to the exe (the previous run's log is kept
/// as `payback.log.1`) and to the console: every gamepad connection (name,
/// USB vendor / product id), every raw button and axis change as the
/// platform reports it, keyboard keys, mouse wheel and motion (a remapper
/// such as Steam Input's desktop layout may turn a pad into keys and
/// mouse), every menu navigation step with its source, and once a second
/// each pad's controls that are away from rest.
pub fn pad_debug_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        crate::args().any(|a| a == "--pad-debug")
            || std::env::var("PAYBACK_PAD_DEBUG").is_ok_and(|v| !v.is_empty() && v != "0")
            || exe_dir().is_some_and(|d| d.join("pad-debug").exists())
    })
}

fn exe_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return None;
    #[cfg(not(target_arch = "wasm32"))]
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

/// One [`pad_debug_enabled`] line: console and `payback.log`.
pub fn pad_log(msg: std::fmt::Arguments) {
    use std::io::Write;
    static FILE: std::sync::Mutex<Option<Option<std::fs::File>>> = std::sync::Mutex::new(None);
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    if !pad_debug_enabled() {
        return;
    }
    let t = START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
    info!("pad-debug {t:9.3}s {msg}");
    let Ok(mut f) = FILE.lock() else { return };
    let f = f.get_or_insert_with(|| {
        let path = exe_dir()?.join("payback.log");
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
        let mut f = std::fs::File::create(&path).ok()?;
        let _ = writeln!(f, "Payback pad diagnostics, {}", std::env::args().collect::<Vec<_>>().join(" "));
        Some(f)
    });
    if let Some(f) = f {
        let _ = writeln!(f, "{t:9.3}s {msg}");
    }
}

/// [`pad_debug_enabled`] logging (`PreUpdate`, after the input systems).
#[allow(clippy::too_many_arguments)]
pub fn pad_debug(
    mut raw: MessageReader<RawGamepadEvent>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut buttons: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    time: Res<Time<Real>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    mut last: Local<(f32, Vec2, u32)>,
) {
    if !pad_debug_enabled() {
        raw.clear();
        keys.clear();
        wheel.clear();
        buttons.clear();
        motion.clear();
        return;
    }
    for ev in raw.read() {
        match ev {
            RawGamepadEvent::Connection(c) => pad_log(format_args!("pad {}: {:?}", c.gamepad, c.connection)),
            RawGamepadEvent::Button(b) => pad_log(format_args!("pad {}: button {:?} = {:.3}", b.gamepad, b.button, b.value)),
            RawGamepadEvent::Axis(a) => pad_log(format_args!("pad {}: axis {:?} = {:.3}", a.gamepad, a.axis, a.value)),
        }
    }
    for k in keys.read() {
        if !k.repeat {
            pad_log(format_args!("keyboard {:?} {:?}", k.key_code, k.state));
        }
    }
    for w in wheel.read() {
        pad_log(format_args!("mouse wheel {:.2},{:.2} ({:?})", w.x, w.y, w.unit));
    }
    for b in buttons.read() {
        pad_log(format_args!("mouse button {:?} {:?}", b.button, b.state));
    }
    for m in motion.read() {
        last.1 += m.delta;
        last.2 += 1;
    }
    let t = time.elapsed_secs();
    if t - last.0 < 1.0 {
        return;
    }
    last.0 = t;
    if last.2 > 0 {
        pad_log(format_args!("mouse motion in the last second: {} events, total {:.1},{:.1}", last.2, last.1.x, last.1.y));
        last.1 = Vec2::ZERO;
        last.2 = 0;
    }
    if pads.is_empty() {
        pad_log(format_args!("no gamepads connected"));
    }
    for (e, g, name) in &pads {
        let axes: Vec<String> = g
            .analog()
            .all_axes_and_values()
            .filter(|(i, v)| matches!(i, bevy::input::gamepad::GamepadInput::Axis(_)) && *v != 0.0)
            .map(|(i, v)| format!("{i:?}={v:.3}"))
            .collect();
        let pressed: Vec<String> = g.get_pressed().map(|b| format!("{b:?}")).collect();
        pad_log(format_args!(
            "pad {e} {:?} (vendor {:04x?}, product {:04x?}): axes off rest [{}], pressed [{}]",
            name.map(|n| n.as_str()),
            g.vendor_id(),
            g.product_id(),
            axes.join(", "),
            pressed.join(", ")
        ));
    }
}

/// What the character keys print on this keyboard layout, learnt from
/// the key presses (the Mac named keys through the system layout,
/// `ALCharCodeFromKeyCodeAndMap`; Bevy's `KeyCode` is the physical US
/// position, so on AZERTY the key `KeyCode::KeyQ` prints "A").
static KEY_LABELS: std::sync::Mutex<Vec<(KeyCode, String)>> = std::sync::Mutex::new(Vec::new());

/// The layout's label of a character key, once it has been pressed.
pub fn key_label(k: KeyCode) -> Option<String> {
    KEY_LABELS.lock().ok()?.iter().find(|(c, _)| *c == k).map(|(_, l)| l.clone())
}

/// Learn [`key_label`]s from the keyboard input (`PreUpdate`).
pub fn learn_key_labels(mut events: MessageReader<bevy::input::keyboard::KeyboardInput>) {
    use bevy::input::keyboard::Key;
    for ev in events.read() {
        let Key::Character(s) = &ev.logical_key else { continue };
        let mut chars = s.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { continue };
        if c.is_whitespace() || c.is_control() || !character_key(ev.key_code) {
            continue;
        }
        let label: String = c.to_uppercase().collect();
        if let Ok(mut labels) = KEY_LABELS.lock() {
            match labels.iter_mut().find(|(k, _)| *k == ev.key_code) {
                Some((_, l)) if *l != label => *l = label,
                Some(_) => {}
                None => labels.push((ev.key_code, label)),
            }
        }
    }
}

/// Keys whose label depends on the layout (letters, digits, punctuation).
fn character_key(k: KeyCode) -> bool {
    use KeyCode::*;
    let id = key_id(k);
    id.starts_with("Key")
        || id.starts_with("Digit")
        || matches!(
            k,
            Minus | Equal | BracketLeft | BracketRight | Backslash | Semicolon | Quote | Backquote | Comma | Period
                | Slash | IntlBackslash
        )
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

pub fn pad_held(pad: &PadState, b: Option<PadInput>) -> bool {
    match b {
        Some(PadInput::Button(b)) => pad.g.pressed(b),
        Some(PadInput::Axis(a, p)) => axis_bit(a, p).is_some_and(|bit| pad.now & bit != 0),
        None => false,
    }
}

pub fn pad_just(pad: &PadState, b: Option<PadInput>) -> bool {
    match b {
        Some(PadInput::Button(b)) => pad.g.just_pressed(b),
        Some(PadInput::Axis(a, p)) => axis_bit(a, p).is_some_and(|bit| pad.now & !pad.prev & bit != 0),
        None => false,
    }
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

/// Settings-file name of a gamepad control: Bevy's `GamepadButton` name,
/// or a `GamepadAxis` name with `+` / `-` for the end.
pub fn pad_id(b: PadInput) -> String {
    match b {
        PadInput::Button(b) => format!("{b:?}"),
        PadInput::Axis(a, p) => format!("{a:?}{}", if p { '+' } else { '-' }),
    }
}

pub fn parse_key(s: &str) -> Option<KeyCode> {
    KEYS.iter().copied().find(|k| key_id(*k) == s)
}

pub fn parse_pad(s: &str) -> Option<PadInput> {
    let buttons = PADS.iter().map(|&b| PadInput::Button(b));
    let axes = AXES.iter().flat_map(|&a| [PadInput::Axis(a, false), PadInput::Axis(a, true)]);
    buttons.chain(axes).find(|b| pad_id(*b) == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let mut c = Controls::default();
        c.set_key(0, NEXT_WEAPON, Some(KeyCode::KeyM));
        c.set_key(6, SHOOT, None);
        c.set_pad(3, BRAKE, Some(PadInput::Button(GamepadButton::West)));
        c.set_pad(1, SHOOT_ALT, Some(PadInput::Axis(GamepadAxis::RightZ, true)));
        c.set_pad(1, NEXT_WEAPON, None);
        c.set_pad(2, LEFT, Some(PadInput::Axis(GamepadAxis::RightStickX, false)));
        c.opts = PadOptions {
            deadzone: 35,
            trigger: 15,
            labels: LabelStyle::Nintendo,
            layout: PAD_LAYOUT,
        };
        let mut t = String::new();
        c.write(&mut t);
        let mut d = Controls {
            keys: [None; 11],
            pads: [[None; 11]; 4],
            mp_keys: [[None; 11]; 4],
            opts: PadOptions::default(),
        };
        for line in t.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(d.read(k.trim(), v.trim()), "{line}");
        }
        assert_eq!(c, d);
        assert_eq!(d.pads[2][BRAKE], Some(PadInput::Button(GamepadButton::West)));
        assert_eq!(d.pads[0][SHOOT_ALT], Some(PadInput::Axis(GamepadAxis::RightZ, true)));
        assert_eq!(d.mp_keys[1][SHOOT], None);
        assert!(!d.read("key.fly", "KeyF"));
        assert!(!d.read("pad5.brake", "West"));
        assert!(d.read("mp2.pad.brake", "East"), "old per-player pad lines are skipped");
        assert_eq!(c, d);
    }

    #[test]
    fn pad_numbers_stable() {
        let e = |i: u32| Entity::from_raw_u32(i).unwrap();
        let id = |n: &str| (n.to_string(), Some(1), Some(2));
        let mut s = PadSlots::default();
        s.update(vec![(e(7), id("b")), (e(5), id("a"))]);
        assert_eq!((s.get(0), s.get(1)), (Some(e(5)), Some(e(7))));
        // Pad 1 unplugged: pad 2 keeps its number.
        s.update(vec![(e(7), id("b"))]);
        assert_eq!((s.get(0), s.get(1), s.first()), (None, Some(e(7)), Some(e(7))));
        // A third pad takes a fresh number, not the absent pad's.
        s.update(vec![(e(7), id("b")), (e(9), id("c"))]);
        assert_eq!(s.get(2), Some(e(9)));
        // Pad 1 comes back as a new entity: same number.
        s.update(vec![(e(7), id("b")), (e(9), id("c")), (e(11), id("a"))]);
        assert_eq!(s.get(0), Some(e(11)));
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
            assert_eq!(parse_pad(&pad_id(PadInput::Button(b))), Some(PadInput::Button(b)));
        }
        for &a in &AXES {
            for p in [false, true] {
                assert_eq!(parse_pad(&pad_id(PadInput::Axis(a, p))), Some(PadInput::Axis(a, p)));
            }
        }
    }

    #[test]
    fn default_pad_layout() {
        use GamepadButton::*;
        let c = Controls::default();
        assert!(c.pads.iter().all(|t| *t == PAD));
        let b = |a: usize| PAD[a];
        assert_eq!(b(ACCELERATE), Some(PadInput::Button(RightTrigger2)));
        assert_eq!(b(REVERSE), Some(PadInput::Button(LeftTrigger2)));
        assert_eq!(b(SHOOT), Some(PadInput::Button(South)));
        assert_eq!(b(SHOOT_ALT), Some(PadInput::Button(West)));
        assert_eq!(b(IN_OUT), Some(PadInput::Button(North)));
        assert_eq!(b(BRAKE), Some(PadInput::Button(East)));
        assert_eq!(b(NEXT_WEAPON), Some(PadInput::Button(RightTrigger)));
        assert_eq!(b(PREV_WEAPON), Some(PadInput::Button(LeftTrigger)));
        assert_eq!(b(REPEAT_MESSAGE), Some(PadInput::Button(Select)));
        // Every action bound, no button twice, Start (pause) left free.
        let mut all: Vec<String> = PAD.iter().map(|b| pad_id(b.unwrap())).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), ACTIONS.len());
        assert!(!PAD.contains(&Some(PadInput::Button(Start))));
        // The keyboard is unchanged.
        assert_eq!(c.keys, SP_KEYS);
        assert_eq!(c.keys[ACCELERATE], Some(KeyCode::ArrowUp));
    }

    #[test]
    fn labels() {
        use GamepadButton::*;
        let l = |b, s| pad_label(PadInput::Button(b), s);
        assert_eq!(l(South, LabelStyle::Xbox), "A");
        assert_eq!(l(South, LabelStyle::PlayStation), "Cross");
        assert_eq!(l(South, LabelStyle::Nintendo), "B");
        assert_eq!(l(RightTrigger2, LabelStyle::Xbox), "RT");
        assert_eq!(l(RightTrigger2, LabelStyle::PlayStation), "R2");
        assert_eq!(l(RightTrigger2, LabelStyle::Nintendo), "ZR");
        assert_eq!(l(LeftTrigger, LabelStyle::Xbox), "LB");
        assert_eq!(pad_label(PadInput::Axis(GamepadAxis::LeftStickX, false), LabelStyle::Xbox), "LS Left");
        assert_eq!(pad_label(PadInput::Axis(GamepadAxis::RightStickY, true), LabelStyle::Xbox), "RS Up");
        assert_eq!(pad_label(PadInput::Axis(GamepadAxis::RightZ, true), LabelStyle::PlayStation), "R2");
        let auto = LabelStyle::Auto;
        assert_eq!(auto.resolve(None), LabelStyle::Xbox);
        assert_eq!(auto.resolve(Some(("Wireless Controller", Some(0x054c)))), LabelStyle::PlayStation);
        assert_eq!(auto.resolve(Some(("DualSense Wireless Controller", None))), LabelStyle::PlayStation);
        assert_eq!(auto.resolve(Some(("Pro Controller", Some(0x057e)))), LabelStyle::Nintendo);
        assert_eq!(auto.resolve(Some(("Xbox Series X Controller", Some(0x045e)))), LabelStyle::Xbox);
        assert_eq!(LabelStyle::PlayStation.resolve(Some(("Xbox", Some(0x045e)))), LabelStyle::PlayStation);
        // Every bindable control has a short label (the setup's columns
        // are 100 virtual pixels, 7 per character).
        for s in [LabelStyle::Xbox, LabelStyle::PlayStation, LabelStyle::Nintendo] {
            for &b in PADS {
                assert!(text_len(&pad_label(PadInput::Button(b), s)) <= 12, "{b:?}");
            }
            for &a in &AXES {
                for p in [false, true] {
                    assert!(pad_label(PadInput::Axis(a, p), s).len() <= 12);
                }
            }
        }
    }

    fn text_len(s: &str) -> usize {
        s.chars().count()
    }

    #[test]
    fn thresholds() {
        let o = PadOptions::default();
        assert_eq!((o.deadzone, o.trigger), (50, 30));
        // Left stick past the deadzone, or the d-pad: left, right, up, down.
        assert_eq!(o.directions(Vec2::new(-0.49, 0.0), Vec2::ZERO), [false; 4]);
        assert_eq!(o.directions(Vec2::new(-0.5, 0.0), Vec2::ZERO), [true, false, false, false]);
        assert_eq!(o.directions(Vec2::new(0.7, 0.8), Vec2::ZERO), [false, true, true, false]);
        assert_eq!(o.directions(Vec2::ZERO, Vec2::new(0.0, -1.0)), [false, false, false, true]);
        let tight = PadOptions { deadzone: 20, ..o };
        assert_eq!(tight.directions(Vec2::new(0.25, 0.0), Vec2::ZERO), [false, true, false, false]);
        // Triggers on Z axes use the trigger threshold, sticks the deadzone.
        assert_eq!(o.axis_threshold(GamepadAxis::RightZ), 0.3);
        assert_eq!(o.axis_threshold(GamepadAxis::RightStickX), 0.5);
        // Out-of-range values are clamped.
        let silly = PadOptions { deadzone: 0, trigger: 200, ..o };
        assert_eq!((silly.stick(), silly.trigger()), (0.1, 0.9));
        let mut c = Controls::default();
        assert!(c.read("pad.deadzone", "3") && c.opts.deadzone == DEADZONE_RANGE.0);
        assert!(c.read("pad.trigger", "x") && c.opts.trigger == 30);
    }

    #[test]
    fn old_settings_upgrade() {
        // A file from before layout 2: the old pad defaults, one edited table.
        let mut old = Controls { pads: [PAD_V1; 4], ..Controls::default() };
        old.pads[1][BRAKE] = Some(PadInput::Button(GamepadButton::LeftThumb));
        let mut t = String::new();
        old.write(&mut t);
        let t: String = t.lines().filter(|l| !l.starts_with("pad.layout") && !l.starts_with("pad.deadzone")).map(|l| format!("{l}\n")).collect();
        let mut c = Controls::default();
        for line in t.lines() {
            let (k, v) = line.split_once('=').unwrap();
            c.read(k.trim(), v.trim());
        }
        assert_eq!(c.opts.layout, 1);
        c.upgrade();
        assert_eq!(c.opts.layout, PAD_LAYOUT);
        assert_eq!(c.pads[0], PAD, "untouched table takes the new defaults");
        assert_eq!(c.pads[1][SHOOT_ALT], None, "edited table kept");
        assert_eq!(c.pads[1][BRAKE], Some(PadInput::Button(GamepadButton::LeftThumb)));
        // A current file stays as written.
        let mut cur = Controls::default();
        cur.pads[0][SHOOT_ALT] = None;
        cur.pads[0][REPEAT_MESSAGE] = None;
        let mut t = String::new();
        cur.write(&mut t);
        let mut d = Controls::default();
        for line in t.lines() {
            let (k, v) = line.split_once('=').unwrap();
            d.read(k.trim(), v.trim());
        }
        d.upgrade();
        assert_eq!(d.pads[0], PAD_V1);
        // Reset.
        d.reset_pads();
        d.keys[SHOOT] = None;
        d.reset_keys();
        assert_eq!(d, Controls::default());
    }

    #[test]
    fn menu_repeat() {
        let mut r = NavRepeat::default();
        let down = IVec2::new(0, -1);
        assert_eq!(r.step(down, 0.0), down, "pressed at once");
        assert_eq!(r.step(down, 0.2), IVec2::ZERO);
        assert_eq!(r.step(down, 0.39), IVec2::ZERO);
        assert_eq!(r.step(down, 0.4), down, "first repeat after the delay");
        assert_eq!(r.step(down, 0.45), IVec2::ZERO);
        assert_eq!(r.step(down, 0.5), down, "then at the repeat rate");
        assert_eq!(r.step(IVec2::ZERO, 0.6), IVec2::ZERO);
        assert_eq!(r.step(IVec2::new(1, 0), 0.61), IVec2::new(1, 0));
        // Adding a direction presses only the new one.
        assert_eq!(r.step(IVec2::new(1, 1), 0.62), IVec2::new(0, 1));
    }

    /// Bevy's gamepad input in a headless App: a pad connects, presses
    /// the d-pad, holds it, moves the stick; the menu keys follow.
    #[test]
    fn menu_input_from_pad_events() {
        use super::super::ui::Input;
        use bevy::input::gamepad::{GamepadConnection, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent};

        #[derive(Resource, Default)]
        struct Menu {
            now: f32,
            nav: Vec<(Entity, NavRepeat)>,
            input: Input,
        }
        fn read(pads: Query<(Entity, &Gamepad)>, axes: Res<PadAxes>, mut m: ResMut<Menu>) {
            let m = &mut *m;
            let mut input = Input::default();
            pad_menu_input(pads.iter(), &axes, &mut m.nav, m.now, &mut input);
            m.input = input;
        }
        let mut app = App::new();
        app.add_plugins(bevy::input::InputPlugin)
            .init_resource::<PadSlots>()
            .init_resource::<PadAxes>()
            .init_resource::<Menu>()
            .add_systems(PreUpdate, (assign_pads, track_pad_axes, apply_pad_options).after(bevy::input::InputSystems))
            .add_systems(Update, read);
        let pad = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(GamepadConnectionEvent::new(
            pad,
            GamepadConnection::Connected { name: "Test pad".into(), vendor_id: Some(0x054c), product_id: Some(1) },
        ));
        for _ in 0..GRACE_FRAMES {
            app.update();
        }
        assert_eq!(app.world().resource::<PadSlots>().first(), Some(pad));
        let frame = |app: &mut App, now: f32, ev: Option<RawGamepadEvent>| -> Input {
            app.world_mut().resource_mut::<Menu>().now = now;
            if let Some(ev) = ev {
                app.world_mut().write_message(ev);
            }
            app.update();
            app.world().resource::<Menu>().input.clone()
        };
        let button = |b, v| Some(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, b, v)));
        frame(&mut app, 0.9, None);
        let i = frame(&mut app, 1.0, button(GamepadButton::DPadDown, 1.0));
        assert!(i.down && !i.up && !i.fire);
        assert!(!frame(&mut app, 1.1, None).down, "held: no repeat yet");
        assert!(frame(&mut app, 1.4, None).down, "held: repeats");
        assert!(!frame(&mut app, 1.45, button(GamepadButton::DPadDown, 0.0)).down);
        let i = frame(&mut app, 1.5, button(GamepadButton::South, 1.0));
        assert!(i.fire && i.enter && i.pad == vec![GamepadButton::South]);
        let i = frame(&mut app, 1.6, button(GamepadButton::Start, 1.0));
        assert!(i.fire && i.start);
        assert!(frame(&mut app, 1.7, button(GamepadButton::East, 1.0)).back);
        // The stick: past the deadzone only.
        let stick = |v| Some(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(pad, GamepadAxis::LeftStickX, v)));
        assert!(!frame(&mut app, 2.0, stick(0.3)).right);
        let i = frame(&mut app, 2.1, stick(0.8));
        assert!(i.right);
        assert_eq!(i.pad_axes, vec![PadInput::Axis(GamepadAxis::LeftStickX, true)]);
        // A trigger past the 30 % threshold counts as pressed (Bevy's
        // default press point is 75 %).
        frame(&mut app, 2.2, button(GamepadButton::RightTrigger2, 0.4));
        let g = app.world().get::<Gamepad>(pad).unwrap();
        assert!(g.pressed(GamepadButton::RightTrigger2));
        // Unplugged: reported once.
        app.world_mut().write_message(GamepadConnectionEvent::new(pad, GamepadConnection::Disconnected));
        app.update();
        let mut slots = app.world_mut().resource_mut::<PadSlots>();
        assert_eq!(slots.first(), None);
        assert!(slots.take_lost());
        assert!(!slots.take_lost());
    }

    /// A headless App with Bevy's gamepad input, the pad trackers and a
    /// menu reader, and one pad connected.
    mod rig {
        use super::super::*;
        use crate::frontend::ui::Input;
        use bevy::input::gamepad::{GamepadConnection, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent};

        #[derive(Resource, Default)]
        pub struct Menu {
            pub now: f32,
            pub nav: Vec<(Entity, NavRepeat)>,
            pub input: Input,
            /// The single-player directions (as `sim::people` reads them).
            pub steer: [bool; 4],
        }

        fn read(pads: Query<(Entity, &Gamepad)>, axes: Res<PadAxes>, mut m: ResMut<Menu>) {
            let m = &mut *m;
            let mut input = Input::default();
            pad_menu_input(pads.iter(), &axes, &mut m.nav, m.now, &mut input);
            m.input = input;
            m.steer = pads.iter().next().map_or([false; 4], |(e, g)| axes.state(e, g).directions());
        }

        pub fn app() -> (App, Entity) {
            app_with(|_| Vec::new())
        }

        /// [`app`], with events the pad reports as it connects.
        pub fn app_with(initial: impl Fn(Entity) -> Vec<RawGamepadEvent>) -> (App, Entity) {
            let mut app = App::new();
            app.add_plugins(bevy::input::InputPlugin)
                .init_resource::<PadSlots>()
                .init_resource::<PadAxes>()
                .init_resource::<Menu>()
                .add_systems(PreUpdate, (assign_pads, track_pad_axes, apply_pad_options).after(bevy::input::InputSystems))
                .add_systems(Update, read);
            let pad = app.world_mut().spawn_empty().id();
            app.world_mut().write_message(GamepadConnectionEvent::new(
                pad,
                GamepadConnection::Connected {
                    name: "Xbox 360 Controller for Windows".into(),
                    vendor_id: Some(0x28de),
                    product_id: Some(0x11ff),
                },
            ));
            app.update();
            for ev in initial(pad) {
                app.world_mut().write_message(ev);
            }
            for _ in 0..GRACE_FRAMES {
                app.update();
            }
            (app, pad)
        }

        pub fn frame(app: &mut App, now: f32, evs: &[RawGamepadEvent]) -> Input {
            app.world_mut().resource_mut::<Menu>().now = now;
            for ev in evs {
                app.world_mut().write_message(ev.clone());
            }
            app.update();
            app.world().resource::<Menu>().input.clone()
        }

        pub fn steer(app: &App) -> [bool; 4] {
            app.world().resource::<Menu>().steer
        }

        pub fn axis(pad: Entity, a: GamepadAxis, v: f32) -> RawGamepadEvent {
            RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(pad, a, v))
        }

        pub fn button(pad: Entity, b: GamepadButton, v: f32) -> RawGamepadEvent {
            RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, b, v))
        }

        pub fn moved(i: &Input) -> bool {
            i.up || i.down || i.left || i.right
        }

        pub fn held(app: &App, pad: Entity, b: PadInput) -> bool {
            let g = app.world().get::<Gamepad>(pad).unwrap();
            pad_held(&app.world().resource::<PadAxes>().state(pad, g), Some(b))
        }
    }

    /// A pad whose triggers rest at -1 (reported as LeftZ / RightZ on the
    /// full range) and whose sticks drift a little: the menu stays put
    /// for 10 s, the player doesn't steer, the redefine prompt sees
    /// nothing; real pushes still navigate and the triggers still pull.
    #[test]
    fn resting_triggers_and_drift_dont_navigate() {
        use rig::*;
        let (mut app, pad) = app();
        let rest = [
            axis(pad, GamepadAxis::LeftZ, -1.0),
            axis(pad, GamepadAxis::RightZ, -1.0),
            axis(pad, GamepadAxis::LeftStickX, 0.09),
            axis(pad, GamepadAxis::LeftStickY, -0.12),
            axis(pad, GamepadAxis::RightStickY, 0.2),
        ];
        let mut t = 0.0;
        let mut first = true;
        while t < 10.0 {
            // The drift wobbles a little, as a real stick does.
            let wobble = [axis(pad, GamepadAxis::LeftStickX, 0.09 + 0.02 * (t * 7.0f32).sin())];
            let i = frame(&mut app, t, if first { &rest } else { &wobble });
            first = false;
            assert!(!moved(&i) && i.pad_axes.is_empty() && i.pad.is_empty(), "at rest at {t}: {i:?}");
            assert_eq!(steer(&app), [false; 4]);
            t += 0.05;
        }
        // The triggers read 0..1: neither end of the axis is held.
        assert!(!held(&app, pad, PadInput::Axis(GamepadAxis::RightZ, true)));
        assert!(!held(&app, pad, PadInput::Axis(GamepadAxis::RightZ, false)));
        assert!(!held(&app, pad, PadInput::Axis(GamepadAxis::LeftZ, false)));
        // A real push still navigates, at once and with repeat.
        let i = frame(&mut app, 10.0, &[axis(pad, GamepadAxis::LeftStickY, -0.9)]);
        assert!(i.down && !i.up);
        assert_eq!(steer(&app), [false, false, false, true]);
        assert!(frame(&mut app, 10.45, &[]).down, "held: repeats");
        assert!(!moved(&frame(&mut app, 10.5, &[axis(pad, GamepadAxis::LeftStickY, -0.1)])));
        assert!(frame(&mut app, 10.6, &[button(pad, GamepadButton::DPadRight, 1.0)]).right);
        frame(&mut app, 10.7, &[button(pad, GamepadButton::DPadRight, 0.0)]);
        // Pulling a -1..1 trigger all the way counts (an Axis(RightZ+) binding).
        let i = frame(&mut app, 10.8, &[axis(pad, GamepadAxis::RightZ, 1.0)]);
        assert_eq!(i.pad_axes, vec![PadInput::Axis(GamepadAxis::RightZ, true)]);
        assert!(!moved(&i), "triggers never move the menu");
        assert!(held(&app, pad, PadInput::Axis(GamepadAxis::RightZ, true)));
    }

    /// A trigger reported as a button and as a Z axis: the redefine prompt
    /// gets the button only.
    #[test]
    fn trigger_button_and_axis_counted_once() {
        use rig::*;
        let (mut app, pad) = app();
        frame(&mut app, 0.0, &[]);
        let i = frame(
            &mut app,
            0.05,
            &[button(pad, GamepadButton::RightTrigger2, 1.0), axis(pad, GamepadAxis::RightZ, 1.0)],
        );
        assert_eq!(i.pad, vec![GamepadButton::RightTrigger2]);
        assert!(i.pad_axes.is_empty());
    }

    /// A stick resting far off centre from the start, and d-pad buttons
    /// stuck on (lost release events, as with Steam Input): no runaway.
    #[test]
    fn stuck_directions_dont_navigate() {
        use rig::*;
        // Off centre since the pad came: ignored until let go.
        let (mut app, pad) = app_with(|pad| vec![axis(pad, GamepadAxis::LeftStickX, 0.8)]);
        for k in 1..100 {
            assert!(!moved(&frame(&mut app, k as f32 * 0.05, &[])));
            assert_eq!(steer(&app), [false; 4]);
        }
        frame(&mut app, 5.0, &[axis(pad, GamepadAxis::LeftStickX, 0.0)]);
        assert!(frame(&mut app, 5.05, &[axis(pad, GamepadAxis::LeftStickX, 0.8)]).right, "once released, it works");
        frame(&mut app, 5.1, &[axis(pad, GamepadAxis::LeftStickX, 0.0)]);
        // Left and right both reported held (the releases never came).
        frame(&mut app, 5.2, &[button(pad, GamepadButton::DPadRight, 1.0)]);
        frame(&mut app, 5.25, &[button(pad, GamepadButton::DPadLeft, 1.0)]);
        for k in 0..100 {
            let i = frame(&mut app, 5.3 + k as f32 * 0.05, &[]);
            assert!(!i.left && !i.right, "opposite directions cancel ({k})");
            assert_eq!(steer(&app), [false; 4]);
        }
        // The other axis still works meanwhile.
        assert!(frame(&mut app, 10.4, &[button(pad, GamepadButton::DPadUp, 1.0)]).up);
    }

    /// A direction held when the menu opens moves nothing until it is let
    /// go (a stuck or still-held direction can't run the menu away).
    #[test]
    fn held_when_menu_opens_is_ignored() {
        use rig::*;
        let (mut app, pad) = app();
        frame(&mut app, 0.0, &[]);
        assert!(frame(&mut app, 0.05, &[button(pad, GamepadButton::DPadDown, 1.0)]).down);
        frame(&mut app, 0.1, &[]);
        // The menu closes (a level runs) and opens again with Down still held.
        for k in 0..60 {
            assert!(!frame(&mut app, 5.0 + k as f32 * 0.05, &[]).down, "held since the menu opened ({k})");
        }
        // In game, the held direction still steers.
        assert_eq!(steer(&app), [false, false, false, true]);
        frame(&mut app, 8.0, &[button(pad, GamepadButton::DPadDown, 0.0)]);
        assert!(frame(&mut app, 8.05, &[button(pad, GamepadButton::DPadDown, 1.0)]).down);
    }
}
