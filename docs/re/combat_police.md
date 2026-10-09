# Combat, police and routing

Reverse-engineering notes for the Payback reimplementation. They cover the route graph,
police, weapons, pickups, effects, damage and scoring.

Sources: 68k Update 7 `Payback.exe` (`analysis/ghidra_68k`, flat image at 0x10000) and the OS4
port (`analysis/ghidra_os4`). The OS4 main per-object update (`FUN_0105f594`, 39680 bytes, the
counterpart of 68k `FUN_000e3a58`) does not decompile; the AI, firing and projectile flight are
inlined in it, so they come from the 68k build. OS4 landmarks inside it, by disassembly [C]:
fire sounds Minigun 0x1062a4c, Pistol 0x1062c14, Shotgun 0x1062ddc, FireMissile 0x1066024,
Flamethrower 0x1068050, Punch 0x1067fb8; projectile spawns (`bl FUN_01046ee4`) petrol 0x1061a44,
minigun 0x1062a90, pistol 0x1062c58, shotgun 0x1062e24/0x1062f30, missile 0x106603c, bomb
0x1067468, grenade 0x1067df0; Ricochet 0x1060280/0x10671b8; GrenadeBounce 0x1067b0c..0x1068bf4;
AI steering atan calls 0x1064050..0x1065750; cop drop-off `FUN_01052714` at 0x1064b44; drowning
0x1063c90. Helpers: atan `FUN_010bcc18`, siren `FUN_0105e02c` (68k `FUN_000e2a50`), PanicPeds
`FUN_010435f4` (68k `FUN_000ccabe`), heli steering `FUN_010ccc5c`, LOS `FUN_010707f0`.
Everything else is cross-checked in both builds.

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
  table-driven: `n·tbl[++k] >> 9`. [C] A second counter over the same table, `random3` (68k
  `FUN_0003fce6`, OS4 `FUN_0103acc4`, Mac `random3__Fi` @ 10083530), feeds every cosmetic draw:
  effect updates, chimney smoke, the weather spawn, effect lights, text wobble. The port's
  `sim::VisualRng` is that stream; render code never draws from the game's `sim::Rng`. [C]
- **PlaySound.** 68k `FUN_000170ce` / OS4 `FUN_0100815c` take (id, loop, volume, rateHz, x, y, z).
  z = −1 means non-positional. There are 500 slots. [C]
- **Sound ids.** These follow the load order in OS4 `FUN_01005864`. [C]

  | Ids | Sounds |
  |---|---|
  | 0–9 | 0 Explosion, 1 Engine, 2 Skid, 3 CarHorn, 4 Footstep.stone, 5 Thud, 6 ShipHorn, 7 Burp, 8 Ambulance, 9 CarStart |
  | 10–19 | 10 Heartbeat, 11 Siren, 12 Squelch, 13 Flames, 14 TelephoneRing, 15 LongAaah, 16 SqueakyVoice, 17 FireMissile, 18 GrenadeBounce, 19 Minigun |
  | 20–29 | 20 PickupWeapon, 21 ShotgunFire, 22 GarageDoor, 23 Bleep, 24 TickTock, 25 MissionFail, 26 MissionComplete, 27 TankEngine, 28 BigGunFire, 29 Ricochet |
  | 30–39 | 30 HitScenery, 31 ThumpingMusic, 32 ChangeSelection, 33 ChangeSelection2, 34 Footstep.grass, 35 Footstep.metal, 36 Footstep.sand, 37–39 Ow1–3 |
  | 40–59 | 40 Doorslam, 41 Flamethrower, 42 Click, 43 Splash, 45 ChangeSelection3, 47 ObjectAppear, 50–52 Footstep.sand (snow, slush, ice), 53 Pistol, 59 Punch_Hit |
  | 65 | Footstep.water |
  | 75–76 | 75 Thud2, 76 Screams |

  The step sound is the top-face texture's `+0x1040` field (default 4 stone); ids 50–51 leave
  footprints (68k `FUN_000e3a58` ~122786). [C]

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
precomputed by the map editor when it saves (`SaveRouting` @ 10077940, only if the graph
changed). **Editor algorithm** (CONFIRMED, Mac `MapEditor (OS X)`, decompile in
`analysis/ghidra_mapedx`; reproduces all 31 shipped tables bit for bit — maps 0–2, *The
Killing Zone* and the 27 Mac user-map routes):

