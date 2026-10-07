# Combat, police and routing

Reverse-engineering notes for the Payback reimplementation. They cover the route graph,
police, weapons, pickups, effects, damage and scoring.

Sources: 68k Update 7 `Payback.exe` (`analysis/ghidra_68k`, flat image at 0x10000) and the OS4
port (`analysis/ghidra_os4`). The OS4 main per-object update (`FUN_0105f594`, the counterpart
of 68k `FUN_000e3a58`) did not decompile. So the AI, firing and projectile flight come from
the 68k build only. Everything else is cross-checked in both builds.

Each fact is tagged **[C]** CONFIRMED (read in code, or verified against the data) or **[I]**
INFERRED. Scratch notes with more line references are in
`analysis/combat/notes_{weapons,police,effects}.md`. The route verifier is
`analysis/combat/route_check.py`.

## 0. Conventions

- **Tick.** One game tick is 1/30 s. [C] The demo time-out (`DAT_0114c588 += ticks; > 9000`
  gives phrase 2490 "Time out!") matches "5 Minute Playable Demo". The OS4 frame pacer converts
  `clock()·60/50/2`. The per-frame tick count is 68k `DAT_00348090` / OS4 `DAT_0114c6a0`,
  clamped to 1..5. [C] Every timer and per-tick rate below is in ticks and is multiplied by the
  frame's tick count.
- **Coordinates.** Object positions use 2048 units per square (square = pos >> 11). Map and route
  units are 256 per square (object = map << 3). z is positive downward. [C]
- **Headings.** 0..5759; 0x1680 = 360°, so 16 units per degree. The trig tables are
  `X = 256·sin`, `Y = 256·cos` (68k `DAT_00311e20` / `DAT_0030c420`, OS4 `DAT_012ac16c` /
  `DAT_012a676c`). [C]
- **Objects.** There is one global list of at most **32** dynamic objects. It holds people,
  vehicles, projectiles and barriers. 68k `DAT_0030c318[]`, OS4 `DAT_012a6270[]`; the last index
  is in 68k `DAT_0011a738`. Create: 68k `FUN_000cf11e` / OS4 `FUN_01046ee4`. The player's object
  index is 68k `DAT_0030c3a0` / OS4 `DAT_012a62f4[player]`. **While driving, the player object is
  the vehicle.** Score, wanted level and crime counters are copied person→vehicle on entry and
  back on exit (68k `FUN_000db2e4` / `FUN_000dab40`, OS4 `FUN_01052a4c` / `FUN_01052398`). [C]
- **Offsets.** 68k offsets are given first. **OS4 = 68k + 2 for fields from about +0x300 up**, and
  +4 for the crime counters. [C]
- **Random.** `rand(n)` = 68k `FUN_0003fc8c` / OS4 `FUN_0103ac94`, giving 0..n−1. It is
  table-driven: `n·tbl[++k] >> 9`. [C]
- **PlaySound.** 68k `FUN_000170ce` / OS4 `FUN_0100815c` take (id, loop, volume, rateHz, x, y, z).
  z = −1 means non-positional. There are 500 slots. [C]
- **Sound ids.** These follow the load order in OS4 `FUN_01005864`. [C]

  | Ids | Sounds |
  |---|---|
  | 0–9 | 0 Explosion, 1 Engine, 2 Skid, 3 CarHorn, 4 Footstep.stone, 5 Thud, 6 ShipHorn, 7 Burp, 8 Ambulance, 9 CarStart |
  | 10–19 | 10 Heartbeat, 11 Siren, 12 Squelch, 13 Flames, 14 TelephoneRing, 15 LongAaah, 16 SqueakyVoice, 17 FireMissile, 18 GrenadeBounce, 19 Minigun |
  | 20–29 | 20 PickupWeapon, 21 ShotgunFire, 22 GarageDoor, 23 Bleep, 24 TickTock, 25 MissionFail, 26 MissionComplete, 27 TankEngine, 28 BigGunFire, 29 Ricochet |
  | 30–39 | 30 HitScenery, 31 ThumpingMusic, 32–34 ChangeSelection(1–3), 35 Footstep.grass, 36 Footstep.metal, 37–39 Ow1–3 (but see the note below) |
  | 40–59 | 40 Doorslam, 41 Flamethrower, 42 Click, 43 Splash, 47 ObjectAppear, 53 Pistol, 59 Punch_Hit |
  | 75–76 | 75 Thud2, 76 Screams |

  Note on ids 35–39: the list order is …Footstep.grass, Footstep.metal, Footstep.sand, Ow1–3…,
  and the code uses `37 + rand(3)` for "Ow". So the exact ids of the footstep entries are [I].

### Object fields (68k offset; OS4 +2)

| Off | Meaning |
|---|---|
| +0x00/04/08 | x, y, z |
| +0x14 | heading |
| +0x24 | model pointer |
| +0x2e6/2ea | AI steer target x, y |
| +0x2fa/2fe/302 | velocity (police notes: stored negated, pos −= v) |
| +0x32e | **wanted level** (int) |
| +0x33a | burn / dying timer (100 when set alight) |
| +0x342 | owner: −1 NPC; 0 empty vehicle (also set on the respawned player); 1 player-driven. ≥ 0 counts as "player side" for crimes and score [C usage, I naming] |
| +0x346 | AI skill / accuracy 0..100 |
| +0x34a | **health** (init 100 for every object) |
| +0x35e | current weapon id |
| +0x362..+0x37a | ammo, see §3 |
| +0x382 | **score** |
| +0x3aa | fire timer |
| +0x3ae | signed forward speed |
| +0x3ba/3bc/3be | route nodes, see §1 |
| +0x3c0 | u8 node-refresh countdown |
| +0x3ca | damage filter, see §6 |
| +0x3ce | cube level 0..2 |
| +0x416 | driver-door angle (0..0x370) |
| +0x41e | target object pointer |
| +0x426 | **state byte**, see below |
| +0x458..+0x470 | crime counters, see §2.1 |
| +0x474 / +0x478 | "death counted" / "injury counted" |
| +0x47c | script / radar id (−1 none) |
| +0x490 | stun / inactive timer (1e9 when busted) |
| +0x4a6 | player index |
| +0x4aa | owning player index |

State byte +0x426 values: 2 player-controlled; 3 drone (walking ped or traffic); 5 parked or
projectile or burning wreck; 6 remove; 7 dying/dead; 8/9 script chase (route / direct);
10/11 police pursuit (route / direct); 0xc barrier; 0xe/0xf script goto; 0x10/0x11 attack;
0x12 flee; 0x14 flee via route; 0x15 script drive-to. [C]

### Model fields

| Off | Meaning |
|---|---|
| +0x48c4 | radius / half-width |
| +0x48c8 | front length |
| +0x48cc | rear length |
| +0x48ec | **mass / toughness T** (the divisor in all damage) |
| +0x4904 | special (RC car etc.) |
| +0x4908 | weapon / pickup byte |
| +0x4909 | class: 0 vehicle, 1 person, 2 projectile/prop |
| +0x490a | **1 = police** |
| +0x491a | horn sound (police 11 = Siren) |
| +0x4930 | traffic spawn weight /256 |

All model fields are [C].

## 1. `Maps/<n>.route` — police routing graph

Loader: 68k `FUN_000a4200(mapname)` (`"%s.route"` string at 0x000a4194; OS4 string
0x01134870). Reads the whole file (0x43800 = 276480 bytes) into the table at 68k
`0x0027d5e0`. If the file is missing, every node's `used` byte is set to 0xFF
(`FUN_000a41dc` does the same clear) and police drive "erratically" (editor guide).
CONFIRMED.

### Layout (CONFIRMED against code and all three shipped files)

File = 1024 node records × 270 (0x10E) bytes, big-endian. Node index = record index.

