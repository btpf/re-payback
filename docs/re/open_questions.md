# Open questions: inventory and resolutions

Every open question, INFERRED statement, "unknown", "TBD" and "not traced" item
found in `docs/research.md`, `docs/formats.md` and `docs/re/*.md` (pass of
2026-10-08), with its status after a resolution pass over the 68k Update 7 and
OS4 decompiles and the data.

Status values:

- **CONFIRMED, applied**: resolved from code (address given) and written into the
  original doc.
- **CONFIRMED, to merge**: resolved, but the doc is being edited by another agent
  (`rendering.md`, `frontend_menus.md`, `vehicles.md`, `combat_police.md` §2.2/2.3/2.5,
  `people.md` §6.1/6.3). The answer is in §1 below, ready to merge.
- **OPEN**: still unknown; the "evidence needed" column says what would settle it.
- **INFERRED, kept**: a reasoned interpretation that the code cannot prove (design
  intent, visual meaning); left marked INFERRED.

68k addresses are Update 7 `Payback.exe` (flat image at 0x10000); "~n" is a line
in the corresponding `decompiled.c`.

## Summary

| Document | Items | Resolved | Open / INFERRED |
|---|---|---|---|
| `research.md` (old list) | 7 | 7 | 0 |
| `formats.md` | 16 | 14 | 2 |
| `rendering.md` | 6 | 5 | 1 |
| `vehicles.md` | 4 | 3 | 1 |
| `people.md` | 10 | 7 | 3 |
| `combat_police.md` | 17 | 15 | 2 |
| `frontend_menus.md` | 14 | 10 | 4 |
| `frontend_audio.md` | 17 | 12 | 5 |
| `script_api.md` | 25 | 23 | 2 |
| `missions_00.md` | 16 | 13 | 3 |
| `missions_01.md` | 17 | 14 | 3 |
| `missions_10.md` | 12 | 6 | 6 |
| `missions_11.md` | 13 | 6 | 7 |
| `missions_20.md` | 10 | 7 | 3 |
| `missions_21.md` | 17 | 14 | 3 |
| **Total** | **201** | **156** | **45** |

"Resolved" counts every CONFIRMED row: applied here, waiting to be merged (§1), or
already fixed by another agent in the same pass.

## 1. Findings waiting to be merged

### 1.1 `rendering.md`

- **Inner-side wall culling (§1 Side faces, "INFERRED")**: CONFIRMED as an explicit
  test. `FUN_0107b910` (Normal) and `FUN_0107bd3c` (Gouraud) return unless the sign of
  the edge offset from the camera matches the top/bottom depth order, and return on
  zero-height walls; `FUN_0107c260` / `FUN_0107d74c` (other face types) return when the
  projected top row is above the bottom row. (The absolute sign convention was not
  re-derived: Ghidra confuses `in_r6` and `param_4` there.)
- **Off-map colour ("INFERRED black", §6 Map edges)**: CONFIRMED with a caveat. The
  software framebuffers (`DAT_0114c7a4/7b0/7ac`) are zeroed only at level start
  (`FUN_01051800`, `FUN_010692bc`, zero-fill `FUN_01111180`); palette entry 0 is black.
  There is no per-frame clear, so (INFERRED) sprites crossing the off-map area leave
  trails, and the lightning palette flash brightens that area too.
- **Tick length "not traced here" (§7)**: resolved in `people.md` §1.1: 68k 25 Hz
  (2 DateStamp ticks per game tick), OS4 30 Hz.
- **Mode 3 texel 0 transparent (§10, if still marked INFERRED)**: CONFIRMED.
  `FUN_0111e410` jumps into an unrolled span ending at 0x01128ae8; every pixel does
  `lbz; cmpwi 0; beq skip; stb`.
- **Info-sign axis order (§10, "axis order INFERRED")**: CONFIRMED (`FUN_010c7218`, call
  site 0x0104cec4): rotate in the X–Z plane by `DAT_0114c76c`, then in the Y–Z plane by
  `DAT_0114c768`, then yaw by `rotation·22.5 + DAT_0114c770`. Per tick (from 0x0105fe08):
  c770 += 60, c76c += 30, c768 −= 45, each wrapping at 5760. Initial values (0x01069954):
  c76c = 20, c768 = 5, c770 = 0.
- **Mode 0x22 (34) faces** (new): 4 faces, one leg face in each of PedMale1–4 (slot 1
  active, texture "White sand", param 3). Bit 0x10 is clear and the low bits are not
  1/2/3, so the software path falls into the mode-4 branch: additive blend (Warp3D also
  blends). Bit 0x20 alone does nothing and does not set the shine flag `+0x4906`.
  CONFIRMED (`FUN_010c101c`).
- **Obj header flag** (new, for §10): model `+0x490c = (flag == 1)` turns on painter's
  sorting of the model's faces (details in `formats.md` Objects). CONFIRMED
  (`FUN_010bf8bc`, `FUN_010c101c`, `FUN_010c4fec`, `FUN_010c7218`).