```
CalculateRouting @ 100776a0:
  visited[0..1023] = -1                       # once, never cleared again
  for a in used nodes (index order):
    next_hop[a][*] = 0
    for b in used nodes (index order):
      slot = CalculateRoute(a, b)             # -1 for a == b or no route
      if slot != -1: next_hop[a][b>>2] |= slot << ((b&3)*2)
CalculateRoute(a, b) @ 100775b0:
  best = 2e9; slot = -1; stamp = b*4
  for k in 0..3 with link[a][k] != 9999:
    visited[a] = stamp
    d = CalculateDist(link[a][k], b, stamp); if d < best: best, slot = d, k
    stamp += 1
CalculateDist(s, b, stamp) @ 10077480:       # BFS, FIFO of 5000 (node, hops)
  push (s, 0)
  while queue: pop (n, d); if visited[n] == stamp: continue
    visited[n] = stamp; if n == b: return d
    push (t, d+1) for every link t of n
  return 2e9
```

So the metric is hop count (first link wins ties), but the visited stamps `b·4 + k` are
shared by every source `a` and never reset, and the target is only recognised when first
expanded: a stamp left by an earlier search hides nodes (sometimes `b` itself) from a later
one. That is why 0.01 % of the pairs are not shortest paths (map0 12, map1 15, map2 30;
`analysis/combat/route_check.py` compared plain BFS). Port:
`payback_formats::Route::calculate_routing`, checked against map 0 in the tests; the game
still loads the shipped tables. Every node reaches every other node in the shipped maps.

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

Aggressiveness drives: the wanted decay period, the police shooting chance, the pursuit trailing point
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
- When the player is on foot, `byWeapon == 0` bumps into vehicles are ignored. [C] Mac `CheckHitObject` @ 100542d0:
  the surviving-vehicle branch (wanted, shunt / criminal damage, `ObjectDamaged` and so ACTOR_ATTACKED, the
  `+0x964` flag) runs only if `objects[player].model+0x8165 == 0` (the player's own object is a vehicle) or
  `param_2 != 0` (weapon, shockwave). `CheckForCollisions` @ 1006a150 calls `CheckHitObject(victim, 0)` for every
  touching player-side/NPC pair whatever the speed or damage (a vehicle victim has no speed gate), so on foot that
  call does nothing for a car, parked or moving; from a car it counts even at zero damage and even when the
  player's car stands still and is the one hit. The destroyed-vehicle branch has no such test. (The port's older
  `player speed == 0` gate was not the original's.)
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
| `tank.obj` | 0 | – | – | never random traffic; script-only [C] |
| `helicopter.obj` | 0 | – | – | never random traffic; script-only [C] |

Traffic keeps a randomly drawn model only if `rand(256) < model+0x4934` (the weight); tank and helicopter
have weight 0, so they are never kept (OS4 `FUN_01047af4` ~37785; weights set in `FUN_0104e13c` ~41164 /
~41247). [C]

`police_red` / `police_blue` and `policevan_red` / `_blue` are only light-flash frames.

**Spawning** (scheduler 68k `FUN_000d0cfc` / OS4 `FUN_010485cc`; drone-at-square 68k
`FUN_000cf5cc` / OS4 `FUN_01047af4`) [C]:

- **Object cap.** Spawning runs every frame while `objCount−1 < min(persistentCount + 8, 20)` (68k);
  the **Mac** (`AddVehicles` @ 10059d40) uses `+ 11`. persistentCount (`IncExtraObjs` @ 10051d30) counts
  the objects flagged `+0x934`: the map's object list, script `AddObject*`, and two per player in
  multiplayer. The cap is re-checked before every try. The hard limit is 32 objects. **There is no
  police-specific cap.** Port: `traffic::spawn_ambient` follows the Mac (`traffic::Persistent`).
- **Window.** The view window has radius `r = ((camZ + 1500)·8 >> 11) + 5` squares around the
  camera square.
- **Edge.** Spawns go on the window edge ahead of the player. The edge comes from the dominant
  axis of the player's velocity (x only when `|vy| < |vx|`; stationary: `random(3)` = N, E or S, never
  W). Each of the `ticks·2` passes per frame tries every cube level 0..2, each at its own random
  square of that edge. In multiplayer each pass serves one player, moving to the next after a pass that
  spawned, and a square inside another player's window is refused. [C, Mac `AddVehicles`/`AddThing`,
  68k `FUN_000d0cfc`/`FUN_000cf5cc`]
- **Square test.** The square must exist and its **top face type** (cube `+9`) must not be Off (0) or
  Reflect (4, water) (68k `FUN_000cf5cc`) [C]. The square and
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
  removed at once. (68k `FUN_000dc5ac` ~117128, Mac `DoCollideStuff` @ 10067b50 l.25138; same list in both.)

