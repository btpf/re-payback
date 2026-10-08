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
* **Event 0 = "level is ending"**: the end countdown `0x34c194` is armed to 100 ticks at
  game over (`0xeee4c`) or demo time-out, 60 on quit-to-title (`0xda2e0`) or when the
  continue key is pressed after level complete, 1 on Esc in a replay. When it runs out,
  `0x34c42c = 1`, `Mission_Event(0)` is called once (`0xeef0c`), the loop exits (`0xef1fa`)
  and `Mission_Shutdown()` runs. Every handler ignores event 0. CONFIRMED.
* On player death/arrest (`0xeed16..`): lives `0x34c428 -= 1`, **bonus multiplier
  `0x30c3b0 = 1`**; if lives < 1 → game over (state 2, phrase 2026 "Game over!",
  `0x34c194 = 100`); otherwise when the respawn timer `0x34c1c8` expires,
  `Trigger_PlayerRespawned()` (`0xa69c2`, fires all type 0x3E triggers) and the player is
  re-created on foot at the respawn point (`0x43c96`) with his money restored in full
  (death costs a life and the multiplier, not money). Death/arrest sets the respawn timer
  to 80 ticks (arrest shows 2017 "Busted!" and the crime summary).
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
(actor `+0x3ce`). Angles: 0x1680 (5760) = 360°; 0x5a0 = 90°. Time: scripts and HUD assume
**30 ticks = 1 "second"** (`*0x1e` in timer setup, `/0x1e` in the HUD, 1800 ticks = 1 trophy
minute), but the **68k game actually runs at 25 ticks/s** (frame loop `0xef13a`: DOS DateStamp
50 Hz ticks halved, clamped 1..5 per frame in `0x348090`), so one script second = 1.2 s real
time on 68k. The OS4 port runs at 30 ticks/s (`0x0106a058`). CONFIRMED.

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
removed (`0xa653a`). `0xa653a` takes a **script id** and is called only from the death/destruction paths (`0xccc96` player kill credit, `0xdc5ac` health < −70, `0xde0b6` collision damage, `0xe3a58` player death / melee kill / bomb detonation). Removal through state 6 (type 0x04 drop-off, 0x12 arrival, 0x0c/0x3d boarding, `Actor_Remove`, `RemoveAllActors` → `0xcf290`) does **not** call it, so pointer triggers on such an actor keep a stale pointer. CONFIRMED.

