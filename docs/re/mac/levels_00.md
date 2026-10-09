# Level 00: Mac OS X script vs our port

Sources compared:

- Mac (`analysis/ghidra_macx/decompiled.c`): `InitScript_00__Fv` @ 101291c0 (l.107128),
  `ProcessTrigger_00__Fi` @ 1012bae0 (l.108114-110173), `FinishMission_00__Fv` @ 10124490
  (l.104902). Switch cases were mapped to events with `jumptables.tsv`. The outer table at
  101910e8 is indexed by curSlot, and the inner tables are 10190fa8 (M1, base 0x65),
  10190ed8 (M10, 0x3e9), 10190d98 (M9, 0x385), 10190c58 (M6, 0x259), 10190b94 (M0, 0x1e)
  and 10190b74 (M3, 0x12e).
- Port: `crates/payback/src/mission/levels/level_00.rs`.
- Amiga: 68k 0xb91dc / 0xb9b6a / 0xb506e as documented in `docs/re/missions_00.md`.

The level-00 handler doesn't call `DoBus`, `DoHijack`, `DoSeekDestroy` or `DoRCBomb`. Every
mission body is inline in `ProcessTrigger_00`.

## Findings (differences)

None in behaviour. Every case body matches the port in these respects: event ids, trigger
types and arguments, spawn ids, models, positions, angles and colours, phrases and speakers,
money, timers, wanted level, map edits, flag and counter logic, and call order (including the
M5 RNG order). The Mac code is the same as the Amiga source as documented, so the port has no
bugs and the Mac has no changes here. The only items are a no-op argument and a stale comment:

1. **M7 phone (event 700), `T(0x2ee, 6, 0x2ee)`**. Mac l.110126 passes `a = 0x2ee` to the
   type-6 (PLAYER_DOWN) trigger. The port passes -1 (`level_00.rs:355`). The 68k does the same
   as the Mac (`FUN_000b4f5a(0x2ee,6,0x2ee,...)`, ghidra_68k l.96370), so this is an
   original-source quirk. PLAYER_DOWN ignores its arguments (`eval.rs:289`), so nothing changes.
   Classification: no-op, not a bug. Recommendation: optionally pass `&[0x2ee]` for fidelity.
   CONFIRMED.
2. **M0 0x3e, `AddSound_script(0,0x30,1,100000000,16000,0x2a800,0x26000,20000)`** (Mac case
   1012ffc4, l.109766). The port makes the same call (`level_00.rs:1134`), but the comment
   above it ("TODO(api): `add_sound` has no range argument (20000)") is stale.
   `Script::add_sound` takes it as `z` (api.rs ~1050). Classification: doc nit. Recommendation:
   delete the TODO. CONFIRMED.

There are also platform artefacts with no behavioural effect. The Mac `Explosion_script` has 5
arguments (x, y, z, owner = 0, size = 3000), while `Script::explosion` drops the owner. The Mac
`AddObjectNearObj(0x3ec,0x3e9,-10)` and `AddObjectNearPlayer(0x32/0x33, -10/-11)` correspond to
`ped_exits_vehicle(.., side -1)`.

## Checked and identical

- **Init (101291c0)**: intro phrase 106. Phones: slot 1→100/phone 2, slot 9→900/3, slot 7→700/1
  hidden, slot 19→0x76c/10 hidden. `T(-1,0,1500000)`, `T(0x898,0x2f,0x25,0,1)`, AddLives 2,
  four cones (model 1001) at the same positions, StartAllPhones, Announce 7.
- **FinishMission (10124490)**: consume slot, then the three waves:
  - {1,9} → 4:400/4 and 2:200/5
  - {4,2} → 11:1100/11, 3:300/12 and 6:600/13
  - {11,3,6} → 5:500/14, 10:1000/17 and 0:20/15

  Then StartAllPhones and curSlot = -1.
