# Police, wanted level, roadblocks, arrest, guard line-of-sight (working notes)

Sources: 68k Update 7 decompile (`analysis/ghidra_68k/decompiled.c`, addresses = flat image
@0x10000) and OS4 decompile (`analysis/ghidra_os4/decompiled.c`). Line numbers below are
`decompiled.c` lines. Tags: **[C]** = CONFIRMED (read in code), **[I]** = INFERRED.

Conventions used below:
- Game tick = 1/30 s. `DAT_00348090` (68k) is the per-frame tick delta (1..5). (Tick rate comes
  from the other agent; the clamp to 1..5 comes from OS4.)
- Objects: `DAT_0030c318[i]` (68k) / `DAT_012a6270[i]` (OS4). Player index `DAT_0030c3a0`
  (OS4 `DAT_012a62f4`). Object count-1 is in `DAT_0011a738`. **While driving, the "player object"
  is the vehicle.** Wanted level and crime counters are copied person->vehicle on entry and back
  on exit (FUN_000db2e4 / FUN_000dab40).
- Object fields (68k offsets; **OS4 is +2** for everything from about 0x32e upward, e.g. wanted
  0x330, state 0x428, cube 0x3d0, weapon 0x360, door 0x418; the model fields 0x4922 and up are
  also +2 in OS4, e.g. 0x4924):
  - +0x00/+0x04/+0x08 x,y,z (2048 units per square; z = (0x5dc - cubeheight)<<4)
  - +0x14 heading (0..0x167f, 0x1680 = 360 deg), +0x24 model pointer
  - +0x2fa/+0x2fe/+0x302 **negated** velocity (position -= vel each tick)
  - +0x2e6/+0x2ea AI target x/y. +0x30e = which side/door of the target the AI is heading for (0..4).
  - +0x32e **wanted level** (int)
  - +0x342 owner: -1 = NPC/drone, 0 = empty/unowned vehicle, 1 = player-side
  - +0x346 driver skill/accuracy (0..100), +0x34a health
  - +0x35e weapon id (0 fist, 1 bazooka, 2 flamethrower, 3 grenade, 4 minigun, 5 shotgun, 0xd pistol,
    0xe bomb; ids checked against the HUD icon loader), +0x362 ammo
  - +0x382 score
  - +0x3ae signed forward speed
  - +0x3ba/+0x3bc/+0x3be route nodes: +0x3ba = node farthest from the player (flee),
    +0x3bc (u16) = own current/next node, +0x3be (u16) = this object's nearest node (published so
    chasers can read it)
  - +0x3c0 (byte) nearest-node refresh countdown in frames (reset to 0x14 = 20)
  - +0x3ce cube level 0..2 of the object
  - +0x416 driver-door open angle (0..0x370; 0x1680 = 360 deg, so 0x370 is about 55 deg)
  - +0x41a script-forced target (1 = player, else object pointer), +0x41e target object pointer
  - +0x422 "has target / move" byte, +0x426 AI state byte
  - +0x428 engine-sound handle, +0x42c horn/siren-sound handle, +0x430 skid-sound handle
  - +0x448 persistent (script) object flag
  - +0x454 light-flash counter (police)
  - +0x458..+0x470 crime counters (below), +0x474 "death already counted", +0x478 "injury already counted"
  - +0x47c script id / radar-blip id (-1 = none)
  - +0x490 (= int[0x124]) stun/inactive timer in ticks; set to 1e9 when the player is busted
  - +0x4a6/+0x4aa player-slot index
- Model fields: +0x4909 kind (0 vehicle, 1 person, 2 prop/projectile, 3 default),
  **+0x490a == 1 police** (police/police_red/police_blue, policevan*, policemale1, policemale1_fire),
  +0x4930 traffic spawn weight /256 (default 0x100), +0x48d4 special class (-1 any; 0 = ice-cream
  class, 1 = police-van class, 2 = truck class), +0x491a horn sound (default 3 CarHorn; police 0xb Siren),
  +0x48c4 radius-ish half-size.

---------------------------------------------------------------------------------------------------
## 1. Wanted level

