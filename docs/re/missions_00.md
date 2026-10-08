# Level 00 (city 0, level 0) — mission reconstruction (Update 7)

Sources: 68k Update 7 `Payback68k.bin` (init `0xb91dc`, handler `0xb9b6a`–`0xbed0c`,
unlock `0xb506e`); OS4 Update 7 (init `0x010eabc0`, handler `0x010d2b24`, unlock
`0x010d29d4`, dispatcher `0x010f45cc`); legacy CD `Maps/00.script` (`analysis/ghidra_script00`,
main `FUN_000115ba`); phrases `extracted/game/Phrases/00.English.phrases`; map `Maps/0.map`.

The handler was read from an annotated 68k disassembly produced by
`analysis/tools_mission/calls68k.py` (folds argument pushes into `CALL Name(args)`, decodes
the switch tables, shows arguments pushed before a `bra` into a shared tail). Every value
below comes from that listing. The OS4 decompile was used as a cross-check: it agrees
everywhere it was compared (slots 0, 1, 2, 3 and 19, init, unlock).

Conventions:
* Coordinates are given as `raw (sx,sy)` with `sx = x>>11`, `sy = y>>11` (1 square = 0x800).
  `floor` = cube level 0–2. Angles: 0x1680 = 360° (0x5a0 = 90°, 0xb40 = 180°, 0x10e0 = 270°).
* `T(ev, type, a, b, c, d)` = `AddMissionTrigger` (tracked, 68k `0xb4f5a`). `KillMT(ev)` =
  `KillMissionTrigger`, `KillAllMT()` = `KillAllMissionTriggers`. `Say(s, n)` =
  `Dialog_Begin(s); Dialog_Say2(n); Dialog_End()`; `Hint(n)` = `Dialog_Begin(4);
  Dialog_Say1(n); Dialog_End()`. Unless marked "global", phrase numbers are level phrases
  (`00.English.phrases`).
* **FAIL(n, s=2)** = `Say(s, n); ShowMessage(2028 "Mission failed!"); UnlockPhones()`.
* **WIN** = `ShowMessage(2022 "Mission complete!")` (bonus multiplier +1, max 4) followed by
  `UnlockPhones()`. `AddMoney(n)` pays `n × multiplier`. Every AddMoney runs **before**
  the 2022 message, so it uses the multiplier from before the increment.
* `UnlockPhones()` = 68k `0xb506e` (§2).
* Every trigger is one-shot (deleted when it fires). Event numbers and actor ids are separate
  namespaces; the script reuses the same numbers for both.
* Model ids: vehicles 0 mundaneo, 1 pug, 3 bus, 7 van, 8 limo, 11 tank, 13 taxi (table
  `0x348120`). Peds `~m` (table `0x3480e0`): −9 → 8 pedmale2, −10 → 9 pedmale3,
  −11 → 10 pedmale4, −12 → 11 pedmale5_shotgun, −13 → 12 pedmale6_KKK. Specials
  (`0x30c3c0`): 1000 barrier, 1001 cone, 1002 dummy. CONFIRMED from the loader
  `FUN_000d7874` and `CreateActor` `0xb231c`.

Level globals (68k): `0x2fba18` (u16) "behind the line" flag; `0x2fba1c` general counter;
`0x2fba20` invalid-hit counter; `0x2fba24` shots counter; `0x2fba28` grenade-hit counter;
`0x2fba2c` loop index; `0x2fba30..0x2fba40` KKK alive[5]; `0x2fba44` "A" flag;
`0x2fba48` KKK not-killed count; `0x2fba4c` "B" flag; `0x2fba50` "C" flag; `0x2fba08`,
`0x2fb144` loop temporaries. `Mission_Init` zeroes only `0x2fba44/4c/50`. The others are
not reset per level. CONFIRMED.

---

## 1. Level setup — `0xb91dc` (OS4 `0x010eabc0`) — CONFIRMED

```
intro phrase 0x2fb224 = 106  "Welcome to Payback. Your target score for this level is 1,500,000 points. ..."
phone slots (all others empty = -1):
  slot 1 : event 100  (0x64),  phone 2,  visible
  slot 9 : event 900  (0x384), phone 3,  visible
  slot 7 : event 700  (0x2bc), phone 1,  HIDDEN (type 7, no arrow)
  slot 19: event 1900 (0x76c), phone 10, HIDDEN
AddTrigger(-1, 0x00, 1500000)        // money goal -> level complete (untracked)
AddTrigger(0x898, 0x2f, 0x25, 0, 1)  // secret: Info pickup at square (37,0) cube 1 collected (untracked)
AddLives(2)
CreateActor(100000, 1001 cone, 0xb400, 0x2e000, 0, 0, 0)    // (22,92)
CreateActor(100001, 1001 cone, 0xb400, 0x2f800, 0, 0, 0)    // (22,95)
CreateActor(100002, 1001 cone, 0x15400,0x2e000, 0, 0, 0)    // (42,92)
CreateActor(100003, 1001 cone, 0x15400,0x2f800, 0, 0, 0)    // (42,95)
RefreshPhones()   // first call: Say(2, 106)
ShowMessage(7)    // level phrase 7 "Answer the phone"
```
The four cones (ids 100000–100003) are never referenced again. They are decoration
(INFERRED).

Phone squares from `Maps/0.map` (scenery 14 with special n; floor 0): 1 (11,1), 2 (9,24),
3 (10,24), 4 (11,24), 5 (12,24), 10 (64,2), 11 (72,48), 12 (74,48), 13 (75,48),
14 (8,97), 15 (8,98), 17 (11,98). CONFIRMED by parsing the map. The secret cell (37,0)
cube 1 holds scenery 24 (Info), rotation 242, on a cube-1 roof at the north map edge.

### Phone answered (handler default branch, `curSlot` = −1) — `0xbe33e`
`0x11a354` (`curSlot`) is set **by the level handler** when it handles the phone event, not
by the engine. Each phone handler first calls `KillAllPhones()` (stops all other phones)
and briefs with **speaker 0** (`Dialog_Begin(0)`). CONFIRMED.

| Event | 68k | Brief (speaker 0) | Spawns / triggers | sets curSlot |
|---|---|---|---|---|
| 20 (0x14) | `0xbea1e` | 3 | `T(0x33,0x0b)` player in any vehicle; `T(0x4b,6)` | 0 |
| 100 | `0xbe3b8` | 0 | `T(0x65,0x0f,0x5c00,0x1e400,0)` (11,60); `T(0x96,6)`; `[2fba20]=0` | 1 |
| 200 (0xc8) | `0xbe87e` | 15 | mundaneo, see M2 | 2 |
| 300 (0x12c) | `0xbe990` | 27 | bus, see M3 | 3 |
| 400 (0x190) | `0xbe80a` | 34 | see M4 | 4 |
| 500 (0x1f4) | `0xbe902` | 39 | van, see M5 | 5 |
| 600 (0x258) | `0xbe502` | 52 | target ped, see M6 | 6 |
| 700 (0x2bc) | `0xbe5f8` | 58 | pug + gang, see M7 | 7 |
| 900 (0x384) | `0xbe420` | 86 | limo, see M9 | 9 |
| 1000 (0x3e8) | `0xbe75c` | 90 | red limo, see M10 | 10 |
| 1100 (0x44c) | `0xbea86` | 98 | KKK, see M11 | 11 |
| 1900 (0x76c) | `0xbec5e` | 20 | tank, see M19 | 19 |

