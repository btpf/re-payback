# Mac vs Amiga: vehicle physics and vehicle AI

Scope: the Mac OS X build (`analysis/ghidra_macx`) compared line by line with the 68k Update 7 build
(`analysis/ghidra_68k`) and OS4 (`analysis/ghidra_os4`, `analysis/ghidra_os4/long_timeout.c`). The Mac runs at
30 Hz like OS4, so that is not listed as a difference. "CONFIRMED" means seen in the Mac decompile and
in at least one Amiga decompile (usually both). "INFERRED" means reasoned from the code, not traced end
to end and not run.

## 0. Offsets and function map

Offset translation used below (CONFIRMED by many matching constants):

- Mac model field = OS4 model field + 0x385c. 0x8120 half-width (OS4 0x48c4), 0x8124 front,
  0x8128 rear, 0x8130 home city, 0x8134 maxF, 0x8138 maxR, 0x813c accel, 0x8140 yaw, 0x8144 grip,
  0x8148 mass, 0x814c brake, 0x8160 RC car (0x4904), 0x8165 class (0x4909), 0x8166 police (0x490a),
  0x8178 horn, 0x817c reverse beep, 0x8180 engine, 0x8184 tracked (0x4928), 0x8186 flies (0x492a),
  0x8188 boat (0x492c), 0x818a invert roll (0x492e), 0x818c bus (0x4930), 0x8190 spawn accept
  (0x4934), 0x8194 radio, 0x8198 name.
- Mac instance field = OS4 instance field + 0x4e0 for the vehicle fields: 0x7c8/0x7cc aim (OS4 0x2e8),
  0x7dc/0x7e0/0x7e4 vx/vy/vz, 0x7e8 lean, 0x7ec dip, 0x808 yaw rate, 0x80c accel ramp, 0x818 on ground
  (OS4 0x338), 0x81a in water (OS4 0x33a), 0x828 skill, 0x82c health, 0x868 script speed cap,
  0x890 speed (OS4 0x3b0), 0x894 wheel speed, 0x8a3 J-turn latch (OS4 0x3c3), 0x910 state (OS4 0x428),
  0x93c impatience timer, 0x984 heli engine on (68k 0x498).
- 68k caution: 68k on-ground is +0x336 and in-water is +0x338 (2 lower than OS4). 68k model +0x4928 is
  OS4 +0x492a (flies) and 68k +0x492a is OS4 +0x492c (boat).

| Mac function | 68k twin | OS4 twin | Role |
|---|---|---|---|
| `do_Other` @10146280 (vehicle part) | `FUN_000e3a58` ~124020-125880 | `FUN_0105f594` (vehicles_os4/main.c) | input, driver AI, throttle, steering, landing, sinking |
| `MoveUnit` @10081080 | `FUN_000a143e` | `FUN_010cb7e4` | traffic junction / lane choice |
| `PositionUnit` @10081450 | `FUN_000a1758` | `FUN_010cbc7c` | body roll/pitch from slope, downhill speed |
| `SkidUnit` @10081830 | `FUN_000a197a` | `FUN_010cc288` | grip, surface, skid marks and smoke, skid sound |
| `PositionAirborneUnit` @10082620 | `FUN_000a241c` | `FUN_010ccc5c` | airborne / helicopter |
| `BounceUnit` @10082bc0 | `FUN_000a27c6` | `FUN_010cd198` | wall bounce, crash damage |
| `MoveObject_Vehicle` @1006e890 | `FUN_000e109e` | `FUN_0105b9ec` | movement sub-steps, landing, gravity |
| `CheckForCubeCollision` @100af980 | `FUN_00044f82` | `FUN_01070868` | box-vs-cube collision |
| `GetCollideInfo` @100af0e0 | `FUN_000446b8` | (inside `FUN_01070868`) | segment wall test |
| `AddVehicles` @10059d40 | (not located; OS4 used) | `FUN_010485cc` | traffic spawn ring |
| `AddThing` @10058350 | ~111700-111900 | `FUN_01047af4` | spawn one traffic car or pavement ped |
| `AddPolice` @10058ba0 | - | `FUN_01047134` | police roadblock spawn (spot-checked only) |
| `ObjTakeOff` / `ObjLand` @10105570 / @10105670 | `FUN_000b4666` / `FUN_000b46dc` | - | `Heli_TakeOff` / `Heli_Land` |
| `ResetObject` @1006e490 | - | - | per frame: count down node cache +0x8a2, store previous x/y/heading |
| `SetObjectDest_IntoVehicle` @10071af0 | - | `FUN_0105ef34` | ped walks to a door (spot-checked) |
| `UpdateMapPosition` @10068cb0 | `FUN_000dd222` | - | camera follow (not vehicle physics, see 3.4) |
| `InitStructures` @1005ffb0 | `FUN_000d7874` | `FUN_0104e13c` | model handling table |

