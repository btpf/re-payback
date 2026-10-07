# Level 20 (city 2, level 0): "Escape from jail"

Update 7 reconstruction of the level script (`Maps/20.script` in the CD release). Markers:
**CONFIRMED** = read from the 68k code or disassembly; **INFERRED** = deduced from context.
Addresses: 68k = Update 7 `Payback.exe` flattened at 0x10000; OS4 = PaybackOS4. API names
come from `docs/re/script_api.md`. Coordinates are raw world units, with squares
(`v>>11`, fractional part where it matters) in brackets. Phrases are from
`Phrases/20.English.phrases` unless marked "global".

| Role | 68k | OS4 |
|---|---|---|
| Init | `0xb98f4` | `0x010eb3d0` (inlines `0xb5468(-1)`) |
| Event handler | `0xc73b4` | `0x010f45cc` routes 100000–199999 → `0x010d9ff4`, 300000–399999 → `0x010da308`, others → `0x010de0b4` |
| Roof/indoor rectangles | `0xb5468` | inlined / `0x010d9ff4` |
| Guards: create, reset, patrol, sight | `0xb5920` | `0x010da308` |
| Alarm | `0xb5692` | `0x010d1664` |

OS4 matches the 68k logic everywhere I compared: the same constants and phrases, and even the
missing `|0x400` on guard 300200's x (CONFIRMED).

---------------------------------------------------------------------------------------------

## 0. Summary

This level has no phone missions. It is a single scripted jailbreak:

1. The player starts locked in a cell on floor 1 of the prison (squares 12–13 × 19–20). An
   accomplice (actor 1) walks to the cell door, says "Psst! Over here!", and opens the cell
   once the player comes close.
2. 15 prison guards (police-officer peds) patrol fixed routes or stand guard. **If a guard
   sees the player outside the cell, or the player attacks or kills a guard, the alarm goes
   off**: alarm siren sound, wanted level 6, a helicopter flies to the helipad, and every
   guard attacks the player.
3. Optional pickups: key 1 at (8,40) opens a locked door at (33/34,17). Key 2 at (37,38)
   opens the weapon store at (33/34,9), which holds weapon crates at (29–32, 8–11).
4. When the player leaves the prison area (squares 0–60 × 0–60) the alarm is raised if it is
   not already on. All guards are then released, wanted level is 6, and the player is told to
   drive to "our garage in the East".
5. Reaching the garage at square (107,3) opens its door. Entering the garage square triggers
   `LevelComplete()`.
6. **Any death or arrest before the end resets the prison.** The player respawns in the
   cell, the alarm is cancelled, and every guard, the boat and the "escaped" trigger are
   re-created.

**The goal (CONFIRMED).** Init calls `AddTrigger(-1, 0, -1, …)`. In `AddTriggerEx`
(`0xb2b12`, case 0 at `0xb2bce`) type 0 **always** stores `a` as the HUD goal
(`0xefe52` → `0x34c440 = a`) and returns without adding a trigger when `a < 1`. So the goal
is −1 and no money trigger exists. The in-game pause menu `FUN_0002550a` (called from
`FUN_000da2e0` when the menu-key flag `0x34c180` is set) prints:

* `Phrase(2056)` = global **"Goal: Escape from jail"** when `GetGoal()` (`0xf0154`) `< 1`;
* otherwise `sprintf("%s%d%s", Phrase(2058) "Goal: Get ", goal, Phrase(2057) " points")`.

Phrase 2056 is hard-coded for any goal < 1, and level 20 is the only level that uses one. The
level ends only through `LevelComplete()` (`0xb4cd0`) at event 200003.

---------------------------------------------------------------------------------------------

## 1. Level setup: `Init` 68k `0xb98f4` (CONFIRMED, checked against the disassembly)