Dispatch: `Mission_Event(ev)` → handler. 68k: `ev==0` → return; `ev==0x898` → `Hint`-style
`Dialog_Begin(4); Dialog_Say1(400); Dialog_End()`; otherwise `switch(curSlot)` (0..19, jump
table at `0xb9baa`; slots 8 and 12–18 and −1 go to the phone branch). Inside each slot,
`switch(ev − base)`. Unknown events are ignored. OS4 does the `0`/`0x898` part in the dispatcher
`0x010f45cc`. CONFIRMED.

---

## 2. Phone / mission progression — `UnlockPhones` `0xb506e` (OS4 `0x010d29d4`) — CONFIRMED

```
UnlockPhones():
  phoneEvent[curSlot] = -1                       // mission consumed: success OR failure
  if curSlot in {1,9} and phoneEvent[1]==-1 and phoneEvent[9]==-1:
      slot 4 <- ev 400,  phone 4
      slot 2 <- ev 200,  phone 5
  elif curSlot in {4,2} and phoneEvent[4]==-1 and phoneEvent[2]==-1:
      slot 11 <- ev 1100, phone 11
      slot 3  <- ev 300,  phone 12
      slot 6  <- ev 600,  phone 13
  elif curSlot in {11,3,6} and phoneEvent[11]==phoneEvent[3]==phoneEvent[6]==-1:
      slot 5  <- ev 500,  phone 14
      slot 10 <- ev 1000, phone 17
      slot 0  <- ev 20,   phone 15
  RefreshPhones()      // re-adds type 1/7 triggers; speaker 2 says random global 2150..2158 if a visible phone rings
  curSlot = -1
```
Hidden flags are never touched, so all unlocked phones are visible.
**A failed mission is removed for good.** No mission can be retried after it fails, except
by retrying inside M9.

Graph: {M1 (tutorial range), M9 (time trial)} → {M4 taxis, M2 dope car} → {M11 KKK, M3 bus,
M6 tail} → {M5 van bomb, M10 escort, M0 getaway}. M7 (pug, hidden phone 1) and M19 (tank,
hidden phone 10) are available from the start and unlock nothing. Completing everything is
not required: the level ends when money ≥ 1,500,000 (type-0 trigger).

---

## 3. Missions

### M1 — slot 1, ev 100, phone 2: weapons tutorial (shooting range) — 68k `0xb9bd2` (base 0x65)
Brief (speaker 0) **0** "So you fancy yourself as a driver, eh? Head to the concrete area
to the south. ..." → `T(0x65,0x0f,0x5c00,0x1e400,0)` (11,60) floor 0; `T(0x96,6)`;
`[2fba20]=0`.

```
ev 0x65 (arrived at range):
  Say(2,112 "open the crate ... <fists ...>")
  SetScenery(14,60,0, scenery 6 crate, rot 0xde, special 5 = shotgun)
  T(0x66,0x25,14,60,0)                // crate cell becomes empty = contents picked up
  SetFlag34c28c(1)
ev 0x66 (shotgun collected):
  Say(2,113 "shoot the dummies to the left ... no more than 20 shots ...")
  CreateActorFx(0x65, 1002 dummy, 0x4400,0x1dc00, 0, 0x5a0, 0, 5)   // (8,59)
  CreateActorFx(0x66, 1002,       0x4400,0x1e400, 0, 0x5a0, 0, 5)   // (8,60)
  CreateActorFx(0x67, 1002,       0x4400,0x1ec00, 0, 0x5a0, 0, 5)   // (8,61)
  behind=1 [2fba18]
  T(0x67,0x1e,0x5400)          // player x <= 0x5400 (10.5 sq): crossed the white line
  T(0x6f,0x28,0x65); T(0x70,0x28,0x66); T(0x71,0x28,0x67)   // dummy destroyed
  hits[2fba1c]=0
  T(0x72,0x22,5)               // player fires weapon 5 (shotgun)
  shots[2fba24]=0
ev 0x67: Dialog_Flush; Say(2,114 "You're too close ..."); behind=0; T(0x68,0x1f,0x5400)  // x > line
ev 0x68: Dialog_Flush; Say(2,115 "Ok, now obliterate the targets."); behind=1; T(0x67,0x1e,0x5400)
ev 0x6f/0x70/0x71 (dummy 0x65/0x66/0x67 destroyed):
  Dialog_Flush; Actor_Release(dummy); Dialog_Begin(2)
  if behind: Say2(116 "Nice one! Have 20,000 points."); AddMoney(20000); hits++
  else:      Say2(117 "That one doesn't count ..."); invalid[2fba20]++
  if hits+invalid == 3:
     Say2(118 "Now go open the next crate ...")
     SetScenery(14,59,0, 6 crate, rot 0x2c, special 3 = grenade); T(0x73,0x25,14,59,0)
     SetScenery(14,61,0, 24 Info, 0, 0); T(0xb4,0x2f,14,61,0)
     KillMT(0x67); KillMT(0x72); invalid=0
  Dialog_End
ev 0x72 (shotgun trigger pulled):
  T(0x72,0x22,5)               // re-arm
  if shots++ == 9:             // 10th pull = 20 shots (two per pull)
     Say(2) 120 "You took too many shots." + 118
     crate 2 / Info / T(0x73..), T(0xb4..) as above
     Actor_Release(0x65,0x66,0x67); KillMT(0x67,0x68,0x6f,0x70,0x71)
     // NOTE: 0x72 is NOT killed here: later shotgun shots keep incrementing [2fba24]
ev 0x73 (grenades collected):
  Say(2,119 "nuke the dummies to the south ... no more than 6 grenades ...")
  CreateActorFx(0x68,1002,0x5400,0x1fc00,0,0xb40,0,3)  // (10,63)
  CreateActorFx(0x69,1002,0x5c00,0x1fc00,0,0xb40,0,3)  // (11,63)
  CreateActorFx(0x6a,1002,0x6400,0x1fc00,0,0xb40,0,3)  // (12,63)
  behind=1; T(0x74,0x21,0x1ec00)   // player y > 0x1ec00 (61.5 sq): too close
  T(0x7c,0x28,0x68); T(0x7d,0x28,0x69); T(0x7e,0x28,0x6a)
  ghits[2fba28]=0; T(0x76,0x22,3) // grenade thrown; shots=0
ev 0x74: Flush; Say(2,114); behind=0; T(0x75,0x20,0x1ec00)
ev 0x75: Flush; Say(2,115); behind=1; T(0x74,0x21,0x1ec00)
ev 0x76 (grenade thrown):
  T(0x76,0x22,3); if shots++ == 5:      // 6th throw fails immediately
     SetFlag34c28c(0); KillAllMT(); Say(2) 120 + 121 "Can't you do anything right?"
     Actor_Release(0x68,0x69,0x6a); SetScenery(14,61,0,0,0,0)
     ShowMessage(2028); UnlockPhones()
ev 0x7c/0x7d/0x7e (grenade dummy destroyed):
  Flush; Actor_Release(dummy); Dialog_Begin(2)
  if behind: Say2(116); AddMoney(20000); ghits++   else: Say2(117); invalid++
  if ghits+invalid == 3:
     KillAllMT(); SetFlag34c28c(0); SetScenery(14,61,0,0,0,0)
     if hits == 3:   // all three SHOTGUN dummies valid (grenade misses do not matter)
         Say2(123 "... have an extra 25 grand!"); Dialog_End; AddMoney(25000); WIN
     else:
         Say2(122 "... you didn't get all the targets ..."); Dialog_End; ShowMessage(2028); UnlockPhones()
  else Dialog_End
ev 0x96 (player dead/arrested): SetFlag34c28c(0); SetScenery(14,61,0,0,0,0); KillAllMT
  Say(2,2 "Can't you bloody well do anything right?"); Actor_Release(0x65..0x6a)
  ShowMessage(2028); UnlockPhones()
ev 0xb4 (Info at (14,61) picked up): Hint(69 "It's okay to walk up to the dummies and put the grenade down ...")
```
Notes: there is no failure for the shotgun stage. The 25,000 bonus and the "Mission complete"
both depend only on all three shotgun kills being made from behind the line. The 6-grenade
limit fails on the 6th throw. 68k addresses: `0xb9c86` (0x65), `0xb9ce0` (0x66), `0xb9e14`,
`0xb9e5a`, `0xb9ea2/0xb9fa4/0xba0a6` (0x6f–0x71), `0xba1a8` (0x72), `0xba2b6` (0x73),
`0xba486` (0x76), `0xba522/0xba608/0xba6f0` (0x7c–0x7e), `0xba7d6` (0x96), `0xba858` (0xb4).
CONFIRMED (code). Type 0x22 fires when the grenade is thrown (on releasing fire, after the ammo is taken), so the 6th throw fails before it can hit anything; type 0x25 on the crate cell fires when the grenades are **picked up**, not when the crate is smashed (script_api.md §3.2, CONFIRMED).

