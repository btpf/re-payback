# Trigger add-time (`SetTrigger_Long`) and `DoKidnap`: Mac OS X vs Amiga vs our port

Sources compared:

- Mac (`analysis/ghidra_macx/decompiled.c`):
  - `SetTrigger_Long__Fiiiiiii` @ 10106e50 (l.102727). Its jump table 101904c0 has 69 entries,
    index = trigger type.
  - `DoKidnap__Fi` @ 101280a0 (l.106743). Its table 10190a30 is indexed by event − 0x12d (65
    entries).
  - Also read: `SetTrigger__Fiiiiii` @ 10106d40, `AddTrigger__Fiiiiisiii` @ 100f4490,
    `AddTrigger_script` @ 10124200, `RemoveTrigger_script` @ 10124170, `GetObject` @ 100530e0,
    `TriggerAlarm2` @ 10127fd0, and `CheckForTriggers` @ 100f4a50 (for type 2 only).
  - Ghidra drops arguments that are passed straight through in registers, and the 9th
    `AddTrigger` argument, which goes on the stack. I filled these in with capstone over
    `scratch_followup/macmem.py` (the disassembly of 10106e50..10108148).
- Amiga:
  - `AddTriggerEx` 68k `FUN_000b2b12` (ghidra_68k l.88595) and OS4 `FUN_010cfa80` (ghidra_os4
    l.98987).
  - DoKidnap: 68k `FUN_000b82ca` (l.93082), OS4 `FUN_010e7738` (l.113305; `AddMissionTrigger`
    is inlined there).
  - `TriggerAlarm2`: 68k `FUN_000b824e`.
- Port: `crates/payback/src/mission/api.rs` (`add_trigger_ex` l.57-130, `apply_actor_setup`
  l.133-153, `actors` l.491-501, `actor_trigger` l.1123-1157), `triggers.rs`, `eval.rs`, and
  `levels/level_11.rs` (M3 = DoKidnap, l.454-631).

Register map used for the Mac disassembly. `SetTrigger_Long(ev, type, p3..p7)` arrives as
r3..r9. It copies r5..r8 to r28..r31 and r9 to r10. `SetTrigger(6 args)` is a stub that sets
p7 = 0 (`li r9,0` at 10106d44), as the port's `add_trigger` → `e = 0` does. Mac object offsets
are 68k + 0x4e2 (e.g. id `+0x968` = 68k `+0x47c`, state `+0x910` = `+0x426`).

## Summary

The add-time switch is the same on the Mac and the Amiga for all 69 types: the same unit
conversions, defaults, sound, phone handling, actor state writes and type remaps. The one
redesign is already documented in missions.md §1.1: pointer-type triggers store the script id
instead of one actor pointer per matching actor. `DoKidnap` is identical in both builds and in
the port. Below are the port deviations from the Mac. Items 1–4 are port bugs where the Amiga
agrees with the Mac. Item 5 refines §1.1. Item 6 is minor.

## Findings (differences)

### 1. Type 0x33–0x36 with a = −1: the port compares against −1 instead of the value (port bug)

- **Mac**: cases 101075b4 (0x33), 101074f4 (0x34), 10107494 (0x35), 10107554 (0x36). When
  `p3 == -1`, it calls `AddTrigger(ev, 0x1e/0x1f/0x20/0x21, p4, -1, -1, …)`. The **threshold
  p4 moves into the first slot**, which is the one `CheckForTriggers` compares the player's
  x/y against (l.585-635 of the function).
- **Amiga**: the same. 68k `case 0x33`: `local_50 = param_4` (type 0x1e). OS4 goes through
  `LAB_010d0a50`, i.e. the plain 0x1e path with p4.
- **Port** (`api.rs:75-79`): it sets only `t.kind = ty - 0x15`. `t.a` stays −1 and the
  threshold stays in `t.b`. `eval.rs:333-336` then tests `p.pos.x <= t.a`, i.e. against −1.
  That means LE/Y_LE never fire and GT/Y_GT fire at once.
- Impact today: none. Every shipped use passes a real actor id (level 10 race checkpoints
  `level_10.rs:317/368`, level 21 `level_21.rs:808`). It is a latent trap for any future script
  or Random-mission use.
- Recommendation: in that arm add `t.a = b; t.b = -1;`, or remap before building the Trigger.
- CONFIRMED (Mac decompile, 68k and OS4).

### 2. Type 0x02 (AT_ACTOR_ON_FOOT): the port follows the live actor instead of a snapshot point (port bug)

