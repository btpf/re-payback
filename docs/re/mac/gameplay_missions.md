# Mac OS X build vs Amiga Update 7: mission engine and level scripts

Scope: trigger engine, script API, the six level scripts, and the Random-mission mode.
Mac = `Payback (OS X)` (`analysis/ghidra_macx`, CodeWarrior names). Amiga = 68k Update 7
(`analysis/ghidra_68k`, flat `analysis/flat/Payback68k.bin`), cross-checked against OS4 where noted.
Mac ticks are 30 Hz like OS4, so that difference is not repeated here.

## 0. Method, and a decompile gap in `analysis/ghidra_macx`

**The existing Mac export is missing most of the level-script code.** Every `ProcessTrigger_XX`,
`SetTrigger_Long`, `ProcessTrigger_Random` and `DoKidnap` dispatches through a jump table that
CodeWarrior reaches via a TOC pointer (`lwz rT,off(r2); slwi; lwzx; mtctr; bctr`). Ghidra could
not resolve these ("Could not recover jumptable ... Too many branches"). As a result the case
bodies are not part of any function. Examples: `ProcessTrigger_00` is exported as 2,840 bytes but
the real body is 22,312. `SetTrigger_Long` is exported as 104 bytes but the real body is 4,856.
The "unnamed gaps" 10106eb8–10108190 and 1012a100–1012bae0 are these case bodies. There are no
separate FUN_ functions there.

How I recovered them (nothing was written to the repo):
1. `capstone` scan of 100f4000–1013e400 for the bctr pattern. Each table's base is read from the
   relocated PEF data (`tools/re/symbols/pef.py`, r2 = `0x10183440`), and its length
   from the preceding `cmplwi`. This found 36 tables.
2. Ghidra headless re-import (same Docker image and language). A post-script adds
   `COMPUTED_JUMP` refs, `JumpTable(addr, targets, true, 0).writeOverride(f)` and
   `CreateFunctionCmd.fixupFunctionBody`. These are the steps of Ghidra's own `SwitchOverride.java`.
   It applies the overrides in passes, because nested switches only become part of a function
   after their parent switch is fixed, then decompiles the mission ranges. The result contains
   no unresolved jump tables. It was kept in `%TEMP%\macm\out\decompiled.c`, outside the repo.
   **Recommendation:** fold this into `tools/ghidra/` and re-export `analysis/ghidra_macx`. Other
   switch-heavy Mac functions (`UpdateInputStatus*`, movie code at 100f7–100fa) have the same
   problem.
3. Comparison: (a) line-by-line reading of the trigger engine and API functions against their
   68k twins; (b) a normalised diff of every constant-argument script call (CreateActor,
   AddMissionTrigger, Say, AddMoney, SetScenery, ...) per level. The Mac side comes from the new
   decompile. The 68k side comes from `tools/re/calls68k.py`, run over 68k
   `b4f00–ca4c8`. (c) The level data tables were compared byte for byte. (d) Every documented
   quirk was checked one by one.

Result: the Mac level scripts are the **same source as Update 7**. There is one gameplay change
(level 21, §2.1). There is a substantial **new feature**: Random missions in Rampage (§3). And
there is one engine-level change in how actor triggers bind to actors (§1.1).

---------------------------------------------------------------------------------------------

## 1. Trigger engine

### 1.1 DIFFERENCE: actor triggers store the script id and resolve it every frame (no more actor pointers)

* Mac: `SetTrigger_Long` @ `10106e50`, `CheckForTriggers` @ `100f4a50`, `ObjectDead` @ `100f6c40`,
  `RemoveTrigger` @ `100f4560`, `GetObject` @ `100530e0`.
* Amiga: `AddTriggerEx` 68k `0xb2b12` (OS4 `0x010cfa80`), `Trigger_Update` `0xa4a68`,
  `0xa653a`.

