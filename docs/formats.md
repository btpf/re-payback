# Payback data formats

All multi-byte values are big-endian (68k). Paths are relative to the install
directory (CD `Payback/` overlaid with Update 7). AmigaOS paths are
case-insensitive; the game refers to files in lower case.

Status: **confirmed** = verified against code and every shipped file;
**observed** = consistent with the data but not yet traced in code.

## Maps — `Maps/<n>.map`, `UserMaps/*.map` (confirmed)

Loader: OS4 `FUN_01079300` ("LoadMap"). Parser: `payback-formats/src/map.rs`.

| Field | Type | Notes |
|---|---|---|
| magic | u8 | `0xFF` |
| version | u32 | 1 |
| night | u32 | 1 for map 2 (night city) |
| variant | u32 | 0, 1, 2 for maps 0, 1, 2; meaning TBD (weather?) |
| squares | 128 × 128 | x-major: outer loop x, inner loop y |

Each square is a `u8` with 2 bits per cube (cube 0 = bits 0–1), followed by
three cube records, bottom to top:

| Kind | Size | Contents |
|---|---|---|
| 0 | 2 | `i16` height (cube off; still stacks) |
| 1 | 8 | header + top face |
| 2, 3 | 16 | header + top, north, east, south, west faces |

Header (6 bytes): `slope<<5 | special`, `rotation`, `scenery`, `flags`,
`i8 height1`, `i8 height2`. Heights ×16 give world units. A face is a `u16`:
`flip<<15 | type<<12 | texture`.

- **slope**: 0 Off, 1 Flat, 2 Vert, 3 Horiz, 4 RVert, 5 RHoriz (map editor names).
  Flat cubes use `height1`; `height2` is a leftover default (usually 256).
- **face type**: 0 Off, 1 Normal, 2 Genlocked (colour 0 transparent),
  3 Alpha (50%), 4 Reflect (water; texture is a diffusion map).
- **texture**: index into `.CubeTextureList`; ≥ 2000 = custom map texture.
- **scenery**: 1-based index into the scenery table (below); 0 = none.
- **rotation**: scenery heading, 256 = full turn (observed).
- **special** (5 bits): crate contents / traffic light sequence / start point
  number / phone reference (map editor F7). Scenery 14 (phone) squares with
  special *n* register phone *n*. Squares with scenery 0, flags `0x20` and
  special 1 are start points (up to 100).
- **flags**: `0x20` pedestrians walk here; low bits probably traffic flow
  N/E/S/W/straight (observed values 1, 2, 4, 8, 16), `0x40`/`0x80` TBD.

World geometry: a square is 256 units wide (observed); ground cubes are
256 high; buildings stack to ~1024. North is −y.

## Companion map files (TBD)

| File | Size | Notes |
|---|---|---|
| `<n>.mini` | 4225 = 65×65 | minimap |
| `<n>.thumb` | 65536 = 256×256 | 8bpp thumbnail (palette below) |
| `<n>.route` | 276480 | police routing graph |
| `<n>.vehicles` | ~100 | vehicle spawns (Update 6+) |
| `<n>.textures` | — | custom textures for user maps |

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
radar 100.

## Objects — `Objects/**/*.Obj` (confirmed grammar)

Text, one token per CRLF line. Parser: `payback-formats/src/obj.rs`.

```
1001                   magic
flag                   0/1, TBD
nverts
x y z w  (×nverts)     z negative = up; w = limb index for people, else 0
nfaces
a b c                  1-based vertex indices
2 × slot               name, u0 v0 u1 v1 u2 v2, mode, param, active
```

Texture names refer to `ObjectTextures/`. `u` is in texels (0–255) and `v` in
half-texels (0–255 over 128 rows). One slot per face has `active = 1`.
Observed `mode` values: 0–4, 17, 18, 20, 49, 50; `param`: 0–4, 13, 18, 24, 25.

People models: y is the long axis (head ≈ −88, feet ≈ +90), limbs 1–14,
animated by `Motion/Human.motion` (format TBD). The loader divides two
people-model fields by 3 (scale TBD).

## Textures (confirmed)

Raw 8bpp chunky, palette-indexed:

| Files | Size |
|---|---|
| `CubeTextures/*.chunky`, `EffectTextures/*` | 64×64 (`#Water.chunky` has 64 extra bytes) |
| `ObjectTextures/*` | 256×128 |
| `Backgrounds/*.chunky` | 320×256 (`*.chunky2` = 24-bit RGB, Update 4+) |

`.CubeTextureList`: one `<flags> <file>` per line; flags observed 4, 34, 35,
36, 50, 51, 52, 65 (likely surface material). `.CubeTextureRemapList`
(Update 7): one file name per line, aligned with the list; used for night
lighting (TBD).

## Palette (confirmed)

Not a file. It is a `LoadRGB32` table in the executable: `u32 0x01000000`,
256 × (`u32` R, G, B) with the component in the top byte, `u32 0`. Update 7's
`Payback.exe` has four identical copies; the CD release's palette differs.
Found by signature (`payback-formats/src/palette.rs`).

`General/64->256.dat` (64 bytes; extracted as `64-_256.dat` on Windows, where `>` is illegal) is
a "palette conversion table". `General/Alpha.256` holds precomputed
alpha-blend tables (regenerated if missing).

## Text — `Phrases/<level>.<Language>.phrases` (confirmed)

`<n> = "<text>"`; `#` comments; `<...>` marks tutorial hints. Shipped
translations have multi-line entries and missing quotes; the game warns
"Missing quote at end of phrase" and carries on.

## Sounds — `Sounds/*` (observed)

Headerless signed 8-bit mono PCM. Sample rates are set in code (TBD).

## Mission scripts — `Maps/<nn>.script` (observed)

AmigaOS hunk executables (compiled C), one per level (00, 01, 10, 11, 20, 21:
two levels per city). They call back into the game through an API whose
error strings end in `_script` (e.g. `AddSound_script`).
