# Mission script system and script API

Status markers: **CONFIRMED** = read directly from code; **INFERRED** = deduced from
context/naming, not proven. Addresses: `68k` = Update 7 `Payback.exe` flattened at
0x10000 (`analysis/ghidra_68k/`); `OS4` = PaybackOS4 ELF (`analysis/ghidra_os4/`);
`CD` = original CD `Payback.exe` (Jan 2001, not decompiled; strings only);
`scriptNN` = `Maps/NN.script` flattened at 0x10000 (`analysis/ghidra_scriptNN/`).

## 1. Two generations of the mission system

### 1.1 CD release (Jan 2001): scripts are separate AmigaOS processes — CONFIRMED

The CD `Payback.exe` starts the level script with `Execute()` of the string

```
cd "%s"
run >nil: /maps/%d%d.script          (city digit, level digit)
```

so each `Maps/<city><level>.script` is a normal AmigaOS CLI program running as its own
task. It is a SAS/C-style executable (startup at `scriptNN:0x10000`, `main` reached
through the jump table in the last code hunk; script00 `main` = `0x115ba`).
Game and script talk through Exec message ports (CONFIRMED from strings in both binaries):

| Port | Owner | Purpose |
|---|---|---|
| `pbmain` | game | script → game commands |
| `pbscript` | script | game → script events / replies |
| `pbmain2`, `pbcdaudio` | game / CD_Audio_Handler | CD audio helper (unrelated) |

Script side (script00 addresses; the other five scripts are the same library):

| Addr | Role |
|---|---|
| `0x10554` | `InitCommunication()`: clear pending list, `CreatePort("pbscript")`, `SendConnection("pbmain")` |
| `0x10146` | `Send(cmd, a0..a9)`: `AllocMem(0x3e)`, `mn_Length=0x3e`, `+0x14 = (u16)cmd`, `+0x16..+0x3c = 10 × u32 args`, `PutMsg(pbmain)` (no reply wait) |
| `0x103e4` | `WaitFor(type)`: returns the first queued/received message whose `+0x14 == type` (other messages are parked in a 100-entry list) |
| `0x10bf8` | `GetEvent()` = `WaitFor(<event type>)`; returns the event number (`+0x16`) |
| `0x10578` | shutdown: wait for message type 2 ("goodbye"), flush, delete port |
| `0x105a0..0x10f3e` | one stub per command (codes 0x03..0x36), see §6 |

The script `main` is a single blocking loop: `for(;;){ ev = GetEvent(); switch(mission_state){ case n: switch(ev) ... } }`.
The game's dispatcher was `DoScript()` (CD strings: `DoScript(): Unexpectedly recieved goodbye message`,
`DoScript: COMM_ADDSOUND - illegal soundref...`, `COMM_DELETESOUND`).

### 1.2 Update 7 (our target) and the OS4 port: scripts compiled into the game — CONFIRMED

`Payback.exe` from Update 7 contains no `pbmain`/`pbscript`/`.script` strings. The six
scripts were turned into C functions inside the executable (68k hunk 4, `0xb2000–0xca4c8`;
OS4 `0x010cd424–0x010f4708`). **The shipped `Maps/*.script` files are dead data** in
Update 7: they are the January 2001 originals and are never loaded. They are still a
useful cross-reference (same logic, same event numbers; coordinates are passed as 16-bit
words in the CD builds where Update 7 uses full 32-bit values, e.g. script `0xb400,0xe000`
→ Update 7 `0xb400,0x2e000`), but **mission documentation must follow the Update 7 code**,
which differs in places (e.g. extra missions/phones, `0x898` easter egg).

The former IPC calls became direct calls (`AddSound_script`, `DeleteSound_script` keep
their old names in error strings). The blocking `GetEvent()` loop became a callback:
the game calls the level's event handler with one event number at a time.

## 2. Entry points (Update 7)