- **`TankBits`** (new): the one object texture exempt from the night `.dark`
  swap/darkening (OS4 `FUN_010bf310`, 68k `FUN_00095efa`). Worth a line in §4 Night.
- Still OPEN: the unrolled Gouraud copies "whose offsets were not all decoded (INFERRED:
  the same diagonal)" (§4). Evidence needed: decode the remaining unrolled offsets.

### 1.2 `frontend_menus.md`

Replace the "Open items" list; all but two are resolved (CONFIRMED):

- **§1 CD music, pause menu `FUN_01008cec(0)`**: command 0x5000+0 = "keep music running".
  The receiver is the "Payback CDDA Thread" (entry 0x010087a0, from the start tags at
  0x0112ee18; no Ghidra function). It waits on signal 0x1000 and switches on
  `DAT_0114c2b4 & 0xF000`: 0x2000 quit, 0x3000 stop, 0x4000+n play track n now
  (`FUN_010084bc`), 0x5000+n the policy at 0x010085cc (low 12 bits 0x64 = stop). Bit
  0x10000 only toggles so a repeated command counts as new. Policy(n): a track is playing
  and its TOC length (start + frames/75) has not run out → nothing, whatever n is; it has
  run out → n > 0 replays n, n = 0 moves to the next track (last wraps to first); nothing
  playing → n = 0 starts a random track in first..last, n > 0 plays n. `FUN_01008cec`
  skips a repeat of the same n within one second (clock/50). The main loop polls this only
  in mode 2 (Continuous); the pause loop `FUN_0101a7dc` in any mode ≠ 0 (side effect,
  INFERRED: pausing in Radio mode on foot starts a random track).
- **§3 "View intro"**: only OS4 refuses (its video code `FUN_01000978`, player
  ~0x01000e10, has no callers). Update 7 68k plays it: main menu `FUN_00033192` item 7 →
  `FUN_00010364("intro")`, `FUN_000106c0` (play), `FUN_00011402` (cleanup), refused only
  if the files are in neither `PROGDIR:Videos` nor `PaybackCD:Payback/Videos`; 68k
  `FUN_000ec686` also plays it automatically at the end of the startup load (call at
  0xeda52, after CubeTextures, before CloseProgress). The CD release: startup call at
  file offset 0x65caa, menu item index 6 at 0x14b70, loader at 0x894c0 (offsets into
  `extracted/iso/Payback/Payback.exe`).
- **§6 config fields**: `DAT_0114c72c` = saved motion blur (live flag `DAT_0114c6c8`,
  toggled by `FUN_01042308` / pause item 3; level start ~49442 turns blur on if set, level
  end ~49887 stores c6c8 back and turns blur off). `DAT_0114c304` = software-render screen
  ModeID (`FUN_0104107c` → GetDisplayInfoData for W/H, sets `DAT_0114c2e0` = native
  non-RTG, 8 bitplanes + c2p `FUN_0112d1d0`; OpenScreen SA_DisplayID at 0x0104ff88 and
  0x010509c4). `DAT_0114c300` = Warp3D screen ModeID (OpenScreen 0x0105013c,
  W3D_CC_ModeID in `FUN_01002024`, `FUN_0104107c` when `DAT_0114c7d8 != 0`).
  `DAT_0114c310` = 3-D view size in 1/256, 0xB0..0x100 step 0x10, keys '=' (raw 0x0C) and
  '-' (0x0B) in `FUN_01051940`/`FUN_01051a10`, refused under Warp3D; `FUN_01051800`
  clears the buffers and sets `DAT_0114c6c4`; forced to 0x100 under Warp3D. (Full text
  now in `frontend_audio.md` §3.7.)
- **§7 "Play again"**: `FUN_01027330` returns void. Items 0 City, 1 Play again, 2 View
  replay, 3 Save replay, 4 End game. Play again sets `DAT_0114c7c0` = the chosen city,
  slides out and returns with `DAT_0114c7c8` still 1; the caller (`FUN_010692bc` ~50007)
  calls `FUN_0102fc44`, which with c7c8 set skips the title and returns `c7c0*2`
  (LAB_01030208), so LoadMap starts the next match at once: same players and kill target,
  chosen city, level 0. End game ("Are you sure") sets `DAT_0114c7c8 = 0` instead.
- **§8 version-3 replay records**: same bit layout. CD encoder at file 0x4773c..0x477de
  (bits 14..8 flags, weapon << 3 or 0x50, ticks separate), decoder 0x473cc
  (`(w & 0xF8) >> 3`, 10 = none, 0xFFFF = end, `w & 7` = ticks), reader 0x7a7a0 (0xFFFF
  at end of data), comment skip 0x7a7d2 (bit 15 as `FUN_010bbcf4`). Only the header
  differs.
