# Mac OS X build vs Amiga: combat, police, scoring

Scope: weapons (player and AI firing), projectiles, hit tests, explosions, crime/wanted,
police AI (pursuit, drop-off, cops on foot, arrest), roadblocks, guard line of sight, pickups,
crates, the per-frame health/death pass, scoring/multiplier/lives, the related script API.
Mac = `analysis/ghidra_macx` (Payback (OS X), PEF, original C++ names). Amiga = 68k Update 7
(`analysis/ghidra_68k`) and OS4 (`analysis/ghidra_os4`). Mac tick 30 Hz as OS4 (not re-reported).

**Bottom line: the Mac port is a straight port of this code. No balance change, no new rule and
no bug fix with gameplay effect was found in this area.** Every constant in
`docs/re/combat_police.md` §2-§7 that has a Mac twin has the same value there. The useful
results are (a) places where our notes misread the Amiga code, which the cleaner, named Mac
decompile settles, and (b) port gaps found while cross-checking.

## 0. Mac struct offsets (for future Mac work)

Mac objects are bigger, but the fields keep their order.

- **Object, Mac = 68k + 0x4e2 for 0x2e6..0x3ce:** steer 0x7c8, vel 0x7dc/7e0/7e4, wanted
  0x810, burn 0x81c, owner 0x824, skill 0x828, health 0x82c, weapon 0x840. Ammo: 0x844 shotgun,
  0x848 bazooka, 0x84c flame, 0x850 grenade, 0x854 minigun, 0x858 pistol, 0x85c bomb, 0x860
  placed bomb. Score 0x864, fire timer 0x88c, speed 0x890, filter 0x8ac, level 0x8b0.
- **Object, Mac = 68k + 0x4ea for about 0x3fa..0x42x:** door 0x8fc, target 0x908, state byte
  0x910.
- **Object, Mac = 68k + 0x4ec from 0x458:** crime counters 0x944 shunt, 0x948 carjack, 0x94c
  murder, 0x950 GBH, 0x954 speeding, 0x958 criminal damage, 0x95c firearms. Death/injury counted
  0x960/0x964, script id 0x968, stun 0x97c, flee timer 0x990, player 0x994, owning player 0x998.
- **Model, Mac = 68k + 0x385c:** radius 0x8120, mass T 0x8148, special 0x8160, weapon byte
  0x8164, class 0x8165, police 0x8166, door offsets 0x8150/0x8154, traffic weight 0x8190.
- **Globals:** object table `_DAT_1017c610`, player index `_DAT_1017c60c`, multiplier
  `_DAT_1017c580`, wanted timer `_DAT_1017c614` (`SetWantedCount`), aggressiveness
  `_DAT_1017c28c`, ticks `_DAT_1017c48c`, multiplayer flag `_DAT_1017c4bc`, death timers
  `_DAT_1017c520`, saved scores `_DAT_1017c524`, copkiller counter `_DAT_1017c4b8`.
- **Object update:** split by class into `do_Person` @1013fbd0, `do_Other` @10146280 (vehicles)
  and `do_Weapon` @1013e390 (class-2 props and projectiles), all called from
  `object_MainLoop`. The end-of-frame logic is `MainLoop` @1014a920 (the tail of 68k
  `FUN_000e3a58`).

## 1. Differences found

### 1.1 Mac-only, apparently unreferenced: `GetRandomVehicleNum` @10057e50 / `GetRandomVehicleNum_Pursue` @100580c0

- Mac:
  `do { v = random(26); if (v==25) v=27; reject 22,23 (and 9, 11, 17 for _Pursue); ... } while (v==-1)`.
  Further rejections depend on `GetHighestLevelCompleted()` (0..5), some with `random(5) < 4`.
- Amiga: no twin found (no `Random(0x1a)` in 68k or OS4).
- No call sites in the Mac decompile, by name or by address. It looks like a vehicle picker
  gated by story progress (an unlock), perhaps for a front-end or multiplayer option that was
  never wired up, or one called through a pointer Ghidra did not resolve.
- Class: new behaviour (dormant). Impact on the port: none. Confidence: INFERRED (unreferenced).

### 1.2 Guard sight triggers resolve the watcher by id (robustness)

- Mac `CheckForTriggers` @100f4a50, triggers 0x1b/0x31:
  `piVar7 = GetObject(trig[2]); if (piVar7 != 0 && ...)`.
