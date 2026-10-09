# Mac OS X build vs Amiga: gameplay comparison

How the Mac OS X build of Payback compares with the Amiga builds on gameplay. The Mac code is
`Payback (OS X)`, decompiled with its original CodeWarrior names in `analysis/ghidra_macx`. The Amiga
references are the 68k Update 7 build (`analysis/ghidra_68k`) and the OS4 build (`analysis/ghidra_os4`,
`analysis/ghidra_os4/long_timeout.c`).

**The Mac release is treated as MORE DEFINITIVE.** Where it differs from the Amiga, the recommendation is
to ADOPT the Mac behaviour, unless the difference is clearly a Mac-platform artefact. Gaps between our port
and the original, in which the Mac and Amiga agree, are listed separately in §11, "Port fixes". Those
apply whatever the build.

Confidence: **CONFIRMED** means seen in the Mac decompile and in at least one Amiga decompile.
**INFERRED** means reasoned from the code but not traced end to end or run.

The report is in two files:
- this file covers §0–§5: summary, timing/FPS, RNG, parameter tables, vehicles, people/boarding/collisions;
- [`gameplay_part2.md`](gameplay_part2.md) covers §6–§12: missions, combat, bug fixes, kept quirks, note
  corrections, port fixes, open items.

Detailed per-area notes (code excerpts, line references):
- `gameplay_vehicles.md`: vehicle physics and vehicle AI.
- `gameplay_combat.md`: combat, police, scoring, and the Mac struct-offset map.
- `gameplay_missions.md`: trigger engine, script API, level scripts, Random missions.
- `scratch_gameplay/levelloop.c`, `startlevel.c`, `initstructures.c`: function extracts.
- `scratch_gameplay/cmp_init.py`: field-by-field diff of the parameter tables (`InitStructures` vs OS4
  `FUN_0104e13c`).
- `tools/re/fn.py <macx|68k|os4> <Name|addr>`: prints one function from a decompile.

The boarding and collision results in §5 come from sub-agent reports that were relayed in condensed form.

---

## 0. Summary

**Overall.** The Mac game code is a straight port of Update 7, with the same structure and almost all of
the same constants. The C++ names make it a good key for naming the Amiga functions.

**Gameplay changes in the Mac build** (all to be adopted):

| # | Area | Change | Kind | Confidence |
|---|---|---|---|---|
| T1 | Game loop | Tick is **30 Hz**: TickCount (60 Hz) halved. Same as OS4; the 68k uses 25 Hz. | platform / intended rate | CONFIRMED |
| T2 | Game loop | The level-end fade counter is 150 ticks (5 s); Amiga 100 ticks. | balance / timing | CONFIRMED |
| V1 | Police AI | Pursuit route-node arrival radius is `300 + speed²>>7`; Amiga `300 + abs(5·speed)`. | balance | CONFIRMED |
| V2 | Traffic AI | Junction look-ahead is `max(speed·1000/yaw, front)`; Amiga `5·speed²/yaw`. | balance / fix | CONFIRMED |
| V3 | Vehicle AI | Low-skill anti-spin no longer zeroes steering while braking. | behaviour fix | CONFIRMED |
| V4 | Traffic | New traffic cars spawn with `speed = min(maxF, 100)`. | new behaviour | CONFIRMED |
| V5 | Tables | Tank max reverse is −220; Amiga −100. | balance | CONFIRMED |
| V6 | Collision | Map edge clamps the vehicle probes **and** counts as a wall hit on the normal (last) sub-step. | bug fix | CONFIRMED |
| V7 | Vehicle | The police light bar resets even if the siren never got a channel. | robustness | CONFIRMED |
| C1 | Collision | A destroyed remote bomb detonates (50000) and clears its owner's bomb slot. | bug fix | CONFIRMED |
| M1 | Missions | Level 21: a new "car 50 destroyed" trigger turns the old soft-lock into a failed mission 1. | bug fix | CONFIRMED |
| M2 | Triggers | Actor triggers store the script id and look up the actor every frame; Amiga stores a pointer. | robustness / redesign | CONFIRMED |
| M3 | Triggers / Rampage | Triggers run in Rampage when the map has a phone. | new behaviour | CONFIRMED |
| M4 | Rampage | **Random missions**: seven generated phone jobs in Rampage. | new feature | CONFIRMED (code) |
| P1 | People | Pavement peds made by the traffic spawner store their spawn point (+0x7d4/+0x7d8). | new behaviour (purpose unknown) | CONFIRMED write / INFERRED use |