### Storage, range, HUD
- **[C]** `player+0x32e` (OS4 `+0x330`), int. Normal play maxes out at **6**: no code path puts it
  above 6. The only other writer is the script API
  `FUN_000b4214(level)` (OS4 `FUN_010cef8c`, called from the mission script with 5 or 6), which
  writes any value.
- **[C]** HUD `FUN_00024a3e` (OS4 `FUN_01019bd8`) draws `wanted` siren icons in a centred row
  (spacing = icon width + 2, doubled in one split-screen mode). The icons table `DAT_00138298` holds
  **10 animation frames** (`icons/siren.1..10`). A global counter `DAT_00117d46` runs 0..29 (+1 per
  HUD draw). Icon k uses frame `(c + 2k) mod 30` if that is < 10, else frame 0. So
  siren.1..10 is a flashing animation, not a level count.

### Decay
- **[C]** In FUN_000e3a58 (end of the per-frame object loop, line ~125921):
  ```
  wantedTimer(DAT_0030c3b8) += ticks
  if wantedTimer >= aggr*100 + 150:      // aggr = DAT_003481d8, 0..5
      wantedTimer = 0
      if wanted > 0: wanted -= 1
  ```
  One level drops every 150/250/350/450/550/650 ticks (5.0/8.3/11.7/15/18.3/21.7 s) for
  aggressiveness 0..5. **Every crime that changes the wanted level resets `DAT_0030c3b8 = 0`.**
- The OS4 main update, which holds this code, is not in the OS4 decompile.

### Aggressiveness ("difficulty") `DAT_003481d8` (OS4 `DAT_0114c69c`)
- **[C]** Story mode: the level number 0..5 (00→0, 01→1, 10→2, 11→3, 20→4, 21→5). User maps: 2.
  "Rampage"/level-select mode (`DAT_0011a6e6 != 0`): the menu option "Police aggressiveness"
  (phrase 2055, values 2491..2496), stored in `DAT_0011a6ea` and cycled 0..5 in FUN_0002f66e.
  (FUN_000ec686 ~127023-127096; the other agent confirmed this too.)
- Used for: decay period (above), police shooting chance, police lead/intercept factor `5-aggr`,
  pursuit driver skill (+0x5c vs +0x5e), traffic driver skill (`rand(90)+aggr*8(-30)`).

### Crime → wanted table (FUN_000ccc96(victimIdx, byWeapon); OS4 FUN_01043924)
The function returns early unless the attacker is player-side (`attacker+0x342 != -1`) and the
victim is an NPC (`victim+0x342 == -1`). Callers: vehicle collisions (FUN_000de0b6, `byWeapon=0`),
weapon/explosion damage (FUN_000de0b6 and the blast code FUN_000cd22e, `byWeapon=1`), punches
(main update line 123670, `0`). Each victim is counted once for injury (+0x478) and once for death
(+0x474). Scores are multiplied by the bonus multiplier `DAT_0030c3b0`. **[C]**

| Victim | Event | Wanted change | Crime counter | Score |
|---|---|---|---|---|
| person (civ or cop) | injured (hp>0) | if w<5: w = min(w+1, 4) | GBH +0x464 | – |
| civilian person | killed | <3→3, 3→4, 4→5, 5→6 | murder +0x460 | 1500 |
| police person | killed | 0→3, 1..4 → w+2, 5→6 | murder +0x460 | 2500; Copkiller counter |
| civilian vehicle | damaged (hp>0) | if w<3: w = max(w+1, 2) | byWeapon? crim.damage +0x46c : shunt +0x458 | – |
| police vehicle | damaged | w = max(w, 3) | same | – |
| vehicle with no driver (+0x342==0) | destroyed | civ: max(w,3); police: max(w,4) | crim.damage +0x46c | – |
| civ vehicle with driver | destroyed | <3→3, 3→4, 4→5, 5→6 | murder +0x460 and crim.damage +0x46c | 1500 |
| police vehicle with driver | destroyed | <2→4, 2..4 → w+2, 5→6 | murder and crim.damage | 2500; Copkiller counter |