- **Mac**: case 1010704c adds one trigger per actor with id p3, storing
  `(obj.x, obj.y, obj+0x8b0 floor)` at add time. In `CheckForTriggers` (l.~715-737 of the
  extracted function) type 2 shares the generic fixed-point branch with phones 1 and 7:
  - |dx|,|dy| < 0x400 around the **stored** point;
  - the player's floor `+0x8b0` equals the stored floor;
  - the player is controlled (state 2) and on foot (`model+0x8165 == 1`).

  The actor is never looked at again.
- **Amiga**: the same (68k case 2 stores `*obj`, `obj+4`, `obj+0x3ce`; script_api.md §3.2
  "position of actor a at add time").
- **Port**: it is in `actor_trigger` (`api.rs:1127`) and is bound to the entity.
  `eval.rs:279-280` tests against the actor's **current** position with no floor check.
  `eval.rs:168-195` drops the trigger silently if the actor is despawned, whereas the original
  keeps the fixed point.
- Impact: no shipped level uses type 2 (no `AT_ACTOR_ON_FOOT` in `levels/`), so it is latent.
- Recommendation: at add time, copy each matching actor's pos and floor into `a, b, c`, with
  no `actor` binding, and evaluate it like the on-foot point test with ±0x400 and a floor
  match. Its arrow then sits at the fixed point.
- CONFIRMED.

### 3. Type 0x3d (ACTOR_BOARDS_ACTOR): the port adds the trigger even if the target vehicle doesn't exist (port bug)

- **Mac**: case 10107a38. `iVar6 = GetObject(p4)`. If it is 0, **nothing happens**: no state
  change and no trigger. Otherwise each actor with id p3 (state ≠ 6) gets state 0x13,
  `+0x90c=1`, `+0x908=target`, `+0x7f0=5`, and one trigger `(0x3d, p3, p4)` is added if at
  least one actor matched.
- **Amiga**: the same (68k `FUN_000efc92(param_4)`, `return` if 0; OS4 `FUN_0104216c`).
- **Port**: `api.rs:115-127` with `apply_actor_setup` `api.rs:144-147`. If `actors(t.b)` is
  empty, the order is `None`, but the per-actor trigger is still queued. It then waits forever
  for a completion report. That is harmless unless the script relies on the event never
  existing, but it is a stray armed trigger.
- Used in `level_11.rs:870-872` (target 0x1f6 normally exists).
- Recommendation: return before queueing when the 0x3d target lookup fails.
- CONFIRMED.

### 4. Order types (0x0c, 0x12, 0x17, 0x32, 0x3d, 0x44): one trigger in the original, one per actor in the port (port bug, duplicate ids only)

- **Mac**: cases 1010796c (0x0c), 10107b28 (0x12), 10107bf0 (0x17), 10107dd0 (0x32→0x17),
  10107a38 (0x3d), 10107ce0 (0x44). Each loops over all actors with id p3 and state ≠ 6 and
  sets their AI state and fields. It then adds **a single** trigger keyed by the id, only if
  the count > 0. Completion hooks (`ObjectAtLocation`, `ObjectIntoBuilding`,
  `ObjectIntoPlVehicle`, `ObjectIntoVehicle`) match on the id, so the first actor to finish
  fires the event once.
- **Amiga**: the same. These types already stored the id on the Amiga (OS4 l.100-124, 152-175,
  221-245, 397-427, 545-575, 609-640).
- **Port**: `api.rs:115-127` queues one trigger per actor, bound to its entity
  (`eval.rs:389-390`). With two live actors sharing an id, the event fires once per actor.
- Impact: none in the shipped scripts. missions.md §1.1 found no simultaneous duplicate ids.
- Recommendation: for these six kinds, give the orders to every actor but queue one trigger.
  Fire it on the first completion report for any actor carrying `t.a`.
- The radius detail lives in the same code: Mac 0x17 always writes `+0x930 = 1000`, while
  0x32 writes `+0x930 = p6` verbatim. The port's `if t.d > 0 { t.d } else { 1000 }`
  (`api.rs:137-140`) gives 0x17 the script's p6 when it is positive and turns a non-positive
  0x32 radius into 1000. No shipped call hits either case: all 0x17 calls pass 0/−1, and all
  0x32 radii are 1500–4000. To match exactly, pick the radius by the original type, not the
  remapped kind.
- CONFIRMED (Mac + 68k + OS4); impact INFERRED.

### 5. Pointer-type triggers when no actor exists at add time (Mac change; refines missions.md §1.1)