- **§8 weapon-select values** (`FUN_01051a10`, 68k `FUN_000da2e0`): 0 fist (`+0x360` id 0),
  1 pistol (id 0xD, ammo `+0x378`), 2 shotgun (5, `+0x364`), 3 flamethrower (2,
  `+0x36c`), 4 grenade (3, `+0x370`), 5 minigun (4, `+0x374`), 6 bazooka (1, `+0x368`),
  7 remote bomb (0xE, `+0x37c > 0` or a placed bomb `+0x380`); 10 = no change; 8 and 9
  unused. A change happens only if the player has that weapon and is not already holding
  it. (Same order as the HUD weapon icons. Not the trigger 0x22 numbering.)
- **§11 `.mini` glyph**: pixels CONFIRMED (`FUN_010727f0`, stores at 0x010728c0): a red
  (0xB0) up-pointing arrowhead with a black (0x08) shadow row at x 29..34, rows 0..4 (ASCII
  picture in `frontend_audio.md` §6.2). Baked into the texture, so it turns with the map in
  rotating view: a north pointer (INFERRED). "Stored at `DAT_0130e254` as the HUD radar
  texture (I)": CONFIRMED by `frontend_audio.md` §2.4 (minimap `FUN_01018954` draws it).
- **Startup Loading screen (68k)**: InitProgress = `FUN_000cc0c2("startup.size")`, called
  from `FUN_000ec686` at 0xed4fe right after the fade (`FUN_0003eb0c`). Software path:
  clears the box with `FUN_000365ea` (cx−0x36..cx+0x35, cy−0x16..cy+0x17), draws
  `icons/Loading` with `FUN_00018a9e` at (cx−0x2e, cy−0x11), shows it with
  `FUN_000cbf20`. DrawProgress = `FUN_000cc456`; CloseProgress is inline at the end of
  `FUN_000ec686`. A duplicate sequence in `FUN_000f0a72` has no callers.
- **§2 model 16 `cross`**: "loaded, use not found" → the spinning object on the Game Over
  screen `FUN_01021434` (z 0x898).
- Check: §7 row 8 says cities 1 and 2 are offered "if the previous city's first level has a
  cup (I)", while §11 (newer text) says a built-in city c+1 is skipped while city c's
  **last** level (`DAT_0117aecc[c*2]`) has no cup (C). One of them should go.
- Still OPEN: §1 p3 of `FUN_0102ca08` "(I: replay state)" and §9 `FUN_0103da68` "(I: input
  reset)". Evidence needed: trace `DAT_0114c8a8` writers and the body of `FUN_0103da68`.

### 1.3 `vehicles.md`

- **§5 `Maps/<n>.vehicles`** is an empty placeholder; the layout is now in `formats.md`
  (Companion map files): loader OS4 `FUN_0106dbbc`, version, three city booleans, count
  ≤ 6, records {model, x, y, layer, heading}; parked vehicles spawn only in Rampage and
  multiplayer (`FUN_010692bc` ~49456). CONFIRMED.
- **§1.2 `+0x4928`** "tracked/no-wheel" (tank, helicopter, boat) is an **OS4** offset = 68k
  `+0x4926`. The 68k `+0x4928` used by `Heli_TakeOff`/`Heli_Land` is OS4 `+0x492a`
  ("flies", helicopter only). Worth a note, as the mission docs use 68k offsets.
- **§6.3 "Port notes (I)"** describes the port's deliberate deviations, not facts about the
  original; relabel it "Port deviations" instead of (I). The original-behaviour facts it
  repeats are already C in §6.1; the port (`sim/vehicle_ai.rs` ~457–475) matches the text.
- **§6.2** could cite the new `Actor_SetAggro` reader list (`script_api.md` §5).
- Still OPEN: §2.10 Slopes (I) (`FUN_010cbc7c`). Not investigated in this pass.

### 1.4 `combat_police.md` §2.2, §2.3, §2.5

- **§2.2 "Square test ... special&0x7f must not be 0 or 4"** (~line 303): the code tests the
  **top face type** (cube `+9`): not Off, not Reflect (68k `FUN_000cf5cc`). CONFIRMED.
- **§2.2 tank / helicopter "never random traffic; script-only [I]"**: CONFIRMED. Traffic
  keeps a random model only if `rand(256) < model+0x4934`; both have weight 0 (OS4
  `FUN_01047af4` ~37785; set ~41164/41247).
- **§2.2 port pursuit note (~351) [I]**: a port fact, CONFIRMED in `sim/pursuit.rs`
  ~216–231 (nearest node refreshed every 20 frames; empty route → straight to Pursuit).
  Tag it as a port fact.