```c
intro_phrase(0x2fb224) = 1;     // "The pigs did a raid on your house and put you away for life. ..."
alarm(0x2fb9fc)        = 0;
escaped(0x2fba0c)      = 0;
AddTrigger(-1, 0x00, -1, -1, -1, -1);   // goal = -1 -> pause menu shows "Goal: Escape from jail"; no trigger
Rects_Event(-1);                        // 0xb5468: 23 indoor rectangles, see §1.3
RefreshPhones();                        // 0xb4df8: no slots filled; first call -> Dialog(2): Say2(1); End
ShowMessage(0);                         // big centre message: phrase 0 "Escape from jail"

AddMissionTrigger(30,  0x2f, 8, 40, 0, -1);           // key 1 picked up: cell (8,40) cube 0 scenery -> 0 (map scenery 34 = key)
AddMissionTrigger(31,  0x41, 0x11000, 0x8c00, 0, -1); // on foot within ±0x200 of (34.0,17.5) floor 0: locked door 1
key1(0x2fba10) = 0;
AddMissionTrigger(40,  0x2f, 37, 38, 0, -1);          // key 2 picked up: cell (37,38) cube 0 (map scenery 34 = key)
AddMissionTrigger(41,  0x41, 0x11000, 0x4c00, 0, -1); // on foot at (34.0,9.5) floor 0: weapon-store door
key2(0x2fba14) = 0;
AddMissionTrigger(50,  0x3f, 0x13000, 0xc800, 0, -1); // within ±6000 of helipad (38,25) floor 0
AddMissionTrigger(51,  0x3f, 0x16000, 0x5c00, 0, -1); // within ±6000 of (44,11) floor 0 (guarded exit/boat area)

CreateActor(1, -10 /*ped model 9 = PedMale3*/, 0x7400, 0xc400 /*(14.5,24.5)*/, 1, 0, 0);  // the accomplice
AddMissionTrigger(1,   0x17, 1, 29000 /*0x7148*/, 0xa400, -1);  // accomplice walks to (14.2,20.5), the cell door
AddMissionTrigger(20,  0x3e, -1, -1, -1, -1);         // player respawned after death/arrest
AddMissionTrigger(10,  0x3a, 0x6000, 0x9800, 0x7000, 0xa800);  // player left the cell rectangle
in_cell(0x2fba00) = 1;
SetSpawnPoint(12, 20, 1);    // 0xb48be -> 0x922ec: forced (re)spawn square (12,20) cube 1 = inside the cell
Guards_Event(-1);            // 0xb5920: create boat, 15 guards, escape trigger; see §1.2
```

Lives: not touched (no `AddLives`). Phone slots: none. `RefreshPhones()` is never called
again, so "head back to the phones" is never said. Mission_Shutdown does nothing special for
this level.

The cell rectangle is `(0x6000,0x9800)-(0x7000,0xa800)`, squares x 12–13, y 19–20. Map 2
has a bed at (12,19) cube 1, and the bars are on the E face of (13,20) and the W face of
(14,20).

### 1.1 `SetSpawnPoint` — new API name for `0xb48be` (was "Set26c040"), CONFIRMED

`0xb48be(sx, sy, cube)` → `0x922ec` stores `0x26c040/44/48`. The player-placement
functions `0x43f0e` (level start, called at `0xee326`) and `0x43c96` (respawn, called at
`0xeed98` right after `Trigger_PlayerRespawned`) use that square centre
(`sx*0x800+0x400, sy*0x800+0x400`) and stack height up to cube `cube`, unless
`0x26c040 == -1`. With −1 they fall back to map start points: `0x43f0e` uses the first one
(`0x26c500`); `0x43c96` picks a random one that was not among the last 4 used. The level
loader resets it to (−1,0,0) at `0xedec8`. Level 20 sets (12,20,1), so **every respawn is
in the cell**. At the end (event 200003) it sets (−1,0,0).

Order caveat (INFERRED): the initial player placement at `0xee326` precedes the
`Mission_Init` call at `0xeea3a` in the main-loop code. The very first spawn therefore
probably uses the map's first start point, which on map 2 is presumably the cell.

### 1.2 Actors created by `Guards_Event(-1)` (68k `0xb5920`, first branch; CONFIRMED)

```c
AddMissionTrigger(200000, 0x3a, 0, 0, 0x1e800, 0x1e800);   // player outside squares 0..60 x 0..60 = escaped the prison area
CreateActor(1000, 23 /*vehicle 23 = Boat*/, 0x17c00, 0x6400 /*(47.5,12.5)*/, 0, 0x5a0 /*90°*/, 0);
```

The boat sits on water: map cell (47,12) cube 0 has a type-4 (reflect/water) top. It gets
no trigger. INFERRED: it is an optional escape vehicle.

Guards all use model −8 = ped model 7 = `PoliceMale1`, colour 0. Waypoints are square
centres, `(X<<11)|0x400, (Y<<11)|0x400`. **Exception:** guard 300200 uses `X<<11` with no
+0x400, a bug kept in OS4. Each patrolling guard gets:

```c
CreateActor(G, -8, wp[start].x, wp[start].y, floor, angle, 0);
AddMissionTrigger(G,    0x17, G, wp[next].x, wp[next].y, -1);   // walk to next waypoint; event = actor id
AddMissionTrigger(G+10, 0x31, G, -1, -1, -1);                   // G sees the player (no arrow)
AddMissionTrigger(G+10, 0x30, G, -1, -1, -1);                   // G attacked by the player or destroyed
```

Static guards get no 0x17 trigger.

| Ids (G) | Sight ev | Floor | Angle | Route state | Waypoints [squares] | Start → first target |
|---|---|---|---|---|---|---|
| 300000–300003 ("group A") | 300010–300013 | 1 | 0 | `0x2fb030[4]`, wraps after 11 | X `0x11a524` / Y `0x11a554`: 0 (14,18), 1 (6,18), 2 (6,15), 3 (14,15), 4 (14,7), 5 (17,7), 6 (17,15), 7 (25,15), 8 (25,18), 9 (17,18), 10 (17,26), 11 (14,26) | guard i: wp[3i] → wp[3i+1] (state = 1,4,7,10) |
| 300100 | 300110 | 0 | 0 | static | (0xcc00,0x8000) = (25.5,16.0) | – |
| 300101 | 300111 | 0 | 0xb40 (180°) | static | (0xcc00,0x9000) = (25.5,18.0) | – |
| 300200 | 300210 | 0 | 0xb40 | `0x2fb040`, wraps after 1 | `0x11a584`/`0x11a58c`: 0 (0x8000,0xcc00)=(16.0,25.5), 1 (0x8000,0x4400)=(16.0,8.5) | wp0 → wp1 |
| 300300–300301 | 300310–300311 | 1 | 0 | `0x2fb048[2]`, wraps after 3 | `0x11a594`/`0x11a5a4`: 0 (29,33), 1 (29,30), 2 (32,30), 3 (32,33) | guard j: wp[2j] → wp[2j+1] |
| 300400 | 300410 | 0 | 0xb40 | `0x2fb050`, wraps after 1 | `0x11a5b4`/`0x11a5bc`: 0 (33,19), 1 (33,14) | wp0 → wp1 |
| 300500–300501 | 300510–300511 | 1 | 0 | `0x2fb058[2]`, wraps after 9 | `0x11a5c4`/`0x11a5ec`: 0 (42,36), 1 (42,22), 2 (36,22), 3 (36,14), 4 (35,14), 5 (35,7), 6 (35,14), 7 (36,14), 8 (36,22), 9 (42,22) | guard j: wp[5j] → wp[5j+1] |
| 300600 | 300610 | 0 | 0xb40 | `0x2fb060`, wraps after 1 | `0x11a614`/`0x11a61c`: 0 (42,11), 1 (36,11) | wp0 → wp1 |
| 300700 | 300710 | 0 | 0x10e0 (270°) | static | (0x17400,0x5c00) = (46.5,11.5) | – |
| 300800 | 300810 | 0 | 0xb40 | `0x2fb064`, wraps after 3 | `0x11a624`/`0x11a634`: 0 (10,41), 1 (6,41), 2 (6,39), 3 (10,39) | wp0 → wp1 |

World values for the table, all `sq*0x800+0x400`: e.g. group A wp0 = (0x7400,0x9400),
wp4 = (0x7400,0x3c00), wp7 = (0xcc00,0x7c00), wp10 = (0x8c00,0xd400). Group 300300 wp0 =
(0xec00,0x10c00). Group 300500 wp0 = (0x15400,0x12400), wp5 = (0x11c00,0x3c00). Group 300800
wp0 = (0x5400,0x14c00). The `0x11a524–0x11a643` data is byte-identical in the CD script
(`script20 0x181f4..`).

### 1.3 Indoor rectangles / roof cut-away (`Rects_Event`, 68k `0xb5468`, CONFIRMED)

Tables (s32 ×23): x0 `0x11a358`, y0 `0x11a3b4`, x1 `0x11a410`, y1 `0x11a46c`, floor
`0x11a4c8`. The current rectangle is in `0x2fb028` (−1 = none).

```c
Rects_Event(-1):   // init
  for i in 0..22: AddTriggerEx(100000+i, 0x39, x0[i]<<11, y0[i]<<11, (x1[i]+1)*0x800, (y1[i]+1)*0x800, floor[i]);  // untracked
  cur = -1;
Rects_Event(ev in 100000..100022):   // player entered rect i = ev-100000 on floor[i]
  if (cur != -1) re-add rect cur's 0x39 trigger (as above);
  cur = i;
  SetAmbience(floor[i] + 1);
  KillTrigger(100100);
  AddTrigger(100100, 0x3a, x0[i]*0x800-0x400, y0[i]*0x800-0x400, x1[i]*0x800+0xc00, y1[i]*0x800+0xc00); // untracked, half-square margin
Rects_Event(100100):  // player left the current rect (+ margin)
  if (cur != -1) re-add rect cur's 0x39 trigger;
  cur = -1;
  SetAmbience(3);
```