- **Mac**: types 8, 0x0d, 0x13, 0x14, 0x16, 0x18, 0x19, 0x1b, 0x1d, 0x2d, 0x31, 0x33–0x36,
  0x3b, 0x3c. Cases 10107350, 101070e8, 10108088, 101080b0, 10107eec, 10107f1c, 10107fc8,
  10107f48, 10107fa0, 1010737c, 10107f74, 10107494.., 10107ebc and 10107114 call `AddTrigger`
  once with the id and **no actor loop at all**. The trigger is created even when no actor
  with that id exists yet. `CheckForTriggers` resolves `GetObject(id)` every frame, so it
  arms itself as soon as an actor with that id is created. It also picks up a later
  re-creation with the same id.
- **Amiga**: loops over the actors. With 0 matches it adds **no** trigger, so the trigger can
  never fire.
- **Port**: follows the Amiga. `actors(a)` is empty, so nothing is queued (`api.rs:115-127`).
  A despawned binding drops the trigger (`eval.rs:168-195`) instead of leaving it dormant for
  a re-created id.
- This is a Mac change. It only matters for scripts that add the trigger before `CreateActor`
  or that re-create an id while the trigger is alive. Those were dead code on the Amiga, so
  the story levels should not change (INFERRED, same caveat as §1.1).
- Recommendation: the same one as §1.1, i.e. store the id and resolve it at evaluation. If we
  do that, no-actor-at-add-time comes for free. It should come with the Mac variant setting.
- CONFIRMED (code). Effect on shipped scripts INFERRED.

### 6. Minor or not worth acting on

- **0x1d ACTOR_NEAR_ACTOR, second actor**:
  - Mac stores p4 and uses `GetObject(p4)` (the first match).
  - The Amiga resolved p4 to the **last** matching actor pointer at add time (68k first loop,
    `unaff_D6`).
  - The port tests against **any** actor with id `t.b` (`eval.rs:316-332`).

  This only differs when the id is duplicated, as at `level_00.rs:800-801`, where the
  gunman/boss ids are unique. No action.
- **Owner `+0x824 = -1`** (68k `+0x342`) is written by 0x17, 0x44 and 0x43 on every matched
  actor (Mac 10107bf0, 10107ce0, 10107ff8). It marks the actor as an NPC driver for the
  crime/score rules. `give_order` (`sim/ai.rs:95`) has no such owner flag. This is only
  observable if an order is given to an empty vehicle (owner 0), which no shipped script does.
  INFERRED. No action unless the port models the owner flag.
- **State-6 skip**: 2, 0x43 and the pointer types do not skip state 6 in the original. The
  port's `actors()` excludes `State::Removed` for every type (`api.rs:498`). Those actors are
  being deleted anyway. No action.
- **Unknown type (> 0x44)**: the Mac prints "AddSound script - illegal sound…" (a misnamed
  string) and the Amiga prints "Recieved unknown trigger". The port queues it and it never
  fires (`eval.rs:404`). Platform artefact.

## DoKidnap (level 11 M3): identical in all three

Mac 101280a0 vs 68k `0xb82ca` / OS4 `0x010e7738` vs `level_11.rs:454-631`. Checked:

- **0x12d–0x130**: increment the pair counter. On "old == 1 or alive == 1" (Mac: `++c == 2`),
  re-arm both 0x17 orders and set counter = 0:
  - 0x12d/0x12e → ends (0x1ac00 / 0x1dc00, 0x38c00) as events 0x12f/0x130.
  - 0x12f/0x130 → middles (0x1c000 / 0x1c800) as events 0x12d/0x12e.

  The 68k `s_c_caron_0001abf9+7` is the constant 0x1ac00.
- **Patrols**: 0x137↔0x138 (guard C) and 0x141→0x142→0x143→0x144→0x141 (guard D), with the
  same points.
- **0x14a**:
  - Phrases 19/20 (speaker 1) and 21 (speaker 2).
  - `SetObjFollowObj(0x131,-1)`.
  - If the alarm is already raised, `SetAggressiveness(0x12d..0x130, 0, -1)`; otherwise
    alarm = 1.
  - Adds `T(0x14b, 8, 0x132)`.
- **0x14b**: phrase 23, `T(0x14c, 0x0c, 0x131)`.
- **0x14c**:
  - Releases 0x12d..0x130 (`RemoveObjectAtWill`) and says phrase 24.
  - Adds `T(0x14d, 0xf, helipad)` and `T(0x14e, 0xe)`.
  - Creates the two −12 peds 0x133/0x134 at (0x35c00, 0x1c400 / 0x1c018), angle 0x10e0.
  - Sets the pad flag to 0.