**Pursuit selection.** Applies to police cars that are not vans, in states 3/8/9/10/11/0x14/0x15
(not 0xf) [C]:
```
if wanted < 3: state = 3
else: target = player; skill = wanted + (aggr < 4 ? 0x5c : 0x5e); if state not in (9,11): state = 10
```
Script orders 0x14 (flee, trigger 0x43) and 0x15 (drive-to, trigger 0x44) are overwritten by this, so a
scripted police car drops its order (the arrival trigger never fires): traffic below wanted 3, pursuit
from 3. Goto 0x0f (triggers 0x17/0x32) is exempt. Port: `select_pursuers` resets such an `Ai` order to
ambient. None of the shipped scripts gives 0x43/0x44 to a police car (their only targets are vans). [C]

**Route following (states 8/10)** [C]:
- Own node is `+0x3bc` (9999 → take the nearest node to self).
- The target's nearest node `target+0x3be` is refreshed by `NearestNode(target>>3)` whenever the
  countdown `target+0x3c0` is 0, which then resets it to 20. The countdown drops 1 per object
  update (per frame).
- The steer target is `node.xy << 3`.
- The node is reached when |dx| and |dy| < 300 + |5·speed|. The **Mac** (`do_Other` @ 10146280) uses
  `300 + (speed² >> 7)` for pursuit (states 8/10) and keeps `300 + |5·speed|` for flee (0x14); at speed
  100/300/600 that is 378/1003/3112 against the Amiga's 800/1800/3300. The port follows the Mac
  (`vehicle_ai.rs` `follow_route`). CONFIRMED.
- On arrival: if own node == target's node, state 8→9 / 10→11 (**leave the graph and drive
  straight at the target**). Otherwise own node = `NextHop(own, targetNode)`.
- There is **no transition back** from 9/11 to the graph.

**Direct chase (9/11)** [C]:
- Police aim at `target.pos + target.v·(5−aggr)·−3` (Mac `do_Other` @ 10146280 l.118091, 68k ~124291). Every
  object moves by `pos += v` (Mac `MoveObject_Vehicle` @ 1006e890, `MoveObject_General` @ 1006e4f0, 68k
  ~123574), so this point lies (5−aggr)·3 ticks **behind** the target: the police follow the player rather
  than cut it off (they lead only at aggr 6). CONFIRMED in both builds. Port: `pursuit.rs` `pursue`.
  Non-police aim straight at the target.
- Steering uses atan2 with turn rate from model +0x48e4.
- Drivers with skill ≥ 99 and hp ≥ 50 skip the obstacle / traffic-light look-ahead, so they ram.
  Skill < 70 stops at red lights. Police (≥ 95) ignore lights.
- Skill ≥ 98 uses the uncapped speed branch.
- The driver itself (steering dead zone, look-ahead, cruise bands, the ±1500 aim clamp on route legs) is
  shared with traffic and script gotos: vehicles.md §6.2. Consequence: at skill 95–97 (wanted 3–5 with
  aggr < 4) the look-ahead sees the player's car as an obstacle, so the car brakes behind it and then drops
  a cop; only skill ≥ 99 cars ram. [C]
- Port (`sim/pursuit.rs`): the target's node and countdown are one `vehicle_ai::TargetNode` (the only
  target is the player), ticked once per frame, refreshed by the first pursuer that finds it at 0, as in
  the original; a missing route file sends pursuers straight to state 11.
- Leaving pursuit (wanted < 3) only rewrites the state byte: the car keeps its skill, route node and
  steering target, and the lane code resumes once it reaches that target. [C, Mac `do_Other`]

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

Spawning (68k `FUN_000cfe0c`, OS4 `FUN_01047134`, Mac `AddPolice` @ 10058ba0) runs only at **wanted ≥ 5**.
The scheduler makes `(wanted−4)·2 + 1` attempts per cube level per pass (3 at wanted 5, 5 at wanted 6),
right after that level's traffic try, on random squares of the same spawn edge; it does not stop after
a success (the occupancy test keeps runs apart).