### M9 — slot 9, ev 900, phone 3: limo time trial — 68k `0xbb9cc` (base 0x385)
```
phone: Say(0,86 "... Get in the Limo parked in the Western car park. <...>")
  CreateActor(0x385, 8 limo, 0x1c00,0x15400, 0, 0xb40, colour 1)    // (3,42)
  T(0x3b6,6); T(0x3b6,9,0x385)          // death / limo destroyed
  T(0x385,8,0x385)                       // player gets into limo
  SetScenery(1,42,0, 24 Info,0,0); T(0x3d4,0x2f,1,42,0)
  curSlot=9
ev 0x385 (in limo): Say(2,87 "head to the first checkpoint. The timer will start then.")
  T(0x3b8,0x0e)                          // left/changed vehicle -> quit
  T(0x386,0x0f,0x2c00,0x10c00,0)         // start line (5,33)
  SetFlag34c28c(1)
ev 0x386 (start): Say(2,30 "Go!"); KillMT(0x3ac) (never added; no-op)
  T(0x387,0x0f,0x3bd0,0x5c00,0)          // CP1 (7,11)
  T(0x3b7,5,110)                         // visible 110 s countdown
  T(0x3a2,0x11,80)                       // hidden 80 s bonus timer
  bonus[2fba1c]=1
checkpoints (each: on arrival add the next, type 0x0f, floor 0):
  0x387 -> T(0x388, 0x16400,0x5400)  (44,10)
  0x388 -> T(0x389, 0x17c00,0x2400)  (47,4)
  0x389 -> T(0x38a, 0x38c00,0x2400)  (113,4)
  0x38a -> T(0x38b, 0x39400,0x13400) (114,38)
  0x38b -> T(0x38c, 0x31c00,0x18c00) (99,49)
  0x38c -> T(0x38d, 0x2bc00,0x17400) (87,46)
  0x38d -> T(0x38e, 0x16c00,0x17400) (45,46)
  0x38e -> T(0x38f, 0x157c0,0x19064) (42,50)
  0x38f -> T(0x390, 0x4400,0x19400)  (8,50)
  0x390 -> T(0x391, 0x3400,0x25c00)  (6,75)
  0x391 -> T(0x392, 0x8400,0x25c00)  (16,75)
  0x392 -> T(0x393, 0x8400,0x15400)  (16,42)
  0x393 -> T(0x394, 0xbc00,0x14400)  (23,40)
  0x394 -> T(0x395, 0xbc00,0x11400)  (23,34)
  0x395 -> T(0x396, 0x2c00,0x10c00)  (5,33) finish
ev 0x396 (finish): KillAllMT; SetFlag34c28c(0); Actor_Release(0x385); Dialog_Begin(2)
  if bonus==1: Say2(89 "Here's a hundred grand ... extra twenty five thousand points."); AddMoney(125000)
  else:        Say2(33 "Looks like you've got what it takes, kid. Here's a hundred grand ..."); AddMoney(100000)
  Dialog_End; WIN
ev 0x3a2 (80 s elapsed): bonus=0
ev 0x3b7 (110 s expired = too slow, RETRY):
  KillAllMT; Say(2,88 "Too slow. Head back to the start and try again. ... handbrake ...")
  T(0x386,0x0f,0x2c00,0x10c00,0); T(0x3b6,6); T(0x3b6,9,0x385); T(0x3b8,0x0e)
  // flag 34c28c stays 1; the Info trigger 0x3d4 was killed (pickup stays on the map without a trigger)
ev 0x3b6 (death / limo destroyed): KillAllMT; SetFlag34c28c(0); SetScenery(1,42,0,0,0,0);
  Actor_Release(0x385); FAIL(2)
ev 0x3b8 (left the limo = give up): same cleanup; FAIL(32 "Aww, was it too tough for the little baby?")
ev 0x3d4 (Info (1,42)): Hint(70 "Drive carefully, because the car will slow down if it is badly damaged.")
```
68k: `0xbe420` (phone), `0xbba84` (0x385), `0xbbaea` (0x386), `0xbbb6e..0xbbd40` (CPs),
`0xbbd60` (finish), `0xbcc60` (0x3a2), `0xbbdc6` (0x3b6), `0xbbe0a` (0x3b7), `0xbbea4` (0x3b8),
`0xbbee8` (0x3d4). CONFIRMED.

### M2 — slot 2, ev 200, phone 5: dope car to Harry's garage — 68k `0xbc7fe` (base 0xc9)
```
phone: Say(0,15 "A runner has dumped a shipment of dope at the Western car park. ... yellow Mundaneo ...")
  CreateActor(1, 0 mundaneo, 0x1c00,0x1ac00, 0, 0xb40, colour 6)   // (3,53)
  T(0xc9,8,1); T(0xfa,9,1)
ev 0xc9 (in car): KillAllMT; T(0xfa,9,1); T(0xfb,0x0e)
  Say(2,16 "Ok, now take the car to Harry's garage in the North-West of the city.")
  T(0xd2,5,25)                  // visible 25 s countdown
  T(0xca,4, 3,11, 1, 1)         // garage door: square (3,11), trigger floor 1, door side 1 (+x)
ev 0xca (delivered): KillAllMT; Actor_Release(1); Say(1,17 "Ok, here's your cut."); AddMoney(100000); WIN
ev 0xfb (left car): KillTrigger(0xca) (raw, untracked kill); Say(2,10 "Get back in the damn car!"); T(0xcb,8,1)
ev 0xcb (back in): T(0xfb,0x0e); Say(2,11 "... take the car to Harry's garage ..., and quit messing about!"); T(0xca,4,3,11,1,1)
ev 0xd2 (time out): KillAllMT; Actor_Release(1); FAIL(18 "You're too bloody slow, kid.", speaker 1)
ev 0xfa (car destroyed): KillAllMT; Actor_Release(1); FAIL(19 "You screwed up, kid.")
```
The countdown keeps running while the player is out of the car. CONFIRMED (`0xbc87c`,
`0xbc8f8`, `0xbc934`, `0xbc98e`, `0xbc9a4`, `0xbc9c4`; OS4 identical).