Extra rules:
- When the player is on foot, `byWeapon==0` bumps into vehicles are ignored.
- A civilian person in state 3 who is hurt flees: state 0x12, `+0x4a2=100`, target = player.
- Copkiller bonus: `DAT_0030c3b4++` per cop killed. When it passes 10 (single player only):
  phrase 2037 "Copkiller bonus!", +100000*mult, counter reset.

**Car entry** (FUN_000db2e4 ~116574, OS4 FUN_01052a4c), once the door is fully open:
- Empty car (`+0x342==0`): no crime. Score and wanted are copied from the person.
- Occupied car (+0x45c **carjacking** ++):
  - civilian: w = max(w, 2); +1000*mult; driver (pedmale `DAT_003480f8`) dragged out
    (`+0x490 = 50`); scream sound 0x25+rand(3)
  - police: w = max(w, 3); +1500*mult; a policeman (`DAT_003480fc`) is dragged out with
    **shotgun (5), ammo 100, +0x490 = 50**
  - a car in state 5 (parked, e.g. a roadblock car): same wanted rules, no one is dragged out.

**Other counters**
- Vehicle-vehicle collisions increment `+0x458` (shunt 'n bump) on both cars (FUN_000de0b6
  line 118604). No wanted change unless FUN_000ccc96 is also called.
- **Firing a weapon** increments `+0x470` (firearms offence) and calls FUN_000ccabe
  (OS4 FUN_010435f4): every civilian person in state 3 within 3D distance < 12 500 units
  (`(d>>3)^2 < 0x17d784`) screams and flees (state 0x12). **Shooting does not raise the wanted
  level**, even near police. **[C]** (No other writers of 0x32e exist.)
- **Speeding** (+0x468) is never incremented: it is only zeroed or copied, so it is dead code. **[C]**
- Crime summary FUN_00094d52(player, arrested) (OS4 FUN_010bcf10): prints phrase 2015 "You are
  charged with" (arrested) or 2030 "You committed" (died), then the counters in this order:
  shunt 0x458 (2001/2), carjack 0x45c (2003/4), murder 0x460 (2005/6), firearms 0x470 (2007/8),
  crim.damage 0x46c (2009/10), speeding 0x468 (2011/12), GBH 0x464 (2013/14). Each counter is
  zeroed after printing. Word 2000 joins the last two entries.
- OS4 counter offsets are +4: 0x45c shunt … 0x474 firearms.

### Effects of each wanted level (summary, all **[C]**, single-player only, `DAT_0011a6e4==0`)
- 0: nothing.
- 1–2: only shown on the HUD. Police cars and cops ignore the player (police cars are forced back to state 3).
- ≥3: police cars and cops pursue. Cops arrest on contact. Police cars run their sirens.
- ≥4: extra police cars spawn as traffic. Cops shoot when aggr ≥ 2.
- ≥5: roadblocks spawn. Cops shoot at any aggressiveness.
- 6: pursuit driver skill reaches 98..100.

---------------------------------------------------------------------------------------------------
## 2. Police vehicles: spawning, pursuit AI, siren

### Models (FUN_000d7874, ~115187-115460) **[C]**
| global | file | notes |
|---|---|---|
| `DAT_00348144` (= `DAT_00348190`) | vehicles/police.obj | traffic table index 9 of 28 (`DAT_00348120[0..27]`), weight 0x4930=200, 0x48d4 = -1 (any city), 0x48ec mass 0x640, 0x491a=0xb Siren, 0x4922=0x44 engine sound, name phrase 0x923 |
| `DAT_0034c100` / `DAT_0034c104` | police_red / police_blue | used only as flashing-light frames for police.obj |
| `DAT_00348170` (= `DAT_0034c108`) | policevan.obj | weight 0x32, 0x48d4=1 (police-van class), name 0x92e |
| `DAT_0034c10c` / `DAT_0034c110` | policevan_red / _blue | light frames |
| `DAT_003480fc` | people/policemale1.obj | kind 1, police, 0x4908=4 |
| `DAT_0034c0e0` | people/policemale1_fire.obj | **firing pose**: the model is swapped to this while shooting (line 124038) |
| `DAT_0034814c` | tank.obj | weight 0, so it is never random traffic. 0x48fc = shell object |
| `DAT_00348164` | helicopter.obj | weight 0, never random |