Requirements:
- The square has bit 0x40, x in 1..0x7e and y in 1..0x7e.
- In-memory cube byte +8 (the slope byte) is non-zero: the cube exists (slope 0 = Off is neither drawn nor solid). The port tests the slope.
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
  door = player.pos + rot(player.48f8 forward, player.48f4 sideways)   // the door point itself:
                                     // no lead, no cop radius; (0, 0) for a player on foot
  d2   = ((cop - door).x>>4)^2 + ((cop - door).y>>4)^2
  reach = player in vehicle ? cop.48c4 >> 3 : (cop.48c4 + player.48c4)*3 >> 5
  if d2 < reach^2 and |player.speed| < 0x1e and same cube level:
      on foot   -> BUSTED
      in a car  -> car.door += min(100*ticks, 0x370 - door); if door >= 0x370 -> BUSTED (dragged out)
  p = aggr < 2 ? (wanted > 4 ? (9-wanted)*10 : none)
    : aggr < 4 ? (wanted > 3 ? (9-wanted)*10 : none)
    :            (wanted > 3 ? (7-wanted)*10 : none)
  fire this frame if rand(p) < ticks
```
- The arrest point and reach are CONFIRMED in Mac `do_Person` @ 1013fbd0 and 68k ~121940
  (`-*(int*)(model+0x48f4)`, no cop radius). The `-(cop.48c4 + player.48f4)` form belongs to the
  guard range test (state 0x10) and the steering target. Port: `police::cops` uses the real radii
  and the target's door offsets, and skips cops while the player is stunned (+0x490).
- Firing has **no range or line-of-sight test**.
- The cop turns toward `player.pos + player.v·(5−aggr)·−3` (Mac `do_Person` @ 1013fbd0 l.115714, 68k ~122108),
  a point (5−aggr)·3 ticks *behind* the player (`pos += v`), and walks straight at it. Cops on foot
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

**Arrested while driving** (68k ~121975–121996). The cop stands at the driver's door and opens it
100·ticks per frame (the player's own door-closing pulls it back by 50·ticks). When it reaches 0x370
and no death timer is running:
```
if rcBody[p] != -1: obj[rcBody[p]].health = -71; rcBody[p] = -1   // DAT_003480a0: the body left
                                                                  // standing while driving the RC car
FUN_000dab40(p)            // the normal exit (people.md §6.3): the player is pulled out on foot at the
                           // door, the car is left in state 5 with its door open (0x370)