**Unchanged:**
- the random number generator and its table;
- combat, weapons, explosions, police and wanted rules, scoring;
- every other handling, mass and spawn-weight value;
- the trigger types and the script API semantics;
- all six story level scripts, apart from M1.

**Documented Amiga quirks the Mac did NOT fix** (keep them): part 2, §9.

**FPS answer:** no. A higher frame rate was not supported. See §1.

---

## 1. Game loop, timing and frame rate

### 1.1 The Mac frame loop (CONFIRMED)

Mac `amiga_levelloop` @10100f50 (extract: `scratch_gameplay/levelloop.c`) is a direct port of the 68k
main loop `FUN_000ec686`. Per frame it runs:
1. input;
2. `AddVehicles(ticks)`;
3. `RespawnWeapons`;
4. `DoCollideStuff`;
5. the lightning flash decay, ×3/4 per tick;
6. `CheckForCollisions` for objects 1..N;
7. `MainLoop` @1014a920, which calls `object_MainLoop` @1014a670 for each object: move
   (`MoveObject_General` / `MoveObject_Vehicle`), then `do_Person`, `do_Other` or `do_Weapon` by model
   class, then re-insert into the collision grid;
8. respawn and lives;
9. the fade counter;
10. `ProcessTrigger(0)` and `CheckForTriggers`;
11. `DrawView` (**one render per frame**);
12. the multiplayer match end;
13. the frame timing.

The frame timing at the end of the loop:

```c
*ticks_raw = (TickCount() - last + 1) >> 1;          // 60 Hz Mac ticks, halved -> 30 Hz game tick
do { now = TickCount(); } while (now < last + ticks * 2);   // limiter: >= 2 Mac ticks per game tick
if (!cd_audio) UpdateAudio(ticks * 0x2df - 1);       // 735 samples per tick = 22050 Hz / 30
ticks = clamp(ticks_raw, 1, 5);
if (!registered && (demo_ticks += ticks) > 9000) "Time out!" (0x9ba), fade 0x96;
last = TickCount();
```

68k `FUN_000ec686` (decompile lines 127905–127938) has the same code but reads the DOS `DateStamp` ticks
(50 Hz):

```c
DAT_0034c41c = (FUN_00110e28() - DAT_0034c438) + 1U >> 1;
do { uVar18 = FUN_00110e28(); } while (uVar18 < DAT_00348090 * 2 + DAT_0034c438);
... FUN_00016d38(DAT_00348090 * 0x2df - 1);   // same 735 samples per tick
DAT_00348090 = clamp(DAT_0034c41c, 1, 5);
```

| Build | Clock | Game tick | Frame cap | Ticks per frame |
|---|---|---|---|---|
| 68k Update 7 | DateStamp 50 Hz, halved | 25 Hz | 25 fps | clamp 1..5 |
| OS4 | 50 Hz timer ×60/50, halved (`FUN_0110761c`) | 30 Hz | 30 fps | clamp 1..5 |
| **Mac** | `TickCount` 60 Hz, halved | **30 Hz** | **30 fps** | clamp 1..5 |

### 1.2 Was a higher FPS supported? No (CONFIRMED)

- The loop is a variable timestep counted in whole 1/30 s ticks. Each frame runs exactly one simulation
  pass, scaled by `ticks` (1..5), and one `DrawView`. Nothing is sub-stepped on the frame level beyond
  what the object code does internally with `ticks`.
- The busy-wait guarantees at least 2 Mac ticks (1/30 s) per game tick, so in-game rendering is capped
  at **30 fps**.
- There is no sub-tick timer: the game loop calls no `Microseconds` and no `UpTime` (the only
  `TickCount` users are the loop, `amiga_startlevel`, `ch2pl`, the scroller and the About box). It does
  not interpolate between ticks. There is no vsync setting (`aglSetInteger` swap interval is never
  called) and no frame-rate preference (`CPaybackPreferences::ClearPreference` @100508c0 has none).
- A slow machine gets up to 5 ticks per frame, i.e. down to 6 fps before the game itself slows down.
- Menus pace on `ch2pl` @10053320 (`while (TickCount() < next); next = TickCount() + 1`): at least one
  Mac tick per frame, so 60 fps. In-game it is the 30 fps above.
