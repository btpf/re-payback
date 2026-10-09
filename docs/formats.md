# Payback data formats

All multi-byte values are big-endian (68k). Paths are relative to the install
directory (CD `Payback/` overlaid with Update 7). AmigaOS paths are
case-insensitive; the game refers to files in lower case.

Status: **confirmed** = verified against code (address given) and every
shipped file; **observed** = consistent with the data but not traced in code.
Details of how the renderer and the simulation use these fields are in
`docs/re/*.md` (index in `docs/README.md`).

## Maps — `Maps/<n>.map`, `UserMaps/*.map` (confirmed)

Loader: OS4 `FUN_01079300` ("LoadMap"), 68k `0x4ad62`. Parser:
`payback-formats/src/map.rs`.

| Field | Type | Notes |
|---|---|---|
| magic | u8 | `0xFF` (read, not checked) |
| version | u32 | 1 (read, not checked) |
| night | u32 | `== 1` switches on the night look via `FUN_0106ecf0` (texture remap, `Stars.chunky`, `Blood.dark`, headlights); 1 for map 2 |
| variant | u32 | **weather** (`DAT_0114c854`, read via `FUN_0106e478`): 0 none, 1 snow, 2 rain + lightning; maps 0, 1, 2 use 0, 1, 2 (`rendering.md` §4) |
| squares | 128 × 128 | x-major: outer loop x, inner loop y |

Each square is a `u8` with 2 bits per cube (cube 0 = bits 0–1), followed by
three cube records, bottom to top:

| Kind | Size | Contents |
|---|---|---|
| 0 | 2 | `i16` height, stored raw (not ×16); the cube is off but still stacks |
| 1 | 8 | header + top face |
| 2, 3 | 16 | header + top, north, east, south, west faces |

Header (6 bytes): `slope<<5 | special`, `rotation`, `scenery`, `flags`,
`i8 height1`, `i8 height2`. Heights ×16 give world units. A face is a `u16`:
`flip<<15 | type<<12 | texture` (type = bits 12–14, texture = `& 0xfff`).
The in-memory cube layout (`+0` rotation, `+1` scenery, `+2` h1, `+4` special,
`+5` flags, `+6` h2, `+8` slope, `+9..+0xd` face types) is in `rendering.md` §1.

- **slope**: 0 Off, 1 Flat, 2 Vert, 3 Horiz, 4 RVert, 5 RHoriz (map editor
  names). Slope 0 is neither drawn nor solid. Which edge gets `height1` and
  `height2` per slope: `rendering.md` §1 (confirmed). Flat cubes use `height1`;
  `height2` is a leftover default (usually 256).
- **face type**: 0 Off, 1 Normal, 2 Genlocked (colour 0 transparent),
  3 Alpha (50%, glass: vehicles break it), 4 Reflect (water; texture is a
  diffusion map). Texture orientation per slope and side, and the flip bit
  (mirrors u only): `rendering.md` §2 (confirmed).
- **texture**: index into `.CubeTextureList`; ≥ 2000 = custom map texture
  (`.textures` slot − 2000).
- **scenery**: 1-based index into the scenery table (below); 0 = none.
- **rotation**: scenery heading, 256 = full turn (confirmed: `SetScenery`
  writes it, `script_api.md` §5).
- **special** (5 bits): crate contents (pickup numbering) / traffic-light
  sequence / start point / phone number (map editor F7). Scenery 14 (phone)
  squares with special *n* register phone *n*; the loader loops x, then y, then
  cube, so a later duplicate wins. Squares with scenery 0, flags exactly `0x20`
  and special 1 join the random start-point pool (up to 100); special =
  level + 2 marks that level's single-player start square (every city has one
  special-2 and one special-3 square; read by `FUN_0106f1ac`).
- **flags** (in-memory cube `+5`):