68k (type 0x13 shown; types 8, 0x0d, 0x13, 0x14, 0x16, 0x18, 0x19, 0x1b, 0x1d, 0x2d, 0x31,
0x33–0x36, 0x3b, 0x3c and 0x43 are the same). The code adds **one trigger per matching actor**
and stores the **actor pointer**:
```c
case 0x13: for (i = 0; i <= numObjs; i++)
    if (param_3 == obj(i)->+0x47c) Trigger_QueueAdd(ev, 0x13, obj(i) /*pointer*/, p4, -1, ...);
// evaluation: iVar8 = *(int *)(puVar18[2] + 0x2fa);   // dereferences the stored pointer
// removal on death (0xa653a): if (param_1 == *(int *)(stored_ptr + 0x47c)) remove
```
Mac. The code adds **one trigger** that stores the **script id**. Evaluation looks up the
first live actor with that id:
```c
case 0x13: AddTrigger(ev, 0x13, param_3 /*id*/, param_4, -1, 0, -1, -1);
// CheckForTriggers:
iVar14 = GetObject(puVar23[2]);            // first object whose +0x968 (= 68k +0x47c) == id
if (iVar14 != 0) { ...speed test... }      // no actor -> trigger stays armed but dormant
// ObjectDead(id): if (param_1 == puVar4[2]) RemoveTrigger(i);   // compares ids directly
```
The same pattern holds for 8/0x2d (`GetObject(p) == player`), 0x0d, 0x3c, 0x18, 0x1b, 0x31,
0x1d (both actors via `GetObject`), 0x3b/0x16 (id moved to p4), 0x19/0x43 (id in p4),
and 0x33–0x36.

Consequences:
* **Stale pointers are gone.** On the Amiga, an actor removed through state 6 (0x04 drop-off,
  0x12 arrival, 0x0c/0x3d boarding, `Actor_Remove`, `RemoveAllActors`) left its pointer
  triggers pointing at a freed or reused actor slot (script_api.md §3.2). On the Mac such a
  trigger goes dormant until a `KillMissionTrigger` removes it.
* **Released actors stop matching.** `Actor_Release`/`Actor_ReleaseIdle`
  (`RemoveObjectAtWill`/`MakeDrone`) set the id to −1, so on the Mac every pointer-type
  trigger on that actor goes dormant. On the Amiga it kept tracking the released actor.
* **Several actors with one id:** the Amiga watched each of them (e.g. type 8 fired for
  whichever one the player entered). The Mac watches only the first actor with that id
  (`GetObject` returns the first match). Type 0x43 still adds one trigger per actor, but all of
  them resolve to the first actor. In the shipped scripts every constant id with a pointer-type
  trigger belongs to a single live actor. The duplicate CreateActor ids I found (00: 50/51,
  01: 802, 10: 201, 20: guards and the boat) are re-creations in mutually exclusive branches,
  so the difference should not show in the story levels (INFERRED).
* Orders: type 0x12/0x17/0x32/0x44/0x0c/0x3d still put **every** matching actor into the AI
  state, but add a single trigger. Their arrival/boarding hooks (`ObjectAtLocation`,
  `ObjectIntoBuilding`, `ObjectIntoPlVehicle`, `ObjectIntoVehicle`) compare ids, as on the
  Amiga. Type 0x3d is not added at all if the target id has no actor.

Classification: robustness fix / internal redesign (Mac). Confidence: CONFIRMED (both
decompiles).
Impact: `crates/payback/src/mission/api.rs` `add_trigger_ex`/`actor_trigger` and
`eval.rs` (`actor_view`, per-type tests). We follow the Amiga: one trigger per actor, bound to
an `Entity`, and dormant when the entity is despawned, which already matches the Mac's stale
case. The open choice is whether a **released** actor (id cleared) should keep firing.
Recommendation: follow the Mac. Resolve the actor through its current script id at evaluation
time, either by storing the id and looking it up, or by checking that the bound entity still
carries the id. Keep the per-actor fan-out only for 0x43 and the order types. The story
missions should behave the same either way.

### 1.2 DIFFERENCE: trigger evaluation runs in Rampage when the map has phones; per-frame gating

* Mac: `amiga_levelloop` @ `10100f50` (see `levelloop.c` lines ~225–250).
* Amiga: main loop 68k `FUN_000ec686` (`0xeef3c..0xeefc0`).

