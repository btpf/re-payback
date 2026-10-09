# Mac OS X build: follow-up pass after the jump-table re-export (Decision 13)

This pass continues [`gameplay.md`](gameplay.md) and [`gameplay_part2.md`](gameplay_part2.md),
whose §12 lists the open items. It uses the re-exported `analysis/ghidra_macx`.
`tools/ghidra/MacJumpTables.java` resolved the 73 CodeWarrior TOC jump tables, so the level
scripts, `SetTrigger_Long`, input, replay and effect code now decompile in full (see
`docs/re/mac_version.md` "Decompiling").

Conventions are the same as in the other notes:
- The Mac is the more definitive build.
- CONFIRMED means the code was seen in the Mac build and in at least one Amiga decompile.
- Addresses are Mac OS X addresses, then OS4 / 68k.

The detailed per-area reports are in this folder:

| Report | Scope |
|---|---|
| [`levels_00.md`](levels_00.md), [`levels_01_10.md`](levels_01_10.md), [`levels_11_20_21.md`](levels_11_20_21.md) | every recovered `ProcessTrigger_*` case, `InitScript_*`, `FinishMission_*` and the mission helpers, compared with `mission/levels/*.rs` |
| [`triggers.md`](triggers.md) | all 69 `SetTrigger_Long` cases and `DoKidnap`, compared with `mission/api.rs` and `eval.rs` |
| [`input_movie.md`](input_movie.md) | weapon cycling, input and the replay recorder |

The tools were local working files in `analysis/mac/scratch_followup/` (not kept):
- `callargs.py` lists every call to a Mac function with **all** of its arguments, including the
  stack arguments past the 8th that Ghidra hides. It reads the AIX/CFM stack slots
  `0x38(r1)+4i`.
- `callargs_os4.py` does the same for OS4 (SysV, `8(r1)+4i`).
- `addeffect_calls.tsv` and `os4_addeffect_raw.tsv` hold their output for `AddEffect`.

---

## 0. Summary

The Mac scripts and engine code recovered from the jump tables are the same source as the Amiga.
The only Mac change is the replay compression (I5). Comparing them against the port found these
port differences:

| # | Item | Class | Port today |
|---|---|---|---|
| E1 | `AddEffect_Gas` layer and blend ignored | port bug | `mission/api.rs:948-957` |
| E2 | Crate break: wrong dust puffs, and they draw from the sim RNG | port bug (RNG) | `sim/combat.rs:714-729` |
| E3 | Pickup collection: the 2 dust puffs (14 sim draws) are missing | port bug (RNG) | `sim/combat.rs:1334-1387` |
| E4 | Pickup respawn: the 2 dust puffs (14 sim draws) are missing | port bug (RNG) | `sim/combat.rs:1390-1414` |
| R1 | RNG order of random arguments taken from OS4 at four script sites | port bug (RNG) | `level_10.rs:559-571`, `level_11.rs:367-371`, `level_21.rs:768-772`, `level_21.rs:938-940` |
| S1 | Level 01 M3: car trigger add order | port bug (minor) | `level_01.rs:256-260` |
| T1 | Trigger 0x33–0x36 with a = −1 | port bug (latent) | `api.rs:75-79`, `eval.rs:333-336` |
| T2 | Trigger 0x02 uses the live actor, not a snapshot | port bug (latent) | `eval.rs:168-195, 279-280` |
| T3 | Trigger 0x3d added without a target vehicle | port bug (latent) | `api.rs:115-127, 144-147` |
| T4 | Order triggers: one per actor instead of one | port bug (latent) | `api.rs` order types |
| T5 | Pointer triggers added before the actor exists | Mac change | `api.rs` `actor_trigger` |
| I1 | Replay in/out bit treated as a pulse | port bug | `replay.rs:369`, `boarding.rs:175` |
| I2 | Weapon change allowed while stunned | port bug | `sim/combat.rs:249` |
| I3 | Number keys don't pick weapons in multiplayer | port gap | `multiplayer.rs:224`, `people.rs:411` |
| I4 | P doesn't stop replay playback | port gap (minor) | `replay.rs:386` |
| I5 | Mac 0x3F3 replays are a compressed bit stream | Mac change; the port can't read them | `payback-formats/src/replay.rs:136-238` |

"Latent" means no shipped script reaches the case today.