deathTimer = 80; savedScore = score; CrimeSummary(new person, 1); message 2017;
newPerson.+0x490 = 1e9; sound 37+rand(3)
```
So the **person** is the one frozen, beside the parked car; the car itself is not harmed. Port:
`police::bust` marks the car `PullOut`; `boarding::use_pressed` runs the shared exit `eject` and
freezes the new person (`Busted`, state dead) for the respawn.

When the timer expires (68k `FUN_000ec686` ~127679; OS4 `FUN_010692bc` ~49630):
- lives −1 and multiplier = 1;
- if lives < 1: game over;
- otherwise a new player person is created at a **random start point**, avoiding the last 4 used
  (68k `FUN_00043c96`), or at the script's respawn square if one is set (`DAT_0026c040 != −1`);
- **the score is restored**: there is no money penalty.

The new player object is freshly allocated and zero-filled, so at level start and after every respawn
the player has fists only (`+0x35e = 0`), no ammo (`+0x362..+0x37e` all 0) and wanted 0; only the
score is restored (68k `FUN_000ec686` ~127150 / ~127750 via `FUN_000cf11e`; `FUN_0003feb2`). The
default-initialiser `FUN_000a2b80` sets health 100. Weapons come only from pickups: no script gives the
player weapons. [C]

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
- increments the firearms counter (+0x470), **but only for a grenade throw, a bomb drop, a bazooka
  shot and a shotgun shot** (plus the tank cannon in `DoVehicleFire` @ 10070c20); pistol, minigun
  and flamethrower do not count [C: Mac `do_Person` @ 1013fbd0, 68k `FUN_000e3a58` ~123180,
  ~123300, ~124022]. Port: `combat::fire_weapons`, `vehicle_ai::tank_cannon` (the player's shots);
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

**Fist** (68k ~123605; [C: Mac `do_Person` @ 1013fbd0 l.117378-117445]).
- Targets: every person (class 1, armed or not) in object-table order but the puncher, with
  |Δz| < 250 and |Δx|, |Δy| < 3·r of the hit point pos + 256·dir, and
  `LineOfSight(pos, z − 0x400 → target, z − 0x400)` clear.
- Any target in reach: 59 Punch_Hit (4e6, 22050) once, at the puncher, after the scan.
- Each target with health ≥ 0 says Ow (`0x25 + random(3)`, 5e6, 8000), takes **30** health,
  flat (not divided by T); at < 1 it enters state 7.
- On an NPC (owner side `+0x824 == −1`) the player's object (`objects[DAT_1017c60c]`) gets
  500 × multiplier and `CheckHitObject(t, 0)` runs, whoever punched (no side test on the
  puncher). Otherwise only `ObjectDead` when the hit killed it.
- A punch also breaks a crate in the square at pos + 512·dir.
- Port (`sim/combat.rs` `fire_weapons`): as above. Before, the targets were only people with
  weapons (so pedestrians could not be punched), in query order, without the line of sight, and
  the score was a flat 500 (CB-1, CB-2, DT-3).

**Remote bomb.**
- On drop, the bomb's id is stored. The next press detonates `Destroy(bomb, 50000)` (§5.2,
  P = 1e9) and points the camera at it.
- It notifies the script (`FUN_000a653a`).
- If the bomb is destroyed by damage (health ≤ 0, e.g. a script `ExplodeObject`) it still goes off in
  full, `Destroy(bomb, 50000)`, and its owner's bomb slot (+0x860) is cleared [C: Mac `DoCollideStuff`
  @ 10067b50]. The Amiga (68k `FUN_000dc5ac`, OS4 `FUN_010542b8`) leaves the slot pointing at the
  dead bomb. Port: `sim/combat.rs` `move_projectiles` follows the Mac.

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
| bullet | −1/tick → 100 ticks | ≈ 25600 units ≈ 12.5 squares | back off 1 tick; 29 Ricochet (pitch 11500+rand 1000 drawn first, then vol 4e6+rand 2e6); 2 Flare sparks (effect type 9) of size rand(150)+150 at pos ± 100, v = rand(80) − 40 − v/8 per axis; removed |
| pellet | −2/tick → 50 ticks | ≈ 6 squares | as the bullet, but **3** sparks of size rand(150)+**250** [C: Mac `do_Weapon` @ 1013e390, 68k ~121790; sim `random`] |
| petrol | −5/tick → 20 ticks | ≈ 2.5 squares | ground-fire effect (type 0xf, life 800, looping Flames). Each tick a random orange light and a type 1 flame |
| missile | none | until impact | wall/ground: fire effect, then **Ignite-explosion** of itself (§5.2, P = 1000·25000 = 2.5e7) |
| grenade | fuse −1/tick from 80 | thrown | walls reflect vx/vy with 18 GrenadeBounce (4e6, 8000); ground: vz = −vz/4 (sound if vz > 500); friction 8·ticks on ground, 1·ticks in air; trailing smoke; fuse < 2 → `Destroy(self, 3000)` (P = 6e7) |
| bomb | none | – | bounces like a grenade (sound at 5000 Hz) |

**Hits on objects.** The object pair loop is 68k `FUN_000de0b6` / OS4 `FUN_01056c58`. A projectile
never hits its own shooter.
- **Minigun bullet:** health −= **10000 / T** (filter +0x3ca ∈ {−1, 4}).
- **Pistol bullet:** health −= **5000 / T** (filter {−1, 7}; Mac `CheckForCollisions` @ 1006a150 and 68k. The OS4 "only −1" reading is a decompiler rendering of the same mask test [I]).
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
pellets, missiles and petrol also break a crate on that square. A missile hitting scenery is
removed **without exploding** (68k `FUN_000cdfdc` ~111148, OS4 `FUN_01045288` ~36606 only set
state 6, which `FUN_000dc5ac` removes before its explosion pass); a missile explodes only on a
cube wall/ground (`FUN_000e3a58` ~121863 → health 0 → `FUN_000cda5e`) or an object. [C] Vehicles driving into a **snowman**
(+0x4908 0xfe) destroy it.

### 3.4 Flamethrower and buildings

Petrol leaves short-lived ground-fire effects (type 0xf), which deal **no damage**. **No
"building on fire" state exists**: cubes cannot burn. [I, from the absence of any writer] The
object texture `burnt` is drawn on any object whose burn timer (OS4 +0x33c) is ≥ 1 (renderer OS4
`FUN_010c101c`, 68k `FUN_000974a0`). The timer is set to 100 when a vehicle is wrecked (OS4
`FUN_01044b70` = 68k `FUN_000cda5e`) or a person is ignited (`FUN_01056c58` / `FUN_000de0b6`), and
parked at 15 when it runs out (OS4 `FUN_010542b8` ~44100), so wrecks and corpses stay burnt. [C]

## 4. Pickups

Code: collect 68k `FUN_000d0988` / OS4 `FUN_01048ee4`; respawn 68k `FUN_000d079e` / OS4
`FUN_01048c40`; crate break 68k `FUN_000d0670` / OS4 `FUN_01040ce4`. All [C].

**Where.** Pickups are scenery objects on cube records (scenery byte = cube +1, special = +4).

**Collection.**
- Only by the player-controlled object (state 2), **by entering the square**: there is no
  distance test. A vehicle checks every square under its 3×3 footprint loop and also breaks
  crates there (any vehicle breaks crates; only the player's collects; after a step without a
  wall hit, before the scenery circles: vehicles.md §2.7). Port: `driving.rs` per box point;
  `combat.rs` `pickups` covers people only.
- Only model types 1..99 qualify. Radar (100), briefcase (23, no type) and phones are not pickups
  in this code. Phones are script triggers 0x01/0x07; nothing in either exe tests radar
  (`+0x4908 == 100`). [C; briefcase not traced further]
- Effects of a pickup: 2 type-3 Dust puffs at (square·2048 + 0x270 + r(800), same for y,
  z + r(300) − 150), v = r(500) − 250 per axis, size 1500 + r(1000), from the **gameplay**
  `random` (7 draws each in the order size, vz, vy, vx, z, y, x: 14 per pickup) [C: Mac
  `CheckForWeapon` @ 100597e0, 68k `FUN_000d0988`]; then for types < 0x50, PlaySound(20
  PickupWeapon, 0, 1e6, 14000).
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
| 0x50 info | 24 | sound + 2105 "Information" | scripts react through trigger 0x25 (cell scenery becomes 0) and show their own text [C] |
| 0x51 key | 34 | sound + 2108 "Key" | |

**Respawn.**
- Each collection queues (sx, sy, layer, clock + **1200 ticks** = 40 s) in a 1000-entry ring
  buffer.
- When due, the square's scenery becomes **6 (crate)** with its special byte unchanged, plus the
  same 2 Dust puffs (14 gameplay draws) at the fixed height z = 0x4d8a + r(300), layer
  min(layer+1, 3), and PlaySound(47 ObjectAppear, 0, 20e6, 8000) [C: Mac `RespawnWeapons`
  @ 100595c0, 68k `FUN_000d079e`].
- So every collected pickup returns as a crate. A weapon placed directly with special 0 returns as
  an empty crate [C logic].

**Crates** (scenery 6).
- Break triggers: a punch, bullets, pellets, missiles, petrol, or a vehicle entering the square.
- On break: `Bang(crate, layer+1, centre, z, angle, 5)` (Mac `Bang` @ 100a6dd0, 68k
  `FUN_000384e8`): `Split` (the crate.obj fragments), then max(2, min(6, 5>>1)) = **2** Dust puffs
  at centre ± 400 (z ± 150), v = r(500) − 250 per axis (÷3), size 1500 + r(1000). All draws come
  from the cosmetic **`random3`** stream (port: `VisualRng`), so a crate break does not move the
  game's sequence [C]. The port does not draw the fragments yet.
- Weapon selection by number key needs health > 0 **and** the stun timer at 0 [C: Mac
  `UpdateInputStatus` @ 100f8490; the Amiga agrees].
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
| 5 | 0 | removed when z ≤ 23000 | bubble: sinking vehicles (`do_Other` @ 10146280) and drowning people (`do_Person` @ 1013fbd0) |
| 6 | −13 | – | dark smoke (Dust2, subtractive): damaged cars, exhaust, chimneys |
| 7 | −66 | like type 4, smoke size 350+rand150 | explosion fireball (Flame.N) |
| 8 | −1 | – | ground decal: blood pool (Blood, 0x30000), scorch (Dust2, 0x10000), skid marks |
| 9 | −7 | – | spray: bullet sparks (Flare), blood spray, script Gas |
| 10 | **+300** | frame = size/2500 (0..3); removed at size > 9999 | **shockwave** ring |
| 11 | −5 | vz += 17; drawn as a lens flare (AddFlare at −size) | explosion flash |
| 12 / 13 / 14 | 0 | wrap around the view | snow / rain / rain splash |
| 15 | −2 | may spawn type 4 flames; loops Flames | fire source (script only) |

**Textures** (`effecttextures/`, 64×64): Dust, Dust2 (alpha variant), Flare, Flame.1..4, Blood
(the gore option swaps in Blood.dark), Gas, Bubble (types 5), Shad (shadows), DirtSkidmark, Dirt.

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
- `TankBits` has nothing to do with debris: it is the one object texture exempt from the night
  `.dark` texture swap/darkening (OS4 `FUN_010bf310`, 68k `FUN_00095efa`). [C]

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
   if class(o) != 2: o.v += clamp(q*d/k, ±1000)   // knock-back, per axis; none for props
   if o.filter in {-1,1,3}:
       o.health -= class(o) == 2 ? (q>>4)/k : q/k  // people AND vehicles take q/k
       single player, src owner >= 0, o owner == -1:
           player.score += mult * (k>>5 < 1 ? 6000 : min(6000, q/(k>>5)));  Crime(o, 1)
flash DAT_0114c748 += P/250000 (decays ×3/4 per tick; brightens the palette; no camera shake)
AddEffect(8,  Dust2, x,y,z, 0,0,0, P>=120e6 ? 6000 : max(P/20000,1000), layer, 0x10000)  // scorch
AddEffect(10, -,     x,y,z, 0,0,0, 300, layer, 0)                                      // shockwave
AddEffect(11, Blood, x±200, y±200, 21000, 0,0,0, 1000, layer, 0)                       // flash
```
- **Class split** [C: Mac `Shockwave` @ 10054ac0, 68k `FUN_000cd22e` ~110460, OS4 `FUN_0104405c`]:
  only class-2 props (barrier, cone, dummy, `model+0x8165 == 2`) take the extra >>4 and get no
  knock-back; people and vehicles take q/k. An earlier reading gave vehicles (q>>4)/k as well, so
  vehicles took 1/16 of the real damage and chain reactions were rare. Port: `sim/combat.rs`
  `explosions` (class 2 = `Class::Projectile` objects without a `Projectile`, i.e. roadblock props).
