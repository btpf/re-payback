# Level 01 (city 0, level 1) — mission script reconstruction

Status markers: **CONFIRMED** = read directly from code (68k Update 7 and, unless noted,
identical in OS4); **INFERRED** = deduced from context, phrases or naming.
API names follow `docs/re/script_api.md`. Coordinates are world units; the square is given
as `(x>>11, y>>11)`. Angles: 0x1680 = 360°. Time: 30 ticks = 1 s.

| Item | 68k (Update 7) | OS4 |
|---|---|---|
| Level init | `0xb9324` | `0x010ead34` |
| Event handler | `0xbed0c` (11230 bytes) | `0x010ebddc` |
| Mission-0 helper ("garage explodes") | `0xb6c46` | `0x010dee4c` |
| Unlock routine (mission finished) | `0xb5198` | `0x010d9c40` |
| Common tail `ShowMessage(r); Unlock()` | `0xc0b74` | `LAB_010ec414/ec100/ec260` |
| Shutdown extras | `Mission_Shutdown` `0xca322` | — |
| Legacy CD script | `Maps/01.script`, `main` = `script01:0x11704` | |

Map: city 0 → `Maps/0.map` (INFERRED from the phone positions below, which match exactly).

OS4 function ↔ 68k API used by this level (CONFIRMED by comparing the two handlers):
`010d100c` AddTrigger, `010d26b8` AddMissionTrigger, `010d28ac` KillMissionTrigger,
`010d2954` KillAllMissionTriggers (often inlined as the loop over `0x013792c4`/count `0x0114c9a0`),
`010f46b8` KillTrigger, `010d25d0` KillAllPhones, `010d2480` RefreshPhones,
`thunk_01015d7c` Dialog_Begin, `010d1614` Dialog_Say1, `010d1638` Dialog_Say2,
`thunk_01015f68` Dialog_End, `010d153c` ShowMessage, `thunk_01041408` AddMoney,
`thunk_01042084` AddLives, `010cef8c` SetWantedLevel, `010ce5e8` CreateActor,
`010cde90` SetScenery, `010cddc8` SetCube, `010ce084` SetFaceTop, `010ce2ac` SetFaceS,
`010ce364` SetFaceW, `010cf830` Actor_Release, `010cefd0` Actor_Kill,
`caseD_10cf184` Actor_Attack, `010cedbc` = 68k `0xb4414`, `010ced44` = 68k `0xb43d4`,
`010cda74` AddSound, `010cdae4` DeleteSound, `010d152c` = 68k `0xb4360` SetFlag34c28c,
`0103ac94` Random, `thunk_01044f28` Explosion.

Level globals (68k / OS4):

| 68k | OS4 | Use |
|---|---|---|
| `0x11a354` | `0x0114c370` | **mission state = phone slot index of the running mission** (−1 = idle). Set by the handler itself when a phone event starts a mission; reset to −1 by the unlock routine. |
| `0x2fba1c` | `0x0114c96c` | per-mission flag / counter "A" |
| `0x2fba08` | `0x0114c97c` | counter "B" (cars/gangsters resolved; loop var in init and ev 0x321) |
| `0x2fb148` | `0x01379c74` | counter "C" (cars dumped / alert flag; loop var in ev 0x321) |
| `0x2fba30[4]` | `0x01379b5c[4]` | per-actor "still active" flags (missions 3 and 7) |
| `0x2fba54` | `0x0114c978` | loop index (mission 7) |
| `0x2fba2c` | `0x0114c970` | loop index (mission 2) |
| `0x2fba48` | `0x0114c958` | mission 8: prisoners still alive |
| `0x2fba4c` | `0x0114c964` | mission 8: phase (0 none, 1 boarding, 2 unloading) |

---

## 1. Level setup — `Init01` 68k `0xb9324` / OS4 `0x010ead34` (CONFIRMED)

Called from `Mission_Init` after it cleared all 20 phone slots (event −1, hidden 0),
set `0x11a354 = -1` and `0x2fb220 = 1` (first-refresh flag).

```
intro_phrase(0x2fb224) = 2     // "Your target for this level is 2,000,000 points. Answer the phones for missions."
phone slots (event / phone / hidden):
  slot 0: ev 1    phone 8   visible   -> mission 0 "Harry"
  slot 1: ev 100  phone 9   visible   -> mission 1 "4 cops"
  slot 2: ev 200  phone 18  visible   -> mission 2 "Quik-e-mart"
  slot 6: ev 600  phone 20  visible   -> mission 6 "ice-cream van"
  slot 8: ev 800  phone 16  HIDDEN (u16 0x2fb208 = 1 -> trigger type 7, no arrow) -> mission 8 "prison bus"
AddTrigger(-1, 0x00, 2000000)          // money goal 2,000,000 -> level complete (untracked, event -1 never handled)
AddLives(2)
CreateActor(100000, 1001, 0xb400, 0x2e000, 0, 0, 0)    // special object 1 = cone, square (22,92)
CreateActor(100001, 1001, 0xb400, 0x2f800, 0, 0, 0)    // cone (22,95)
CreateActor(100002, 1001, 0x15400,0x2e000, 0, 0, 0)    // cone (42,92)
CreateActor(100003, 1001, 0x15400,0x2f800, 0, 0, 0)    // cone (42,95)
for sx in 0x1a..0x1e: for sy in 0x3d..0x46:            // squares x 26-30, y 61-70
    SetCube(sx, sy, 1, 0x200, 0x200, 0, 0, 0)            // flatten/remove cube level 1 (INFERRED: opens a passage)
    // loop counter is the global 0x2fba08 (left = 0x1f afterwards; reset before use by missions)
RefreshPhones()                         // first call -> Dialog_Begin(2) Say2(2) End
ShowMessage(14)                         // level phrase 14 "Answer the phone" (big centre text)
CreateActor(0x835, 2, 0x3ac00, 0x2c400, 0, 0, 6)       // vehicle model 2 = diblo, colour 6, square (117,88): booby-trapped car
AddTrigger(0x834, 0x2d, 0x835)          // player gets into the car (no arrow), untracked
SetScenery(0x75, 0x57, 0, 24, 0, 0)     // Info pickup at square (117,87)
AddTrigger(0x836, 0x2f, 0x75, 0x57, 0)  // info collected (no arrow)
AddTrigger(0x898, 0x2f, 0x25, 0, 1)     // secret: cell (37,0) cube 1 — the map already has an Info (24, rot 242) there
```