The handler sends every event in 100000–199999 here; only 100000–100022 and 100100 do
anything.

**Correction to `SetAmbience` (0xb4c58), CONFIRMED.** `0xefd60` stores `0x34c078`, which is
the **number of map cube layers drawn**. The renderer `FUN_000d1058` draws layers
`0..min(0x34c078,2)` and also uses it in its cube-0 test (68k decompile ~112780). So `n` = 1
draws the ground layer only, 2 draws two layers, 3 draws all. Inside a building on floor f
the roofs above the player disappear. If the rain/ambient loop exists (`0x34c434 >= 0`), its
volume is set to 0xff for n == 3 (outdoors) and 0x7f otherwise (muffled). A better name is
`SetVisibleLayers(n)`. Rectangles on floor 2 give n = 3, the same as outdoors.

| i | Event | Squares x × y (inclusive) | Floor | Enter rect (0x39) a,b,c,d | Layers | Exit rect (ev 100100, 0x3a) |
|---|---|---|---|---|---|---|
| 0 | 100000 | 26–27 × 16–17 | 0 | `0xd000,0x8000,0xe000,0x9000` | 1 | `0xcc00,0x7c00,0xe400,0x9400` |
| 1 | 100001 | 30–31 × 20–21 | 0 | `0xf000,0xa000,0x10000,0xb000` | 1 | `0xec00,0x9c00,0x10400,0xb400` |
| 2 | 100002 | 28–33 × 7–12 | 0 | `0xe000,0x3800,0x11000,0x6800` | 1 | `0xdc00,0x3400,0x11400,0x6c00` |
| 3 | 100003 | 28–33 × 14–19 | 0 | `0xe000,0x7000,0x11000,0xa000` | 1 | `0xdc00,0x6c00,0x11400,0xa400` |
| 4 | 100004 | 27–34 × 22–41 | 0 | `0xd800,0xb000,0x11800,0x15000` | 1 | `0xd400,0xac00,0x11c00,0x15400` |
| 5 | 100005 | 21–40 × 28–35 | 0 | `0xa800,0xe000,0x14800,0x12000` | 1 | `0xa400,0xdc00,0x14c00,0x12400` |
| 6 | 100006 | 6–25 × 13–20 | 0 | `0x3000,0x6800,0xd000,0xa800` | 1 | `0x2c00,0x6400,0xd400,0xac00` |
| 7 | 100007 | 12–19 × 7–26 | 0 | `0x6000,0x3800,0xa000,0xd800` | 1 | `0x5c00,0x3400,0xa400,0xdc00` |
| 8–11 | 100008–100011 | same as 4–7 | 1 | same as 4–7 | 2 | same as 4–7 |
| 12–15 | 100012–100015 | same as 4–7 | 2 | same as 4–7 | 3 | same as 4–7 |
| 16 | 100016 | 15–16 × 27 | 0 | `0x7800,0xd800,0x8800,0xe000` | 1 | `0x7400,0xd400,0x8c00,0xe400` |
| 17 | 100017 | 20 × 31–32 | 0 | `0xa000,0xf800,0xa800,0x10800` | 1 | `0x9c00,0xf400,0xac00,0x10c00` |
| 18 | 100018 | 6–19 × 28–41 | 0 | `0x3000,0xe000,0xa000,0x15000` | 1 | `0x2c00,0xdc00,0xa400,0x15400` |
| 19 | 100019 | 20–25 × 37–41 | 0 | `0xa000,0x12800,0xd000,0x15000` | 1 | `0x9c00,0x12400,0xd400,0x15400` |
| 20 | 100020 | 36–42 × 37–41 | 0 | `0x12000,0x12800,0x15800,0x15000` | 1 | `0x11c00,0x12400,0x15c00,0x15400` |
| 21 | 100021 | 43–45 × 10–12 | 0 | `0x15800,0x5000,0x17000,0x6800` | 1 | `0x15400,0x4c00,0x17400,0x6c00` |
| 22 | 100022 | 38–42 × 15–19 | 0 | `0x13000,0x7800,0x15800,0xa000` | 1 | `0x12c00,0x7400,0x15c00,0xa400` |

Notes:

* Type 0x39 fires only for the *controlled* player and requires the floor match. Type 0x3a
  has no floor argument, so changing floor inside the same XY area does not fire 100100.
  The other floor's 0x39 trigger is still armed, though, and switches `cur` directly.
* Init never calls `SetAmbience`, so the level starts with the engine default
  (INFERRED: 3).
* The cell (floor 1) lies inside rect 10 (6–25 × 13–20), so the first frame triggers
  100010 → 2 layers.

---------------------------------------------------------------------------------------------

## 2. Progression graph

```
           [init: cell, accomplice walks]                         death/arrest at any time before 200003
                     | ev1 accomplice at door                      -> ev20: alarm off, full prison reset (§4)
                     v                                                and respawn in the cell
           ev2 player near accomplice -> cell bars removed
                     | (ev3 -> ev4 accomplice walks off and is released)
                     v
  in_cell toggles (ev10 leave cell / ev11 re-enter)
  outside the cell: any guard 0x31 sight / 0x30 hit  --------> ALARM (0xb5692)
  optional: ev30 key1 -> ev31 door (33/34,17)                         |
  optional: ev40 key2 -> ev41 weapon store (33/34,9)                  |
  hints: ev50 helipad, ev51 guarded area                              |
                     | player leaves squares 0..60 (ev200000)  <------+
                     v
  ALARM if not yet + release guards + wanted 6 + "Drive to our garage in the East"
                     | ev200002: within ±10000 of (107.5,3.5)
                     v
  garage door opens, "You made it - come in!"
                     | ev200003: within ±0x400 of (107.5,3.5), floor 0
                     v
  "I knew you could do it!" -> SetSpawnPoint(-1) -> escaped=1 -> LevelComplete()
```

---------------------------------------------------------------------------------------------

## 3. Event-by-event state machine

Handler 68k `0xc73b4(ev)` (CONFIRMED from the disassembly `0xc73b4–0xc79ca`):

```c
if ((unsigned)(ev - 300000) < 100000) { Guards_Event(ev); return; }   // 0xb5920
if ((unsigned)(ev - 100000) < 100000) { Rects_Event(ev);  return; }   // 0xb5468
switch (ev) { ... below ... default: return; }                        // ev 0 (game-over tick) ignored
```

Dialogue notation: `D(s, SayN(p))` = `Dialog_Begin(s); Dialog_SayN(p); Dialog_End()`.
Speaker 1 is used for the player's own lines (Say1 style) and for the accomplice (Say2).
Speaker 2 is the gang on the radio (INFERRED).

### 3.1 The accomplice and the cell

| Ev | Trigger that sends it | Action |
|---|---|---|
| 1 | `0x17` actor 1 arrived at (29000,0xa400) = (14.2,20.5) | `D(1, Say2(4))` "Psst! Over here!"; `AddMissionTrigger(2, 0x3c, 1, -1,-1,-1)` (player within ±1000 of actor 1, same floor; arrow). |
| 2 | `0x3c` player near actor 1 | `D(1, Say2(5))` "I've been sent to pick open your cell. Make the most of it."; `SetFaceE(13,20, cube 2, type 0, tex 0, flip 0)`; `SetFaceW(14,20, 2, 0,0,0)`, which removes the bars between the cell and the corridor (map: both faces are type 2 genlocked, tex 0x25d). `AddMissionTrigger(3, 0x17, 1, 0x7400, 0x9400, -1)`: actor 1 walks to (14.5,18.5). |
| 3 | `0x17` actor 1 arrived | `AddMissionTrigger(4, 0x17, 1, 0x30d4, 0x9664, -1)`: walks to (12500,38500) = (6.1,18.8). |
| 4 | `0x17` arrived | `Actor_Release(1)` (becomes an ordinary pedestrian). |
| 10 | `0x3a` player outside the cell rect `(0x6000,0x9800)-(0x7000,0xa800)` | `in_cell = 0`; `AddMissionTriggerEx(11, 0x39, 0x6000,0x9800,0x7000,0xa800, floor 1)`. |
| 11 | `0x39` player back inside the cell rect on floor 1 | `in_cell = 1`; `AddMissionTrigger(10, 0x3a, 0x6000,0x9800,0x7000,0xa800)`. |

Faces: the player stands on top of cube 1, so the walls around him are cube 2's sides. This
is why floor-1 doors edit cube 2 and floor-0 doors edit cube 1 (INFERRED from the map data:
(13,20) has bars on both cube 1 and cube 2, and only the cube-2 face is removed).