- **Score** [C, same functions]: in single player only. The original pays whenever the exploding
  object is on the player's side (owner side `+0x824 >= 0`) and the victim is an NPC (`== −1`).
  `Crime(o, 1)` (= `CheckHitObject`) is called for every such object, whatever the damage, and
  there is no radius, so every NPC in the object table counts as hit (once, `+0x964`). Port:
  `Obj::side` (below), `Explode::by_player` = the exploding object's side >= 0, and `Hurt` at
  zero damage (CB-4, CB-5).
- **Owner side `+0x824`** [C: Mac]: −1 from `NewObj` @ 100575b0 (traffic, pedestrians, AI
  cars); 0 for the player object (level start and respawn), the car he gets into
  (`GetInOutVehicles` @ 10066140, kept after he leaves), the person who gets out
  (`GetOutVehicle` @ 10065660) and script actors (`AddObject` @ 10105780, `AddObjectSpecial`
  @ 10105b00); back to −1 by the script goto / goto-15 / route-flee calls (`SetTrigger_Long`
  cases 10107bf0, 10107ce0, 10107ff8); 1 for person shots (`do_Person`): grenade, bomb,
  missile, petrol, minigun and pistol bullets whoever fires them (so AI grenades and missiles
  pay the player and count as his crime), shotgun pellets only from a state-2 shooter (else
  −1); 1 for the tank cannon (`DoVehicleFire` @ 10070c20). Bullet and pellet hits test it as
  well (`CheckForCollisions` l.27315-27750); the port's bullet hits still use the shooter being
  the player [I: not ported, open].
