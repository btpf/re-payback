# Draft: police route graph and scoring (working notes)

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
precomputed by the map editor. Verification (`tools/re/route_check.py`): it equals
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

Waypoint following (68k `FUN_000e3a58`, police AI states 8/10 and 0x14): the car stores
its current node at `obj+0x3be` (u16; 9999 = none → take nearest node) and its steering
target at `obj+0x2e6/0x2ea` = node xy << 3. The node counts as reached when
|pos − target| < 300 + |5·speed| on both axes (`obj+0x3ae` = speed), then it advances
with `FUN_000a43f2(current, target_node)`. Target node of the chased object is
refreshed (nearest node to the chased object, stored at `target+0x3be`) whenever the
u8 countdown `target+0x3c0` is 0, which then resets it to 0x14; the countdown is
decremented by 1 per object update (once per frame, not per tick) at the top of
`FUN_000e3a58`. So the chase target node is re-evaluated every 20 frames. CONFIRMED.


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
| Explosion damage to an object | min(6000, k / (d²>>5)) where k = power/16 / model mass(`+0x48ec`); exactly 6000 when very close |
| Run someone over / ram (vehicle collision) | min(6000, max(|vx|,|vy|)²>>9) (velocity scaled by impact) |
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
  (80 ticks; tick = 1/30 s) and the current score is saved to `DAT_012a65f0[p]`;
  in single player the crime summary is printed (`FUN_010bcf10`). The death message
  depends on cause: 2016 Wasted (killed on foot), 2017 Busted, 2018 Drowned,
  2019 Toasted (burnt), 2020 Splattered (explosion on foot), 2021 Flattened (run
  over), 2024 Nuked (killed in vehicle), 2025 Obliterated (vehicle blown up).
- When the timer runs out: lives −1, multiplier = 1. If lives > 0 a new player
  person is created (`FUN_01046ee4(model)`, placed by `FUN_0106ef6c`), state 2
  (player-controlled), `+0x344 = 0`, **score restored** from the saved value (no
  score penalty). If lives reach 0: "Game over!" (2026), game state
  `DAT_0114c730 = 2`, 250-tick timer. CONFIRMED.

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