| Bit | Meaning | Readers |
|---|---|---|
| `0x01`/`0x02`/`0x04`/`0x08` | traffic may leave the square N/E/S/W (the neighbour must have the same bit) | spawn OS4 `FUN_01047af4` / 68k `FUN_000cf5cc`; lane choice `FUN_010cb7e4` / `FUN_000a143e` (`flags & 0x1f`, U-turn masked unless it is the only option) |
| `0x10` | "straight on" is also an option (editor F6); shipped only with one direction bit | `FUN_010cb7e4` |
| `0x20` | pedestrians walk here (editor F1) | ped spawn `FUN_01047af4`; wander turn 68k `FUN_000e3a58` ~122313; start points |
| `0x40` | roadblock allowed (editor B) | OS4 `FUN_01047134` / 68k `FUN_000cfe0c` |
| `0x80` | **unused**: no reader in either build, and the Mac map editor neither sets nor reads it (its keys toggle only 0x01–0x40; load, save, copy and paste keep the byte whole). Only 18 cube-1 Horiz ramps in map 0 (x ≈ 37, y 10–15) carry it, from an older editor (CONFIRMED, `MapEditor (OS X)`, `docs/re/open_questions.md`) | — |

World geometry: a square is 256 units wide (observed; 0x800 in object units);
ground cubes are 256 high; buildings stack to ~1024. North is −y.

## Companion map files

| File | Size | Format | Reference |
|---|---|---|---|
| `<n>.mini` | 4225 = 65×65 | 8bpp minimap, cropped to 64×64 with a red north arrow stamped at the top (confirmed) | `frontend_audio.md` §6.2 |
| `<n>.thumb` | 65536 = 256×256 | 8bpp top-down picture for city selection (confirmed) | `frontend_audio.md` §6.1 |
| `<n>.route` | 276480 | police routing graph, 1024 nodes × 270 bytes (confirmed, 68k `FUN_000a4200`); optional: when missing, Mac `LoadRouting` @ 100f1560 calls `InitRouting` (all nodes unused) | `combat_police.md` §1 |
| `<n>.vehicles` | ~100 | parked vehicles and user-map traffic (confirmed, below) | — |
| `<n>.textures` | — | custom textures for user maps (confirmed, below) | — |
| `<n>.scores` | 1024 | rampage high scores | `frontend_menus.md` §7 |

**`.vehicles`** (Update 6+; loader OS4 `FUN_0106dbbc`, 68k `FUN_00043148`),
big-endian i32:

| Field | Notes |
|---|---|
| version | 1 |
| cityA, cityB, cityC | booleans: allow the home-city 0/1/2 traffic models on a user map (`FUN_01047af4` via getters `FUN_0106d958/64/70`) |
| count | ≤ 6 |
| count × {model, x, y, layer, heading} | model id 0–27 (`DAT_012a6364`); x, y in object units (0x800 per square); layer 0–2; heading 5760 = full turn |

Optional: with no file, or a version other than 1, Mac `Obj_Load` @ 100ad180
falls back to `Obj_Init` @ 100ad130 (all three flags 0, no vehicles).
Parked vehicles spawn only in Rampage and multiplayer (`DAT_0114c7c6 ||
DAT_0114c7c8`; `FUN_010692bc` ~49456), with z on top of the cubes up to
`layer`. Shipped: map 0 (1,0,0) 4 records (one all zero), map 1 (0,1,0) 4,
map 2 (0,0,1) 5.

**`.textures`** (loader OS4 `FUN_0106e8ec`, reload `FUN_0106eb88`): `u32
count`, then per slot `u32 present`; if 1: `char name[40]`, `u32 type`
(surface/footstep flag, as in `.CubeTextureList`), `u32 gouraud_tex` (texture
used when Gouraud is on, read in `FUN_010766c8`; the editor's "Select image
(gouraud)"), then 4096 bytes (64×64). Slot i = texture index 2000 + i. At night
only names starting with `$` are exempt from the remap. The in-memory field
`+0x30` is never read: in the map editor it is a "used by the loaded map" stamp
(`GetCubeTexture` @ 1007dc40 writes a generation flag `LoadMap` flips), not stored in
the file (CONFIRMED, `MapEditor (OS X)` `SaveUserTextures` @ 1007d950). The editor guide also mentions `.objects`; the
game never loads it. Parser: `payback_formats::texture::CustomTextures`; 12 of the Mac
release's user maps use it (a 4-byte file is an empty set; the largest,
`Amirace`, has the full 112 slots).