- There is no hard radius: damage falls off as ~1/d³.
- Distances at which a person (T 90) takes ≥ 100 damage [I, computed]: missile (2.5e7) ≈ 1 square;
  grenade (6e7) ≈ 1.4 squares; remote bomb (1e9) ≈ 3.5 squares; exploding car (1400·25000 =
  3.5e7) ≈ 1 square (151 at 1 square, 19 at 2).

**`Ignite(obj)`** (OS4 `FUN_01044b70`, 68k `FUN_000cda5e`). Used when a vehicle's health reaches
≤ 0, and on missile impact.
```
burn = 100; speed = 0; state = 5 (burning wreck; [C: Mac `Explode` @ 10055570])
v += (rand(100)-50, rand(100)-50, -(200+rand(300)))            // hop
n = (T>>10) + 2 ; c = clamp(n/2, 2, 6)
c × fireball type 7 (Flame, ±800, size 2500+rand1500, layer 3)
c × burning debris type 2 (size 800)
Explosion(owner, pos.z+500, P = T·25000)
```

**Player in a wrecked vehicle** [C: Mac `DoCollideStuff` @ 10067b50 l.25160-25450, OS4
`FUN_010542b8`]. The driver is the vehicle object itself (a boarded passenger is removed), so
there is no occupant to kill: the vehicle's death is the driver's.
- Health < −70: `Detonate(i, 0)` (state 6). If it is a player's object and his death timer is
  not running: death timer 80, score saved, "Obliterated!" (2025; 2020 for a person).
- Health < 1, not burning: `Explode(i)` = Ignite above, which sets **state 5**. The player loses
  the controls and cannot get out (`GetInOutVehicles` @ 10066140 acts only in state 2).