| Off | Type | Name | Notes |
|---|---|---|---|
| 0x00 | u16[4] | `link[0..3]` | target node index, **9999 (0x270F) = no link**. Links are **directed** (editor: "add link from selected node to nearest node"). Gaps allowed (a deleted link leaves 9999 in the middle; seen twice in map 0) — always test every slot. |
| 0x08 | u16 | `x` | map units: 256 per square (square = x>>8). Object coords = `x << 3` (2048 per square). |
| 0x0A | u16 | `y` | same, +y = south |
| 0x0C | u8 | `used` | 0xFF = slot unused; else node is live. Shipped values 0 (most) and 1 (5 nodes in map 1: 112, 116, 117, 309, 310). Getter `FUN_000a43d0` has no callers — value 1 has no runtime effect (INFERRED: leftover editor flag). |
| 0x0D | u8[256] | `next_hop` | 2 bits per destination node: `(next_hop[d>>2] >> ((d&3)*2)) & 3` = which `link[]` slot to follow from this node to reach node `d`. Low bits first. 0 for self / unused destinations. |
| 0x10D | u8 | pad | always 0 |

Unused records: links all 9999, x=y=0, used=0xFF, table zero.
Shipped files: map 0 = 303 nodes, map 1 = 315, map 2 = 528; used nodes are always
contiguous from index 0. Out-degree 1–4 (mostly 2). 398/524, 418/619, 838/978 links
are one-way. Node coordinates sit at road-lane centres (x&0xFF mostly 0x70–0x90).

### Next-hop table semantics

Runtime `FUN_000a43f2(node, dest)` = `link[ hop2(node, dest) ]` (CONFIRMED). The table is
precomputed by the map editor. Verification (`analysis/combat/route_check.py`): it equals
a **hop-count (BFS) shortest path over the directed link graph** for 99.98 % of
(src,dst) pairs (map0: 91494 ok / 12 off; map1: 98895 / 15; map2: 278226 / 30). The
mismatches are all on paths ≥ 24 hops where the stored choice is 0–3 hops longer
(INFERRED: editor relaxation with a bounded number of passes). Euclidean or Manhattan
weighting matches far worse (~10 % mismatches) — so **the metric is hop count**.
Reimplementation: load the table as-is; when generating new `.route` files compute
BFS next hops (ties: any; lowest link slot is fine). Every node reaches every other
node in the shipped maps.

Parser (Python, verified):
```python
def parse_route(d):
    assert len(d) == 1024*270
    nodes = []
    for i in range(1024):
        r = d[i*270:(i+1)*270]
        if r[12] == 0xFF: continue
        links = [l for l in struct.unpack('>4H', r[0:8]) if l != 9999]
        x, y = struct.unpack('>2H', r[8:12])
        nodes.append((i, x, y, links, r[13:269]))
    return nodes
def next_hop_slot(tab, dest): return (tab[dest >> 2] >> ((dest & 3) * 2)) & 3
```

### Runtime helpers (68k, hunk 3; all CONFIRMED)

| Addr | Function |
|---|---|
| `FUN_000a4200` | load route file |
| `FUN_000a41dc` | clear (all unused) |
| `FUN_000a42ac(x,y)` | nearest node to (x,y) in map units (callers pass obj pos >> 3). Uses `FUN_000a4000` per used node: candidate = node itself (squared distance), **plus points sampled along each outgoing link** every 512 map units (n = (|dx|+|dy|)>>9 steps, samples k/n for k=1..n-1); if a sample is the closest so far the result becomes that link's *target* node. Globals: best dist² `0x2c1000` (init 1e9), best node `0x2c1004`. |
| `FUN_000a430a(x,y)` | farthest used node (max squared distance) |
| `FUN_000a438c(n)` / `FUN_000a43ae(n)` | node x / y |
| `FUN_000a43d0(n)` | used byte (no callers) |
| `FUN_000a43f2(n, dest)` | next node from n toward dest |
| `FUN_000a4544(n)` | next used index after n, −1 at end |

OS4 equivalents [C]: table `DAT_0131669c`, loader `FUN_010cd838`, nearest `FUN_010cd590`
(per-node `FUN_010cd424`), farthest `FUN_010cd684`, x `FUN_010cd72c`, y `FUN_010cd744`,
next hop `FUN_010cd774`. OS4 has no getter for the flag byte and no next-used helper.

Waypoint following (68k `FUN_000e3a58`, police AI states 8/10 and 0x14): the car stores
its current node at `obj+0x3be` (u16; 9999 = none → take nearest node) and its steering
target at `obj+0x2e6/0x2ea` = node xy << 3. The node counts as reached when
|pos − target| < 300 + |5·speed| on both axes (`obj+0x3ae` = speed), then it advances
with `FUN_000a43f2(current, target_node)`. Target node of the chased object is
refreshed (nearest node to the chased object, stored at `target+0x3be`) whenever the
u8 countdown `target+0x3c0` is 0, which then resets it to 0x14; the countdown is
decremented by 1 per object update (once per frame, not per tick) at the top of
`FUN_000e3a58`. So the chase target node is re-evaluated every 20 frames. CONFIRMED.


## 2. Police

Police only operate in single player (`DAT_0011a6e4 == 0`). Multiplayer has no wanted level, no
route police and no roadblocks. [C]

### 2.0 Police aggressiveness

The global is 68k `DAT_003481d8` / OS4 `DAT_0114c69c`. It first holds the level number 0..5,
which loads map `n>>1`, sub-level `n&1`. After the map loads it is overwritten with the
aggressiveness, 0..5 [C] (68k `FUN_000ec686` ~127030–127100):

- Story mode keeps the level number: level 00 → 0, 01 → 1, 10 → 2, 11 → 3, 20 → 4, 21 → 5.
- User maps use 2.
- Rampage uses the menu choice "Police aggressiveness" (phrases 2491–2496: Very low..Very high),
  stored in 68k `DAT_0011a6ea` / OS4 `DAT_0114c2ec`.

Aggressiveness drives: the wanted decay period, the police shooting chance, the pursuit lead
factor `5 − aggr`, the pursuit skill, and the traffic driver skill. [C]

### 2.1 Wanted level

**Storage.** `player+0x32e` (OS4 +0x330), int. Normal play never puts it above **6**. Only the
script API `SetWanted` writes it directly (68k `FUN_000b4214`, OS4 `FUN_010cef8c`; the scripts
use 5 or 6). [C]

**HUD.** One siren icon per wanted level, in a centred row with spacing = icon width + 2. The
icons `icons/siren.1..10` are **animation frames**, not a level count. A counter c runs 0..29,
+1 per HUD draw. Icon k shows frame `(c + 2k) mod 30` when that is < 10, else frame 0. 68k
`FUN_00024a3e`, OS4 `FUN_01019bd8`. [C] The game calls these "stars" nowhere; they are sirens.

**Decay.** At the end of the 68k object loop (`FUN_000e3a58` ~125921). [C]
```
wantedTimer(DAT_0030c3b8) += ticks
if wantedTimer >= aggr*100 + 150: wantedTimer = 0; if wanted > 0: wanted -= 1
```
So one level drops every 150..650 ticks (5.0–21.7 s). **Every crime that raises the wanted level
resets the timer to 0** (68k `DAT_0030c3b8`, OS4 `DAT_0114c724`).

**Crimes.** Handled by `Crime(victimIdx, byWeapon)` (68k `FUN_000ccc96`, OS4 `FUN_01043924`). It
only acts when the attacker is player-side (`+0x342 != −1`) and the victim is an NPC
(`victim+0x342 == −1`). `byWeapon` is 0 for collisions and punches, 1 for weapons and explosions.
Each victim counts once for injury (+0x478) and once for death (+0x474). [C]