### M4 — slot 4, ev 400, phone 4: two taxis to Rob's garage — 68k `0xbc61a`
```
phone: Say(0,34 "The boss wants you to take two taxis to Rob's garage in the middle of the city.")
  T(0x191,3,13)        // player driving a taxi (vehicle model 13)
  T(0x1c2,6); curSlot=4; Set11a734(13); count[2fba1c]=0
ev 0x191 (in a taxi): KillAllMT; Say(2,35 "Ok, now take it to Rob's garage."); Set11a734(-1)
  T(0x1c2,6); T(0x192,0x0e); T(0x193,4, 22,43, 1, 0)   // Rob's garage (22,43), door side 0 (-y)
ev 0x192 (left taxi): KillAllMT; Say(2,36 "... Get back in a taxi, you worthless turd!"); Set11a734(13)
  T(0x191,3,13); T(0x1c2,6)
ev 0x193 (taxi delivered): KillAllMT; Dialog_Begin(1)
  if count++ == 0: Say2(37 "Great work - now go get another one."); End; T(0x191,3,13); T(0x1c2,6); Set11a734(13)
  else:            Say2(38 "Nice one - here's your cut."); End; AddMoney(75000); WIN
ev 0x1c2 (death): KillAllMT; Say(2,2); Set11a734(-1); ShowMessage(2028); UnlockPhones()
```
`Set11a734(m)` (68k `0xb4284`) sets `0x11a734`. The traffic spawner then spawns model *m*
whenever no actor of that model exists (CONFIRMED: spawner `0xcf5cc`, script_api.md §5). This guarantees that a taxi
appears in traffic. No timer. CONFIRMED (`0xbc642`, `0xbc6c4`, `0xbc72e`, `0xbc7d2`).

### M3 — slot 3, ev 300, phone 12: bus to Jon's garage (bridge jump) — 68k `0xbe160` (base 0x12e)
```
phone: Say(0,27 "A large shipment has just been dumped in the central car park - go pick it up.")
  CreateActor(0x12d, 3 bus, 0x24400,0x32400, 0, 0x5a0, colour 6)   // (72,100)
  T(0x12e,8,0x12d); T(0x12f,9,0x12d)
ev 0x12e (in bus): KillAllMT; Say(2,28 "... take it to Jon's garage as fast as you can. ... jump the bridge ...")
  T(0x12f,9,0x12d); T(0x130,5,50)        // visible 50 s
  T(0x131,4, 12,116, 1, 3)               // Jon's garage (12,116), door side 3 (-x)
  T(0x133,0x0e)
ev 0x131 (delivered): KillAllMT; Say(2,29 "Great job!"); AddMoney(125000); WIN
ev 0x133 (left bus): Say(2,31 "What the hell are you doing? Get back in the bus!"); T(0x135,8,0x12d); KillMT(0x131)
ev 0x135 (back in): Say(2,28); T(0x131,4, 3,11, 1, 1); T(0x133,0x0e)
     // BUG (faithful): re-arms the garage at Harry's (3,11) side 1, not Jon's (12,116)
ev 0x12f (bus destroyed): KillAllMT; Actor_Release(0x12d); FAIL(19)
ev 0x130 (time out): KillAllMT; Actor_Release(0x12d); FAIL(18)
```
CONFIRMED (`0xbe188`, `0xbe20a`, `0xbe23e`, `0xbe28a`, `0xbe2e6`, `0xbe306`). The 0x135
garage coordinates are identical in OS4 (`AddTrigger(0x131,4,3,0xb,1,1)`) and in legacy.

### M6 — slot 6, ev 600, phone 13: tail the traitor — 68k `0xbbefa` (base 0x259)
```
phone: Say(0,52 "... his right-hand man is working for the other side. ... tail him, but be careful not to get spotted.")
  CreateActor(0x259, -9 pedmale2, 0x59d8,0x12400, 0, 0, 0)     // (11,36)
  T(0x259,0x1a,0x59d8,0x12400,0)  // player within 10000 of him
  T(0x28a,6); T(0x28c,9,0x259)     // death / target killed
  T(0x28b,0x1b,0x259)              // target sees player
  SetScenery(20,39,0, 24 Info,0,0); T(0x2a8,0x2f,20,39,0)
ev 0x259: Say(2,53 "Ok, he's on the move. ... keep your distance!")
  T(0x25a,0x17,0x259,0x7400,0x12c00); T(0x28d,0x19,0x259)   // walk; "lost" if > 33000 away
walk chain (type 0x17 on actor 0x259; arrival -> next):
  0x25a (14,37) -> T(0x25b, 0x7918,0x13c04) (15,39)
  0x25b -> T(0x25c, 0x9c40,0x13c04) (19,39)
  0x25c -> T(0x25d, 0x9c40,0x15400) (19,42)
  0x25d -> T(0x25e, 0x9470,0x15400) (18,42)
  0x25e -> T(0x25f, 0x9470,0x17c00) (18,47)
  0x25f -> T(0x260, 0xa400,0x18400) (20,48)
  0x260 -> T(0x261, 0xa400,0x19c00) (20,51)
  0x261 -> T(0x262, 0x9400,0x1a3ec) (18,52)
  0x262 -> T(0x26c, 0x9400,0x1d4c0) (18,58)
  0x26c -> T(0x263, 0x9400,0x20c00) (18,65)
  0x263 -> T(0x264, 0xa3de,0x20c00) (20,65)
  0x264 -> T(0x265, 0xbc00,0x20400) (23,64)
  0x265 -> T(0x266, 0x12, 0x259, 0xd1f4,0x20c00)  // (26,65): goes into the house, removed on arrival
ev 0x266: KillAllMT; Say(2,57 "That's a rival's house he's just gone into! ..."); AddMoney(100000)
  ShowMessage(2022); SetScenery(20,39,0,0,0,0); UnlockPhones()
fail events (each: KillAllMT; Actor_ReleaseIdle(0x259); Say(2,n); ShowMessage(2028); SetScenery(20,39,0,0,0,0); UnlockPhones()):
  0x28a death -> n=2;  0x28b spotted -> 54 "You idiot - he spotted you!"
  0x28c killed -> 55 "You moron! He's dead!";  0x28d lost -> 56 "You twat! You let him escape!"
ev 0x2a8 (Info (20,39)): Hint(71 "People only have a 180 degree field of view ...")
```
The rival's house at (26,65) is the same one that M5 blows up (INFERRED from coordinates).
CONFIRMED (`0xbbfb2..0xbc2be`).