Effect sizes past the 8th argument (§1), `GetNextWeapon`/`GetPrevWeapon`/`GetWeaponAmmo`
(`input_movie.md`), `UpdateEffects` and `DoEffectsLighting` (§3), `DoKidnap`, and every level
script case not listed above match the port.

**A finding that cuts across several items: argument evaluation order (CONFIRMED).**
- When one call takes several `random()` arguments, the Mac (CodeWarrior) and the 68k build both
  evaluate them **last argument first**. OS4 (GCC) evaluates them first to last.
- Example: the crate respawn draws `random(1000)` first and `random(800)` (x) last on the Mac
  @ 10059674 and on the 68k `FUN_000d079e`. OS4 `FUN_01048c40` uses the reverse order.
- So the reverse order is how the original Amiga build behaves too, not a Mac change.
  `docs/re/mac/rendering.md` D8/D9 call it a "Mac change"; it should say that OS4 is the odd
  one out.
- Every port site that was transcribed from an OS4 call with several random arguments therefore
  draws in the wrong order (R1). This doesn't change the number of draws, so the RNG stays in
  step, but the values land on the wrong axes.
- Recommendation: follow the Mac/68k order everywhere. Audit the remaining multi-random calls in
  `sim/` the same way (the sites in `sim/combat.rs` that cite the 68k already do this).

---

## 1. Effect-size arguments past the 8th (`AddEffect` 100a7f80 / OS4 `FUN_01035da0` / 68k `FUN_0003d97a`)

`AddEffect(type, tex, x, y, z, vx, vy, vz, size, layer, blend)` takes 11 arguments. The Mac
decompile showed only 8, because arguments 9–11 go on the stack. `callargs.py` read all 112 Mac
call sites from the disassembly, and `callargs_os4.py` read the 99 OS4 ones.

**Result: no size, layer or blend differs between the Mac and OS4 at any matching site.**
CONFIRMED. I compared them function by function:
- `Shockwave`, `Detonate`, `Explode`, `Explosion`, `AddCollideObj`;
- `RespawnWeapons` and `CheckForWeapon` (size `1500+r(1000)`);
- `DrawMap` (`300+random3(200)`);
- `DoCollideStuff`: the flames are `radius·3/2 + r(300)` or `radius + r(150)`, scorch is
  `r(120)+120` and `r(50)+50`;
- `CheckForCollisions`: debris `100+r(50)`, sparks `150+r(150)`, `(speed·8+80)+r(…)`, 800;
- `MoveObject_General` (250), `DoVehicleFire` (400, blend 0x10000), `SkidUnit`;
- the level-start weather in `amiga_startlevel`: snow `100+random3(50)`, rain 1, list 3;
- `AdjustCubeNew`, `AddObjectSpecial`, `AddEffect_Gas`;
- the `do_Weapon`, `do_Person` and `do_Other` parts of OS4 `FUN_0105f594` (1300/800,
  `150+r(150)`, `250+r(150)`, blood 130 with blend 0x10000, `70+r(50)`, flame `100+r(150)`,
  `130+r(50)`).

The raw call counts differ in a few functions (`CheckForCollisions` 15 vs 17, `DoCollideStuff` 5
vs 4, `SkidUnit` 10 vs 2+3). These come from GCC merging or duplicating tails and from an
inlined `SkidUnit` helper. The code paths are the same.

The port's spot-checked sites match:
- `sim/contact.rs:214`: debris kind 9 `100+r(50)`;
- `sim/contact.rs:296, 313-314, 326`: scorch, explosion 2500+r(1500)/800 in list 3, burning
  `radius+r(150)` or `1.5·radius+r(300)`;
- `sim/combat.rs:894-929`: bullet sparks `r(150)+150` and `+250`;
- `sim/combat.rs:1324`: blood `120+r(120)`;
- `weather.rs:176-192`.

### E1. `AddEffect_Gas` passes its 7th and 8th arguments as size and layer, with blend 0x30000 (port bug), CONFIRMED

- Mac `AddEffect_Gas` @ 10108310, from the disassembly at 10108348-10108378, calls
  `AddEffect(9, GetGas(), x, y, z, vx, vy, vz, size = arg7, layer = arg8, blend = 0x30000)`.
  It stores r9 at `0x38(r1)`, r10 at `0x3c(r1)`, and `lis r0,3` at `0x40(r1)`.