- `amiga_startlevel` @100fff40 seeds `last = TickCount() - 1` and `ticks = 1`, as the 68k does.
- **The audio mixer length suggests 30 Hz is the design rate.** 735 = 22050/30, and the 68k code uses
  the same 735 per tick. So 30 Hz is the native rate, and the 68k's 25 Hz is an artefact of the Amiga's
  50 Hz DateStamp (INFERRED).

### 1.3 Other loop details

- **T2. The fade counter is now 150 ticks** (CONFIRMED). The Mac sets it to `0x96` on game over (lives 0,
  phrase 0x7ea), on the multiplayer match end, and on the demo time-out. The 68k sets 100 (lines 127702,
  127851, 127934). OS4 also sets 100 (decompile 49677, 49838).
  - That is 4 s on the 68k, 3.3 s on OS4 and 5 s on the Mac.
  - Classification: timing tweak.
  - Port: `crates/payback/src/frontend/mod.rs` `END_DELAY = 4.0` becomes 5.0 s (150 ticks at 30 Hz).
- **Identical** (Mac `amiga_levelloop` / `MainLoop` vs 68k `FUN_000ec686` / the tail of `FUN_000e3a58`
  ~125890–125966):
  - lightning (random(1000) < ticks; 2/4/6 flashes with sound 0x3e);
  - the wanted decay every `aggr·100 + 150` ticks;
  - health regeneration +1 per 250 ticks for hp 1..99;
  - the traffic-light phase every 61 ticks over 6 phases;
  - the sky rotations (+60/+30/−45 per tick mod 0x1680);
  - the respawn sequence (`PlRestart`, new player object, state 2, x/y/heading reset, score restore);
  - the multiplayer win/draw/lose.
- One trigger-gating change is described in part 2, §6 (M3).

### 1.4 Impact on our port

- `crates/payback/src/main.rs` runs `Time::<Fixed>::from_hz(25.0)`.
- `camera.rs` (`delta_secs()*25.0`) and `replay.rs` ("fixed 25 Hz") also assume 25 Hz.
- `docs/re/people.md` §1.1 tells the port to use 25 Hz.

**Recommendation:** with the Mac as the reference, switch the fixed tick to **30 Hz**:
- the fixed timestep;
- camera smoothing;
- replay tick units (the original replays record 30 Hz ticks in bits 2..0);
- `END_DELAY` (150 ticks);
- any per-second conversions in the docs.

Notes:
- All per-tick rules stay the same: they are per tick, and the Mac uses identical per-tick constants.
- The game will run 20% faster in real time than the 68k build, which is what Mac and OS4 players
  experienced.
- Keep one sim step per tick. The port may render faster than 30 fps with interpolation, but that would
  be a new feature, not original behaviour.
- The replay golden files must be re-recorded.

---

## 2. Random number generator (CONFIRMED: unchanged)

| Mac | 68k | Our port |
|---|---|---|
| `random(n)` @100834e0: `n * T[++i & 0xffff] >> 9`, counter `_DAT_1017c6f4` (u16) | `FUN_0003fc8c` (`DAT_0023a080`) | `sim/mod.rs` `Rng::rand` |
| `random3(n)` @10083530: the same table with a second counter `_DAT_1017c6f0` | `FUN_0003fce6` (`DAT_0023a084`) | **not present** |
| `RandomFromSeed(s, n)` @10083580: `n * T[s] >> 9` (absolute index) | (68k has the relative peek `FUN_0003fcb8`, `T[i+s]`) | `Rng::hash` (OS4 `FUN_0103ad20`) |
| `GetSeed`/`SetSeed` @100835c0/@100835f0: read/write the main counter only | `FUN_0003fd12` | `Rng::index`/`seed` |
| `InitRandom` @100833b0: zeroes both counters; loads `General/RandomNumbers` (65536 BE shorts), or regenerates `rand()>>6` and saves it | same | same |