### M11 — slot 11, ev 1100, phone 11: KKK meeting — 68k `0xba86a` (base 0x44d)
```
phone: Say(0,98 "The KKK are holding a meeting in central park. The boss wants 'em all dead.")
  CreateActor(0x44d,-13 KKK,0x31400,0xe400,0,0x226,0)   // all at (98,28)
  CreateActor(0x44e,-13,0x31018,0xe400,0,0x1068,0)
  CreateActor(0x44f,-13,0x31400,0xe018,0,0xdac,0)
  CreateActor(0x450,-13,0x31400,0xe7e8,0,0x8fc,0)
  CreateActor(0x451,-13,0x317e8,0xe400,0,0x6a4,0)
  for i in 0..4: T(0x489+i,0x28,0x44d+i)       // killed (arrow on each)
  T(0x47e,6); T(0x44d,0x1a,0x31400,0xe400,0)   // player within 10000
  resolved[2fba1c]=0; left[2fba48]=5 (not killed); alive[0..4]=1; fleeing[2fba44]=0; curSlot=11
Flee(): for i in 0..4 with alive[i]: Actor_Attack(0x44d+i,-1,-1)       // flee from player
                                     if !fleeing: T(0x475+i,0x19,0x44d+i)  // escaped (> 33000 away)
        fleeing=1
ev 0x44d (player approached): Flee(); Say(2,107 "Ok, now nuke the bastards.")
ev 0x489+i (KKK i killed):
  alive[i]=0; Actor_ReleaseIdle(0x44d+i); KillMT(0x475+i); Flee(); left--
  if ++resolved <= 4: Say(2, 112-left)  // left 4:108 "Hah! You nuked him", 3:109, 2:110, 1:111 "just one more left!"
  else: KillAllMT; Say(2, 99+left); AddMoney(20000*(5-left)); WIN
ev 0x475+i (KKK i escaped):
  alive[i]=0; Actor_ReleaseIdle(0x44d+i); KillMT(0x489+i); Say(2,105 "... You let one of them escape! ...")
  if ++resolved > 4:
     KillAllMT; Say(2, 99+left)   // 99 all/100k, 100 four/80k, 101 three/60k, 102 two/40k, 103 one/20k, 104 none
     if left <= 4: AddMoney(20000*(5-left)); WIN   else: ShowMessage(2028); UnlockPhones()
ev 0x47e (death): for alive i: Actor_ReleaseIdle(0x44d+i); KillAllMT; FAIL(2)
```
So the payout is 20,000 × kills. CONFIRMED (`0xbea86`, `0xba904`, `0xba9a8..0xbac88`,
`0xbad40..0xbb168`, `0xbb272`, `0xbb292`). The `112-left` and `99+left` phrase arithmetic
is computed in registers (0x70−n, 0x63+n).

### M5 — slot 5, ev 500, phone 14: van bomb outside the rival's house — 68k `0xbca08` (base 0x1f5)
```
phone: Say(0,39 "... Go pick up the van filled with explosives to the east of the city and park it outside his house.")
  CreateActor(0x1f5, 7 van, 0x37c00,0x16c00, 0, 0xb40, colour 4)  // (111,45)
  T(0x1f5,8,0x1f5); T(0x226,9,0x1f5)
ev 0x1f5 (in van): KillAllMT; T(0x226,6); T(0x226,9,0x1f5); T(0x1f6,0x0e)
  Say(2,40 "Right, now park it outside the bastard's house."); T(0x1f7,0x0f,0xc400,0x21400,0)  // (24,66)
ev 0x1f6 (left van): KillAllMT; T(0x226,6); T(0x226,9,0x1f5); Say(2,41 "... Get back in the van!"); T(0x1f5,8,0x1f5)
ev 0x1f7 (near house): T(0x1f8,0x10,0xc400,0x21400,0); Say(2,14 "Stop here!")     // 0x1f6 stays armed
ev 0x1f8 (stopped at house): KillAllMT; T(0x226,6); T(0x226,9,0x1f5)
  T(0x1fa,0x16,0x1f5,0xc400,0x21400)    // van moved outside ±0x1800 of the spot
  Say(2,42 "Get the hell out of there - she's gonna blow!"); T(0x1f9,5,10)   // visible 10 s
  moved[2fba1c]=0
ev 0x1fa: moved=1
ev 0x1f9 (bomb goes off): KillAllMT; Actor_Kill(0x1f5)
  if moved==0:
     for x in 26..30: for y in 61..70:
        if Random(2)==0:
           z  = Random(0x5dc)+0x4268     // RNG call order: z, then y offset, then x offset
           dy = Random(0x800); dx = Random(0x800)
           Explosion(x*0x800+dx, y*0x800+dy, z, 0, 0xbb8)
     for x in 26..30: for y in 61..70: SetCube(x,y,1, 0x200,0x200, 0,0,0)   // wreck the house
     Say(2,44 "Oh yeah! Look at that baby burn! ..."); AddMoney(100000); WIN
  else: Say(2,43 "You twat! You parked the van too far away from the house!"); Actor_Release(0x1f5)
        ShowMessage(2028); UnlockPhones()
ev 0x226 (death or van destroyed): KillAllMT; Actor_Release(0x1f5); FAIL(19)
```
CONFIRMED (`0xbe902`, `0xbca84..0xbcdfc`; explosion loop `0xbccae`, cube loop `0xbcd4a`).
`0xb4296` arguments are (x, y, z, layer, size): the 4th is a particle layer (scripts pass 0), not an owner (CONFIRMED, script_api.md §5).