## Scenery table (confirmed)

68k `FUN_000d7874`, table at `0x348000`. Ids are 1-based:

1 trafficlight1, 2 lamppost, 3 rock, 4 apexlogo, 5 tree, 6 crate, 7 newtree,
8 newtree2, 9 bazooka, 10 flamethrower, 11 largegrenade, 12 minigun,
13 shotgun, 14 phone, 15 PedestrianCrossing, 16 PedestrianCrossing_flash,
17 Chimney, 18 Bench, 19 phone_arrow, 20 radar, 21 AlarmBox,
22 AlarmBox_flash, 23 Briefcase, 24 Info, 25 newtree3, 26 snowman, 27 pistol,
28 bigbomb, 29 pole, 30 bed, 31 bedsidetable, 32 potplant, 33 lamppost_dark,
34 key, 35 lamp, 36 lamppost_dark2.

Object field `+0x4908` is the pickup type: bazooka 1, flamethrower 2,
grenade 3, minigun 4, shotgun 5, pistol 6, bigbomb 7, info 0x50, key 0x51,
radar 100. A smashed crate with special 1..7 becomes scenery
9/10/11/12/13/27/28.

## Objects — `Objects/**/*.Obj` (confirmed)

Text, one token per CRLF line. Loader OS4 `FUN_010bf8bc`; parser
`payback-formats/src/obj.rs`.

```
1001                   magic
flag                   1 = depth-sort the faces, 0 = file order
nverts
x y z w  (×nverts)     z negative = up; w = limb index for people, else 0
nfaces
a b c                  1-based vertex indices
2 × slot               name, u0 v0 u1 v1 u2 v2, mode, param, active
```

- **magic**: < 1000 = legacy file without header (first number is nverts,
  flag = 1); 1000 skips two extra lines; 1001 is current. All 130 shipped files
  are 1001.
- **flag** → model `+0x490c`: painter's sorting of the model's faces. Moving
  objects (`FUN_010c101c`) insertion-sort by the sum of the 3 vertex depths and
  re-sort whenever roll/pitch changes; scenery (`FUN_010c4fec`) sorts once by
  the deepest vertex; the tumbling info sign (`FUN_010c7218`) sorts every
  frame. 106 files have 1, 24 have 0 (doors, arrows, PowerBar, World, AlarmBox,
  weapon pickups, Chimney, Crate, Info, Rock, Vehicles/Bus, bullets, grenade).
- Texture names refer to `ObjectTextures/`. `u` is in texels (0–255) and `v`
  in half-texels; the loader halves v and clamps it to 2..125.
- A face has 0 (36 faces), 1 (7984) or 2 (28) active slots.
- Up to 180 vertices and 344 faces per model.

Slot **mode** (renderer `FUN_010c101c`; counts are active slots in the data;
`rendering.md` §10):

| Mode | Count | Meaning |
|---|---|---|
| 0 | 0 | unused (all inactive) |
| 1 | 1077 | flat colour: `param` through `General/64->256.dat` |
| 2 | 3244 | textured; `param` ignored |
| 3 | 113 | textured, texel 0 transparent (`FUN_0111e410`) |
| 4 | 24 | textured, additive (Alpha table 0; table 4 when drawn with 100) |
| 5 | 6 | line between the first two vertices, colour `param` |
| 6 | 5 (+17 inactive) | lens flare; `param` = Alpha table |
| 0x11 (17) | 357 | own texture, then `CloudsReflect` env-mapped over it (table 9), per-face normals |
| 0x12 (18) | 358 | slot texture (`Shine.*`) env-mapped, opaque, per face |
| 0x14 (20) | 24 | env-mapped through table 4 (50%); `CD.obj` only |
| 0x22 (34) | 4 | one leg face in each of PedMale1–4 ("White sand", param 3): bit 0x10 clear and low bits not 1/2/3, so it falls into the mode-4 branch = additive |
| 0x31 (49), 0x32 (50) | 608, 2220 | as 0x11 / 0x12 with per-vertex env coordinates (vertex normals at model `+0x3580`) |

