# Mac OS X build vs Amiga: gameplay comparison (part 2)

Continuation of [`gameplay.md`](gameplay.md), which has the summary (§0), timing/FPS (§1), RNG (§2),
parameter tables (§3), vehicles (§4) and people/boarding/collisions (§5). Same conventions: the Mac is
MORE DEFINITIVE, so Mac changes are adopted; CONFIRMED = seen in Mac and at least one Amiga decompile.

---

## 6. Mission engine and script API

(Detail: `gameplay_missions.md`.)

**Decompile gap.** The current `analysis/ghidra_macx` export is missing most level-script code.
- Every `ProcessTrigger_XX`, `SetTrigger_Long`, `ProcessTrigger_Random` and `DoKidnap` dispatches
  through TOC-based jump tables that Ghidra did not follow, so the case bodies sit outside any function.
- Example: `ProcessTrigger_00` is exported as 2,840 bytes but is really 22,312; `SetTrigger_Long` is
  104 bytes in the export but really 4,856.
- The sub-agent recovered all 36 tables with switch overrides; the output is outside the repo.
- **Re-export `analysis/ghidra_macx` with jump-table overrides.** Input and movie code are affected too.

### M1. Level 21 soft-lock fixed (bug fix), CONFIRMED

- Mac `InitScript_21` @10129d20:
  ```c
  CreateActor(50, 0, 0x26400, 0x17000, 0, 0, 4);
  AddTrigger_script(100, 8, 50, -1, -1, -1);   // get into car 50 -> mission 1
  AddTrigger_script(50, 9, 50, -1, -1, -1);    // NEW: car 50 destroyed
  ```
- The handler in `ProcessTrigger_21` @1013b2b0 then runs:
  1. `RemoveObjectAtWill(50)`;
  2. slot = 1;
  3. `StopStoredTriggers`;
  4. Say "You idiot!" (5);
  5. `Announce(2028 "Mission failed!")`;
  6. `FinishMission_21()`, which opens phones 200/300/600.
- 68k init `0xb9aaa` (OS4 `0x010ebbe8`) has no such trigger. If car 50 was destroyed before the player
  got in, the level could never progress.
- **Adopt** in `crates/payback/src/mission/levels/level_21.rs`:
  - add the trigger in `init()`;
  - add an event-50 arm in `start()`;
  - update `docs/re/missions_21.md`.

### M2. Actor triggers bind by script id (robustness / redesign), CONFIRMED

- Mac `SetTrigger_Long` @10106e50 / `CheckForTriggers` @100f4a50 / `ObjectDead` @100f6c40 store the id
  and resolve it every frame with `GetObject(id)` @100530e0, which returns the first actor whose
  `+0x968` matches.
- 68k `AddTriggerEx` `0xb2b12` adds one trigger per matching actor and stores the pointer; evaluation
  (`0xa4a68`) dereferences it.
- Effects:
  - no stale pointers after an actor is removed;
  - **released actors (id cleared to −1) stop matching**;
  - with duplicate ids only the first actor is watched. The story scripts never share a live id
    (INFERRED: no story-visible change).
- Type 0x43 and the order types still affect every matching actor.
- **Adopt:** in `mission/api.rs` / `eval.rs`, resolve the actor through its current script id at
  evaluation time, or check that the bound entity still carries that id.

### M3. Trigger evaluation in Rampage (new), CONFIRMED

- Mac `amiga_levelloop`:
  ```c
  if (!paused && GetNumAltPhones() > 0 && !multiplayer && !ending)
      r = CheckForTriggers(...);
  ```
- 68k `FUN_000ec686`:
  ```c
  if (... && !rampage && !multiplayer && !ending)
      r = Trigger_Update(...);
  ```
- On `r == 2` in Rampage the message is phrase 0x707 (1799) instead of 2023. Phrase 1799 is missing
  from the Mac phrase file (unresolved).
- **Adopt with M4:** `mission/mod.rs` `start_level` / `eval::run_triggers` evaluate triggers in Rampage
  on maps with at least one phone.

### M4. Random missions in Rampage (new Mac feature), CONFIRMED in code