| Victim | Event | Wanted (w) | Counter | Score ×mult |
|---|---|---|---|---|
| person (any) | injured, hp > 0 | if w < 5: w = min(w+1, 4) | GBH | – |
| civilian person | killed | <3→3, 3→4, 4→5, 5→6 | murder | 1500 |
| police person | killed | 0→3, 1..4 → w+2, 5→6 | murder | 2500 + copkiller |
| civilian vehicle | damaged | if w < 3: w = max(w+1, 2) | byWeapon ? criminal damage : shunt | – |
| police vehicle | damaged | w = max(w, 3) | same | – |
| empty vehicle (+0x342 == 0) | destroyed | civilian: max(w,3); police: max(w,4) | criminal damage | – |
| civilian vehicle, driven | destroyed | <3→3, 3→4, 4→5, 5→6 | murder + criminal damage | 1500 |
| police vehicle, driven | destroyed | <2→4, 2..4 → w+2, 5→6 | murder + criminal damage | 2500 + copkiller |
| carjack civilian car | door fully open | max(w, 2) | carjacking | 1000 |
| carjack police car (also a parked roadblock car) | door fully open | max(w, 3) | carjacking | 1500 |

Notes:
- When the player is on foot, `byWeapon == 0` bumps into vehicles are ignored. [C]
- A hurt civilian in state 3 flees: state 0x12, timer 100, target = player. [C]
- **Firing** a weapon only increments the firearms counter and makes peds flee (§3.2). **It never
  raises the wanted level**, even next to police. [C]
- Vehicle–vehicle collisions increment the shunt counter on both cars, with no wanted change of
  their own. [C]
- **Speeding** is never incremented (dead code). [C]

**Crime counters** (68k offsets; OS4 is +4) [C]:

| 68k | Crime | Phrases |
|---|---|---|
| +0x458 | shunt 'n bump | 2001/2002 |
| +0x45c | carjacking | 2003/2004 |
| +0x460 | murder | 2005/2006 |
| +0x464 | GBH | 2013/2014 |
| +0x468 | speeding | 2011/2012 |
| +0x46c | criminal damage | 2009/2010 |
| +0x470 | firearms | 2007/2008 |

The crime summary is 68k `FUN_00094d52(player, arrested)` / OS4 `FUN_010bcf10`. It prints 2015
"You are charged with" (arrest) or 2030 "You committed" (death), then the counts in the order
shunt, carjack, murder, firearms, criminal damage, speeding, GBH. It uses ", " between items and
2000 "and" before the last, ends with ".", and zeroes each counter after printing it. [C]

**What each wanted level does** [C]:
- 0–2: HUD only. Police cars in pursuit are forced back to state 3 and cops ignore the player.
- ≥ 3: police cars pursue (siren on), cops chase and can arrest.
- ≥ 4: extra police cars spawn as traffic. Cops shoot if aggr ≥ 2.
- ≥ 5: roadblocks appear. Cops shoot at any aggressiveness.
- 6: pursuit skill reaches 98..100.

### 2.2 Police vehicles

**Models** (68k `FUN_000d7874`, OS4 `FUN_0104e13c`) [C]:

| Model | Traffic weight | Class (`+0x48d4`) | Mass | Notes |
|---|---|---|---|---|
| `police.obj` (68k `DAT_00348144`) | 200 | −1 (any map) | 1600 | horn = 11 Siren |
| `policevan.obj` | 50 | 1 ("police vans" per-map toggle, editor object-mode key 2) | 2500 | |
| `tank.obj` | 0 | – | – | never random traffic; script-only [I] |
| `helicopter.obj` | 0 | – | – | never random traffic; script-only [I] |

`police_red` / `police_blue` and `policevan_red` / `_blue` are only light-flash frames.

**Spawning** (scheduler 68k `FUN_000d0cfc` / OS4 `FUN_010485cc`; drone-at-square 68k
`FUN_000cf5cc` / OS4 `FUN_01047af4`) [C]:

- **Object cap.** Spawning runs every frame while `objCount−1 < min(persistentCount + 8, 20)`.
  The hard limit is 32 objects. **There is no police-specific cap.**
- **Window.** The view window has radius `r = ((camZ + 1500)·8 >> 11) + 5` squares around the
  camera square.
- **Edge.** Spawns go on the window edge ahead of the player. The edge comes from the dominant
  axis of the player's velocity (random if stationary). Random points on that edge are tried for
  cube levels 0..2, `ticks·2` times per frame.
- **Square test.** The square must exist and its special&0x7f must not be 0 or 4. The square and
  its 4 neighbours must be free in the occupancy grid (68k `DAT_003178a0[x][y][cube]`). A traffic
  flow bit (1 N, 2 E, 4 S, 8 W) must continue into the neighbouring square; that picks the heading
  0xb40 / 0x5a0 / 0 / 0x10e0. Squares with flag 0x20 spawn a pedestrian with chance 1/5 instead.
- **Vehicle choice.**
  ```
  repeat up to 20 times:
     if wanted >= 4 and rand(10 - wanted) == 0: model = police.obj        // 1/6, 1/5, 1/4 at w=4,5,6
     else model = trafficTable[rand(28)]                                   // 68k DAT_00348120[28]
     accept if rand(256) < model.weight and class ok
  ```
  So plain police cars also drive around as ordinary traffic at any wanted level.
- **Spawned state.** Spawned cars start in state 3 with engine and skid loop sounds. Skill =
  `clamp(rand(90) + aggr·8 − 30, 0, 99)`.
- **Despawn.** Objects in states 3, 5, 7, 0xa, 0xb or 0xc without a script id are removed once
  their square leaves the ±r window. **This includes police in pursuit.** State 6 objects are
  removed at once. (68k `FUN_000dc5ac` ~117125.)

**Pursuit selection.** Applies to police cars that are not vans, in states 3/8/9/10/11/0x14/0x15
(not 0xf) [C]:
```
if wanted < 3: state = 3
else: target = player; skill = wanted + (aggr < 4 ? 0x5c : 0x5e); if state not in (9,11): state = 10
```

**Route following (states 8/10)** [C]:
- Own node is `+0x3bc` (9999 → take the nearest node to self).
- The target's nearest node `target+0x3be` is refreshed by `NearestNode(target>>3)` whenever the
  countdown `target+0x3c0` is 0, which then resets it to 20. The countdown drops 1 per object
  update (per frame).
- The steer target is `node.xy << 3`.
- The node is reached when |dx| and |dy| < 300 + |5·speed|.
- On arrival: if own node == target's node, state 8→9 / 10→11 (**leave the graph and drive
  straight at the target**). Otherwise own node = `NextHop(own, targetNode)`.
- There is **no transition back** from 9/11 to the graph.

**Direct chase (9/11)** [C]:
- Police aim at `target.pos + target.v·(5−aggr)·3`, i.e. they lead the target by (5−aggr)·3 ticks.
  Non-police aim straight at the target.
- Steering uses atan2 with turn rate from model +0x48e4.
- Drivers with skill ≥ 99 and hp ≥ 50 skip the obstacle / traffic-light look-ahead, so they ram.
  Skill < 70 stops at red lights. Police (≥ 95) ignore lights.
- Skill ≥ 98 uses the uncapped speed branch.
- The driver itself (steering dead zone, look-ahead, cruise bands, the ±1500 aim clamp on route legs) is
  shared with traffic and script gotos: vehicles.md §6.2. Consequence: at skill 95–97 (wanted 3–5 with
  aggr < 4) the look-ahead sees the player's car as an obstacle, so the car brakes behind it and then drops
  a cop; only skill ≥ 99 cars ram. [C]
- Port (`sim/pursuit.rs`): the target's nearest node is cached per chaser (refreshed every 20 ticks) instead
  of on the target object; a missing route file sends pursuers straight to state 11. [I]

**Flee (0x14).** The target is the route node farthest from the player (`FarthestNode`, stored at
+0x3ba). Used by script objects. [C]

**Dropping off a cop.** Pursuing car, not a van, wanted ≥ 3 [C]:
```
if |car - player|_manhattan < 8000 and ((|car.speed| < 100 and |player.speed| < 100) or car.hp < 50):
   car.state = 5; car.door = 0x370
   cop = ExitVehicle(car, policemale1); cop.state = 3; cop.weapon = shotgun(5); cop.ammo = 100
```