- 68k `FUN_000a4a68` ~81850 uses the stored object pointer `puVar18[2]` directly, with no null
  test.
- Everything else is identical:
  - same level, health > 0, stun == 0;
  - per-axis |d| < 0x571c, else 5e8;
  - dist² < 6,000,000 (16,000,000 for a vehicle watcher in 0x1b);
  - |Δangle| < 0x5a0;
  - `LineOfSight` at (z − 0x400) >> 4.
- Class: platform artefact / robustness (the Mac script layer stores ids). No gameplay change.
  Impact on the port: none (the port already looks up entities). CONFIRMED.

### 1.3 `CheckForCrate` @100593e0 bounds-checks inside the callee

- Mac: `if (0<=x<0x80 && 0<=y<0x80 && 0<=layer<3)`.
- 68k `FUN_000d0670`: `(uint)x < 0x80 && 0<=y<0x80 && 0<=layer<3`. Same effect, because x is
  unsigned.
- OS4 `FUN_01040ce4`: no bounds check in the callee; it relies on the caller.
- The punch caller in 68k only tests `x < 0x80 && y < 0x80` (the Mac adds `-1 < x`, `-1 < y`).
  An off-map punch was already caught by the 68k callee.
- Class: platform artefact. Impact: none. CONFIRMED.

No other difference was found. Nothing was rebalanced: weapon damage numerators, fire rates,
ammo per pickup, explosion powers, masses, traffic weights, wanted tables, police shooting odds,
arrest reach, roadblock rules and all score values use the same constants.

## 2. Notes in `combat_police.md` that the Mac (and a re-read of the Amiga) contradicts

None of these are Mac changes. The Mac and 68k/OS4 agree; our note is wrong.

### 2.1 Explosion damage to vehicles is q/k, not (q>>4)/k — PORT BUG

- Mac `Shockwave` @10054ac0, with `q = P >> 4`:
  `if (class == 2) dmg = (q/T) >> 4; else dmg = q/T;  health -= dmg / k;`
- 68k `FUN_000cd22e` ~110460:
  `if (*(char*)(model+0x4909) == 2) iVar7 = iVar8/T >> 4; else iVar7 = iVar8/T;`
- OS4 `FUN_0104405c` is the same: class 2 goes to `LAB_0104426c` (`(q>>4)/k`); persons and
  vehicles take `q/k`.
- Note §5.2 says `o.health -= person ? q/k : (q>>4)/k`. In fact only class-2 props (barrier,
  cone, dummy and the rest of the exempt list) get the >>4. Knock-back is applied only to
  class != 2.
- Port: `crates/payback/src/sim/combat.rs` `explosions()` (~line 1075) has
  `let dmg = if is_person { q / k } else { (q >> 4) / k };`. Vehicles therefore take 1/16 of
  the original explosion damage, and `o.vel += push` also pushes props.
- Recommendation:
  - branch on the model class: `class == 2` → `(q>>4)/k`, otherwise `q/k`;
  - skip knock-back for class 2;
  - fix §5.2.
  
  Expect many more chain reactions (cars igniting near grenades and missiles) after the fix.
  Re-check the replay determinism baselines.
- CONFIRMED (all three builds).

### 2.2 Explosion damage score is missing in the port

- Mac `Shockwave` pays, in single player, when the attacker's owner >= 0 and the victim's owner
  == -1:
  `score += mult * (k>>5 < 1 ? 6000 : min(6000, (q/T) / (k>>5))); CheckHitObject(i, 1);`
- 68k `FUN_000cd22e` and OS4 `FUN_0104405c` are identical.
- Port: `combat.rs::explosions` makes no such payment; it only sends `Hurt`.
- Recommendation: pay it there (this needs `by_player`/owner and the multiplier). CONFIRMED.

### 2.3 "Every person's shot increments the firearms counter": only some weapons do

- Mac `do_Person` @1013fbd0 adds 1 to `+0x95c` on a grenade throw, a bomb drop, a bazooka shot
  and a shotgun shot. Pistol, minigun and flamethrower petrol do not. `DoVehicleFire` @10070c20
  also adds 1 for the tank cannon.
- 68k `FUN_000e3a58` (`puVar12[0x11c]` = +0x470) matches: grenade ~123180, bomb ~123300,
  bazooka, shotgun ~124022. Minigun ~123783, pistol ~123880 and flame ~123577 do not count.