- OS4 `FUN_010d13f4` is the same: it passes `param_7`, `param_8` and `0x30000`.
- The only caller is level 10 M200, event 0xcc (Mac case 10135cd8), which passes layer **1**.
- Port: `Script::add_effect` (`mission/api.rs:948-957`) hard-codes list **3** and `Blend::Add`.
  `level_10.rs:568` has a TODO about the missing 8th argument.
- Effect on play: the gas cloud is drawn in the top effect list instead of the street-level list.
  It is also additive instead of the tint table (0x30000, `s + d·(255−max s)/256`), so it looks
  brighter and is not occluded the same way.
- Recommendation: give `add_effect` a `layer` parameter and use `Blend::Tint`. Pass `1` from
  level 10. Also apply R1's draw order to that call (`level01_10.md` item 2). Render-only, so
  replays are unaffected.

### E2. Crate break (`CheckForCrate` 100593e0 / OS4 `FUN_01040ce4` / 68k `FUN_000d0670`) (port bug), CONFIRMED

- All three builds call `Bang(crate model, layer+1, centre x, centre y, z, angle, 5)`. The Mac
  `Bang` @ 100a6dd0 (OS4 `FUN_01035730`, 68k `FUN_000384e8`) first calls
  `Split` (the crate fragments).
- `Bang` then adds `max(2, min(6, 5>>1)) = 2` type-3 Dust puffs, each:
  - x = cx + r(800) − 400, y = cy + r(800) − 400, z = z + r(300) − 150;
  - velocity (r(500) − 250)/3 on each axis;
  - size 1500 + r(1000), blend 0.
- All 7 draws per puff come from **`random3`**, the cosmetic stream (OS4 `FUN_0103acc4`, 68k
  `FUN_0003fce6`, 7 calls in each). Order on the Mac/68k: size, vz, vy, vx, z, y, x.
- Port `break_crate_at` (`sim/combat.rs:714-729`) adds **3** puffs at the square centre with zero
  velocity, size `1500 + rng.rand(1000)`, drawn from the **sim** `Rng`. That is 3 extra
  gameplay draws per crate break, which shifts every later sim random, and the visuals are wrong
  too.
- I did not check whether the port shows crate fragments.
- Recommendation: replace the loop with a `bang` helper that uses `VisualRng`, adds 2 puffs with
  the jitter above and spawns the fragments. Re-record the replay baselines.

### E3. Pickup collection dust (`CheckForWeapon` 100597e0 / OS4 `FUN_01048ee4` / 68k `FUN_000d0988`) (port bug), CONFIRMED

- On a pickup, the original adds 2 type-3 Dust puffs before the sound. The loop runs
  `i = 1; do … while (i < 3)`, and the 68k uses `dbf` with count 1. Each puff:
  - x = sx·2048 + 0x270 + r(800), y = sy·2048 + 0x270 + r(800);
  - z = obj z + r(300) − 150;
  - velocity r(500) − 250 on each axis;
  - size 1500 + r(1000), layer from the object.
- The draws use the **gameplay** `random`: 7 per puff, 14 per pickup. Order (Mac/68k): size,
  vz, vy, vx, z, y, x.
- Port `pickups` (`sim/combat.rs:1334-1387`) spawns no effect and makes no draws, so the sim RNG
  falls 14 draws behind the original on every pickup.
- Recommendation: add the two puffs, drawing from the sim `Rng` in that order. Re-record the
  baselines.

### E4. Pickup respawn dust (`RespawnWeapons` 100595c0 / OS4 `FUN_01048c40` / 68k `FUN_000d079e`) (port bug), CONFIRMED

- When the 1200-tick respawn falls due, the original adds the same 2 puffs:
  - x and y = square·2048 + 0x270 + r(800);
  - z = **0x4d8a + r(300)**, a fixed height;
  - layer = min(stored layer + 1, 3) on OS4.
- Again 14 **gameplay** draws, then `AddSound(0x2f, 0, 20e6, 8000, centre, 20000)`.
- Port `respawn_pickups` (`sim/combat.rs:1390-1414`) plays the sound but spawns no puffs and
  makes no draws.
- Recommendation: as E3.

---

## 2. Level scripts and the trigger engine (recovered `ProcessTrigger_*`, `SetTrigger_Long`, `DoKidnap`)

Every case of the six story handlers was compared with the port, event by event. The comparison
covered:
- spawns, trigger types and arguments, timers, money;
- phrases and speakers, wanted levels, map edits;
- slot and flag logic, and RNG order.