- The shipped `General/RandomNumbers` is byte-identical in the Mac and Amiga data (`cmp`).
- **`random3` is a cosmetic stream.**
  - Callers: `DrawMap` (71 calls), `UpdateEffects` (64), `UpdateEffectsMulti1` (36),
    `amiga_startlevel` (12), `Split` (9), `DisplayTextWibble` (8), `Bang` (7), front-end screens, and
    `AddVehicles` (2).
  - Gameplay code uses `random` (top users: `CheckForCollisions` 113, `do_Person` 92, `AddCollideObj`
    64, `do_Weapon` 61, `DoVehicleFire` 48).
  - The 68k already has the same split, so this is not a Mac change.
  - **Port:** effects, rendering and HUD wobble should draw from a second `Rng` index and not from the
    gameplay `Rng`. Otherwise effect randomness shifts the gameplay sequence away from the original's.
    The two `random3` calls in `AddVehicles` need checking before splitting (INFERRED: cosmetic).
  - This is a port fix (§11) and matters for replay determinism against original replays.

---

## 3. Parameter tables

### 3.1 Vehicle handling, people and scenery models (`InitStructures` @1005ffb0 vs OS4 `FUN_0104e13c` / 68k `FUN_000d7874`)

Method:
- `scratch_gameplay/cmp_init.py` matched all 115 model loads, in the same order in both builds.
- It diffed every field written, with Mac model offset = OS4 + 0x385c.
- Mismatches that were only byte-pair or u16-pair writes (e.g. OS4 writing `0x4908..0x4909` as one u16),
  or the same expressions written differently, were resolved by hand.

- **V5. Tank max reverse speed: −220 on the Mac, −100 on OS4 and 68k.**
  ```c
  // Mac InitStructures
  *(undefined4 *)(piVar16[0xb] + 0x8138) = 0xffffff24;   // -220
  // OS4 FUN_0104e13c / 68k FUN_000d7874
  *(undefined4 *)(DAT_012a6390 + 0x48dc) = 0xffffff9c;   // -100
  ```
  - Classification: balance change. CONFIRMED.
  - Port: `crates/payback/src/vehicles.rs` tank row `[220, -100, ...]` becomes `[220, -220, ...]`.
    Also update `docs/re/vehicles.md` §1.3.
- **Identical:**
  - the other 27 types plus the police and police-van variants: maxF, maxR, accel, brake, yaw, grip,
    mass, spawn weight, engine/horn/reverse sounds, radio, home city, names, door offsets and doors;
  - the `policevan_red` typo (brake 10, grip 20, mass 2000) is kept;
  - helicopter radius /3, people and weapon pose codes (`+0x4908`), scenery pickup codes.
- Combat cross-check: all 66 masses and all 30 traffic weights are identical (`combat.md` §3).

### 3.2 Weapons, people constants

- **Weapons:** identical in all builds (`combat.md` §3):
  - damage numerators: minigun 10000/T, pistol 5000/T, pellet 20000/T;
  - filters, spreads, recoils, fire timers, ammo per pickup (4/75/6/100/20/30/1), the 1200-tick
    respawn, explosion powers.
- **People:** of `do_Person`, the firing and AI-combat parts were compared and are identical. The
  movement and animation constants (speed caps, turning, gravity, swimming) remain to be compared line
  by line (open item, part 2 §12).

### 3.3 Map layout (platform artefact)

- The Mac map cube is 0x78 bytes (row 0x3c00); the 68k cube is 0x72 bytes (row 0x3900). Height words
  are at +2/+0x2a/+0x52 on the Mac and +2/+0x28/+0x4e on the 68k (`StartObject` @100adb70 vs 68k
  `FUN_00043c96`). Object instances are also larger (Mac = 68k + 0x4e2..0x4ec; models + 0x385c).
- Structure padding only; no behaviour change.

---

## 4. Vehicle physics and vehicle AI

(Detail: `gameplay_vehicles.md`.)

Function map:
- `MoveUnit` @10081080 = 68k `FUN_000a143e` / OS4 `FUN_010cb7e4` (lane and junction choice).
- `PositionUnit` @10081450 = `FUN_000a1758` / `FUN_010cbc7c` (slope).
- `SkidUnit` @10081830 = `FUN_000a197a` / `FUN_010cc288` (grip and skids).
- `PositionAirborneUnit` @10082620 = `FUN_000a241c` / `FUN_010ccc5c`.
- `BounceUnit` @10082bc0 = `FUN_000a27c6` / `FUN_010cd198`.
- `MoveObject_Vehicle` @1006e890 = `FUN_000e109e` / `FUN_0105b9ec` (sub-steps).
- The vehicle part of `do_Other` @10146280 = `FUN_000e3a58` / `FUN_0105f594`.

