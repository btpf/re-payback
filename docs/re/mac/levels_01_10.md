# Levels 01 and 10: Mac OS X scripts vs our port

Sources compared:

- Mac (`analysis/ghidra_macx/decompiled.c`):
  - Level 01: `InitScript_01__Fv` @ 10129390 (l.107166), `ProcessTrigger_01__Fi` @ 101315d0
    (l.110176-111268), `FinishMission_01__Fv` @ 10124620 (l.104952), `BlowUpGarage__Fv` @
    101264f0 (l.105692) and the level-01 part of `CloseScript__Fv` @ 1013e230 (l.114759).
    Jump tables: the outer table 101913a0 is indexed by curSlot (0..8). The inner tables are
    101912d4 (M0, indexed by event), 10191200 (M4, base 0x191) and 10191138 (M5, base 0x1f5).
  - Level 10: `InitScript_10__Fv` @ 10129650 (l.107220), `ProcessTrigger_10__Fi` @ 10134800
    (l.111269), `FinishMission_10__Fv` @ 10124720 (l.104986), `DoRace__Fi` @ 10126f60
    (l.106121), `DoHijack__Fi` @ 101273b0 (M500) and `DoSeekDestroy__Fi` @ 101278b0 (M600).
    Level 10 doesn't use `DoRace2`. Jump tables: the outer table 10191600 is indexed by
    curSlot (0..7). The inner tables are 101915d4 (race, events 0..10), 1019148c (M100, base
    0x65) and 101913c4 (M200, base 0xc9).
  - I read the data tables used by DoRace, DoHijack and DoSeekDestroy (through TOC 1017d0b4..
    1017d0e8 → 101743f0..1017456c) from `mem_macx.pkl`.
- Port: `crates/payback/src/mission/levels/level_01.rs`, `level_10.rs`.
- Amiga: 68k 0xb9324 / 0xbed0c / 0xb5198 (L01) and 0xb953c / 0xc1a92 / 0xb5230 (L10). OS4
  010ead34 / 010ebddc / 010d9c40 and 010eaf98 / 010eef18 / 010d9d08. Docs:
  `docs/re/missions_01.md`, `missions_10.md`.

## Findings (differences)

No gameplay-relevant differences in money, phrases, spawns, triggers, timers or map edits.
The Mac is the same source as the Amiga in both levels. I found two small port deviations
from Mac+68k (one ordering, one RNG order) and a few no-op or comment nits.

1. **L01 M3 phone 300: order of the car triggers.** Mac phone-answer tail of
   `ProcessTrigger_01` (l.111180-111189) adds the triggers grouped by kind:
   `0x137,0x138,0x139` (type 8), then `0x141..0x143` (type 0x2b), then `0x14b..0x14d` (type 9),
   then `0x154`. The 68k (ghidra_68k l.~101310-101345) and OS4 (l.116225-116234) use the same
   order. The port interleaves them per car: 0x137, 0x141, 0x14b, 0x138, ...
   (`level_01.rs:256-260`, the `for i in 0..3` loop). All ids, types and args are identical.
   Only the position in the trigger list differs, which decides the order in which events
   fire on the same tick. For example, if car 0 is wrecked (0x14b) and car 1 reaches the sea
   (0x142) on the same tick, the Mac/Amiga fire 0x142 first (phrase 23, then on the last car
   the 24 path). The port fires 0x14b first (phrase 25). The final pay and result are the
   same; only the phrase order can differ.
   Classification: port bug (minor, ordering). Recommendation: use three separate loops in
   the Mac order. CONFIRMED (order); the practical impact is INFERRED, and it can only happen
   when two cars resolve on the same tick.