### 3.2 Keys, doors and hints (all player lines use speaker 1, `Dialog_Say1`)

```c
case 30:   // 0x2f: key 1 cell (8,40,0) scenery became 0 (picked up)
  D(1, Say1(6));                               // "This could come in handy."
  if (key1 == 2) AddMissionTrigger(31, 0x41, 0x11000, 0x8c00, 0, -1);   // re-arm door if already tried
  key1 = 1;
case 31:   // 0x41: on foot within ±0x200 of (34.0,17.5), floor 0
  if (key1 == 1) {
    D(1, Say1(8));                             // "Great! The key fits!"
    SetFaceW(34,17, 1, 0,0,0); SetFaceE(33,17, 1, 0,0,0);   // open door between squares 33 and 34 at y=17
  } else {
    key1 = 2; D(1, Say1(7));                   // "Damn! It's locked. I better go back and look for a key."
  }                                            // (trigger consumed; re-armed by ev30)
case 40:   // 0x2f: key 2 cell (37,38,0) picked up
  D(1, Say1(9));                               // "I wonder what this key is for?"
  if (key2 == 2) AddMissionTrigger(41, 0x41, 0x11000, 0x4c00, 0, -1);
  key2 = 1;
case 41:   // 0x41: on foot at (34.0,9.5), floor 0
  if (key2 == 1) {
    D(1, Say1(11));                            // "It fits! Now I should be able to fight my way out!"
    SetFaceW(34,9, 1, 0,0,0); SetFaceE(33,9, 1, 0,0,0);     // weapon store door
    store_open(0x2fba50) = 1;
  } else {
    key2 = 2; D(1, Say1(10));                  // "This must be the weapon store. I bet the guards keep the key in their quarters."
  }
case 50:   // 0x3f: within ±6000 of (0x13000,0xc800) = helipad (38,25), floor 0. One-shot.
  if (alarm == 0) D(1, Say1(12));              // "Hmm..a helipad. Perhaps reinforcements are sent in by helicopter?"
case 51:   // 0x3f: within ±6000 of (0x16000,0x5c00) = (44,11), floor 0. One-shot.
  if (alarm == 0 && store_open == 0) D(1, Say1(13));   // "It's too heavily guarded here. I'm going to have to get some serious fire power..."
```

Map 2 contents (from `Maps/2.map`): the weapon store at squares 29–32 × 8–11 holds 16 crates
(specials 3–6, i.e. crate contents), next to the door at x 33/34, y 9. There is also a
pistol pickup at (23,10).

### 3.3 Guards: `Guards_Event(ev)` for ev 300000–399999 (68k `0xb6878..`, CONFIRMED)

```c
if (alarm != 0) return;                         // once the alarm is on, patrol/sight events are ignored
for i in 0..3:                                  // group A
  if (ev == 300000+i) { st=++A[i]; if (st > 11) st = A[i] = 0;
                        AddMissionTrigger(ev, 0x17, ev, wpA[st]); }      // next waypoint
  else if (ev == 300010+i) {
    if (in_cell == 0)      Alarm();
    else if (in_cell == 1) AddMissionTrigger(ev, 0x31, 300000+i, -1,-1,-1);  // ignore while player is in his cell; re-arm sight only
  }
if (ev == 300110 || ev == 300111) Alarm();
if (ev == 300200) { st = ++B; if (st > 1) st = B = 0; AddMissionTrigger(ev, 0x17, ev, X200[st]<<11, (Y200[st]<<11)|0x400); }
else if (ev == 300210) Alarm();
for j in 0..1:
  if (ev == 300300+j) { st=++C[j]; if (st > 3) st=C[j]=0; AddMissionTrigger(ev,0x17,ev, wp300[st]); }
  else if (ev == 300310+j) Alarm();
if (ev == 300400) { cycle 0..1 } else if (ev == 300410) Alarm();
for j in 0..1:
  if (ev == 300500+j) { cycle 0..9 } else if (ev == 300510+j) Alarm();
if (ev == 300600) { cycle 0..1 } else if (ev == 300610) Alarm();
if (ev == 300710) Alarm();
if (ev == 300800) { cycle 0..3; return; }
if (ev == 300810) Alarm();
```

Note: when a group-A guard's 0x31 fires while the player is in the cell, only the 0x31
trigger is re-added. The 0x30 (attacked) trigger with the same event stays armed. Hitting a
group-A guard from inside the cell is therefore ignored the first time; after that only
sight re-arms.

### 3.4 Alarm: `Alarm()` 68k `0xb5692` (CONFIRMED from the disassembly)