- Tank and helicopter spawns were not found in the police code, so they are probably
  script-only. **[I]**
- The weight 0 on the tank and helicopter is **[C]**.

### Spawning (FUN_000d0cfc scheduler, OS4 FUN_010485cc; FUN_000cf5cc drone-at-square, OS4 FUN_01047af4) **[C]**
- The scheduler runs every frame from the main loop (line 127559) when `DAT_0034c198 == 0`.
- **Object cap:** spawns happen only while `objCount-1 < min(persistentCount(DAT_0034809c)+8, 20)`.
  FUN_000cf11e (create; OS4 FUN_01046ee4) hard-caps the total at 32. There is **no
  police-specific cap**.
- **Where:** on the edge of the view window, ahead of the player.
  - Window radius `r = ((camZ(DAT_003480d0)+1500)*8 >> 11) + 5` squares, centred on the camera
    square (`DAT_003480b0/c0`).
  - The edge is picked from the dominant axis of the player's velocity: 1 = +x, 3 = −x, 2 = +y,
    0 = −y, random if stationary.
  - A random point is picked along that edge. It is tried for cube levels 0..2, `ticks*2` times
    per frame.
- FUN_000cf5cc(x,y,cube) requirements:
  - the square exists;
  - the low 7 bits of the special byte are not 0 or 4;
  - the occupancy grid `DAT_003178a0[x][y][cube]` is free at the square and its 4 neighbours.
  - Traffic flow bits 1/2/4/8 (N/E/S/W) must continue into the neighbouring square. A random
    valid one is picked; it gives the heading 0xb40/0x5a0/0/0x10e0.
  - Flag 0x20 is a pedestrian square: 1 in 5 chance of spawning a ped (people models
    `DAT_003480e0[6..10]`, skipping index 7).
- **Vehicle choice (police):**
  ```
  repeat up to 20 times:
     if singleplayer and wanted >= 4 and rand(10 - wanted) == 0:  model = police car (DAT_00348190)
     else model = DAT_00348120[rand(28)]
     accept if rand(256) < model.weight(0x4930) and class ok (0x48d4 == -1, or == city for stock maps,
            or the per-map toggle for user maps: 0 ice cream / 1 police vans / 2 trucks)
  ```
  So the plain police car also appears as **ordinary traffic** at any wanted level (weight 200).
  At wanted 4/5/6 the forced chance per attempt is 1/6, 1/5, 1/4.
- **Spawned state:**
  - state 3 (drone), heading from the flow direction, two looping sounds (engine 0x4922, skid 2).
  - Driver skill `+0x346 = clamp(rand(90) + aggr*8 - 30, 0, 99)`. Exception: the "van"
    (`DAT_0034813c`) with route profile `DAT_003481b4` skips the −30.
  - `+0x3c2/+0x3c6` = random entry k = rand(8) from the tables `DAT_00348198` / `DAT_003481b8`.
- **Despawn** (FUN_000dc5ac ~117125): objects in states 3, 5, 7, 0xa, 0xb or 0xc that have no blip
  (`+0x47c == -1`) are removed (FUN_000cf290 RemoveObj; OS4 equivalent not traced) once their
  square leaves the same ±r window. **This includes police in pursuit (states 0xa/0xb).**
  State 6 objects are removed immediately.

### Vehicle AI states (+0x426) relevant to police **[C]** (main update ~124165-124590)
- 2 = player controlled, 3 = drone traffic, 5 = stopped/parked (roadblock car, abandoned car),
  6 = remove, 7 = dead,
  8/10 = follow the route graph to the target, 9/11 = drive straight at the target,
  0xc = barrier/static (a police vehicle in state 0xc keeps its siren on), 0xe/0xf = script
  goto-point (0xf: reached when within `+0x444`; 0x15 also sets `+0x444 = 1000`),
  0x14 = flee (target the route node farthest from the player), 0x15 = script drive-to.
  (8/9 = script "chase" for non-police; 10/11 = police pursuit.)