- PanicPeds (`ScareDrones` @10054000 = 68k `FUN_000ccabe`) is called for every weapon.
- Port: `Crimes.firearms` (`sim/police.rs`) is never incremented, so the crime summary never
  lists firearms.
- Recommendation: increment it for those four weapons and the tank cannon, and fix the §3.2
  wording. CONFIRMED.

### 2.4 Health regenerates (+1 per 250 ticks)

- Mac `MainLoop` @1014a920:
  `if (counter += ticks) > 0xf9 { counter = 0; for each player: if (0 < hp < 100) hp++ }`.
  The on-foot health is used on foot and the stored on-foot health while driving.
- 68k tail of `FUN_000e3a58` ~125945 (`DAT_0034c410`, `hp - 1U < 99`) is identical.
- §6 says "There is no regeneration". The port already implements it (`sim/people.rs`
  `regen_and_respawn` with `REGEN_TICKS = 250`, and `multiplayer.rs`). Only the note is wrong.
  CONFIRMED.

### 2.5 Shotgun pellets make 3 sparks, larger

- Mac `do_Weapon`, pellet branch `_DAT_1017c4a0[4]`: life −2/tick, 3 × type 9 Flare.
- 68k ~121790 (`DAT_0030c410`): pellets make 3 sparks of size `rand(150)+250`; bullets make 2
  sparks of size `rand(150)+150`.
- §3.3 says pellets are "same as bullet". Check the pellet spark count in `combat.rs`.
  CONFIRMED.

### 2.6 Pistol damage filter is {-1, 7} on OS4 too

- Mac `CheckForCollisions` @1006a150, pistol bullet: `if (filter == -1 || filter == 7)`. The
  68k is the same.
- OS4 `FUN_01056c58` reads `x+1U == 0 && (x+1U & 0xfffffff7) == 0`. That is how Ghidra renders
  the record-form mask test `((x+1) & ~8) == 0`, i.e. {-1, 7}.
- §3.3 "OS4 only −1" is therefore a decompiler artefact. The port already uses 7
  (`Weapon::filter_class`).
- CONFIRMED (Mac, 68k); the OS4 reading is INFERRED.

### 2.7 Cop arrest distance uses the target's door point without the cop radius

- Mac `do_Person`, police branch:
  - door = `t.pos + ((m.8154·X[h] + m.8150·Y[h]) >> 8, (−m.8150·X[h] + m.8154·Y[h]) >> 8)`,
    using only the target model's door offsets;
  - d² in >>4 units;
  - reach = vehicle ? cop.r>>3 : (cop.r + t.r)·3>>5;
  - also |t.speed| < 0x1e and the same level.
- 68k ~121940 is identical (`iVar11 = -*(int*)(piVar13[9]+0x48f4)`, no cop radius).
- The guard range test (state 0x10) and the cop's steering target do add the cop radius
  (`m.8150 + cop.8120`). §2.4 gives `-(cop.48c4 + player.48f4)` for the arrest test, but that
  is the guard/steering formula.
- The Mac guard y term reads `(-m.8150 + cop.r)` where the 68k has `-(cop.r + m.48f4)`. The sign
  differs only for a ≈200-unit term inside a 6000-unit Manhattan range test, so this is
  INFERRED to be decompiler noise.
- Port: `sim/police.rs` `cops()` measures the arrest distance to `goal`, which includes the lead,
  a `radius + 192` lateral offset and a `max_y/2` longitudinal offset. The on-foot reach uses
  fixed 192 radii. It also skips the original's "player stunned" (+0x490) test.
- Recommendation: measure to the door point (the equivalents of model +0x48f4/+0x48f8) without
  the lead or the cop radius, and use the real radii. CONFIRMED.

### 2.8 Shared quirks the Mac did NOT fix (keep them)

- **Roadblock cop height in an x-run (flow N/S).** The Mac `AddPolice` @10058ba0 reads the cop's
  ground height from cube `x0*0x3c00 + xi*0x78`, i.e. with the indices transposed. The 68k
  `FUN_000cfe0c` does the same (`local_26 + param_1*0x3900`). Car and barrier heights are
  correct. The port (`sim/pursuit.rs` `roadblocks`) samples the right square. Keeping that is
  harmless; note the quirk in §2.3.
- **Missile into scenery:** state 6 and a crate break, but no explosion (Mac `AddCollideObj`
  @10055c70).
- **Snowman:** any non-person touching a snowman's circle (+0x4908 == 0xfe) destroys it,
  projectiles included (Mac `AddCollideObj`, 68k `FUN_000cdfdc` ~300). §3.3 says "vehicles".