Bit 0x10 = environment-mapped, bit 0x20 = per vertex instead of per face
(bit 0x20 alone does nothing). The loader maps `param` through 64→256 for
every mode except 6.

People models: y is the long axis (head ≈ −88, feet ≈ +90), limbs 1–14,
animated by `Motion/Human.motion`. The loader (`FUN_0104e13c`) divides
MaleNormal's `+0x48c4` (max |X| = collision radius → 192) and `+0x48d0`
(max |Z| → 64) by 3 and every people model copies them; this matches the
horizontal /3 of the people transform (`people.md` §2).

## `Motion/Human.motion` (confirmed)

ASCII integers: 12 frames × 14 limbs × (A, B). Loader 68k `FUN_0009508a`, OS4
`FUN_010bd3f0`; meaning and blending in `people.md` §3. Parser
`payback-formats/src/motion.rs`.

## Textures (confirmed)

Raw 8bpp chunky, palette-indexed:

| Files | Size |
|---|---|
| `CubeTextures/*.chunky`, `EffectTextures/*` | 64×64 (`#Water.chunky` has 64 extra bytes) |
| `ObjectTextures/*` | 256×128 |
| `Backgrounds/*.chunky` | 320×256 (`*.chunky2` = 24-bit RGB, Update 4+) |
| `Icons/*` | w×h, row stride rounded up to 16 (`frontend_audio.md` §1) |

`.CubeTextureList`: one `<flags> <file>` per line. The flag is the footstep
sound id and the vehicle grip class (4, 34, 35, 36, 50, 51, 52, 65; stored at
texture `+0x1040`, `FUN_0106dfcc`; `rendering.md` §8). `.CubeTextureRemapList`
(Update 7): line i names the texture that replaces texture i when Gouraud
lighting is on (`FUN_010766c8`; `rendering.md` §4).

## Palette (confirmed)

Not a file. It is a `LoadRGB32` table in the executable: `u32 0x01000000`,
256 × (`u32` R, G, B) with the component in the top byte, `u32 0`. Update 7's
`Payback.exe` has four identical copies; the CD release's palette differs.
Found by signature (`payback-formats/src/palette.rs`).

`General/64->256.dat` (64 bytes; extracted as `64-_256.dat` on Windows, where
`>` is illegal) maps the 64 flat-colour indices of object faces to palette
indices. `General/Alpha.256` holds precomputed alpha-blend tables
(regenerated if missing; `rendering.md` §3). `General/RandomNumbers` is 65536
big-endian shorts in 0..511 (`Random`, `script_api.md` §5).

## Text — `Phrases/<level>.<Language>.phrases` (confirmed)

`<n> = "<text>"`; `#` comments; `<...>` marks tutorial hints. Shipped
translations have multi-line entries and missing quotes; the game warns
"Missing quote at end of phrase" and carries on.

## Sounds — `Sounds/*` (confirmed)

Headerless signed 8-bit mono PCM. There is no per-file rate: every AddSound
call (OS4 `FUN_0100815c`) passes its own frequency, quantised to 200 Hz steps;
the mixer runs at 22050 Hz; menu sounds use Paula periods
(`frontend_audio.md` §4).

## Intro video — `Videos/intro.video`, `intro.audio` (confirmed)

Autodesk Animator Pro FLC plus raw signed 8-bit PCM (`frontend_audio.md` §5;
parser `payback-formats/src/flc.rs`).

## Replays — `Replays/*` and saves (confirmed)

Replay header and records: `frontend_menus.md` §8 (version 10 in Update 7;
the CD's version 3 has a shorter header and the same records). Save slots
`Saves/PaybackSlot%d.save`: `script_api.md` §7. Config `S:Payback.config`:
`frontend_audio.md` §3.7.

## Mission scripts — `Maps/<nn>.script` (confirmed)

AmigaOS hunk executables (compiled C), one per level (00, 01, 10, 11, 20, 21:
two levels per city). In the CD release they run as separate processes and
talk to the game through messages; Update 7 compiles the level code into the
game and ignores them (`script_api.md` §1).
