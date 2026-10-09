# Controls: keyboard and gamepads

The game's actions are the original's eleven control-setup rows (phrases
2457..2467, `DAT_012a63fc`; docs/re/frontend_menus.md §6), a twelfth row
added by the port (Horn, below), plus the fixed keys: digits 0..7 pick a
weapon, Esc pauses. Code:
`crates/payback/src/frontend/bindings.rs` (tables, options, pad input),
`frontend/misc.rs` (`ControlSetup`), `sim/people.rs::read_keyboard` (single
player), `multiplayer.rs::read_devices` (multiplayer).

The keyboard defaults are the Mac's and are unchanged (frontend_menus.md §6).
The gamepad support below is a port addition: the Amiga read PSX/CD32 pads
and the Mac any HID device, both with a plain button table.

## Default gamepad layout (layout 3)

Bevy names buttons by position (`South` is the bottom face button), so the
same table serves every pad; the control setup shows each pad's own names.
The same buttons work on foot and in vehicles.

| Action | Bevy control | Xbox | PlayStation | Switch |
|---|---|---|---|---|
| Accelerate / walk forward | `South` | A | Cross | B |
| Reverse / walk back | `West` | X | Square | Y |
| Steer / turn left, right | `DPadLeft` / `DPadRight` (and the left stick, fixed) | D-pad, LS | D-pad, LS | D-pad, LS |
| Shoot (punch unarmed; horn or vehicle gun when driving) | `RightTrigger2` | RT | R2 | ZR |
| Shoot (second slot) | unbound | – | – | – |
| Get in / out of a vehicle | `North` | Y | Triangle | X |
| Brake (handbrake) | `RightTrigger` | RB | R1 | R |
| Next weapon | `LeftTrigger` | LB | L1 | L |
| Previous weapon | `LeftTrigger2` | LT | L2 | ZL |
| Repeat last message | `Select` | Back / View | Share / Create | − |
| Horn (vehicles only, port addition) | `East` | B | Circle | A |
| Pause (fixed) | `Start` | Start / Menu | Options | + |

Choices: the weapon steps sit on LB / LT because the left index finger
reaches them while the left thumb steers (the d-pad would mean letting go
of the stick); LB, the quicker press, steps forward. Shoot's second slot
is left free on purpose: RT is the one fire button, and B is the horn.
D-pad up / down and the stick clicks are free for the player to use.

Fixed, not in the table: the left stick's left / right and the d-pad's
left / right always steer, on foot and in vehicles. The stick's (and
d-pad's) up / down do **not** drive by default; the gamepad option
"Left stick forward/back" (`pad.stick_drive`, default Off) brings back the
original's joystick directions (up accelerates, down reverses, stick and
d-pad alike). The menus always navigate with all four directions. The
control model stays the original's (turn and move forward/back, digital).
Weapon slots 0..7 stay on the keyboard digits.

### Horn