- **Police car pursuit selection.** Applies in single player, for police cars except vans and
  except state 0xf, while in state 3/8/9/10/11/0x14/0x15:
  ```
  if wanted < 3:  state = 3 (back to normal traffic)
  else:
     target(+0x41e) = player
     skill(+0x346) = wanted + (aggr < 4 ? 0x5c : 0x5e)      // 95..100
     if state not in (9, 11): state = 10
  ```
- **Route following (states 8/10):**
  - The target's nearest node (`target+0x3be`) is refreshed with FUN_000a42ac(x>>3, y>>3, cube)
    whenever `target+0x3c0` reaches 0; the countdown is then reset to 20 frames.
  - If its own node `+0x3bc == 9999`, it takes the nearest node to itself.
  - The steer target is `node.xy << 3`.
  - On arrival (|dx| and |dy| < 300 + |speed|*5):
    - if own node == target's node: state 8→9 / 10→11 (**leave the route and drive straight at
      the target**);
    - else own node = FUN_000a43f2(own, targetNode), which is the next hop.
- **Direct chase (states 9/11), police:**
  `aim = target.pos - target.negvel * (5-aggr) * 3`, i.e. it leads the target by (5−aggr)*3 ticks.
  Non-police aim straight at the target position. There is **no transition back** from 9/11 to
  8/10 in the game code, so once in direct mode they stay there.
- **Driving:**
  - Steering uses atan2 to the aim point; the turn rate comes from model 0x48e4.
  - The look-ahead obstacle scan uses the occupancy grid and traffic-light squares (cube byte +4 =
    light 1..6). It is **skipped when skill ≥ 99 and hp ≥ 50**: those drivers ignore traffic,
    obstacles and red lights, and in practice ram whatever is in front.
  - Skill < 0x46 stops at red lights, skill < 0x5a stops on some phases. Police skill is always
    ≥ 95, so police ignore lights.
  - Skill ≥ 0x62 (98) police use the uncapped speed branch.
- **Deploy a cop on foot** (main ~124545). This only happens while the car is in pursuit mode
  (police, not a van, wanted ≥ 3); in that mode `_DAT_0034c3b4 = 1` (siren on):
  ```
  if |copcar - player|_manhattan < 8000 and
     ( (|copcar.speed| < 100 and |player.speed| < 100) or copcar.hp < 50 ):
     state = 5 (stopped); door(+0x416) = 0x370; DAT_0034c1b0 = 1
     cop = FUN_000db03e(car, policemale1, -1)  // exit-vehicle spawner
     cop.state = 3; cop.weapon = 5 (shotgun); cop.ammo = 100
  ```
- **Siren / lights** (FUN_000e2a50, OS4 not traced). This runs when `_DAT_0034c3b4` (the "horn"
  input; for police AI it means pursuit) is set and the model has no special action
  (0x4908 != 0 and 0x4900 == 0):
  - Starts a looping sound `model+0x491a` (police = **0xb = sounds/Siren**; the sound list order is
    Explosion0, Engine1, Skid2, CarHorn3, Footstep4, Thud5, ShipHorn6, Burp7, Ambulance8,
    CarStart9, Heartbeat10, Siren11, …) at volume 25 000 000, rate 10000. The handle is kept in +0x42c.
  - Every tick `+0x454 = (+0x454 + ticks) mod 10`. When < 5 the model is the _blue variant, else
    the _red variant (police car and van alike).
  - Adds a coloured dynamic light at z−500: red `(200M,20M,20M)` or blue `(20M,20M,200M)` via
    FUN_00046650.
  - When `_DAT_0034c3b4 == 0` the sound is stopped and the model goes back to plain police /
    policevan (line 125866).

---------------------------------------------------------------------------------------------------
## 3. Roadblocks

- **[C] Square flag bit 0x40 = "police may place roadblock here"** (map editor key B). It is tested
  as `squareFlags & 0x40` in FUN_000cfe0c (OS4 FUN_01047134). The flags byte is the per-cube flags
  (68k in-memory: `map + x*0x3900 + y*0x72 + cube*0x26 + 5`; OS4: `x*0x3c00 + y*0x78 + cube*0x28 + 5`).