2. **L10 M200 gas puff (event 0xcc): RNG call order.** Mac case 10135cd8 (M200 inner table
   idx 3, l.~111830) calls `random` in reverse argument order: `random(500)` (life),
   `random(100)` (vz), `random(0x50)` (vy), `random(0x50)` (vx), `random(200)` (z),
   `random(200)` (y), `random(200)` (x). The 68k (ghidra_68k l.102338-102364) has the same
   right-to-left order. OS4 (l.117491-117499) evaluates left to right (x, y, z, vx, vy, vz,
   life), and the port follows OS4 (`level_10.rs:559-571`, comment "Random calls in argument
   order (OS4 0x010d13f4)"). The port's `garage_explodes` in level 01 already uses the
   68k/Mac right-to-left order (`level_01.rs:911-914`, Mac `BlowUpGarage` l.105710-105714),
   so the two levels are inconsistent. The number of RNG calls is the same, so the stream
   stays in sync, and only the puff positions/velocities/lifetimes get different draws
   (visual only).
   Classification: port follows the OS4 compiler artefact; Mac and 68k agree. Recommendation:
   draw in the order life, vz, vy, vx, z, y, x to match the Mac (this doesn't affect sim
   determinism because the call count is unchanged; replays that show the effect will draw
   different particles). CONFIRMED.

3. **L10 `FinishMission_10` guard (no-op).** The Mac (l.104999) tests `-1 < cur && cur < 4`
   (signed). The 68k `0xb5230` tests `cur < 4` unsigned, which is equivalent. The port uses a
   signed `cur < 4` (`level_10.rs:108-109`) with the comment "Signed compare as in the
   original". That comment is inaccurate, because the original excludes -1 either way. It
   can't be reached, because unlock only runs from inside a slot handler. Classification:
   comment nit. Recommendation: use `(0..4).contains(&cur)` or fix the comment. CONFIRMED.

4. **L01 M2 alarm `AddSound(0,0x30,1,100000000,16000,0x2e400,0x2b400,20000)`** (Mac l.110407).
   The port makes an identical call (`level_01.rs:409`), but the comment at
   `level_01.rs:407-408` ("the original's range 20000 has no equivalent in `add_sound`") is
   stale: the 8th parameter is passed as `z` (same nit as level00.md #2). Classification:
   doc nit. CONFIRMED.

There are also platform artefacts with no behavioural effect:

- **Arguments**: the Mac `Explosion_script` has 5 arguments (owner 0 is dropped by
  `Script::explosion`). `AddObjectNearObj(0x1f6,0x1f5,-12)` and
  `AddObjectNearPlayer(0x322,-10)` map to `ped_exits_vehicle(.., side -1)`. `ExplodeObject`
  maps to `actor_kill`.
- **Trigger args**: the Mac passes -1 where the port passes 0 for unused trigger args
  (PLAYER_DOWN, the `d` of ACTOR_GOTO). `apply_actor_setup` treats d ≤ 0 as 1000, so both
  behave the same.
- **AddGas 8th argument**: it is 1 in M200 and is still unmodelled (existing
  `TODO(api)` at `level_10.rs:568`).

## Checked and identical

### Level 01

- **Init (10129390)**:
  - Intro phrase 2.
  - Phones: 0→1/8, 1→100/9, 2→200/18, 6→600/20, 8→800/16 hidden.
  - MONEY_GOAL 2,000,000 and AddLives 2.
  - Four cones (1001) at the same spots.
  - Flatten squares 0x1a..0x1e × 0x3d..0x46 (cube 1). The Mac also uses counter B
    (cbd0) as the loop variable.
  - StartAllPhones then Announce 14.
  - Trap car 0x835 (model 2, colour 6) and T(0x834,0x2d,0x835).
  - Scenery (0x75,0x57,0,24) and T(0x836,0x2f) / T(0x898,0x2f,0x25,0,1).
- **Globals**: 0x835 → ExplodeObject. 0x834 → speaker 1, phrase 42, T(0x835,5,10). 0x836 →
  hint 51. 0x898 → hint 400. 0 is ignored. The unsigned `curSlot < 9` sends -1 to the phone
  answers.
- **FinishMission (10124620)**: when {0,1,2,6} are done, opens 3:300/6, 4:400/7, 5:500/19
  and 7:700/21, then StartAllPhones and curSlot = -1.
- **CloseScript (1013e230)**: Release 0x835, kill 0x834 and 0x835 (matches `shutdown`).
- **Phone answers**: all briefings, speakers, spawns, triggers and inits match, for
  1, 100, 200, 300 (apart from #1), 400, 500, 600, 700 and 800.
  - 400 is the 3-line dialog 31/32/33 with no `a` reset.
  - 700 resets b, c and active[0..3]; 300 resets a, b and c.
- **M0 (table 101912d4)**:
  - 2/3: one-shot `a`, kill the other trigger.
  - BlowUpGarage: grid 2..3 × 10..12 with `random(2)`, then z (0x5dc), y, x RNG order, size
    3000. Cube wipe, five top faces and the four 0xda faces, phrase 3, killer (-9) at
    (0x2400,0x6c00).
  - Route 4..10 including 42000 = 0xa410, vanish at (0xa410,0x12400).
  - Endings 11/0x1e/0x32: phrases 5/4/1, 100000.
- **M1**: 4× COP_KILLED, 180 s, phrase 7+a, 200000 at 4, phrases 12/13.
- **M2**: 11 window faces (W 0x1da / S 0x1db), loot point and 60 s garage (12,116,1,3), the
  alarm one-shot with wanted 5, sound and scenery 22→21 restore when `a==1`,
  phrases 16/17/19/20/18, 150000.
- **M3**: 0x137..0x139 one-shot `a` and kill 137/138/139/168, then VEHICLE_CHANGED 0x168.
  Re-arm by active flag. The in-sea path (c++, b++, phrase 23) and wreck path (b++,
  phrase 25/26). Finish 24, c×75000. Timeout 28 with partial pay and a failure.
- **M4 (table 10191200)**: briefcase drop (0x14400,0x1fc00), park point (0x29df4,0xa400),
  checkpoint marker on/off, scenery 23/0x59 and 23/0xb2 and their clears. Courier (-10) and
  all six legs. Vanish (0x2dc00,0x445c), 200000, failures 34/37/38/40.
- **M5 (table 10191138)**: all 12 patrol legs. Attack → driver -12 out of the pickup,
  aggression 0x50, ACTOR_GONE/PLAYER_DOWN. 300000 / 44.
- **M6**: van (14, 0xb40, colour 7), garage (0x6d,0x25,1,2), 55 s. ExplodeObject on
  0x280/0x281 and Release elsewhere, phrases 49/48/50/29/30, scenery clear, hint 59.
- **M7**:
  - Spawns -12 with angles 0x5a0/0x10e0, and the trigger types 0x31/0x30/9/0x28/0x28/9.
  - Kill bookkeeping: 50000 + 53/54, the fourth gives 100000 + 55 + complete.
  - At b==3, ACTOR_LOST 0x2da and Flee on the survivors.
  - The alert (aggression 0x46 once, every event 0x2bd..0x2d4).
  - 0x2da/0x2ee announce *complete* (0x7e6) with phrases 56/57, the original quirk kept.
  - Hint 58.
- **M8**:
  - Wall breach 0x32..0x35 × 0x68..0x69, 40 s.
  - NEAR_POINT (0x19000,0x34800), prisoner -10 at (110000,0x34800).
  - Boarding and unloading counters with wanted 5 on each boarding.
  - Safe house AT_POINT (0x1c000,0x8000) and vanish (0x1cad4,0x7c00).
  - Phase handling in 0x325/0x326 (re-arm order 0x328 then 0x324; 0x329 then 0x32a).
  - Phrases 72-80, 0x56-alive, alive×30000, then SetWanted(3) after Finish.

### Level 10

- **Init (10129650)**: intro 0. Phones 0→1/1, 1→100/2, 2→200/3, 3→300/4. Goal 2,000,000,
  lives 1, StartAllPhones then Announce 1. Helicopter (1, model 17, colour 1). Six IN_RECT
  triggers on R1 (0x1000,0x34000,0x9800,0x3d800) and R2 (0x1a800,0x36800,0x1e000,0x38800),
  floors 0/1/2.
- **Ambience (100000..100003, 100010..100013)**: SetHighestCube 1/2/3/3, the kill set and
  re-add set and OUT_OF_RECT, exactly as `ambience()`.
- **FinishMission (10124720)**: opens 4:400/5, 5:500/6, 6:600/7 and 7:700/8.
- **Phone answers** (the unsigned `curSlot > 7` covers -1):
  - Race car 10 (18, 0x10e0, colour 1, not driveable).
  - M100: -12/-11/-11 at the same spots, TIMER 20, SEES_PLAYER, count = 0, pay = 3.
  - M200: truck 19 at (0x18400,0x15400,0x2d0), gas = 0.
  - M300: -10/-12/-12, NEAR_ACTOR, count = 3.
  - M400: -16/-12/-12 with angles 5000/0x708/2000, TIMER 30, both henchmen alive.
  - M500: 9/20/9 at y = 0x2dc00, DoHijack(-1).
  - M600: -12/-10, DoSeekDestroy(-1).
  - M700: -10/-12/-12 at z = 1 and object 0x3eb, AT_POINT (0x8800,0x38c00), count = 2.
- **Race (table 101915d4 + DoRace)**:
  - Grid 11..15 with colours 2..6 and aggression 100/100/99/98/95, lights 0→1→2→3 (5 s/1 s/1 s).
  - Phrase 4, driveable, DoRace(-1), lap HUD.
  - Failure 10 → 11. Events 0, 6..9 and ≥ 11 (including -1, through the unsigned `< 0xb`)
    go to DoRace.
  - DoRace tables (AI_X, AI_Y, AI_SP, CP_TYPE 0x36,0x34,0x36,0x33,0x35,0x34,0x35,0x33 and
    CP_VAL) all equal the port's constants.
  - Waypoints wrap at 14, 8 checkpoints per lap, phrase 10+lap, placing n → phrase 5+n,
    (3-n)×85000 when n < 3, re-arm only when not finished and curSlot ≠ -1.
- **M100 (table 1019148c)**: all 18 route links, including 190000 = 0x2e630 and the
  vanish legs with d = 1. Meetings 0x71/0x74 (count 1/2, phrases 21/22). Contact deaths
  (pay 1, or 2 - (count == 0), phrase 17 or 18/19, kill 0x6d..0x79, Flee, ACTOR_LOST 0x69).
  Spotted 0x8c (kill 0xad/0xb6, aggression 0x50, phrase 24, pay 1 if count < 2). Contacts
  leaving (phrase 26). Endings 15/16 (pay×85000), 20, 23, 25.
- **M200 (table 101913c4)**:
  - Wanted 6, garage (0x4f,0x43,1,1), 3 s, east face open and close (0x76, flip 1).
  - Hiss sound (1,0x3a,4000000,0x206c,0x27cb8,0x21c00,20000).
  - Tick triggers 0xcc/0xcd/0xe6, poison phrase 35 once with -1 HP.
  - Mutants walk to (170000,0x20f58) and (170000,0x226c8) with `random(2)+2` timers in the
    same order. Aggression 0x3c/-1 alternation.
  - Count 2 → 34 / 250000, otherwise 33. Failure 28. The gas flag is never cleared.
- **M300**: speaker 0 phrase 38, 2 s, aggression -1/0x46/0x46, count → 39/250000 or 41/40,
  failure 37.
- **M400**: stop 0x1c4 and Kyle's target choice (0x5a), henchmen 0x3c → Kyle, survivor
  re-aim 0x3c/-1, phrase 44, 45/250000, failures 46/47/48.
- **M500 (DoHijack)**: route tables equal M500_X/Y. Station door north face (0x4d,0x25)
  opens at counter 12 for the van. Escort end points (0x25c00/0x27400, 0x11c00). Van at 13
  → phrase 53. Hijack 0x1fe (wanted 6, phrase 51, garage), 52/250000, 50.
- **M600 (DoSeekDestroy)**: route tables equal M600_X/Y, tail = [0,1]. The tail advances
  only while behind Ike. Ike vanishes at (0x14400,0xab20) at 11 and fails 58 at 12.
  0x280 aggression 0x5a, phrase 59. 225000 = 0x36ee8 with phrase 56. Phrases 55/57.
- **M700**: phrases 61-68, IN_RECT floor 1 (0x6000,0x38000,0x9000,0x38800), door object
  ACTOR_KILLED → wall faces N(0x11,0x70) / S(0x11,0x6f) at cube 2, PLAYER_Y_LE 0x36000,
  aggression 0x46. Deaths are counted only after 0x2be. 250000; failures 64/66.
