# Mac vs Amiga: people (on foot) and collisions

Scope: `do_Person` @1013fbd0, `object_MainLoop` @1014a670, `MainLoop` @1014a920,
`MoveObject_General` @1006e4f0, `CheckForCollisions` @1006a150, `DoCollideStuff` @10067b50,
`AddCollideObj` @10055c70 / `RemoveCollideObj` / `GetCollideObjs`, `CheckForCubeCollision` @100af980,
`GetCollideInfo(_alt)`, `GetCubeHeight`, `GetInOutVehicles` @10066140, `GetOutVehicle`,
`GetOutOfVehicle(2)`, `SetObjectDest_IntoVehicle`, `ScareDrones(2)`, `AddThing` @10058350,
`CreateObject` @10075630, `ReadMotion`, `ReadInput`, plus the people data files.

Amiga references: 68k Update 7 `FUN_000e3a58` (people branch ~121875–123700), `FUN_000e0e46`,
`FUN_00044f82`/`FUN_000446b8`/`FUN_00044378`, `FUN_000ec686`; OS4 `FUN_010485cc`/`FUN_01047af4`,
`FUN_010c0c1c`, `FUN_010435f4`/`FUN_01043788`, `FUN_01051a10`, `FUN_01052a4c` etc.

Field map used here: Mac model field = 68k model field + 0x385c (radius 0x48c4→0x8120, class
0x4909→0x8165, police 0x490a→0x8166, mass 0x48ec→0x8148, weapon code 0x4908→0x8164). Mac instance
field = OS4 + 0x4e0 for the early block (vx 0x2fc→0x7dc, wander T 0x2f0→0x7d0, health 0x34c→0x82c,
wanted 0x330→0x810, speed 0x3b0→0x890, level 0x3d0→0x8b0) and OS4 + 0x4e8 for the late block
(state 0x428→0x910, target 0x420→0x908, stun 0x494→0x97c, flee 0x4a8→0x990, player 0x4ac→0x994).
Mac sin table `_DAT_1017c498`, cos `_DAT_1017c49c`; "hit wall" flag `_DAT_1017cc7c` (= 68k
`_DAT_0034c3a0`, set in `object_MainLoop` from the `MoveObject_General` collision flags).

Differences are listed first (most important first), then quirks shared by every build, then
port deviations found on the way (not Mac changes), then the identical areas.

---

## 1. Differences

### M1. Pedestrian wander (state 3) rewritten: peds keep to the pavement (new behaviour) — CONFIRMED

- Mac: `do_Person` @1013fbd0, decompile lines 105953–106096.
- Amiga: 68k `FUN_000e3a58` ~122295–122400 (people.md §7.3). OS4 `FUN_0105f594` didn't
  decompile, but the OS4 constructor and spawner (below) show the same data as 68k.

68k (the version people.md documents):
```c
if (T < 0) T -= ticks;                                  // count straight-walking ticks
if (T < -0x32 && (hit_wall || !(cube[level].flags & 0x20)))
    T = heading + 0x5a0 + rand(0xb40);                  // turn 90..270 deg
... snap/ease/steer ...
if (T < 0 && speed < 0x30) throttle = -1;
```
Mac:
```c
if (0 < obj+0x900) obj+0x900 -= ticks;                  // new re-aim timer
if (!hit_wall) {
  if (!(cube(x,y)[level].flags & 0x20)) goto BACK;
  ax = clamp((x + sin[h]*2) >> 11, 0, 127); ay = clamp((y + cos[h]*2) >> 11, 0, 127);
  if (!(cube(ax,ay)[level].flags & 0x20)) goto BACK;    // look 512 units ahead
  if (obj+0x900 < 1) {
    if (random(0x32) == 0) T = -1;
    else { T = heading + random(1000) - 500; obj+0x900 = 3; }
  }
  anchor(+0x7d4,+0x7d8) = centre of current square;     // (x & ~0x7ff) + 0x400
} else {
BACK:
  if (obj+0x900 < 1) { T = GetAngle(x - anchor.x, y - anchor.y);   // head back to the anchor
                       obj+0x900 = 0x14; }
}
if (T >= 0) { same 90-deg snap, ease 16/tick, steer, T = -1 within 200 (0xa0 one branch) }
if (T < 0) { if (speed < 0x30) throttle = -1; }
else if (speed < 0x18) throttle = -1;                   // new: also walk (slowly) while turning
```