```c
if (alarm != 0) return;
CreateActor(1001, 17 /*Helicopter*/, 0x29c00, 0x19400 /*(83.5,50.5)*/, 0, 0, 4 /*colour*/);
Actor_Siren(1001, on);                                          // 0xb4666
AddMissionTrigger(200001, 0x17, 1001, 0x13000, 0xc800, -1);     // fly to the helipad (38.0,25.0)
for every guard G (ids above):
  KillMissionTrigger(G);           // patrol trigger (also called for static guards; harmless)
  KillMissionTrigger(G+10); KillMissionTrigger(G+10);
  Actor_Attack(G, 0x50, -1);       // attack the player
alarm = 1;
D(2, Say2(3));                     // "You tripped the alarm! You've just made escaping about 10 times harder!"
SetWantedLevel(6);
AddSound(0, 0x40, 1, 1000000000, 0x2b11, 0xc800, 0xc800, 19000);   // looping alarm at (25,25), range 19000
```

The static guards get only the sight-event kills (300110/300111 and 300710, each twice),
not `KillMissionTrigger(G)`. Ordering in the code: group A, 300100/1, 300200, 300300/1,
300400, 300500/1, 300600, 300700, 300800.

| Ev | Trigger | Action |
|---|---|---|
| 200001 | `0x17` helicopter 1001 arrived at the helipad | `Actor_Siren(1001, off)` (0xb46dc; also zeroes speed). The helicopter just sits there. |

### 3.5 Escape and level end (CONFIRMED)

```c
case 200000:  // 0x3a: player outside (0,0)-(0x1e800,0x1e800) = left squares 0..60 (the prison block)
  Alarm();                                     // no-op if already raised
  for k in 0..3: Actor_Release(300000+k);
  for k in 0..1: Actor_Release(300100+k); Actor_Release(300300+k); Actor_Release(300500+k);
  Actor_Release(300200); Actor_Release(300400); Actor_Release(300600); Actor_Release(300700); Actor_Release(300800);
  SetWantedLevel(6);
  D(2, Say2(2));   // "The guards have noticed you're gone and have set off the alarm. Drive to our garage in the East of the city, but watch out for the law!"
  AddMissionTrigger(200002, 0x1a, 0x35c00, 0x1c00, 0, -1);   // within ±10000 of (107.5,3.5), floor 0 (arrow)
case 200002:
  SetFaceE(107, 3, 1, 0,0,0);                  // open garage door (E face of (107,3) cube 1)
  D(2, Say2(14));                              // "You made it - come in!"
  AddMissionTrigger(200003, 0x40, 0x35c00, 0x1c00, 0, -1);   // within ±0x400 of (107.5,3.5), floor 0, any mode (arrow)
case 200003:
  D(2, Say2(15));                              // "I knew you could do it! It's great to have you back!"
  SetSpawnPoint(-1, 0, 0);
  escaped = 1;
  LevelComplete();                             // state 3, global 2023 "Level complete!", sound 0x1a
```

If the alarm was not already on at 200000, phrases 3 and 2 are queued back to back.

* **Rewards:** none. The level never calls `AddMoney`, `ShowMessage(2022)` or `AddLives`.
* **Failure:** none scripted; dying only resets the prison (§4).
* **Released guards:** after 200000 they become ambient AI, and the alarm sound keeps
  playing until death or the end of the level.

---------------------------------------------------------------------------------------------

## 4. Global events

| Ev | Trigger | Handling |
|---|---|---|
| 20 (0x14) | `0x3e` player respawned after death/arrest | Re-arm: `AddMissionTrigger(20, 0x3e, …)`. If `alarm == 1`: `DeleteSound(0)`, `alarm = 0`. If `escaped == 0`: `Guards_Event(-2)` (full reset, below). The player reappears at the spawn point (12,20) cube 1 = the cell (§1.1). |
| — | type 6 (death) | not used |
| 0 | game-over tick | ignored (falls through the switch) |

`Guards_Event(-2)` (68k `0xb5ff2–0xb6874`, CONFIRMED):

```c
KillMissionTrigger(200000); KillMissionTrigger(200002); KillMissionTrigger(200003);
AddMissionTrigger(200000, 0x3a, 0, 0, 0x1e800, 0x1e800);
RemoveAllActors();                 // 0xb4602: boat, helicopter, guards, accomplice, all ambient actors
KillMissionTrigger(200001);
CreateActor(1000, 23, 0x17c00, 0x6400, 0, 0x5a0, 0);     // boat again
for every guard: KillMissionTrigger(G); KillMissionTrigger(G+10) (x2);
                 reset route state to its initial value; CreateActor + 0x17 + 0x31 + 0x30 exactly as in §1.2
```