These correct `docs/re/vehicles.md` §0, which has several of the 68k addresses wrong.

### V1. Pursuit route-node arrival radius (balance), CONFIRMED

- Mac `do_Other`: `r = (speed*speed >> 7) + 300` for pursuit states 8/10.
- 68k ~124245 / OS4 main.c ~800: `r = abs(speed*5) + 300`.
- Flee (state 0x14) keeps the Amiga formula on the Mac.
- Radii at speed 100/300/600: Amiga 800/1800/3300; Mac 378/1003/3112.
- **Adopt:** in `sim/vehicle_ai.rs` `follow_route`, use the Mac radius for pursuit (callers
  `sim/pursuit.rs` ~228) and keep `300 + abs(5v)` for flee (`vehicle_ai.rs` ~449).

### V2. Traffic junction look-ahead (balance / fix), CONFIRMED

- Mac `MoveUnit`: `r = speed*1000/yaw; if (r < front) r = front;`
- 68k / OS4: `r = speed*speed*5/yaw` with the same floor.
- The two agree at speed 200. Fast traffic no longer re-targets five or more squares ahead.
- **Adopt:** in `sim/traffic.rs` `drive_traffic`, use `(speed*1000/turn).max(front)`. The floor is the
  model's front extent (`+0x48c8`), not 512.

### V3. Low-skill anti-spin rule (behaviour fix), CONFIRMED

- Mac `do_Other`: `if (skill < 0x51) { if (jturn_latch) steer = 0; }`
- 68k ~124855 / OS4 1138: `if (jturn_latch || throttle == 1) steer = 0;`
- Low-skill drivers can now steer while braking or reversing, so they no longer back out straight or
  miss turns while braking.
- **Adopt:** in `sim/vehicle_ai.rs` `controller`, drop `|| throttle == 1`.

### V4. Traffic spawns moving (new), CONFIRMED

- Mac `AddThing` @10058350 sets the new car's `+0x890` speed to `min(maxF, 100)`. The Amiga leaves it 0.
  Velocity is not set; grip brings it up.
- **Adopt:** in the `sim/traffic.rs` spawn loop, set `o.speed = max_speed.min(100)`.

### V6. Map edge counts as a wall on the full-box sub-step (bug fix), CONFIRMED

The same finding came from the collision sub-agent as D1.

- Mac `CheckForCubeCollision` @100af980 vehicle 3×3 path: each probe is clamped to `[0, 0x3ffff]` with
  hitX/hitY = ±1:
  ```c
  if (px < 0) { hitX = 1; px = 0; }   if (0x3ffff < px) { hitX = -1; px = 0x3ffff; }   // same for y
  ```
- 68k `FUN_00044f82` / OS4 `FUN_01070868` clamp only in the 2×2 path (earlier sub-steps). At 1 tick per
  frame the 3×3 path is the only one that runs, so on the Amiga the map edge never bounces a car
  (effect INFERRED).
- **Adopt:** in `sim/collide.rs` `resolve_walls`, report a wall hit when the clamp moves a coordinate,
  so `driving.rs` applies `BounceUnit` damage and sound and the heading deflection (N7).

### V7. Police light bar reset (robustness), CONFIRMED

- The Mac resets the red/blue model whenever the horn is off. The Amiga does it only if a siren sound
  handle exists.
- Port: reset the model on horn-off and don't tie it to the sound entity. Low priority.

### Mac-only helpers

- `GetRandomVehicleNum` @10057e50 and `GetRandomVehicleNum_Pursue` @100580c0 are not called by any
  story or engine code. They are used by the Random missions (part 2, §6 M4) through the jump-table
  code that the current export missed (`missions.md` §0/§3).
- They pick `random(26)` (25 becomes 27), skip the RC car and the boat (`_Pursue` also skips police,
  tank, helicopter), and gate the tank, helicopter, rocket car, RC truck, Pug Racer and Vapour GT by
  `GetHighestLevelCompleted`.

### Checked and identical