| Type | Args | Fires when | Arrow |
|---|---|---|---|
| 0x00 | a = money | player money (`0xf0060`) ≥ a → handler(ev) **and level complete** (evaluator returns 2). Add always sets the goal `0x34c440 = a` (`0xb2bce`, also when a ≤ 0), but the trigger is only added if a > 0. Goal < 1 makes the pause menu show 2056 "Goal: Escape from jail" (level 20), else 2058 "Goal: Get …". All levels use event −1. | – |
| 0x01 | a = phone no. | **Phone call.** Position = `GetPhoneCoords(a)` (`0x91d24`, phones are map squares with scenery 14 + special n); starts looped ring sound 0xe at the phone; while active, toggles the phone's scenery 14↔19 (phone/phone_arrow, flashing); fires when player controlled, on foot, same floor, within ±0x400 of the phone centre. | yes |
| 0x02 | a = actor id | position of actor a at add time; fires like type 1 (on foot, ±0x400). | yes |
| 0x03 | a = vehicle model | player's current model == vehicle model a (player is driving a car of model a). | – |
| 0x04 | a,b = square, c = floor, d = side 0–3 | **Garage/door drop-off** (door flag set). Within ~6 squares the door face of the cube (side d) opens (face type set to 5, sound 0x16) and closes when leaving. Fires when controlled player is on floor c−1 and within ~1000 units of the square centre: player actor (the car) is removed (state 6) and a pedestrian player is created 0x800 outside side d (d=0 −y, 1 +x, 2 +y, 3 −x; facing 0xb40/0x5a0/0/0x10e0), inheriting weapons/money/wanted level and the stored on-foot health. | yes |
| 0x05 | a = seconds | **visible countdown** (HUD clock, ticks a*30, beep sound 0x18 every second); fires at 0. | – |
| 0x06 | – | player dead or arrested (respawn countdown > 0). | – |
| 0x07 | a = phone no. | like 0x01 (ringing, flashing, on foot) but **no arrow** (used for "secret"/optional phones). | – |
| 0x08 | a = actor id | player **gets into** actor a (player actor == a). | yes |
| 0x09 | a = actor id | actor id a **destroyed or killed** (`0xa653a`). Not fired when the actor is removed via state 6 (drop-off, arrival, boarding, Actor_Remove). CONFIRMED. | – |
| 0x0a | a = amount | money ≥ (money at add time + a). | – |
| 0x0b | – | player is in any vehicle. | – |
| 0x0c | a = actor id | Add: pedestrians a → AI state 0x0d (`+0x422=1`, `+0x30e=5`): walk to and **board the player's vehicle**; AI state 0x0d re-targets `+0x41e` to the player's current actor each frame; boarding (`0xe361e`) waits while the player is on foot. Fires (`0xa6a7a`) when the ped is within the door radius and ±0x800 in z; the ped is then set to state 6 and removed next frame by `0xdc5ac` → `0xcf290`, **without** a type-9 event. CONFIRMED (used for passengers in levels 00, 01, 21). | – |
| 0x0d | a = actor id | controlled player within ±6000 of actor a. | yes |
| 0x0e | – | player's vehicle type (`0xf00a8`) differs from the type at add time (left/changed vehicle). | – |
| 0x0f | a,b = x,y; c = floor | controlled player within ±0x1000 of (a,b) on floor c (any mode). | yes |
| 0x10 | a,b,c | as 0x0f and \|speed\| (`+0x3ae`) < 0x14 (stopped). | yes |
| 0x11 | a = seconds | hidden timer (a*30 ticks). | – |
| 0x12 | a = actor id; b,c = x,y; d unused | Add: actors a → AI state 0x0e "go to (b,c)" (`+0x2e6/+0x2ea`); fires when it arrives (\|dx\|+\|dy\| < 0x579, `0xa6b5a`); actor is then removed (state 6, no type 9). The 4th argument some scripts pass (1, −1) is ignored (68k `0xb2b12`, OS4 `0x010cfa80`). CONFIRMED. | – |
| 0x13 | a = actor id; b = speed | actor a's speed > b. | – |
| 0x14 | a = actor id; b = speed | actor a's speed < b. | – |
| 0x15 | a,b,c | controlled player within ±12000 of (a,b), floor c. | – |
| 0x16 | a = actor id; b,c = x,y | actor a is **outside** ±0x1800 of (b,c). | – |
| 0x17 | a = actor id; b,c = x,y | Add: actors a → AI state 0x0f "drive/walk to (b,c)", `+0x444 = 1000` (arrival radius), `+0x342=-1`; fires on arrival (`0xa6640`, actor → state 4 idle). | – |
| 0x18 | a = actor id | actor a within ±6000 of player (no state check). | – |
| 0x19 | a = actor id | actor a **outside** ±33000 of player ("lost him"). | – |
| 0x1a | a,b,c | controlled player within ±10000 of (a,b), floor c. | yes |
| 0x1b | a = actor id | **actor a sees the player**: same floor, alive, dist² < 6e6 (16e6 if a is in a vehicle), player within ±90° of a's heading, line of sight (`0x9217e`). | yes |
| 0x1c | a,b = x,y | player outside ±0x1800 of (a,b). | – |
| 0x1d | a, b = actor ids | actor a within ±3000 of actor b and facing it (±90°). | – |
| 0x1e / 0x1f | a = x | player x ≤ a / player x > a. | – |
| 0x20 / 0x21 | a = y | player y ≤ a / player y > a. | – |
| 0x22 | a = weapon | player fires weapon a (`0xa6c62` from the fire code in `0xe3a58`): 1 bazooka, 2 flamethrower, 3 grenade, 4 minigun, 5 shotgun, 7 pistol, 8 remote bomb **placed** (0 fists and 6 are never sent). Sent once per shot, after the ammo is taken and the projectile is created (`0xcf11e`), only for the player. Grenade: on **releasing** fire (holding builds `+0x3aa` up to 0x19). Bomb: the first press places it (event 8; the bomb gets the `SetProjectileId` id, else −2); a second press detonates it and calls `0xa653a(bomb id)`. CONFIRMED. | – |
| 0x23 | a = weapon | player's selected weapon (`+0x35e`) == a. `+0x35e` ids: 0 fists, 1 bazooka, 2 flame, 3 grenade, 4 minigun, 5 shotgun, 0xd pistol, 0xe bomb (differ from the 0x22 numbering). | – |
| 0x24 | – | player switches weapon (`0xa6bc2`). | – |
| 0x25 | a,b,c = square x,y,cube | scenery at that cell (+1) becomes 0. Smashing a full crate (`0xd0670`) only turns it into its item (special 1..7 → scenery 9/10/11/12/13/27/28); picking the item up (`0xd0988`) sets 0. So it fires **on pickup**, and on the smash only for an empty crate. CONFIRMED. | yes (cell centre) |
| 0x26 | a = 1..5 | player is out of ammo for weapon slot a (1:`+0x366`, 2:`+0x36a`, 3:`+0x36e`, 4:`+0x372`, 5:`+0x362`). | – |
| 0x27 | – | `0xa6c12`: the player does a **J-turn** (vehicles.md §2.3): on the ground, throttle against the direction of travel while steering, in a model with 68k `+0x4926 == 0` (not tank, heli, boat); latch `+0x3c1` cleared each frame. CONFIRMED. | – |
| 0x28 | a = actor id | actor a destroyed/killed (`0xa653a`). | yes (on actor) |
| 0x29 | – | a policeman is killed (`0xa6cca`, cop-kill code `0xccc96`). Fires one trigger per kill. | – |
| 0x2a | a,b,c = cell | cell byte +8 (slope) becomes 0. The only runtime writer is the cube collision `0x446b8` (from `0x44f82`): a moving non-pedestrian actor (a vehicle) crossing a face of type 3 (glass) sets +8 = 0 and plays 0x31 BrokenGlass. Line-of-sight tests pass actor 0 and never break glass. CONFIRMED. | – |
| 0x2b | a = actor id | actor a **lands in water** (`0xa6a12`, top face water `+0x338`; callers `0xe0e46`, `0xe109e` and two in `0xe3a58`); latch `+0x44a` makes it fire once per entry, sound 0x2b Splash. An RC car (`+0x4904`) is also killed there (health −71), so type 9 fires too. CONFIRMED. | – |
| 0x2c | a,b,c | controlled player **on foot** within ±0x200 of (a,b), floor c. | yes |
| 0x2d | a = actor id | player gets into actor a (as 0x08, no arrow). | – |
| 0x2e | a = actor id | actor a destroyed (`0xa653a`) or first attacked by the player (`0xa66ce`). | yes |
| 0x2f | a,b,c = cell | as 0x25 without arrow. | – |
| 0x30 | a = actor id | as 0x2e, no arrow. | – |
| 0x31 | a = actor id | actor a sees the player (as 0x1b, 6e6 range only, no arrow). | – |
| 0x32 | a = id; b,c = x,y; d = radius | as 0x17 with arrival radius d and flag `+0x49e=1`; stored as type 0x17. | – |
| 0x33 / 0x34 | a = actor id; b = x | actor x ≤ b / actor x > b. With a = −1 they become player types 0x1e / 0x1f. | – |
| 0x35 / 0x36 | a = actor id; b = y | actor y ≤ b / actor y > b (a = −1 → 0x20 / 0x21). | – |
| 0x37 | a = ticks | hidden timer in ticks (stored as 0x11). | – |
| 0x38 | a,b,c | controlled player on foot within ±0x4b0 of (a,b), floor c. | – |
| 0x39 | a,b,c,d = x0,y0,x1,y1; e = floor | controlled player strictly inside the rectangle on floor e. | – |
| 0x3a | a,b,c,d = rect | controlled player outside the rectangle. | – |
| 0x3b | a = actor id; b,c | actor a inside ±0x1800 of (b,c). | – |
| 0x3c | a = actor id | controlled player within ±1000 of actor a, same floor. | yes |
| 0x3d | a = actor id; b = target id | Add: peds a → AI state 0x13: walk to and board actor b's vehicle; fires via `0xa6ae2(a,b)` when boarded. CONFIRMED. | – |
| 0x3e | – | player respawns after a death/arrest (`0xa69c2`). | – |
| 0x3f | a,b,c | controlled player within ±6000 of (a,b), floor c. | – |
| 0x40 | a,b,c | controlled player within ±0x400 of (a,b), floor c (any mode). | yes |
| 0x41 | a,b,c | as 0x2c (on foot, ±0x200) without arrow. | – |
| 0x42 | – | `0xa6d26` (`0xe9644`): fires each frame the player presses throttle (bit 0x1000 of `0x34c158`) while remote-controlling a model with `+0x4904 != 0` (RC car); evaluated only when such triggers exist (`0x1196f0`). Fire in the same branch self-destructs the RC car. This is the only call site (68k decompile ~124072), so level 21 M2's 0x42 on the rocket car (no `+0x4904`) never fires. CONFIRMED. | – |
| 0x43 | a = actor id | as 0x19 (actor escaped beyond ±33000); Add also sets actor `+0x342=-1`, state 0x14 (flee). | – |
| 0x44 | a = id; b,c = x,y | as 0x17 but AI state 0x15. The arrival routine `0xa6640` fires the actor's 0x17 **and** 0x44 triggers alike. Unlike 0xf, state 0x15 is in the police pursuit-selection list (vehicles.md §6.1): a police car given 0x44 drops the order and goes to traffic (wanted < 3) or pursuit (wanted ≥ 3). CONFIRMED (68k `FUN_000e3a58` ~124171). | – |

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
other phones stop ringing during a mission). The level code itself stores the slot index of the running
mission in `0x11a354` when it starts it (CONFIRMED in levels 01, 11).

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