68k:
```c
if (DAT_0034c198 == 0 && DAT_0011a6e6 == 0 /*rampage*/ && DAT_0011a6e4 == 0 /*mp*/ && !ending)
    r = Trigger_Update(...);  if (r == 2) { state = 3; message(2023); sound(0x1a); }
```
Mac:
```c
if (*_DAT_1017c374 == 0 && GetNumAltPhones() > 0 && !multiplayer && !ending) {
    r = CheckForTriggers(...);
    if (r == 2) { state = 3; SetAnnouncement(0, GetPhrase(rampage ? 0x707 : 0x7e7)); sound(0x1a); }
}
```
* The rampage flag no longer blocks evaluation. Instead the loop requires at least one phone
  on the map (`GetNumAltPhones` @ `100b4270` = the number of scenery-14 cells collected by
  `LoadMap` @ `100b3a60`, up to 100). This exists for Random missions (§3). The story maps all
  have phones, so story play is unchanged.
* When the money goal completes in Rampage, the message is phrase `0x707` (1799). That phrase
  is not in the shipped Mac `Global.English.phrases`, so `GetPhrase` returns its fallback
  string (INFERRED: a placeholder or a cut "high score beaten" line).
* `ProcessTrigger(0)` (the "level is ending" event) is still sent only outside Rampage and
  multiplayer, as `Mission_Event(0)` was. `PlRestart` (type 0x3e) is still called on respawn.

Classification: new behaviour (supports §3). Confidence: CONFIRMED.
Impact: `crates/payback/src/mission/mod.rs` `start_level` returns early for non-story modes,
and `eval::run_triggers` assumes a level script. To support §3, evaluate triggers in Rampage
when the map has at least one phone.

### 1.3 Identical (CONFIRMED in both)

* Storage: 500 entries (Mac stride 0x28 vs 68k 0x26: alignment only), add/kill to-do list
  applied after evaluation (`ProcessToDoList` @ `100f4880` = `0xa4920`). This includes
  "a kill of type 0x17/0x44 resets an actor in state 0x0f/0x15 to 4" and the 0x42 counter.
  `RemoveTrigger` swaps in the last entry, stops the ring sound, restores the phone cell
  14↔19 and closes an open door, and frees 0x0c/0x3d boarders. `KillTrigger` kills every
  trigger with that event. Neither version decrements the index after a fire inside
  `CheckForTriggers`, so in both the entry swapped in is skipped for one frame.