Model tables used by this level (CONFIRMED from loader `0xd7874`): vehicles
`0x348120[]`: 2 diblo, 3 bus, 4 vapour, 10 scooby, 12 evo, 14 icecreamvan, 15 pickup.
Pedestrians `0x3480e0[~m]`: −9 → 8 `pedmale2`, −10 → 9 `pedmale3`, −12 → 11 `pedmale5_shotgun`.
Special objects `0x30c3c0[m−1000]`: 0 barrier, **1 cone**, 2 dummy.

Phone positions in `Maps/0.map` (scenery 14 with special n; GetPhoneCoords table
`0x279ef0` is filled while loading, a later square overwrites an earlier one — phone 6
exists twice, at (10,37) and (55,91); the later one, (55,91), wins — INFERRED from load order):

| Phone | Square | Used by |
|---|---|---|
| 8, 9, 18, 20 | (84,111), (85,111), (86,111), (87,111) | slots 0, 1, 2, 6 (first group) |
| 16 | (11,97) | slot 8 (hidden) |
| 6, 7, 19, 21 | (55,91), (56,91), (57,91), (58,91) | slots 3, 4, 5, 7 (second group) |

## 2. Progression — unlock routine 68k `0xb5198` / OS4 `0x010d9c40` (CONFIRMED)

Called at the end of every mission (success **and** failure, so a mission can never be
retried):

```
Unlock01():
    s = state(0x11a354)
    slot_event[s] = -1
    if (s in {0,1,2,6}) and slot_event[0]==slot_event[1]==slot_event[2]==slot_event[6]==-1:
        slot 3: ev 300 phone 6
        slot 4: ev 400 phone 7
        slot 5: ev 500 phone 19
        slot 7: ev 700 phone 21          (hidden flags untouched = visible)
    RefreshPhones()        // re-adds triggers for all non-empty slots; "Head back to the phones" (2150-2158) if any visible one
    state = -1
```

Graph: {M0, M1, M2, M6} all finished → {M3, M4, M5, M7}. M8 (hidden phone 16) is
available from the start and does not take part in unlocking. Nothing opens after the
second group; the level ends only via the money goal (type 0 trigger, 2,000,000).
Base rewards available: 100k + 200k + 150k + 200k + ≤150k + ≤225k + 200k + 300k + ≤250k
= 1,775,000 before the bonus multiplier (and cop kills 2,500 each), so the multiplier
(+1 per "Mission complete!", reset on death) is needed to finish (INFERRED).

Phone events are only handled while `state == -1` (they sit in the `default:` arm of the
state switch). Starting a mission: `KillAllPhones()` (all other phones stop ringing),
briefing with `Dialog_Begin(0)`, then `state = slot`.

## 3. Event dispatch (CONFIRMED)

```
Handler01(ev):
    // global events, checked first, any state
    if ev == 0x835: Actor_Kill(0x835); return           // booby-trapped car explodes
    if ev == 0:     return
    if ev == 0x834: Dialog_Begin(1); Say2(42); Dialog_End()   // "Security system activated ... self destruct in 10 seconds."
                    AddMissionTrigger(0x835, 0x05, 10); return  // visible 10 s countdown (tracked!)
    if ev == 0x836: Dialog_Begin(4); Say1(51); Dialog_End(); return   // hint: "Watch out for suspiciously parked cars..."
    if ev == 0x898: Dialog_Begin(4); Say1(400); Dialog_End(); return  // "I really didn't think anyone would figure out how to get this - well done!"
    switch state:
        case 0..8: mission handlers below
        default:   phone events 1,100,...,800 start missions
```

Common endings (used below):

* `SUCCESS(money)` = `AddMoney(money)` (× multiplier); `ShowMessage(2022)` "Mission complete!"
  (multiplier +1); `Unlock01()`.
* `FAIL` = `ShowMessage(2028)` "Mission failed!"; `Unlock01()`.
* `Say(n)` = `Dialog_Begin(2); Dialog_Say2(n); Dialog_End()` (speaker 2 = the boss). Hints
  use speaker 4 + `Dialog_Say1`; briefings speaker 0; the car's security system speaker 1.

Note: the countdown 0x835 is a **tracked** trigger. Any mission ending that calls
`KillAllMissionTriggers()` while the 10 s count is running cancels it, and the car never
explodes (CONFIRMED code, consequence INFERRED). Trigger 0x834 itself is untracked and
one-shot.

---

## 4. Missions

### Mission 0 — "Harry" (slot 0, phone 8, ev 1) — state 0

68k start ≈`0xc0f1c`, state block ≈`0xbee40–0xbf030`, helper `0xb6c46`; OS4 `0x010ebddc` ev 1 / `code_r0x010ec288`.

Phrases: 0 "Harry's in trouble - go help him out." · 3 "My God! The garage just blew up,
with Harry still inside - kill the sod who did it!" · 4 "You got him! Nice one - here's a
hundred grand." · 5 "You idiot - you let him escape!" · 1 "You're useless, you know that?"