The original has no horn key: in a vehicle the shoot key honks
(`do_Other` @ 10146280 state 2 reads the input word's shoot bit; phrase
2461 "Shoot/horn"; docs/re/vehicles.md §6.2). The port adds a Horn row
(`bindings::HORN`) that sets the same per-tick shoot bit
(`PlayerInput::fire`), but only while the player is in a vehicle; on foot
it does nothing. So Horn is exactly what Shoot is when driving: it honks,
and in a vehicle with a weapon (tank gun, bomb truck's RC car) it fires
that, as Shoot does there. Shoot keeps honking in vehicles, as in the
original. No new sim state: replays record the shoot bit as before.
Default keys: B / Circle / A on pads, `H` on the single-player keyboard
(free in the Mac map), unbound in the four multiplayer key sets (they
share one keyboard).

## Menus

Every front-end screen, the pause menu, the cheat menu and the graphics
options take the pad: d-pad or left stick moves (held directions repeat
after 0.4 s, every 0.1 s), A / Cross (or Start) confirms, B / Circle goes
back. In a level, Start opens the pause menu and Start or B resumes.
The keyboard keeps its original behaviour (no auto-repeat). Triggers,
the right stick and other axes never move the menus, and neither does
the mouse (the original menus were keyboard / joystick only). A
direction already held when a menu opens is ignored until it is let go
(`bindings::NavRepeat`), so a stuck direction can't run a menu away.

## Thresholds (the game is digital)

The original polled switches; the sim takes on/off controls per tick
(`PlayerInput`), so analogue inputs are thresholded:

| Option | Default | Range | Applies to |
|---|---|---|---|
| Stick deadzone | 50 % | 10–90 % | left stick steering, stick-end bindings, menu navigation |
| Trigger threshold | 30 % | 5–90 % | LT/RT as buttons (written into Bevy's `GamepadSettings`, whose default press point is 75 %) and triggers reported as `LeftZ`/`RightZ` axes |

The Mac counted an axis as pressed about 60 % of the way out
(`IsHIDControlPressed` @ 10158300).

## Misbehaving pads (guards)

Real pads don't all rest at zero, and some events get lost; the
guards (`bindings::track_pad_axes`, `PadState::directions`, used by
the menus, single player and multiplayer alike):

- Whatever is held during the first 30 frames after a pad connects
  (when the platform reports its initial state) is taken as rest: that
  axis end or direction counts only once it has been let go. A stick
  resting off centre or a button reported held from the start can't
  steer or navigate.
- Triggers reported as `LeftZ` / `RightZ` on the full -1..1 range (rest
  at -1, seen once a value below zero arrives) are read as (v + 1) / 2.
- Opposite directions held together (d-pad Left + Right, Up + Down)
  cancel: a d-pad can't press both, but a pad whose release events are
  lost reports exactly that.
- Small stick drift stays inside the deadzone (10 % minimum).
- A trigger that reports as a button and as a Z axis is bound as the
  button (the redefine prompt doesn't see the axis while the button
  moves).

### Lost button releases (Steam Controller, Steam Input)

The first real-hardware test, a Steam Controller launched through Steam
(Steam Input presents it as "Xbox 360 Controller for Windows", vendor
28de product 11ff), ran the menus away: `payback.log` showed
`DPadRight = 1` and then `DPadLeft = 1` with no release for either, after
which Bevy kept both pressed for good; a release of A was missing too.
Bevy reads pads through gilrs, which on Windows polls
Windows.Gaming.Input and turns differences between readings into
events, and those events were incomplete for this pad (Bevy itself
follows the events faithfully). On Windows the port therefore cross-checks
XInput-style pads (vendor 045e / 28de, or "Xbox" / "XInput" in the name)
against XInput's polled state every frame (`frontend/xinput.rs`): with one
such pad and one XInput slot in use, its buttons and LT / RT follow the
slot (a lost press or release is put right, logged as `xinput: ... (the
event was lost)`); with several, a button held on no XInput slot is
released. The `--pad-sim` pad is left alone.

Steam's desktop layout (used when the game is not launched through Steam)
sends keys and the mouse instead: arrow keys and Enter move the menus as
the keyboard does, the mouse is ignored.

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
  the pad's USB vendor or name: Sony 054c, Nintendo 057e, else Xbox), left
  stick forward/back (Off / On), reset gamepad buttons, reset keyboard keys,
  and the connected pads by number.
- Every action, Horn included, takes any button, trigger or stick end, on
  each gamepad column and each keyboard column; with the Horn row selected
  the hint line reads "Horn: works in vehicles only".
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
pad.stick_drive = off    # on: stick / d-pad up and down drive
pad.layout = 3
```

`key.horn`, `pad.horn`, `pad<n>.horn` and `mp<n>.key.horn` hold the Horn
row. Upgrades at start-up (`bindings::upgrade_pad_layout`,
`Controls::upgrade`): a file with `pad.layout` below 3 (or none, layout 1)
has its pad tables compared with the old defaults in the original 11
actions; tables still equal to layout 1 (no second shoot or
repeat-message button) or layout 2 (RT/LT drive, A/X shoot, B brake,
RB/LB weapons) take layout 3. Edited tables are kept as they were: their
horn goes unbound if B already has an action there, and if gamepad 1's
table is an edited one, "Left stick forward/back" is turned on so its
stick and d-pad keep driving. The first save writes `pad.layout = 3`.

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

## Diagnostics

`--pad-debug`, `PAYBACK_PAD_DEBUG=1`, or a file named `pad-debug` next to
the exe (for launches without a console, such as a Steam shortcut) log
to `payback.log` next to the exe (the previous run's is kept as
`payback.log.1`) and to the console: pad connections (name, vendor /
product id), every raw button and axis change, keyboard keys, mouse
wheel / buttons and motion (summed per second), XInput slots and
corrections, every menu navigation step with its source (keyboard, or
the pad with its stick and d-pad values), and once a second each pad's
axes away from rest and buttons held (`bindings::pad_debug`).

## Testing without a pad

`--pad-sim "<secs>:<control>[/<hold>],..."` connects a virtual pad and
presses controls at real-time offsets (`DPadDown`, `South`, `LeftStickY-`,
`disconnect`, `connect`); it is then the only pad that gets a number, so a
real pad on the test machine can't take gamepad 1. With `--pad-debug` the
log also shows player 1's controls as the sim gets them (`player 1 input
[accel] on foot, at ...`) and `player 1 horn sounds`. Example: `--map 0
--park 1 --pad-debug --pad-sim "10:South/2,13:West/2,16:LeftStickY+/2,
19:RightTrigger2/0.3,21:East/0.5,23:North,28:South/2,30.5:RightTrigger/1,
32:East/1"` walks forward and back, ignores stick up, punches, gets into
the parked car, drives, brakes and honks; `--pad-sim-name`, `--pad-sim-vendor 054c` change
what it reports. `--frontend-screen controls-pad` / `controls-pad-options`
open the control setup on gamepad 1's column / the options page.
`--screenshot-delay <secs>` takes the `--screenshot` later (e.g. a menu
after 10 s idle with `--pad-sim "1.5:DPadLeft/100,1.5:DPadRight/100"`).