- **Dispatch**: 0x898 → Hint(400). 0 is ignored. Slots 8 and 12-18, and -1 (the unsigned
  compare `< 0x14`), go to the phone answers.
- **Phone answers** (20, 100, 200, 300, 400, 500, 600, 700, 900, 1000, 1100, 1900; l.110008-110170):
  all briefings, spawns, triggers and inits match. This includes `invalid=0` (M1), `count=0` and
  forced taxi (M4), `flag_b=count=0` (M7), and the KKK init (alive x5, left=5, count=0, flag_a=0).
- **M1** (inner table 10190fa8, events 0x65-0x68, 0x6f-0x76, 0x7c-0x7e, 0x96, 0xb4): crates,
  dummies (model 0x3ea, angles 0x5a0/0xb40, special 5/3), line triggers 0x1e/0x1f/0x20/0x21, the
  10th shotgun pull and the 6th throw, the kept quirks (0x68 not killed, `invalid` not reset
  after the pull limit), +20000 per dummy, +25000 only when the shotgun count is 3, and the
  failure order.
- **M2** (case 1012ec80): 25 s countdown, garage (3,11,1,1), raw `KillTrigger(0xca)` on 0xfb,
  speaker 1 for 17 and 18, 100000.
- **M3** (case 10130904): 50 s, garage (12,116,1,3); on re-entry (0x135) it re-arms
  (3,11,1,1), the original bug, which the port keeps. 125000.
- **M4** (case 1012ea68): RequestVehicle(13/-1) at the same places, two deliveries, speaker 1
  for 37/38, 75000.
- **M5** (case 1012eee4): parking spot (0xc400,0x21400), ACTOR_OUTSIDE 0x1fa, 10 s fuse,
  ExplodeObject. The explosion grid is x 26..30 by y 61..70 with `random(2)`, then
  `random(0x5dc)` → z+17000, then `random(0x800)` → y, then `random(0x800)` → x, the same RNG
  order as the port. Size 3000, the cube wipe (1, 0x200, 0x200), phrases 44/43, 100000.
- **M6** (inner table 10190c58): all 14 walk legs (31000 = 0x7918, 40000 = 0x9c40,
  38000 = 0x9470, 120000 = 0x1d4c0), the last leg type 0x12, failure phrases 2/54/55/56,
  MakeDrone, Info (20,39) cleared, Hint 71.
- **M7** (case 1012e694): aggression values 0x1e/0x3c/0x28, garage (22,43,1,0), phrases 64-68,
  59+count, +25000 at 2 kills, 150000 with speaker 1.
- **M9** (inner table 10190d98): all 16 checkpoints (88000 = 0x157c0), 110 s countdown, 80 s
  timer, `StopTrigger(0x3ac)` no-op, retry re-arm list, 125000/100000, Hint 70.
- **M10** (inner table 10190ed8): gunmen (-12, 0x10e0), all 17 route legs and their order,
  ambush at 0x3ee when count < 2, the stop/go watchdog 0x413-0x417, boss exit and walk
  (0x25c00,0x274e8), failure phrases 19/92/95, 125000.
- **M11** (case 1012c980): Flee loop with SetAggressiveness(-1,-1) and a one-shot 0x19 arm,
  kill/escape bookkeeping, phrases 112-left / 105 / 99+left, (5-left)×20000, failure when none
  are killed.
- **M19** (case 1012f34c): Remove tank on entry, Announce 2029, 60 s, MONEY_GAIN 15000, 50000,
  phrases 21/22/18/19/23.
- **M0** (inner table 10190b94, all 32 cases): Ron and Trey (-10/-11), getaway id 5000, the
  flag mapping (cb94 = flag_c, cb98 = flag_b, cb9c = flag_a), every re-arm list, the alarm cubes
  (85,74/77, 22 → 21), sound slot 0, wanted level 5 then 0, garage (3,11,1,1), 50 s countdown
  and 40 s timer, 150000. The fail paths 0x4c/0x4d don't restore the alarm, the same as the
  port.