- **[C] Trigger:** in the spawn scheduler, single player only, when **wanted > 4** (5 or 6).
  Per cube level per pass it makes `(wanted-4)*2 + 1` attempts (3 at wanted 5, 5 at wanted 6). Each
  attempt picks a random square on the same spawn edge and calls FUN_000cfe0c. Not limited by
  anything except the global object cap and the occupancy test.
- **[C] FUN_000cfe0c(x, y, cube):**
  - The square must exist and have bit 0x40.
  - It and its 4 neighbours must be unoccupied (`DAT_003178a0` grid).
  - Road direction comes from the flow bits:
    - flow N or S (bits 1|4): the roadblock line runs along **x**. It extends left and right while
      neighbouring squares also have 0x40 (the whole contiguous 0x40 run).
    - otherwise, flow E or W (bits 2|8): the line runs along **y**.
  - Along the run, alternating and starting with a car:
    - even positions: **police car** (`DAT_00348144`), state 5 (parked), heading 0 (y-run) or
      0x5a0 (x-run), at square centre + 500 along the run axis, with engine and skid sounds.
    - odd positions:
      - a **barrier** (`objects/misc/barrier.obj` = `DAT_0030c3c0`, kind 2, state 0xc, mass 0x32)
        at +600;
      - plus a **policeman** (`DAT_003480fc`) at +0x600: state 3, random heading,
        `+0x10 = 0x5a0`, **weapon 5 (shotgun), ammo 100**.
  - z of each object comes from the square heights for that cube level.
  - `cone.obj` (`DAT_0030c3c4`, kind 2) is loaded and treated like the barrier in collisions, but
    roadblocks do not place it.
- The roadblock objects are normal drones and despawn when they leave the window.
- The parked car can be carjacked: carjacking with a police car in state 5 gives max(w,3) and +1500.

---------------------------------------------------------------------------------------------------
## 4. Police on foot (policemale1)

Spawn sources **[C]**:
- (a) roadblocks (above);
- (b) pursuing police car stopped near the player (above);
- (c) carjacking a police car (cop dragged out, +0x490 = 50 ticks before it acts);
- (d) random? Ped spawns use `DAT_003480e0[6..10]`, which do not include the policeman (index 7
  is skipped). **[I]** Cops are not random pedestrians.

Person AI for police (main update ~121928-122130, single player only) **[C]**:
```
if wanted < 3 or cop.+0x490 != 0 or player.+0x490 != 0 or cop.hp < 1 or player.hp < 1:
    target = self (idle; keeps its drone behaviour)
else:
    target(+0x41e) = player; +0x422 = 1 (move)
    -- contact / arrest test --
    door point = player pos + rotated (0x48f8 forward, -(cop.48c4 + player.48f4) side)  // driver door of a car
    d2 = ((dx>>4)^2 + (dy>>4)^2)
    reach = player in vehicle ? cop.48c4 >> 3 : (cop.48c4 + player.48c4)*3 >> 5
    if d2 < reach^2 and |player.speed(+0x3ae)| < 0x1e and cop.cube == player.cube:
        if player is on foot:  BUSTED (see 5)
        else:  player_vehicle.door(+0x416) += min(100*ticks, 0x370-door)
               if door >= 0x370: BUSTED (dragged out of the car)
    -- shooting decision (sets fire input _DAT_0034c3b4) --
    if aggr < 2:      if wanted > 4: p = (9 - wanted)*10      // wanted 5:40, 6:30
    elif aggr < 4:    if wanted > 3: p = (9 - wanted)*10      // 4:50, 5:40, 6:30
    else:             if wanted > 3: p = (7 - wanted)*10      // 4:30, 5:20, 6:10
    fire this frame if rand(p) < ticks
```
- Fire chance per frame is about ticks/p, which works out to 0.6–3 shots per second at 30 Hz.
  There is **no range or LOS test** on this police fire decision. The shot goes along the cop's
  heading; the cop turns toward the aim point
  `player.pos - player.negvel*(5-aggr)*3` (same lead formula as the cars; line ~122094).
- The player's car door closes by `50*ticks` per frame while the player drives (state 2), with
  sound 0x28 when it shuts (line 124113). So an arrest from a car needs the cop to stay at the
  door while the car speed is < 30.
