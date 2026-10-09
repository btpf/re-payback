//! XInput cross-check of gamepad buttons (Windows; port addition,
//! docs/controls.md "Lost button releases").
//!
//! Bevy reads pads through gilrs, which on Windows polls
//! Windows.Gaming.Input and turns the differences between readings into
//! press / release events. With a Steam Controller under Steam Input (a
//! virtual "Xbox 360 Controller for Windows", vendor 28de product 11ff)
//! some of those events never arrive: d-pad Left and Right both stayed
//! pressed for good, a release of A went missing, and the menus ran away
//! on the stuck direction. XInput is a polled state with no events to
//! lose, so every frame the XInput-style pads' buttons are set from it:
//! with one such pad and one XInput slot in use, the pad's buttons and
//! LT / RT follow that slot (presses and releases); with several (the
//! slot of each pad is not known), a button held on no slot is released.

use bevy::input::gamepad::{GamepadInput, GamepadSettings};
use bevy::prelude::*;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct XGamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XState {
    packet: u32,
    pad: XGamepad,
}

type GetState = unsafe extern "system" fn(u32, *mut XState) -> u32;

unsafe extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut core::ffi::c_void;
    fn GetProcAddress(module: *mut core::ffi::c_void, name: *const u8) -> *mut core::ffi::c_void;
}

/// `XInputGetState` from xinput1_4.dll (Windows 8+) or xinput9_1_0.dll.
fn get_state_fn() -> Option<GetState> {
    static F: std::sync::OnceLock<Option<GetState>> = std::sync::OnceLock::new();
    *F.get_or_init(|| {
        for dll in [&b"xinput1_4.dll\0"[..], b"xinput9_1_0.dll\0"] {
            // SAFETY: NUL-terminated names; the module stays loaded for
            // the process, and the symbol has the documented signature.
            unsafe {
                let m = LoadLibraryA(dll.as_ptr());
                if m.is_null() {
                    continue;
                }
                let f = GetProcAddress(m, b"XInputGetState\0".as_ptr());
                if !f.is_null() {
                    return Some(std::mem::transmute::<*mut core::ffi::c_void, GetState>(f));
                }
            }
        }
        None
    })
}

fn read_slot(f: GetState, slot: u32) -> Option<XGamepad> {
    let mut s = XState::default();
    // SAFETY: `s` is a valid XINPUT_STATE.
    (unsafe { f(slot, &mut s) } == 0).then_some(s.pad)
}

/// XInput button bits and the Bevy buttons they are.
const BUTTONS: [(u16, GamepadButton); 14] = [
    (0x0001, GamepadButton::DPadUp),
    (0x0002, GamepadButton::DPadDown),
    (0x0004, GamepadButton::DPadLeft),
    (0x0008, GamepadButton::DPadRight),
    (0x0010, GamepadButton::Start),
    (0x0020, GamepadButton::Select),
    (0x0040, GamepadButton::LeftThumb),
    (0x0080, GamepadButton::RightThumb),
    (0x0100, GamepadButton::LeftTrigger),
    (0x0200, GamepadButton::RightTrigger),
    (0x1000, GamepadButton::South),
    (0x2000, GamepadButton::East),
    (0x4000, GamepadButton::West),
    (0x8000, GamepadButton::North),
];

/// A pad that Windows reports through XInput as well: an Xbox pad, or a
/// Steam Input virtual pad (Microsoft 045e, Valve 28de, or "Xbox" /
/// "XInput" in the name).
fn xinput_like(g: &Gamepad, name: Option<&Name>) -> bool {
    let n = name.map(|n| n.as_str().to_ascii_lowercase()).unwrap_or_default();
    matches!(g.vendor_id(), Some(0x045e | 0x28de)) || n.contains("xbox") || n.contains("xinput")
}

/// The pressed state of a button and the XInput state of it.
fn button_state(x: &XGamepad, b: GamepadButton, gs: &GamepadSettings, was: bool) -> Option<(bool, f32)> {
    let analog = |v: u8| {
        let v = v as f32 / 255.0;
        let s = gs.get_button_settings(b);
        let on = if was { !s.is_released(v) } else { s.is_pressed(v) };
        (on, v)
    };
    match b {
        GamepadButton::LeftTrigger2 => Some(analog(x.left_trigger)),
        GamepadButton::RightTrigger2 => Some(analog(x.right_trigger)),
        b => BUTTONS.iter().find(|p| p.1 == b).map(|&(bit, _)| {
            let on = x.buttons & bit != 0;
            (on, on as u8 as f32)
        }),
    }
}

/// Polled slots: the connected ones every frame, the others once a
/// second (a call on an empty slot is slow).
#[derive(Default)]
pub struct Slots {
    pads: [Option<XGamepad>; 4],
    rescan: f32,
}

/// Set the XInput-style pads' buttons from XInput (`PreUpdate`, after
/// Bevy's gamepad input, before the pad readers).
pub fn sync_buttons(
    time: Res<Time<Real>>,
    mut pads: Query<(Entity, &mut Gamepad, &GamepadSettings, Option<&Name>)>,
    mut slots: Local<Slots>,
) {
    // The `--pad-sim` virtual pad has no XInput side.
    let mut ours: Vec<_> = pads.iter_mut().filter(|(_, g, _, n)| xinput_like(g, *n)).collect();
    if ours.is_empty() || crate::arg("--pad-sim").is_some() {
        return;
    }
    let Some(f) = get_state_fn() else { return };
    let now = time.elapsed_secs();
    let rescan = now >= slots.rescan;
    if rescan {
        slots.rescan = now + 1.0;
    }
    for i in 0..4 {
        if slots.pads[i].is_some() || rescan {
            let before = slots.pads[i];
            slots.pads[i] = read_slot(f, i as u32);
            if before.is_some() != slots.pads[i].is_some() {
                super::bindings::pad_log(format_args!(
                    "xinput slot {i} {}",
                    if slots.pads[i].is_some() { "connected" } else { "gone" }
                ));
            }
        }
    }
    let connected: Vec<XGamepad> = slots.pads.iter().flatten().copied().collect();
    if connected.is_empty() {
        return;
    }
    let exact = ours.len() == 1 && connected.len() == 1;
    let all = [
        BUTTONS.map(|b| b.1).as_slice(),
        &[GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2],
    ]
    .concat();
    ours.sort_by_key(|p| p.0.index_u32());
    for (e, g, gs, _) in &mut ours {
        for &b in &all {
            let was = g.pressed(b);
            if exact {
                let Some((on, v)) = button_state(&connected[0], b, gs, was) else { continue };
                if on != was {
                    super::bindings::pad_log(format_args!(
                        "xinput: pad {e} {b:?} {} (the event was lost)",
                        if on { "pressed" } else { "released" }
                    ));
                    if on {
                        g.digital_mut().press(b);
                    } else {
                        g.digital_mut().release(b);
                    }
                }
                if g.get(b).is_some() || v != 0.0 {
                    g.analog_mut().set(GamepadInput::Button(b), v);
                }
            } else if was && !connected.iter().any(|x| button_state(x, b, gs, true).is_some_and(|s| s.0)) {
                super::bindings::pad_log(format_args!("xinput: pad {e} {b:?} released (held on no XInput slot)"));
                g.digital_mut().release(b);
                g.analog_mut().set(GamepadInput::Button(b), 0.0);
            }
        }
    }
}