```
on ev 1 (phone):
    KillAllPhones(); Dialog_Begin(0); Say2(0); Dialog_End()
    AddMissionTrigger(2,    0x1a, 7000, 0x5c00, 0)   // within ±10000 of (3,11) = Harry's garage, arrow
    AddMissionTrigger(3,    0x11, 100)                // hidden 100 s timer
    AddMissionTrigger(0x32, 0x06)                     // player dead/arrested
    state = 0; A = 0

state 0:
  ev 2: if A==0: KillMissionTrigger(3); A=1; GarageExplodes()
  ev 3: if A==0: KillMissionTrigger(2); A=1; GarageExplodes()   // happens anyway after 100 s
  ev 4:  AddMissionTrigger(5,  0x17, 1, 0x2400, 0xf400)    // (4,30)
  ev 5:  AddMissionTrigger(6,  0x17, 1, 0x3c00, 0xf400)    // (7,30)
  ev 6:  AddMissionTrigger(7,  0x17, 1, 0x3c00, 0x103c4)   // (7,32)
  ev 7:  AddMissionTrigger(8,  0x17, 1, 0x9400, 0x103c4)   // (18,32)
  ev 8:  AddMissionTrigger(9,  0x17, 1, 0x9400, 0x11c00)   // (18,35)
  ev 9:  AddMissionTrigger(10, 0x17, 1, 0xa410, 0x11c00)   // (20,35)
  ev 10: AddMissionTrigger(11, 0x12, 1, 0xa410, 0x12400)   // (20,36): go there and vanish (actor removed)
  ev 11 (killer escaped):  KillAllMissionTriggers(); Actor_Release(1); Say(5); FAIL
  ev 0x1e (killer killed): KillAllMissionTriggers(); Actor_Release(1); Say(4); SUCCESS(100000)
  ev 0x32 (player died):   KillAllMissionTriggers(); Actor_Release(1); Say(1); FAIL

GarageExplodes()  (68k 0xb6c46, OS4 0x010dee4c):
    for gx in {0x1000, 0x1800}:                 // squares x 2..3
      for gy in {0x5000, 0x5800, 0x6000}:       // squares y 10..12
        if Random(2) == 0:
            Explosion(x = gx + Random(0x800), y = gy + Random(0x800),
                      z = 17000 + Random(1500), owner 0, size 3000)
            // 68k evaluates the Randoms in the order z, y, x; OS4 x, y, z
    for sx in 2..3: for sy in 10..12: for cube in 1..2: SetCube(sx,sy,cube,0x200,0x200,0,0,0)   // building removed
    SetCube(1, 0x0b, 1, 0x200, 0x200, 0, 0, 0)
    SetFaceTop(2, 0x0b, 0, 1, 0x14c, 1); SetFaceTop(3, 0x0b, 0, 1, 0x14e, 1)
    SetFaceTop(3, 0x0d, 0, 1, 0x0d4, 1); SetFaceTop(2, 0x0d, 0, 1, 0x0d0, 1)
    for sy in 10..13: SetFaceTop(4, sy, 0, 1, 0xda, 1)            // scorched ground textures (INFERRED)
    Say(3)
    CreateActor(1, -9 /*pedmale2*/, 0x2400, 0x6c00, 0, 0, 0)       // the killer at (4,13)
    AddMissionTrigger(4,    0x17, 1, 0x2400, 0xbc00)               // run to (4,23)
    AddMissionTrigger(0x1e, 0x28, 1)                                // killer destroyed (arrow on him)
```

No escape-distance trigger: the killer is lost only when he reaches the end of his route.

### Mission 1 — "Kill 4 cops in 3 minutes" (slot 1, phone 9, ev 100) — state 1

68k start ≈`0xc0fa0`, state block ≈`0xbf040–0xbf0e0`; OS4 ev 100 / `caseD_1`.

Phrases: 6 briefing · 7 "One down, three to go." · 8 "That's two feds out the picture." ·
9 "You've got a real violent streak..." · 10 "Good job! Here's your share of the dough." ·
12 "Time's up, you pathetic looser - you just cost me 500 grand!" · 13 "Looks like my
associate was right to doubt you..."

```
on ev 100:
    KillAllPhones(); Dialog_Begin(0); Say2(6); Dialog_End()
    SetWantedLevel(5)
    4 × AddMissionTrigger(0x65, 0x29)      // four independent "a cop was killed" triggers, same event
    AddMissionTrigger(0x8c, 0x05, 180)     // visible 3:00 countdown
    AddMissionTrigger(0x96, 0x06)          // dead/arrested
    state = 1; A = 0

state 1:
  ev 0x65: Say(7 + A)
           A += 1
           if A == 4: KillAllMissionTriggers(); SUCCESS(200000)
  ev 0x8c: KillAllMissionTriggers(); Say(12); FAIL
  ev 0x96: KillAllMissionTriggers(); Say(13); FAIL
```

(AddMissionTrigger first untracks the event, so only one 0x65 entry is in the tracked list;
`KillAllMissionTriggers` still removes all four because KillTrigger kills every trigger
with that event number. Each cop kill fires exactly one type-0x29 trigger — CONFIRMED in
script_api §3.2.) The usual cop-kill bonus (2,500 × multiplier) is paid by the engine on top.

### Mission 2 — "Ram-raid the Quik-e-mart" (slot 2, phone 18, ev 200) — state 2

68k start ≈`0xc108e`, state block ≈`0xbf130–0xbf600`; OS4 ev 200 / `caseD_2`.

Phrases: 15 "There's a Quik-e-mart south of the army base. Ram-raid it." · 16 "That's it -
now go get the stuff." · 17 "Great - now take it to Jon's garage." · 18 "You just can't
take the heat, can you?" · 19 "Nicely done. Here's your cut - a cool 150 grand." ·
20 "You just ain't fast enough."

```
on ev 200:
    KillAllPhones(); Dialog_Begin(0); Say2(15); Dialog_End()
    // shop windows: west faces of x=0x5a and south faces of y=0x5c become glass (face type 3 = alpha)
    for sy in {0x53,0x54,0x55,0x57,0x58,0x5a,0x5b}: SetFaceW(0x5a, sy, 1, 3, 0x1da, 0)   // square x 90, y 83..91
    for sx in {0x5b,0x5c,0x5d,0x5e}:                SetFaceS(sx, 0x5c, 1, 3, 0x1db, 0)   // y 92, x 91..94
    AddMissionTrigger(0xc9, 0x0f, 0x2e400, 0x2b400, 0)     // reach the loot: within ±0x1000 of (92,86), arrow
    AddMissionTrigger(0xd2..0xdc, 0x2a, <cell>, 1) for the 11 window cells above, in the same order
         (0xd2 (90,83) 0xd3 (90,84) 0xd4 (90,85) 0xd5 (90,87) 0xd6 (90,88) 0xd7 (90,90)
          0xd8 (90,91) 0xd9 (91,92) 0xda (92,92) 0xdb (93,92) 0xdc (94,92))
    AddMissionTrigger(0xfa, 0x06)
    state = 2; A = 0      // A = "alarm ringing"

state 2:
  ev 0xd2..0xdc (a window broken, INFERRED meaning of type 0x2a):
      if A != 0: return
      A = 1
      for e in 0xd2..0xdc: KillMissionTrigger(e)
      Say(16)
      SetWantedLevel(5)
      AddSound(ref 0, sample 0x30, loop 1, 100000000, 16000, x 0x2e400, y 0x2b400, range 20000)   // alarm bell
      AlarmBoxes(22)       // flashing alarm boxes
  ev 0xc9 (got the stuff):
      Say(17)
      AddMissionTrigger(0xca, 0x04, 0x0c, 0x74, 1, 3)   // Jon's garage: square (12,116), floor 1, door side 3 (-x)
      AddMissionTrigger(0xcb, 0x05, 60)                  // visible 60 s countdown
  ev 0xca (delivered):  KillAllMissionTriggers(); StopAlarm(); Say(19); SUCCESS(150000)
  ev 0xcb (time up):    KillAllMissionTriggers(); StopAlarm(); Say(20); FAIL
  ev 0xfa (died):       KillAllMissionTriggers(); StopAlarm(); Say(18); FAIL

AlarmBoxes(s): SetCube(0x59,0x52,2,0x40,0x150,s,0xbf,1); SetCube(0x59,0x56,2,0x40,0x150,s,0xbf,1)
               SetCube(0x59,0x5c,2,0x40,0x150,s,0xbf,1); SetCube(0x5a,0x5d,2,0x40,0x150,s,0,1)
               SetCube(0x5f,0x5d,2,0x40,0x150,s,0,1)
               // squares (89,82) (89,86) (89,92) (90,93) (95,93), cube 2; scenery 22 AlarmBox_flash / 21 AlarmBox
StopAlarm():  if A == 1: DeleteSound(0); AlarmBoxes(21)
```