- **0x14d**: pad = 1, phrase 29, `T(0x14f, 0x1c, helipad)`. **0x14f**: pad = 0, re-adds 0x14d.
- **0x14e**:
  - At the pad: phrase 26, `AddObjectNearObjSide(0x131,0x132,-9,1)`, follow −1, then
    `T(0x150, 0x3c, 0x134)`.
  - Otherwise: phrase 25, kill 0x14d, `T(0x151, 8, 0x132)`.
- **0x151**: phrase 28, re-adds 0x14d and 0x14e.
- **0x150**:
  - Kills all mission triggers, follow(0x131, 0x133), releases 0x131..0x134.
  - Phrase 26 (the original reuses 26; 27 is unused), +350000, Announce 2022.
  - Kidnap flag = 1, finish.
- **0x154/0x155**: kill 0x168/0x169, re-arm the partner's 0x17 to its end, `alive--`.
  **0x156/0x157**: kill 0x16a/0x16b.
- **0x15e / 0x164 / 0x165**: kill all, phrase 17/18/22, release 0x12d..0x134, Announce 2028,
  finish. Only 0x164 sets the flag to −1.
- **0x15f–0x162**: if no alarm, phrase 15, `SetAggressiveness(guard, 0x50, -1)` and
  `T(0x168+k, 0x11, 3)`.
- **0x168–0x16b and 0x16d** → `TriggerAlarm2`: if no alarm, alarm = 1, phrase 16, all four
  guards aggressiveness 0x50. This is identical to 68k `0xb824e` and to the port's `m3_alarm`.
- **Setup (default)**: the same cube and face edits (0x38/0x39, 0x72/0x73), the same 6
  actors (ids, models, positions, floors, angles, colours), the same 17 triggers in the same
  order, alive = 2 and counter = 0.

## Checked identical (add time, Mac = Amiga = port)

- **0 MONEY_GOAL**: the goal is always set; the trigger is added only if a > 0 (10106e94;
  `api.rs:61-66`).
- **1 / 7 phone**:
  - `GetPhoneCoords(a)` → square·0x800+0x400, floor in the 3rd slot.
  - Ring `AddSound(0xe, 1, 10000000, 17000, x, y, GetCubeHeight)`. Its handle is the 9th
    `AddTrigger` argument (Mac `stw r3,0x38(r1)` at 10106f7c; 68k `local_68`; OS4 `uVar3`).
  - a = −1 picks a random phone (Mac `GetPhoneCoords(-1)`).
  - Port: `api.rs:80-111`, `Trigger.sound`.
- **3, 9, 0x0b, 0x22, 0x23, 0x26, 0x28, 0x2b, 0x2e, 0x30, 0x24, 0x27, 0x29, 0x3e, 0x42, 6**:
  plain pass-through with −1 / 0 defaults.
- **4 garage**:
  - `(sx·0x800+0x400, sy·0x800+0x400, p5, flag=1, p6, 0, −1)`.
  - The port keeps the squares and converts them in eval (`eval.rs:391-400`).
- **5 COUNTDOWN / 0x11 TIMER**: a·30. **0x37** → type 0x11 with a in ticks (`api.rs:67-71`).
- **0x0a**: player money + a (Mac `+0x864`, 68k `+0x382`). **0x0e**: current vehicle
  structure.
- **0x0f, 0x10, 0x15, 0x1a, 0x25, 0x2a, 0x2c, 0x2f, 0x38, 0x3f, 0x40, 0x41**:
  `(p3, p4, p5, 0, −1, −1, −1)` (register pass-through on the Mac, confirmed in the
  disassembly).
- **0x39**: `(p3, p4, p5, 0, p6, p7, −1)`. **0x3a**: `(p3, p4, p5, 0, p6, −1, −1)`. The
  6-argument `AddTrigger` gives p7 = 0, as the port's `add_trigger` does.
- **0x1c, 0x1e–0x21**: pass-through.
- **0x0c, 0x3d, 0x12, 0x17, 0x32, 0x44**: AI field writes (state 0x0d / 0x13 / 0x0e / 0x0f /
  0x0f / 0x15; `+0x90c=1` and `+0x7f0=5` for boarding; goal p4/p5; radius 1000 or p6;
  `+0x98a=1` for 0x32). The state-6 skip matches the port's `State::Removed` filter.
- **0x43**: one trigger per matching actor (no state check), state 0x14, owner −1.
- **2**: per-actor fan-out (only the snapshot differs, see 2).
- **Tracked triggers**:
  - `AddTrigger_script` first untracks the same event (`RemoveTrigger_script`, which does not
    kill the trigger) and then tracks it. Port: `t_ex`, `api.rs:161-165`.
  - The Mac adds only if fewer than 500 are tracked, a cap the port doesn't need.