What changed:
- The negative-T countdown and the "turn 90–270° after 50 ticks" rule are gone.
- While the current square **and** the square 512 units ahead are pavement (flag 0x20) and no wall
  was hit: every 3 ticks the target is re-rolled to heading ±500 units (±31°), 1-in-50 chance of
  "go straight" (T = −1, re-rolled next tick). The 90° snap then pulls it back toward the nearest
  axis, so peds amble along the pavement with small wobbles. The anchor (+0x7d4/+0x7d8) is updated
  to the centre of the current square every such tick.
- When the ped leaves pavement, would step off it within 512 units, or hits a wall: every 20 ticks
  it re-aims (snapped to 90°) at the anchor, i.e. the centre of the last good pavement square. So
  peds turn back at kerbs instead of wandering across roads.
- New throttle rule: while T ≥ 0 (turning) they still walk if speed < 0x18; on Amiga they coast to
  a stop while turning.
- `AddThing` @10058350 sets the anchor to the spawn square centre (Mac only, lines 18726–18728).
  `CreateObject` @10075630 initialises it to (−1, −1) (OS4 `FUN_010c0c1c` sets the same OS4 fields
  0x2f4/0x2f8 = −1, so the fields exist on Amiga but the ped AI never reads them).

Quirk (INFERRED): peds that enter state 3 by other routes (ejected drivers via `GetOutOfVehicle`,
flee timeout, cops leaving cars, scripts) keep anchor (−1, −1) until they first stand on a good
pavement square. If they start off-pavement (roads usually lack flag 0x20), every 20 ticks they
aim at the map's north-west corner until they reach pavement.

Classification: new behaviour (AI improvement; arguably a fix for peds wandering into traffic).
Impact: `crates/payback/src/sim/people.rs::ped_ai` (`State::Wander`) implements the 68k rule.
Recommendation: port the Mac logic (needs a per-person timer and anchor in `Person`; set the
anchor in `spawn_peds`, (−1,−1) default elsewhere). RNG call pattern changes (`rand(50)`,
`rand(1000)`), so regenerate replay references. Update people.md §7.3.

### M2. On-foot velocity is smoothed (inertia) instead of set from speed (new behaviour) — CONFIRMED

- Mac: `do_Person`, lines 106712–106721 (end of the movement block).
- 68k: `FUN_000e3a58` ~123026–123030.

```c
// 68k
vx = sin[h] * speed >> 8;            vy = cos[h] * speed >> 8;
// Mac
vx = (vx + (speed * sin[h] >> 7)) / 3;  vy = (vy + (speed * cos[h] >> 7)) / 3;
```
Steady state is the same (v = sin·speed>>8), but each frame only closes 2/3 of the gap. Effects:
- Starting/stopping/turning are slightly softer (the old velocity bleeds out over ~3 frames).
- Anything that changes a person's velocity outside this line now persists: the wall-bounce sign
  flips at `LAB_101414c4` (`|vx|` with the collision side), and the push/momentum from
  `CheckForCollisions` (car hits). On Amiga the next frame overwrites them, so a person hit by a
  car is only pushed by the position correction; on Mac the push decays over a few frames, so
  people are visibly knocked along.
- Note: per frame, not multiplied by ticks.

Classification: new behaviour (feel / knock-back). Impact: `people.rs::move_people` sets
`obj.vel` directly from speed; `contact.rs` adds impulses to `vel` that are then lost. Recommend
`vel = (vel + (sin*speed >> 7)) / 3` (Rust `/` truncates toward zero like the PPC `divw`).
Replays change.

### M3. People models re-based so the feet are at the origin (data change) — CONFIRMED

- Data: every `Objects/People/*.Obj` differs. Vertex X/Z/limb and the face data are identical;
  every vertex Y is shifted by −max Y so the feet sit at Y = 0 (MaleNormal −90, most others −92,
  PedMale4 −100; MaleNormalBazooka also X +2). Checked with a vertex diff against `extracted/game`.
- Code: Mac `DrawObject` @10075880 (lines 31485–31496) keeps the Amiga people transform
  unchanged (`if person && pitch==0: Y += 0x5dc`; depth `>>10`; horizontal `/3`), and the loader
  keeps the same people constants (radius = MaleNormal/3, mass 90/120/100/95, codes; lines
  21997–22202). `Human.motion` is byte-identical.