0xc9 is armed from the start, so the loot point does not require breaking a window
first (CONFIRMED code; whether it is physically reachable without smashing glass is a map
question). The windows stay glass after the mission (no restore).

### Mission 3 — "Dump the cars in the sea" (slot 3, phone 6, ev 300) — state 3

68k start ≈`0xc1364`, state block ≈`0xbfdd0–0xc0210`; OS4 ev 300 / `LAB_010ec188`.

Phrases: 21 briefing · 22 "Ok, now dump it in the sea." · 23 "Great! Now go get the next
one." · 24 "Ok, here's your money." · 25 "You idiot - you were supposed to dump it in the
sea!..." · 26 "This blows rabid goats! You failed to dump a single car in the sea!" ·
28 "You're just not fast enough!" (27 "You useless moron!" is **unused**.)

```
on ev 300:
    KillAllPhones(); Dialog_Begin(0); Say2(21); Dialog_End()
    CreateActor(0x12d, 12 /*evo*/,    0x3dc00, 0x3400, 0, 0x0a0,  1)   // (123,6), 10°,  colour 1
    CreateActor(0x12e, 10 /*scooby*/, 0x3e418, 0x3400, 0, 0,      4)   // (124,6), 0°,   colour 4
    CreateActor(0x12f, 4  /*vapour*/, 0x3ebe8, 0x3400, 0, 0x15e0, 0)   // (125,6), 350°, colour 0
    active[0..2] = 1
    for i in 0..2:
        AddMissionTrigger(0x137+i, 0x08, 0x12d+i)   // player gets in car i (arrow)
        AddMissionTrigger(0x141+i, 0x2b, 0x12d+i)   // car i fell into the water (see §7)
        AddMissionTrigger(0x14b+i, 0x09, 0x12d+i)   // car i destroyed
    AddMissionTrigger(0x154, 0x05, 200)            // visible 200 s countdown
    state = 3; C(dumped) = 0; B(resolved) = 0; A = 0
    // NOTE: no type-6 (death) trigger in this mission.

state 3:
  ev 0x137..0x139 (got into a car):
      if A != 0: return
      KillMissionTrigger(0x137); KillMissionTrigger(0x138); KillMissionTrigger(0x139); KillMissionTrigger(0x168)
      A = 1; Say(22)
      AddMissionTrigger(0x168, 0x0e)               // left / changed vehicle
  ev 0x168 (left the car):
      ReArmGetIn(); A = 0
  ev 0x141+i (car i in the sea):
      KillMissionTrigger(0x168)
      if active[i] != 1: return
      B += 1; C += 1; active[i] = 0
      if B < 3: ReArmGetIn(); Say(23); return       // A is NOT reset here (see note)
      Finish3()
  ev 0x14b+i (car i destroyed):
      KillMissionTrigger(0x168)
      if active[i] != 1: return
      B += 1; active[i] = 0
      if B < 3: ReArmGetIn(); Say(25); return
      if C > 0: Finish3()
      else: KillAllMissionTriggers(); Actor_Release(0x12d,0x12e,0x12f); Say(26); FAIL
  ev 0x154 (time up):
      KillAllMissionTriggers(); Actor_Release(0x12d,0x12e,0x12f); Say(28)
      if C > 0: AddMoney(C * 75000)
      FAIL

ReArmGetIn(): for i in 0..2: if active[i]==1: AddMissionTrigger(0x137+i, 0x08, 0x12d+i)
Finish3():    KillAllMissionTriggers(); Actor_Release(0x12d,0x12e,0x12f); Say(24); SUCCESS(C * 75000)
```

Quirk (CONFIRMED, both 68k and OS4, also in the CD script): after a car is dumped, `A`
stays 1, so getting into the next car is ignored (no phrase 22, no 0x168). This is
harmless for the outcome. A car that is first dumped and later destroyed is counted once
(`active[]` guard).

### Mission 4 — "The briefcase" (slot 4, phone 7, ev 400) — state 4

68k start ≈`0xc1538`, state block ≈`0xc02a0–0xc0680`; OS4 ev 400 / `code_r0x010ec164`.

Phrases: 31 "Jimmy's just dropped a briefcase behind the church. Go pick it up." ·
32 "What's in it?" (Say1 = player) · 33 "Trust me, you don't wanna know." · 34 "You idiot!" ·
35 "Ok, now take it to the drop off point in the park." · 36 "Now wait for the pickup, but
don't let him see you. He'll be coming from the South-East." · 37 "You moron! You let him
see you!" · 38 "You killed him, you idiot!" · 39 "Ok, he's made the pickup. Don't let him
out of your sight." · 40 "You let him escape!" · 41 "Good job! Here's the dough."