### M10 — slot 10, ev 1000, phone 17: escort the boss's red limo — 68k `0xbb308` (base 0x3e9)
```
phone: Say(0,90 "... he wants an escort. Tail his red limo.")
  CreateActor(0x3e9, 8 limo, 0x3bc00,0x3bc00, 0, 0x5a0, colour 4)   // (119,119)
  T(0x3e9,0x0d,0x3e9)       // player within 6000 of the limo
  T(0x41a,6); T(0x41b,9,0x3e9)
ev 0x3e9: Say(2,91 "Ok, now tail him. ..."); kills[2fba1c]=0
  CreateActor(0x3ea,-12 shotgun ped,0x29800,0x339ec,0,0x10e0,0)   // (83,103)
  CreateActor(0x3eb,-12,0x29800,0x33e14,0,0x10e0,0)                // (83,103)
  T(0x410,9,0x3ea); T(0x411,9,0x3eb)                 // gunmen killed
  T(0x412,0x1d,0x3ea,0x3e9); T(0x412,0x1d,0x3eb,0x3e9)  // a gunman is near & facing the limo
  T(0x3ea,0x17,0x3e9,0x3ac00,0x3ac00)                 // limo drives to WP1
  T(0x41c,0x19,0x3e9)                                 // limo > 33000 from player: lost
  T(0x415,0x13,0x3e9,0x50)                            // limo speed > 0x50
limo route (type 0x17 on 0x3e9; arrival event -> next waypoint):
  0x3ea (117,117) -> 0x3eb (0x39400,0x3ac00)(114,117) -> 0x3f7 (0x39400,0x3c400)(114,120)
  -> 0x3f8 (0x37400,0x3c400)(110,120) -> 0x3f9 (0x33400,0x3c400)(102,120)
  -> 0x3fa (0x30400,0x3c400)(96,120) -> 0x3fb (0x2d400,0x3c400)(90,120)
  -> 0x3ec (0x2a400,0x3c400)(84,120) -> 0x3fc (0x26c00,0x3c400)(77,120)
  -> 0x3fd (0x26c00,0x39400)(77,114) -> 0x3ed (0x26c00,0x36400)(77,108)
  -> 0x3ee (0x26c00,0x34008)(77,104) -> 0x3ef (0x26c00,0x30400)(77,96)
  -> 0x3fe (0x26400,0x2ec00)(76,93) -> 0x3f0 (0x26400,0x2bc00)(76,87)
  -> 0x3f1 (0x26400,0x29400)(76,82) -> 0x3f2 (0x24c00,0x29400)(73,82)
  on ev 0x3ee additionally: if kills <= 1: Actor_Attack(0x3ea,0x41,0x3e9); Actor_Attack(0x3eb,0x41,0x3e9)
                            Say(2,93 "Shit, look out!")     // ambush
ev 0x3f2 (limo arrived): PedInVeh_b4414(0x3ec, 0x3e9, -10)  // boss (pedmale3, id 0x3ec) gets out of the limo (0xdb03e; CONFIRMED)
  KillMT(0x41b); KillMT(0x413); KillMT(0x414); KillMT(0x415)
  T(0x41b,9,0x3ec); T(0x3f3,0x12,0x3ec,0x25c00,0x274e8)      // boss walks in at (75,78), removed
ev 0x3f3: Actor_Release(0x3e9,0x3ea,0x3eb); KillAllMT; Say(2,94 "Nicely done. Here's one hundred and twenty five grand.")
  AddMoney(125000); WIN
ev 0x410: kills++; Actor_Attack(0x3eb,0x50,-1); Say(2,62 "Nicely done!")    // other gunman turns on player
ev 0x411: kills++; Actor_Attack(0x3ea,0x50,-1); Say(2,63 "Great move!")
ev 0x412: Actor_Attack(0x3ea,0x50,0x3e9); Actor_Attack(0x3eb,0x50,0x3e9)    // attack the limo
limo-stopped watchdog:
  ev 0x415 / 0x414: (0x414 also KillMT(0x416)); T(0x413,0x14,0x3e9,1)         // wait for speed < 1
  ev 0x413: T(0x416,0x11,2); T(0x414,0x13,0x3e9,0x50)                          // stopped: 2 s timer
  ev 0x416: Say(2,96 "Hey kid! Come and help us out!"); KillMT(0x414); T(0x417,0x13,0x3e9,0x50)
  ev 0x417: Flush; Say(2,97 "Thanks!"); T(0x413,0x14,0x3e9,1)
fails (each: Actor_Release(0x3e9,0x3ea,0x3eb); KillAllMT; FAIL(n)):
  0x41a death -> 19; 0x41b limo (later: boss) destroyed -> 92 "... it's a good job the Limo was a decoy!"
  0x41c lost -> 95 "You idiot! You lost him!"
```
The same phrase 92 is used when the boss ped is killed after leaving the limo. Faithful.
CONFIRMED (`0xbe75c`, `0xbb388..0xbb99e`).

**Arrival and the boss's exit** (checked against the decompile, CONFIRMED):
- The limo is a script actor with skill 0, so it cruises in the lowest band (about 80–150; vehicles.md §6.2).
  It "arrives" when its bonnet is within `2·1000` (Manhattan) of the last waypoint; `0xa6640` then sets state
  4, which gives **no input at all**: no brake, so the limo coasts on from about 100 and rolls to a stop.
  The original does exactly this; the port matches.
- `PedInVeh_b4414(0x3ec, 0x3e9, -10)` creates the boss through `0xdb03e`, which places him `1.2·(192 +
  half-width)` to one side at the door's longitudinal position (2/3 of the limo's length ahead of its centre),
  facing away from the car. The side comes from the toggle `0x34c1b0`, which the main loop sets to 1 right
  before every frame's trigger update (68k `FUN_000ec686` ~127806; OS4 ~49717). The first exit of a frame
  therefore flips it to 0, the **driver's door side** (model −X, the right). That is the road side of a car
  driving on the left. The port used to start this toggle at the other side and never reset it, so the boss
  got out on the kerb side. Fixed: `MissionState::exit_toggle` is reset every frame.
- The boss then walks straight to (75,78) (goto-and-vanish 0x12). He has no protection from traffic, so a
  passing car faster than 50 can still run him over (fail 92). In port test runs that car was a pursuing police
  car (the bot was wanted). This is faithful: nothing in the original stops or steers cars around him.