**Siren and lights** (68k `FUN_000e2a50`) [C]:
- While a police car pursues, it loops sound 11 Siren (volume 25e6, 10000 Hz).
- Every tick `+0x454 = (+0x454 + ticks) mod 10`. Values < 5 show the `_blue` model variant, ≥ 5
  the `_red` variant.
- It adds a dynamic light at z−500: red (200e6, 20e6, 20e6) or blue (20e6, 20e6, 200e6).
- When pursuit stops, the sound stops and the plain model comes back.

### 2.3 Roadblocks [C]

Square flag **bit 0x40** = "police may place roadblock here" (map editor key B). It sits in the
cube header `flags` byte, in-memory cube +5.

Spawning (68k `FUN_000cfe0c`, OS4 `FUN_01047134`) runs only at **wanted ≥ 5**. The scheduler makes
`(wanted−4)·2 + 1` attempts per cube level per pass (3 at wanted 5, 5 at wanted 6), on random
squares of the spawn edge.

Requirements:
- The square has bit 0x40, x in 1..0x7e and y in 1..0x7e.
- In-memory cube byte +8 is non-zero (the port tests "top face visible" instead [I]).
- The square and its 4 neighbours are unoccupied.
- The run is extended in both directions while the neighbours have 0x40; only the first square is tested
  for occupancy.

Layout:
- Flow N/S (bits 1|4) → the line runs along x. Otherwise (E/W) it runs along y.
- The line covers the whole contiguous run of 0x40 squares.
- Alternating along the run, starting with a car:
  - even positions: **police car**, state 5 (parked), heading 0 (y-run) / 0x5a0 (x-run), at
    square centre + 500 along the run;
  - odd positions: **barrier** (`objects/misc/barrier.obj`, class 2, state 0xc, mass 50) at +600,
    plus a **policeman** at +0x600 with shotgun and 100 ammo.
- `cone.obj` is loaded but not placed.

Roadblock objects are ordinary drones and despawn with the window. The parked cars can be
carjacked.

### 2.4 Police on foot (`policemale1`) [C]

**Sources.** Cops come from roadblocks, from pursuing cars stopped near the player, or are dragged
out of a carjacked police car (+0x490 = 50 ticks before acting). They are never random peds.

**AI** (68k ~121928–122130):
```
if wanted < 3 or cop.stun or player.stun or cop.hp < 1 or player.hp < 1: idle drone
else:
  target = player; move = 1
  door = player.pos + rot(model.48f8 forward, -(cop.48c4 + player.48f4) sideways)   // driver door
  d2   = (dx>>4)^2 + (dy>>4)^2
  reach = player in vehicle ? cop.48c4 >> 3 : (cop.48c4 + player.48c4)*3 >> 5
  if d2 < reach^2 and |player.speed| < 0x1e and same cube level:
      on foot   -> BUSTED
      in a car  -> car.door += min(100*ticks, 0x370 - door); if door >= 0x370 -> BUSTED (dragged out)
  p = aggr < 2 ? (wanted > 4 ? (9-wanted)*10 : none)
    : aggr < 4 ? (wanted > 3 ? (9-wanted)*10 : none)
    :            (wanted > 3 ? (7-wanted)*10 : none)
  fire this frame if rand(p) < ticks
```
- Firing has **no range or line-of-sight test**.
- The cop turns toward `player.pos + player.v·(5−aggr)·3` and walks straight at it. Cops on foot
  never use route nodes.
- While driving, the player's car door closes by 50·ticks per frame (sound 40 Doorslam when it
  shuts).

**Weapon.** Shotgun (§3). The model swaps to `policemale1_fire` while firing.

### 2.5 Arrest ("Busted!") [C]

Police cars never arrest; only cops on foot do (§2.4). On a bust (68k ~121964):
```
deathTimer[p] = 80; savedScore[p] = score
CrimeSummary(player, arrested=1)   // "You are charged with ..."
message 2017 "Busted!"; player.+0x490 = 1e9 (frozen); sound 37+rand(3)
```
A death takes the same path, with the "You committed" summary and a death phrase (§7).

When the timer expires (68k `FUN_000ec686` ~127679; OS4 `FUN_010692bc` ~49630):
- lives −1 and multiplier = 1;
- if lives < 1: game over;
- otherwise a new player person is created at a **random start point**, avoiding the last 4 used
  (68k `FUN_00043c96`), or at the script's respawn square if one is set (`DAT_0026c040 != −1`);
- **the score is restored**: there is no money penalty.

The new object starts with defaults, so weapons, ammo and wanted level are presumably lost: fist
only, wanted 0. [I] The default-initialiser `FUN_000a2b80` sets health 100 [C]; ammo defaults
were not traced.

### 2.6 Guard line of sight [C]

**LOS test.** `LOS(x0,y0,z0,x1,y1,z1)` is 68k `FUN_0009217e` (OS4 `FUN_010707f0`). It sweeps a ray
through the cubes (68k `FUN_000446b8`) and fails on any X/Y block, using about eye height
(`(z − 0x400) >> 4`).

**"Guard sees player" triggers.** Script trigger types 0x31 (person watcher) and 0x1b (watcher may
be a vehicle), evaluated in 68k `FUN_000a4a68` / OS4 `FUN_010f4cbc`. They fire when all of:
- the watcher is alive, not stunned, and on the same cube level as the target;
- dist² < **6,000,000** (≈ 2450 units ≈ 1.2 squares), or 16,000,000 for a vehicle watcher in 0x1b;
- the target is within **±90°** of the watcher's heading (`|Δangle| < 0x5a0`);
- `LOS` is clear.

So **guards do not see through walls**.

Level 11 uses these triggers: guard sight → phrase "one of the guards saw you!" → `SetAttack`.

**SetAttack.** The script API is 68k `FUN_000b28ac(group, accuracy, targetGroup)`:
- accuracy < 0: flee;
- 0: passive (state 4);
- > 0: state 0x10 (attack the player) or 0x11 (attack a group), `+0x346 = accuracy`, shotgun with
  100 ammo.

**Attacking guards fire** when `(|dx|+|dy|)/2 < 6000` and `rand(101 − accuracy) < ticks`.
Accuracy 0x50 gives p = 21, about 1.4 shots/s. There is **no LOS check for firing**: once alerted,
a guard keeps shooting at a player behind a wall within range. Its pellets still need LOS to do
damage (§3.3).

## 3. Weapons

Code: 68k `FUN_000e3a58` ~123030–124080, selection 68k `FUN_000da2e0` / OS4 `FUN_01051a10`.
All facts in this section are [C] unless marked.

### 3.1 Ids, ammo slots, selection

| HUD slot (key) | Weapon | id (+0x35e) | Ammo field (68k / OS4) | People model | Model +0x4908 |
|---|---|---|---|---|---|
| 0 | fist | 0 | – | malenormal | 0xfe |
| 1 | pistol | 0xd | +0x376 / +0x378 | malenormalpistol | 6 |
| 2 | shotgun | 5 | +0x362 / +0x364 | malenormalshotgun | 4 |
| 3 | flamethrower | 2 | +0x36a / +0x36c | malenormalflamethrower | 1 |
| 4 | grenade | 3 | +0x36e / +0x370 | malenormalgrenade | 2 |
| 5 | minigun | 4 | +0x372 / +0x374 | malenormalminigun | 3 |
| 6 | bazooka | 1 | +0x366 / +0x368 | malenormalbazooka | 0 |
| 7 | remote bomb | 0xe | +0x37a / +0x37c (placed-bomb id at +0x37e / +0x380) | malenormalbomb | 7 |

**Firing dispatch.** Firing branches on the held people model's +0x4908, not on +0x35e. The
`*_fire.obj` / `malenormalpunch` pose models are swapped in for one frame per shot.

**HUD bar.**
- Icons appear left to right in the order fist, pistol, shotgun, flame, grenade, minigun, bazooka,
  bomb.