| 68k | OS4 | Name | Semantics |
|---|---|---|---|
| `0xca25c` | `0x010f42xx` (inlined) | `Mission_Init(city, level)` | CONFIRMED. Stores `city`→`0x2fba58`, `level`→`0x2fba5c`; clears sound refs (`0xb4156`), tracked-trigger count `0x11a350=0`, `0x11a354=-1` (current phone mission), `0x2fba44/4c/50=0`, `0x2fb220=1` (first-phones flag); fills the 20 phone slots (`0x2fb158[20]=-1`, `0x2fb1f8[20]=0`); calls the level init (table below). |
| `0xca322` | — | `Mission_Shutdown()` | CONFIRMED. Level 01 only: `Actor_Release(0x835)`, `KillTrackedTrigger(0x834/0x835)`; then stops all script sounds (`0xb4176`). |
| `0xca368` | `0x010f45cc` | `Mission_Event(ev)` | CONFIRMED. Calls the handler of the current level with `ev`. |

| City/level | Script file | Init 68k | Handler 68k | Init OS4 | Handler OS4 |
|---|---|---|---|---|---|
| 0/0 | `00.script` | `0xb91dc` | `0xb9b6a` | `0x010eabc0` | `0x010d2b24` (ev 0 ignored; ev `0x898` = phrase 400 easter egg, inlined) |
| 0/1 | `01.script` | `0xb9324` | `0xbed0c` | `0x010ead34` | `0x010ebddc` |
| 1/0 | `10.script` | `0xb953c` | `0xc1a92` | `0x010eaf98` | `0x010eef18` |
| 1/1 | `11.script` | `0xb96ec` | `0xc436a` | `0x010eb188` | `0x010f16ec` |
| 2/0 | `20.script` | `0xb98f4` | `0xc73b4` (+ helpers `0xb5468`, `0xb5920`) | `0x010eb3d0` | `0x010d9ff4` (ev 100000–199999), `0x010da308` (ev 300000–399999), `0x010de0b4` (others) |
| 2/1 | `21.script` | `0xb9aaa` | `0xc79cc` | `0x010ebbe8` | `0x010df934` (ev 0 ignored) |

Level-specific helpers live in `0xb5468–0xb91dc` (68k): `0xb5468`, `0xb5692`, `0xb5920`,
`0xb6c46`, `0xb6e48`, `0xb73d0`, `0xb7844`, `0xb7c40`, `0xb8028`, `0xb824e`, `0xb82ca`,
`0xb8f4e`, and the per-level "phone mission finished → unlock next phones" functions
`0xb506e`, `0xb5198`, `0xb5230`, `0xb52c2`, `0xb538e` (see §4).

### 2.1 Who calls them — main game loop `FUN_000ec686` (68k) — CONFIRMED

* Level start (`0xeea3a`): if single player (`0x11a6e6 == 0 && 0x11a6e4 == 0`, i.e. not
  a multiplayer/user-map game) → `Mission_Init(city=0x348194, level=0x34c418)`.
  User maps and multiplayer have no script.
* Every frame, while the game state `0x34c190 == 0` (playing), `0x34c198 == 0` and
  `0x34c42c == 0`:
  `r = Trigger_Update(map=0x30c3bc, player=actors[0x30c3a0], &playerIndex, ticks=0x348090,
  storedFootHealth=0x34c1a0, respawnTimer=0x34c1c8)` (`0xa4a68`).
  If `r == 2` → **level complete**: state `0x34c190 = 3`, big message phrase 2023
  "Level complete!", sound 0x1a.
* When `0x34c42c != 0` (set when countdown `0x34c194` reaches 1 — it is armed to 100
  ticks at game over) the loop calls `Mission_Event(0)` each frame instead of
  evaluating triggers. Most handlers ignore event 0. (INFERRED: also used after level
  end; needs checking.)
* On player death/arrest (`0xeed16..`): lives `0x34c428 -= 1`, **bonus multiplier
  `0x30c3b0 = 1`**; if lives < 1 → game over (state 2, phrase 2026 "Game over!",
  `0x34c194 = 100`); otherwise when the respawn timer `0x34c1c8` expires,
  `Trigger_PlayerRespawned()` (`0xa69c2`, fires all type 0x3E triggers) and the player is
  re-created on foot with his money.
* Shutdown (`0xef444`): `Mission_Shutdown()`.

So **scripts are event-driven state machines** (not coroutines in Update 7): the only
code that runs per frame is the generic trigger evaluator; level code runs only when a
trigger fires (or at init). Mission state lives in level globals (e.g. level 00 uses
`0x2fb9f8..`, `0x2fba00..`).

## 3. Triggers (the core mechanism)