The Mac matches the Amiga apart from the known level-21 car-50 trigger, which the port already
has (`level_21.rs:127-130, 215-222`). The port differs only in the items below. Details and the
list of everything that was checked are in the three level reports.

- **R1. RNG argument order transcribed from OS4 (port bug), CONFIRMED (Mac and 68k agree).**
  The random calls are the same in number, but each value lands on the wrong axis:
  - level 10 event 0xcc gas puff: Mac 10135cd8; port `level_10.rs:559-571`;
  - level 11 M1 event 0x6a bomb explosions: Mac 10137f88; port `level_11.rs:367-371`;
  - level 21 M7 event 740 truck explosion: Mac 1013d194; port `level_21.rs:768-772`;
  - level 21 M8 event 830 phone explosion: Mac 1013d750; port `level_21.rs:938-940`.

  Levels 00 and 01 already use the Mac/68k order (`level_00.rs:756-758`, `level_01.rs:911-914`).
  Also add the draw order to the pseudocode in `docs/re/missions_11.md:257` and
  `missions_21.md:571/604`.
- **S1. Level 01 M3 phone 300: trigger add order (minor port bug), CONFIRMED.**
  - The Mac, 68k and OS4 add the car triggers grouped by kind: 0x137–0x139, 0x141–0x143,
    0x14b–0x14d, then 0x154.
  - The port adds them car by car (`level_01.rs:256-260`).
  - This only changes the order of events that fire on the same tick, such as boss lines 23/25.
- **Level 20 event 200000**: the guards are released in a different order (`level_20.rs:588-590`).
  It's the same set, so there is no effect (INFERRED).
- **No-ops and stale comments**:
  - `level_00.rs:355` passes −1 where the original passes 0x2ee, to a type that ignores it.
  - Stale "no range argument" TODOs at `level_00.rs:1133` and `level_01.rs:407`.
  - The comment at `level_10.rs:108` is inaccurate.

Trigger add time (`SetTrigger_Long` @ 10106e50, 69 cases, vs `AddTriggerEx` 68k `0xb2b12` /
OS4 `0x010cfa80`). See `triggers.md`:
- **T1 (port bug, latent).** Types 0x33–0x36 with a = −1 become 0x1e–0x21 **and** move the
  threshold p4 into the first slot. The port only changes the type (`api.rs:75-79`), so
  `eval.rs:333-336` compares against −1. Mac cases 101075b4, 101074f4, 10107494, 10107554.
- **T2 (port bug, latent).** Type 0x02 snapshots the actor's x, y and floor once, when the
  trigger is added. The port follows the live actor, skips the floor check, and drops the
  trigger when the actor goes (`eval.rs:168-195, 279-280`). Mac case 1010704c.
- **T3 (port bug, latent).** Type 0x3d adds nothing when the target vehicle id has no actor. The
  port adds a trigger that can never fire (`api.rs:115-127, 144-147`). Mac case 10107a38.
- **T4 (port bug, duplicate ids only).** For order types 0x0c, 0x12, 0x17, 0x32, 0x3d and 0x44,
  the order goes to every actor with the id, but **one** trigger is added. The port adds one per
  actor.
  - Radius detail: the Mac always uses 1000 for 0x17, and the script's value as-is for 0x32.
  - The port's `d > 0 ? d : 1000` (`api.rs:137-140`) mixes the two.
- **T5 (Mac change, refines `gameplay_missions.md` §1.1).** Pointer-type triggers are
  added even when no actor has the id yet. They come alive when one appears, including a
  re-created actor. Pairs with the existing recommendation to resolve actors by id. It only
  matters for scripts that were dead code on the Amiga (INFERRED).
- `DoKidnap` @ 101280a0 (level 11 M3, with `TriggerAlarm2`) is the same in all three builds.

---

## 3. Other recovered jump-table bodies