Effects:
- Standing: feet are drawn on the floor (Amiga: hip on the floor, feet ~360 z below). Resolves
  people.md §9 open question "feet below floor": the Mac fixed it in data.
- Lying (dead/stunned, pitch 0): Y + 1500 now spans about −1380..+1500 (×16 units) instead of
  +92..+2940, so a body lies roughly centred on where the person stood instead of extending ~1 m
  forward.
- Logic is unaffected (z is the floor in both; collision uses radius and z only).

Classification: bug fix (visual). Impact: `people.rs` render helpers (~line 1071 `standing`
uses a fixed `90 - y`; `lying` uses `y*16 + 1500`). Recommendation: shift each model by its own
max Y (PedMale4 is 100, others 92), and apply the same shift before the lying +1500 to match the
Mac. Or load the Mac `.Obj` files when present.

### M4. Punch crate check bounds-checks negative squares (bug fix, minor) — CONFIRMED

- Mac `do_Person` lines 107446–107453: `if (-1 < sx && sx < 0x80) ... if (-1 < sy && sy < 0x80)
  CheckForCrate(...)`.
- 68k ~123683: only `sx < 0x80 && sy < 0x80` (negative indices pass).
- Only reachable within 512 units of the west/north map edge. Port: check the punch/crate code
  uses unsigned or `0..128` bounds. No behaviour change otherwise.

### M5. Collisions (from the collision sub-investigation) — CONFIRMED

- **Vehicle wall probes clamped to the map.** Mac `CheckForCubeCollision` @100af980 clamps the 9
  vehicle probe points to [0, 0x3ffff] and sets the collision flags in the normal path; 68k
  `FUN_00044f82` / OS4 `FUN_01070868` only clamp in the `param != 0` path, so off-map squares
  (−1, 128) reached the pickup lookups and the edge didn't count as a hit. Bug fix. Port:
  `collide::resolve_walls` already clamps and blocks off-map squares; nothing to do.
- **Destroyed remote bomb.** Mac `DoCollideStuff` @10067b50: when the destroyed object is
  bomb.obj, clear the owner's planted-bomb slot (+0x860, 68k +0x37e) and `Detonate(i, 50000)`;
  68k `FUN_000dc5ac` detonates at mass/2 and leaves the slot stale. Bug fix / new behaviour.
  Port: bombs are `Projectile`s that `contact.rs` skips; if they become damageable, follow the Mac.
- **`GetCollideInfo_alt` @100aea60 / `LineOfSight_alt` / `GetSafePos` @100b42b0** exist only on
  the Mac (no glass breaking, water at level 0 blocks). `GetSafePos` has no callers: dead code.
- Platform artefacts only: double `sqrt` vs 68k float library (±1), `GetCubeNumFast`
  out of line, model padding.

Twins found (new): `CheckForCubeCollision` = 68k `FUN_00044f82` = OS4 `FUN_01070868`;
`GetCollideInfo` = `FUN_000446b8` = OS4 `FUN_010700b8`; `GetCubeHeight` = 68k `FUN_000920de`;
`GetCubeInfo`/`GetCubeInfo2` = `FUN_00044378`/`FUN_000444d0`; `LineOfSight` = `FUN_0009217e` =
OS4 `FUN_010707f0`; `AddCollideObj` = 68k `FUN_000cdfdc` = OS4 `FUN_01045288`;
`RemoveCollideObj` = `FUN_000cefa0`; `RemoveAllCollideObj` = `FUN_000cf462`; `GetCollideObjs` =
`FUN_000cf08c`; `CheckForCollisions` = 68k `FUN_000de0b6` = OS4 `FUN_01056c58`;
`DoCollideStuff` = 68k `FUN_000dc5ac` (the post-collision death/despawn/burn/gib pass, not
object–object contact).

### M6. Boarding / exiting — no differences (from the boarding sub-investigation) — CONFIRMED

`GetInOutVehicles`, `GetOutVehicle`, `GetOutOfVehicle(2)`, `SetObjectDest_IntoVehicle` (= 68k
`FUN_000e361e`), door closing, police leaving cars, cop arrest: every constant, condition and
order matches OS4/68k. The only mismatches are decompiler artefacts (an OS4 call site missing the
5th `AddLabelPercent` argument, `void` signatures, signed-division idioms).