A trigger = "when condition X happens, send event number E to the level handler, then
delete the trigger". Event numbers are arbitrary ints chosen by the script (phone
missions use 100, 200, …; per-mission events 0x28.., ids ≥ 100000 for rectangles, etc.).

Storage (68k, CONFIRMED): `0x2c2778`, 500 entries × 0x26 bytes, count `0x1196ec`.

| Off | Field |
|---|---|
| +0x00 | event number |
| +0x04 | type (0x00–0x44) |
| +0x08 | p1 |
| +0x0c | p2 |
| +0x10 | p3 |
| +0x14 | u16 `door` flag (only type 4 sets 1) |
| +0x16 | p4 |
| +0x1a | p5 |
| +0x1e | p6 (type 1/7: looping ring-sound handle; stopped when the trigger is removed) |
| +0x22 | cached map-cell pointer (door) |

Adds and kills are queued (`0x2c71b8`, 500 × 0x28) and applied at the end of
`Trigger_Update` (`0xa4920`), so a handler may add/kill triggers while triggers are being
evaluated. Killing an event kills **every** trigger with that event number
(`AddTrigger(): Too many triggers`, `KillTrigger(%d): Too many entries` errors).

Low level: `Trigger_QueueAdd(ev,type,p1,p2,p3,door,p4,p5,p6)` `0xa6842`;
`Trigger_QueueKill(ev)` `0xa692a` (OS4 `0x010f46b8`); `Trigger_Remove(index)` `0xa45c6`
(also stops the ring sound and restores the phone/door map cell); `Trigger_ClearAll`
`0xa6834`; `Trigger_GetTimerSeconds()` `0xa6978` (first type-5 trigger's ticks / 30, used by the HUD clock).

### 3.1 Script-level AddTrigger

`AddTrigger(ev, type, a, b, c, d)` = 68k `0xb41dc` → `0xb2b12(ev,type,a,b,c,d,0)`;
OS4 `0x010d100c` → `0x010cfa80` (`Recieved unknown trigger (%d)` on bad type).
`AddTriggerEx(ev,type,a,b,c,d,e)` = `0xb2b12` directly (7th arg used by type 0x39).

Most level code uses the **tracked** variants, which also remember the event in a list
(`0x2fb228[500]`, count `0x11a350`) so a whole mission can be cancelled at once:

| 68k | Name | Semantics (CONFIRMED) |
|---|---|---|
| `0xb4f5a` | `AddMissionTrigger(ev,type,a,b,c,d)` | remove `ev` from the tracked list, add trigger, append `ev` |
| `0xb4fb2` | `AddMissionTriggerEx(…,e)` | same with 7 args |
| `0xb5010` | `KillMissionTrigger(ev)` | queue kill + untrack |
| `0xb5030` | `KillAllMissionTriggers()` | queue kill of every tracked event; list emptied |
| `0xa692a` | `KillTrigger(ev)` | queue kill only (untracked) |

Coordinates: world units, one map square = 0x800 (2048); square (sx,sy) centre =
`sx<<11 | 0x400`. Map cell address = `x*0x3900 + y*0x72 + cube*0x26` (cube 0–2 =
floor level). `floor`/`z` arguments are the cube level 0–2 the actor stands on
(actor `+0x3ce`). Angles: 0x1680 (5760) = 360°; 0x5a0 = 90°. Time: **30 ticks = 1 s**
(INFERRED from `*0x1e` in timer setup and `/0x1e` in the HUD).

"Player" below = the actor the player currently controls (`actors[0x30c3a0]`); when he
drives, that is the **vehicle actor**. "On foot" = player model flag `+0x4909 == 1`.
"Controlled" = player actor state `+0x426 == 2`. "Arrow" = the trigger's position is
added to the HUD direction-arrow/radar target list (`0xeff2a`; several targets cycle
every 20 ticks).

### 3.2 Trigger types

Evaluator: `Trigger_Update` 68k `0xa4a68` (CONFIRMED unless marked). Argument names are
those of `AddTrigger(ev,type,a,b,c,d)`; "actor a" = every live actor whose script id
(`+0x47c`) equals `a` at the time the trigger is added (one trigger per matching actor,
the stored value is the actor pointer). If an actor referenced by pointer is destroyed,
triggers of types 8,0x0d,0x13,0x14,0x16,0x2d,0x33–0x36,0x3b,0x3c on it are silently
removed (`0xa653a`).

| Type | Args | Fires when | Arrow |
|---|---|---|---|
| 0x00 | a = money | player money (`0xf0060`) ≥ a → handler(ev) **and level complete** (evaluator returns 2). Add also sets the HUD target `0x34c440 = a` ("Goal: Get …"); not added if a ≤ 0 (level 20 passes −1: no money goal). | – |
| 0x01 | a = phone no. | **Phone call.** Position = `GetPhoneCoords(a)` (`0x91d24`, phones are map squares with scenery 14 + special n); starts looped ring sound 0xe at the phone; while active, toggles the phone's scenery 14↔19 (phone/phone_arrow, flashing); fires when player controlled, on foot, same floor, within ±0x400 of the phone centre. | yes |
| 0x02 | a = actor id | position of actor a at add time; fires like type 1 (on foot, ±0x400). | yes |
| 0x03 | a = vehicle model | player's current model == vehicle model a (player is driving a car of model a). | – |
| 0x04 | a,b = square, c = floor, d = side 0–3 | **Garage/door drop-off** (door flag set). Within ~6 squares the door face of the cube (side d) opens (face type set to 5, sound 0x16) and closes when leaving. Fires when controlled player is on floor c−1 and within ~1000 units of the square centre: player actor (the car) is removed (state 6) and a pedestrian player is created 0x800 outside side d (d=0 −y, 1 +x, 2 +y, 3 −x; facing 0xb40/0x5a0/0/0x10e0), inheriting weapons/money/wanted level and the stored on-foot health. | yes |
| 0x05 | a = seconds | **visible countdown** (HUD clock, ticks a*30, beep sound 0x18 every second); fires at 0. | – |
| 0x06 | – | player dead or arrested (respawn countdown > 0). | – |
| 0x07 | a = phone no. | like 0x01 (ringing, flashing, on foot) but **no arrow** (used for "secret"/optional phones). | – |
| 0x08 | a = actor id | player **gets into** actor a (player actor == a). | yes |
| 0x09 | a = actor id | actor id a destroyed/removed (`0xa653a`, called from damage/explosion code). | – |
| 0x0a | a = amount | money ≥ (money at add time + a). | – |
| 0x0b | – | player is in any vehicle. | – |
| 0x0c | a = actor id | Add: actors a → AI state 0x0d (`+0x422=1`, `+0x30e=5`); fires when one of them completes that action (`0xa6a7a`, from AI `0xe3822`). INFERRED: "get into/approach target vehicle". | – |
| 0x0d | a = actor id | controlled player within ±6000 of actor a. | yes |
| 0x0e | – | player's vehicle type (`0xf00a8`) differs from the type at add time (left/changed vehicle). | – |
| 0x0f | a,b = x,y; c = floor | controlled player within ±0x1000 of (a,b) on floor c (any mode). | yes |
| 0x10 | a,b,c | as 0x0f and \|speed\| (`+0x3ae`) < 0x14 (stopped). | yes |
| 0x11 | a = seconds | hidden timer (a*30 ticks). | – |
| 0x12 | a = actor id; b,c = x,y | Add: actors a → AI state 0x0e "go to (b,c)" (`+0x2e6/+0x2ea`); fires when it arrives (`0xa6b5a`); actor is then removed (state 6). | – |
| 0x13 | a = actor id; b = speed | actor a's speed > b. | – |
| 0x14 | a = actor id; b = speed | actor a's speed < b. | – |
| 0x15 | a,b,c | controlled player within ±12000 of (a,b), floor c. | – |
| 0x16 | a = actor id; b,c = x,y | actor a is **outside** ±0x1800 of (b,c). | – |
| 0x17 | a = actor id; b,c = x,y | Add: actors a → AI state 0x0f "drive/walk to (b,c)", `+0x444 = 1000` (speed), `+0x342=-1`; fires on arrival (`0xa6640`, actor → state 4 idle). | – |
| 0x18 | a = actor id | actor a within ±6000 of player (no state check). | – |
| 0x19 | a = actor id | actor a **outside** ±33000 of player ("lost him"). | – |
| 0x1a | a,b,c | controlled player within ±10000 of (a,b), floor c. | yes |
| 0x1b | a = actor id | **actor a sees the player**: same floor, alive, dist² < 6e6 (16e6 if a is in a vehicle), player within ±90° of a's heading, line of sight (`0x9217e`). | yes |
| 0x1c | a,b = x,y | player outside ±0x1800 of (a,b). | – |
| 0x1d | a, b = actor ids | actor a within ±3000 of actor b and facing it (±90°). | – |
| 0x1e / 0x1f | a = x | player x ≤ a / player x > a. | – |
| 0x20 / 0x21 | a = y | player y ≤ a / player y > a. | – |
| 0x22 | a = weapon | player fires/uses weapon a (`0xa6c62`, from weapon code). INFERRED wording. | – |
| 0x23 | a = weapon | player's selected weapon (`+0x35e`) == a. | – |
| 0x24 | – | player switches weapon (`0xa6bc2`). | – |
| 0x25 | a,b,c = square x,y,cube | scenery at that cell becomes 0 (crate smashed / pickup collected). | yes (cell centre) |
| 0x26 | a = 1..5 | player is out of ammo for weapon slot a (1:`+0x366`, 2:`+0x36a`, 3:`+0x36e`, 4:`+0x372`, 5:`+0x362`). | – |
| 0x27 | – | `0xa6c12`, from vehicle physics (reverse/handbrake condition). INFERRED: handbrake turn. | – |
| 0x28 | a = actor id | actor a destroyed/killed (`0xa653a`). | yes (on actor) |
| 0x29 | – | a policeman is killed (`0xa6cca`, cop-kill code `0xccc96`). Fires one trigger per kill. | – |
| 0x2a | a,b,c = cell | cell byte +8 becomes 0. INFERRED: object/crate on that cell destroyed. | – |
| 0x2b | a = actor id | `0xa6a12` from vehicle damage code: actor a's vehicle is wrecked/set on fire. INFERRED. | – |
| 0x2c | a,b,c | controlled player **on foot** within ±0x200 of (a,b), floor c. | yes |
| 0x2d | a = actor id | player gets into actor a (as 0x08, no arrow). | – |
| 0x2e | a = actor id | actor a destroyed (`0xa653a`) or first attacked by the player (`0xa66ce`). | yes |
| 0x2f | a,b,c = cell | as 0x25 without arrow. | – |
| 0x30 | a = actor id | as 0x2e, no arrow. | – |
| 0x31 | a = actor id | actor a sees the player (as 0x1b, 6e6 range only, no arrow). | – |
| 0x32 | a = id; b,c = x,y; d = speed | as 0x17 with speed d and flag `+0x49e=1`; stored as type 0x17. | – |
| 0x33 / 0x34 | a = actor id; b = x | actor x ≤ b / actor x > b. With a = −1 they become player types 0x1e / 0x1f. | – |
| 0x35 / 0x36 | a = actor id; b = y | actor y ≤ b / actor y > b (a = −1 → 0x20 / 0x21). | – |
| 0x37 | a = ticks | hidden timer in ticks (stored as 0x11). | – |
| 0x38 | a,b,c | controlled player on foot within ±0x4b0 of (a,b), floor c. | – |
| 0x39 | a,b,c,d = x0,y0,x1,y1; e = floor | controlled player strictly inside the rectangle on floor e. | – |
| 0x3a | a,b,c,d = rect | controlled player outside the rectangle. | – |
| 0x3b | a = actor id; b,c | actor a inside ±0x1800 of (b,c). | – |
| 0x3c | a = actor id | controlled player within ±1000 of actor a, same floor. | yes |
| 0x3d | a = actor id; b = target id | Add: actors a → AI state 0x13 targeting actor b; fires via `0xa6ae2(a,b)` when the action completes. INFERRED. | – |
| 0x3e | – | player respawns after a death/arrest (`0xa69c2`). | – |
| 0x3f | a,b,c | controlled player within ±6000 of (a,b), floor c. | – |
| 0x40 | a,b,c | controlled player within ±0x400 of (a,b), floor c (any mode). | yes |
| 0x41 | a,b,c | as 0x2c (on foot, ±0x200) without arrow. | – |
| 0x42 | – | `0xa6d26` (from `0xe9644`). Unknown. | – |
| 0x43 | a = actor id | as 0x19 (actor escaped beyond ±33000); Add also sets actor `+0x342=-1`, state 0x14 (flee). | – |
| 0x44 | a = id; b,c = x,y | as 0x17 but AI state 0x15. | – |

## 4. Phones, missions and progression

### 4.1 Phone slots — CONFIRMED (68k `0xb4df8` "RefreshPhones", OS4 `0x010d2480`)

Each level owns 20 phone slots:

* `0x2fb158[i]` = event number sent when the phone is answered (−1 = slot empty);
* `0x2fb1a8[i]` = phone number (map phone *n*, `GetPhoneCoords`);
* `0x2fb1f8[i]` (u16) = 1 → hidden phone (trigger type 7, no arrow; does not count as
  "missions available").

`RefreshPhones()` adds a type 1/7 trigger for every non-empty slot. On the first call of
a level (`0x2fb220 == 1`) it shows the level intro phrase `0x2fb224` (speaker 2); later,
if at least one visible phone rings, speaker 2 says a random global phrase 2150–2158
("Head back to the phones for more missions.").
`KillAllPhones()` `0xb4ed8` kills all slot events (done when a phone is answered, so the
other phones stop ringing during a mission). The answered slot index is stored in
`0x11a354`.

When a mission ends (success **or** failure) the level calls its *unlock* routine
(`0xb506e` level 00, `0xb5198`, `0xb5230`, `0xb52c2`, `0xb538e` for other levels —
mapping per level in the missions docs), which clears the answered slot, may fill new
slots when prerequisite slots are all empty (= missions done), and calls
`RefreshPhones()`. Example (level 00, `0xb506e`): finishing slot 1 or 9 when both are
done opens slots 6 (ev 400, phone 4) and 2 (ev 200, phone 5); finishing 4 or 2 opens
0x0b (ev 0x44c, phone 0x0b), 3 (ev 300, phone 0x0c), 6 (ev 600, phone 0x0d); finishing
0x0b/3/6 opens 5 (ev 500, phone 0x0e), 0x0a (ev 1000, phone 0x11), 0 (ev 0x14, phone 0x0f).

### 4.2 Money, multiplier, lives — CONFIRMED

| 68k | Name | Semantics |
|---|---|---|
| `0xb4202` → `0xefe72` | `AddMoney(n)` | player money (`+0x382`) += n × **bonus multiplier** `0x30c3b0`; floating score text at the player. |
| `0xf0060` | `GetMoney()` | player `+0x382` (multiplayer: `0x34c1d8`). |
| `0xb4074` | `ShowMessage(phrase)` | big centre message (`0x35fe0` slot 0). Phrase 2022 "Mission complete!" also plays sound 0x1a and **multiplier += 1 (max 4)** (`0xeff00`); 2028 "Mission failed!" plays sound 0x19. |
| `0xb434e` → `0xefede` | `AddLives(n)` | lives `0x34c428` += n. |
| `0xb4cd0` | `LevelComplete()` | game state `0x34c190 = 3`, message 2023, sound 0x1a (same as a type-0 trigger firing). |
| `0xb43c2` → `0xefdfa` | `AddHealth(n)` | on foot: actor `+0x34a` += n (min 0); in a car: stored foot health `0x34c1a0` += n. |
| `0xb4214` | `SetWantedLevel(n)` | player `+0x32e = n` (police level 0–6), resets wanted timer `0x30c3b8`. |

Multiplier: starts at 1, +1 per "Mission complete!" (max 4), reset to 1 on death/arrest
(`0xeed24`). Killing a cop gives 2500×mult, every 11th cop "Copkiller bonus!" 100000×mult.

Level goal: type-0 trigger. Levels 00 = 1,500,000; 01, 10, 11, 21 = 2,000,000;
level 20 has none (−1) and ends through `LevelComplete()` (CONFIRMED from the six inits).

### 4.3 Level select, saving, time trophies

See §7 (filled from the progression analysis).

## 5. Script API (Update 7 functions called by level code)

Actors: up to 32 (`0x30c318[]`, count−1 in `0x11a738`); each has a script id `+0x47c`
(−1 = none). All "id" APIs act on **every** actor with that id. Model argument of the
create functions: `< 0` → pedestrian model `~m` (table `0x3480e0`), `0..999` → vehicle
model m (table `0x348120`, 28 entries), `≥ 1000` → special object `m−1000` (table `0x30c3c0`).

| 68k | Calls | Name | Arguments → semantics | Status |
|---|---|---|---|---|
| `0xb231c` | 107 | `CreateActor(id, model, x, y, floor, angle, colour)` | spawns at (x,y) on cube level `floor`, z from map heights; vehicles get paint `colour` (tables `0x348198/0x3481b8`) and engine sounds. | CONFIRMED |
| `0xb2590` | 6 | `CreateActorFx(id, model, x, y, floor, angle, colour, p8)` | as above with sparkle particles + sound 0x2f; `+0x3ca = p8`. | CONFIRMED (p8 meaning unknown) |
| `0xb21a6` | 57 | `SetScenery(sx, sy, cube, scenery, special, rot)` | sets cell scenery (ids in formats.md, e.g. 6 crate, 13 shotgun, 23 briefcase, 24 info); sparkle + sound 0x2f unless scenery 0 or 23. | CONFIRMED |
| `0xb48d8` | 34 | `SetCube(sx,sy,cube,h1,h2,scenery,b0,flags)` | rewrites a map cube (+2,+6,+1,+0,+8). | CONFIRMED |
| `0xb4958..0xb4b40` | 57 | `SetFaceTop/N/E/S/W(sx,sy,cube,type,texture,flip)` | face byte (+9..+0xd) and texture pointer. | CONFIRMED |
| `0xb3d0e` / `0xb3f08` | – | `StoreRegion(x0,y0,x1,y1,cube,slot)` / `PasteRegion(x,y,cube,slot)` | copy ≤40×40 cubes to/from 2 buffers. | CONFIRMED |
| `0xb40ec` | 415 | `Dialog_Begin(speaker)` | queues a "start" entry (type 1) in the subtitle queue (`0x21e00`). Values used: 1, 2, 4. | CONFIRMED / speaker meaning INFERRED |
| `0xb40fe` | 12 | `Dialog_Say1(phrase)` | queues phrase text, style 1. | CONFIRMED |
| `0xb411c` | 328 | `Dialog_Say2(phrase)` | queues phrase text, style 2. | CONFIRMED |
| `0xb413a` | 312 | `Dialog_End()` | queues end marker. | CONFIRMED |
| `0xb4148` | 11 | `Dialog_Flush()` | drops queued subtitles. | CONFIRMED |
| `0xb4074` | 48 | `ShowMessage(phrase)` | see §4.2. | CONFIRMED |
| `0xb4202` | 70 | `AddMoney(n)` | §4.2 | CONFIRMED |
| `0xb43c2` | 1 | `AddHealth(n)` | §4.2 | CONFIRMED |
| `0xb434e` | 4 | `AddLives(n)` | §4.2 | CONFIRMED |
| `0xb4214` | 17 | `SetWantedLevel(n)` | §4.2 | CONFIRMED |
| `0xb4cd0` | 1 | `LevelComplete()` | §4.2 | CONFIRMED |
| `0xb4510` | 244 | `Actor_Release(id)` | actors id → state 5 unless player-controlled/removed; id cleared (becomes ambient traffic/pedestrian). | CONFIRMED (state 5 = ambient AI, INFERRED) |
| `0xb42b8` | 21 | `Actor_ReleaseIdle(id)` | as above with state 3; `+0x2ee = -1`. | CONFIRMED / meaning INFERRED |
| `0xb459a` | – | `Actor_Remove(id)` | state 6 (deleted) unless controlled. | CONFIRMED |
| `0xb423a` | 7 | `Actor_Kill(id)` | health `+0x34a = -71`. | CONFIRMED |
| `0xb28ac` | 72 | `Actor_Attack(id, mode, target)` | mode 0: stop (state 4); mode>0, target −1: attack the player (state 0x10, `+0x346 = mode`, weapon 5 with 100 rounds); mode>0, target id: attack actor `target` (state 0x11); mode<0, target −1: state 0x12 (flee from player). Typical call `(id, 0x50, -1)`. | CONFIRMED / AI meaning INFERRED |
| `0xb2072` | 7 | `Actor_Follow(id, leader)` | `+0x41a` = leader actor (leader −1 → 1 = player?, other negatives → none). | INFERRED |
| `0xb47e0` | 13 | `Actor_SetAggro(id, v)` | `+0x346 = v`. | INFERRED |
| `0xb475c` | 1 | `Actor_Set386(id, v)` | `+0x386 = v`. | unknown |
| `0xb483c` | 5 | `Actor_SetFlag49c(id, on)` | `+0x49c`. | unknown |
| `0xb4666` / `0xb46dc` | 2 | `Actor_Siren(id, on/off)` | vehicles with `model+0x4928` (siren/emergency vehicles): `+0x498` on/off; off also zeroes speed. | INFERRED |
| `0xb41b0` | 2 | `SetPlayerId(id)` | if player is on foot: player `+0x47c = id` (so actor-id triggers can target the player). | CONFIRMED |
| `0xb48ac` | 2 | `SetProjectileId(id)` | `0x11a77e`: script id given to objects the player throws/drops (grenades, bombs). | INFERRED |
| `0xb4296` | 4 | `Explosion(x,y,z,owner,size)` | `0xcdd02`: debris/fire particles + damage. | CONFIRMED (args INFERRED) |
| `0xb4382` | 1 | `AddEffect(...8 args)` | particle emitter type 9. | INFERRED |
| `0xb43d4`, `0xb4414`, `0xb448e` | – | ped-in-vehicle helpers (`0xdb03e`/`0xdb196`) | create a driver/passenger of model `~m` for vehicle/actor id. | INFERRED |
| `0xb4602` | – | `RemoveAllActors()` | removes every non-player actor. | CONFIRMED |
| `0xb4bba` | 3 | `AddSound(ref 0–9, sample, loop, vol, period, x, y, range)` | `AddSound_script`; handle in `0x2fb000[ref]`. | CONFIRMED |
| `0xb4c0e` | 10 | `DeleteSound(ref)` | `DeleteSound_script`. | CONFIRMED |
| `0xb4c58` | 16 | `SetAmbience(n)` | `0x34c078 = n`; rain/ambient loop volume 0xff if n==3 else 0x7f. Used by level 20 indoor/outdoor rectangles. | INFERRED |
| `0xb47b8` | 12 | `SetTimerMode(n)` | `0x117d3e = n`; n 1/2 sound 0x37, 3 sound 0x38. | INFERRED (race/checkpoint HUD) |
| `0xb47ca` | 4 | `SetTimerMode2(n, v)` | `0x117d42 = n`, `0x139288 = v`; n>1 sound 0x39. | INFERRED |
| `0xb4284` | 5 | `Set11a734(v)` | `0x11a734 = v`. | unknown |
| `0xb4360` | 15 | `SetFlag34c28c(b)` | `0x34c28c = b!=0`. | unknown |
| `0xb48be` | 2 | `Set26c040(a,b,c)` | `0x26c040..48`. | unknown |
| `0xb4df8` / `0xb4ed8` | 6 / 45 | `RefreshPhones()` / `KillAllPhones()` | §4.1 | CONFIRMED |
| `0xb4f5a`, `0xb4fb2`, `0xb5010`, `0xb5030`, `0xb41dc`, `0xa692a` | | trigger functions | §3.1 | CONFIRMED |
| `0x3fc8c` | 43 | `Random(n)` | 0..n−1 | INFERRED |

## 6. CD script command codes (legacy)

Stub address (script00) → command code sent in `+0x14`. The mapping to Update 7 functions
was established by comparing script00 `main` with 68k `0xb91dc/0xb9b6a`:

| Code | script00 stub | Update 7 equivalent |
|---|---|---|
| 0x03 | `0x10e76` | `ShowMessage` |
| 0x04 | `0x108ac` | `CreateActor` (7 args) |
| 0x07 | `0x10e9e` | `Dialog_Begin` |
| 0x08 | `0x10ec6` / `0x10ef0` | `Dialog_Say1` / `Dialog_Say2` |
| 0x09 | `0x10f1a` | `Dialog_End` |
| 0x0a | `0x10c44` / `0x10c9e` | `AddTrigger` (6 / 7 args) |
| 0x0b | `0x10cf8` | `KillTrigger` |
| 0x13 | `0x10b80` | `AddMoney` (INFERRED from mission 0x33 in script00) |
| 0x1b | `0x10e26` | `AddLives` |
| others (0x05, 0x0c–0x12, 0x14–0x1a, 0x1c–0x36) | see `analysis/ghidra_script00/` | not mapped |

Game → script messages: type 1 connect, type 2 goodbye, event messages carry the event
number in `+0x16`.

## 7. Progression, level select, saving

(filled in below)
