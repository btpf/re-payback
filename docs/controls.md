# Controls: keyboard and gamepads

The game's actions are the original's eleven control-setup rows (phrases
2457..2467, `DAT_012a63fc`; docs/re/frontend_menus.md §6), plus the fixed
keys: digits 0..7 pick a weapon, Esc pauses. Code:
`crates/payback/src/frontend/bindings.rs` (tables, options, pad input),
`frontend/misc.rs` (`ControlSetup`), `sim/people.rs::read_keyboard` (single
player), `multiplayer.rs::read_devices` (multiplayer).

The keyboard defaults are the Mac's and are unchanged (frontend_menus.md §6).
The gamepad support below is a port addition: the Amiga read PSX/CD32 pads
and the Mac any HID device, both with a plain button table.

## Default gamepad layout (layout 2)

Bevy names buttons by position (`South` is the bottom face button), so the
same table serves every pad; the control setup shows each pad's own names.

| Action | Bevy control | Xbox | PlayStation | Switch |
|---|---|---|---|---|
| Accelerate / walk forward | `RightTrigger2` | RT | R2 | ZR |
| Reverse / walk back | `LeftTrigger2` | LT | L2 | ZL |
| Steer / turn left, right | `DPadLeft` / `DPadRight` | D-pad | D-pad | D-pad |
| Shoot | `South` | A | Cross | B |
| Shoot (second button) | `West` | X | Square | Y |
| Get in / out of a vehicle | `North` | Y | Triangle | X |
| Brake | `East` | B | Circle | A |
| Next weapon | `RightTrigger` | RB | R1 | R |
| Previous weapon | `LeftTrigger` | LB | L1 | L |
| Repeat last message | `Select` | Back / View | Share / Create | − |
| Pause (fixed) | `Start` | Start / Menu | Options | + |

Fixed, not in the table (as the original's joystick directions): the left
stick and the d-pad always steer (left/right) and accelerate/reverse
(up/down), on foot and in vehicles alike; the control model stays the
original's (turn and move forward/back, digital). Weapon slots 0..7 stay on
the keyboard digits; on a pad, LB/RB step through the carried weapons.

## Menus

Every front-end screen, the pause menu, the cheat menu and the graphics
options take the pad: d-pad or left stick moves (held directions repeat
after 0.4 s, every 0.1 s), A / Cross (or Start) confirms, B / Circle goes
back. In a level, Start opens the pause menu and Start or B resumes.
The keyboard keeps its original behaviour (no auto-repeat).

## Thresholds (the game is digital)

The original polled switches; the sim takes on/off controls per tick
(`PlayerInput`), so analogue inputs are thresholded:

| Option | Default | Range | Applies to |
|---|---|---|---|
| Stick deadzone | 50 % | 10–90 % | left stick steering, stick-end bindings, menu navigation |
| Trigger threshold | 30 % | 5–90 % | LT/RT as buttons (written into Bevy's `GamepadSettings`, whose default press point is 75 %) and triggers reported as `LeftZ`/`RightZ` axes |

The Mac counted an axis as pressed about 60 % of the way out
(`IsHIDControlPressed` @ 10158300).

## Control setup (in game)

Main menu → Control setup: columns for the keyboard, gamepads 1–4 and the
four multiplayer keyboard key sets. With a pad:

- A on a binding opens the prompt; the next button, trigger or stick end
  (pushed past the deadzone / threshold) is bound. Start cancels the prompt
  (so Start itself can't be bound; it pauses), Esc too; 0 on the keyboard
  clears.
- X / Square (or Backspace) clears the selected binding.
- "Gamepad options" (between the last row and Done): stick deadzone, trigger
  threshold, button labels (Auto / Xbox / PlayStation / Switch; Auto picks by
  the pad's USB vendor or name: Sony 054c, Nintendo 057e, else Xbox), reset
  gamepad buttons, reset keyboard keys, and the connected pads by number.
- Done saves to `Payback.cfg`, Cancel (or B) drops the edits.

Labels look like "A", "RT", "LS Left", "RS Up", "D-pad Up", "Cross", "ZR".

## Settings file

Unchanged keys: `pad.<action>` (gamepad 1), `pad2..4.<action>`, values are
Bevy names (`South`, `RightTrigger2`, `LeftStickX-`). New keys, written after
the pad tables:

```
pad.deadzone = 50
pad.trigger = 30
pad.labels = auto        # auto | xbox | playstation | nintendo
pad.layout = 2
```

A file without `pad.layout` predates the modern defaults: pad tables still
equal to the old default (no second shoot or repeat-message button) take the
new one at start-up (`bindings::upgrade_pad_layout`); edited tables are kept.

## Gamepad numbers and hot-plugging

Pads keep their number (1–4) across unplugging (`PadSlots`); a new pad takes
the first free number. Single player reads the keyboard and the
lowest-numbered connected pad together (the Mac's "keyboard + device 1").
Multiplayer offers "Joystick n" for connected pads only. Unplugging a pad
during a level opens the pause menu (not during replay playback); connects
and disconnects are logged (`Gamepad 1 connected: <name> (vendor ...)`).

Rumble: not used (no original event maps to it cleanly).

## Determinism

Pads are read in the same per-frame input systems as the keyboard into
`PlayerInputs`, with the per-tick latch (PL-9); replays record and replay
those, so thresholds and bindings never reach the sim directly.

## Testing without a pad

`--pad-sim "<secs>:<control>[/<hold>],..."` connects a virtual pad and
presses controls at real-time offsets (`DPadDown`, `South`, `LeftStickY-`,
`disconnect`, `connect`); `--pad-sim-name`, `--pad-sim-vendor 054c` change
what it reports. `--frontend-screen controls-pad` / `controls-pad-options`
open the control setup on gamepad 1's column / the options page.