Corrections to `docs/re/vehicles.md` §0 (CONFIRMED by structure and by the Mac names): 68k
`FUN_000a1758` is the slope function (PositionUnit), not grip. Grip/skid is 68k `FUN_000a197a`
(SkidUnit). The movement sub-step loop is 68k `FUN_000e109e`, not "inside FUN_000a197a". The wall bounce
is 68k `FUN_000a27c6`. The traffic lane choice is 68k `FUN_000a143e` = OS4 `FUN_010cb7e4`. The segment
wall test is 68k `FUN_000446b8`.

---

## 1. Differences (Mac vs Amiga)

### D1. Pursuit route-node arrival radius (states 8/10)

- Mac `do_Other` @10146280 (about line 253 of the function); 68k `FUN_000e3a58` ~124245; OS4 main.c ~800.
- Mac:
  ```c
  iVar18 = (piVar42[0x224] * piVar42[0x224] >> 7) + 300;      // speed^2/128 + 300
  if ((aimx - iVar18 < x) && (x < aimx + iVar18) && (same for y)) -> next node, or state 9/11
  ```
- 68k / OS4:
  ```c
  iVar14 = |speed * 5|;  if ((aimx - 300) - iVar14 < x && x < aimx + 300 + iVar14) ...
  ```
- The flee route (state 0x14) is unchanged in the Mac: still `|5*speed| + 300` (`FUN_101609c0` = abs).
- Effect: at speed 100/300/600/730 the radius is 800/1800/3300/3950 on the Amiga and
  378/1003/3112/4463 on the Mac. Slow and mid-speed pursuers must get much closer to a node before they
  switch to the next one; very fast ones switch a little earlier.
- Classification: balance change (AI tuning).
- Port: `crates/payback/src/sim/vehicle_ai.rs` `follow_route` uses `300 + |5*speed|` for both pursuit
  (`sim/pursuit.rs:228`) and flee (`vehicle_ai.rs:449`). To follow the Mac, give `follow_route` a mode:
  pursuit `300 + (speed*speed >> 7)`, flee unchanged.
- Confidence: CONFIRMED (Mac, 68k and OS4 all seen).

### D2. Traffic junction look-ahead radius (`MoveUnit`)

- Mac `MoveUnit` @10081080; 68k `FUN_000a143e`; OS4 `FUN_010cb7e4`.
- Mac: `r = speed * 1000 / yaw; if (r < front) r = front;`
- 68k / OS4: `r = speed * speed * 5 / yaw; if (r < front) r = front;`
  (OS4: `iVar7 = (param_1[0xec]*param_1[0xec]*5) / *(int*)(param_1[9]+0x48e4)`).
- The rest of the function is identical: map bounds, cube flags `+5 & 0x1f`, layer fallback +/-1,
  heading masks 0xf7/0xfb/0xfd/0xfe, random exit pick, lane direction byte, exit points at +/-0x400 and
  +0xc00.
- Effect (yaw 40): at speed 100/200/300 the radius is 1250/5000/11250 on the Amiga and 2500/5000/7500 on
  the Mac. The two agree at speed 200 for any yaw. Fast traffic no longer picks its next junction 5+
  squares ahead; slow traffic picks it a little earlier.