- **§2.5 "ammo defaults ... were not traced" (~446–447)**: CONFIRMED. The new player object
  is freshly allocated and zero-filled, so it has fists only (`+0x35e = 0`), no ammo
  (`+0x362..+0x37e`) and wanted 0, at level start and after every respawn; only the score
  is restored (68k `FUN_000ec686` ~127150 / ~127750 via `FUN_000cf11e`; `FUN_000a2b80`,
  `FUN_0003feb2`). Weapons come only from pickups; no script gives the player weapons.
- §2.3 roadblock cube `+8`: already fixed in the doc.

### 1.5 `people.md` §6.3

- **Exit speed "(I)" (~801)**: CONFIRMED. The exit code writes no velocity (68k
  `FUN_000dab40`, OS4 `FUN_01052398`) and the new person is zero-filled.
- **Helicopter exit**: the answer is in `people.md` §9 (applied); §6.3's mention of
  +0x49c/+0x49e can point there.
- Still INFERRED: the link between the prev_pos behaviour and the Update 4 changelog fix
  "could get out on top of a building" (the behaviour itself is C).

## 2. Inventory per document

### `docs/research.md` (old "Open questions" list)

| Item | Status | Evidence / where |
|---|---|---|
| Sloped cubes: which edge `height1` belongs to | CONFIRMED, applied | `rendering.md` §1 |
| Texture orientation, side-face flip bit | CONFIRMED, applied | `rendering.md` §2 |
| Square flag bits; header `variant` | CONFIRMED, applied | `formats.md` maps (bit 0x80 still open, below) |
| `.route`, `.mini`, `.vehicles`, `.textures`, `Human.motion` | CONFIRMED, applied | `formats.md` companion files |
| Object face `mode`/`param` | CONFIRMED, applied | `formats.md` objects, `rendering.md` §10 |
| Sound rates, water reflection, night remap | CONFIRMED, applied | `frontend_audio.md` §4, `rendering.md` §3–4 |
| Mission script API and entry | CONFIRMED, applied | `script_api.md` |

### `docs/formats.md`

| Item | Status | Evidence / where |
|---|---|---|
| Header `variant` "meaning TBD" | CONFIRMED, applied | weather, `DAT_0114c854` / `FUN_0106e478` |
| Header `night` | CONFIRMED, applied | `== 1` → `FUN_0106ecf0` |
| Flags low bits "probably traffic flow" | CONFIRMED, applied | `FUN_01047af4`, `FUN_010cb7e4` |
| Flags `0x40` "TBD" | CONFIRMED, applied | roadblock allowed, `FUN_01047134` |
| Flags `0x80` "TBD" | OPEN | no reader in either build (OS4 `FUN_0105f594` does not decompile; its 68k twin was scanned); 18 squares in map 0; probably an editor leftover. Needs the map editor or a full OS4 disassembly scan. |
| Scenery rotation "(observed)" | CONFIRMED, applied | `SetScenery` writes cell `+0` |
| Obj header `flag` "TBD" | CONFIRMED, applied | face depth sorting, `FUN_010bf8bc` |
| Slot mode/param "observed" values | CONFIRMED, applied | mode table, `FUN_010c101c` |
| People-model /3 "scale TBD" | CONFIRMED, applied | `+0x48c4`, `+0x48d0`, `FUN_0104e13c` |
| `Human.motion` "format TBD" | CONFIRMED, applied | `people.md` §3.1 |
| `.mini`, `.thumb` "TBD" | CONFIRMED, applied | `frontend_audio.md` §6 |
| `.route` "TBD" | CONFIRMED, applied | `combat_police.md` §1 |
| `.vehicles` "TBD" | CONFIRMED, applied | `FUN_0106dbbc` |
| `.textures` "TBD" | CONFIRMED, applied | `FUN_0106e8ec` |
| `.textures` in-memory `+0x30` | OPEN | never read; needs the map editor to see what it writes |
| `.CubeTextureList` flags / RemapList "TBD" | CONFIRMED, applied | `rendering.md` §8, §4 |

(Sound sample rates "TBD" and the mission-script status are covered by the rows of
`research.md` and `script_api.md`.)

### `docs/re/rendering.md` (not edited here; §1.1)

| Item | Status | Evidence / where |
|---|---|---|
| Inner-side wall culling (§1) | CONFIRMED, to merge | `FUN_0107b910`, `FUN_0107bd3c`, `FUN_0107c260`, `FUN_0107d74c` |
| Unrolled Gouraud copies "same diagonal" (§4) | OPEN | decode the remaining unrolled offsets |
| Off-map colour black (§6) | CONFIRMED, to merge | buffers zeroed at level start only |
| Tick length "not traced here" (§7) | CONFIRMED, to merge | `people.md` §1.1 |
| Mode 3 texel 0 transparent (§10) | CONFIRMED, to merge | `FUN_0111e410` |
| Info-sign axis order (§10) | CONFIRMED, to merge | `FUN_010c7218` |

### `docs/re/vehicles.md` (not edited here; §1.3)