- Only weapons with ammo > 0 are shown (the bomb also when one is placed).
- Spacing is 10 px (20 px in hi-res). The selected icon alternates `.normal` / `.bright`.
- A 5-digit ammo counter is drawn for the current weapon.

**Direct select (keys).**
- A slot is accepted if its ammo > 0 (bomb: or a bomb is placed) and it is not already current.
- On accept: PlaySound(20 PickupWeapon, 0, 0xff, 14000), then set the weapon and model.
- Requires health > 0.

**Next / prev.** 68k `FUN_000409f2` / `FUN_00040ad2` cycle through slots 0..7, skipping empty
ones. The fist is always valid.

**Out of ammo.**
- Firing with no ammo while the cooldown is 0 sets the cooldown to 10 ticks and plays
  PlaySound(42 Click, 0, 3e6, 16000).
- It then auto-switches to the first weapon with ammo in this priority:
  - grenade / bomb / bazooka / flamethrower → minigun, shotgun, pistol;
  - minigun → shotgun, pistol, flamethrower;
  - pistol → shotgun, minigun, flamethrower;
  - shotgun (ammo < 2) → pistol, minigun, flamethrower;
  - if nothing has ammo, it stays on the weapon and clicks.

**No maximum ammo.** Pickups add without clamping.

### 3.2 Firing

**Fire timer.**
- Fire timer = `+0x3aa`. While fire is held: `if t < 25: t = min(t + ticks, 25)`.
- A shot sets t negative (the reload).
- When fire is released, a negative t keeps recovering toward 0, otherwise t resets to 0.
- Bazooka, bomb and tank cannon fire only on a **press edge**.

**Every person's shot also:**
- increments the firearms counter;
- makes civilian peds in state 3 within 3D distance 10000 units (`(d>>3)² < 0x17d784`, ≈ 4.9
  squares) scream (37+rand3) and flee for 100 ticks (68k `FUN_000ccabe`, OS4 `FUN_010435f4`).

**Projectile spawn.**
- Projectiles are objects: state 5, z = shooter z − 50, layer copied, owner `+0x342` / `+0x4aa`
  (AI: `+0x342 = −1`, `+0x41e = shooter`).
- Bullets start one tick behind the spawn point.
- `dir` = (Y[h], X[h]), 256 units per tick.

| Weapon | Rate | Ammo/shot | Projectile | Speed | Spread | Sound (id, vol, Hz) | Recoil +0x3ae |
|---|---|---|---|---|---|---|---|
| Fist | once per press | – | melee | – | – | 59 Punch_Hit (4e6, 22050) on hit | – |
| Pistol | t = −15 → 2/s | 1 | `bullet.obj` #2 | 256/tick + shooter v | ±20 (±1.25°) | 53 Pistol (4e6, 14000) | −4 |
| Minigun | t = −3 → 10/s | 1 | `bullet.obj` #1 | 256 + v | ±40 (±2.5°) | 19 Minigun (4e6, 16000) | −0x10 |
| Shotgun | needs ≥ 2 ammo; t = −30 → 1/s | **2** | `shotgunbullet.obj` × **2 pellets** | 256 + v | each ±80 (±5°); 2nd pellet offset sideways by dir/2 | 21 ShotgunFire (4e6, 14000) | −0x20 |
| Flamethrower | while held: one petrol every 3rd sub-tick | 1 per petrol | `petrol.obj` | 256 + v, ±25 jitter in x/y | jitter | 41 Flamethrower **looped** (5e6, 12500) | – |
| Grenade | hold to charge P = t (0..25); throw on **release** | 1 | `grenade.obj`, fuse +0x34a = **80** | v + dir·P·10/256 (≤ 250/tick), vz = −P·40 (up, ≤ 1000) | spawned 256 units to the side (hand) | none | – |
| Bazooka | press edge | 1 | `missile.obj` | 256 + v | none | 17 FireMissile (5e6, 17000) | −0x20 + 8 dust particles (back-blast) |
| Remote bomb | press: drop if none placed, else **detonate** | 1 on drop | `bomb.obj`, 512 units ahead, v = shooter v | – | – | – | – |

Muzzle lights (when the lighting option is on): pistol (40e6, 40e6, 10e6), minigun (60e6, 60e6,
8e6), shotgun (60e6, 60e6, 20e6).

**Fist** (68k ~123605).
- Targets: every person with |Δz| < 250 and |Δx|, |Δy| < 3·r of the hit point pos + 256·dir, with
  `LOS` clear.
- Each target takes **30** health, flat (not divided by T), and says Ow. At ≤ 0 it enters state 7.
- A hit on an NPC gives score 500 × multiplier.
- A punch also breaks a crate in the square at pos + 512·dir.

**Remote bomb.**
- On drop, the bomb's id is stored. The next press detonates `Destroy(bomb, 50000)` (§5.2,
  P = 1e9) and points the camera at it.
- It notifies the script (`FUN_000a653a`).
- If the bomb is destroyed by damage it goes off as a small explosion instead.

**Tank cannon** (68k `FUN_000e2a50`, OS4 `FUN_0105e02c`).
- Applies to vehicles whose model +0x4908 == 0 (only `tank.obj`). Fires on a press edge, with
  unlimited ammo.
- Sound: PlaySound(28 BigGunFire, 0, 15e6, 9000).
- Spawns `missile.obj` 1280 units ahead (pos + 5·dir), v = tank v + 2·dir (512/tick), no
  gravity, plus 8 dust particles.
- Tank T = 40000, so bullets do 0 damage (integer division) and only explosions hurt it.
- The bombtruck's fire button launches the RC car instead.

### 3.3 Projectiles

**Physics** (68k `FUN_000e0e46`). Per tick: `vz += 32` unless no-gravity, then z += vz, x += vx,
y += vy, then cube collision. Bullets, pellets, petrol and missiles have no gravity. Grenades and
bombs do.

| Projectile | Lifetime (+0x34a from 100) | Range | On wall / ground / timeout |
|---|---|---|---|
| bullet | −1/tick → 100 ticks | ≈ 25600 units ≈ 12.5 squares | back off 1 tick; 29 Ricochet (vol 4e6+rand 2e6, 11500+rand 1000 Hz); 2 Flare sparks (effect type 9); removed |
| pellet | −2/tick → 50 ticks | ≈ 6 squares | same as bullet |
| petrol | −5/tick → 20 ticks | ≈ 2.5 squares | ground-fire effect (type 0xf, life 800, looping Flames). Each tick a random orange light and a type 1 flame |
| missile | none | until impact | wall/ground: fire effect, then **Ignite-explosion** of itself (§5.2, P = 1000·25000 = 2.5e7) |
| grenade | fuse −1/tick from 80 | thrown | walls reflect vx/vy with 18 GrenadeBounce (4e6, 8000); ground: vz = −vz/4 (sound if vz > 500); friction 8·ticks on ground, 1·ticks in air; trailing smoke; fuse < 2 → `Destroy(self, 3000)` (P = 6e7) |
| bomb | none | – | bounces like a grenade (sound at 5000 Hz) |

**Hits on objects.** The object pair loop is 68k `FUN_000de0b6` / OS4 `FUN_01056c58`. A projectile
never hits its own shooter.
- **Minigun bullet:** health −= **10000 / T** (filter +0x3ca ∈ {−1, 4}).
- **Pistol bullet:** health −= **5000 / T** (68k filter {−1, 7}; OS4 only −1).
- **Shotgun pellet:** health −= **20000 / T**, only if `LOS(shooter, target)` (filter {−1, 5}).
- Worked examples: pedestrians have T = 90 (pedmale3 120, pedmale4/5 100, pedmale7 95), so
  minigun 111, pistol 55, pellet 222 against 100 health. A default car (T 1400) takes 7 / 3 / 14.
  Vehicles hit by bullets play Ricochet with 2 sparks; people say Ow.
- **Missile:** explodes (§5.2).
- **Petrol:** a type 1 flame on the target. Burn damage = `clamp(ticks·3000/T, 1, health+71)`
  (filter {−1, 2}). A person brought to ≤ 0 is set to 0 and **set alight**: burn timer 100,
  15 LongAaah (5e6, 6000), murder +1. A burning person dies when the timer runs out (§6).