- Throttle, brake and coast ramps.
- The damage top-speed cap `clamp(H*max/50, max/4, max)`.
- The J-turn (±2·yaw) and `UseHandbrake`.
- Steering ramp and caps, the 45° snap.
- Lean and dip.
- Engine pitch and damaged-engine samples.
- Landing (vz > 0xa0) and terrain follow.
- Splash, sinking and the "Drowned!" sequence.
- Helicopter lift 0xaa and gravity 0x20.
- `BounceUnit` damage `v²>>14`, Thud/Thud2, bus scream.
- `SkidUnit`: the surface grip table, slip after correction, skid marks and sound.
- `PositionUnit`, `PositionAirborneUnit`.
- The driver AI: states, police entry (wanted ≥ 3, skill wanted+0x5c/0x5e), the ±1500 aim clamp,
  look-ahead and occupancy, traffic lights, impatience/honk, the arrest drop-off (8000, slow or hp < 50),
  the cruise band table.
- `AddVehicles`: cap `min(level+11, 20)`, 2·ticks attempts, ring size, roadblocks at wanted > 4.

---

## 5. People, boarding and collisions

(The boarding and collision results below are condensed from sub-agent reports. `do_Person` movement and
animation were not compared line by line; see part 2, §12.)

### 5.1 Mac differences

- **C1 (D2). A destroyed remote bomb detonates in full (bug fix), CONFIRMED.**
  - In the Mac death/despawn pass (`DoCollideStuff` @10067b50, = 68k `FUN_000dc5ac` / OS4
    `FUN_010542b8`), a placed remote bomb that is destroyed detonates at power 50000 and clears its
    owner's placed-bomb slot (`+0x860`).
  - On the Amiga the owner's slot is left set (stale bomb reference).
  - **Adopt:** in the port's death pass (`sim/combat.rs` destroy/detonate, `sim/people.rs` bomb slot),
    detonate at 50000 and clear the owner's bomb slot.
- **V6 / D1. The map-edge probe clamp** (see §4). Adopt.
- **D3. New helpers.**
  - `GetCollideInfo_alt` @100aea60, `LineOfSight_alt` @100af8d0 and `GetSafePos` @100b42b0 are new.
  - `GetSafePos` serves the Random missions: up to 5000 random squares 1..126 with a drivable ground
    cube, empty cubes 1–2, and line of sight to the nearest road node.
  - The `_alt` pair has no story caller (INFERRED: used by `GetSafePos`).
  - Port: needed only with M4.
- **P1. Pavement peds store their spawn point.**
  - Mac `AddThing` person branch writes `+0x7d4/+0x7d8 = x/y` (OS4 +0x2f4/+0x2f8). The Amiga does not.
  - The reader was not traced (INFERRED: a home point or despawn reference).
  - Port: no action until the reader is found.

### 5.2 Checked and identical

- **Boarding.** No behavioural Mac differences:
  - `GetInOutVehicles` @10066140 = OS4 `FUN_01052a4c` / 68k `FUN_000db2e4`;
  - `GetOutVehicle` @10065660 = `FUN_01052398` / `FUN_000dab40`;
  - `GetOutOfVehicle`(2) @10065c90/@10065ef0 = `FUN_01052714`/`FUN_010528b8` / `FUN_000e361e`;
  - `SetObjectDest_IntoVehicle` @10071af0 = `FUN_0105ef34`.
- **Collisions.** Identical apart from §5.1:
  - `CheckForCubeCollision`;
  - `GetCollideInfo` @100af0e0 = 68k `FUN_000446b8` / OS4 `FUN_010700b8`;
  - `CheckForCollisions` @1006a150 = `FUN_000de0b6` / `FUN_01056c58`;
  - `DoCollideStuff`;
  - `MoveObject_General` @1006e4f0 = `FUN_000e0e46`;
  - `AddCollideObj` @10055c70 = `FUN_000cdfdc` / `FUN_01045288`.

  This covers the person branch (4 radius probes, 0x80 push, 9 heights, falling-up bounce
  `vz = abs(vz)·2/3`, map clamp), breakable face type 3 with sound 0x31, run-over thresholds 50/100, the
  ram score `min(6000, v²>>9)`, the Irony bonus 10000 (0x7eb), and Flattened (0x7e5).
- **N7 (all builds; missing from our notes and port).** On a wall hit that opposes the velocity, the
  heading turns by ±0x5a (about 5.6°). This is the tail of `CheckForCubeCollision` / 68k `FUN_00044f82`.

Continued in [`gameplay_part2.md`](gameplay_part2.md).