- **Weapon:** shotgun (id 5), ammo 100 **[C]**. Shotgun fire (main ~123920-124040):
  - two pellets (projectile model `DAT_0030c410`), each at heading + rand(160) − 80, i.e. ±80/5760
    of a turn, about ±5°;
  - ammo −2, sound 0x15 (ShotgunFire);
  - recoil/cooldown `+0x3aa = -30` ticks (1 s);
  - firearms counter +1 on the shooter, nearby peds panic;
  - while firing, the model is swapped to `policemale1_fire`.
  - Pellet damage and range belong to the weapons topic.
- **Chasing:** on foot the cop steers straight at the aim point. It does **not** use route nodes
  (route helpers are only called from the vehicle branch).
- Script guards (see 6) use the same shotgun loadout.

---------------------------------------------------------------------------------------------------
## 5. Arrest ("Busted!") **[C]**

Conditions: see 4. Either:
- a cop touches the on-foot player while the player's |speed| < 30 (same cube level, wanted ≥ 3); or
- a cop at the driver door of the player's stationary car (|speed| < 30) opens the door fully,
  0x370, at +100 per tick. Then FUN_000dab40 (OS4 FUN_01052398) ejects the player, sets door 0x370
  and copies the counters back to the person. The companion object `DAT_003480a0[slot]` is killed
  (hp = −71).

Police cars themselves never arrest. They ram and deploy cops.

On bust (main ~121964 / ~121990):
```
respawnTimer(DAT_0034c1c8) = 80 ticks; savedScore(DAT_0034c1d8) = player.score
FUN_00094d52(player, 1)        -> "You are charged with N crimes..." (counters zeroed)
message phrase 2017 "Busted!"  (0x7e1)
player.+0x490 = 1e9            (player frozen / ignored by AI)
sound 0x25 + rand(3)
```
Death uses the same path: FUN_00094d52(player, 0) → "You committed …", plus a Wasted/etc. phrase.

After the timer runs out (FUN_000ec686 ~127679):
- lives `DAT_0034c428 -= 1` (single player); bonus multiplier `DAT_0030c3b0 = 1` (phrases.txt:
  "reset if you die or get arrested").
- If lives < 1: game over, phrase 2026 "Game over!" (0x7ea), with a 250-tick timer.
- Else:
  - a fresh player person object (`DAT_003480e0`, state 2) is created;
  - placed by FUN_00043c96 at a **random start point** (special=1 + 0x20 squares; it avoids the
    last 4 used). If the script fixed a respawn square (`DAT_0026c040 != -1`), it uses that instead;
  - **score restored** to the saved value, so there is no money loss beyond what was already
    counted.
- Because the player is a brand-new object, weapons, ammo and wanted level go back to the
  object-create defaults. **[I]** The defaults (FUN_000a2b80) were not traced; weapons are
  presumably fist only and wanted 0.

Phrases: 2015 "You are charged with", 2017 "Busted!", 2184 "Caught By The Fuzz" (probably the
death-type stats label), 1/01 tutorial: multiplier reset on arrest.

---------------------------------------------------------------------------------------------------
## 6. Guards / line of sight **[C]**

- **LOS function:** `FUN_0009217e(map, x0, y0, z0>>4-ish, x1, y1, z1)` returns 1 when the segment
  is clear. It wraps the cube ray/collision sweep FUN_000446b8 and fails if that sweep reports an
  X or Y block. Both endpoints are passed with z = (z − 0x400) >> 4, i.e. about eye height.
- **Guard "sees player" script triggers** (trigger evaluator FUN_000a4a68, OS4 FUN_010f4cbc).
  Trigger 0x31 (person watcher) and 0x1b (watcher may be a vehicle) fire when all of:
  - the watcher is alive, its timer +0x490 is 0, and it is on the same cube level as the target;
  - distance² < **6 000 000** (about 2450 units ≈ 1.2 squares). For 0x1b with a vehicle watcher
    the limit is 16 000 000 (4000 units);
  - the target is within **±90° of the watcher's heading** (`angle diff < 0x5a0`), a 180° FOV;
  - **FUN_0009217e line of sight is clear.**
  - So guards **cannot** see through walls (cubes).
  - Level 11 script registers 0x31 triggers (ids 0x15f..0x162) for guards 0x12d..0x130. On fire it
    shows phrase 15 "one of the guards saw you!", calls SetAttack(guard, 0x50), and arms a 3 s
    timer (trigger type 0x11 = param*30 ticks).