```
on ev 400:
    KillAllPhones(); Dialog_Begin(0); Say2(31); Say1(32); Say2(33); Dialog_End()
    SetScenery(0x28, 0x3f, 0, 23 /*Briefcase*/, 0x59, 0)       // square (40,63)
    AddMissionTrigger(0x191, 0x2c, 0x14400, 0x1fc00, 0)         // on foot at (40,63), arrow
    AddMissionTrigger(0x1c2, 0x06)
    state = 4

state 4:
  ev 0x191 (picked up):
      SetScenery(0x28, 0x3f, 0, 0, 0x59, 0)
      Say(35)
      AddMissionTrigger(0x192, 0x2c, 0x29df4, 0xa400, 0)        // on foot at drop-off (83,20)
      SetFlag34c28c(1)                                           // see open questions
  ev 0x192 (dropped off):
      SetScenery(0x53, 0x14, 0, 23, 0xb2, 0)                     // briefcase now lies at (83,20)
      SetFlag34c28c(0)
      Say(36)
      CreateActor(0x191, -10 /*pedmale3*/, 0x2ec00, 0xdc00, 0, 0, 0)   // courier at (93,27)
      AddMissionTrigger(0x1c3, 0x1b, 0x191)                      // courier sees the player
      AddMissionTrigger(0x1c4, 0x09, 0x191)                      // courier destroyed
      AddMissionTrigger(0x193, 0x17, 0x191, 0x2ec00, 0xcc00)     // walk to (93,25)
  ev 0x193: AddMissionTrigger(0x194, 0x17, 0x191, 0x2ac00, 0xcc00)    // (85,25)
  ev 0x194: AddMissionTrigger(0x195, 0x17, 0x191, 0x29c00, 0xa400)    // (83,20) = the briefcase
  ev 0x195 (pickup made):
      SetScenery(0x53, 0x14, 0, 0, 0xb2, 0)
      Say(39)
      AddMissionTrigger(0x196, 0x17, 0x191, 0x28c00, 0x8c00)     // (81,17)
      AddMissionTrigger(0x1c5, 0x19, 0x191)                      // courier > ±33000 from player = lost
  ev 0x196: AddMissionTrigger(0x197, 0x17, 0x191, 0x28c00, 0x6400)    // (81,12)
  ev 0x197: AddMissionTrigger(0x198, 0x17, 0x191, 0x2dc00, 0x5c00)    // (91,11)
  ev 0x198: AddMissionTrigger(0x199, 0x12, 0x191, 0x2dc00, 0x445c)    // (91,8), courier removed on arrival
  ev 0x199 (delivered): Actor_Release(0x191); KillAllMissionTriggers(); Say(41); SUCCESS(200000)
  ev 0x1c2 (died):
      SetFlag34c28c(0); KillAllMissionTriggers()
      SetScenery(0x28,0x3f,0,0,0x59,0); SetScenery(0x53,0x14,0,0,0x59,0)
      Actor_Release(0x191); Say(34); FAIL
  ev 0x1c3 (seen):    KillAllMissionTriggers(); SetScenery(0x53,0x14,0,0,0x59,0); Actor_Release(0x191); Say(37); FAIL
  ev 0x1c4 (killed):  same with Say(38); FAIL
  ev 0x1c5 (lost):    same with Say(40); FAIL
```

Note: the 0x1c3 "sees the player" trigger stays armed during the follow phase, so being
seen while tailing him also fails (CONFIRMED: it is only removed by KillAllMissionTriggers).

### Mission 5 — "Wipe out the pickup driver" (slot 5, phone 19, ev 500) — state 5

68k start ≈`0xc15d0`, state block ≈`0xc06a0–0xc09b0`; OS4 ev 500 / `code_r0x010ec130`.