- Classification: balance change, or a bug fix (the quadratic radius makes fast cars re-target far too
  early; the intent is INFERRED).
- Port: `crates/payback/src/sim/traffic.rs` `drive_traffic`:
  `let r = (obj.speed * obj.speed * 5 / t.turn.max(1)).max(512);` becomes
  `(obj.speed * 1000 / t.turn.max(1)).max(front)`. The original floor is the model's front extent
  (`+0x48c8`), not 512.
- Confidence: CONFIRMED.

### D3. AI anti-spin rule for low-skill drivers

- Mac `do_Other` (about lines 795-809 of the function); 68k `FUN_000e3a58` ~124855 (`LAB_000ea87a`);
  OS4 main.c 1138-1140.
- Mac:
  ```c
  if (skill < 0x51) { if (jturn_latch == 1) steer = 0; }
  else if (jturn_latch == 1) { drop steering that fights the yaw rate }
  ```
- 68k / OS4:
  ```c
  if (skill < 0x51) { if (jturn_latch == 1 || throttle == 1) steer = 0; }   // OS4: || DAT_0114c5c8 == 1
  ```
- Effect: on the Mac, drivers with skill <= 80 keep steering while they brake or reverse (throttle +1).
  On the Amiga they drive straight while braking, so blocked low-skill cars back out straight and
  braking cars miss turns.