---

## 2. Quirks shared by all builds (Mac did NOT fix them)

- Police arrest with the player on foot but a respawn timer running takes the "in a car" branch and
  "opens the door" of the person (Mac 105568–105607 = 68k ~121955–122000).
- Arrest RC kill writes `obj[0].health = −71` instead of the RC object (68k
  `DAT_0030c318+0x34a`, Mac `*piVar6+0x82c`).
- Carjack counter is incremented and then overwritten by the stats copy (dead store): Mac
  `veh+0x948` 24579→24631, OS4 `puVar33[0x118]`, 68k +0x45c. Hijacks never count.
  people.md §6.1 step 3 should say so.
- Target-pick door point is 192 units *inside* the car's right side (people.md §6.1 wording reads
  as outside).
- Player number written to two different fields on walk-in (+0x4b0) vs boat/exit (+0x4b4).
- AI steer asymmetry (`< 0xc9` in one branch, `< 200`/`< 0xa0` elsewhere), kept.
- Scream on impact plays at the other object's position when the first object is hurt.
- Footstep cube read for water happens before the map bounds check (both builds).

---

## 3. Port deviations found on the way (identical in Mac and Amiga)

people.rs / ai.rs:
- `ped_ai` flee: the original throttles only when the angle error is < 200 (shared AI block, then
  released above 0x18). Ours throttles whenever speed < 0x18, even while turning.
- `Person.wander` starts at −1; the original constructor sets T = spawn heading (Mac
  `CreateObject` `puVar3[500] = param_4`; OS4 `puVar3[0xbc]`). Moot if M1 is ported.
- `animate_people` plays no footsteps when walking backwards; the original also checks N ∈ {0,6}
  in the t < 0 branch (Mac 106448; 68k ~122773).
- Order: the original moves with last frame's velocity (`MoveObject_General` / `FUN_000e0e46`
  runs before `do_Person`) and computes the new velocity at the end; ours computes velocity then
  moves in the same tick (one tick less latency).

collide.rs / contact.rs (from the collision sub-investigation):
- `entry_blocked` blocks face type 5; the original lets {0, 5} through.
- `level_at` uses AND where `GetCubeNumFast` uses OR (enter if `zh >= top_h || (!solid_flat &&
  zh >= base − h2)`), so slopes/water are detected too low.
- `resolve_person`: both-sides-blocked pushes 0 (original −0x80, the later probe wins); no
  "beyond the previous square" restriction; the second phase should be one 2D segment test from the
  previous position, reverting flagged axes until clear.
- Glass is never broken (original: any non-person object crossing a type-3 face deletes the cube,
  sound 0x31).
- Scenery circle collision (`AddCollideObj`) not implemented.
- Contact damage: in the original, the "dv" terms are the averaged push-correction vector (z
  doubled then halved), vehicle shift >>5; no sub-stepping; "Irony bonus!" (2027) missing.
- `floor()`: `z/16` vs `z>>4` for negative z; unknown slopes fall back to Flat (original uses
  RVert); off-map returns None (original keeps z at level 0).

boarding.rs / pursuit.rs (from the boarding sub-investigation):
- Hijack doesn't reset the wanted-decay timer (original sets it to 0).
- Parked cars that still have the AI-driver flag (e.g. police car its cop left) should give the
  state-5 hijack branch (wanted ≥3/≥2, score 1500/1000 × mult, no eject).
- Exit lateral offset is `−(maxX + 230)`; original `−1.2·(maxX + 192)`; `pursuit.rs` drop-off
  uses `max_y/2` instead of `door_y`.
- Target pick uses `door_point(extra = 192)` (outside); original is 192 inside.
- Player walks straight to the door; the original uses waypoint 5 + contact re-plan + corners,
  arrival is a circle (`(r/12)<<5`), and the door target is the bare side.
- Boats: original boards instantly within 4·r and |dz| ≤ 0xbff, else nothing; exit needs the
  1.6L left/right water test.
- No burn-timer check before entering; `close_doors` only for the player; helicopter exit is not
  the three-press cycle.

Doc corrections for people.md: §5.2 z gate is `|floor_under_object − z| ≤ 500`; scenery sub-steps
every other tick; a vehicle destroys the snowman (5 snow effects). §5.1 the segment walk runs from
the second point to the first; glass is broken by any class ≠ 1 (vehicles too).