- **Grenade / bomb:** no damage; they bounce off.

**Hits on scenery.** A projectile entering a square with a scenery object stops there. Bullets,
pellets, missiles and petrol also break a crate on that square. A missile hitting scenery is only
removed; whether it explodes is not confirmed [I]. Vehicles driving into a **snowman**
(+0x4908 0xfe) destroy it.

### 3.4 Flamethrower and buildings

Petrol leaves short-lived ground-fire effects (type 0xf), which deal **no damage**. **No
"building on fire" state exists**: cubes cannot burn. [I, from the absence of any writer] The
object texture `burnt` exists; its trigger was not traced [I].

## 4. Pickups

Code: collect 68k `FUN_000d0988` / OS4 `FUN_01048ee4`; respawn 68k `FUN_000d079e` / OS4
`FUN_01048c40`; crate break 68k `FUN_000d0670` / OS4 `FUN_01040ce4`. All [C].

**Where.** Pickups are scenery objects on cube records (scenery byte = cube +1, special = +4).

**Collection.**
- Only by the player-controlled object (state 2), **by entering the square**: there is no
  distance test. A vehicle checks every square under its 3×3 footprint loop and also breaks
  crates there.
- Only model types 1..99 qualify. Radar (100), briefcase (23, no type) and phones are not pickups
  in this code; their effect is mission-script logic [I].
- Effects of a pickup: 2 sparkle particles; for types < 0x50, PlaySound(20 PickupWeapon, 0, 1e6,
  14000).
- A floating message at the square centre shows phrase `0x834 + type − 1`.
- The square's scenery is then set to 0.

| Type (model +0x4908) | Scenery | Gives | Phrase |
|---|---|---|---|
| 1 bazooka | 9 | +4 | 2100 "Bazooka (4)" |
| 2 flamethrower | 10 | +75 | 2101 |
| 3 grenade | 11 (largegrenade) | +6 | 2102 |
| 4 minigun | 12 | +100 | 2103 |
| 5 shotgun | 13 | +20 (= 10 shots) | 2104 |
| 6 pistol | 27 | +30 | 2106 |
| 7 bomb | 28 (bigbomb) | +1 | 2107 "Remote-triggered bomb (1)" |
| 0x50 info | 24 | sound + 2105 "Information" | text comes from the script [I] |
| 0x51 key | 34 | sound + 2108 "Key" | |

**Respawn.**
- Each collection queues (sx, sy, layer, clock + **1200 ticks** = 40 s) in a 1000-entry ring
  buffer.
- When due, the square's scenery becomes **6 (crate)** with its special byte unchanged, plus 2
  sparkles and PlaySound(47 ObjectAppear, 0, 20e6, 8000).
- So every collected pickup returns as a crate. A weapon placed directly with special 0 returns as
  an empty crate [C logic].

**Crates** (scenery 6).
- Break triggers: a punch, bullets, pellets, missiles, petrol, or a vehicle entering the square.
- On break: crate.obj fragments plus Dust clouds (effect type 3, size 1500+rand1000).
- The scenery then becomes the contents given by the square's 5-bit `special`: 1→9 bazooka,
  2→10 flamethrower, 3→11 grenade, 4→12 minigun, 5→13 shotgun, 6→27 pistol, 7→28 bigbomb, anything
  else → empty.

## 5. Effects

### 5.1 Effect particle system [C]

**Spawner.** `AddEffect(type, tex, x, y, z, vx, vy, vz, size, layer, blend)`: OS4 `FUN_01035da0`,
68k `FUN_0003d97a`.
- Velocities are stored /3.
- `size` is both the sprite half-extent (world units) and the lifetime: the effect is removed at
  size ≤ 0, at z > 24000, or when off-screen (immediately). Weather wraps around the view instead.
- Type 15 also starts a looping 13 Flames sound (1.5e6, 19000 Hz).

**Lists.** 4 lists, one per layer = cube storey + 1 (layer 3 = top). Max **5000** each. Records are
0x2c bytes (OS4) / 0x2a bytes (68k). Removal swaps in the last entry (`RemoveEffect`: OS4
`FUN_01035f48`).

**Updates.** Per tick `pos += vel`. Flame textures animate Flame.1→2→3→4→1 on every update.

| Type | Size Δ/tick | Behaviour | Use |
|---|---|---|---|
| 1 | −20 | – | flame/smoke puff (petrol hit Flame.N 800/250; Dirt spray; smoke from 4/7) |
| 2 | −3 | vz += 17; may spawn type 4 (rand(5) < ticks) | burning debris from explosions |
| 3 | −57 | – | dust cloud (crate / wall) |
| 4 | −23 | at size 100..250, may spawn type 1 smoke (Dust2) | flame turning into smoke; burning vehicles |
| 5 | 0 | removed when z ≤ 23000 | bubble (never spawned) |
| 6 | −13 | – | dark smoke (Dust2, subtractive): damaged cars, exhaust, chimneys |
| 7 | −66 | like type 4, smoke size 350+rand150 | explosion fireball (Flame.N) |
| 8 | −1 | – | ground decal: blood pool (Blood, 0x30000), scorch (Dust2, 0x10000), skid marks |
| 9 | −7 | – | spray: bullet sparks (Flare), blood spray, script Gas |
| 10 | **+300** | frame = size/2500 (0..3); removed at size > 9999 | **shockwave** ring |
| 11 | −5 | vz += 17; drawn as a lens flare (AddFlare at −size) | explosion flash |
| 12 / 13 / 14 | 0 | wrap around the view | snow / rain / rain splash |
| 15 | −2 | may spawn type 4 flames; loops Flames | fire source (script only) |

**Textures** (`effecttextures/`, 64×64): Dust, Dust2 (alpha variant), Flare, Flame.1..4, Blood
(the gore option swaps in Blood.dark), Gas, Bubble (unused), Shad (shadows), DirtSkidmark, Dirt.

**Blend.** The blend field selects a 64 KiB table in `General/Alpha.256`:
- 0 = additive (default);
- 0x10000 = subtractive (smoke, scorch, shadow);
- 0x30000 = tint (blood).
- In Warp3D, blend 1 and 3 map to SRC_ALPHA / ONE_MINUS_SRC_ALPHA and everything else to
  ONE/ONE. [C mechanism, I naming]

**Shockwave.** Texture `Shockwave.FAB`: F = frame 1..4, A = x half (0 left, 1 right), B = y half
(0 top, 1 bottom). The 128×128 ring is drawn as 4 quadrant sprites around the centre.

