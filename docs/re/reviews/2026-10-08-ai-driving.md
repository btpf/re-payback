# AI driving review: Mac vs Amiga vs port

Line-by-line comparison of the AI driving code: traffic, police, scripted drives.
Sources: Mac OS X `analysis/ghidra_macx/decompiled.c` (original C++ names; the more
definitive copy), 68k Update 7 `analysis/ghidra_68k`, OS4 `analysis/ghidra_os4` (the OS4
main update `FUN_0105f594` does not decompile; its AI is identical to the 68k one wherever
`analysis/ghidra_os4/long_timeout.c` shows it). Scratch extracts were in `analysis/scratch/aidrive/` (local working files, not kept;
`docs/CONVENTIONS.md` "Paths in the notes").

Port line numbers are from `HEAD` (commit 978a0ac) before this pass.

## Function map

| Role | Mac | 68k | OS4 |
|---|---|---|---|
| Per-object AI + vehicle update (vehicle branch) | `do_Other` @ 10146280 | `FUN_000e3a58` ~124165-124900 | `FUN_0105f594` |
| Lane choice (state 3) | `MoveUnit` @ 10081080 | `FUN_000a143e` | `FUN_010cb7e4` |
| Spawn scheduler | `AddVehicles` @ 10059d40 | `FUN_000d0cfc` | `FUN_010485cc` |
| Spawn one drone (car / pedestrian) | `AddThing` @ 10058350 | `FUN_000cf5cc` | `FUN_01047af4` |
| Roadblock | `AddPolice` @ 10058ba0 | `FUN_000cfe0c` | `FUN_01047134` |
| Occupancy grid add / remove | `AddCollideObj` @ 10055c70 / `RemoveCollideObj` @ 100570d0 | `FUN_000cdfdc` / `FUN_000cefa0` | |
| Horn / siren / tank gun (fire input) | `DoVehicleFire` @ 10070c20 | `FUN_000e2a50` | `FUN_0105e02c` |
| Route: nearest / farthest / next hop | `FindNearestNode` @ 100f11d0 (`FindClosestPoint` @ 100f1020), `FindFurthestNode` @ 100f1300, `Route` @ 100f14b0 | `FUN_000a42ac`, `FUN_000a430a`, `FUN_000a43f2` | `FUN_010cd590`, `FUN_010cd684`, `FUN_010cd774` |
| Per-frame countdown (`+0x3c0`) | `ResetObject` @ 1006e490 | top of `FUN_000e3a58` | |
| Object constructor (skill `random(100)`, aim = position) | `CreateObject` @ 10075630 | `FUN_000a2b80` | `FUN_010c0c1c` |
| Movement integration `pos += v` | `MoveObject_Vehicle` @ 1006e890, `MoveObject_General` @ 1006e4f0 | `FUN_000e3a58` ~123574 | |
| Despawn outside the windows | `DoCollideStuff` @ 10067b50 | `FUN_000dc5ac` | `FUN_010542b8` |

Mac model offsets are OS4 + 0x385c (front `+0x8124` = OS4 `+0x48c8`, accel `+0x813c`, yaw
`+0x8140`, mass `+0x8148`, brake `+0x814c`, police `+0x8166`). Mac object offsets: aim
`+0x7c8/+0x7cc`, velocity `+0x7dc/+0x7e0`, turn rate `+0x808`, skill `+0x828`, health `+0x82c`,
script cap `+0x868`, speed `+0x890`, nodes `+0x89c/+0x89e/+0x8a0`, countdown `+0x8a2`, J-turn latch
`+0x8a3`, target `+0x908`, lane dir `+0x90d`, state `+0x910`, horn handle `+0x918`, goto radius
`+0x930`, impatience `+0x93c`, flash `+0x940`, direct flag `+0x98a`.

## What already matched

Checked constant by constant and found equal in Mac, 68k and port (`sim/vehicle_ai.rs`
`controller`, `speed_band`, `follow_route`; `sim/pursuit.rs`):

- Which states the AI drives (3, 8-11, 0xf, 0x14, 0x15); pursuit selection (police, not a
  police van or its flash frames `_DAT_1017c3e4/56c/570`, state != 0xf, single player; wanted < 3
  -> state 3, else skill = wanted + 0x5c / 0x5e, state 10 unless 9/11).