Functions:
- `InitScript_Random` @10129e50, `ProcessTrigger_Random` @1012a0a0, `FinishMission_Random` @1012a020,
  `GetPointLocation` @10129f50;
- helpers `GetPhoneCoords(-1)` @100b4540 (random phone), `GetSafePos`, `EstimateTime(2)`
  @10057c60/@10057d60, `GetRandomVehicleNum(_Pursue)`, `GetRandomColour`, `GetHighestScore` @100a05a0,
  and a new formatted say `MessageThem_Parse` @101087d0 / `FlushOldMessages` @10108730.

How it runs:
1. **Start.** `amiga_startlevel` calls `InitScript_Random` in Rampage when the map has phones. City
   index 3 routes `ProcessTrigger` to the random script.
2. **Goal.** The money goal is the top Rampage high score for the chosen police level P (0–5).
3. **Phone.** A random phone rings: "ANSWER THE PHONE" (1400), "Welcome to %s. Prepare to rampage!"
   (1401).
4. **Pick.** Answering picks one of 7 jobs, `Random(7)+1`:
   - steal and deliver a car;
   - kill a gang;
   - a 5-checkpoint race;
   - kill N cops;
   - destroy a watching vehicle;
   - survive at wanted 6;
   - earn X points against the clock.
5. **Scaling.** Timers, payouts and wanted levels scale with P.
6. **Result.** A win says 1404–1406, pays `k·1000` and shows 2022. A failure says 1407–1409 and shows
   2028. Then a new phone rings.

The full per-job table is in `missions.md` §3. The text is in global phrases 1400–1470 and 1800–1838,
which exist only in the Mac phrase files.

- Neither Amiga build has it.
- **Recommendation:** implement as `mission/levels/random.rs` for Rampage on maps with phones (Mac
  behaviour, so adopt). It needs M3, the formatted say, random-phone selection, `GetSafePos`,
  `EstimateTime`, and the Mac phrases.

### Checked and identical (CONFIRMED)

- **Trigger engine:**
  - 500 slots and the to-do list (`ProcessToDoList` @100f4880 = `0xa4920`);
  - `RemoveTrigger` swap-remove, so the swapped-in entry is skipped for one frame;
  - every trigger type's radii, angles and state tests;
  - guard sight: 6e6/16e6, ±90°, LOS. The Mac only adds a null check on the watcher;
  - door and phone-flash logic, the countdown beep, arrow cycling, the type semantics at add time.
- **Script API:** every function matches its 68k twin call for call (`missions.md` §2.3 has the pairs).
- **Level scripts 00/01/10/11/20/21:** all constant-argument calls match apart from M1:
  - mission order, spawns, timers, rewards, wanted levels, map edits;
  - the level-20 guard and roof tables and the level-10/21 race tables, byte for byte.

---

## 7. Combat, police and scoring

(Detail: `gameplay_combat.md`.)

**No gameplay difference.** The Mac differences are robustness only:
- the guard-sight watcher is looked up by id with a null check (`CheckForTriggers`);
- `CheckForCrate` @100593e0 bounds-checks inside itself, with the same effect as the 68k.

Identical function pairs:
- `CheckHitObject` @100542d0 = Crime (68k `FUN_000ccc96` / OS4 `FUN_01043924`);
- `Shockwave` @10054ac0 = Explosion (`FUN_000cd22e` / OS4 `FUN_0104405c`);
- `Explode` @10055570 / `Detonate` @100550c0 / `Explosion` @100558f0;
- `do_Weapon` @1013e390 (projectiles, grenade, bomb, missile);
- `do_Person` firing and AI combat;
- `DoVehicleFire` @10070c20 = `FUN_000e2a50`;
- `AddPolice` @10058ba0 = `FUN_000cfe0c` (roadblocks);
- `CheckForWeapon` @100597e0 / `RespawnWeapons` (1200 ticks);
- `ScareDrones`;
- wanted steps, copkiller, the multiplier cap 4, score caps;
- `AddHealth`, `AddPlScore`, `SetAggressiveness`.

Damage from ground fire and gas: none in any build (`UpdateEffects` touches no object).

---

## 8. Bug fixes: summary

These look like fixes relative to the Amiga code:

| Fix | Mac function | Amiga | Status |
|---|---|---|---|
| Map edge counts as a wall on the full sub-step (V6/D1) | `CheckForCubeCollision` @100af980 | 68k `FUN_00044f82`, OS4 `FUN_01070868` | adopt |
| Destroyed remote bomb detonates and clears the owner's slot (C1/D2) | `DoCollideStuff` @10067b50 | `FUN_000dc5ac` / `FUN_010542b8` | adopt |
| Level-21 soft-lock (M1) | `InitScript_21` / `ProcessTrigger_21` | 68k `0xb9aaa` / `0xc79cc` | adopt |
| Stale actor pointers in triggers (M2) | `SetTrigger_Long` / `CheckForTriggers` | `0xb2b12` / `0xa4a68` | adopt |
| Low-skill AI steering while braking (V3) | `do_Other` @10146280 | `FUN_000e3a58` ~124855 | adopt |
| Junction radius no longer quadratic (V2) | `MoveUnit` @10081080 | `FUN_000a143e` | adopt |
| Light-bar reset without a siren handle (V7) | `do_Other` tail | ~125860 | adopt (minor) |
| Null-check on the guard-sight watcher; crate bounds in the callee | `CheckForTriggers`, `CheckForCrate` | `FUN_000a4a68`, OS4 `FUN_01040ce4` | no port impact |
| Wall-hit damage overflow guard | `BounceUnit` | 68k has it, OS4 lacks it | no port impact |

---

## 9. Documented quirks the Mac did NOT fix (keep them)

**Vehicles:**
- the `policevan_red` brake/grip/mass typo;
- airborne wheel spin uses brake when ws < 1 (sic);
- skill ≤ 50 subtracts brake instead of accel;
- a fleeing car (0x14) that reaches its goal falls into state 11;
- state 8 is never written;
- police in 0x14/0x15 can be taken over by pursuit selection.

**Combat:**
- the roadblock cop's height is read with transposed indices (x-runs);
- a missile into scenery doesn't explode;
- any non-person, projectiles included, destroys a snowman;
- a punch pays 500 × multiplier without checking that the player threw it.

**Boarding:**
- the carjack counter is a dead store, so it never counts;
- the player-number fields differ between the walk-in and exit paths;
- the target-pick door point is 192 inside the car side;
- the RC kill on arrest hits obj[0].

**Missions** (full list: `missions.md` §2.3):
- 00 M1: the shotgun trigger 0x72 is not killed after 20 shots;
- 00 M3: re-entering the bus re-arms Harry's garage;
- 01 M5: the failure endings show "Mission complete!";
- 20: the group-A 0x30 trigger is not re-armed;
- 21 M5: the alive flags are never cleared, so Nick always targets 510;
- 21 M6: the east/west face mismatch;
- 21: ev 205 is dead;
- guard 300200 is missing its +0x400 offset.

**Loop:** `RemoveTrigger` swap-remove skips the swapped-in entry for one frame.

---

## 10. Corrections to our notes (true in every build)

- **`docs/re/people.md` §1.1:** recommend 30 Hz (§1 of part 1). The tick rate is per build, not "25 Hz
  is faithful".
- **`docs/re/vehicles.md` §0:**
  - 68k `FUN_000a1758` is PositionUnit (slope);
  - grip/skid is `FUN_000a197a`;
  - the sub-steps are `FUN_000e109e`;
  - the wall bounce is `FUN_000a27c6`;
  - the junction choice is `FUN_000a143e`.
- **`docs/re/vehicles.md` §2:**
  - slip is measured after the grip correction;
  - the downhill factor is the tick count (CONFIRMED);
  - the "flying model gets no input" note means a boat out of water;
  - the J-turn sign comment is wrong;
  - the ±0x5a wall heading deflection (N7) is missing;
  - tank reverse is −220 on the Mac.
- **`docs/re/combat_police.md`:**
  - §5.2: explosion damage is `q/k` for people **and vehicles**, `(q>>4)/k` only for class-2 props, and
    there is no knock-back for class 2;
  - §3.2: firearms counts only shotgun, bazooka, grenade, bomb and the tank cannon;
  - §6: health does regenerate;
  - §3.3: pellets make 3 larger sparks; the OS4 pistol filter is {−1, 7};
  - §2.4: the arrest distance is measured to the door point without the cop radius.