---

## 4. Identical (checked)

`do_Person` vs 68k `FUN_000e3a58` people branch:
- Stun decrement, dead/stunned → stick zero, speed 0, pitch 0; else pitch 0x5a0.
- Scripted attack 0x10/0x11: door-point distance (|dx|+|dy|)/2 < 6000, `rand(0x65 − aggr) < ticks`.
- Police on foot: wanted ≥ 3, stun/health gates, lead `−3·(5−diff)·vel`, arrest radii
  (`r>>3` in car, `(r+r)·3>>5` on foot, in >>4 units), |speed| < 30, same level, door +100/tick
  to 0x370, bust (0x50 ticks, phrase 0x7e1, stun 1e9, scream). Shooting `rand(N) < ticks`, N and
  wanted gates per difficulty identical.
- Follow-target (+0x904), states 0xd/0x13 → `SetObjectDest_IntoVehicle`, 0xe (vanish < 0x579),
  0xf (stop within +0x930), flee 0x12 (away heading, timer 100 → state 3).
- Speed caps per state (0x30 police/0xe/0x10/0x11, 0x20 0xf, 0x18 flee), AI max speed clamp,
  throttle cut within 1000 of the follow target.
- Player stick (accel/reverse/brake/left/right/fire) and CD track check.
- Wall-hit velocity sign flip and floor/level restore.
- Water: splash once (0x2b, mass·20000, 0x209e), sink 24/tick, bubbles 1-in-5 per corner,
  drown at z ≥ 27000 → respawn timer 0x50, phrase 0x7e2 "Drowned!", lives−1 in multiplayer
  path, RC kill, state 6.
- Turn rate ±15/tick to ±100, heading wrap; speed accel 24/6, caps `clamp(H+50, 24|6, 96|24)`
  and `clamp(H·70/100+35, 24|6, 64|16)`, coast 8/tick.
- Walk-cycle advance (`t += speed·ticks·3/2`, 3× loops, start frame 5, rest fade 64/tick),
  footsteps on frames 0/6 (splash 0x41 in water, surface sound, snow footprints at ±cos/2,∓sin/2).
- Weather (rain splash) timer 0xb4; weapon fire code (punch 30 damage, ±3r box, |dz| < 250,
  line of sight at z−0x400, sound 0x3b, kill → state 7 / respawn 0x50 / frag count).

Others:
- `MainLoop` @1014a920 vs 68k `FUN_000ec686` tail: health regen +1 every 250 ticks for health
  1..99 (on-foot or stored), wanted decay every `difficulty·100 + 150` ticks.
- `ScareDrones` / `ScareDrones2` vs OS4 `FUN_010435f4` / `FUN_01043788`: state 3, person, not
  police, (d/8)² < 0x17d784, scream, state 0x12, timer 100, target = player.
- `AddThing` vs OS4 `FUN_01047af4`: camera-window exclusion, square 1..126, cube present, top face
  not 0/4, 5-square occupancy, road-option count + 0x20 option, ped 1-in-5, PedMale1–4 (index 7
  skipped), heading rand(0x167f), pitch 0x5a0, state 3, level, z = top − 0x200, prev z, on ground.
  (Only addition: the wander anchor, M1.)
- `ReadMotion` vs 68k `FUN_0009508a` (12 × 14 × 2 ints, zero fill on failure); data identical.
- `ReadInput` @10064bf0 vs OS4 `FUN_01051a10`: replay bit layout (0x4000..0x100, weapon <<3,
  10 = none, ticks & 7); weapon keys gated on health > 0 and stun == 0, ammo > 0, different
  weapon, sound 0x14.
- People loader constants (radius /3 from MaleNormal, +0x812c /3, masses, weapon codes, police
  flag) identical.
- Collision functions other than M5 (see the sub-report summary in §1 M5 and the full identical
  list: probe height, ±0x80 push, ceiling vz = 2|vz|/3, clamp, 64 step, slopes, water surface,
  gravity 32, broad phase, sphere offsets, `(ra+rb)·17/16`, damage formula and shifts, run-over
  thresholds, speed blend `(8s + s·m_b/(m_a+m_b))/9`, despawn states {3,5,7,10,11,12}).