- Classification: behaviour change, probably a deliberate fix (intent INFERRED).
- Port: `crates/payback/src/sim/vehicle_ai.rs` `controller` (comment "Don't steer into a handbrake
  spin (68k ~124730)"): change `if drive.j_turn || throttle == 1` to `if drive.j_turn`.
- Confidence: CONFIRMED.

### D4. Traffic cars spawn already moving

- Mac `AddThing` @10058350; OS4 `FUN_01047af4` tail; 68k ~111845-111890.
- Mac, right after it sets the new car's state, lane and layer:
  ```c
  iVar22 = 100; if (*(int *)(model + 0x8134) < 100) iVar22 = *(int *)(model + 0x8134);
  *(int *)(obj + 0x890) = iVar22;                       // speed = min(maxF, 100)
  ```
- Amiga: no speed write, so speed stays 0 from the constructor.
- The velocity (vx, vy) is not set, so the grip step brings it up over the first ticks.
- Classification: new behaviour (traffic no longer appears standing still in the road).
- Port: `crates/payback/src/sim/traffic.rs` spawn loop, after `spawn_vehicle_world` where
  `o.state = State::Wander` is set: add `o.speed = TYPES[kind].max_speed.min(100)`.
- Confidence: CONFIRMED.

### D5. Tank reverse speed

- Mac `InitStructures` @1005ffb0 (decompile line 22461): tank `+0x8138 = 0xffffff24` (-220).
- 68k `FUN_000d7874` (line 115256) and OS4 `FUN_0104e13c` (line ~41163): `0xffffff9c` (-100).
- Every other handling value of all 28 types and of the police car and police van variants is the same.
  A script compared every model field written in both setup functions.
- Classification: balance change (the tank now reverses as fast as it drives forward).
- Port: `crates/payback/src/vehicles.rs` tank entry `[220, -100, 6, 20, 55, 50, 40000, 0]` becomes
  `[220, -220, ...]`. Also update the tank row in `docs/re/vehicles.md` §1.3.
- Confidence: CONFIRMED.

### D6. Map edge in the full (last sub-step) vehicle box test

- Mac `CheckForCubeCollision` @100af980, vehicle branch, `param_4 == 0` path (3x3 box points);
  68k `FUN_00044f82` (lines 229-271 of the function); OS4 `FUN_01070868` `param_3 == 0` path.
- Mac 3x3 path, for each box point:
  ```c
  if (px < 0)       { hitX = 1;  px = 0; }      if (0x3ffff < px) { hitX = -1; px = 0x3ffff; }
  if (py < 0)       { hitY = 1;  py = 0; }      if (0x3ffff < py) { hitY = -1; py = 0x3ffff; }
  ```
- 68k / OS4: this clamp and the hit flag exist only in the 2x2 path (`param_3 != 0`, the earlier
  sub-steps). The 3x3 path stores the points unclamped and never flags the edge. The vehicle branch never
  clamps the car's own position either; only the person branch does.
- Effect: with 1 tick per frame (the normal case) only the 3x3 path runs. On the Amiga the map edge then
  does not bounce the car: `GetCollideInfo` clamps segment ends, so it does not see the edge as a wall,
  and the car's box and centre can leave [0, 0x3ffff] (consequence INFERRED, not run). The Mac bounces
  the car (BounceUnit damage and sound) and turns its heading away (+/-0x5a, see N7).
- Classification: bug fix.
- Port: `crates/payback/src/sim/collide.rs` `resolve_walls` clamps x/y to `WORLD_MAX` without
  reporting a hit. To match the Mac, set `hit.x`/`hit.y` when the clamp changes a coordinate, so
  `driving.rs` applies the bounce and damage.
- Confidence: CONFIRMED for the code difference; the effect is INFERRED.

### D7. Police light-bar reset when the siren stops

- Mac `do_Other` tail (about lines 1970-1985 of the function); 68k `FUN_000e3a58` ~125860;
  OS4 main.c ~2532.
- Mac: `if (horn == 0) { if the model is a police red/blue variant -> base model; if (siren handle >= 0)
  delete it; }`
- Amiga: `if (horn == 0 && siren handle >= 0) { delete it; if police variant -> base model; }`
- Effect: the Mac resets the flashing model even if the siren never got a sound handle (for example, no
  free channel). This only shows when sound allocation fails.
- Classification: robustness fix (close to a platform artefact).
- Port: nothing to change, unless the port ties the light-bar reset to the siren sound entity. In that
  case, reset the model whenever the horn input is off.
- Confidence: CONFIRMED.

### D8. Flee (state 0x14) node cache

- Mac `do_Other` (about lines 317-323 of the function); 68k ~124330; OS4 main.c ~840.
- When the player's node-cache byte is 0, the Amiga also writes the player's nearest route node into
  the player's own current-node slot (68k player `+0x3bc`, OS4 `+0x3be`). The Mac only computes the
  farthest node for the fleeing car.
- Effect: none visible, because players do not use that slot (INFERRED).
- Classification: cleanup.
- Port: nothing to change.
- Confidence: CONFIRMED for the code; "no effect" is INFERRED.

### D9. `GetRandomVehicleNum` / `GetRandomVehicleNum_Pursue` (Mac only)

- Mac @10057e50 / @100580c0 pick `random(0x1a)` (25 becomes 27) and skip 22/23 (RC car, boat). The
  Pursue version also skips 9 (police), 11 (tank) and 17 (helicopter). More exclusions depend on
  `GetHighestLevelCompleted()`: the tank, rocket car (24), RC truck (21), helicopter (17), Pug Racer (18)
  and Vapour GT (27) are gated by progress, some only 1 time in 5.
- Nothing in the Mac OS X decompile calls either function, and neither the 68k nor the OS4 decompile has
  a matching function (one that calls random(0x1a)).
- Classification: new code that seems unused (dead). INFERRED: meant for a "random vehicle" option
  (Rampage or multiplayer) that was never connected.
- Port: nothing to change.
- Confidence: CONFIRMED that the functions exist and have no direct callers; the purpose is INFERRED.

### D10. Pavement pedestrians from the traffic spawner store their spawn point

- Mac `AddThing` person branch (cube flag 0x20, `random(5) == 0`) also writes `obj[0x1f5] = x` (+0x7d4)
  and `+0x7d8 = y` (OS4 +0x2f4/+0x2f8). The 68k (~111790) and OS4 (`FUN_01047af4`) versions do not.
- The purpose was not traced (people area; INFERRED to be a home point). The rest of the branch is the
  same: model `6 + random(4)` skipping 7, heading `random(0x167f)`, pitch 0x5a0, state 3,
  z = ground - 0x200.
- Classification: new behaviour (people). Port: pass to the people comparison.
- Confidence: CONFIRMED for the write; the purpose is INFERRED.

### D11. Wall-hit damage overflow guard (OS4 artefact, not a Mac change)

- `BounceUnit` (Mac) and 68k `FUN_000a27c6` both set the damage to 0 when `v*v` overflows
  (`(v2 >> 14) & ~(v2 >> 31)` on the Mac, a sign test on the 68k). OS4 `FUN_010cd198` does not. This
  only matters for |v| > 46340. Port: nothing to change.

---

## 2. Our notes' quirks and bugs, checked against the Mac

| Note (vehicles.md) | What the Mac does | Result |
|---|---|---|
| §1.3 `policevan_red` brake 10 / grip 20 / mass 2000 "typo" | `InitStructures` line ~22625: grip 0x14, mass 2000, brake 10 | NOT fixed (CONFIRMED) |
| §2.3 airborne wheel spin uses brake when ws < 1 and accel otherwise "(sic)" | `do_Other` ~853-871 (forward) and ~1012-1031 (reverse) | NOT fixed (CONFIRMED) |
| §2.3 J-turn sign comment | Forward key + reversing + steer < 0 gives yaw rate -2*yaw; reverse key + moving forward + steer < 0 gives +2*yaw (same in 68k ~124880 / ~125055) | Same in all builds. The note's "steer<0 -> +2*yaw in reverse" is wrong or confusing: the sign always matches normal steering. Port `driving.rs` already matches. |
| §2.3 coast "stop the beeper" | The beeper starts or continues whenever speed < 0 and stops when speed >= 0, whatever the throttle (Mac ~1144-1210; 68k `LAB_000eb0d6`) | Note is imprecise; port `vehicle_sounds` already does this |
| §2.5 "slip measured before the correction" | Mac SkidUnit, 68k `FUN_000a197a` and OS4 `FUN_010cc288` all measure `abs(vx - tvx) + abs(vy - tvy)` after the grip step | Note is wrong for all builds; port `driving.rs` measures after (correct) |
| §2.10 downhill factor k "I: ticks" | `PositionUnit__FP6Objecti(obj, ticks)` is called with `*_DAT_1017c48c` (ticks); 68k calls `FUN_000a1758(obj, DAT_00348090)` | k = ticks, CONFIRMED. The port has no slope pitch or downhill acceleration at all (`driving.rs`), a gap that predates this comparison. |
| §2.7 wall bounce `v = abs(vx)/2` | `abs(vx >> 1)` in all builds | Trivial rounding detail |
| §2.7 heading deflection (not in the notes) | See N7 below | Missing from the notes; present in all builds |
| §2.8 bubbles "per wheel" | 9 points (3x3 grid, -1..1 by -1..1), each `random(5) == 1` | Note is imprecise; same in all builds |
| §2.6 splash volume mass*8000 | The sub-step path (MoveObject_Vehicle / 68k `FUN_000e109e` / OS4 `FUN_0105b9ec`) uses mass*8000; the inline path in `do_Other` / 68k `FUN_000e3a58` uses mass*10000 | Both exist in all builds; the port uses 10000 |
| §6.1 state 8 is never written | No literal `+0x910 = 8` anywhere in the Mac decompile | Unchanged (INFERRED) |
| §6.1 state 0x14 reaching its goal falls into state 11 | Mac ~345-351: `0x910 == 8 ? 9 : 0xb` | NOT fixed |
| §6.1 a police car in 0x15/0x14 enters the pursuit-selection list (0x0f excluded) | Mac ~192-233: same state set and same exclusions | Unchanged |
| §6.2 skill <= 50 subtracts brake instead of accel "(sic)" | Mac ~593/607 use `+0x814c` (brake) | NOT fixed |
| §6.2 traffic lights: skill < 70 and phase in {1,2,3,6} | Mac ~533-545 (the inner `skill < 0x5a` branch is dead code, as in the 68k) | Unchanged |
| §6.2 "a flying model (+0x492a) gets no input while +0x338 is 0" | Mac: `boat (0x8188) && in_water (0x81a) == 0` means no input. The 68k offsets in the note are the boat flag and the in-water flag. | The note mislabels the fields; behaviour is the same. The port's `driving.rs` "Boats only drive on water" is right. |
| §6.2 look-ahead bounds | Mac and OS4 reject x < 0, y < 0 and > 0x7f; the 68k decompile has no x < 0 test | 68k-only artefact or bug; Mac = OS4. The port already tests all four. |
| §6.1 police lead `v*(5-aggr)*3` | Mac ~279-282, same | Unchanged |

### N7. Missing from our notes: heading deflection on wall hits (all builds)

The tail of Mac `CheckForCubeCollision` (about lines 580-611 of the function) and of 68k
`FUN_00044f82` use the same logic. If the x hit opposes vx (`hitX > 0 && vx < 1`, or
`hitX < 0 && vx >= 0`), the heading turns by 0x5a (90 units, 5.6 degrees): `-0x5a` if the heading is in
[1, 0x5a0] or [0xb41, 0x10e0], otherwise `+0x5a`. For y the signs are swapped. This is what makes cars
glance off walls. Port: `crates/payback/src/sim/driving.rs` (after `collide::resolve_walls`) does not
turn the heading; add the turn to match all builds.

---

## 3. Areas checked and the same in all builds (Mac = 68k = OS4 unless noted)

### 3.1 Physics

- Throttle and brake: ramp +5/tick up to accel or brake, forward/reverse limits, the "wheels driven"
  stop-at-zero rule, and coasting toward 0 by 1/tick. That includes the extra +1 toward 0 when the
  forward key is held while rolling backward.
- Damage top-speed cap `clamp(H*max/50, max/4, max)` in both directions.
- Reversing beeper (sound `+0x817c`, volume 10000000, pitch 0x5622) and the rocket-car thrust frame swap.
- J-turn trigger (not tracked, latch clear, steer != 0), yaw +/-2*yaw, `UseHandbrake` for the player
  (68k `FUN_000a6c12`), and the latch cleared every frame after the AI step.
- Tyre smoke while braking: `random(2) == 0` per corner. The particle size arguments are past the Mac
  decompiler's 8-argument view, so they could not be compared.
- Steering: ramp 5/tick, cap `min(abs(speed), yaw)` (or `yaw` when tracked), reversing inverts it, zero
  with no steer. Heading snap toward 45 degrees by 4*ticks, with the 1000 / 0x1298 wrap special cases.
- Visual lean (0x40*ticks slew, 0x20*ticks relax, boat inversion) and dip (+/-16/tick, +/-0x80 clamp).
- Engine pitch: tank `abs(ws)*10000/cap + 4000`, others `*20000/cap + 6000`, helicopter with the engine
  off 4000. A damaged engine (health < 50) swaps its sample: 1->0x2e, 0x43->0x47, 0x44->0x48,
  0x45->0x49, 0x46->0x4a.
- Landing: if vz > 0xa0, sound 0x1e (volume abs(vz)*20000, pitch 15000, at z+100) and
  vz = 0x80 - abs(vz>>2). Terrain follow `clamp(ground + 0x80 - z, +/-1000)`.
- Splash: the RC car dies, ObjectSunk (= 68k `FUN_000a6a12`), sound 0x2b at pitch 0x209e. Boat bobbing
  (+/-2 springs) and wake particles.
- `MoveObject_Vehicle`: sub-step loop and landing from step 2. Helicopter lift `vz -= 0xaa` when the
  engine is on, z > 0 and vz > -256. Gravity +0x20 unless the car is both in water and on the ground.
  The last step uses the full collision test.
- `BounceUnit`: damage `v*v >> 14`; Thud (< 11, pitch 12000) or Thud2 (pitch 8000, crash flag); bus
  scream for the player; keeps half the normal velocity and pushes 0x20 off the wall.
- Sinking: speed 0, pitch +4*ticks up to 0x5a0, z += 32*ticks while z < 27000, bubbles. At depth the
  player loses the vehicle (announcement phrase 0x7e2 "Drowned!", ShowRecord, a lives decrement in
  multiplayer), the object dies and goes to state 6.
- `SkidUnit`:
  - Surface lookup (off the map or layer > 2 gives 4).
  - Grip table: 0x22 g/2 with a cap of 450 at 3/tick; 0x24 2g with a cap of 350 at 20/tick; 0x32 4 with
    a cap of 200 at 10/tick; 0x33 g*10/25; 0x34 1; otherwise g*4/5. J-turn grip 4.
  - Velocity correction, helicopter early return, slip formula, doubling in water.
  - Marks: size 0xe6 (100 for the RC car), spacing *7/8, texture by surface group, none for the boat.
  - Wheel smoke `random(2) < ticks`, water spray.
  - Skid sound volume `slip*40000`, pitch `slip*10+10000`, at z+0xfa.
- `PositionUnit`: roll and pitch targets from the 3x3 heights, `(old+target)/2` filter, downhill speed
  term.
- `PositionAirborneUnit`:
  - Skid trails reset; steering `min(yaw*ticks, abs(speed>>1))`.
  - Pitch tips +/-16*ticks outside [0x5a0,0xb40] / [0xb40,0x10e0].
  - Helicopter branch: grip-limited velocity, layer 2, yaw ramp, pitch = speed.
  - Engine pitch.
- `CheckForCubeCollision`:
  - Person branch: 4 radius probes, 0x80 push, repeat until clear, 9 heights, water flag, falling-up
    bounce `vz = abs(vz)*2/3`, map clamp.
  - Vehicle box geometry: rotated by roll, pitch and heading, heights `z>>4 - 0x40` lowered for points
    below.
  - 8 edge segments plus the centre segment; CheckForWeapon / CheckForCrate on success.
  - Only D6 differs.
- `GetCollideInfo`:
  - Delta limit 0xae9d, endpoint clamp, per-square walk.
  - Face type 3 (breakable) is broken by non-person objects, with sound 0x31; type 5 is ignored.
  - The Mac decompile hides the 6th int argument (z2) on the stack, so it is INFERRED unchanged.
- `ObjTakeOff` / `ObjLand`: same as 68k `FUN_000b4666` / `FUN_000b46dc`.

### 3.2 Driver AI

- Which states drive (3, 8-11, 0x0f, 0x14, 0x15). State 0x0c police cars only run the siren. The
  RC-car remote-control branch, the controls-disabled check, and the door-slam countdown (50*ticks,
  sound 0x28) are the same.
- Police pursuit entry: wanted >= 3; skill wanted+0x5c (+0x5e when the level is >= 4); state 10 unless
  already 9 or 11. Police-van exclusion, route start (`FindNearestNode`, 9999 sentinel) and the police
  lead on the target are the same.
- Script goto arrival: bonnet point `front*3/4`, `abs(dx)+abs(dy) <= 2r`, ObjectAtLocation.
- Aim clamp +/-1500 for states 8/10/0x0f/0x14/0x15 unless `+0x98a` is set. The +/-1000 clamp before it
  is dead code in all builds. Steering dead zone `ticks*yaw/2`.
- Look-ahead:
  - Skipped at skill >= 99 and health >= 50.
  - Direction heading + 2*ticks*yawrate; reach `front + v^2*4/brake + 0x400`, at least 0x800.
  - Occupancy grid with the car's own bits masked; traffic lights.
- Impatience timer table and honk thresholds; the still-blocked brake/reverse rule.
- Police arrest: within 8000, and both slow or health < 50. The car goes to state 5; a policeman comes
  out via GetOutOfVehicle with state 3, +0x840 = 5, +0x844 = 100.
- Cruise band table (all five rows, including the "sic" brake row).
- Script speed cap `+0x868`. Helicopter: no horn, rotor animation, no input with the engine off. Boat out
  of water: no input.

### 3.3 Traffic spawning

- `AddVehicles` vs OS4 `FUN_010485cc`:
  - Cap `min(level + 11, 20)`; `2*ticks` attempts.
  - Ring size `((z + 0x5dc)*8 >> 11)*2 + 11`; the side comes from the dominant velocity axis (random
    when still); 3 layers.
  - Multiplayer player rotation.
  - Police roadblocks when wanted > 4: `(wanted-4)*2+1` tries, single player only.
- `AddThing` vs OS4 `FUN_01047af4` and 68k:
  - Placement: multiplayer view exclusion, square 1..0x7e, cube visible with face type not 0/4,
    5-cell occupancy, exit flags shared with the neighbour, direction pick.
  - Model choice: forced type (`Actor` script) logic; police substitution `random(10 - wanted) == 0`
    when wanted >= 4; `random(256) < accept`; home city / user-map booleans
    (ObjGetIceCreamVans/PoliceVans/Trucks); 20 tries.
  - Setup: colour `random(8)`, skill formula and white-van rule, z = ground - 0x200, engine and skid
    sounds.
  - Only D4 and D10 differ.
- `AddPolice` was only spot-checked: the bounds check 1..0x7e moved from the caller (OS4) into the
  function, with the same effect.

### 3.4 Other

- `SetObjectDest_IntoVehicle` vs OS4 `FUN_0105ef34` (spot-checked): door offsets by door slot 0-5,
  reach radius `halfwidth/12` (in >>5 units), height window +/-0x800, slot cycling.
- `UpdateMapPosition` is the camera follow (68k `FUN_000dd222`).
  - Same constants: velocity*0x140 clamped to +/-0x2af80, heading lead 300/200/150/100, 1/5 smoothing,
    height terms 0x5dc / 1000, cap 3000, -600 / -800.
  - The Mac keeps two camera sets. It always computes the 300-lead set; it computes the 200-lead set
    only when a flag is 0 and copies the first set otherwise. The 68k picks one set by `DAT_0011a6ac`.
  - Not vehicle physics; left for the render/camera comparison.
- `ResetObject`: the 20-frame countdown on the target-node cache (`+0x8a2`) and the store of the
  previous x/y/heading.
- Handling table: every value is the same except D5.

---

## 4. Port impact summary

| # | File / function | Change to follow the Mac |
|---|---|---|
| D1 | `sim/vehicle_ai.rs` `follow_route` (callers `sim/pursuit.rs:228`, `vehicle_ai.rs:449`) | pursuit radius `300 + (v*v >> 7)`; flee keeps `300 + abs(5v)` |
| D2 | `sim/traffic.rs` `drive_traffic` | junction radius `max(v*1000/yaw, front)` |
| D3 | `sim/vehicle_ai.rs` `controller` | low-skill anti-spin: drop `\|\| throttle == 1` |
| D4 | `sim/traffic.rs` spawn loop | spawn speed `min(max_speed, 100)` |
| D5 | `vehicles.rs` tank row; `docs/re/vehicles.md` §1.3 | max reverse -220 |
| D6 | `sim/collide.rs` `resolve_walls` / `sim/driving.rs` | a map-edge clamp counts as a wall hit |
| N7 | `sim/driving.rs` | +/-0x5a heading turn on wall hits (all builds) |
| §2.10 | `sim/driving.rs` | slope pitch and `speed += pitch*ticks >> 8` are missing (all builds) |

D1-D6 all change the simulation, so the replay golden files (`tools/checks/replay_check.sh`) must
be re-recorded after any of them is adopted.

## 5. Limits

- The Mac `AddEffect__FUsPUciiiiiiiii` calls show only 8 arguments in the decompile, so particle size and
  flags (arguments 9 and up) could not be compared.
- `GetCollideInfo`'s 6th int (z2) is a stack argument that the decompiler hides at the call sites.
- Effects marked INFERRED were not run.