- **Script SetAttack API** `FUN_000b28ac(group, accuracy, targetGroup)`:
  - accuracy < 0: flee (state 0x12);
  - 0: passive (state 4, skill 0);
  - > 0: state 0x10 (attack the player) or 0x11 (attack the group's object), with
    `+0x346 = accuracy` (0x50 in the scripts), shotgun, 100 ammo.
- **Attack-mode fire rule** (persons in state 0x10/0x11; main ~121886): when +0x490 == 0 and the
  guard is alive, then if `(|dx|+|dy|)/2 < 6000` to the target, fire if
  `rand(101 - accuracy) < ticks`.
  - With accuracy 0x50: p = 21, about 1.4 shots per second.
  - **No LOS check here**, so an alerted guard keeps firing at a player behind a wall within range.
    Whether the pellets are stopped by cubes is a projectile-collision question.
- LOS is also used for punches (main ~123629) and blast/damage checks in FUN_000de0b6
  (~119091, 119522).

---------------------------------------------------------------------------------------------------
## Route graph extras
- **[C] Node flag byte (+0xc, 0/1):** the getter FUN_000a43d0 has **no callers** in the 68k game,
  and FUN_000a4544 (next used node) has none either. The OS4 build has no flag getter at all.
  In the shipped maps only 5 nodes (map 1) have flag 1.
  **[I]** It is unused at runtime, probably editor state such as the selected/locked node.
- Only these route helpers are actually used, all inside the vehicle AI:
  - FUN_000a42ac nearest node,
  - FUN_000a430a farthest node (flee state 0x14),
  - FUN_000a438c / FUN_000a43ae node x/y,
  - FUN_000a43f2 next hop.
- **OS4 route equivalents [C]:**
  - route records at `DAT_0131669c` (x at 0x013166a4, y at 0x013166a6, flag at 0x013166a8,
    hop table at 0x013166a9, record stride 0x10e);
  - loader FUN_010cd838 (`%s.route`, 0x43800 bytes; if the file is missing every flag is set to 0xff);
  - nearest node FUN_010cd590 (per-node helper FUN_010cd424, results in `DAT_0114c93c/40`);
  - farthest FUN_010cd684; x FUN_010cd72c; y FUN_010cd744; next hop FUN_010cd774.
  - No flag getter or next-used helper exists. The OS4 main per-object update, which calls these,
    is **not** in the OS4 decompile: no xrefs.

## Address map (68k → OS4)
| role | 68k | OS4 |
|---|---|---|
| per-object main update / AI | FUN_000e3a58 | not decompiled |
| victim hurt/killed → wanted | FUN_000ccc96 | FUN_01043924 |
| gunfire panics peds | FUN_000ccabe / FUN_000ccbac | FUN_010435f4 / FUN_01043788 |
| crime summary | FUN_00094d52 | FUN_010bcf10 |
| spawn scheduler | FUN_000d0cfc | FUN_010485cc |
| drone spawn at square | FUN_000cf5cc | FUN_01047af4 |
| roadblock | FUN_000cfe0c | FUN_01047134 |
| create object | FUN_000cf11e | FUN_01046ee4 |
| enter vehicle / carjack | FUN_000db2e4 | FUN_01052a4c |
| eject from vehicle | FUN_000dab40 | FUN_01052398 |
| HUD (wanted sirens) | FUN_00024a3e | FUN_01019bd8 |
| script SetWanted | FUN_000b4214 | FUN_010cef8c |
| script SetAttack (guards) | FUN_000b28ac | not traced |
| trigger evaluator (guard sight 0x31/0x1b) | FUN_000a4a68 | FUN_010f4cbc |
| LOS | FUN_0009217e | not traced |
| siren/lights/horn action | FUN_000e2a50 | not traced |
| model loader | FUN_000d7874 | FUN_0104e13c |
| respawn after bust/death | FUN_000ec686 ~127679 | FUN_010692bc region |

<!-- DONE -->