- **Projectiles vs scenery** are circles, not squares: the same `AddCollideObj` test as people
  and vehicles, radius scenery size / 2 + the projectile's 0x8120, within 500 of its ground
  (combat_police.md §3.3).
- **Punch:** it pays 500 × multiplier and calls Crime whenever the victim is an NPC, without
  checking that the puncher is on the player's side (Mac `do_Person` punch, 68k ~123640).
- **Pursuit:** pursuit selection overwrites script states 0x14/0x15, and there is no return from
  states 9/11 to the route graph.
- **Copkiller bonus:** `multiplier × 100000`, single player only, phrase 0x7f5 (Mac
  `CheckHitObject`, OS4 `FUN_01043924`). The port (`sim/score.rs`) adds a flat 100000 and also
  plays sound 26. Check this against the multiplier (the §7 table says +100000).

## 3. Checked and identical

Function pairs (Mac @addr = Amiga):

- **`CheckHitObject` @100542d0** = Crime, 68k `FUN_000ccc96` / OS4 `FUN_01043924`: every wanted
  step, the counters, 1500/2500, copkiller, flee on hurt, the ObjectDamaged/ObjectDead triggers.
- **`Shockwave` @10054ac0** = Explosion, OS4 `FUN_0104405c` / 68k `FUN_000cd22e`:
  - sound rate `12000 − min(P/20000, 7000)` (the same as the OS4 form);
  - k, ±1000 knock-back, filter {−1,1,3};
  - flash P/250000;
  - scorch, shockwave and flash effects;
  - multiplayer kill credit.
- **`Explode` @10055570** = Ignite, OS4 `FUN_01044b70`.
- **`Detonate` @100550c0** = Destroy, OS4 `FUN_0104465c`.
- **`Explosion` @100558f0** (script explosion; extra type-6 smoke when T > 2500) = the OS4 code
  at decompiled.c ~36135.
- **`do_Weapon` @1013e390** = 68k ~121560-121870:
  - props bounce (Thud / HitScenery);
  - missile spin 0x140/tick and a type-15 fire on impact;
  - petrol −5/tick;
  - grenade: bounce at 8000 Hz, vz = −|vz|/4, friction 8/1 × ticks, spin 0xb4, fuse < 2 →
    Detonate 3000, smoke;
  - bomb bounce at 5000 Hz;
  - bullets −1/tick with 2 sparks, pellets −2/tick with 3 sparks;
  - Ricochet 4e6+rand2e6 / 0x2cec+rand1000.
- **`do_Person` firing** = 68k ~123030-124080:
  - grenade: charge (v·P·10>>8, vz −40·P), fuse 80;
  - bomb: drop/detonate (50000, camera, ObjectDead);
  - bazooka: press edge, recoil −0x20 clamped to −0x40, 8 dust;
  - flamethrower: every 3rd sub-tick, ±25, timer −2, looped sound 41;
  - punch: 30 hp, |dz| < 250, 3·r box, LOS, 500 score, crate at +2·dir;
  - minigun ±40, −3, recoil −0x10; pistol ±20, −15, −4;
  - shotgun: ≥ 2 ammo, 2 pellets ±80, offset dir/2, −30, −2 ammo, −0x20;
  - out-of-ammo click (10 ticks, 3e6/16000) and the auto-switch orders;
  - muzzle lights, fire-timer clamp 25.
- **AI firing in `do_Person`:**
  - guard attack: state 0x10 range 6000 Manhattan, state 0x11 no range,
    `rand(101 − skill) < ticks`;
  - cop chase, arrest and drag-out: door 100·ticks, RC body −71, 80-tick timer, phrase 0x7e1,
    1e9 stun, Ow sound;
  - shooting odds by aggressiveness.
- **`do_Other` @10146280:**
  - door closing 50·ticks and Doorslam;
  - pursuit selection: van/tank/heli exempt, skill wanted+0x5c/0x5e, state 10;
  - lead (5−aggr)·3;
  - drop-off: Manhattan < 8000, speeds < 100 or hp < 50, shotgun with 100 ammo.
- **`DoVehicleFire` @10070c20** = 68k `FUN_000e2a50`:
  - tank cannon: BigGunFire 15e6/9000, missile at +5·dir, v + 2·dir, 8 dust, firearms +1;
  - horn/siren: police 25e6/10000; others 20e6 at 16e6/T clamped to 3000..20000;
  - light bar: ±10 counter, red/blue 200e6/20e6;
  - RC car launch.