* `ResetTriggers`, `GetTimerVal` (first type 5, ticks/30), `PlRestart` (fires every 0x3e),
  `GetGarageOpen`, `ObjectSunk` (0x2b), `ObjectIntoPlVehicle` (0x0c), `ObjectIntoVehicle`
  (0x3d), `PlAccelerate` (0x42; same RC-flag call site, Mac model `+0x8160` = 68k `+0x4904`,
  so level 21's ev 205 is still dead), `ObjectAtLocation` (0x17/0x44 → state 4),
  `ObjectIntoBuilding` (0x12), `UseHandbrake` (0x27), `ChangeWeapon` (0x24), `FireWeapon`
  (0x22), `PoliceDead` (0x29, one trigger per kill), `ObjectDamaged` (0x2e/0x30). `ObjectDead`
  fires 9/0x28/0x2e/0x30 and silently drops the same pointer types as `0xa653a`, plus 0x3c
  in both.
* Every per-type test in `CheckForTriggers` has the same radii, comparisons and state checks
  (±0x400/0x200/0x4b0/0x1000/0x1800/3000/6000/10000/12000/33000, 6e6/16e6 sight with ±90° and
  LOS, controlled state 2, on-foot flag, strict rectangles for 0x39/0x3a). Countdown beep 0x18
  each second. Phone flashing 14↔19. The door open/close sequence (radius² 150000 on /16
  coordinates, ±4/tick, sound 0x16, fire at < 4000 on floor p−1, ped spawned 0x800 outside the
  side with the same inventory/wanted/money copy). Arrow list cycling every 20 ticks. Type 0
  returns 2.
* Add-time semantics: 0 always sets the goal and adds the trigger only when a > 0; 1/7 phone
  coords plus ring sound (0xe, 10,000,000, 17000 Hz); 2 snapshots the actor position; 4 square
  centre plus door flag; 5/0x11 seconds ×30; 0x37 → 0x11 in ticks; 0x0a money + a; 0x0e
  current vehicle type; 0x33–0x36 with a = −1 → 0x1e–0x21; 0x32 → 0x17 with radius and
  `+0x49e`; 0x43 sets state 0x14; 0x12's 4th argument is ignored.

---------------------------------------------------------------------------------------------

## 2. Script API and level scripts

### 2.1 DIFFERENCE: level 21 init guards the starting car (no more soft-lock)

* Mac: `InitScript_21` @ `10129d20`, `ProcessTrigger_21` @ `1013b2b0` (phone/default branch).
* Amiga: init 68k `0xb9aaa` (OS4 `0x010ebbe8`), handler `0xc79cc`. missions_21.md §1.

68k init:
```c
CreateActor(50, 0, 0x26400, 0x17000, 0, 0, 4);
AddMissionTrigger(100, 0x08, 50);           // get into car 50 -> mission 1
CreateActor(1000000, 23, ...);
```
Mac init:
```c
CreateActor(50, 0, 0x26400, 0x17000, 0, 0, 4);
AddTrigger_script(100, 8, 50, -1, -1, -1);
AddTrigger_script(50, 9, 50, -1, -1, -1);   // NEW: car 50 destroyed
CreateActor(1000000, 23, 0x1e2d0, 0x1db64, 0, 0x10e0, 0);
```
Mac handler (slot −1 branch):
```c
else if (param_1 == 50) {
    RemoveObjectAtWill(50);                  // Actor_Release
    *curSlot = 1;
    StopStoredTriggers();                    // KillAllMissionTriggers
    StartMessage(2); MessageThem(5); StopMessage();   // "You idiot!"
    Announce(0x7ec);                         // 2028 "Mission failed!"
    FinishMission_21();                      // slot 1 -> opens phones 200/300/600
}
```
On the Amiga, level 21 has no phones until mission 1 starts, and mission 1 starts only by
getting into car 50. If car 50 was destroyed before the player got in (traffic, explosions),
the level could never progress. The Mac now counts that as a failed mission 1 and unlocks the
next phones. After ev 100 the trigger stays armed but cannot fire, because `Actor_Release(50)`
clears the id that `ObjectDead` matches. The trigger is then killed by the next
`KillAllMissionTriggers`.

Classification: bug fix. Confidence: CONFIRMED (Mac code; absent from 68k `0xb9aaa` and from
the 68k/OS4 call lists).
Impact: `crates/payback/src/mission/levels/level_21.rs`. Add `s.t(50, kind::ACTOR_GONE, 50,
0, 0, 0)` in `init()` after the ENTER_ACTOR trigger. Add an `ev 50` arm in `start()` that
does `actor_release(50); set_cur_slot(1); kill_all_mt(); say(2,5); show_message(2028);` and
calls the level-21 unlock. Update missions_21.md §1/§2 citing `InitScript_21` @ `10129d20`.

### 2.2 DIFFERENCE (new API): `MessageThem_Parse` / `FlushOldMessages`

* Mac: `MessageThem_Parse(phrase, ...)` @ `101087d0` does `vsprintf` of a phrase into one of 20
  heap buffers (ring at `cb84`/`cb80`) and passes it to `SetMessage(buf, 2)`. The phrase may be
  at most 400 characters. `FlushOldMessages` @ `10108730` frees the buffers. It is called from
  `Script_reset` @ `10104990` (the init-time reset, which otherwise matches 68k `0xb4156`: 10
  sound refs = −1) and from `Script_close` @ `10104a10` (= `0xb4176`).
* Used only by the Random missions (§3). No story level calls it.

Classification: new behaviour. Confidence: CONFIRMED (Mac). Impact: `api.rs` needs a
`say2_fmt(n, args)` with C `%s`/`%d` substitution if §3 is implemented.

### 2.3 Identical (CONFIRMED unless noted)

* **Script API**: each Mac function matches its 68k twin call for call, with Mac object
  offsets = 68k + 0x4e2..0x4ec (`+0x968` id = `+0x47c`, `+0x910` state = `+0x426`, `+0x828`
  skill = `+0x346`, ...). The pairs: `AddObject`=CreateActor `0xb231c` (sets skill 50,
  `+0x934=1`, z from the map); `AddObjectSpecial`=CreateActorFx `0xb2590` (2 dust bursts, sound
  0x2f, damage filter); `AdjustCubeNew`=SetScenery; `AdjustCube`=SetCube;
  `AdjustFace*`=SetFace*; `SetObjFollowObj`=Actor_Follow `0xb2072`;
  `SetAggressiveness`=Actor_Attack `0xb28ac` (modes, shotgun 100 shells, flee 0x12);
  `SetGeneralAggressiveness`=`0xb47e0`; `SetObjectDriveable`=SetControls `0xb483c`;
  `SetObjectTopSpeed`=`0xb475c`; `RemoveObjectAtWill`=Actor_Release `0xb4510`;
  `MakeDrone`=ReleaseIdle `0xb42b8`; `ExplodeObject`=Actor_Kill (−71);
  `FlushAllObjs`=RemoveAllActors `0xb4602`; `AssignIDToPlayerVehicle`=SetPlayerVehicleId (in a
  vehicle only); `ObjTakeOff`/`ObjLand`=Heli_TakeOff/Land; `AddObjectNearPlayer`/`NearObj`/
  `NearObjSide`=Ped_ExitPlayerVehicle/Ped_ExitVehicle/Ex (first matching vehicle only);
  `SetWanted` (resets the wanted counter); `Announce`=ShowMessage `0xb4074` (2022: sound 0x1a +
  multiplier; 2028: sound 0x19); `LevelComplete`=`0xb4cd0`; `AddScore`,
  `AddLives`, `AlterPlHealth`, `RequestVehicle`, `SetCheckpoint`, `SetHighestCube`
  (=SetVisibleLayers with rain 0xff/0x7f), `SetLight/LapState`, `SetIDForBomb`,
  `AddEffect_Gas`, `Explosion_script`, `AddSound_script`/`DeleteSound_script`,
  `SetRestartPos`.
* **Tracked triggers**: `AddTrigger_script`/`_Long_script` @ `10124200`/`101242c0` =
  AddMissionTrigger(Ex) (untrack + add + track, 500 cap); `StopTrigger` = KillMissionTrigger;
  `StopStoredTriggers` = KillAllMissionTriggers; `SetTrigger` = untracked AddTrigger.
* **Phones**: `StartAllPhones` @ `10123fa0` = RefreshPhones `0xb4df8` (types 1/7, intro
  phrase on first call, else Say2(2150 + rand·9>>15)); `StopAllPhones` = KillAllPhones.
  `FinishMission_00/01/10/11/21` = `0xb506e/0xb5198/0xb5230/0xb52c2/0xb538e`: the same
  slot/event/phone graphs and conditions, and level 20 has no unlock routine in either build.
* **Dispatch**: `InitScript` @ `1013e050` = Mission_Init `0xca25c` (same resets: slots −1,
  hidden 0, tracked count 0, curSlot −1, the three level flags, first-phones = 1);
  `ProcessTrigger` @ `1013e2c0` = Mission_Event `0xca368` (plus city 3 → Random);
  `CloseScript` @ `1013e230` = Mission_Shutdown `0xca322` (level 01 releases 0x835 and kills
  0x834/0x835).
* **Level scripts 00, 01, 10, 11, 20, 21 (apart from §2.1)**: the per-level diff of all
  constant-argument calls shows no other difference. Every remaining mismatch was traced to an
  artefact of the 68k listing (register-held ids such as `0x2c7cdc`/`0xb4360`, shared tails,
  loop variables). All phrase numbers that the Mac uses appear in the matching 68k level code.
  This covers mission order and unlocks, spawn positions/models/colours, trigger types and
  arguments, timer lengths (type 5/0x11 seconds), money rewards, wanted levels and map edits.
* **Data tables, byte for byte**: level 20 rectangles (`0x11a358..`) and guard waypoints
  (`0x11a524..0x11a643`); level 10 race AI tables (14 waypoints, speeds, 8 checkpoints); level
  21 race (13 waypoints with dx/dy offsets, speeds, `v386`). Guard 300200 still uses `X<<11`
  without `+0x400` (`AddGuards` @ `10125040`).
* **Documented quirks: all still present on the Mac**:
  * 00 M1: the shotgun trigger 0x72 is not killed after 20 shots.
  * 00 M3: re-entering the bus re-arms Harry's garage `(3,11,1,1)`, not Jon's
    (`T(305,4,3,11,1,1)`).
  * 00 M11: the KKK `112-left`/`99+left` phrase arithmetic.
  * 01: the 0x835 countdown is tracked, so `KillAllMT` defuses it. The ev 0x898 and 0x836 hints.
  * 01 M3: `A` is not reset after a car is dumped. 01 M4: 0x1c3 stays armed.
  * 01 M5: the failure endings 730/750 show 2022 "Mission complete!".
  * 10: the partial `KillMT(0x6d..0x79)`, and the explicit `P = 3`.
  * 11: the M4 taxi-destroyed failure, and the helper `default:` branches that rebuild.
  * 20: the group-A 0x30 trigger is not re-armed while the player is in the cell. The alarm
    order in `TriggerAlarm` @ `10124cd0` is the same as `0xb5692`.
  * 21 M5: the alive flags are never cleared, so Nick always targets 510 (`ProcessTrigger_21`
    0x208..0x211 branch).
  * 21 M5: trigger 510 is not re-armed. 21 M7: trigger 710 is not re-armed.
  * 21 M6: `SetFaceE` vs `SetFaceW` at (124,67). Phrase 60 on hijack.
  * 21 M8: the phone explosion RNG order (y first) and `ShowMessage(2029)`.
  * 21: ev 205 is still dead, because 0x42 needs the RC flag.

---------------------------------------------------------------------------------------------

## 3. NEW: Random missions (Rampage on any map with phones)

* Mac: `InitScript_Random` @ `10129e50`, `ProcessTrigger_Random` @ `1012a0a0` (6656 bytes,
  two jump tables), `FinishMission_Random` @ `1012a020`, `GetPointLocation` @ `10129f50`.
  Helpers: `GetPhoneCoords(-1)` @ `100b4540` (random phone), `GetSafePos` @ `100b42b0`,
  `EstimateTime` @ `10057c60`, `EstimateTime2` @ `10057d60`, `GetRandomVehicleNum` @
  `10057e50`, `GetRandomVehicleNum_Pursue` @ `100580c0`, `GetRandomColour` @ `10051e20`
  (`random(8)`), `GetHighestScore` @ `100a05a0`.
* Amiga: absent. No such code exists in 68k or OS4 (searched for the phrase ids). The Amiga
  `Global.English.phrases` has no phrases 1400–1470 or 1800–1838. These are Mac-only text.

Classification: new behaviour (Mac feature). Confidence: CONFIRMED for the code.
`_DAT_10188fe4` is the Rampage police-aggressiveness choice 0–5 ("P" below). It is copied to
the police AI level in `amiga_startlevel` when in Rampage, and it indexes the Rampage
high-score tables. That identification is CONFIRMED from those two uses. The UI meaning is
INFERRED.

**Activation** (`amiga_startlevel` @ `100fff40`): if Rampage or multiplayer, the code spawns
the map's placed objects (ids `2000+i`, user-map objects). Then, if Rampage and
`GetNumAltPhones() > 0`, it calls `InitScript_Random()`. City index `3` routes
`ProcessTrigger` to `ProcessTrigger_Random`.

**Init:** `Script_reset`; tracked count = 0; curSlot = 0; city = 3.
`SetTrigger(-1, 0, GetHighestScore())` sets the money goal to the top Rampage score for P;
reaching it ends the game as "complete" (§1.2). `SetTrigger(100, 1, -1)` rings one random
map phone. `Announce(1400 "ANSWER THE PHONE")`. `Say(2, 1401 "Welcome to %s. Prepare to
rampage!", map name)`.
**`FinishMission_Random`:** `SetTrigger(100,1,-1)` (a new random phone), curSlot = 0.
Answering phone ev 100 picks `curSlot = Random(7)+1`. Every mission ends with a success line
(random of 1404–1406 with the amount in thousands), `AddScore(k·1000)` and 2022, or a failure
line (random of 1407–1409) and 2028. Then `FinishMission_Random`. `Loc(p)` = phrase
`1830 + GetPointLocation(p)` (map split at squares 43/86 into N, NE, E, SE, S, SW, W, NW and
centre).

| Slot | Brief (speaker 0) | Setup | Win / pay | Fail |
|---|---|---|---|---|
| 1 Steal car | mode m: P<3 → Random(3), else Random(2)+1. m0: 1410 (no clock); m1: 1411 (+t); m2: 1412 (+t) | `GetRandomVehicleNum` (filters RC/boat; tank/heli/rocket/bomb truck/Vapour GT by highest level completed), car id 101 at a safe pos, random angle/colour; T(150,6), T(151,9,101), T(101,8,101), T(152,5,t) if t | ev 101 (in car): marker on. m2: say 1414 (clock keeps running). Else KillMT(152), then 50 %: new t = EstimateTime to destination, say 1415 + T(152,5,t), or say 1414. Wanted: P1 1/3 → 4–5, P2 1/2 → 5, P3 5, P4 5–6, P5 6. T(102,0x0f,dest), T(140,0x0e). ev 140: say 1417, T(141,8,101). ev 141: say 1418, re-arm. ev 102: (Random(20)+50)k | 150/151/152 |
| 2 Gang | n = Random(P+1)+2 peds (−12) ids 201.. near a safe pos; timed if P = 5, or P 3–4 at 50 %. 1420 or 1421(t) | per ped T(201,0x18,id), T(202,0x28,id); T(250,6); T(252,5,t) | first 201/202: all attack (skill P·15+20); KillMT(201). Each kill: "n left" (random 1422–1425). Last kill: (n·20+Random(20))k | 250/252 |
| 3 Checkpoints | 5 safe positions; t = Σ EstimateTime2 legs + Random(20) (×2/3 if P ≥ 4); 1430(Loc) | T(350,6), T(301,0x0f,p0), marker on | 301: say 1431 "Go!", T(302,0x0f,p1), T(351,5,t); 302 ×4 → (Random(30)+70)k | 350/351 |
| 4 Kill cops | n = P+Random(2)+1, t = 10n+Random(10)+15; 1440(n,t); wanted 6 | n × T(401,0x29); T(450,6); T(452,5,t) | each kill: n left (1422–1425); last: (35n+Random(20))k | 450/452 |
| 5 Watchers | `GetRandomVehicleNum_Pursue` (also no police/tank/heli); id 501 at a safe pos, controls off; 1450(Loc, vehicle) | T(501,0x28,501), T(502,0x15,pos), T(550,6) | 502: say 1451, skill 100, T(552,0x43,501), 50 %: T(551,5,t). 501 destroyed: (Random(20)+70)k | 550/551; 552 says 1452 |
| 6 Survival | t = Random(10)+50; Say 1460, Announce 1461 "SURVIVAL!"; wanted 6 | T(650,6), T(601,5,t), T(610,0x11,1) | 610: wanted 6 again every second; 601: t thousand | 650 |
| 7 Points | a = Random(30)+20; t = 2a (P<4) or 1.5a; Say 1470(a,t); Announce 2029 | T(701,0x0a,a·1000), T(750,6), T(751,5,t) | 701: (Random(20)+30)k | 750/751 |

Notes: `EstimateTime(p, veh)` = (|dx|+|dy| from the player)>>12 with no vehicle, else divided by
`veh+0x8134 · veh+0x813c · 2`. `GetSafePos` takes up to 5000 random squares 1..126 with a
drivable ground cube, empty cubes 1–2 and line of sight to the nearest road node; the result is
always cube 0. Vehicle names are phrase `vehicle+0x8198` (1450) or that −530 (1800–1825).

Impact: not implemented. Our Rampage (`crates/payback/src/frontend/rampage.rs`,
`level.rs::is_rampage`, `mission/mod.rs::start_level`) runs without a script. Implementing
this needs: a `levels/random.rs` `LevelScript`; trigger evaluation in Rampage (§1.2);
`GetPhoneCoords(-1)`; the formatted say (§2.2); and the Mac Global phrases 1400–1470 and
1800–1838 (Mac data only). Recommendation: implement as an option, since it exists only in the
Mac release. Treat the 1799 goal message as unresolved.

---------------------------------------------------------------------------------------------

## 4. Summary of actions

1. `levels/level_21.rs`: add the car-50 destroyed trigger and failure (§2.1). CONFIRMED bug fix.
2. Trigger actor binding: optionally adopt the Mac's resolve-by-current-id (§1.1). It matters
   only for released actors and shared ids.
3. Optional new feature: Random missions in Rampage (§3), with the loop gating change (§1.2).
4. Tooling: re-export `analysis/ghidra_macx` with the jump-table override (§0). The current
   export silently lacks most of the level-script and `SetTrigger_Long` code.