| Item | Status | Evidence / where |
|---|---|---|
| §5 `.vehicles` placeholder | CONFIRMED, to merge | `FUN_0106dbbc` |
| `+0x4928` offset (OS4 vs 68k) | CONFIRMED, to merge | 68k `+0x4926` / `+0x4928` |
| §6.3 Port notes (I) | CONFIRMED, to merge | relabel as port deviations |
| §2.10 Slopes (I) | OPEN | trace `FUN_010cbc7c` against the 68k twin |

### `docs/re/people.md`

| Item | Status | Evidence / where |
|---|---|---|
| Helicopter exit sequence, refused above a speed? | CONFIRMED, applied (§9) | 68k `FUN_000db2e4` ~116985, OS4 `FUN_01052a4c` ~43335; no speed/height check |
| Feet-below-floor offset (§2.3) | OPEN | compare a `--screenshot` of a standing person with the original |
| Atan helper convention | CONFIRMED, applied | `FUN_00094b44`, OS4 `FUN_010bcc18` |
| Zero-initialised instances | CONFIRMED, applied | `FUN_0003feb2`, `FUN_0103a73c` (calloc) |
| No fall damage (§4.3) | CONFIRMED, applied | no health writer depends on vz |
| Cannot climb out of rivers (§4.4) | OPEN | step limit is C; scan the maps for banks within 64 h-units of the water |
| Respawn weapons/wanted "fresh" (§4.5) | CONFIRMED, applied | zero-filled object |
| Update 4 "on top of a building" fix (§6.3) | INFERRED, kept | changelog link only |
| Exit gives no speed (§6.3) | CONFIRMED, to merge | `FUN_000dab40` / `FUN_01052398` |
| `+0x2ee` wander heading (§8) | CONFIRMED, applied | 68k `FUN_000e3a58` ~122305 |

### `docs/re/combat_police.md`

| Item | Status | Evidence / where |
|---|---|---|
| Footstep sound ids [I] (§0) | CONFIRMED, applied | OS4 `FUN_01005864` load order |
| Route `used` byte value 1 (§1) | INFERRED, kept | getter `FUN_000a43d0` has no callers |
| Editor's next-hop algorithm (§1) | OPEN | reverse-engineer the map editor; BFS matches 99.98 % |
| Square test "special&0x7f" (§2.2) | CONFIRMED, to merge | top face type, `FUN_000cf5cc` |
| Tank/heli never random traffic [I] (§2.2) | CONFIRMED, to merge | weight `+0x4934` = 0 |
| Port pursuit note [I] (§2.2) | CONFIRMED, to merge | `sim/pursuit.rs` |
| Roadblock cube `+8` [I] (§2.3) | CONFIRMED (already fixed) | slope byte |
| Ammo defaults not traced (§2.5) | CONFIRMED, to merge | zero-filled player object |
| Missile hitting scenery [I] (§3.3) | CONFIRMED, applied | removed without exploding, `FUN_000cdfdc` |
| `burnt` trigger not traced (§3.4) | CONFIRMED, applied | burn timer ≥ 1, `FUN_010c101c` |
| Radar/briefcase/phones effect [I] (§4) | CONFIRMED, applied | no exe code tests radar; briefcase not traced further |
| Info pickup text [I] (§4) | CONFIRMED, applied | scripts via trigger 0x25 |
| `TankBits` = tank debris [I] (§5.1) | CONFIRMED, applied (corrected) | night-swap exemption, `FUN_010bf310` |
| No drowning damage [I] (§6) | CONFIRMED, applied (corrected) | drowning exists, 68k ~122505 |
| `k/dist` impact score [I] (§7) | CONFIRMED, applied (corrected) | `k/T`, `FUN_000de0b6` |
| OS4 AI/firing addresses (header, §8) | CONFIRMED, applied | landmarks inside `FUN_0105f594` |
| Rampage end condition (§8) | CONFIRMED, applied | `FUN_010692bc` ~50022 |

### `docs/re/frontend_menus.md` (not edited here; §1.2)

| Item | Status | Evidence / where |
|---|---|---|
| Pause menu `FUN_01008cec(0)` | CONFIRMED, to merge | CDDA thread 0x010087a0, policy 0x010085cc |
| "View intro" | CONFIRMED, to merge | 68k `FUN_00033192`, OS4 has no caller |
| Config `DAT_0114c72c`, `c304`, `c300`, `c310` | CONFIRMED, to merge | `FUN_0104107c`, `FUN_010692bc` |
| "Play again" after multiplayer | CONFIRMED, to merge | `FUN_01027330` → `FUN_0102fc44` |
| Version-3 replay records | CONFIRMED, to merge | CD exe 0x473cc / 0x4773c |
| Weapon-select values 0..7 | CONFIRMED, to merge | `FUN_01051a10`, 68k `FUN_000da2e0` |
| `.mini` overlay glyph | CONFIRMED, to merge (meaning INFERRED) | `FUN_010727f0` |
| `.mini` stored as HUD radar texture (I) | CONFIRMED, to merge | minimap `FUN_01018954` |
| 68k startup Loading screen | CONFIRMED, to merge | `FUN_000cc0c2` at 0xed4fe |
| Model 16 `cross` use | CONFIRMED, to merge | Game Over `FUN_01021434` |
| Screen modes chosen in PaybackSetup (I) | INFERRED, kept | the game only reads the ModeIDs; the setup tool is not decompiled |
| City row unlock rule (I) vs §11 (C) | OPEN (doc conflict) | §11's `DAT_0117aecc[c*2]` rule is the traced one |
| Game Menu p3 "(I: replay state)" | OPEN | trace `DAT_0114c8a8` writers |
| Language select `FUN_0103da68` "(I: input reset)" | OPEN | read `FUN_0103da68` |