- Aim clamp ±1500 for 8/10/0xf/0x14/0x15 without `+0x98a` (the Mac also computes a ±1000 clamped
  angle first and then overwrites it: dead code).
- Look-ahead: skipped at skill >= 99 with health >= 50; heading + 2·ticks·yawrate; reach
  `max(front + speed²·4/brake + 0x400, 0x800)`; squares every 0x800; off-map blocks; light on the last
  square, blocked at skill < 70 for states 1, 2, 3, 6 (the Mac's nested `< 0x5a` test is dead code).
- Cruise bands (all five rows, `brake` on both sides for skill <= 50), corner = steer && !direct.
- Impatience table (<= 30 / 31-60 / 61-90 / >= 91), still-blocked brake/throttle, anti-spin (Mac
  form, already adopted), script speed cap.
- Goto 0xf/0x15 arrival at the bonnet, `|dx|+|dy| <= 2r`; state 0x15 police drop the order.
- Route arrival radii (Mac `300 + speed²>>7` pursuit, `300 + |5·speed|` flee, already adopted);
  `FindClosestPoint` sampling every 512 map units along links; next-hop table.
- Cop drop-off (8000 Manhattan, both speeds < 100 or health < 50, state 5, door 0x370, side toggle
  1, cop state 3 with shotgun 100).
- Siren model flash (`+0x940 += ticks mod 10`, < 5 blue) and loop (25e6, 10000 Hz).
- Traffic type choice (`pick_type`), skill and colour, starting speed min(maxF, 100) (Mac only).

## Findings and fixes

Each: original (Mac, Amiga) - port before (file:line) - fix.

### F1. Traffic used its own, different driver (major)

- Original: state 3 runs `MoveUnit` and then the same driver as every other AI state
  (`do_Other` @ 10146280 l.367 onwards; 68k `FUN_000e3a58` ~124700).
- Port: `traffic.rs:331-490` `drive_traffic` had a separate controller:
  - look-ahead along the direction to the target instead of heading + 2·yawrate, reach with
    `512 + ...` instead of the model's front (`:405`);
  - no skip of the look-ahead for skill >= 99 (traffic reaches 99);
  - red lights only checked when not already blocked (`:427`), on the last square;
  - impatience timer only counted down while blocked (`:447`), honked once per timer reset
    instead of continuously while the timer is above the threshold, skill <= 30 never pushed on,
    skill 31-60 used the 50/50 row;
  - still blocked: only braked; never throttled when rolling backwards (`:479`);
  - lower band `max(V, L) − accel` (no L floor) and skill <= 50 used `accel` not `brake` (`cruise`
    `:316`);
  - dead zone `|err| > turn/2` symmetric; no J-turn anti-spin.
- Fix: `drive_traffic` now calls `move_unit` then `vehicle_ai::controller` (skill = traffic skill,
  police per model, not direct, no clamp), plus the horn and script cap. CONFIRMED.

### F2. Lane choice (`MoveUnit`)

- Original (Mac `MoveUnit` @ 10081080 = 68k `FUN_000a143e`; only the radius differs):
  - U-turn exclusion by the car's **heading** quadrant (heading 0x2d0..0x870, facing E, drops W;
    0x870..0xe10, facing N, drops S; 0xe10..0x13b0, facing W, drops E; otherwise, facing S, drops
    N: the exit behind the car), and only **if another exit remains**: a dead end turns round.
  - Flags from the cube level under a point 0x200 above the car at the target square
    (`GetCubeNum(x, y, (z−0x200)>>4) − 1`, 2 when above all cubes), falling back to the level
    above, then below, when it has no flow bits.
  - No flow bits at all: the target stays (the car parks on it).
  - `random(n)` over N, E, S, W, 0x10 in that order; 0x10 continues in the stored direction
    `+0x90d` (0 = west).
  - The first choice happens on the spawn square itself: `CreateObject` sets the target to the
    spawn position, so `MoveUnit` re-picks from the spawn square at once.
- Port: `traffic.rs:363-392`: U-turn by stored direction (`:368`), no U-turn ever (straight on
  forced, `:379`), `obj.level` only, first target = the spawn exit's neighbour (`:296`), and
  `resume` (`:90`) re-aimed at the neighbour nearest the heading.