- Burning: burn −= ticks; below 2 the player's vehicle goes to state 7 with the death timer
  80, the score saved and "Nuked!" (2024); any other vehicle goes to state 7; burn = 15.
- God mode is a port cheat: the player's object takes no damage, so it never gets here.
- Port (`sim/contact.rs` `vehicle_damage`): as above. Before, the player's burning car stayed in
  state 2, so pressing in/out during the 100 burning ticks got him out alive. Other vehicles go
  to state 7 at once (the original's 5 then 7; nothing reads the difference).

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
- Every object starts with **100** health. There is **no max clamp** on the script's `AddHealth`
  (OS4 `FUN_01040f70`).
- Players regenerate **+1 health every 250 ticks** while 0 < hp < 100 (the on-foot value, also
  while driving) [C: Mac `MainLoop` @ 1014a920, 68k tail of `FUN_000e3a58` ~125945]. Port:
  `sim/people.rs` `regen_and_respawn`.
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
- **Water:** 43 Splash (volume T·8000/20000). Models with +0x4904 are destroyed (−71). A
  **person** in water sinks 24 z/tick; at z ≥ 27000 the person is removed (state 6, not via
  health) and, for the player, the respawn timer starts with 2018 "Drowned!"; in multiplayer it
  costs one kill. Entering water clamps the burn timer to 1 (puts a fire out). [C: 68k
  `FUN_000e3a58` ~122505–122585 and ~125836, OS4 0x1063c90/0x1063edc; people.md §4.4]

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
| Copkiller bonus: every 11th cop kill (counter `DAT_0114c6a4` > 10, then reset to 0), single player only | 100000 (× multiplier like the rest), message 2037 "Copkiller bonus!", no sound [C: Mac `CheckHitObject` @ 100542d0, OS4 `FUN_01043924`] |
| Carjack a civilian car (occupied) | 1000 |
| Carjack a police car | 1500 |
| Explosion damage to an object (single player) | min(6000, q / (k>>5)) with q, k from §5.2 (6000 when k>>5 < 1), per object [C: Mac `Shockwave` @ 10054ac0] |
| Punch an NPC (owner side −1), by anyone | 500 [C: Mac `do_Person` @ 1013fbd0] |
| Run someone over / ram (vehicle collision), a player-side object (owner side >= 0) on an NPC (−1) | min(6000, v²>>9), v = max(|px|, |py|)·min(m)/m_victim with p the contact push, per touching tick, whatever the damage; then `CheckHitObject(victim, 0)` [C: Mac `CheckForCollisions` @ 1006a150 l.27048-27071, l.27189-27213]. Port: `sim/contact.rs` `contacts` (CB-3) |
| Burning (flamethrower/fire contact): per tick damage = min(health+0x47, ticks·3000/mass), ≥1 | damage × 32 (OS4 `FUN_01056c58` ~l.46400) |
| Bullet/pellet hits by a player-owned shot on a non-player | min(8000, k/T), k = 400000 minigun / 200000 pistol / 800000 shotgun pellet (= 40 × the health-damage numerator), T = target toughness `model+0x48ec` (not a distance); × multiplier [C: 68k `FUN_000de0b6` ~119024–119125, ~119455–119500; OS4 `FUN_01056c58` ~46548] |
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
  (§1). Use BFS. (Needs the map editor reverse-engineered.)
- The route node `used` byte value 1 (§1): no reader, so "leftover editor flag" stays [I].

Resolved:

- ~~Ammo and weapons at level start and after respawn~~ The player object is freshly allocated and
  zero-filled: fists only, no ammo, wanted 0, both at level start and after every respawn; only
  the score is restored. Weapons come only from pickups; no script gives the player weapons.
  [C: 68k `FUN_000ec686` ~127150 / ~127750 via `FUN_000cf11e`, `FUN_0003feb2`]
- ~~Drowning and `burnt`~~ §6 Water and §3.4. [C]
- ~~Missile hitting scenery~~ Removed without exploding (§3.3). [C]
- ~~OS4 addresses~~ No separate functions: everything is inlined in `FUN_0105f594`; landmarks in
  the header of this note. [C]
- ~~Rampage end condition~~ None besides the obvious: in rampage (`DAT_0114c7c6`) the mission
  script is neither loaded nor ticked, so "level complete" never happens; the game ends at game
  over (lives 0), on quitting from the pause menu, or at the demo build's 9000-tick time-out. At
  game end `DAT_0114c7c4` = the rampage flag (key-file build only) enables the high-score check,
  whether the player lost or quit. [C: OS4 `FUN_010692bc` ~50022, `FUN_0102ca08` ~23421]