| Function (tables) | Finding |
|---|---|
| `UpdateEffects` 100aa1f0 (per-type update, 16 cases; 8684 → 11000 bytes) | Matches `effects.rs:251-317` for every type. Size decrements are 1:−20, 2:−3 + gravity, 3:−57, 4:−23, 6:−13, 7:−66, 8:−1, 9:−7, 11:−5 + gravity, 15:−2. Type 10 grows by 300, frame = size/2500, and dies past 3. Types 2 and 15 spawn flames and types 4 and 7 spawn smoke, with the same sizes, jitter and `random3` order. The flame-frame cycle is the same. Types 12–14 wrap. CONFIRMED. |
| `UpdateEffectsMulti1` 100a8580 | Split-screen variant of the same switch; not compared beyond the case targets. |
| `DoEffectsLighting` 100a82b0 (new: 156 → 668 bytes) | Effect lights: type 2 adds size·(140000, 120000, 100000) + `random3`; type 7 adds ·(50000, 40000, 30000) + `random3`; type 11 adds ·(900000, 900000, 700000); type 15 adds ·(100000, 70000, 50000) + `random3`. Gated on the effects-lighting option `_DAT_10188fc0`. The port already has this (`lighting.rs:771-820`). CONFIRMED. |
| `GetNextWeapon`, `GetPrevWeapon`, `GetWeaponAmmo` 100f7d00–100f7ee0 | Identical to the Amiga and the port's `cycle_weapon` (`input_movie.md`). |
| `UpdateInputStatus` 100f8490 (472 → 3848 bytes), `WriteControlName` | I2, I3 and I4 above; the rest is platform input (HID/ISp, no autorepeat). |
| `GetNextFrame` 100f9ea0, `GetAbbreviatedVals` 100fa2b0 | The replay bit-stream codec (I5). |
| `DrawAffine*` / `DrawPerspective*` triangle fillers, `API_3D_*`, `W3D_SetBlendMode` | Software rasteriser and the Warp3D-on-OpenGL layer. Platform artefact. |
| `HandleAppleEvent`, `RepositionWindow`, `FUN_1015d640`…`FUN_1016b7c0` | PowerPlant, the Toolbox and the MSL C runtime (`printf`/`scanf` format switches). Platform artefact. |
| `ProcessTrigger_Random` 1012a0a0 (2 tables) | Now complete in the export, for the "missions" agent (`docs/re/missions_random.md`). Not re-compared here. |

## 4. Input and the replay recorder

Summary of `input_movie.md`:

- **I5 (Mac change; the port can't read Mac replays).**
  - The Mac 0x3F3 records are a bit stream: run-length coded repeats, and short codes for
    single-field changes. The comment bit 15 is gone. The Amiga stores raw u16 words.
  - Four of the Mac's codes are lossy. For example, a weapon pick is re-sent on the next
    unchanged frames.
  - The port reads and writes raw words (`payback-formats/src/replay.rs:136-238`).
  - Recommendation:
    - decode the Mac format exactly as the Mac does, losses included;
    - write it without the lossy shortcuts;
    - mark the format in the `PBrm` trailer, so existing baselines still load the old way.
  - No Mac replay ships, so the decoder is untested against real files (INFERRED).
  - The notes that call the records "unchanged" need fixing: the header of `replay.rs`,
    `docs/re/frontend_menus.md:368` and `frontend.md:455`.
- **I1 (port bug; Amiga agrees).** Replay bit 9 (in/out) is the *held* key. The port treats it
  as a one-tick press, so a held bit starts boarding, cancels it, and starts again
  (`replay.rs:369`, `boarding.rs:175`). Act on the rising edge during playback.
- **I2 (port bug; Amiga agrees).** Weapon select needs health > 0 **and** the stun timer at 0.
  The port checks only health (`sim/combat.rs:249`). This may move the baselines.
- **I3 (port gap).** Digit keys 0–7 don't pick weapons for keyboard players in multiplayer
  (`multiplayer.rs:224`, `people.rs:411`).
- **I4 (minor).** P also ends replay playback in both originals (`replay.rs:386`).
- **Mac bug, don't copy.** The HID next/previous-weapon latch is shared by all pads.
- **`symbols.tsv` corrections.**
  - `GetNextWeapon`/`GetPrevWeapon` are OS4 `0103d2a8`/`0103d438` (68k `000409f2`/`00040ad2`).
  - The writer `AddFrame` is OS4 `010bbd58` / 68k `000947e4`. The listed pair
    (`010bbcb8`/`0009490e`) is the frame reader.
- **Intro film.** `Videos/payback_high_mpeg1.mpg` plays at start-up and from "View intro",
  scaled to fit, and any key or click skips it. This matches `intro.rs`.

## 5. Replay determinism

E2, E3, E4, I2 and every R1 site change the simulation RNG sequence. After fixing them,
re-record the replay baselines (`tools/checks/replay_check.sh`). E1, I1 (playback of
original files only), I3 and I4 don't affect the port's own replays.