**Lens flares.**
- `AddFlare` (OS4 `FUN_0103451c`, 68k `FUN_0003da7e`) keeps at most **199** per frame ("Too many
  flares").
- Occlusion: AddFlare writes 0 into the framebuffer pixel; if that pixel is overdrawn the flare is
  hidden.
- Option: 0 off, 1 glow only (LensFlare.2xx), 2 full: adds the LensFlare.11xx / 12xx groups and
  the ghosts .1, .3_4, .5, .6_7, .8, .9, .10 along the centre→flare line (positions and scales in
  `notes_effects.md` §1.7).
- Also used for lamps and headlights.

**Effect lights** (lighting option on):
- type 2: (size·140000, ·120000, ·100000) + random;
- type 7: (·50000, ·40000, ·30000);
- type 11: (·900000, ·900000, ·700000);
- type 15: (·100000, ·70000, ·50000).

**Debris fragments.** A separate list, max **500** per layer, one triangle per model face (persons
min(nfaces, 20)). Spawner: OS4 `FUN_01035438`, 68k `FUN_00038f44`.
- Velocity: v + rand(S) − S/2 with S = 250 (persons 100); vz −= rand(S) + S; spin ±125.
- Per update: gravity vz += 50. At ground z 22500: 1/3 chance to bounce (vz = −vz/2), else removed.
- Trail: person fragments leave blood puffs, others Dust2 smoke.
- On removal: a blood pool (persons) or 3 dust puffs.
- `TankBits` is only a texture-name lookup used for tank debris [I].

### 5.2 Explosions [C]

**`Explosion(owner, x, y, z, P, layer, src)`** (OS4 `FUN_0104405c`, 68k `FUN_000cd22e`):
```
PlaySound(0 Explosion, 0, 50e6, P < 140e6 ? 12000 - P/20000 : 5000, x,y,z+100)
PanicPeds(x,y,z)                 // civilian peds within 10000 units: Ow, flee 100 ticks
for each object o != src (class 2 skipped except barrier/cone/dummy/bullet):
   d  = (o.pos - (x,y,z)) >> 8           // 256-unit cells (1/8 square)
   r2 = |d|^2 ; u = (r2>>4) * isqrt(r2)  // ~ |d|^3 / 16
   k  = person ? (u ? 5u : 1) : max(u>>1, 1)
   q  = (P>>4) / T(o)
   o.v += clamp(q*d/k, ±1000)            // knock-back, per axis
   if o.filter in {-1,1,3}: o.health -= person ? q/k : (q>>4)/k
   score/crime if player-caused (§7)
flash DAT_0114c748 += P/250000 (decays ×3/4 per tick; brightens the palette; no camera shake)
AddEffect(8,  Dust2, x,y,z, 0,0,0, P>=120e6 ? 6000 : max(P/20000,1000), layer, 0x10000)  // scorch
AddEffect(10, -,     x,y,z, 0,0,0, 300, layer, 0)                                      // shockwave
AddEffect(11, Blood, x±200, y±200, 21000, 0,0,0, 1000, layer, 0)                       // flash
```
- There is no hard radius: damage falls off as ~1/d³.
- Distances at which a person (T 90) takes ≥ 100 damage [I, computed]: missile (2.5e7) ≈ 1 square;
  grenade (6e7) ≈ 1.4 squares; remote bomb (1e9) ≈ 3.5 squares; exploding car (1400·25000 =
  3.5e7) ≈ 1 square (151 at 1 square, 19 at 2).

**`Ignite(obj)`** (OS4 `FUN_01044b70`, 68k `FUN_000cda5e`). Used when a vehicle's health reaches
≤ 0, and on missile impact.
```
burn = 100; speed = 0; state = 5 (burning wreck)
v += (rand(100)-50, rand(100)-50, -(200+rand(300)))            // hop
n = (T>>10) + 2 ; c = clamp(n/2, 2, 6)
c × fireball type 7 (Flame, ±800, size 2500+rand1500, layer 3)
c × burning debris type 2 (size 800)
Explosion(owner, pos.z+500, P = T·25000)
```

**`Destroy(obj, power)`** (OS4 `FUN_0104465c`, 68k `FUN_000cd6ae`). Used when health < −70
(`−71` = "blow apart" marker), by the grenade fuse (3000), the remote bomb (50000), and on
destruction (power 0 → T/2).
- Two fragment passes, then state 6.
- Person: 12 Squelch (15e6, 18000) and a blood pool (size 500+rand300).
- Otherwise: n = min((power>>8)+2, 20); fireballs and debris; then `Explosion(P = power·20000)`.
- **Chain reactions** come about because these blasts damage other vehicles, which then ignite and
  explode through the same health checks.

### 5.3 Blood, smoke, fire on vehicles (per-frame health pass, OS4 `FUN_010542b8`) [C]

- **Health 1..50, not burning:** with chance `rand(health+5) ≤ 5·ticks`, a person drips blood
  (type 8, 50+rand50) and a vehicle emits **damage smoke** (type 6 Dust2, subtractive) from the
  rear: v = v/2 + (30+rand30, rand20−10, −10), size = radius + rand300. Smoke gets more frequent
  as health falls.
- **Burning (timer ≥ 2):**
  - loop 13 Flames (volume radius·5000, ·10000 while the timer ≥ 16) and a dynamic light;
  - with chance rand(3) < ticks, a type 4 flame at pos ± 150, vz −600, size radius + rand150
    (1.5·radius + rand300 while the timer ≥ 16);
  - timer −= ticks, so a burn lasts 100 ticks = 3.3 s.
- **Burn timer reaches < 2:** a person dies (state 7) and a vehicle's wreck goes out.
- **Dead person:** with chance rand(6) ≤ ticks, a blood pool (120+rand120).
- **Run-over blood:** a type 8 pool (size 16·min(dmg,50)+50+rand50) plus 3 type 9 sprays (size
  8·dmg+80+rand(2·dmg)).
- **Weather:** snow and rain particles (types 12–14) on layer 3.

## 6. Health, damage and hit detection [C]

**Health.**
- Every object starts with **100** health. There is **no regeneration** and **no max clamp** on
  the script's `AddHealth` (OS4 `FUN_01040f70`).
- While driving, the player's on-foot health is kept in OS4 `DAT_012a6304[player]` and restored on
  exit.

**Hearts HUD** (OS4 `FUN_01018518`). 5 hearts 10 px apart (20 px in hi-res). Heart i is `heart.big`
if health/20 ≥ i, else `heart.small`.

**Death thresholds.**
- Person at ≤ 0: state 7 (dying), sound 15 LongAaah, burn/dying timer 100.
- Vehicle at ≤ 0: Ignite.
- Anything < −70: Destroy (gibs / obliteration).

**Damage filter `+0x3ca`.** −1 = vulnerable to everything. Otherwise the object only takes damage
from the matching class: 4 minigun, 7 pistol, 5 shotgun, 2 flame, 1/3 explosion. Used for mission
objects [I purpose].

**Collision damage** (OS4 `FUN_01056c58` pairs, `FUN_010cd198` walls, `FUN_01045288` scenery):
- **Object vs object:** `dmg = ((dvx²+dvy²+(dvz/2)²) >> 7) · min(mA,mB) / mSelf`, then
  - a person takes /8 if it is the player and /2 otherwise;
  - a vehicle takes /32.
- Vehicle–vehicle thud: 5 Thud if dmg < 11, else 75 Thud2.
- **Run over:** a person is only hurt if the vehicle's |speed| > **50**, or > **100** if the
  person is the player. dmg > 10 plays Ow. Health < 1 → dead ("Flattened!" for the player).
- **Cube wall:** `dmg = v² >> 14` per axis, bounce `v = −|v|/2`, pushed 32 units out.
- **Scenery** (non-person): `dmg = (dvx²+dvy²) >> 12`.
- **Ground:** vz > 160 plays 30 HitScenery and bounces with `vz = 128 − |vz|/4`. **No fall
  damage.**
- **Water:** 43 Splash (volume T·8000/20000). Models with +0x4904 are destroyed (−71). **No
  drowning damage for people was found** [I], even though phrase 2018 "Drowned!" exists (open).

**Collide grid** (`AddCollideObj` OS4 `FUN_01045288` / 68k `FUN_000cdfdc`; `RemoveCollideObj`
OS4 `FUN_01046778`; `RemoveAllCollideObj` OS4 `FUN_01047848`):
- Max 32 objects; object i owns bit `1<<i`. The grid is u32 masks [128 x][128 y][3 layers]
  (OS4 `DAT_012b1bec`, offset x·0x600 + y·0xc + layer·4).
- Footprint AABB: a person is a rotated square of radius r; a vehicle is a rotated box
  [−rear, +front] × [±r]. It is converted to squares (>>11), clamped 0..127, and stored at
  `+0x484..+0x490`. "Called twice" errors guard double add/remove.
- Adding also resolves scenery collisions (scenery model radius `+0x4910..0x4918`), push-back,
  damage, and crate breaks by projectiles.

**Pair test.**
- Object i ORs the masks over its AABB ±1 and tests only objects j < i.
- Shapes: a person or projectile is 1 circle. A vehicle is 3 circles along its heading at
  {r − rear, front − r, 2·front/3 − r}.
- Contact when `dx²+dy²+(dz/2)² < ((rA+rB)·17/16)²` and |dz|/2 < 20000.
- Motion is sub-stepped per tick (finer when |v·ticks| ≥ 30).
- Response: momentum exchange weighted by the smaller mass.

**Masses T** (`+0x48ec`):

| Group | Masses |
|---|---|
| People | 90 (some 95 / 100 / 120) |
| Light cars | pug 900, pug GTI 850, pug racer 700, scooby 1235, fjord 1300, taxi 1380 |
| Mid cars | mundaneo 1400, diblo 1400, evo 1400, helicopter 1500, police 1600, vapour 1800, pickup / hotrod 1800, vapour racer 1800 |
| Heavy vehicles | ice-cream van 2000, van 2200, policevan 2500, bombtruck 2500, rocketcar 2500, limo 3000, RC car 5000, bus 10000, boat 10000, truck 12000, tank 40000 |
| Weapons / props | missile 1000, bomb 200, dummy 115, barrier 50, cone 20, bullets / grenade / petrol 1 |

## 7. Scoring, multiplier, lives, rampage, multiplayer

Offsets below are OS4; the 68k struct is 2 bytes lower for these fields
(OS4 score +0x384 = 68k +0x382; wanted +0x330 = 68k +0x32e). Object table OS4
`DAT_012a6270[idx]`, player object index `DAT_012a62f4[player]` (68k `DAT_0030c318[]`,
`DAT_0030c3a0`).

### Score (money)
- Stored per controlled object at `+0x384` (i32). When the player enters/leaves a
  vehicle the score and wanted level are copied to the new controlled object
  (OS4 `FUN_01052a4c`: `veh+0x384 = ped+0x384; veh+0x330 = ped+0x330`). CONFIRMED.
- Generic add: OS4 `FUN_01041408(n)` (script API, thunk at 0x010d1018) /
  68k `FUN_000efe72`: `score += n * multiplier`, then spawns a floating label
  (`FUN_0101c9a8(x,y,z,2)`). CONFIRMED.
- HUD high-water `DAT_0114c420` = max score seen (used for the high-score table).

Per-event awards (all × multiplier; all only when the damage source is a
player-owned object, i.e. attacker `+0x344 >= 0` and victim `+0x344 == -1`;
`+0x344` = owning player number, −1 = AI) — CONFIRMED in OS4 `FUN_01043924`,
`FUN_0104405c`, `FUN_01056c58`, `FUN_01052a4c`:

| Event | Points |
|---|---|
| Kill a civilian (person, or occupied civilian vehicle destroyed) | 1500 |
| Kill a police officer / police vehicle (`model+0x490a == 1`) | 2500 |
| Copkiller bonus: every 11th cop kill (counter `DAT_0114c6a4` > 10, then reset to 0), single player only | +100000, message 2037 "Copkiller bonus!" |
| Carjack a civilian car (occupied) | 1000 |
| Carjack a police car | 1500 |
| Explosion damage to an object | min(6000, q / (k>>5)) with q, k from §5.2 (6000 when k>>5 < 1) |
| Punch an NPC | 500 |
| Run someone over / ram (vehicle collision) | min(6000, vmax²>>9), vmax = max of abs(vx), abs(vy) of the impact velocity scaled by mass ratio |
| Burning (flamethrower/fire contact): per tick damage = min(health+0x47, ticks·3000/mass), ≥1 | damage × 32 (OS4 `FUN_01056c58` ~l.46400) |
| Other projectile impacts (several branches) | min(8000, k/dist) (INFERRED: k = impact energy) |
| Irony bonus (phrase 2027): a player kills someone with the vehicle that belonged to the victim (`victim+0x340 == killer+0x498`) | 10000 |
| Script `AddScore` | n |

### Multiplier (green number on HUD)
- `DAT_0114c74c` (68k `DAT_0030c3b0`). Set to 1 at level start and **reset to 1 on
  every death**. CONFIRMED.
- +1 each time a "Mission complete!" message (phrase 2022 = 0x7e6) is shown
  (68k `FUN_000b4074` → `FUN_000eff00`; also plays sound 26 MissionComplete at
  11025 Hz); capped at **4**. "Mission failed!" (0x7ec) plays sound 25 MissionFail,
  no change. CONFIRMED.
- HUD: drawn as icon `small.x` followed by digit icon `small.<n>` (OS4 `FUN_01019bd8`).

### Lives
- 3 at level start (68k `DAT_0034c428 = 3`, OS4 `DAT_0114c6d0`); script API
  `AddLives(n)` (68k `FUN_000efede`). Shown as `small.Person` × digit. CONFIRMED.
- On player death (health ≤ 0) or arrest: death timer `DAT_012a65e0[p] = 0x50`
  (80 ticks = 2.7 s) and the current score is saved to `DAT_012a65f0[p]`;
  in single player the crime summary is printed (`FUN_010bcf10`). The death message
  depends on cause: 2016 Wasted (killed on foot), 2017 Busted, 2018 Drowned,
  2019 Toasted (burnt), 2020 Splattered (explosion on foot), 2021 Flattened (run
  over), 2024 Nuked (killed in vehicle), 2025 Obliterated (vehicle blown up).
- When the timer runs out: lives −1, multiplier = 1. If lives > 0 a new player
  person is created (`FUN_01046ee4(model)`, placed by `FUN_0106ef6c`), state 2
  (player-controlled), `+0x344 = 0`, **score restored** from the saved value (no
  score penalty). If lives reach 0: "Game over!" (2026), game state
  `DAT_0114c730 = 2`, 250-tick timer (then the rampage high-score check). CONFIRMED.

### Rampage mode and high scores
- Rampage = single-player free play (menu 2196 "RAMPAGE SETUP"); options City and
  Police aggressiveness 0..5 (phrases 2491..2496). The aggressiveness replaces the
  level-based value (story mode: aggressiveness = level number 0..5; user maps 2).
- High scores: `Maps/<n>.scores` (`n` = 0/1/2, or user map name) — OS4 load
  `FUN_01029b3c`, save `FUN_0102a0fc`, setup screen `FUN_0102a428`, entry
  `FUN_0102ca08`. CONFIRMED.
- File format: 6 tables (one per police aggressiveness) × 5 entries, each entry
  `char name[30]` + `i32 score` (BE), written table-major; then `u32 checksum` =
  `~(0x5dc1dcd1 ^ XOR of words)` where the XOR covers, per table, the first 7 u32 of
  each of the 5 names (28 bytes each) and the 5 scores. Total = 6·5·34 + 4 = 1024
  bytes. Bad checksum or missing file → defaults: name "Travis", table t scores
  `500000·(t+1)` halving: 500000·(t+1), /2, /4, /8, /16.
- A new score enters if > 5th entry of the current aggressiveness table; name up to
  25 characters (phrase 1611). Score used = `DAT_0114c420`.

### Multiplayer (2–4 players, split screen)
- Kills per player `DAT_012a6730[4]`: killing another player +1, killing yourself −1
  (killer id = victim's `+0x4b0` "last attacker player", victim's own id `+0x4ac`).
  CONFIRMED.
- No lives in multiplayer: after the 80-tick death timer the player respawns.
- Match ends when any player reaches the required kills (`DAT_0114c2e8`, phrase
  2329); messages 2186 You win! / 2187 You lose! / 2188 You draw!; sound 26.
- Crimes/wanted level and route-based police are unused in multiplayer (editor guide).

## 8. Open questions

- The algorithm the map editor uses for the `.route` next-hop table: BFS matches 99.98 %
  (§1). Use BFS.
- Ammo and weapons at level start and after respawn (mission scripts / object defaults).
- Drowning ("Drowned!") and the `burnt` texture trigger.
- Whether a missile that hits scenery (not a cube wall) explodes.
- OS4 addresses of the main AI, firing and projectile code (`FUN_0105f594` did not decompile).
- Rampage end condition besides game over (`DAT_0114c7c4`).