### `docs/re/frontend_audio.md`

| Item | Status | Evidence / where |
|---|---|---|
| `You.c8..c11` button meaning (§1.3) | CONFIRMED, applied | `FUN_0103c690` / `FUN_0103d7b0` |
| Lives start at 3 (I) (§2.2) | CONFIRMED, applied | `FUN_010692bc` ~49232 |
| Font 1 red = player speaking (§2.3) | CONFIRMED, applied | `Dialog_Say1` / `Say2` |
| View intro no-op (§3.3) | CONFIRMED, applied (corrected) | 68k plays it |
| Config fields (§3.7) | CONFIRMED, applied | see §1.2 |
| Scripts play Heartbeat etc.? (§4.4) | CONFIRMED, applied | none do; CD command 0x20 |
| §4.4 "square flags & 0x7F == 4 → water" | OPEN | probably the top face type (cube `+9`); check the reader |
| §4.4 truck Bus/Bleep/12 (I), deleted-on-release (I) | INFERRED, kept | not investigated in this pass |
| CD_Audio_Handler format, track list (§4.6) | OPEN | needs the original CD's audio tracks / CD-release handler |
| Pause `FUN_01008cec(0)` (§7) | CONFIRMED, applied | CDDA thread |
| "MERP" = Adobe Premiere (§5.1) | INFERRED, kept | creator tag only |
| FLC frame rate "× 0.5" (§5.1) | CONFIRMED, applied (corrected) | `FUN_000106c0`: +0.5 rounding, 66 ms |
| Audio nominal 22,050 Hz (§5.2) | INFERRED, kept | Paula period 163 is C; the nominal rate is a reading |
| `.mini` marker (§6.2) | CONFIRMED, applied (meaning INFERRED) | `FUN_010727f0` |
| Version-3 replay layout (§7) | CONFIRMED, applied | CD exe decoder |
| `Icons/Siren` unused (§1.2, §7) | CONFIRMED, applied | CD release's wanted star |
| `Cross.Obj` unused (§3.2, §7) | CONFIRMED, applied | Game Over screen |

### `docs/re/script_api.md`

| Item | Status | Evidence / where |
|---|---|---|
| Type 0x09 "destroyed/removed" | CONFIRMED, applied | `0xa653a` only on death/destruction |
| Type 0x0c boarding (INFERRED) | CONFIRMED, applied | `0xa6a7a`, state 6, no type 9 |
| Type 0x12 4th argument | CONFIRMED, applied | ignored, `0xb2b12` |
| Type 0x22 firing moment, weapon 8 | CONFIRMED, applied | `0xa6c62` in `0xe3a58` |
| Type 0x25 smash or pickup | CONFIRMED, applied | pickup, `0xd0988` |
| Type 0x27 wording (INFERRED) | CONFIRMED, applied | J-turn, `0xa6c12` |
| Type 0x2a (INFERRED) | CONFIRMED, applied | glass broken by a vehicle, `0x446b8` |
| Type 0x2b (INFERRED) | CONFIRMED, applied | lands in water, `0xa6a12` |
| Type 0x42 condition | CONFIRMED, applied | RC car only; single call site ~124072 |
| `Actor_Release` meaning (INFERRED) | CONFIRMED, applied | state 5 = no AI input |
| `Actor_SetAggro` = "fire rate" | CONFIRMED, applied (extended) | skill, all readers |
| `Actor_SetMaxSpeed` `+0x386` | CONFIRMED, applied | clamp also on the player, `0xea8c6` |
| `Actor_SetControls` `+0x49c` | CONFIRMED, applied | player-driven branch only |
| `Heli_TakeOff`/`Land` flag offset | CONFIRMED, applied | 68k `+0x4928` = OS4 `+0x492a` |
| `SetProjectileId` (INFERRED) | CONFIRMED, applied | remote bomb only, `0xefece` |
| `Explosion` args (INFERRED) | CONFIRMED, applied | 4th = particle layer, `0xcd22e` |
| `AddGas` args (INFERRED) | CONFIRMED, applied | `0x3d97a` |
| `Ped_ExitPlayerVehicle` / `Ped_ExitVehicle` (INFERRED) | CONFIRMED, applied | `0xdb03e` |
| `Ped_ExitVehicleEx` side | CONFIRMED, applied | `0xdb196`: 0 right |
| `Random` (INFERRED) | CONFIRMED, applied | table max 511 |
| `SetScenery` b0/b4 | CONFIRMED, applied | rotation, special |
| `SetCube` slope 0 | CONFIRMED, applied | removes the cube |
| `Dialog_Begin` speaker ids | CONFIRMED, applied | icon only |
| §7 "2159 View outro is unreferenced (INFERRED: outro unused)" | INFERRED, kept | no reference found; design intent |
| CD script command codes "not mapped" (§6) | OPEN | map the remaining stubs against `analysis/ghidra_script00/` (legacy only; low value) |