- Fix: `traffic::move_unit` as above; the steering target lives in `Nav::aim` for every state, so
  a car handed back to traffic (pursuit off, `Actor_ReleaseIdle`) keeps its target and lane code
  picks up from there; new cars start aimed at their own square. CONFIRMED.

### F3. Occupancy grid covers the object's box

- Original: `AddCollideObj` @ 10055c70 (68k `FUN_000cdfdc`) marks every square of the
  axis-aligned box of the 4 corners (±radius across, −rear/front along; a person's 4 corners all
  at +radius along, sic), at level min(level, 2); a player's person model one square wider each
  side. All objects count, projectiles included.
- Port: `vehicle_ai.rs:97` marked only the centre square and skipped projectiles; people.rs and
  traffic.rs spawners had their own centre-only grids, ignoring the level.
- Fix: `vehicle_ai::occupancy` computes the box; the spawner uses it, per level. CONFIRMED.

### F4. Police aim *behind* the player in a direct chase

- Original: state 9/11 police aim at `target + v·(5−aggr)·−3` (Mac `do_Other` l.118091, 68k ~124291);
  objects move with `pos += v` (Mac `MoveObject_Vehicle` @ 1006e890 l.76, `MoveObject_General`
  @ 1006e4f0; 68k ~123574), so the point trails the player by (5−aggr)·3 ticks (leads only at aggr 6).
  The cops on foot use the same expression (`do_Person` @ 1013fbd0 l.115714, 68k ~122108).
- Port: `pursuit.rs:239` `ppos + pvel·(5−aggr)·3` (lead); docs said "lead".
- Fix: `ppos − pvel·(5−aggr)·3` for the cars (`pursuit.rs`) and the cops on foot (`police.rs`
  `cops`). CONFIRMED.

### F5. Route-node cache lives on the target

- Original: the target's nearest node `+0x8a0` and countdown `+0x8a2` are fields of the **target**;
  `ResetObject` decrements the countdown per frame, the first pursuer to see 0 refreshes and sets
  20. Flee (0x14) re-picks the farthest node whenever the player's countdown is 0 and only resets
  its own, so without pursuers it re-picks every frame.
- Port: per-chaser `Nav.refresh` (`pursuit.rs:226`, `vehicle_ai.rs:455`), each refreshing on its own
  21-frame phase.
- Fix: `vehicle_ai::TargetNode` resource (owner = player object, reset when it changes),
  ticked once per frame; pursuit and flee read it. CONFIRMED.

### F6. Horn loops while honking

- Original: the AI honk is the fire input; `DoVehicleFire` starts a **looped** horn (`AddSound(...,
  loop 1, 20e6, clamp(16e6/mass, 3000, 20000))`, police: siren 25e6/10000) and moves it with the
  car; `do_Other` deletes it (`DeleteSound(+0x918)`) the first frame the input is off.
- Port: `vehicle_ai.rs:321` one-shot on the press edge; traffic one-shot per timer reset.
- Fix: `vehicle_ai::horn` keeps a looped sound (child of the car) while honking. CONFIRMED.

### F7. Spawner: budget, edge, levels, pedestrians, roadblocks, multiplayer

- Original (`AddVehicles` @ 10059d40 / 68k `FUN_000d0cfc`, `AddThing` @ 10058350 / `FUN_000cf5cc`):
  - budget: spawn while `lastIndex < min(extra + 11, 20)` (Amiga `+ 8`), extra = objects flagged
    `+0x934` (map object list, script `AddObject*`; `IncExtraObjs` @ 10051d30) + 2 per player in
    multiplayer; re-checked before every try;
  - edge: y axis on a tie (`|vy| < |vx|` for x); stationary: `random(3)` -> N, E or S (never W);
  - each of the `2·ticks` passes tries **every** cube level, with its own random square on the edge;
  - `AddThing`: multiplayer rejects squares inside other players' windows; the occupancy test is
    per level; a flag-0x20 square adds one "pavement" choice to `random(n)` alongside the lane exits
    (pedestrian 1 in 5);
  - roadblocks are tried inside the same loop, after each `AddThing`, `(wanted−4)·2+1` times per
    level per pass, on the same edge (not stopping after the first);
  - multiplayer: one player per pass, moving to the next only after a pass that spawned.