- **`docs/re/people.md` §4.5 and §5.1–5.3:** corrections from the collision sub-agent: face type 5 does
  not block, the level lookup is an OR, the push details, glass breaking.
- **`docs/re/script_api.md`:** trigger binding by id on the Mac (M2); triggers in Rampage (M3).

---

## 11. Port fixes (port vs original; Mac and Amiga agree)

These apply whatever the build.

**Timing and RNG**
1. Second RNG stream (`random3`) for effects, render and HUD (part 1 §2). File: `sim/mod.rs` `Rng`, and
   the effect users in `effects.rs` and `weather.rs`.

**Vehicles** (`sim/driving.rs`, `sim/collide.rs`)
2. Heading deflection of ±0x5a on wall hits (N7).
3. Slope pitch and downhill speed `speed += pitch·ticks >> 8` are missing (`PositionUnit`).

**Combat** (`sim/combat.rs`, `sim/police.rs`, `sim/score.rs`)
4. Explosion damage: `q/k` for vehicles; `(q>>4)/k` only for class 2; no push for class 2
   (`explosions()` ~1075). Expect more chain reactions.
5. Explosion score: `min(6000, (q/T)/(k>>5)) × multiplier` for player-owned blasts.
6. Firearms crime counter: never incremented. Count the shotgun, bazooka, grenade, bomb and tank cannon.
7. Cop arrest distance: to the target's door point (model +0x48f4/+0x48f8 equivalents), without the lead
   or the cop radius, using the real radii. Add the "player stunned" test (`cops()`).
8. Copkiller bonus: `100000 × multiplier`, not a flat 100000.
9. Shotgun pellets: 3 sparks of size `rand(150)+250`.

**Boarding** (`sim/boarding.rs`)
10. D1: a hijack must reset `WantedTimer`.
11. D2: a state-5 car with a driver still in it takes the wanted/score branch.
12. D3: the exit lateral offset is `(maxX+192)·6/5`; the cop drop-off uses `door_y`.
13. D4: the target pick uses extra = −192.
14. D5: the player walks to waypoint 5 with a corner re-plan, and arrival is a circle `(r/12)<<5`.
15. D6: boats board instantly within `boat.r/8<<5` with abs(dz) ≤ 0xbff. The exit water test is at
    1.6·L, left then right.
16. D7: no boarding while burning; `close_doors` for AI states; the helicopter 3-press cycle.

**Collisions** (`sim/collide.rs`, `sim/contact.rs`)
17. Face type 5 must not block.
18. `level_at` must OR the layer bits (`GetCubeNumFast`).
19. `resolve_person`:
    - the −0x80 push when both sides are blocked;
    - push only beyond the previous square;
    - a single 2D segment repeat.
20. Glass breaks for class ≠ 1 (vehicles and projectiles): clear the cube and play sound 0x31.
21. Scenery circle collision (`AddCollideObj`) is missing.
22. `contacts`: the damage base should be the averaged push correction including z. The Irony bonus
    (phrase 2027, 10000) is missing.
23. Minor `floor()` details.

After any of the simulation changes (part 1 §1, §4, and §11 here), re-record the replay baselines
(`tools/checks/replay_check.sh`).

---

## 12. Open items

- **`do_Person` movement and animation, Mac vs 68k `FUN_000e3a58` / `FUN_000e0e46`, has not been
  compared line by line.**
  - Already checked: firing, AI combat, regeneration, boarding, collisions.
  - Still unchecked: walk/run speed caps, turning, swimming and drowning, the animation frame advance,
    ped panic, and `ReadInput` stick handling.
- **Re-export `analysis/ghidra_macx` with jump-table overrides.** Then re-check `GetNextWeapon` /
  `GetPrevWeapon` / `GetWeaponAmmo`, input and the movie code.
- **Effect-size arguments past the 8th** (particle sizes) are hidden by the Mac decompiler and were not
  compared.
- **Readers of P1** (`+0x7d4/+0x7d8` on pavement peds) and **phrase 1799** (the Rampage goal message)
  are unresolved.