See §7.

## 5. Script API (Update 7 functions called by level code)

Model indices (load order in `0xd7874`, CONFIRMED by missions_20): vehicles (`0x348120+4*m`):
0 Mundaneo, 1 Pug, 2 Diblo, 3 Bus, 4 Vapour, 5 Pug GTI, 6 Fjord, 7 Van, 8 Limo, 9 Police,
10 Scooby, 11 Tank, 12 Evo, 13 Taxi, 14 IceCreamVan, 15 Pickup, 16 HotRod, 17 Helicopter,
18 Pug Racer, 19 Truck, 20 PoliceVan, 21 BombTruck, 22 RC car, 23 Boat, 24 RocketCar,
25–26 RocketCar thrust, 27 Vapour Racer. Pedestrians (`0x3480e0+4*m`, script passes `~m`):
0–5 MaleNormal variants, 6 PedMale1, 7 PoliceMale1, 8 PedMale2, 9 PedMale3, 10 PedMale4,
11 PedMale5, 12 PedMale6, 13–14 MaleNormal variants, 15 PedMale7. Special object 1001 =
`objects/misc/cone.obj`.


Actors: up to 32 (`0x30c318[]`, count−1 in `0x11a738`); each has a script id `+0x47c`
(−1 = none). All "id" APIs act on **every** actor with that id. Model argument of the
create functions: `< 0` → pedestrian model `~m` (table `0x3480e0`), `0..999` → vehicle
model m (table `0x348120`, 28 entries), `≥ 1000` → special object `m−1000` (table `0x30c3c0`).