- **`CheckForCollisions` @1006a150** = OS4 `FUN_01056c58`:
  - minigun 10000/T {−1,4}; pistol 5000/T {−1,7}; pellet 20000/T {−1,5} + LOS;
  - hit score min(8000, 400000/200000/800000 ÷ T);
  - petrol burn `clamp(ticks·3000/T, 1, hp+71)`, score ×32, filter {−1,2}; set alight 100 +
    LongAaah + murder;
  - vehicle–vehicle Thud/Thud2/Screams;
  - run-over thresholds 50/100; ram score min(6000, v²>>9);
  - irony bonus 10000 (0x7eb); Flattened (0x7e5).
- **`DoCollideStuff` @10067b50** = OS4 `FUN_010542b8`:
  - despawn window and states {3,5,7,10,11,12};
  - health < −70 → Detonate (bomb 50000);
  - burning: Flames at vol r·5000 / r·10000, lights, type 4 flames, timer −ticks, parked at 15;
  - damage smoke/blood `rand(hp+5) ≤ 5·ticks` below 51 hp; dead-person blood `rand(6) ≤ ticks`;
  - death phrases 0x7e0/0x7e3/0x7e4/0x7e8/0x7e9.
- **`MainLoop` @1014a920:** wanted decay `aggr·100 + 150`, regeneration (see 2.4).
- **`AddPolice` @10058ba0** = roadblock, 68k `FUN_000cfe0c` / OS4 `FUN_01047134`: bounds
  1..0x7e, slope byte, 5-square occupancy, bit 0x40, alternating car / barrier + cop, offsets
  500/600/0x600, headings.
- **`AddThing` @10058350** = drone spawn: police chance `rand(10 − w) == 0` at w ≥ 4,
  `rand(28)` table, 20 tries, weights.
- **`CheckForWeapon` @100597e0** = 68k `FUN_000d0988`: amounts 4/75/6/100/20/30/1, types 1..99,
  sound below 0x50, phrases 0x834.., 0x50 info / 0x51 key, respawn queue +1200 ticks, 1000-entry
  ring.
- **`RespawnWeapons` @100595c0:** crate 6, 2 dust, ObjectAppear 20e6/8000. `WipeWeapons` is
  also identical.
- **`CheckForCrate`:** contents mapping 1..7 → 9, 10, 11, 12, 13, 27, 28.
- **`ScareDrones` / `ScareDrones2`** = 68k `FUN_000ccabe` / `FUN_000ccbac`: radius 0x17d784,
  flee 100.
- **Script API:**
  - `SetAggressiveness` @10106570 = 68k `FUN_000b28ac`;
  - `SetGeneralAggressiveness`;
  - `SetWanted` (resets the decay timer), `SetWantedCount`;
  - `AddHealth` = OS4 `FUN_01040f70`: min 0, no max; the on-foot value while driving;
  - `AddPlScore` = 68k `FUN_000efe72`: the label shows the multiplied amount;
  - `IncScoreMultiplier` (cap 4), `AddPlLives`, `ExplodeObject` (−71);
  - `ObjectDead` / `PoliceDead` / `ObjectDamaged` / `FireWeapon` / `ChangeWeapon` (trigger
    dispatch).
- **`LineOfSight` @100af820** = 68k `FUN_0009217e`: a collide-info ray that fails on an X/Y
  block.
- **`UpdateEffects` @100aa1f0** touches no object: ground fire and Gas deal no damage.
- **HUD:**
  - `DisplayHealth`: 5 hearts at hp/20; all small at hp < 1;
  - `DisplayWanted`: frame (c+2k) if < 10, spacing w+2;
  - `DisplayBonusMultiplier`: digit clamped to 9.
- **Tables:** all 66 model masses set in code (`+0x8148` vs `+0x48ec`) and all 30 traffic
  weights (`+0x8190` vs `+0x4934`) are identical in the Mac, OS4 and 68k builds.

Not resolved:

- `GetNextWeapon` / `GetPrevWeapon` / `GetWeaponAmmo` dispatch through jump tables Ghidra did
  not recover. Their fall-through path (8 tries, wrap) matches 68k `FUN_000409f2` /
  `FUN_00040ad2`.
- Effect-size arguments that the Mac decompiler truncated (scorch size, spark sizes) were not
  compared.