### `docs/re/missions_00.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 `SetFlag34c28c` | CONFIRMED, applied | `ShowCheckpointMarker`, `0xefef0`, `0xeff2a` |
| Q2 0x22 firing moment | CONFIRMED, applied | at the throw |
| Q3 0x25 on the crate | CONFIRMED, applied | pickup |
| Q4 `PedInVeh_*` | CONFIRMED (other agent) | `0xdb03e` |
| Q5 0x0c on Ron/Trey | CONFIRMED, applied | removed on boarding, no type 9 |
| Q6 `SetCube` block (26..30,61..70) | CONFIRMED, applied | removes the house's pitched roof |
| Q7 `AddSound` args | CONFIRMED (other agent) | vol / Hz / z |
| Q8 paint colour indices | CONFIRMED, applied | 0 black … 7 white |
| Q9 / init: the four cones | INFERRED, kept | no reference; at the ends of the broken bridge; needs an in-game look |
| `Set11a734` spawner (INFERRED) | CONFIRMED, applied | `0xcf5cc` |
| `Explosion` arg names (INFERRED) | CONFIRMED, applied | layer, size |
| Ron/Trey board (INFERRED) | CONFIRMED, applied | type 0x0c |
| Ron/Trey get out (INFERRED) | CONFIRMED, applied | `0xdb03e` |
| Boss exits the limo (INFERRED) | CONFIRMED (other agent) | M10 section |
| Rival's house = M5 target (INFERRED) | INFERRED, kept | coordinates only |
| Legacy KKK payout same result (INFERRED) | INFERRED, kept | compare `FUN_0001d62c` output |

### `docs/re/missions_01.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 `SetFlag34c28c` in M4 | CONFIRMED, applied | marker over the (83,20) drop-off |
| Q2 `SetScenery` order, cell `+0` | CONFIRMED, applied | rotation |
| Q3 `SetCube` blocks | CONFIRMED, applied | roof removed; M8 wall gap |
| Q4 `AddSound` 100000000 | CONFIRMED, applied | loudness numerator |
| Q5 0x2b and type 9 together | CONFIRMED, applied | only for RC cars |
| Q6 phone 6 loop order | CONFIRMED, applied | x, y, cube; last wins |
| Map = city 0 (INFERRED) | CONFIRMED, applied | level index → city |
| Init `SetCube` "opens a passage" (INFERRED) | CONFIRMED, applied | removes the roof |
| Phone 6 "(INFERRED from load order)" | CONFIRMED, applied | as Q6 |
| Window events 0x2a (INFERRED) | CONFIRMED, applied | `0x446b8` |
| §7 type 0x2b (INFERRED) | CONFIRMED, applied | `0xa6a12` |
| §7 type 0x0c (INFERRED) | CONFIRMED, applied | `0xa6a7a` |
| §7 type 0x2a (INFERRED) | CONFIRMED, applied | `0x446b8` |
| §7 `SetScenery` args | CONFIRMED, applied | rotation, special |
| Multiplier needed to finish (INFERRED) | INFERRED, kept | arithmetic over the rewards |
| 0x835 countdown cancelled → car never explodes (INFERRED) | INFERRED, kept | in-game test |
| Scorched ground textures (INFERRED) | INFERRED, kept | look at texture 0xda |

### `docs/re/missions_10.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 type 0x12 4th argument | CONFIRMED, applied | ignored |
| Q2 type 9 on removal | CONFIRMED, applied | no |
| Q3 `Actor_SetFlag49c` | CONFIRMED, applied | holds the player's car |
| Q4 `Actor_SetAggro` values | CONFIRMED, applied | driver skill |
| Q5 `AddSound` M200 | CONFIRMED, applied | Hiss, 8300 Hz, z |
| Q6 CD race, M500, M600 helpers not compared | OPEN | compare the CD `Maps/10.script` handlers with Update 7 |
| Free helicopter, id 1 (INFERRED purpose) | INFERRED, kept | no reference |
| Early kidnapper kill → mission impossible (INFERRED) | INFERRED, kept | in-game test |
| Pay factor notes "consequences INFERRED" | INFERRED, kept | follows from C code |
| CD `P` uninitialised → Update 7 fix (INFERRED) | INFERRED, kept | design intent |
| CD 0xa3/0xad decompiler artefact (INFERRED) | INFERRED, kept | disassemble the CD script words |
| Race `SetTimerMode` lights | CONFIRMED (already in doc) | `0xb47b8` |