### M0 — slot 0, ev 20, phone 15: getaway driver (bank job) — 68k `0xbcf88` (base 0x1e)
State: `near`=[2fba44] (player near the pickup/bank), `inCar`=[2fba50], `robbed`=[2fba4c]
(alarm on), `cnt`=[2fba1c].
```
phone: Say(0,3 "... Nick a car and then pick up Ron and Trey."); T(0x33,0x0b); T(0x4b,6)
ev 0x33 (player in any vehicle):
  KillAllMT; SetPlayerId(5000)        // the player's vehicle gets id 5000
  T(0x4e,6)
  CreateActor(0x32 Ron, -10 pedmale3, 0x32c00,0x13400,0,0,0)    // (101,38)
  CreateActor(0x33 Trey,-11 pedmale4, 0x33400,0x13400,0,0,0)    // (102,38)
  T(0x4c,9,0x32); T(0x4d,9,0x33); T(0x34,0x0d,0x32); T(0x34,0x0d,0x33)   // near Ron/Trey
  T(0x4e,9,5000); T(0x28,0x0e)
  Say(2,4 "Ok, now go and pick up Ron and Trey."); cnt=1
ev 0x28 (left car before pickup): KillAllMT; Say(1,10); T(0x4b,6); T(0x4e,9,5000); T(0x29,8,5000)
ev 0x29 (back in): KillAllMT; Say(1,4); T(0x4b,6); T(0x4c,9,0x32); T(0x4d,9,0x33); T(0x34,0x0d,0x32)
  T(0x4e,9,5000); T(0x28,0x0e)            // only Ron's proximity trigger is re-added
ev 0x34 (near them): if cnt==1:
  KillAllMT; Say(1,9 "Glad you could make it!")
  T(0x4e,6); T(0x4c,9,0x32); T(0x4d,9,0x33)
  T(0x35,0x0c,0x32); T(0x36,0x0c,0x33)   // Ron/Trey walk to and board the player's car; they vanish on boarding, no type-9 event (CONFIRMED)
  T(0x4e,9,5000); T(0x2a,0x0e); T(0x1e,0x19,0x32)   // player > 33000 from Ron
  cnt=0; inCar=1; near=1
ev 0x1e (drove away): KillMT(0x35); KillMT(0x36); T(0x1f,0x0d,0x32); T(0x1f,0x0d,0x33)
  Say(1,49 "Get back to the pick up point!"); near=0
ev 0x1f (back): if near==0: near=1
  if inCar==1: T(0x35,0x0c,0x32); T(0x36,0x0c,0x33); Say(1,50 "Ok, now wait for Ron and Trey.")
  else: Say(1,51 "There ain't much point in you being here if you're not going to bring the bloody car! ...")
  T(0x1e,0x19,0x32)
ev 0x2a (left car at pickup): inCar=0; Say(1,10); KillMT(0x35); KillMT(0x36); T(0x2b,8,5000)
ev 0x2b (back in): if inCar==0: inCar=1; Say(1,4)
  if near==1: T(0x35,0x0c,0x32); T(0x36,0x0c,0x33)
  T(0x2a,0x0e)
ev 0x35/0x36 (one boarded): if cnt++ == 1:     // both aboard
  KillAllMT; Say(1,8 "Take us to the bank.")
  T(0x4e,6); T(0x38,0x0e); T(0x37,0x0f,0x2cc00,0x25c00,0) (89,75); T(0x4e,9,5000)
ev 0x37 (near bank): KillAllMT; Say(1,14 "Stop here!")
  T(0x4e,6); T(0x38,0x0e); T(0x3a,0x10,0x2cc00,0x25c00,0); T(0x4e,9,5000)
ev 0x38 (left car en route): KillAllMT; Say(1,10); T(0x4b,6); T(0x4e,9,5000); T(0x39,8,5000)
ev 0x39 (back in): KillAllMT; Say(1,13 "Quit messing about and take us to the bank!")
  T(0x4b,6); T(0x38,0x0e); T(0x37,0x0f,0x2cc00,0x25c00,0)
ev 0x3a (stopped at bank): KillAllMT; Say(1,12 "Wait here!")
  PedInVeh_b43d4(0x32,-10); PedInVeh_b43d4(0x33,-11)   // Ron & Trey get out of the player's car (`0xdb03e`, CONFIRMED)
  T(0x4e,6); T(0x4e,9,0x32); T(0x4e,9,0x33); T(0x3d,0x0e)
  T(0x3b,0x12,0x32,0x2a5bc,0x26400)   // Ron walks in (84,76), removed on arrival
  T(0x3c,0x12,0x33,0x2a5bc,0x25c00)   // Trey walks in (84,75)
  T(0x4e,9,5000); T(0x2e,0x1c,0x2cc00,0x25c00)   // player leaves ±0x1800 of the bank stop
  cnt=0; inCar=1; robbed=0; near=1
ev 0x3b/0x3c: if cnt++ == 1: KillMT(0x4e); T(0x4e,6); T(0x4e,9,5000); T(0x3e,0x11,10)   // 10 s robbery
ev 0x3e (robbery done): robbed=1
  SetCube(85,74,2, 0x40,0x150, scenery 22 AlarmBox_flash, rot 0x3f, flags 1)
  SetCube(85,77,2, 0x40,0x150, 22, 0x3f, 1)
  AddSound(0, 0x30, 1, 100000000, 16000, 0x2a800,0x26000, 20000)   // alarm loop at (85,76)
  CreateActor(0x32,-10,0x2a84c,0x26400,0,0,0); CreateActor(0x33,-11,0x2a84c,0x25c00,0,0,0)  // (85,76)/(85,75)
  T(0x4e,9,0x32); T(0x4e,9,0x33)
  if inCar==1 and near==1: T(0x41,0x0c,0x32); T(0x42,0x0c,0x33)
  cnt=0
ev 0x2e (player drove off): near=0; KillMT(0x41); KillMT(0x42)
  Say(2,46 "Where the hell are you going? Get back to the bank!"); T(0x2f,0x0f,0x2cc00,0x25c00,0)
ev 0x2f (back): if near==0: near=1
  if inCar==1 and robbed==1: T(0x41,0x0c,0x32); T(0x42,0x0c,0x33)
  T(0x2e,0x1c,0x2cc00,0x25c00); Say(2,47 "Right, now stay put and wait for Ron and Trey.")
ev 0x3d (left car at bank): Say(2,10); T(0x3f,8,5000); KillMT(0x41); KillMT(0x42); inCar=0
ev 0x3f (back in): if inCar==0: Say(2,45 "Ok, now stay in the damn car!"); T(0x3d,0x0e); inCar=1
  if near==1 and robbed==1: T(0x41,0x0c,0x32); T(0x42,0x0c,0x33)
ev 0x41/0x42 (boarded): if cnt++ == 1:
  KillAllMT; Say(1,24 "Take us to Harry's garage."); SetWantedLevel(5)
  T(0x3d,0x0e); T(0x4e,9,5000); T(0x43,4, 3,11, 1, 1)   // Harry's garage
  T(0x4e,5,50)                // visible 50 s -> fail
  T(0x44,0x11,40)             // hidden 40 s -> hurry
  T(0x4e,6)
ev 0x44: Say(1,26 "Hurry up! There's only ten seconds left!")
ev 0x43 (delivered): KillAllMT
  if robbed==1: SetCube(85,74,2,0x40,0x150, 21 AlarmBox,0x3f,1); SetCube(85,77,...21...); DeleteSound(0); robbed=0
  Say(1,25 "A job well done! Here's you're cut - a cool hundred and fifty grand!")
  SetWantedLevel(0); AddMoney(150000); WIN
fails:
  0x4b / 0x4e: KillAllMT; restore alarm as above if robbed; Actor_Release(0x32,0x33,5000); FAIL(19)
  0x4c (Ron killed): KillAllMT; Actor_Release(0x32,0x33,5000); FAIL(5 "You idiot! You killed Ron!")
  0x4d (Trey killed): same with FAIL(6 "You moron! You nuked Trey!")
```
After 0x41/0x42, the re-armed `T(0x3d,0x0e)` leads to 0x3d/0x3f again if the player gets out.
CONFIRMED (`0xbcffe..0xbe0c2`; OS4 `0x010d2b24` case 0 identical).

### M7 — slot 7, ev 700, hidden phone 1: steal the white Pug — 68k `0xbc2dc` (base 0x2bd)
```
phone: Say(0,58 "... over three hundred grand in the white Pug ... we'll split it with you 50-50.")
  CreateActor(0x2bd, 1 pug, 0x33068,0x6400, 0, 0xfa0, colour 7)     // (102,12)
  CreateActor(0x2be,-12 shotgun ped,0x32400,0x6400,0,0x6e0,0)       // (100,12)
  CreateActor(0x2c0,-12,0x33c20,0x6400,0,0xfa0,0)                   // (103,12)
  T(0x2bd,0x1b,0x2be); T(0x2bd,0x1b,0x2c0)       // a guard sees the player
  T(0x2be,8,0x2bd)                               // player gets into the pug
  T(0x2da,9,0x2be); T(0x2da,9,0x2c0)             // guard killed
  T(0x2ee,6); T(0x2ef,9,0x2bd)
  alerted[2fba4c]=0; kills[2fba1c]=0
ev 0x2bd (spotted): if !alerted: alerted=1; KillMT(0x2bd)
  Say(2,64 "Bugger! They've spotted you - nick the car and get the hell out of there!")
  Actor_Attack(0x2be,0x1e,-1); Actor_Attack(0x2c0,0x1e,-1)
ev 0x2be (in pug): if !alerted: alerted=1; KillMT(0x2bd); Actor_Attack(0x2be,0x3c,-1); Actor_Attack(0x2c0,0x3c,-1)
  T(0x2bf,4, 22,43, 1, 0)     // Rob's garage
  T(0x2c0,0x0e); Say(2,65 "Great, now take it back to Rob's garage.")
ev 0x2c0 (left pug): KillMT(0x2bf); T(0x2c1,8,0x2bd); Say(2,66 "What the hell are you up to? Get in the damn pug!")
ev 0x2c1 (back in): T(0x2bf,4,22,43,1,0); T(0x2c0,0x0e); Say(2,67 "If you've finished messing about, take the car to Rob's garage.")
ev 0x2da (guard killed): kills++
  if !alerted: alerted=1; KillMT(0x2bd); Say(2,64); Actor_Attack(0x2be,0x28,-1); Actor_Attack(0x2c0,0x28,-1)
  Say(2, 59+kills)           // 60 "One down, one to go!" / 61 "... got all the little gits! ... extra twenty five grand"
  if kills==2: AddMoney(25000)
ev 0x2bf (delivered): KillAllMT; Actor_Release(0x2bd); Actor_ReleaseIdle(0x2be); Actor_ReleaseIdle(0x2c0)
  Say(1,68 "Nicely done! Here's your cut - a cool one hundred and fifty grand!"); AddMoney(150000); WIN
fails (KillAllMT; Actor_Release(0x2bd); Actor_ReleaseIdle(0x2be,0x2c0); FAIL(n)):
  0x2ee death -> 19; 0x2ef pug destroyed -> 59 "You fool! You let the pug get nuked!"
```
CONFIRMED (`0xbe5f8`, `0xbc35a..0xbc5e8`).