| 68k | Calls | Name | Arguments → semantics | Status |
|---|---|---|---|---|
| `0xb231c` | 107 | `CreateActor(id, model, x, y, floor, angle, colour)` | spawns at (x,y) on cube level `floor`, z from map heights; vehicles get paint `colour` (tables `0x348198/0x3481b8`) and engine sounds. | CONFIRMED |
| `0xb2590` | 6 | `CreateActorFx(id, model, x, y, floor, angle, colour, dmgFilter)` | as above with sparkle particles + sound 0x2f; `+0x3ca = dmgFilter`: only this weapon damages it (−1 all; 4 minigun, 7 pistol, 5 shotgun, 2 flame, 1/3 explosion). | CONFIRMED |
| `0xb21a6` | 57 | `SetScenery(sx, sy, cube, scenery, rotation, special)` | (OS4 `FUN_010cde90`) writes cell `+1` scenery (ids in formats.md, e.g. 6 crate, 13 shotgun, 23 briefcase, 24 info), `+0` **rotation** (256 = full turn) and `+4` **special** (crate contents in pickup numbering, e.g. 7 big bomb, 1 bazooka; the map loader reads +4 as the phone number). Runtime cell: +0 rotation, +1 scenery, +2 h1, +4 special, +5 flags, +6 h2, +8 slope, +9..+0xd face types (68k cell = map + x·0x3900 + y·0x72 + cube·0x26; OS4 x·0x3c00 + y·0x78 + cube·0x28). Sparkle + sound 0x2f unless scenery 0 or 23. | CONFIRMED |
| `0xb48d8` | 34 | `SetCube(sx,sy,cube,h1,h2,scenery,b0,slope)` | (sx, sy, cube, h1→+2, h2→+6, scenery→+1, rotation→+0, slope→+8) (OS4 `FUN_010cddc8`); face types kept. Slope 0 = Off **removes** the cube: not drawn, no surface (height 1e9), not solid; h1 only keeps the stacking base for the cubes above. | CONFIRMED |
| `0xb4958..0xb4b40` | 57 | `SetFaceTop/N/E/S/W(sx,sy,cube,type,texture,flip)` | face byte (+9..+0xd) and texture pointer. | CONFIRMED |
| `0xb3d0e` / `0xb3f08` | – | `StoreRegion(x0,y0,x1,y1,cube,slot)` / `PasteRegion(x,y,cube,slot)` | copy ≤40×40 cubes to/from 2 buffers. | CONFIRMED |
| `0xb40ec` | 415 | `Dialog_Begin(icon)` | queues a "start" entry in the subtitle queue (`0x21e00`); icon drawn by `0x22300`: 0 phone, 1 lips, 2 mobile, 3 pager, 4 info (scripts use 0, 1, 2, 4). The icon is the only effect: the voice is always SqueakyVoice 0x10 at 12000 Hz, with no portrait. In practice 0 = payphone briefing, 1 = a person present, 2 = the boss on the mobile, 4 = hint. | CONFIRMED |
| `0xb40fe` | 12 | `Dialog_Say1(phrase)` | phrase in font 1 (`you_`): **the player (Travis) speaks**; sound 0x10. | CONFIRMED |
| `0xb411c` | 328 | `Dialog_Say2(phrase)` | phrase in font 2 (`them_`): **the other party speaks**. | CONFIRMED |
| `0xb413a` | 312 | `Dialog_End()` | queues end marker. | CONFIRMED |
| `0xb4148` | 11 | `Dialog_Flush()` | drops queued subtitles. | CONFIRMED |
| `0xb4074` | 48 | `ShowMessage(phrase)` | see §4.2. | CONFIRMED |
| `0xb4202` | 70 | `AddMoney(n)` | §4.2 | CONFIRMED |
| `0xb43c2` | 1 | `AddHealth(n)` | §4.2 | CONFIRMED |
| `0xb434e` | 4 | `AddLives(n)` | §4.2 | CONFIRMED |
| `0xb4214` | 17 | `SetWantedLevel(n)` | §4.2 | CONFIRMED |
| `0xb4cd0` | 1 | `LevelComplete()` | §4.2 | CONFIRMED |
| `0xb4510` | 244 | `Actor_Release(id)` | actors id → state 5 = no AI input (a vehicle coasts to a halt, a ped stands still; the same state as an abandoned car, so hijacking it ejects no driver); states 2/6/7 left alone; id always cleared. Released actors (id −1, state 3/5/7/0xa–0xc) are culled when far from every player (`0xdc5ac`). | CONFIRMED |
| `0xb42b8` | 21 | `Actor_ReleaseIdle(id)` | as above with state 3 = ambient drone (wanders like normal traffic/peds); `+0x2ee = -1`. | CONFIRMED |
| `0xb459a` | – | `Actor_Remove(id)` | state 6 (deleted) unless controlled. | CONFIRMED |
| `0xb423a` | 7 | `Actor_Kill(id)` | health `+0x34a = -71`. | CONFIRMED |
| `0xb28ac` | 72 | `Actor_Attack(id, mode, target)` | mode 0: stop (state 4); mode>0, target −1: attack the player (state 0x10); mode>0, target id: attack the last actor with that id (state 0x11); both give a shotgun with 100 shells and `+0x346 = mode` = fire rate (fires when target within ~6000 and `rand(0x65−mode) < ticks`); mode<0, target −1: flee from player (state 0x12). Typical `(id, 0x50, -1)`. | CONFIRMED |
| `0xb2072` | 7 | `Actor_Follow(id, leader)` | `+0x41a` = leader actor; leader −1 → follow the player (`+0x41a=1`); −2 (other negatives) → stop following. | CONFIRMED |
| `0xb47e0` | 13 | `Actor_SetAggro(id, v)` | `+0x346 = v` = driver/shooter **skill** 0..100 (CreateActor default 50). Readers: fire chance `rand(0x65−v) < ticks`; v < 99 (or health < 50) looks ahead for blocking cubes; traffic lights (v < 70 stops at phases 1–3, 6; v < 90 at 1–2; ≥ 90 runs reds); corner-speed bands 31/51/61/91/95/98; v < 81 limits steering while skidding (vehicles.md §6.2). Race values 95–100 = never brake or stop. CONFIRMED | CONFIRMED |
| `0xb475c` | 1 | `Actor_SetMaxSpeed(id, v)` | `+0x386 = v`: hard speed clamp (default 1,000,000) in the on-foot branch and at the common vehicle point `0xea8c6`, so it also caps the **player** while driving that actor (level 21 M6 van). Race 200–550. | CONFIRMED |
| `0xb483c` | 5 | `Actor_SetControls(id, on)` | `+0x49c` controls enabled (default 1, `0x974a0`/`0xa2b80`). The only reader is the player-driven branch (state 2) of `0xe3a58`: 0 zeroes steering and throttle. No effect on AI drivers; in the race it holds the player's car (id 10) until "Go!". | CONFIRMED |
| `0xb4666` / `0xb46dc` | 2 | `Heli_TakeOff(id)` / `Heli_Land(id)` | helicopter models (68k `model+0x4928 != 0` = OS4 `+0x492a`, "flies"; the vehicles.md `+0x4928` tank/heli/boat flag is the OS4 offset of 68k `+0x4926`): `+0x498` 1 = rotors lift (`0xe109e`: vz −= 0xaa) and the AI can steer, 0 = controls zeroed, it sinks; Land also zeroes speed. No-op on other models. TakeOff is required for a 0x17 "fly to" order. (Some mission docs call these `Actor_Siren on/off`.) | CONFIRMED |
| `0xb41b0` | 2 | `SetPlayerVehicleId(id)` | if the player is **in a vehicle** (`+0x4909 == 0`): that vehicle actor `+0x47c = id` (tags the player's car). | CONFIRMED |
| `0xb48ac` | 2 | `SetProjectileId(id)` | `0xefece` → `0x11a77e`: script id (`+0x47c`) given to the next **remote bomb** the player places (`0xe3a58`); the default −1 (reset at level start) gives the bomb id −2. Grenades never get an id. | CONFIRMED |
| `0xb4296` | 4 | `Explosion(x,y,z,layer,size)` | `0xcdd02` → `0xcd22e` (OS4 `thunk_01044f28`). `layer` = particle layer 0..3 (as actor `+0x3ce`; scripts pass 0), not an owner: the blast is unattributed. Debris ≈ clamp(((size>>10)+2)/2, 2, 6) per type, flames if size > 2500; power size·25000 pushes actors and damages filters −1/1/3; camera shake += size/10; sound Explosion vol 50,000,000, Hz 12000 − min(1.25·size, 7000); peds within ~5 squares flee (`0xccbac`). | CONFIRMED |
| `0xb4382` | 1 | `AddGas(x,y,z,vx,vy,vz,life,layer)` | `0x3d97a(9, Gas, x,y,z, vx/3,vy/3,vz/3, life, layer, scale 0x30000)`; life −7 per tick (`0x3997c`), dies at life < 1 or z > 24000. Level 10 only (life 400+R(500), layer 1). | CONFIRMED |
| `0xb43d4` | – | `Ped_ExitPlayerVehicle(id, pedModel<0)` | `0xdb03e(veh, ~m, id)` spawns ped `~m` with script id `id` beside whatever the player controls (not checked to be a vehicle): at the door (veh `+0x48f8`), 1.2·(radius + half-width) to the side, heading veh − 0x5a0, `+0x448 = 1`; ped count `0x34809c` +1. The side alternates via toggle `0x34c1b0` (people.md §6.3). | CONFIRMED |
| `0xb4414` | – | `Ped_ExitVehicle(newId, vehId, pedModel<0)` | same, beside the first actor with id `vehId`. | CONFIRMED |
| `0xb448e` | – | `Ped_ExitVehicleEx(newId, vehId, pedModel<0, side)` | via `0xdb196`; `side` 0 = right (the player's own exit side), non-zero = left. | CONFIRMED |
| `0xb4602` | – | `RemoveAllActors()` | removes every non-player actor. | CONFIRMED |
| `0xb4bba` | 3 | `AddSound(ref 0–9, sample, loop, vol, hz, x, y, z)` | `AddSound_script` (OS4 `FUN_010cda74`); passes the rest straight to the engine's AddSound (68k `FUN_000170ce`, OS4 `FUN_0100815c`); handle in `0x2fb000[ref]`. The 8th argument is the sound's (downward) **z**, not a range: the shipped calls use 20000 (street level) and 19000 (level 20 alarm). Audibility comes only from `vol / d²` over the 3D distance (frontend_audio.md §4.3). | CONFIRMED |
| `0xb4c0e` | 10 | `DeleteSound(ref)` | `DeleteSound_script`. | CONFIRMED |
| `0xb4c58` | 16 | `SetVisibleLayers(n)` | `0x34c078 = n` = number of cube layers the renderer draws (roof cut-away indoors); rain loop volume 0xff if n==3 else 0x7f. Used with indoor rectangles (levels 10, 11, 20). | CONFIRMED (missions_20) |
| `0xb47b8` | 12 | `SetStartLights(n)` | `0x117d3e = n`: icon `lights_n` 0..3 (3 = go; sounds 0x37/0x38); −1 off. | CONFIRMED |
| `0xb47ca` | 4 | `SetLapDisplay(lap, laps)` | `0x117d42 = lap`, `0x139288 = laps` ("Lap %d of %d"); −1 off; sound 0x39 on lap > 1. | CONFIRMED |
| `0xb4284` | 5 | `SetTrafficModel(m)` | `0x11a734` = forced traffic vehicle model; spawner `0xcf5cc` spawns one when none exists; −1 = random. | CONFIRMED |
| `0xb4360` | 15 | `ShowCheckpointMarker(b)` | `0x34c28c = b!=0`; renderer draws a flashing Checkpoint.red/blue icon at the current arrow target (`0x34c288/94/90`) if `0x34c288 ≥ 0`. | CONFIRMED |
| `0xb48be` | 2 | `SetSpawnPoint(sx, sy, cube)` | forced start/respawn square (`0x26c040..48`, used by `0x43f0e` start and `0x43c96` respawn); −1 = map start points (respawn picks a random one not among the last 4). Reset by the level loader (`0xedec8`). | CONFIRMED (missions_20) |
| `0xb4df8` / `0xb4ed8` | 6 / 45 | `RefreshPhones()` / `KillAllPhones()` | §4.1 | CONFIRMED |
| `0xb4f5a`, `0xb4fb2`, `0xb5010`, `0xb5030`, `0xb41dc`, `0xa692a` | | trigger functions | §3.1 | CONFIRMED |
| `0x3fc8c` | 43 | `Random(n)` | `n * table[++i] >> 9`, i a u16 wrapping at 65536, over `General/RandomNumbers` (65536 BE shorts; regenerated as `rand()>>6` if missing). Shipped table: 0..511, so the result is always 0..n−1 (OS4 `FUN_0103ac94`). | CONFIRMED |

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
| 0x0b | `0x10cf8` | `AddMoney` |
| 0x13 | `0x10b80` | `SetPlayerVehicleId` |
| 0x1b | `0x10e26` | `AddLives` |
| others (0x05, 0x0c–0x12, 0x14–0x1a, 0x1c–0x36) | see `analysis/ghidra_script00/` | not mapped |

Game → script messages: type 1 connect, type 2 goodbye, event messages carry the event
number in `+0x16`.

## 7. Progression, level select, saving (68k addresses; CONFIRMED unless marked)

**Menus.** Title menu `0x33192` (2090 Start new game, 2091 Load, 2092 replay, 2093 Credits,
...). New game asks for a name (default "Travis", up to 25 chars, `0x136f30`), clears best
times `0x136f38[6] = -1` and trophies `0x136f50[6] = 0`, and opens the game menu `0x31150`:
0 "Play single player game" (2134) → level select `0x2c8ea`; 1 multiplayer; 2 rampage
(sets `0x11a6e6`); 3 Save game; 4/5 replays; 6 End game. "GAME COMPLETE!" (2047) is shown
under item 0 once every level has a trophy.

**Level select** (`0x2c8ea`, title 2087). Six entries 00, 01, 10, 11, 20, 21 (index i →
`0x3481d8`; city = i>>1 → `0x348194`, level = i&1 → `0x34c418`; police AI difficulty also
reads `0x3481d8`). The cursor starts on the first level without a trophy. A level can be
started only if i == 0 or the previous level has a trophy (else sound 0x2d): **strictly
linear unlock 00→01→10→11→20→21**. An intro card (`0x2c05a`) shows the city name
(2170+city), level name (2180+i: Ready Or Not, Gangster Tripping, Going For Gold, Remote
Control, Caught By The Fuzz, Born Slippy), gold/silver times and the best time.

**Level start** (every level, before `Mission_Init`): multiplier 1, **lives 3**, state 0,
visible layers 3, traffic model −1, marker off, level timer `0x349180 = 0`, spawn point −1,
money 0, no weapons or ammo (the player object is zero-filled; CONFIRMED, 68k `0xec686`). Level inits then add lives: 00 +2, 01 +2, 10 +1,
11 +1, 20/21 none.

**Level end.** State 3 (complete) → `0x2b666(city, level, minutes = 0x349180 / 1800)` (the
timer only runs in state 0), then back to the menu: **no automatic advance**. State 2 (game
over) → `0x29e1a` (2045/2046), nothing recorded.

**Trophies** (`0x2b666`, OS4 `0x0102395c`). Table `0x137758[city*2+level] = {gold, silver}`
in game minutes: 00 8/12, 01 10/14, 10 11/15, 11 8/12, 20 4/7, 21 10/15. t ≤ gold → gold
(3, 2043), t ≤ silver → silver (2, 2042), else bronze (1, 2041); "Total time:" 2040.
Best time and trophy are written together only if t < best or best == −1; t ≥ 1,080,000 is
ignored. Overall rating = minimum trophy over the 6 levels; when it rises the GAME COMPLETE
screen `0x2a842` shows 2047/2048, 2072+r (rating) and 2075+r (hint / "Well done! You got
gold!"). 2159 "View outro" is unreferenced (INFERRED: outro unused). A game minute is
1800 ticks = 72 s real time on 68k (25 ticks/s).

**Pause-menu goal** (`0x2550a`, single player): goal `0x34c440` ≤ 0 → 2056 "Goal: Escape from
jail" (level 20), else "Goal: Get " + amount + " points" (2058/2057).

**Saving** (only on demand from the game menu between levels; no autosave). Load `0x1dfb4`
(OS4 `0x0101182c`), Save `0x1e198` (OS4 `0x01011a70`), slot-exists `0x36d5c`, slot name
`0x36df8`. Path `PROGDIR:Saves/PaybackSlot%d.save`, fallback `S:PaybackSlot%d.save`;
14 slots. Default slot name `"%s - C%d, L%d"` (player name, next unfinished city+1, level+1).
File = 152 bytes, big-endian:

| Off | Type | Field |
|---|---|---|
| 0 | u32 | version (1; ignored on load) |
| 4 | char[50] | slot name |
| 54 | char[50] | player name |
| 104 | 6 × {s32 best_minutes (−1 none), s32 trophy 0–3} | levels 00, 01, 10, 11, 20, 21 |

No lives, money or multiplier are saved; unlocks are derived from the trophies.

**Demo build** (`0x34c114 != 1`): 9000-tick limit → 2490 "Time out!".