**Not reset** (CONFIRMED by absence):

* `in_cell` (re-synchronised by the pending ev10/ev11 trigger);
* `key1`, `key2`, `store_open`;
* map edits (opened cell bars, doors, garage door, collected keys);
* the roof-rectangle state;
* triggers 30/31/40/41/50/51 (consumed one-shots stay consumed);
* the accomplice. He is removed and not re-created, but the cell bars are already open.

Wanted level and money after respawn are handled by the engine, not the script.

---------------------------------------------------------------------------------------------

## 5. Differences from the legacy CD script (`Maps/20.script`, `analysis/ghidra_script20/`)

The logic is the same, step for step. CD `main` = `0x135bc`, alarm = `0x11704`,
guards = `0x11a8e`, rects = `0x1140c`. The data tables `0x18028–0x18313` are byte-identical
to 68k `0x11a358–0x11a643` (checked).

* CD runs as a blocking `GetEvent()` loop that exits on event 0. Update 7 is a callback that
  ignores event 0.
* Ghidra artefacts in the CD decompile, so not real differences: the helicopter trigger shows
  as event `0x41`, but the bytes at `script20:0x1175c` are `2f3c 0003 0d41` = 200001; the
  alarm `AddSound` shows 2 args, but the disassembly at `0x11a5c` pushes the same 8 args.
  Some CD stubs pass `0xffff` (16-bit) where Update 7 passes −1 (e.g. the unused 5th/6th
  args).
* The in-cell flag and the weapon-store flag were locals or globals in CD (`bVar2`,
  `0x3a8ac`) and are globals in Update 7 (`0x2fba50`, `0x2fba00`). No behavioural change.

---------------------------------------------------------------------------------------------

## 6. Corrections and new facts for `script_api.md`

* **Type 0x00:** the goal `0x34c440 = a` is stored **even when a ≤ 0** (`0xb2bce`). Goal < 1
  makes the pause menu (`FUN_0002550a`) show global 2056 "Goal: Escape from jail".
* **`0xb48be` = `SetSpawnPoint(sx, sy, cube)`**: forced start/respawn square (`0x26c040`).
  −1 = use map start points. Used by `0x43f0e` (start) and `0x43c96` (respawn).
* **`SetAmbience` (`0xb4c58`) is really `SetVisibleLayers(n)`**: number of cube layers the
  renderer draws (roof cut-away), plus rain volume 0xff/0x7f.
* `0x30` / `0x31` are used on peds. `0x17` on a helicopter works as "fly to" (INFERRED).
* Vehicle model indices (load order in `0xd7874` → `0x348120+4*m`): 0 Mundaneo, 1 Pug,
  2 Diblo, 3 Bus, 4 Vapour, 5 Pug GTI, 6 Fjord, 7 Van, 8 Limo, 9 Police, 10 Scooby,
  11 Tank, 12 Evo, 13 Taxi, 14 IceCreamVan, 15 Pickup, 16 HotRod, **17 Helicopter**,
  18 Pug Racer, 19 Truck, 20 PoliceVan, 21 BombTruck, 22 RC car, **23 Boat**, 24 RocketCar,
  25–26 RocketCar thrust, 27 Vapour Racer. Ped models (`0x3480e0+4*m`): 0–5 MaleNormal
  variants, 6 PedMale1, **7 PoliceMale1**, 8 PedMale2, **9 PedMale3**, 10 PedMale4,
  11 PedMale5, 12 PedMale6, 13–14 MaleNormal variants, 15 PedMale7.

## 7. Open questions

1. Does the first spawn use (12,20,1)? Placement at `0xee326` seems to run before
   `Mission_Init` (`0xeea3a`). If so, map 2's first start point must be the cell; this has
   not been checked against the map's start-point list.
2. `AddSound` arg 4 = 1000000000: exact meaning (volume/priority/duration) for
   `FUN_000170ce`.
3. `Actor_Siren` on the helicopter: is it a visual/sound effect, or a no-op if the model
   lacks the `+0x4928` flag?
4. Whether `SetAmbience` is reset by the engine on respawn. After a reset the player
   respawns inside rect 10, which re-fires only if that rect's trigger is armed.
5. The boat (1000) has no script role. Confirm it is just an escape option, and whether the
   garage trigger 0x40 at floor 0 works from a boat.
