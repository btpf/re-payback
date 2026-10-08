# Level 10 (city 1 "Los Francos", level 0): mission script, Update 7

Status markers: **CONFIRMED** means read from the 68k disassembly (and cross-checked against OS4 where noted).
**INFERRED** means deduced from context. API names are the ones in `docs/re/script_api.md`.
Coordinates are given raw and as map squares `(x>>11, y>>11)`. One square is 0x800 units, and a square centre is
`sq<<11 | 0x400`. Phrase numbers refer to `Phrases/10.English.phrases` unless marked "global".

Working files (scratch): `analysis/missions_10/` (`h68k.dis` is the full disassembly with decoded jump
tables, `h68k.calls` holds the reconstructed call arguments of every call site, and the `calls.py` and `dis_all.py` tools).

## 0. Code map

| What | 68k | OS4 |
|---|---|---|
| Level init | `0xb953c` | `0x010eaf98` |
| Event handler | `0xc1a92` (code runs to `0xc4368`; Ghidra's size is short) | `0x010eef18` |
| Race helper (mission "1") | `0xb73d0` | `0x010e544c` (Ghidra name `switchD_010ef650::default`) |
| Mission 500 helper (Stan's van) | `0xb7844` | `0x010e5b7c` |
| Mission 600 helper (Ike's tail) | `0xb7c40` | `0x010e6684` |
| Unlock routine ("phone mission finished") | `0xb5230` | `0x010d9d08` |

Level globals (68k):

| Addr | OS4 | Use |
|---|---|---|
| `0x11a354` | `0x0114c370` | current mission = phone slot index (−1 = none). **The level code sets it itself** when a phone event arrives (0..7). |
| `0x2fba08` | `0x0114c97c` | per-mission counter `S` (enemies left / progress stage) |
| `0x2fb14c` | `0x01379c78` | per-mission value `P` (M100 pay factor, M200 "poison warning shown" flag) |
| `0x2fba2c` | `0x0114c970` | loop variable |
| `0x2fba4c` | `0x0114c964` | M200: gas sound ref 1 active |
| `0x2fba30` / `0x2fba34` | `0x01379b5c/60` | M400: henchman A / B alive |
| `0x2fb0c0[5]`, `0x2fb0d8[6]`, `0x2fb0f0[6]`, `0x2fb108[6] (u16)` | | race: AI waypoint index, checkpoint index, lap, finished flag |
| `0x2fb118[3]` | | M500: waypoint counters of actors 0x1f5..0x1f7 |
| `0x2fb128[2]` | | M600: waypoint counters of actors 0x259 (tail), 0x25a (Ike) |

### Dispatch order of the handler `0xc1a92` (CONFIRMED)

1. Events `100000..100003` and `100010..100013` (ambience rectangles, §1.3) are handled first in every state.
2. Event 0 returns immediately.
3. `switch (0x11a354)`: slot 0 → race, 1 → M100, 2 → M200, 3 → M300, 4 → M400, 5 → `Mission500(ev)` (`0xb7844`),
   6 → `Mission600(ev)` (`0xb7c40`), 7 → M700. Unsigned value > 7 (i.e. −1, no mission) → phone events:
   `1, 100, 200, 300, 400, 500, 600, 700` start the missions. Any other event is ignored.
   In slot 0, every event other than 1–5 and 10 goes to `Race10(ev)` (`0xb73d0`).

## 1. Level setup: init `0xb953c` (CONFIRMED, identical in OS4)

```
intro phrase 0x2fb224 = 0       // phrase 0 "Welcome to Los Francos city ... target 2,000,000 points. Answer the phones for missions."
slots (event / phone / hidden):
  slot 0: ev 1   phone 1  visible   (race)
  slot 1: ev 100 phone 2  visible
  slot 2: ev 200 phone 3  visible
  slot 3: ev 300 phone 4  visible
  (slots 4..19 empty; Mission_Init already set every slot to -1 and every hidden flag to 0)
AddTrigger(-1, 0x00, 2000000, -1,-1,-1)      // money goal 2,000,000 → level complete
AddLives(1)
RefreshPhones()                               // rings phones 1-4; first call → Dialog(2): phrase 0
ShowMessage(1)                                // big text: phrase 1 "Answer the phone"
CreateActor(id 1, model 17 = helicopter, (0x35000,0x1b800) sq (106,55), floor 0, angle 0, colour 1)
AddTriggerEx(100000, 0x39, 0x1000,0x34000, 0x9800,0x3d800, floor 0)   // rect R1, squares x 2..19, y 104..123
AddTriggerEx(100001, 0x39, R1, floor 1)
AddTriggerEx(100002, 0x39, R1, floor 2)
AddTriggerEx(100010, 0x39, 0x1a800,0x36800, 0x1e000,0x38800, floor 0) // rect R2, squares x 53..60, y 109..113
AddTriggerEx(100011, 0x39, R2, floor 1)
AddTriggerEx(100012, 0x39, R2, floor 2)
```

* Vehicle model 17 is `objects/vehicles/helicopter.obj`. Vehicle table `0x348120` in load order: 0 mundaneo, 1 pug, 2 diblo,
  3 bus, 4 vapour, 5 pug GTI, 6 fjord, 7 van, 8 limo, 9 police, 10 scooby, 11 tank, 12 evo, 13 taxi,
  14 icecreamvan, 15 pickup, 16 hotrod, 17 helicopter, 18 pug racer, 19 truck, 20 policevan (CONFIRMED from
  the loader near `0x114d00` in decompiled.c). Actor id 1 is never referenced again, so it is a free
  helicopter that stays for the whole level (INFERRED purpose).
* Pedestrian models used by this level (`~m`, table `0x3480e0`): 9 = `pedmale3`, 10 = `pedmale4`,
  11 = `pedmale5`, 15 = `pedmale7_shotgun` (CONFIRMED load order).
* Special object 3 (model 1003) = `objects/weapons/bullet.obj` with `+0x48ec = 10` (CONFIRMED). Used as a
  destructible door target in M700.
* There are no hidden phones, no type 0x3E respawn triggers and no secret or easter-egg events in this level.

### 1.1 Phone / progression graph: unlock routine `0xb5230` (CONFIRMED)

```
UnlockPhones10():
  slot[cur] = -1                      // cur = 0x11a354; runs after success AND failure
  if cur < 4 and slot[0..3] all == -1:
      slot 4: ev 400 phone 5
      slot 5: ev 500 phone 6
      slot 6: ev 600 phone 7
      slot 7: ev 700 phone 8
  RefreshPhones()                     // re-adds type-1 ring triggers; says a random global 2150-2158 if any ring
  cur = -1
```

* Stage 1: race (phone 1), M100 (phone 2), M200 (phone 3), M300 (phone 4), in any order.
* Stage 2 opens when all four have been attempted, whether passed or failed: M400 (phone 5), M500 (phone 6),
  M600 (phone 7), M700 (phone 8).
* Every mission can be attempted only once. A failed mission does not return to its phone. After the eight
  missions no phone rings. The level ends only through the 2,000,000 money trigger.
* Every phone answer first calls `KillAllPhones()`, so the other phones stop ringing during a mission.
* Base rewards (before the multiplier, which `AddMoney` applies): race ≤255,000; M100 ≤255,000; M200, M300,
  M400, M500 and M700 250,000 each; M600 225,000. The maximum base total is 1,985,000.

### 1.2 Mission end conventions (used by every mission)

* **Success**: `Dialog_Begin(2); Dialog_Say2(p); Dialog_End(); AddMoney(n); ShowMessage(2022 "Mission complete!"); UnlockPhones10()`.
* **Failure**: the same with `ShowMessage(2028 "Mission failed!")` and no money.
* Before either: `KillAllMissionTriggers()` and `Actor_Release(...)` for the mission's actors, in the order given below.
* Phone briefings use `Dialog_Begin(0)` (speaker 0). In-mission lines use speaker 2, except M300's
  ambush line (speaker 0).

### 1.3 Ambience rectangles (global, all states; `0xc1a92` top; CONFIRMED)

These triggers are untracked (`AddTrigger`/`AddTriggerEx`), so mission cleanup does not kill them.
R1 = (0x1000,0x34000)–(0x9800,0x3d800) is the hotel block used by M700. R2 = (0x1a800,0x36800)–(0x1e000,0x38800) is a second building.
`SetAmbience` = `0xb4c58`.

```
on 100000+f (f = 0,1,2; player entered R1 on floor f):
    SetAmbience(f == 0 ? 1 : f == 1 ? 2 : 3)
    KillTrigger(the other two of 100000..100002); KillTrigger(100003)
    AddTriggerEx(each of those two, 0x39, R1, its floor)
    AddTrigger(100003, 0x3a, R1)            // left R1
on 100003:
    SetAmbience(3)                           // outdoors
    KillTrigger(100000..100002); AddTriggerEx(100000..100002, 0x39, R1, floors 0..2)
on 100010+f / 100013: the same for R2 (events 100010..100013)
```

Floor 2, the roof, uses the outdoor value 3. The init adds only the three "enter" triggers.

---

## 2. Mission "1": street race (phone 1, slot 0)

68k: phone start `0xc3c30`; slot-0 events `0xc1fe4..0xc2298`; helper `Race10` `0xb73d0`. OS4: `0x010eef18`
case 0 and `0x010e544c`. Briefing phrase 2 is "...Go to the racetrack in the East of the city and get in the car
we've kindly provided."

```
on ev 1 (phone answered, 0x11a354 == -1):
    KillAllPhones()
    Dialog(0): phrase 2
    CreateActor(10, model 18 pug racer, (0x3ac00,0x2400) sq (117,4), floor 0, angle 0x10e0, colour 1)
    Actor_SetFlag49c(10, 0)                  // Actor_SetControls: the player's car is held until "Go!" (CONFIRMED)
    AddMissionTrigger(1, 0x08, 10)           // player gets into car 10 → ev 1 again (now in slot 0)
    AddMissionTrigger(10, 0x06)              // player dead/arrested → ev 10
    AddMissionTrigger(10, 0x09, 10)          // car 10 destroyed → ev 10
    0x11a354 = 0

slot 0:
  ev 1 (player in car 10):
    Dialog(2): phrase 3 "Ok, you've got three laps..."
    CreateActor(11, 18, (0x37400,0x1c00) sq(110,3), 0, 0x10e0, colour 2)
    CreateActor(12, 18, (0x37c00,0x2400) sq(111,4), 0, 0x10e0, colour 3)
    CreateActor(13, 18, (0x38c00,0x1c00) sq(113,3), 0, 0x10e0, colour 4)
    CreateActor(14, 18, (0x39400,0x2400) sq(114,4), 0, 0x10e0, colour 5)
    CreateActor(15, 18, (0x3a400,0x1c00) sq(116,3), 0, 0x10e0, colour 6)
    Actor_SetAggro(11,100); (12,100); (13,99); (14,98); (15,95)   // opponent driver skill (CONFIRMED)
    SetTimerMode(0)                          // HUD start lights icons/lights_0
    AddMissionTrigger(2, 0x11, 5)            // 5 s
  ev 2: SetTimerMode(1); AddMissionTrigger(3, 0x11, 1)
  ev 3: SetTimerMode(2); AddMissionTrigger(4, 0x11, 1)
  ev 4: Dialog(2): phrase 4 "Go, go, go!"; SetTimerMode(3); Actor_SetFlag49c(10, 1)
        Race10(-1)                           // arm AI waypoints and checkpoints
        AddMissionTrigger(5, 0x11, 1)
  ev 5: SetTimerMode(-1)                     // lights off
        SetTimerMode2(1, 3)                  // HUD "Lap 1 of 3"
  ev 10 (dead or car destroyed):
        KillAllMissionTriggers(); SetTimerMode(-1); SetTimerMode2(-1, 3)
        Actor_Release(10..15)
        Dialog(2): phrase 11 "What on earth are you playing at?"; ShowMessage(2028); UnlockPhones10()
  any other ev: Race10(ev)
```

**`Race10(ev)` `0xb73d0`** (CONFIRMED; tables are locals that the function rebuilds on every call):

```
AI_X  = [106,106,116,123,123,121,105,106,110,115,117,120,120,112]   // squares
AI_Y  = [  5, 22, 23, 24, 35, 39, 39, 31, 29, 28, 17, 15,  5,  4]
AI_SP = [2500,3000,1500,3000,4000,2500,3000,3500,2500,3000,3500,3500,2500,3000]
CP_TYPE = [0x36,0x34,0x36,0x33,0x35,0x34,0x35,0x33]
CP_VAL  = [0xa800,0x3b800,0x12800,0x37000,0x10000,0x39000,0x3800,0x36400]
//  cp0 y>0xa800 (sq21), cp1 x>0x3b800 (sq119), cp2 y>0x12800 (sq37), cp3 x<=0x37000 (sq110),
//  cp4 y<=0x10000 (sq32), cp5 x>0x39000 (sq114), cp6 y<=0x3800 (sq7), cp7 x<=0x36400 (sq108)

Race10(-1):
  for i in 0..4:  wp[i]=0
      AddMissionTrigger(10000+i, 0x32, 11+i, AI_X[0]<<11|0x400, AI_Y[0]<<11|0x400, AI_SP[0])
  for i in 0..5:  cp[i]=0; lap[i]=1; fin[i]=0          // i=0 is the player's car (id 10)
      AddMissionTrigger(20000+i, CP_TYPE[0], 10+i, CP_VAL[0], -1, -1)

Race10(ev):
  if ev == 10000+i (i 0..4):                     // opponent 11+i reached its waypoint
      wp[i] += 1; if wp[i] > 13: wp[i] = 0
      AddMissionTrigger(ev, 0x32, 11+i, AI_X[wp[i]]<<11|0x400, AI_Y[wp[i]]<<11|0x400, AI_SP[wp[i]])
  if ev == 20000+i (i 0..5):                     // car 10+i passed its next checkpoint
      cp[i] += 1
      if cp[i] > 7:                              // a lap is 8 checkpoints
          cp[i] = 0; lap[i] += 1
          if lap[i] < 4:
              if i == 0: SetTimerMode2(lap[0], 3); Dialog(2): phrase 10+lap[0]   // 12 "2 laps left", 13 "1 lap left"
          else:
              fin[i] = 1
              if i == 0:                         // the player finished
                  KillAllMissionTriggers()
                  n = 0; for j in 0..5: Actor_Release(10+j); if j != 0 and fin[j]: n += 1
                  SetTimerMode2(-1, 3)
                  Dialog(2): phrase 5+n          // 5 first, 6 second, 7 third, 8 fourth, 9 fifth, 10 sixth
                  if n < 3: AddMoney((3-n)*85000); ShowMessage(2022)  // 255000 / 170000 / 85000
                  else:     ShowMessage(2028)
                  UnlockPhones10()
      if fin[i] == 0 and 0x11a354 != -1:
          AddMissionTrigger(20000+i, CP_TYPE[cp[i]], 10+i, CP_VAL[cp[i]], -1, -1)
```

* `SetTimerMode` (`0xb47b8`) is **CONFIRMED** as the HUD race start lights: it draws icon `icons/lights_<n>`
  (n = 0..3; −1 hides it; renderer `0x3712e`). `SetTimerMode2(lap,total)` (`0xb47ca`) is **CONFIRMED** as the HUD
  text `"Lap %d of %d"` (`0x3715a`), hidden when lap ≤ 0.
* The player's checkpoint triggers are bound to car 10, not to the player. If the player leaves the car, it no
  longer advances, and leaving the car does not fail the race.
* The opponents are not removed at the end. They are released to ambient traffic.

## 3. Mission 100: follow the henchman (phone 2, slot 1)

68k: start `0xc3ce4`; events `0xc229c..0xc2ae8`. OS4 case 1 (`switchD_010eef80_caseD_1`).
Briefing phrase 14: "Kenny's just spotted a rival's henchmen... Follow him and wipe out anyone he meets, then
wipe HIM out."

Actors: `0x65` henchman (ped 11), `0x66` contact 1 (ped 10), `0x67` contact 2 (ped 10).
`S` = number of meetings that have happened (0..2). `P` = pay factor.

```
on ev 100:
    KillAllPhones(); Dialog(0): phrase 14
    CreateActor(0x65, ~11, (0x1e400,0x25400) sq(60,74), 0, angle 0xb40, 0)
    CreateActor(0x66, ~10, (0x18818,0x2b400) sq(49,86), 0, angle 0x870, 0)
    CreateActor(0x67, ~10, (0x20c00,0x2e7e8) sq(65,92), 0, angle 0xe10, 0)
    AddMissionTrigger(0x65, 0x09, 0x65)    // henchman dead
    AddMissionTrigger(0x66, 0x09, 0x66)
    AddMissionTrigger(0x67, 0x09, 0x67)
    AddMissionTrigger(0x6d, 0x11, 20)      // he starts walking after 20 s
    AddMissionTrigger(0x8c, 0x1b, 0x65)    // henchman sees the player (arrow on him)
    AddMissionTrigger(0x96, 0x06)          // player dead
    S = 0; P = 3; 0x11a354 = 1
```

Henchman route (each event adds the next `0x17` "walk to" trigger for actor 0x65):

| On ev | adds | target | square |
|---|---|---|---|
| 0x6d | 0x6e | (0x1e400,0x27c00) | (60,79) |
| 0x6e | 0x82 | (0x1bd50,0x27c00) | (55,79) |
| 0x82 | 0x6f | (0x18c00,0x27c00) | (49,79) |
| 0x6f | 0x70 | (0x188f8,0x2b368) | (49,86), at contact 1 |
| 0x70 | 0x71 | type 0x11, 2 s ("meeting") | |
| 0x71 | see below | | |
| 0x83 | 0x72 | (0x18c00,0x2e310) | (49,92) |
| 0x72 | 0x79 | (0x1c138,0x2e310) | (56,92) |
| 0x79 | 0x73 | (0x20aa8,0x2e630) | (65,92), at contact 2 |
| 0x73 | 0x74 | type 0x11, 2 s | |
| 0x74 | see below | | |
| 0xa1 | 0x75 | (0x29c00,0x2e400) | (83,92) |
| 0x75 | 0x76 | (0x29c00,0x2fc00) | (83,95) |
| 0x76 | 0x77 | (0x2ac00,0x2fc00) | (85,95) |
| 0x77 | 0x78 | **type 0x12** (0x2ac00,0x2f508), d=1 (removed on arrival) | (85,94) |

Contact 1 route (actor 0x66): 0xaa → (0x17000,0x29c00) as ev 0xab → (0x15c00,0x29c00) as ev 0xac →
type 0x12 (0x15c00,0x2bb38, d=1) as ev 0xad. Squares (46,86) → (46,83) → (43,83) → (43,87).
Contact 2 route (actor 0x67): 0xb4 → type 0x17 (0x21c00,0x32400) as ev 0xb5 → type 0x12 (0x21c00,0x323d0, d=1) as ev 0xb6.
Squares (65,100) → (67,100) → (67,100).

```
ev 0x71: S = 1
         AddMissionTrigger(0x83, 0x17, 0x65, 0x18c00, 0x2bc00)         // henchman leaves, sq (49,87)
         Dialog(2): phrase 21 "Ok, now get the guy he just met with!"
         AddMissionTrigger(0xaa, 0x17, 0x66, 0x17000, 0x2b400)         // contact 1 walks off, sq (46,86)
ev 0x74: S = 2
         AddMissionTrigger(0xa1, 0x17, 0x65, 0x253bc, 0x2e400)         // sq (74,92)
         Dialog(2): phrase 22 "Now kill the guy he met with, then wipe out the henchman before he escapes!"
         AddMissionTrigger(0xb4, 0x17, 0x67, 0x20c00, 0x32400)         // sq (65,100)
ev 0xad: Actor_Release(0x66); Dialog(2): phrase 26 "You let him get away!"      // only a message
ev 0xb6: Actor_Release(0x67); Dialog(2): phrase 26                               // only a message

ev 0x66 (contact 1 dead): Actor_Release(0x66)
    if S > 0: Dialog(2): phrase 18 "Good one - now keep an eye on the henchman..."
    else:     KillMissionTrigger(0x6d..0x79)        // stop the henchman route
              Actor_Attack(0x65, -1, -1)            // henchman flees from the player
              Actor_Release(0x67); P = 1
              Dialog(2): phrase 17 "Why did you kill HIM? ... The henchman's making a break for it - kill him!"
              S = 2; AddMissionTrigger(0x69, 0x19, 0x65)    // henchman beyond 33000 → escaped
ev 0x67 (contact 2 dead): Actor_Release(0x67)
    if S >= 2: Dialog(2): phrase 19 "Excellent. Now wipe out the henchman before he gets away!"
    else:     KillMissionTrigger(0x6d..0x79); Actor_Attack(0x65,-1,-1); Actor_Release(0x66)
              P = (S == 0) ? 1 : 2
              Dialog(2): phrase 17; S = 2; AddMissionTrigger(0x69, 0x19, 0x65)
ev 0x8c (henchman saw the player):
    KillMissionTrigger(0x6d..0x79); KillMissionTrigger(0xad); KillMissionTrigger(0xb6)
    Actor_Attack(0x65, 0x50, -1)                    // he attacks the player
    Actor_Release(0x66); Actor_Release(0x67)
    Dialog(2): phrase 24 "The henchman's seen you - kill him before he escapes!"
    if S <= 1: P = 1; S = 2
    AddMissionTrigger(0x69, 0x19, 0x65)
ev 0x65 (henchman dead): KillAllMissionTriggers(); Actor_Release(0x65), (0x66), (0x67)   // loop 100+i, i=1..3
    if S <= 1: Dialog(2): phrase 15 "You killed him too early..."; ShowMessage(2028)
    else:      Dialog(2): phrase 16 "Nicely done - here's your payment."; AddMoney(P*85000); ShowMessage(2022)
    UnlockPhones10()
ev 0x69 (escaped): KillAllMissionTriggers(); Actor_Release(0x65); phrase 20 "He got away!"; fail
ev 0x78 (reached the end of the route): KillAllMissionTriggers(); Actor_Release(0x65), (0x66), (0x67)
    phrase 23 "You idiot - you let him escape. I'm not paying you anything for that!"; fail
ev 0x96 (player dead): KillAllMissionTriggers(); Actor_Release(0x65..0x67); phrase 25 "You screwed that up completely, didn't you?"; fail
```

Notes (CONFIRMED from code, consequences INFERRED):
* Success needs only `S >= 2` when the henchman dies. Killing the contacts is optional. Pay is 255,000 (P=3)
  if the player waits for both meetings, 170,000 if contact 2 is killed before meeting 2, and 85,000 if the
  player is spotted early or kills a contact before meeting 1.
* `KillMissionTrigger(0x6d..0x79)` covers 0x6d–0x79 only. 0x82, 0x83, 0xa1, 0xaa–0xac and 0xb4/0xb5 are not killed.
  After `0x8c` the henchman can therefore re-add the next route trigger, and the contacts keep walking (0xad/0xb6 are killed but
  may be re-added from 0xac/0xb5). This is literal behaviour.
* The end-of-mission loop (`0xc2386`, `0xc2982`, `0xc2ab6`) releases ids `100+i`, i = 1..3, i.e. 0x65..0x67 (CONFIRMED).

## 4. Mission 200: Cartman's shipment (phone 3, slot 2)

68k: start `0xc3e34`; events `0xc2aec..0xc3224`. OS4 case 2. Briefing phrase 27: "...secret government shipment
...parked in the central car park... go nick it!"

```
on ev 200:
    KillAllPhones(); Dialog(0): phrase 27
    CreateActor(0xc9, model 19 truck, (0x18400,0x15400) sq(48,42), 0, angle 0x2d0, 0)
    AddMissionTrigger(0xc9, 0x08, 0xc9)    // player gets in the truck
    AddMissionTrigger(0xfa, 0x09, 0xc9)    // truck destroyed
    AddMissionTrigger(0xfa, 0x06)          // player dead
    gasSound(0x2fba4c) = 0; 0x11a354 = 2
ev 0xc9: SetWantedLevel(6)
         Dialog(2): phrase 29 "Ok, take it to Cartman's garage to the south. Watch out for the cops."
         AddMissionTrigger(0xca, 0x04, 0x4f, 0x43, 1, 1)  // garage, square (79,67), cube 1, door side 1 (+x/east)
ev 0xca (truck delivered; type 4 removes the car and puts the player on foot outside the east side):
         Dialog(2): phrase 30 "Stick around - Cartman's going to open it up..."
         AddMissionTrigger(0xcb, 0x11, 3)
ev 0xcb:
    CreateActor(0xc9, 19, (0x27ac4,0x21c00) sq(79,67), 0, 0x10e0, 0)     // truck re-created inside the garage
    CreateActor(0xca, ~11, (0x27eac,0x21822) sq(79,67), 0, 0x5a0, 0)    // mutant 1
    CreateActor(0xcb, ~11, (0x27eac,0x21ef8) sq(79,67), 0, 0x5a0, 0)    // mutant 2
    SetFaceE(0x4f, 0x43, 1, type 0, tex 0, flip 0)                      // open the garage's east face
    AddSound(1, 0x3a, 1, 0x3d0900, 0x206c, 0x27cb8, 0x21c00, 20000)     // looping gas hiss at the garage
    AddMissionTrigger(0xcc, 0x37, 5)                                    // gas particle tick (5 ticks)
    AddMissionTrigger(0xcd, 0x38, 0x27c00, 0x21c00, 0)                  // player on foot in the gas (±0x4b0)
    AddMissionTrigger(0xd5, 0x09, 0xca)
    AddMissionTrigger(0xdf, 0x09, 0xcb)
    AddMissionTrigger(0xce, 0x11, 2)
    Dialog(2): phrase 31 "What the hell is that?!?"
    S = 2; P = 0; gasSound = 1
ev 0xcc: AddEffect(0x27d2c+R(200), 0x21b9c+R(200), 20000-R(200), 0x1e+R(80), R(80)-0x28, -R(100), 400+R(500), 1)
         AddMissionTrigger(0xcc, 0x37, 5)          // R(n) = Random(n); argument order from OS4 0x010d13f4
ev 0xcd: if P == 0: P = 1; Dialog(2): phrase 35 "Watch out! That stuff's poisonous!"
         AddHealth(-1); AddMissionTrigger(0xe6, 0x37, 5)
ev 0xe6: AddMissionTrigger(0xcd, 0x38, 0x27c00, 0x21c00, 0)     // so -1 HP about every 5 ticks while in the gas
ev 0xce: Dialog(2): phrase 32 "They've gone mad - wipe 'em out!"
         AddMissionTrigger(0xd2, 0x17, 0xca, 0x29810, 0x20f58)   // sq (83,65)
         AddMissionTrigger(0xd3, 0x11, Random(2)+2)
         AddMissionTrigger(0xdc, 0x17, 0xcb, 0x29810, 0x226c8)   // sq (83,68)
         AddMissionTrigger(0xdd, 0x11, Random(2)+2)
ev 0xd3: KillMissionTrigger(0xd2); Actor_Attack(0xca, 0x3c, -1); AddMissionTrigger(0xd4, 0x11, Random(2)+2)
ev 0xd4: KillMissionTrigger(0xd2); Actor_Attack(0xca, -1, -1);   AddMissionTrigger(0xd3, 0x11, Random(2)+2)
ev 0xdd: KillMissionTrigger(0xdc); Actor_Attack(0xcb, 0x3c, -1); AddMissionTrigger(0xde, 0x11, Random(2)+2)
ev 0xde: KillMissionTrigger(0xdc); Actor_Attack(0xcb, -1, -1);   AddMissionTrigger(0xdd, 0x11, Random(2)+2)
         // each mutant alternates attack (2-3 s) and flee (2-3 s) indefinitely
ev 0xd5 / 0xdf (mutant dead): S -= 1
    if S == 0: SetFaceE(0x4f,0x43,1, 1, 0x76, 1)   // restore the garage face
               KillAllMissionTriggers(); if gasSound == 1: DeleteSound(1)
               Actor_Release(0xc9), (0xca), (0xcb)
               Dialog(2): phrase 34 "Nice one - here's your money."; AddMoney(250000); ShowMessage(2022); UnlockPhones10()
    else:      Dialog(2): phrase 33 "Great - now kill the other one."
ev 0xfa (truck destroyed or player dead): SetFaceE(0x4f,0x43,1,1,0x76,1); KillAllMissionTriggers()
    if gasSound == 1: DeleteSound(1); Actor_Release(0xc9..0xcb)
    Dialog(2): phrase 28 "Oh WELL DONE! You're a piece of crap, you know that?"; ShowMessage(2028); UnlockPhones10()
```

Events 0xd2 and 0xdc (mutant reached its walk target) have no case and are ignored.

## 5. Mission 300: rival gang meeting (phone 4, slot 3)

68k: start `0xc3ee6`; events `0xc3228..0xc34ae`. OS4 case 3. Briefing phrase 36: "A rival gang wants to meet us.
Go check it out, but be cautious."

```
on ev 300:
    KillAllPhones(); Dialog(0): phrase 36
    CreateActor(0x12d, ~9,  (0x3400,0x12c00) sq(6,37), 0, angle 0x5a0, 0)   // gang contact
    CreateActor(0x12e, ~11, (0x2400,0x11c00) sq(4,35), 0, 0x5a0, 0)        // gunman
    CreateActor(0x12f, ~11, (0x2400,0x13c00) sq(4,39), 0, 0x5a0, 0)        // gunman
    AddMissionTrigger(0x12d, 0x0d, 0x12d)    // player within ±6000 of the contact (arrow)
    AddMissionTrigger(0x137, 0x28, 0x12d)    // contact killed (arrow)
    AddMissionTrigger(0x138, 0x09, 0x12e)
    AddMissionTrigger(0x139, 0x09, 0x12f)
    AddMissionTrigger(0x15e, 0x06)           // player dead
    S = 3; 0x11a354 = 3
ev 0x12d: Dialog(0): phrase 38 "Look out! It's a trap! Wipe 'em out!"; AddMissionTrigger(0x12e, 0x11, 2)
ev 0x12e: Actor_Attack(0x12d, -1, -1)       // contact flees
          Actor_Attack(0x12e, 0x46, -1); Actor_Attack(0x12f, 0x46, -1)    // gunmen attack the player
ev 0x137 / 0x138 / 0x139: Actor_Release(0x12d / 0x12e / 0x12f); S -= 1
    if S == 0: KillAllMissionTriggers(); Dialog(2): phrase 39 "Great job - here's your dough!"
               AddMoney(250000); ShowMessage(2022); UnlockPhones10()
    else:      Dialog(2): phrase (S == 1 ? 41 "Niiice - now get the last one!" : 40 "Nice one! Now kill the rest of 'em!")
ev 0x15e: KillAllMissionTriggers(); Actor_Release(0x12d..0x12f)
          Dialog(2): phrase 37 "I told you to be cautious!"; ShowMessage(2028); UnlockPhones10()
```

All three must die. Killing them before the approach also counts.

## 6. Mission 400: help Kyle in a gunfight (phone 5, slot 4)

68k: start `0xc4008`; events `0xc34b2..0xc3768`. OS4 case 4. Briefing phrase 42: "Kyle's got caught up in a
gunfight with a rival's henchmen - help him out."

```
on ev 400:
    KillAllPhones(); Dialog(0): phrase 42
    CreateActor(0x191, ~15 pedmale7_shotgun, (0x1c908,0x13400) sq(57,38), 0, angle 5000, 0)  // Kyle
    CreateActor(0x192, ~11, (0x18400,0x15400) sq(48,42), 0, angle 0x708, 0)                  // henchman A
    CreateActor(0x193, ~11, (0x19c00,0x16c00) sq(51,45), 0, angle 0x7d0, 0)                  // henchman B
    AddMissionTrigger(0x191, 0x1a, 0x1c908, 0x13400, 0)   // player within ±10000 of Kyle's spot (arrow)
    AddMissionTrigger(0x19a, 0x09, 0x192)
    AddMissionTrigger(0x19b, 0x09, 0x193)
    AddMissionTrigger(0x1c2, 0x06)
    AddMissionTrigger(0x1c3, 0x09, 0x191)                 // Kyle dead
    AddMissionTrigger(0x1c4, 0x11, 30)                    // 30 s to get there
    A = 1; B = 1; 0x11a354 = 4
ev 0x191 (arrived): KillMissionTrigger(0x1c4)
    Dialog(2): phrase 43 "Our man is the one on the right - protect him!"
    if A == 1:      Actor_Attack(0x191, 0x5a, 0x192)      // Kyle shoots A
    else if B == 1: Actor_Attack(0x191, 0x5a, 0x193)
    Actor_Attack(0x192, 0x3c, 0x191); Actor_Attack(0x193, 0x3c, 0x191)   // both shoot Kyle
ev 0x19a (A dead): KillMissionTrigger(0x1c4); Actor_Release(0x192); A = 0
    if B == 0: success (below)
    else: Actor_Attack(0x193, 0x3c, -1); Dialog(2): phrase 44 "Great - now wipe out the other one."
ev 0x19b (B dead): KillMissionTrigger(0x1c4); Actor_Release(0x193); B = 0
    if A == 0: success
    else: Actor_Attack(0x192, 0x3c, -1); Dialog(2): phrase 44
success: Actor_Release(0x191); KillAllMissionTriggers()
         Dialog(2): phrase 45 "Nice job. Here's your cash."; AddMoney(250000); ShowMessage(2022); UnlockPhones10()
ev 0x1c2 (player dead): fail, phrase 46 "Awww - was it too tough for the little baby?"
ev 0x1c3 (Kyle dead):   fail, phrase 47 "You loser! You let him die!"
ev 0x1c4 (30 s):        fail, phrase 48 "You're too slow - he died before you got there."
fail: KillAllMissionTriggers(); Actor_Release(0x191), (0x192), (0x193); Dialog(2): phrase; ShowMessage(2028); UnlockPhones10()
```

* Nobody fights before the player arrives. The 30 s countdown is a hidden timer (type 0x11), not the HUD clock.
* After one henchman dies, the other turns on the player, while Kyle keeps his old target.

## 7. Mission 500: hijack the police van (phone 6, slot 5)

68k: start `0xc415a`; helper `Mission500` `0xb7844`. OS4 `0x010e5b7c`. Briefing phrase 49: "The pigs've done a raid on
Stan's house... hijack the van he's in and take it to Cartman's garage."

```
on ev 500:
    KillAllPhones(); Dialog(0): phrase 49
    CreateActor(0x1f5, model 9 police,     (0x1ac00,0x2dc00) sq(53,91), 0, angle 0x10e0, 0)
    CreateActor(0x1f6, model 20 policevan, (0x1bc00,0x2dc00) sq(55,91), 0, 0x10e0, 0)    // Stan's van
    CreateActor(0x1f7, model 9 police,     (0x1cc00,0x2dc00) sq(57,91), 0, 0x10e0, 0)
    Mission500(-1)
    0x11a354 = 5

ROUTE_X = [50,50,61,61,72,72,72,77,82,82,82,77,77]       // squares; index 0..12
ROUTE_Y = [91,80,80,72,72,59,47,47,47,40,35,35,37]

Mission500(-1):
    for i in 0..2: cnt[i] = 0
        AddMissionTrigger(0x1f5+i, 0x17, 0x1f5+i, ROUTE_X[0]<<11|0x400, ROUTE_Y[0]<<11|0x400)
    AddMissionTrigger(0x226, 0x06)                 // player dead
    AddMissionTrigger(0x226, 0x09, 0x1f6)          // van destroyed
    AddMissionTrigger(0x1fe, 0x08, 0x1f6)          // player gets into the van

Mission500(0x1fe):   // van hijacked
    KillMissionTrigger(0x1f5), (0x1f6), (0x1f7)    // the convoy stops following its route
    SetWantedLevel(6)
    Dialog(2): phrase 51 "Great, now take him to Cartman's garage."
    AddMissionTrigger(0x208, 0x04, 0x4f, 0x43, 1, 1)   // same garage as M200, sq (79,67), east door
Mission500(0x208):   // delivered
    Actor_Release(0x1f5), (0x1f6), (0x1f7); KillAllMissionTriggers()
    SetFaceN(0x4d, 0x25, 1, 1, 0x76, 1)            // (re)close the police-station door
    Dialog(2): phrase 52 "Nice one - here's your reward."; AddMoney(250000); ShowMessage(2022); UnlockPhones10()
Mission500(0x226):   // dead or van destroyed
    release x3; KillAllMissionTriggers(); SetFaceN(0x4d,0x25,1,1,0x76,1)
    Dialog(2): phrase 50 "You idiot!"; ShowMessage(2028); UnlockPhones10()
Mission500(ev = 0x1f5+i), i = 0 police, 1 van, 2 police:     // the vehicle reached its route point
    n = ++cnt[i]
    if n == 12:
        if i == 1: SetFaceN(0x4d, 0x25, 1, 0, 0, 0)          // open the station door (face off)
                   target = (ROUTE_X[12],ROUTE_Y[12]) = sq (77,37) → (0x26c00,0x12c00)
        if i == 0: target = (0x25c00,0x11c00) sq (75,35)      // police cars park beside the door
        if i == 2: target = (0x27400,0x11c00) sq (78,35)
    else if n == 13:
        if i == 1: // van is inside the station
            release x3; KillAllMissionTriggers(); SetFaceN(0x4d,0x25,1,1,0x76,1)
            Dialog(2): phrase 53 "You're too slow - they've got Stan locked up now!"; ShowMessage(2028); UnlockPhones10()
        no target
    else: target = (ROUTE_X[n], ROUTE_Y[n])
    if target: AddMissionTrigger(0x1f5+i, 0x17, 0x1f5+i, target.x, target.y)
```

Waypoint coordinates are `sq<<11 | 0x400` (centres). The police-station door is the north face of cube 1 at
square (77,37).

## 8. Mission 600: kill Ike's tail (phone 7, slot 6)

68k: start `0xc41f2`; helper `Mission600` `0xb7c40`. OS4 `0x010e6684`. Briefing phrase 54: "Ike thinks he's got a tail.
Hunt them down and kill 'em! <Note: Ike will be targetted, not the tail - make sure you don't kill him by mistake!>"
(the `<...>` part is a tutorial hint).

```
on ev 600:
    KillAllPhones(); Dialog(0): phrase 54
    CreateActor(0x259, ~11, (0x2400,0x10400) sq(4,32), 0, angle 0x5a0, 0)   // the tail
    CreateActor(0x25a, ~9,  (0x5c00,0x10400) sq(11,32), 0, angle 0, 0)      // Ike
    Mission600(-1)
    0x11a354 = 6

X = [11,11,16,20,21,21,24,24,34,34,40]     // squares, index 0..10
Y = [32,35,35,35,34,27,27,24,24,22,22]

Mission600(-1):
    cnt[0] = 0 (tail); cnt[1] = 1 (Ike)
    for i in 0..1: AddMissionTrigger(0x259+i, 0x17, 0x259+i, X[cnt[i]]<<11|0x400, Y[cnt[i]]<<11|0x400)
    AddMissionTrigger(0x26c, 0x09, 0x259)      // tail dead
    AddMissionTrigger(0x280, 0x31, 0x259)      // tail sees the player (no arrow)
    AddMissionTrigger(0x28a, 0x06)             // player dead
    AddMissionTrigger(0x28b, 0x28, 0x25a)      // Ike killed. Arrow on Ike ("Ike will be targetted").
Mission600(0x26c): KillAllMissionTriggers(); Actor_Release(0x259), (0x25a)
    Dialog(2): phrase 56 "Nice one!"; AddMoney(225000); ShowMessage(2022); UnlockPhones10()
Mission600(0x280): Actor_Attack(0x259, 0x5a, -1); Dialog(2): phrase 59 "The tail's seen you - drop him!"
Mission600(0x28a): fail, phrase 55 "You moron - how're you supposed to kill the tail now?"
Mission600(0x28b): fail, phrase 57 "You total loser! ... kill Ike by mistake ..."
Mission600(ev = 0x259+i):
    if i == 1 or (i == 0 and cnt[0] < cnt[1]):     // the tail may only advance while it is behind Ike
        n = ++cnt[i]
        if n == 11: if i == 1: AddMissionTrigger(0x25a, 0x12, 0x25a, 0x14400, 0xab20)   // sq (40,21); Ike vanishes on arrival
        else if n == 12: if i == 1: fail, phrase 58 "You idiot - you didn't catch the tail in time!"
        else: AddMissionTrigger(0x259+i, 0x17, 0x259+i, X[n]<<11|0x400, Y[n]<<11|0x400)
fail: KillAllMissionTriggers(); Actor_Release(0x259), (0x25a); Dialog(2): phrase; ShowMessage(2028); UnlockPhones10()
```

When the tail reaches a point while not behind Ike (`cnt[0] >= cnt[1]`), no new trigger is added and the
tail stays idle for the rest of the mission. This is literal code behaviour and may be unintended. The "sees the
player" switch to attack is permanent.

## 9. Mission 700: kidnapped brother in the hotel penthouse (phone 8, slot 7)

68k: start `0xc4266`; events `0xc378a..0xc3bcc`. OS4 case 7. Briefing phrase 60: "Kyle's younger brother has been
kidnapped... penthouse suite of the hotel in the South of the city..."

```
on ev 700:
    KillAllPhones(); Dialog(0): phrase 60
    CreateActor(0x2bd, ~9,  (0x8c00,0x35400) sq(17,106), floor 1, angle 0x10e0, 0)   // Kyle's brother
    CreateActor(0x2be, ~11, (0x8400,0x36800) sq(16,109), floor 1, angle 0, 0)        // kidnapper 1
    CreateActor(0x2bf, ~11, (0x8400,0x35400) sq(16,106), floor 1, angle 0x10e0, 0)   // kidnapper 2
    CreateActor(0x2c0, 1003 = special 3 (bullet.obj), (0x8c00,0x38000) sq(17,112), floor 1, angle 0, 0)  // door target
    AddMissionTrigger(0x2bd, 0x0f, 0x8800, 0x38c00, 0)   // player at the hotel entrance sq (17,113), floor 0 (arrow)
    AddMissionTrigger(0x2ee, 0x06)
    S = 2; 0x11a354 = 7
ev 0x2bd: Dialog(2): phrase 61 "Ok, now go upstairs."
          AddMissionTriggerEx(0x2be, 0x39, 0x6000,0x38000, 0x9000,0x38800, floor 1)   // sq x 12..18, y 112..113
          AddMissionTrigger(0x2c6, 0x28, 0x2c0)          // door object destroyed (arrow)
ev 0x2be (upstairs): Dialog(2): phrase 62 "Great, now blow the door at end of the corridor. You'll need to use explosives."
          AddMissionTrigger(0x2ef, 0x09, 0x2bd)          // brother dead
          AddMissionTrigger(0x2d1, 0x09, 0x2be)
          AddMissionTrigger(0x2d2, 0x09, 0x2bf)
ev 0x2c6 (door blown): SetFaceN(0x11, 0x70, 2, 0,0,0); SetFaceS(0x11, 0x6f, 2, 0,0,0)   // remove the wall between sq (17,112) and (17,111)
          Dialog(2): phrase 63 "Now wipe out the kidnappers."
          Actor_Attack(0x2be, 0x46, -1)
          AddMissionTrigger(0x2c7, 0x20, 0x36000)        // player y <= 0x36000 (square row 108)
ev 0x2c7: Dialog(2): phrase 65 "Look out! There's another one. Be careful you don't kill Kyle's brother."
          Actor_Attack(0x2bf, 0x46, -1)
ev 0x2d1 / 0x2d2 (kidnapper dead): S -= 1
    if S == 0: KillAllMissionTriggers(); Actor_Release(0x2bd..0x2c0)
               Dialog(2): phrase 68 "Nicely done - here's your money."; AddMoney(250000); ShowMessage(2022); UnlockPhones10()
    else:      Dialog(2): phrase 67 "Great - now get the other one."
ev 0x2ee (player dead): fail, phrase 64 "You plonker!"
ev 0x2ef (brother dead): fail, phrase 66 "Kyle's brother is dead - he'll never forgive you!"
fail: KillAllMissionTriggers(); Actor_Release(0x2bd..0x2c0); Dialog(2): phrase; ShowMessage(2028); UnlockPhones10()
```

* The kidnapper deaths and the brother's death are watched only after the player reaches floor 1 of the hotel
  (ev 0x2be). Killing a kidnapper earlier is not counted, so the mission could then never complete. This follows
  from the code (INFERRED consequence).
* The "door" is not a map door. It is the special object 3 placed at sq (17,112), floor 1. Destroying it removes
  two wall faces at cube level 2.

## 10. Global events

* **Event 0**: ignored (returns at the top of `0xc1a92`).
* **Player death/arrest**: no level-wide handling. Each mission adds its own type 0x06 trigger (race ev 10,
  M100 0x96, M200 0xfa, M300 0x15e, M400 0x1c2, M500 0x226, M600 0x28a, M700 0x2ee). In every case the result is
  "Mission failed" and the phone is not re-offered. No type 0x3E (respawn) triggers are used.
* **Money goal**: the type-0 trigger has event −1. When it fires, the handler is called with −1. With no
  mission active this does nothing. During the race (slot 0) −1 reaches `Race10(-1)`, in slot 5 `Mission500(-1)`,
  and in slot 6 `Mission600(-1)`, which re-arm those missions. This is harmless because the game state has
  already switched to "level complete" (CONFIRMED from the dispatch: `ev-1` is unsigned > 9, which takes the slot-0
  default path).
* No secret or hidden phones and no easter-egg events.

## 11. Differences from the legacy CD script (`analysis/ghidra_script10/`, main `0x1248a`; helpers `0x114c0` race, `0x11a48` M500, `0x11fb2` M600)

Same structure, events, phones (1–4, then 5–8 via the unlock at `0x113c2`), money goal, helicopter and ambience
rectangles. Differences found in the main function (helpers not diffed):
* **M100 route**: the CD has no intermediate points 0x82 (55,79) and 0x83 (49,87). In the CD, 0x6e → 0x6f directly and
  0x71 → 0x72 directly. The CD 0x72 is (0x18c00,0x2e400) and its 0x79 is (0x1cc00,0x2e400). Update 7 uses 0x2e310 and 0x1c138.
* **M100 pay factor**: in the CD it is an uninitialised register variable (`D5`), which M200 also reuses as the poison flag.
  Update 7 explicitly sets `P = 3` at the mission start, a bug fix (INFERRED).
* **M200 mutants** spawn at x = 0x278d0 in the CD and at 0x27eac in Update 7.
* The CD's ambience exit-trigger events decompile as `0xa3`/`0xad` (word-truncated). This is likely a decompiler
  display artefact of the 16-bit argument packing (INFERRED).

## 12. Open questions

1. ~~Type 0x12 4th argument~~ Resolved: ignored in Update 7 (Add sets only state 0x0e and the goal from b, c;
   68k `0xb2b12`, OS4 `0x010cfa80`). Probably left over from the CD scripts. CONFIRMED.
2. ~~Type 0x09 on removal~~ Resolved: no. `0xa653a` is called only on death or destruction; a type 0x12 arrival
   (`0xa6b5a` → state 6) and a type 0x04 drop-off (state 6 inside `0xa4a68`) never call it. M200 delivery is safe.
   CONFIRMED.
3. ~~`Actor_SetFlag49c(10, 0/1)`~~ Resolved: `Actor_SetControls`. `+0x49c` is read only in the player-driven
   branch of `0xe3a58`; 0 zeroes the player's steering and throttle, so it holds the player's car (id 10) on the
   grid until "Go!". No effect on AI drivers. CONFIRMED.
4. ~~`Actor_SetAggro` values~~ Resolved: `+0x346` is driver skill (vehicles.md §6.2, script_api.md §5). 95–100
   never brake for corners or stop at lights; 99/100 also skip the look-ahead. CONFIRMED.
5. ~~`AddSound` for M200~~ Resolved: (sample 0x3a Hiss, vol 0x3d0900 = 4,000,000, 0x206c = 8300 Hz, x, y,
   z 20000). Loudness `vol / d²`: full within ~1 square, silent beyond ~3.6. CONFIRMED.
6. The CD race, M500 and M600 helpers were not compared with Update 7.