### M19 — slot 19, ev 1900, hidden phone 10: tank kill frenzy — 68k `0xbce1c` (base 0x76d)
```
phone: Say(0,20 "Go nick a tank from the military base to the south-east of the city.")
  CreateActor(0x76c, 11 tank, 0x36400,0x25400, 0, 0xb40, colour 6)   // (108,74)
  T(0x76d,8,0x76c); T(0x78a,6); T(0x794,9,0x76c)
ev 0x76d (in tank): KillAllMT; Actor_Release(0x76c); T(0x78a,6)
  Say(2,21 "Ok - now nuke everything you can. If you can get over 20,000 points in the next 60 seconds, ...")
  ShowMessage(2029 global "Kill frenzy!")
  T(0x776,5,60)               // visible 60 s
  T(0x780,0x0a,15000)         // money >= money_at_add + 15000 (raw score, includes the multiplier)
ev 0x780: KillAllMT; Say(2,22 "Nicely done - have an extra 50,000 points!"); AddMoney(50000); WIN
ev 0x776 (time out): KillAllMT; FAIL(18)
ev 0x78a (death): KillAllMT; FAIL(19)
ev 0x794 (tank destroyed before entering): KillAllMT; Actor_Release(0x76c); FAIL(23 "How the hell did you manage to blow up a tank?!?")
```
The text promises 20,000 points, but the code requires +15,000. Once the player is in the
tank, destroying it is no longer a failure (0x794 is not re-armed). CONFIRMED (`0xbec5e`,
`0xbce84..0xbcf68`).

---

## 4. Global events
* **ev 0**: ignored (68k `0xb9b78`; OS4 dispatcher). CONFIRMED.
* **ev 0x898 (2200)**: secret Info pickup at (37,0) cube 1 → `Dialog_Begin(4);
  Dialog_Say1(400 "I really didn't think anyone would figure out how to get this - well
  done!"); Dialog_End()`. No reward. One-shot, untracked, works during missions. CONFIRMED.
* **ev −1**: money goal, handled by the engine (level complete). The handler ignores it.
* **Death/arrest**: no global handling. Each mission arms its own type-6 trigger and fails
  the mission. No type 0x3e (respawn) triggers in this level. CONFIRMED.
* Hints with speaker 4 / `Dialog_Say1`: 69 (M1), 70 (M9), 71 (M6), 400 (secret).

## 5. Update 7 vs legacy CD `00.script`
* Structure, phone slots, unlock rules, events, triggers, phrases, money amounts, actors
  and coordinates match wherever compared. Legacy decompile values are byte/word-truncated,
  e.g. `0xe360` for 1,500,000 and angle `0x40` for 0xb40. CONFIRMED by spot checks of init,
  phone handlers, M0, M3, M11 and M19.
* Legacy runs a blocking loop (`FUN_00010bf8` waits for an event) and exits on event 0. Update 7
  ignores event 0.
* Legacy computes the KKK payout with a multiply helper (`FUN_0001d62c`). Update 7 inlines
  20000×(5−n). Same result (INFERRED).
* The M3 "garage re-armed at Harry's" quirk is present in legacy too (`(0x31,4,3,0xb,1,1)`).
* Legacy stub codes (corrections for script_api §6): stub `0x10cf8` sends code **0x0b** and
  is **AddMoney** (e.g. 150000 at M0 0x43). Stub `0x10b80` sends code **0x13** and is
  **SetPlayerId** (M0 0x33). Legacy KillMissionTrigger/KillAll/Unlock are local helpers
  `0x11372`/`0x11390`/`0x113c0`.

## 6. Open questions
1. ~~`SetFlag34c28c(b)`~~ Resolved: `ShowCheckpointMarker` (`0xb4360` → `0xefef0`). While
   on, a flashing Checkpoint.red/blue icon is drawn at the current HUD-arrow target
   (`0x34c288/94/90`, written each tick by `0xeff2a` from the active arrow triggers; −1 =
   none). In M1 that is the crate at (14,60). CONFIRMED.
2. ~~Firing moment of type 0x22~~ Resolved: sent when the projectile is created, after the
   ammo is taken; a grenade on releasing fire. The 6th grenade fails at the throw. CONFIRMED
   (`0xa6c62` in `0xe3a58`).
3. ~~Type 0x25 on the crate cell~~ Resolved: fires on **pickup** (`0xd0988` sets scenery 0);
   smashing a full crate only turns it into the item (`0xd0670`). CONFIRMED.
4. ~~`PedInVeh_b43d4` / `PedInVeh_b4414`~~ Resolved: they spawn ped model ~m with script id
   `id` beside the player's vehicle / vehicle `veh` through `0xdb03e` (alternating sides, see
   M10 above and people.md §6.3).
5. ~~Type 0x0c on Ron/Trey~~ Resolved: they walk to and board the player's vehicle, are set
   to state 6 and removed next frame (`0xdc5ac` → `0xcf290`) without a type-9 event. CONFIRMED.
6. ~~`SetCube(x,y,1,0x200,0x200,0,0,0)` over (26..30,61..70)~~ Resolved: slope 0 removes cube 1
   there, which is the pitched roof (slopes 3/5) of a 5×10 house; the house stays, open on
   top. CONFIRMED (`0xb48d8`, map 0 data).
7. ~~`AddSound` arguments~~ Resolved: (vol 100000000, 16000 Hz, x, y, z = 20000 street level);
   see script_api.md §6.
8. ~~Colour indices~~ Resolved: paint index 0 black, 1 blue, 2 green, 3 purple/orange,
   4 red, 5 silver, 6 yellow, 7 white (`0x348198` Shine.*, `0x3481b8` Cardef.*, loaded at
   ~`0xed682`). Matches the phrases. CONFIRMED.
9. The init cones (ids 100000–100003 at (22,92),(22,95),(42,92),(42,95)) are never used
   again; the exe's built-in level 00/01 inits (`0xb91dc`, `0xb9324`) create the same four.
   They stand at both ends of the elevated road over the canal (x 22–43, y 90–96, gap at
   x 31–33): probably a road-closure decoration. INFERRED; needs an in-game look.