Phrases: 43 briefing ("...a competitor driving a pickup truck near the bank. Wipe him
out.") · 44 "You really screwed that up, didn't you?" · 45 "Good job! ... Here's your money."

```
on ev 500:
    KillAllPhones(); Dialog_Begin(0); Say2(43); Dialog_End()
    CreateActor(0x1f5, 15 /*pickup*/, 0x2bc00, 0x2f400, 0, 0xb40, 4)   // (87,94), 180°, colour 4
    AddMissionTrigger(0x1f5, 0x17, 0x1f5, 0x2bc00, 0x23280)  // drive to (87,70)
    AddMissionTrigger(0x21c, 0x2e, 0x1f5)                    // pickup attacked or destroyed (arrow)
    AddMissionTrigger(0x226, 0x06)
    state = 5

state 5 — endless patrol (each arrival arms the next leg; all type 0x17 on actor 0x1f5):
  0x1f5 -> 0x1f6 (0x2bc00,0x17400) (87,46)    0x1f6 -> 0x1f7 (0x21400,0x17400) (66,46)
  0x1f7 -> 0x1f8 (0x16c00,0x17400) (45,46)    0x1f8 -> 0x1f9 (0x16c00,0x1ac00) (45,53)
  0x1f9 -> 0x1fa (0x1ac00,0x1ac00) (53,53)    0x1fa -> 0x1fb (0x1ac00,0x23c00) (53,71)
  0x1fb -> 0x1fc (0x15400,0x23c00) (42,71)    0x1fc -> 0x1fd (0x15400,0x28400) (42,80)
  0x1fd -> 0x1fe (0x1dc00,0x28400) (59,80)    0x1fe -> 0x1ff (0x26c00,0x28400) (77,80)
  0x1ff -> 0x200 (0x26c00,0x2f400) (77,94)    0x200 -> 0x1f5 (0x2bc00,0x2f400) (87,94)  // loop
  ev 0x21c (attacked):
      KillAllMissionTriggers()
      Ped_ExitVehicle(0x1f6, 0x1f5, -12)    // 68k 0xb4414: driver id 0x1f6, pedmale5_shotgun, gets out of 0x1f5
      Actor_Release(0x1f5)                   // empty pickup becomes ambient
      Actor_Attack(0x1f6, 0x50, -1)          // driver attacks the player
      AddMissionTrigger(0x21d, 0x09, 0x1f6)
      AddMissionTrigger(0x226, 0x06)
  ev 0x21d (driver dead): KillAllMissionTriggers(); Actor_Release(0x1f6); Say(45); SUCCESS(300000)
  ev 0x226 (died):        KillAllMissionTriggers(); Actor_Release(0x1f5); Actor_Release(0x1f6); Say(44); FAIL
```

There is no time limit; the pickup patrols until attacked.

### Mission 6 — "Booby-trapped ice-cream van" (slot 6, phone 20, ev 600) — state 6

68k start ≈`0xc167c`, state block ≈`0xc09f0–0xc0b60`; OS4 ev 600 / `caseD_6`.

Phrases: 46 briefing ("...drug shipment in an ice cream van in the north of city. Steal
it.") · 47 "It's a set-up! The van's rigged to explode in 55 seconds or if you try to get
out. Take it to Ray's garage..." · 48 "Too slow." · 49 "Great! Here's your danger money." ·
50 "I told you the doors were rigged!" · 29 "You blew up the damned van!" · 30 "Can't you do
ANYTHING right?" · hint 59 "Keep an eye out for shortcuts - they're not necessarily shown
on the map."

```
on ev 600:
    KillAllPhones(); Dialog_Begin(0); Say2(46); Dialog_End()
    CreateActor(0x259, 14 /*icecreamvan*/, 0x1f400, 0x9400, 0, 0xb40, 7)   // (62,18), 180°, colour 7
    AddMissionTrigger(0x259, 0x08, 0x259)          // get in (arrow)
    AddMissionTrigger(0x28a, 0x09, 0x259)          // van destroyed
    AddMissionTrigger(0x28b, 0x06)
    SetScenery(0x3f, 0x12, 0, 24 /*Info*/, 0, 0)   // (63,18)
    AddMissionTrigger(0x2a8, 0x2f, 0x3f, 0x12, 0)  // info collected
    state = 6

state 6:
  ev 0x2a8: Dialog_Begin(4); Say1(59); Dialog_End()
  ev 0x259 (in the van):
      Say(47)
      AddMissionTrigger(0x25a, 0x04, 0x6d, 0x25, 1, 2)   // Ray's garage (109,37), floor 1, door side 2 (+y)
      AddMissionTrigger(0x280, 0x05, 55)                  // visible 55 s
      AddMissionTrigger(0x281, 0x0e)                      // left the van
  ev 0x25a (garage): KillAllMissionTriggers(); Actor_Release(0x259); Say(49); ClearInfo(); SUCCESS(200000)
  ev 0x280 (time):   KillAllMissionTriggers(); Actor_Kill(0x259);    Say(48); ClearInfo(); FAIL
  ev 0x281 (got out):KillAllMissionTriggers(); Actor_Kill(0x259);    Say(50); ClearInfo(); FAIL
  ev 0x28a (wrecked):KillAllMissionTriggers(); Actor_Release(0x259); Say(29); ClearInfo(); FAIL
  ev 0x28b (died):   KillAllMissionTriggers(); Actor_Release(0x259); Say(30); ClearInfo(); FAIL
ClearInfo(): SetScenery(0x3f, 0x12, 0, 0, 0, 0)
```

In the success path the 68k order is Say, ClearInfo, AddMoney, ShowMessage; OS4 identical.
Actor_Kill sets health −71 → the van explodes (with the player in it on 0x280).

### Mission 7 — "Wasteland gang deal" (slot 7, phone 21, ev 700) — state 7

68k start ≈`0xc1804`, state block ≈`0xc0b90–0xc0ec0`; OS4 ev 700 / `LAB_010ebfa4`.

Phrases: 52 briefing ("...a deal in the wasteland behind the buildings to the South of the
military base. Wipe 'em out.") · 53 "Nice one - now get the rest of them." · 54 "Great. The
last one's making a run for it - eliminate him." · 55 "A job well done." · 56 "You let him
escape!" · 57 "You're fecking useless!" · hint 58 (backup / run them down in a tough vehicle).

```
on ev 700:
    KillAllPhones(); Dialog_Begin(0); Say2(52); Dialog_End()
    SetScenery(0x5f, 0x5d, 0, 24 /*Info*/, 0, 0)        // (95,93)
    AddMissionTrigger(0x30c, 0x2f, 0x5f, 0x5d, 0)
    CreateActor(0x2bd, -12 /*pedmale5_shotgun*/, 0x2fc00, 0x28c00, 0, 0x5a0,  0)   // (95,81),  90°
    CreateActor(0x2be, -12, 0x33c00, 0x2ac00, 0, 0x5a0,  0)                       // (103,85), 90°
    CreateActor(0x2bf, -12, 0x34400, 0x2ac00, 0, 0x10e0, 0)                       // (104,85), 270°
    CreateActor(0x2c0, -12, 0x38c00, 0x28c00, 0, 0x10e0, 0)                       // (113,81), 270°
    for i in 0..3: AddMissionTrigger(0x2bd+i, 0x31, 0x2bd+i)   // gangster i sees the player
    for i in 0..3: AddMissionTrigger(0x2c7+i, 0x30, 0x2bd+i)   // gangster i attacked/destroyed
    AddMissionTrigger(0x2d1, 0x09, 0x2bd)                       // killed (no arrow)
    AddMissionTrigger(0x2d2, 0x28, 0x2be)                       // killed (arrow)
    AddMissionTrigger(0x2d3, 0x28, 0x2bf)                       // killed (arrow)
    AddMissionTrigger(0x2d4, 0x09, 0x2c0)                       // killed (no arrow)
    AddMissionTrigger(0x2ee, 0x06)
    state = 7; C(alerted) = 0; B(killed) = 0; active[0..3] = 1

state 7 (ev in 0x2bd..0x2d4 runs both parts in order):
  part 1 — ev 0x2d1..0x2d4 (gangster i = ev-0x2d1 killed):
      if active[i] == 1:
          active[i] = 0; B += 1
          Actor_Release(0x2bd + i)
          Dialog_Begin(2)
          if B < 3:     AddMoney(50000);  Say2(53); Dialog_End()
          elif B == 3:  AddMoney(50000);  Say2(54); Dialog_End()
          else:         KillAllMissionTriggers(); SetScenery(0x5f,0x5d,0,0,0,0)
                        AddMoney(100000); Say2(55); Dialog_End(); ShowMessage(2022); Unlock01()
          if B == 3:    for each j with active[j]==1:
                            AddMissionTrigger(0x2da, 0x19, 0x2bd + j)   // last one escapes beyond ±33000
                            Actor_Attack(0x2bd + j, -1, -1)             // flee from player
  part 2 — ev 0x2bd..0x2d4 (any sighting / attack / kill):
      if C == 0:
          C = 1
          for each j with active[j]==1: Actor_Attack(0x2bd + j, 0x46, -1)   // whole gang attacks
      return
  ev 0x2da (last one escaped):
      KillAllMissionTriggers(); Actor_Release(all active); SetScenery(0x5f,0x5d,0,0,0,0)
      Say(56); ShowMessage(2022) /* sic */; Unlock01()
  ev 0x2ee (player died):
      KillAllMissionTriggers(); Actor_Release(all active); SetScenery(0x5f,0x5d,0,0,0,0)
      Say(57); ShowMessage(2022) /* sic */; Unlock01()
  ev 0x30c: Dialog_Begin(4); Say1(58); Dialog_End()
```

Rewards: 50k + 50k + 50k (+ 100k for the fourth) × multiplier at the time of each kill.
**Quirk (CONFIRMED in 68k, OS4 and CD):** the failure endings 0x2da and 0x2ee show
"Mission complete!" (2022), so they also play the success sound and raise the multiplier.
Reimplement as-is for faithfulness. Since part 2 runs after part 1, the first kill also
alerts the gang if it was not yet alerted; when B reaches 3 the flee order is given first
and C is already 1, so it is not overridden.

### Mission 8 — "Prison break bus" (slot 8, hidden phone 16, ev 800) — state 8

68k start ≈`0xc1760`, state block ≈`0xbf680–0xbfdc0`; OS4 ev 800 / `code_r0x010ec1c4`.

Phrases: 69 "The boss is about to spring the lads from jail. Drive the bus in the Eastern car
park to the prison." · 70 "Tough luck, kid." · 71 "Can't you do anything right?" ·
72 "Ok, the boss has just sprung the joint. Get down there before the pigs do!" ·
73 "You're too slow! The police arrived and nicked them all!" · 74 "Ok, here they come. Wait
here for them all to get in." · 75 "What the hell are you up to? Get back in the bus!" ·
76 "Ok, now quit screwing around!" · 77 "Great, now take them to the safe house." ·
78 "You fool! You let one of the prisoners die! I'm docking thirty grand..." · 79 "Ok, now
wait for all the lads to get off." · 80 "...Not a single inmate survived!" · 81–85 graded
success texts (5…1 survivors).

```
on ev 800:
    KillAllPhones(); Dialog_Begin(0); Say2(69); Dialog_End()
    CreateActor(0x321, 3 /*bus*/, 0x28400, 0x1dc00, 0, 0, 0)    // (80,59)
    AddMissionTrigger(0x321, 0x08, 0x321)     // get into the bus (arrow)
    AddMissionTrigger(0x352, 0x06)
    AddMissionTrigger(0x353, 0x09, 0x321)     // bus destroyed
    state = 8

state 8:   (A = still to board/unload, alive = 0x2fba48, phase = 0x2fba4c)
  ev 0x321 (in the bus):
      Say(72)
      for sx in 0x32..0x35: for sy in 0x68..0x69: SetCube(sx, sy, 1, 0x200, 0x200, 0, 0, 0)   // (50..53,104..105): prison wall breached
      AddMissionTrigger(0x322, 0x05, 40)                      // visible 40 s
      AddMissionTrigger(0x323, 0x1a, 0x19000, 0x34800, 0)     // within ±10000 of (50,105)
      AddMissionTrigger(0x325, 0x0e)                          // left the bus
      A = 5; alive = 5; phase = 0
  ev 0x322 (time up): Actor_Release(0x321); KillAllMissionTriggers(); Say(73); FAIL
  ev 0x323 (at the prison):
      KillMissionTrigger(0x322)                               // clock stops
      Say(74)
      SpawnPrisoner()
      phase = 1
  ev 0x324 (prisoner boarded):
      KillMissionTrigger(0x328)
      NextBoarding()
  ev 0x328 (prisoner killed while boarding):
      Actor_Release(0x322)
      alive -= 1
      if alive > 0: KillMissionTrigger(0x324); Say(78); NextBoarding()
      else:         Actor_Release(0x321); KillAllMissionTriggers(); Say(80); FAIL
  ev 0x327 (at the safe house; ignored if phase == 2):
      Say(79)
      A = alive
      Ped_ExitPlayerVehicle(0x322, -10)                       // 68k 0xb43d4: a pedmale3 gets out of the player's vehicle
      AddMissionTrigger(0x329, 0x09, 0x322)
      AddMissionTrigger(0x32a, 0x12, 0x322, 0x1cad4, 0x7c00)  // walk into the house (57,15), then removed
      phase = 2
  ev 0x32a (one prisoner inside):
      KillMissionTrigger(0x329)
      NextUnloading()
  ev 0x329 (prisoner killed while unloading):
      Actor_Release(0x322)
      alive -= 1
      if alive > 0: KillMissionTrigger(0x32a); Say(78); NextUnloading()
      else:         Actor_Release(0x321); KillAllMissionTriggers(); Say(80); FAIL
  ev 0x325 (left the bus):
      if phase == 1: KillMissionTrigger(0x324); KillMissionTrigger(0x328)
      if phase == 2: KillMissionTrigger(0x32a); KillMissionTrigger(0x329)
      AddMissionTrigger(0x326, 0x08, 0x321)                   // get back in
      Say(75)
  ev 0x326 (back in the bus):
      if phase == 1: KillMissionTrigger(0x324)
                     AddMissionTrigger(0x328, 0x09, 0x322); AddMissionTrigger(0x324, 0x0c, 0x322)
      if phase == 2: KillMissionTrigger(0x32a)
                     AddMissionTrigger(0x329, 0x09, 0x322); AddMissionTrigger(0x32a, 0x12, 0x322, 0x1cad4, 0x7c00)
      AddMissionTrigger(0x325, 0x0e)
      Say(76)
  ev 0x352 (died):        Actor_Release(0x321); Actor_Release(0x322); KillAllMissionTriggers(); Say(70); FAIL
  ev 0x353 (bus wrecked): Actor_Release(0x321); Actor_Release(0x322); KillAllMissionTriggers(); Say(71); FAIL

SpawnPrisoner():
    CreateActor(0x322, -10 /*pedmale3*/, 110000, 0x34800, 0, 0, 0)   // (53,105)
    AddMissionTrigger(0x324, 0x0c, 0x322)     // he walks to the player's vehicle and boards it
    AddMissionTrigger(0x328, 0x09, 0x322)

NextBoarding():
    SetWantedLevel(5)
    A -= 1
    if A < 1:
        AddMissionTrigger(0x327, 0x0f, 0x1c000, 0x8000, 0)   // safe house (56,16), arrow
        Say(77); phase = 0
    else:
        SpawnPrisoner()

NextUnloading():
    A -= 1
    if A < 1:
        Actor_Release(0x321); KillAllMissionTriggers()
        Say(0x56 - alive)               // 81 (5 alive) ... 85 (1 alive)
        AddMoney(alive * 30000)         // 150k ... 30k
        ShowMessage(2022); Unlock01()
        SetWantedLevel(3)               // after Unlock (CONFIRMED order)
    else:
        Ped_ExitPlayerVehicle(0x322, -10)
        AddMissionTrigger(0x329, 0x09, 0x322)
        AddMissionTrigger(0x32a, 0x12, 0x322, 0x1cad4, 0x7c00)
```

Exact 68k order inside 0x328/0x329 when someone survives: `Actor_Release(0x322)`,
`alive--`, `KillMissionTrigger(other)`, `Say(78)`, then (0x328) `SetWantedLevel(5)`,
`A--`… — boarding prisoners are re-created at the prison one at a time; unloading
prisoners are re-created out of the bus one at a time (the bus does not really carry
actors). In phase 0 (driving to the safe house) leaving the bus only arms 0x326; the
safe-house trigger 0x327 (type 0x0f, any mode) stays armed.

---

## 5. Global / non-mission events (CONFIRMED)

| Event | Source | Action |
|---|---|---|
| −1 | init, type 0 money ≥ 2,000,000 | not handled by the script; the evaluator returns 2 → level complete. |
| 0 | main loop after game over (`0x34c42c`) | ignored. In the CD script event 0 meant "shut down" (see §6). |
| 0x834 | type 0x2d: player enters the diblo 0x835 at (117,88) | Dialog(1) phrase 42; tracked 10 s visible countdown 0x835. |
| 0x835 | countdown expired | `Actor_Kill(0x835)` — the car blows up. |
| 0x836 | type 0x2f: Info at (117,87) collected | hint phrase 51 (speaker 4, Say1). |
| 0x898 | type 0x2f: secret Info pickup at (37,0) cube 1 (placed by the map, not the script) | phrase 400 (speaker 4, Say1). |

There is **no level-wide death/respawn handling**: this level never adds a type 0x3E
trigger, and death (type 6) is handled per mission (missions 0,1,2,4,5,6,7,8; mission 3
has none). `Mission_Shutdown` (`0xca322`, only for city 0 / level 1): `Actor_Release(0x835)`,
`KillMissionTrigger(0x834)`, `KillMissionTrigger(0x835)`.

## 6. Differences from the legacy CD script (`Maps/01.script`)

Compared call-by-call (argument reconstruction of `script01:0x11704`, helper script
`analysis/m01/cdcalls.py`, output `analysis/m01/cdcalls.txt`). **The mission logic,
events, coordinates, phrases and rewards are identical.** Differences are structural only:

* CD: blocking `GetEvent()` loop; global events 0x834/0x835/0x836/0x898 fall through into
  the state switch (harmless). Update 7: early `return`s.
* CD: event **0** ends the script: `Actor_Release(0x835)`, kill 0x834/0x835, wait for goodbye
  (`0x10578`). Update 7 moved this into `Mission_Shutdown` and ignores event 0.
* CD script01 coordinates are full 32-bit here (e.g. `0x2e000`, `0x103c4`, `0x11c00`) —
  no truncation was observed in this script.
* CD unlock `script01:0x113c0`, RefreshPhones `0x11104` and KillAllPhones `0x11226` are the
  same algorithms; the CD hidden array `0x3d0a0` is also u16 (`move.w #1, $10(a1)` → slot 8).
* In script01 the stub `0x10cf8` receives the AddMoney amounts (e.g. `0x186a0`, `0x249f0`,
  `0x493e0`), `0x112bc` is the tracked AddMissionTrigger, `0x11372` KillMissionTrigger,
  `0x11390` KillAllMissionTriggers, `0x10ba8` Actor_Release, `0x10a52` Actor_Kill,
  `0x10ad4` Actor_Attack, `0x10944`/`0x10918` the two ped-exit helpers, `0x109b0`
  SetWantedLevel, `0x10632` SetCube, `0x1068e` SetScenery, `0x107ea`/`0x107a6`
  SetFaceW/SetFaceS, `0x105a0`/`0x105e0` AddSound/DeleteSound, `0x10e4e` SetFlag34c28c.
  (script_api §6 lists `0x10cf8` as KillTrigger for script00; the stub table differs per
  script or that entry needs re-checking.)

## 7. Corrections / additions to `script_api.md` from this level

* **Type 0x2b** = vehicle actor `a` **falls into water**: `0xa6a12` is called from the vehicle
  physics (`FUN_000e0e46`, `FUN_000e109e`) when the vehicle's z reaches the water level, sets
  `+0x44a = 1` and plays sound 0x2b (splash). Used here as "car dumped in the sea".
  (INFERRED from code + phrases 22/23; replaces "wrecked/set on fire".)
* **Type 0x0c** confirmed in use as "pedestrian walks to and boards the player's vehicle"
  (prisoners boarding the bus, phrase 74 "Wait here for them all to get in"). INFERRED.
* **Type 0x2a** used on glass (face type 3) shop-window cells; fires when that window is
  smashed (INFERRED, phrase 16).
* `0xb43d4(id, pedModel)` = `Ped_ExitPlayerVehicle`: creates ped `~pedModel` with script id
  `id` getting out of the player's current vehicle (`+0x448 = 1`). `0xb4414(newId, vehId,
  pedModel)` = `Ped_ExitVehicle`: same from the first actor with id `vehId` (driver bails out).
* `0xb4360(b)` → `0xefef0` stores `0x34c28c`; the renderer (`~0xe1890`) then draws something
  at world (`0x34c288`, `0x34c294`, floor `0x34c290`) only if `0x34c288 >= 0` (default −1).
* The mission state variable of this level is `0x11a354` itself (= slot index), written
  by the level handler at mission start, not by KillAllPhones.
* Special object model 1001 = `objects/misc/cone.obj` (special table index 1).
* `SetScenery` (`0xb21a6`) 5th/6th args: 5th → cell byte `+0` (probably rotation), 6th → cell
  `+4` (special/phone number) — see open question 2.

## 8. Open questions

1. `SetFlag34c28c(1)` during the briefcase carry (mission 4): what is drawn, and is
   `0x34c288` ever ≥ 0 in level 01? (Default −1 → probably nothing visible.)
2. SetScenery argument order: `0xb21a6(sx,sy,cube,p4,p5,p6)` writes p4 → runtime cell `+1`
   (scenery), **p5 → cell `+0`**, **p6 → cell `+4`** (the byte the map loader reads as the
   phone/special number). So the API doc's `(…, scenery, special, rot)` is really
   `(…, scenery, cell+0, special)`; cell `+0` is probably the scenery rotation (values
   0x59 ≈ 125°, 0xb2 ≈ 250° here) — INFERRED, needs the runtime cell layout.
3. Exact meaning of the init SetCube block (x 26–30, y 61–70, cube 1) and mission-8 block
   (x 50–53, y 104–105): heights 0x200/0x200 with scenery 0 — "remove cube" vs "flat
   ground at 0x200"; check against `0xb48d8` and the map.
4. AddSound 4th arg 100000000 (volume? duration?) in mission 2.
5. Whether type 0x2b and type 9 can both fire for the same car (handled by `active[]`
   anyway).
6. GetPhoneCoords for duplicated phone 6: confirm the loader's x/y loop order (assumed
   x-outer ascending → (55,91) wins).