### `docs/re/missions_11.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 weapon 8 | CONFIRMED, applied | remote bomb placed |
| Q2 M5 boarding over-count | CONFIRMED, applied | no type 9 |
| Q3 M3 `SetCube` at (56..57,114..115) | INFERRED, kept (removal CONFIRMED, applied) | fence gap: in-game look |
| Q4 M1 texture 0x1dd ring | OPEN | view texture 0x1dd in `--screenshot` |
| Q5 speaker ids | CONFIRMED, applied | icon only |
| Q6 Kill-frenzy clock vs mission timers | OPEN | run RC bonus during M2/M6 |
| Q7 helper `default:` branches | INFERRED, kept | design note for the port |
| Q8 ped skins | CONFIRMED, applied (body parts not checked) | load order `0xd7874` |
| Zone A = hotel (INFERRED) | INFERRED, kept | coordinates |
| (63,116) door (INFERRED) | INFERRED, kept | map texture match |
| CD unlock logic not traced | OPEN | CD stub `0x11384` |
| `0xb448e` p4 side (INFERRED) | CONFIRMED, applied | `0xdb196` |
| Speaker meaning (INFERRED) | CONFIRMED, applied | icon only |

### `docs/re/missions_20.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 / order caveat: first spawn | CONFIRMED, applied | `0x43f0e` before `Mission_Init`; special 2 = cell |
| Q2 `AddSound` 1000000000 | CONFIRMED, applied | loudness, z 19000 |
| Q3 `Actor_Siren` on the heli | CONFIRMED, applied | `Heli_TakeOff`/`Land` |
| Q4 `SetAmbience` on respawn | CONFIRMED, applied | not reset; consistent |
| Q5 boat role, garage 0x40 from a boat | OPEN | in-game test (or trace 0x40's mode check) |
| Boat = escape option (INFERRED) | INFERRED, kept | no trigger |
| Ambience default 3 (INFERRED) | CONFIRMED, applied | `0xec686` |
| Speaker 2 = gang radio (INFERRED) | CONFIRMED, applied | icon Mobile |
| Floor-1 doors edit cube 2 (INFERRED) | INFERRED, kept | map data |
| 0x17 on a helicopter (INFERRED) | CONFIRMED, applied | needs TakeOff |

### `docs/re/missions_21.md`

| Item | Status | Evidence / where |
|---|---|---|
| Q1 `+0x386` / `+0x49c` | CONFIRMED, applied | clamp; player controls |
| Q2 `SetFlag34c28c` in M8 | CONFIRMED, applied | marker cycles over the crates |
| Q3 trigger 0x42 | CONFIRMED, applied | RC car only: ev 205 never fires |
| Q4 cell bytes `+0`/`+4` | CONFIRMED, applied | rotation, special |
| Q5 model names | CONFIRMED, applied | `0x348120`, `0x3480e0` |
| Q6 `Random` table ≥ 512 | CONFIRMED, applied | max 511 |
| Q7 phone 4 axis | CONFIRMED, applied | phrase error |
| Speakers 0/1 (INFERRED) | CONFIRMED, applied | icon only |
| Race car held (INFERRED) | CONFIRMED, applied | `Actor_SetControls` |
| Start lights / lap display (INFERRED) | CONFIRMED, applied | `0xb47b8`, `0xb47ca` |
| Grid speed cap (INFERRED) | CONFIRMED, applied | `+0x386` |
| Van `SetFlag49c` "parked" (INFERRED) | CONFIRMED, applied | player only |
| Nick boards (type 0x0c) (INFERRED) | CONFIRMED, applied | `0xa6a7a` |
| `Random(1)` always 0 (INFERRED) | CONFIRMED, applied | table |
| Static vehicle 1000000 (INFERRED) | INFERRED, kept | no reference |
| Player killed by the truck explosion (INFERRED) | INFERRED, kept | in-game test |
| OS4 call names (INFERRED) | INFERRED, kept (3 now CONFIRMED) | match the remaining thunks |

## 3. Notes for the port

- Trigger 0x42 is RC-car-only in the original, so level 21's "Holy Shhhh" line never
  plays; the port never reports `RC_THROTTLE` either, which matches.
- `AddSound` has no range argument: the 8th script argument is z (already in
  `script_api.md`), so the progress item "Script AddSound audible range" is not a gap.
- The CD release and Update 7 68k both play the intro from the main menu and at startup;
  only the OS4 port refuses.
- Pause in Radio CD mode can start a random track (side effect of command 0x5000+0).