- Port: `traffic.rs:164-310` cap 20 (`:166`), x on ties (`:212`), first level only and one random
  square per pass (`:236`), any-level occupancy (`:230`), split screen by clock; pedestrians from a
  separate spawner (`people.rs:1026`, cap 20 `:1053`, `rand(5)` on every 0x20 square `:1097`);
  roadblocks once per tick, first success only (`pursuit.rs:324-466`).
- Fix: one spawner `traffic::spawn_ambient` / `add_thing` / `add_police`, run on the `World` so each
  spawn is in the grid and the count at once (as `NewObj` does); `traffic::Persistent` marks the
  `+0x934` objects (`CreateActor`, parked map vehicles); `people.rs` no longer spawns. Each object
  creation draws `random(100)` (the constructor's skill draw). CONFIRMED.

### F8. Pursuit off / on keeps the car's state

- Original: dropping out of pursuit (wanted < 3) only writes state 3: the skill stays the pursuit
  skill, the target point and route node stay; starting a pursuit keeps the node too.
- Port: `pursuit.rs:146` resumed traffic with skill 95 and a fresh lane target; `:156` reset `Nav`.
- Fix: keep skill, `Nav` and lane direction. CONFIRMED.

### F9. Steering dead zone tie

- Original: turn "up" (steer −1) when the up way is shorter and `> ticks·yaw/2`; otherwise turn down
  when the down way is `>= ticks·yaw/2` (Mac l.437-478, 68k ~3140).
- Port: symmetric `|err| > dead` (`vehicle_ai.rs:175`).
- Fix: the original comparison. CONFIRMED (tiny).

## Kept as port deviations (documented in vehicles.md §6.3)

- Attack / follow / plain flee / goto-and-vanish for vehicles (the original leaves them idle).
- Flee (0x14) on its goal node: the original falls into state 11 (straight chase of `+0x908`); the
  port waits there.
- No route file: flee runs straight away from the player.

## Not changed here (outside this pass's files)

- (Done in a follow-up: `police.rs` cops on foot now trail like the cars (F4); `people.rs`
  `despawn_far` no longer culls state 0x12 (flee): the list is 3, 5, 7, 10, 11, 12 in both Mac
  `DoCollideStuff` @ 10067b50 l.25138 and 68k `FUN_000dc5ac` ~117128.)
- The original runs each object's AI right after its own movement, in table order; the port runs
  all AI, then all physics.

## Skill levels (summary)

| Skill | Source | Effect |
|---|---|---|
| 0..99 random | traffic `clamp(rand(90)+aggr·8−30, 0, 99)`; white van: no −30 | cruise row, lights, impatience |
| 95..100 | police `wanted + 0x5c` (aggr >= 4: `+0x5e`) | 98+: uncapped band; 99+ (health >= 50): no look-ahead (rams) |
| `Random(100)` | every new object (constructor) | script cars until `Actor_SetAggro` |
| < 70 | | stops at red (states 1, 2, 3, 6) |
| < 81 | | no steering while the J-turn latch is set |
| >= 98 and (police or `+0x98a`) | | 730/500 straight, 240/120 corner |

There is no stuck detection or reversing logic in the AI: a blocked driver brakes, pushes on
when its impatience timer expires, and only throttles if rolling backwards; J-turns come from
the physics when reversing with steer (vehicles.md §2.3).

## Verification (2026-10-08)

- Traces (`analysis/scratch/aidrive/dumpstats.py` over `--dump` checkpoints, map 0, seed 1234, 1500
  ticks, `runs/b_*` = prior build, `runs/a_*` = this pass): object count now stays at or under the
  `extra + 11` budget (max 17 vs 21 before); pedestrians 1.5 vs 3.5 on average (they now share the
  spawner's choices); at wanted 4 cars reach state 11 and drop a cop; at wanted 6 roadblocks spawn
  (5 vs 4).
- `cargo test -p payback -p payback-formats`: pass.
- Replays (`analysis/scratch/replay/check_aidrive.sh`): story 1500 ×2, rampage 1000, multiplayer 1000
  MATCH; wanted 6 (pursuit + roadblocks, played back with the same `--wanted`) MATCH.
- Autoplay `--fast 20 --seed 1234`: all six levels complete; the level-10 race is won; the level-21
  race fails the same way as before this pass (bot, "What the hell are doing?").
