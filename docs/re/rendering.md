# City rendering (reverse-engineered)

Source of truth: the AmigaOS4 PPC build (`analysis/ghidra_os4/decompiled.c`, addresses
below are OS4 unless marked 68k). The OS4 port contains three renderers sharing one
code path: the original **software** (chunky, Amiga) renderer, a Warp3D **hardware**
path (`DAT_0114c7d8 != 0`) and the **rotating view** (`DAT_0114c7dc != 0`). The
software / non-rotating path is what the Amiga game shows by default and is the
reference here; the hardware renderer is §11.

Tags: **CONFIRMED** = traced in code (and checked against data where possible);
**INFERRED** = consistent with code/data but not fully traced.

---

## 0. Units and coordinate systems

| Quantity | Unit | Status |
|---|---|---|
| Map square | 128 × 128 squares, square (x, y), x east, **y south** (screen down) | CONFIRMED |
| Camera position | 16.16 fixed point, `0x10000` = 1 square | CONFIRMED (`FUN_01049374`) |
| Object / effect position (x, y) | `0x800` (2048) per square; square centre = `sq*0x800 + 0x400` | CONFIRMED (`FUN_0106ef6c`) |
| Height ("h-unit") | in-memory cube height; file `i8 << 4`; flat ground cube = 256 | CONFIRMED (`FUN_01079300`) |
| Object z | `16` per h-unit, measured **downward** from a reference plane 1500 h-units above the bottom of cube 0: `z = (1500 − H) << 4` | CONFIRMED (`FUN_0106ef6c`, `FUN_0106f1ac`) |
| Depth (renderer) | h-units below the camera: `D = camH + 1500 − H` | CONFIRMED |
| Angles | `0x1680` = 5760 per turn; header `rotation` byte → `rot*0x5a>>2` = `rot*22.5` units (256 = full turn) | CONFIRMED |

**The bottom of cube 0 is at H = 0** and there is no other global height offset: the
frame renderer starts every square's stack at depth `camH + 0x5dc`
(`iVar41 = param_3 + 0x5dc` in `FUN_01049374`). Objects and effects get a small
extra offset: in the non-rotating view they are projected with camera depth
`camH*16 − 0x100`, i.e. drawn as if **16 h-units nearer the camera** than the
city (`FUN_01049374`: `iVar52 = param_3*16 − 0x100`; `FUN_0101cd00`). CONFIRMED.

### Square size vs. height; .Obj scale

**1 square = 256 h-units** (so a flat ground cube, 256, is a true cube) — CONFIRMED
from the rotating view, which is a rigid 3D rotation and must mix the axes in
consistent units: in `FUN_01091f84`/`FUN_01018844` the forward axis is
`(cos·(y>>8) − sin·(x>>8)) >> 9` = 128 units per square and it is added directly to
`Z>>1` (depth halved) — 128 × 2 = 256 h-units per square. (The top-down projection
alone cannot fix this ratio; it only fixes focal length × square width.)

Trig tables (CONFIRMED, generated at init, decompiled.c ~41850):
`cos[i] = DAT_012a676c[i] = (int)(256·cos(i·2π/5760))`,
`sin[i] = DAT_012ac16c[i] = (int)(256·sin(i·2π/5760))`, i = 0…5759.

**.Obj model scale** (CONFIRMED): vertices are ×16 at load (`FUN_010bf8bc`) and
projected as `X = 2·cos·vx + 2·sin·vy + px·0x200` (`FUN_010c4fec`), so
**1 model unit = 1/128 square = 2 h-units horizontally**. In the rotating view the
model's z gets the same 2 h-units (all depths are halved there), i.e. models are
uniform. In the default top-down view object depth is *not* halved, so model z is
drawn at **1 h-unit per model unit — models appear squashed 2× vertically** relative
to the cubes (a quirk; only visible as slightly weaker perspective on tall objects).

Port mapping with 1 square = 256 world units (as `crates/payback/src/city.rs`):
`world = (x_sq·256, H, y_sq·256)`, models scaled uniformly ×2 (×1 on the vertical
axis to mimic the top-down quirk). Positions: `world_xy = pos/8`, `world_up =
1500 − z/16`. Model examples: crate 40³ → 80³ units (0.31 sq), Evo car
64 × 146 × 44 → 128 × 292 × 88, lamppost 252 tall → 504 (≈ 2 storeys).

---

## 1. Cube stacking and slopes

In-memory cube (0x28 bytes, square at `map + x*0x3c00 + y*0x78`, cube i at `+i*0x28`),
built by LoadMap `FUN_01079300` (CONFIRMED):

| Off | Field |
|---|---|
| +0 | rotation (scenery heading) |
| +1 | scenery id |
| +2 | `short h1` = file `i8 << 4` (kind 0 / "Off" record: raw `i16`) |
| +4 | special (5 bits) |
| +5 | flags |
| +6 | `short h2` = file `i8 << 4` (Off: = h1) |
| +8 | slope (0 Off … 5 RHoriz) |
| +9 … +0xd | face type byte for top, N, E, S, W: `type (bits 0–2) | flip (bit 7)` |
| +0x10 … +0x20 | texture data pointers for top, N, E, S, W |
| +0x24 | pointer to 24 shorts of per-vertex light (only with Gouraud on, §4) |

Kind-1 records (header + top only) get side types 0 → no side faces.

### Stacking (CONFIRMED)

Per square the renderer keeps a running base depth (`DAT_012e5ae0[x*128+y]`). Each
cube is drawn by `FUN_010a3f1c(cube, X, Y, base)` which returns `base − h1`, and that
becomes the base of the next cube. So:

```
H_base(cube0) = 0
H_base(cube i+1) = H_base(cube i) + h1(cube i)      # for EVERY cube: Off, Flat and all slopes
```

`h2` never contributes to stacking. (The current `crates/payback/src/city.rs` uses
`max(h1,h2)` for sloped cubes — that is wrong.)

### Top surface per slope (CONFIRMED: `FUN_010920d8` Flat, `FUN_010992ac` Vert,
`FUN_0109bdbc` Horiz, `FUN_0109e8cc` RVert, `FUN_010a13f4` RHoriz; dispatch in
`FUN_010a3f1c` on cube+8; top is skipped if slope = 0 or `(type & 0x7f) == 0`)

Corner heights above the cube base (x0/y0 = west/north edge, x1/y1 = east/south):

| slope | NW | NE | SE | SW | edge with h1 | edge with h2 |
|---|---|---|---|---|---|---|
| 1 Flat | h1 | h1 | h1 | h1 | all | — |
| 2 Vert | h1 | h1 | h2 | h2 | north | south |
| 3 Horiz | h1 | h2 | h2 | h1 | west | east |
| 4 RVert | h2 | h2 | h1 | h1 | south | north |
| 5 RHoriz | h2 | h1 | h1 | h2 | east | west |

Exception — **Reflect (type 4) on a Flat cube**: the water surface is drawn at
**h2**, not h1 (`FUN_010920d8`: `param_4 = param_4 − *(short*)(cube+6)`), while the
stack still advances by h1. Data: map 0 water is `h1=256, h2=0` (water surface at
H=0, i.e. a one-cube-deep canal between 256-high banks); maps 1/2 use `h1=h2=128`.
Reflect is only ever used on flat tops in the shipped maps.

### Side faces (CONFIRMED: `FUN_010bb8f0` → N `FUN_010a4004`, E `FUN_010b5a88`,
S `FUN_010a9e64`, W `FUN_010afcc4`)

* Drawn only if slope ≠ 0 and the face's `(type & 0x7f) != 0`.
* Always a **rectangle from H_base to H_base + h1** along the square edge — the side
  functions never read slope or h2.
* Face order in the file/struct: top, **N (y0 edge), E (x1 edge), S (y1 edge),
  W (x0 edge)**.
* Back faces are culled (hardware/rotating paths: screen-space cross-product sign,
  CONFIRMED; software top-down path: an explicit test, CONFIRMED: `FUN_0107b910`
  (Normal) and `FUN_0107bd3c` (Gouraud) return unless the sign of the edge offset from
  the camera matches the top/bottom depth order, and return on zero-height walls;
  `FUN_0107c260` / `FUN_0107d74c` (the other face types) return when the projected top
  row is above the bottom row. The absolute sign convention was not re-derived: Ghidra
  confuses `in_r6` and `param_4` there).
* Draw order within a layer (§6) is: all side faces of cube layer L, then all tops of
  layer L, so a sloped top **paints over** the part of a side rectangle that rises
  above a lower slope edge. Where a slope edge is *higher* than h1 (h2 > h1 at that
  edge — 82/546 Vert cubes in map 0) there is a gap between the wall top and the
  slope edge in the original.
* Port recommendation (visually equivalent with a z-buffer): build side quads as
  trapezoids whose top edge follows `min(cornerHeight, h1)`, *or* draw the full h1
  rectangle and clip it against the slope plane. Do not extend walls up to h2 > h1.

### Layer limit (interiors) — CONFIRMED

`DAT_0114c728` (default 3, set by scripts via `FUN_01041064`, called from
`FUN_010cf9f0`) limits drawing to cube layers `< limit`; at layer `== limit` only
sloped cubes draw sides and only slope 2/5 cubes draw tops. Used to cut away upper
storeys when the player is inside.

---

## 2. Texture mapping

Cube textures are 64×64, row-major; **u = column (byte within row), v = row**. Software
spans index `tex[v*64 + u]` (`FUN_0112c108`). All mappings below are for flip bit = 0;
**flip = 1 mirrors u only** (u → 63 − u) for every face and slope (CONFIRMED in all
renderers: the flip bit selects `u0,u1 = 63,0` vs `0,63`). Texel centres are not
treated specially: u/v run 0 → 63 across the face (software: `0x3f0000/width`,
`0xfc0/height` steps), i.e. one texture copy per face, **no tiling**.

### Top faces (CONFIRMED)

| slope | u = 0 … 63 runs | v = 0 … 63 runs |
|---|---|---|
| Flat | **east → west** (u=63 at x0) | north → south (v=0 at y0) |
| Vert, RVert | east → west (u=63 at x0) | **south → north** (v=0 at y1) |
| Horiz, RHoriz | north → south (u=0 at y0) | west → east (v=0 at x0) |

So a flat top with flip = 0 shows the texture **mirrored left-right** when viewed with
north up; flip = 1 shows the file as stored (row 0 at the north edge, column 0 at the
west edge). The shipped maps set flip on almost every top (map 0: 17862 of 17931
tops), i.e. "flip" is effectively the normal orientation. Sloped tops are mapped
bilinearly over the four projected corners (affine per scanline in software, not
perspective-correct).

Exception: **Alpha (type 3) flat tops in the software renderer ignore the flip bit**
and always use u = 0 at x0 (`FUN_01036044` → `FUN_01033d50(tex, 1, 0, …)`).

### Side faces (CONFIRMED)

The whole 64×64 texture is **stretched over the full rectangle** (one copy per face,
regardless of h1): v = 0 at the top edge (H_base + h1), v = 63 at the bottom (H_base).
Horizontally u = 63 at the face's x0 / y0 end and 0 at its x1 / y1 end, for all
four faces:

| face | u = 63 at | u = 0 at | seen from outside (flip 0) |
|---|---|---|---|
| N (y0) | west | east | correct |
| E (x1) | north | south | correct |
| S (y1) | west | east | mirrored |
| W (x0) | north | south | mirrored |

(Shipped data: map 0 sides are mostly flip 0, maps 1/2 almost all flip 1.)
Type 5 (animated door, set at runtime) moves the bottom edge up by
`FUN_010f693c()·h1 >> 8` with v still spanning 0…63 (texture squashes).

---

## 3. Face types (CONFIRMED unless noted)

| type | name | top | side |
|---|---|---|---|
| 0 | Off | not drawn | not drawn |
| 1 | Normal | opaque | opaque |
| 2 | Genlocked | texel 0 transparent (`FUN_0112c5cc` skips 0) | same |
| 3 | Alpha | blended through Alpha.256 table 4 (50 %) | drawn as Normal (software) |
| 4 | Reflect | screen-space reflection of the background (below) | drawn as Normal |
| 5 | (runtime) door | — | rectangle with raised bottom |

### Alpha.256 (from `FUN_0103c408` / generator `FUN_0103b540`)

0xB0000 bytes = 11 tables `T[k][dst*256 + src]` (dst = framebuffer pixel, src =
texel), each entry = nearest palette colour (`ClosestMatch`, Mac @ 10083dc0: metric
`25|ΔR|+36|ΔG|+16|ΔB|`, first best wins, result 0 replaced by 8) of the per-channel
result clamped to 0…255. Exact formulas (CONFIRMED, Mac `CalcAlpha__Fv` @ 10083ec0,
called by `GetAlpha__Fv` @ 10084710 when the file is missing; OS4 `FUN_0103b540`):

| k | offset | op (per channel) | used by |
|---|---|---|---|
| 0 | 0x00000 | min(d+s,255) additive | flares, flames, dust |
| 1 | 0x10000 | max(d−s,0) subtract | shadows, smoke, night remap (T1[c][0x67]) |
| 2 | 0x20000 | max(d−16,0) where s (index) ≠ 0, else d | |
| 3 | 0x30000 | (s·255 + (255 − max(sR,sG,sB))·d) >> 8 | blood, dirt |
| 4 | 0x40000 | s if s = d (index), else (d+s) >> 1 | **cube Alpha faces** |
| 5–8 | | add R / R+G / G / B only | coloured flares |
| 9 | | d+s−32 | |
| 10 | | pal[s] + d − 128 (d = the index itself) | |

The Mac's shipped `Alpha.256` is exactly this computation on the current palette (all
11 tables). The Amiga Update 7 file was built from the CD palette (colour 8 = 7,7,7)
and differs in 0.2–0.8 % of entries; the OS4 generator also lacks the table 4 `s = d`
shortcut (7 diagonal entries between duplicate colours). Port (`postfx.rs`
`alpha_tables`, `night_remap`): computes the tables with these formulas and ignores
the file (the night remap is `T1[c][0x67]`, §4). `Gouraud.256` is computed too (the
Mac's shipped one is stale, built from the CD palette).

### Palette output and motion blur (port: `crates/payback/src/postfx.rs`)

Every pixel of the original is a palette index. The port renders true colour and runs
one final pass per game view (after the HUD, before the window blit): each pixel is
snapped to the nearest palette colour with the metric above (a 256³ table computed
exactly by a separable weighted-L1 distance transform, ties to the lower index, 0 →
8), then the palette flash is added (§4). Gouraud results (`Gouraud.256` = nearest of
`pal[c] + light − 128`) and the shine overlay (table 9) therefore come out exactly as
the tables, up to blending order. Still approximate: blends with the framebuffer
(effects, Alpha tops, additive faces) run in linear light on the sRGB target and are
snapped once at the end, where the original snaps after each blend and adds the
gamma-encoded palette RGB (additive effects look weaker than the original's).
`--no-palette-snap` turns the snap off for comparison.

**Motion blur** (CONFIRMED; option "Motion blur", live flag `DAT_0114c6c8`, toggled by
`FUN_01042308`, saved as config field 2 `DAT_0114c72c`; software renderer only — the
pause menu refuses it under Warp3D): at the end of each frame `FUN_01042f00` calls
`FUN_01034438` → `FUN_0112cdb8(DAT_0114c7a8, DAT_0114c7ac, DAT_0114c7b0)`, which for
every pixel writes `T[a·256 + b]` with the table pointer `DAT_0114c440 + 0x40000`
(table 4, the average) from the two render buffers into the display buffer
`DAT_0114c7b0` (`lhz`/`rlwimi`/`lbzx`, 32 pixels per iteration, `(W·H >> 5) − 1`
iterations); then `DAT_0114c6bc` flips and the next frame renders into the other
buffer. So the visible frame is `T4[previous][current]`, the nearest palette colour to
the average of the last two rendered frames (no feedback). Switching blur on: the Mac
`ToggleMotionBlur__Fv` @ 10053210 blanks both render buffers (`BlankChunky`), so the
first blended frame averages with black (CONFIRMED); OS4 `FUN_01042308` does not, and
shows a stale frame from the last time blur was on. The port does the same in the
final pass (it keeps each pixel's palette index of the previous frame and clears it to
index 0 when blur turns on); in the hardware render mode (§11) neither the snap nor
the blur runs.

Cube Alpha tops: `FUN_01036044(tex, x0,y0,x1,y1, 0x40000)` temporarily adds 0x40000
to the current table pointer `DAT_0114c440` and draws with `FUN_0112bfdc`
(`out = T4[dst][texel]`; texel 0 is **not** skipped). Shipped uses: `Ice2.chunky`
over ice (map 1, on a zero-height cube 1 above the Reflect ice), `RoofWindow.chunky`.

### Reflect / water (software renderer)

* `#Water.chunky` is the only `#`-prefixed texture; its 4160 bytes are 64 rows + a
  **copy of row 0** (the extra 64 bytes, verified). The loader reads 0x1040 bytes for
  `#` names and remembers the pointer in `DAT_0114c870` (`FUN_0106b7a0`).
* Each frame `FUN_01071d90(ticks)`: `scroll = (scroll − ticks) mod 64`
  (`DAT_0114c87c`), called from the game loop at 0x105fdfc.
* Drawing (`FUN_010920d8` → span `FUN_0112ca44`): if the face texture is the `#`
  texture, the texture base pointer is advanced by `scroll` **bytes**; then for each
  screen pixel `(sx, sy)` covered by the water square, with texel `t` (u,v mapped like
  a flat top):

  ```
  out[sy][sx] = BG[sy*W + sx + t]        # t = water texel value, 113..141
  ```

  `BG` (`DAT_0114c888`) is the current background (`Backgrounds/Clouds.chunky` by day,
  `Stars.chunky` at night) rescaled once at load to W × (H+10) (nearest neighbour,
  `FUN_0106e564`); the 10 spare rows absorb the byte offset. So the water texture is a
  **displacement map**: the reflection is the sky picture in fixed screen space,
  shifted right by ~113–141 px with ripples, and the ripples drift 1 byte (≈ 1 texel
  along u, wrapping into the next row) per tick. Nothing in the scene (buildings,
  cars) is reflected.
* Other Reflect textures (`Ice.chunky`, map 1) use the same displacement but do not
  scroll.
* Water tops are always lit "fully lit" (light 0x80) under Gouraud.
* Warp3D rotating view (§11.4 has the non-rotating formulas): the background is a 320×256 texture sampled at
  `u,v = 128 + 8·(screen offset from centre rotated by camera yaw)/f + wobble`, where
  wobble = `sin(((x+y)·16 + scroll)·90 units)·k/16384` per vertex (`FUN_010920d8`
  ~line 3020). The "CloudsReflect" *object* texture (cars/glass) is unrelated to
  cube water.

---

## 4. Night maps, lighting, weather

### Night (header `night = 1`, map 2) — `FUN_0106ecf0(1)` (CONFIRMED)

* Every cube texture (and custom `.textures` entry) whose **file name does not start
  with `#` or `$`** is darkened in place: `c → T1[c][0x67]` = nearest(pal[c] −
  (87,87,87)), colour 0 unchanged. (Only `#Water` and `$Bars` are exempt.)
* Background becomes `Stars.chunky` (water reflects stars); day reload uses
  `Clouds.chunky` and re-reads the textures (`FUN_0106e3a8`).
* Effect texture `Blood` is replaced by `Blood.dark` (`FUN_01035250`).
* Object textures (`FUN_010bf310(1)`): `Shine.*` (not `Shine.cup*`) and
  `CloudsReflect` are swapped for their `.dark` files; `Joystick`, `Red Shade`,
  `Lights` untouched; everything else darkened through T1[c][0x67] except `TankBits`,
  the one object texture exempt from both the `.dark` swap and the darkening
  (CONFIRMED: OS4 `FUN_010bf310`, 68k `FUN_00095efa`).
* Vehicles get an extra polygon textured `Lights` — the **headlight beam**
  (CONFIRMED, `FUN_0104e00c` → `FUN_010bf070` on night load; `FUN_0104e0a4` →
  `FUN_010bf254` removes it again for a day map). It is added to the model itself (4
  vertices, 2 faces) for every model of the vehicle table `DAT_012a6364…63d0` and the
  four police siren variants (`DAT_0114c710…720`), except models with `+0x4928` set
  (tank, helicopter, boat). With r = `+0x48c4` (max |x|, ×16 units) and L = `+0x48c8`
  (max y of the near-ground vertices), in model ×16 units: vertices `(±2r, L − r/2, 0)`
  and `(±2r, L + 10r, 0)` — a rectangle on the model's ground plane, twice the car's
  width, from just behind the front bumper to ~10 half-widths ahead. Faces `(v0 v1 v2)`
  and `(v1 v3 v2)`, slot 0 only (one-sided), texture `Lights` (`DAT_0114c90c`), UVs
  (77,3) (17,3) (77,82) and (17,3) (17,82) (77,82) (texel, row — the beam picture in
  the top-left of `Lights`), **mode 4** (additive, §10) param 2.

### Gouraud lighting (option; `DAT_0114c2fc`, "Gouraud shading" menu) — CONFIRMED

**Default on** in the OS4 build: the initialised data holds `DAT_0114c2fc = 1` and
`DAT_0114c2f8 = 1` (the "Gouraud available" flag; LoadConfig forces the option off
when it is 0, the menu entry is disabled then, and `Gouraud.256` is only built/loaded
when it is set, `FUN_0103c080`). `S:Payback.config` field 17 overrides it. Toggling it
in the pause menu relights the loaded map (`FUN_010792dc` → `FUN_010766c8`; off:
`FUN_01074cbc` + `FUN_01079c54` reload the plain map). When on, `FUN_010766c8` runs
after LoadMap:

1. **`.CubeTextureRemapList`** (one name per line, aligned with `.CubeTextureList`):
   every face's texture index i is replaced by the index of the texture named on line
   i. 295 lines differ and they map *baked-lighting variants back to plain textures*
   (DarkPavement→Pavement, RoadShadow1→Road, AltRoofShad2→NewRoof …): dynamic light
   replaces the painted-in shading.
2. Per cube 24 shorts at cube+0x24: three channel groups R, G, B × 8:
   `[0..3]` top corners, `[4..7]` bottom corners (the bottom of the cube's walls).
   Corner order: **0 = NW (x0,y0), 1 = NE (x1,y0), 2 = SW (x0,y1), 3 = SE (x1,y1)**
   (CONFIRMED from the per-vertex assignments in the lit rotating paths of
   `FUN_010920d8`: vertex (x0,y0) takes `[0],[8],[0x10]`, (x1,y0) `[1],[9],[0x11]`, …;
   the walls use top/bottom pairs, e.g. the N wall `[0] [1]` over `[4] [5]`,
   `FUN_010a4004`; values are clamped to 255 when drawn).
3. **Voxel grid** `DAT_0114c80c` (`FUN_010759e0`): 256×256×10 bytes, index
   `(hx·256 + hy)·10 + z`, hx/hy = half squares, z = `H >> 7` clamped 0…9. It marks
   *faces*, not solids: a visible top marks the sub-cell under each corner at that
   corner's height (Reflect tops at h2, slopes per §1); a visible side face marks the
   column `base>>7 … (base+h1)>>7` of the two sub-cells along its edge.
4. **Base light** per corner (`FUN_010766c8`):

   | | lit R,G / B | shadow | slope step |
   |---|---|---|---|
   | day, software | 0x80 / 0x80 | 0x30 | 0x40 |
   | night, software | 0x60 / 0x60 | 0x30 | 0x20 |
   | day, Warp3D | 0xbe / 0xbe | 0x80 | 0x30 (+ random ±8 jitter per vertex) |
   | night, Warp3D | 0x20 / 0x30 | 0x0c / 0x10 | 0x20 |

   **Shadow test**: from the corner's grid point the ray checks the cells
   `(gx − k, gy − k, z0 + k)`, k = 1, 2, … while z ≤ 9 (z0 = the corner's `H >> 7`);
   any marked cell puts the corner in shadow. One half square west, one north and 128
   h-units up per step: a sun in the north-west at atan(128 / (128·√2)) ≈ **35°**
   elevation, consistent with object shadows falling to the south-east (§7). Squares
   on the map edge (x or y = 0 or 127) are not tested (lit). Reflect tops are lit with
   0x80 even at night.
   **Slopes**: lit top corners (value = lit) get `+step` on slopes facing the sun —
   RVert/RHoriz with h1 > h2, Vert/Horiz with h1 < h2 — and `−step` on slopes facing
   away (Vert/Horiz with h1 > h2, RVert/RHoriz with h1 < h2); flat tops and shadowed
   corners are unchanged.
   **Vert height quirk (CONFIRMED, not reproduced):** for Vert (slope 2) cubes the
   lighting pass adds h1 twice (`FUN_010766c8` ~01076cf4; Mac `DoLightSourcing` @
   1015bd18, same bug). Its corner heights are NW/SW = base + 2·h1 and NE/SE =
   base + h1 + h2, and the next cube up starts at base + 2·h1. The port deliberately
   uses the correct heights (decision in `mac_version.md`).
5. **Static lamps** (`FUN_0106c904`, never removed): scenery 33 (`lamppost_dark`; the
   loop renumbers it to 36 afterwards) and 36 (`lamppost_dark2`):
   `(800M + rand(100M), 600M + rand(80M), 400M + rand(60M))`; 35 (`lamp`):
   `(500M + rand(50M), 500M + rand(50M), 400M + rand(50M))`. Position: square centre,
   `z = (1500 − top)·16 − 2000` with top = the stack below plus the cube's own top
   (h2 on Reflect tops, the higher end of slopes, else h1). Square box
   `centre ± ((isqrt(max I) >> 12) + 1)` clamped to 1…126 (±8 for a lamppost); every
   corner in the box except those on its outer edge adds `min(255, I / d²)` per channel
   (integer division; d in object units, z 16 per h-unit). No occlusion. Scenery 2
   (`lamppost`, day) emits nothing. Map 2 has 257 lampposts and 113 lamps (the jail
   interior is lit by lamps).
6. **Dynamic lights**, added each frame with `FUN_0106ba08` (via `FUN_010729c8`, only
   while the option is on and fewer than 1000 are active) and subtracted again after
   drawing (`FUN_010729ec`). Same falloff as the lamps but box radius
   `(isqrt(max I) >> 13) + 1`, and **all intensities halved on day maps**
   (`DAT_0114c84c == 0`, software). Sources (at the object's position unless noted):

   | source | R, G, B | code |
   |---|---|---|
   | traffic light (scenery 1), k = special + phase − 1 (mod 6) | k < 2 (300M, 50M, 50M); k = 2 (600M, 300M, 50M); k 3–4 (50M, 300M, 50M); k = 5 (300M, 300M, 50M) | `FUN_010753e8`; list built at load by `FUN_01074cd8` |
   | crossing beacon (15), by sub-counter `DAT_0114c754` | 24: (150M, 150M, 25M); ≥ 25: (300M, 300M, 50M); 0: (200M, 200M, 33.3M); 1: (100M, 100M, 16.7M); else none | ditto |
   | effect type 0xB (explosion flare), size r | (900k·r, 900k·r, 700k·r) | `FUN_01036574` |
   | type 2 (fireball) | (140k·r + rnd 5M, 120k·r + rnd 4M, 100k·r + rnd 3M) | ditto |
   | type 7 (explosion) | (50k·r + rnd 2M, 40k·r + rnd 1.5M, 30k·r + rnd 1M) | ditto |
   | type 0xF (fire) | (100k·r + rnd 5M, 70k·r + rnd 3M, 50k·r + rnd 2M) | ditto |
   | burning object, timer 2…15, I = (radius + rnd 150)·200k | (I + rnd 5M, I + rnd 5M − 2.5M, I − rnd 5M) at pos ± rnd 150 | object update (decompiled.c ~43990) |
   | burning object, timer ≥ 16, I = (1.5·radius + rnd 300)·200k | (I + rnd 10M, I + rnd 10M − 5M, I − rnd 10M) | ditto |
   | missile (`DAT_012a6750`) | (100M, 90M, 80M), z − 500 | `AddCollideObj` (decompiled.c ~36294) |
   | petrol bomb (`DAT_012a6754`) | (30M + rnd 10M, 15M + rnd 5M, 5M + rnd 2M), z − 500 | decompiled.c ~47200 |
   | police car / van siren | red model (200M, 20M, 20M), blue model (20M, 20M, 200M), z − 500 | decompiled.c ~48831 |
   | muzzle flash, the tick a person fires | minigun (60M, 60M, 8M), pistol (40M, 40M, 10M), shotgun (60M, 60M, 20M), z − 500 | Mac `do_Person` @ 1013fbd0, next to the fire sounds |

   The effect-light jitter (`DoEffectsLighting` @ 100a82b0 on the Mac) is drawn from the
   cosmetic generator `random3`, B first, then G, then R (the Mac and 68k builds evaluate
   arguments last first; OS4 first to last, the odd one out; analysis/mac/followup.md). The burning-object and petrol-bomb jitter is drawn from the game's
   generator inside the object update in both builds; the port draws it from the
   cosmetic generator as well (spread, B, G, R, then the z, y, x offsets).

   Traffic lights and beacons sit at the square centre plus the model's `+0x4910 /
   +0x4914` (mean x / y of the ground vertices) rotated by the cube rotation
   (`x' = (cos·mx + sin·my) >> 8`, `y' = (cos·my − sin·mx) >> 8`), 500 z-units above the
   cube top. There is no headlight *light*: headlights are only the polygon above.
7. Software shading: `out = Gouraud[(R&0xF8)<<15 | (G&0xF8)<<10 | (B&0xF8)<<5 | texel]`
   (`FUN_0112c6ec`); 128 = unchanged. `Gouraud.256` (not shipped, generated by
   `FUN_0103c080` and saved): 32×32×32×256, value = nearest(clamp(pal[c] + (r,g,b) −
   128)) — an *additive* offset per channel, so 0x30 (shadow) is −80 and 0x60 (night)
   −32 on top of the night-darkened textures.

**Shadow ray start** (CONFIRMED for the NW top corner, `FUN_010766c8` at the
first test after the per-cube setup): the square loop runs x, y = 1…126 (grid offset
`local_74 = −0xa3c + 0x1400·(x−1)`, `+0x14` per y); the first cell read is
`local_74 + 0x1446 + z` = cell `(2x − 1, 2y − 1, z0 + 1)` for the corner at half-grid
`(2x, 2y)`, the next `(2x − 2, 2y − 2, z0 + 2)` (offset −0xa09 per step), unrolled
while z ≤ 9; `z0 = clamp((base + h) >> 7, 0, 9)`.

**Paired rays** (CONFIRMED, Mac `DoLightSourcing__FP3Map` @ 1015b840 calling
`CheckVoxels__Fiii` @ 100b3960, which returns 0 = shadow if any cell `(px−k, py−k,
z+k)`, z+k ≤ 9, is marked; OS4 `FUN_010766c8` has the same pairs inlined): NW tests
`(2x, 2y)`; NE is shadowed only if both `(2x+1, 2y)` and `(2x+2, 2y)` are blocked, SW
both `(2x, 2y+1)` and `(2x, 2y+2)`, SE both `(2x+1, 2y+1)` and `(2x+2, 2y+2)`; the
bottom corners repeat the four tests at `z = base >> 7`. A corner is lit if either of
two parallel rays (from the corner and from the half-square point beside it) is clear.
The port follows this.

**Vert height quirk** (CONFIRMED, not reproduced by decision): for a Vert (slope 2) cube
the shadow test adds h1 twice (Mac 1015bd18, OS4 ~0x01076cf4): corner z NW/SW =
base + 2·h1, NE/SE = base + h1 + h2, and the running base of later cubes in that square
is h1 too high. The port uses the correct heights (docs/re/mac_version.md, Decisions).

Port (`crates/payback/src/lighting.rs`, `shaders/city.wgsl`): static light per cube
corner computed at load or on toggle (items 1–5; the lamp intensities come from a
local generator), dynamic lights (item 6) collected per frame: those whose square box
reaches the squares a view draws (every split-screen view), nearest a view first, at
most 510 (the original's limit is 1000; 510 fit a 16 KB uniform block). Every city
vertex carries its quad's four corners and static lights; the vertex shader adds the
dynamic lights at each corner, clamps to 255, and the fragment shader interpolates the
four corners bilinearly (the software rasteriser interpolates along the edges and the
spans; splitting the quad into two triangles left a diagonal seam), quantises
`& 0xF8` and adds the light to the texel in gamma-encoded RGB; the palette snap of
`Gouraud.256` is the view's final pass ("Palette output", §3). The pause-menu option
(`Settings::gouraud`, default on) drives it; `--gouraud 0|1` overrides.

### Header `variant` = weather (`DAT_0114c854`, read via `FUN_0106e478`) — CONFIRMED

| value | maps | effect |
|---|---|---|
| 0 | 0, user maps | none |
| 1 | 1 | **snow**: 250 particles (type 0xC, `Dust`, additive, r 100–150) around the camera; settle as ground marks |
| 2 | 2 (night) | **rain + lightning**, **level 0 only** (`DAT_0114c558 == 0`; level 1 of map 2 is dry): 250 streaks (type 0xD, additive grey 0x67 lines) + splashes; looping `Rain` sound; random lightning (`rnd(1000) < ticks`) flashes the palette (adds to every RGB, decays ×3/4 per tick) with `Lightning` thunder |

All header values verified in the files: `Maps/0.map` 0, `1.map` 1, `2.map` 2 (night 1),
`UserMaps/*.map` 0. Weather runs only in single player (`DAT_0114c7c8 == 0`).

**Level start** (main loop `FUN_010692bc`, ~0x1069390, after the cameras are placed;
random numbers from `FUN_0103ac94`, call order x, y, z, vx, vy, vz, size. The Mac
`amiga_startlevel` @ 100fff40 draws them from the cosmetic generator `random3` in
last-first order (size, vz, vy, vx, z, y, x; the 68k order too), except the y of a raindrop, which is the
game's `random(0x5000)`, so the rain spawn still takes 250 game numbers):

```
cam = (camX >> 5, camY >> 5)                     # object units
repeat 250:
  x = cam.x + rand(0x8000) - 0x4000              # ±8 squares
  y = cam.y + rand(0x5000) - 0x2800              # ±5 squares
  z = 0x57e4 - rand(25000)                       # 22500 … -2500 (from the ground up to the camera)
  snow: AddEffect(0xC, Dust, x,y,z, rand(20)+10, rand(20)-10, rand(100)+200, rand(50)+100, list 3, blend 0)
  rain: AddEffect(0xD, Dust, x,y,z, rand(30)+50, rand(20)-10, rand(200)+1000, 1,          list 3, blend 0)
rain: PlaySound(61 Rain, loop, vol 0xff, 14000 Hz, non-positional)
```

(Velocities are /3 inside AddEffect: snow falls ≈ 67–100 z/tick and drifts east, rain
≈ 333–400 z/tick.)

**Update** (`FUN_010386e8`, types 0xC–0xE, no size change; 0xE gets gravity 17):

* View rectangle (non-rotating view), in object units, with W×H = screen size
  (320×256): `half = ((camH·16 + 0x2af2)·W + 0x800) >> 9` around `camX >> 5` (likewise
  y with H). A record outside it is **wrapped** by one rectangle width/height (once
  per update, x then y) instead of being deleted as other types are. The Mac
  (`UpdateEffects__Fiiiii` @ 100aa1f0, CONFIRMED) uses the drawn square range instead,
  `cx·(camH + 1500)/(fx << 12) + 1` squares (y with cy, fy) × 0x800, which does not
  grow with the screen width; rotating view `cx·(camH + 1500)/(fx << 10) + 1` both ways,
  rotating split screen `·3/(fx << 11)`.
* Removal at z > 24000 (H = 0, i.e. *below* the street level of 256; drops fall through
  roofs and streets alike). The Mac removes snow at street level, z > 0x4dc0 (19904),
  and puts the mark there (CONFIRMED, `UpdateEffects` @ 100aa1f0):
  * snow: a **mark** `AddEffect(8, Dust, x, y, 0x57e4, 0,0,0, 200+rand(50), list 0,
    blend 0)` (additive, lives ~225 ticks) and a new flake in list 3 at
    `x = cam.x + rand(0x10000) − 0x8000` (±16 squares), `y = cam.y + rand(0xa000) −
    0x5000` (±10), **z = 0x100 − camH·16** (the camera plane), same velocities/size as
    at level start;
  * rain: a new drop (±8 × ±5 squares, z = 0x100 − camH·16) and **3 splashes**
    `AddEffect(0xE, Dust, x, y, 0x5780, rand(100)−50, rand(100)−50, −200−rand(200), 1,
    list 3)` at the old drop's x, y. These random numbers come from `FUN_0103acc4`
    (`random3`). Argument order: the Mac and the 68k evaluate a call's random arguments
    last first, OS4 first to last (so OS4's order is the exception, not a Mac change;
    analysis/mac/followup.md). `AddEffect` arguments 9-11 match OS4 at all 112 call
    sites. The port draws last first everywhere.
* Drawing (software): type 0xC is an ordinary sprite (Dust through table 0, mirrored);
  type 0xD is a **1-pixel line** (`FUN_010360fc`) from the projection of `pos − v` to
  that of `pos`, coloured through `T0 + 0x6700` (= adds palette colour 0x67 to the
  framebuffer); type 0xE is **one pixel** through the same table. As list 3 is drawn
  after everything, weather is never hidden by the city; rain streaks near the camera
  plane are long and radiate from the screen centre.

**Lightning** (main loop, once per frame after `FUN_010542b8`; only map weather 2,
level 0, single player; counter `DAT_0114c58c`, cleared at level start):

```
flash = flash·(3/4)^ticks                         # DAT_0114c748, every frame, all maps
if counter == 0 and rand(1000) < ticks:
    counter = (rand(3)+1)·2                       # 2, 4 or 6
    4: PlaySound(62 Lightning, vol 0xff, 6500 + rand(500) Hz)
    2: PlaySound(62 Lightning, vol 0xf0, 7500 + rand(500) Hz)
    6: PlaySound(62 Lightning, vol 0xa0, 8000 + rand(500) Hz)      # all non-positional
if counter > 0:
    if counter even: flash += 100 + rand(150)     # Mac: flash = 100 + rand(150)
    counter -= 1                                   # per frame, not per tick
```

The Mac (`amiga_levelloop` @ 10100f50, CONFIRMED) **sets** the flash instead of adding,
so a multi-flash strike does not build up. The draws are the game's `random` in both.
Rain volume (Mac `SetHighestCube__Fi` @ 10106d90, CONFIRMED): when the visible-layer
limit changes, the `Rain` loop is set to volume 0xff if the limit is 3, else 0x7f
(quieter indoors).

So a strike is 1–3 flashes on alternate frames, with thunder at the strike (no delay).

**Palette flash** `DAT_0114c748` (also `+= power/250000` per explosion,
`FUN_0104405c`): the frame renderer `FUN_0105bd98` (end) calls
`FUN_0103ad6c(screen, flash)` while it is > 0, and once with 0 when it has just run out
(`DAT_0114c744` = last frame's value, stored by the main loop). Software: every palette entry becomes `min(255, rgb + flash)` per
channel (from the unflashed copy at `DAT_012a510c/550c/590c`) and is loaded with
`LoadRGB32`; Warp3D: a full-screen quad of colour `flash/255` (clamped to 1) drawn
additively. Nothing is darkened or recoloured per weather apart from this (the night
remap is the header's night flag, not the weather).

**Split screen** (CONFIRMED, level start in `FUN_010692bc`: the weather spawn and the
`Rain` sound sit inside `if (DAT_0114c7c8 == 0)`, the lightning likewise): no weather at
all in multiplayer, in any view.

Rotating view: the wrap rectangle is a fixed `±0xf00000 >> 9` = ±0x7800 object units
(15 squares) around the camera (`FUN_010386e8`, `DAT_0114c7dc != 0`).

Port: `crates/payback/src/weather.rs` (spawn, lightning, rain volume; off in
multiplayer), `effects.rs` (wrap, respawn, drawing), `ScreenFlash` (applied by the
view's final pass after the palette snap, as the original changes the palette; see
§3). It follows the Mac: cosmetic generator (`sim::VisualRng`) for the spawn and the
updater, reversed argument order, snow removed at 0x4dc0, the view-sized rectangle,
lightning setting the flash, and the rain volume. The snow marks are put on the top
of cube 0 under the flake (street level 0x4dc0 where there is none; the port
depth-tests list 0).

---

## 5. Sky / backgrounds (CONFIRMED)

`Backgrounds/Clouds.chunky` / `Stars.chunky` (320×256, 8-bit) are **never drawn as a
visible sky**: the camera looks straight down and nothing is drawn behind the city.
The background is used **only as the source image for Reflect faces** (§3). It is
static in screen space (no scrolling with the camera, no rotation in the software
renderer); in the Warp3D rotating view the reflection UVs are rotated with the camera
yaw. `.chunky2` (24-bit) files are only used by the hardware path (`FUN_0106e564`
opens the name with the last char patched to '2' when `DAT_0114c7d8 != 0`).

---

## 6. Camera and projection

### Screen constants (`FUN_0104107c`, CONFIRMED)

```
W = screen width (c31c), H = screen height (c344)
fx = c328 = (W+8)*16/320        fy = c324 = ((H+8)*16) >> 8
cx = c330 = ((W-1)>>1) + 1      cy = c32c = ((H-1)>>1) + 1      # 320×256: fx=fy=16, cx=160, cy=128
```

### Top-down (default) projection — CONFIRMED (`FUN_010920d8`, `FUN_010c4fec`)

```
X, Y = point − camera, in 1/65536 square          (camera x,y are 16.16 squares)
D    = camH + 1500 − H                            (h-units below the camera; camH = "param_3")
sx = cx + fx·X / (16·D)       sy = cy + fy·Y / (16·D)
```

The camera looks **straight down**, north (−y) up, no roll, no tilt. Focal length
for 320×256 = 256 px per (h-unit/h-unit) with 1 square = 256 h-units:
**horizontal FOV = 2·atan(160/256) = 64.0°, vertical = 2·atan(128/256) = 53.1°**
(scales with resolution: fx/cx stays ≈ 1/10 of the width).

Both angles hold at every screen size, CONFIRMED in the Mac build too
(`ReadScreenParams__Fv` @ 10051fc0 = OS4 `FUN_0104107c`: fx = (W+8)·16/320,
fy = ((H+8)·16)>>8): at 640×480 fx 32, fy 30, so the 10:8 world area is stretched
6.7 % wide on the 4:3 screen. Port (`camera::view_fov_for`): with
`Settings::original_aspect` (the default, "Aspect: Original" in the Graphics
options) the horizontal angle is fixed at the original's (64°; two players 49°) and
the vertical one follows the window's aspect with square pixels; "Aspect: Free"
fixes the vertical 53.1° and widens with the window instead.

### Camera motion — CONFIRMED (`FUN_010556e0(p)`, once per game tick, from `FUN_0105bd98`)

State per player p: `camX = DAT_012e1c2c[p]`, `camY = DAT_012e1c3c[p]` (16.16),
`camH = DAT_012e1c4c[p]`. Followed object: the driven vehicle (`DAT_012e1c1c[p]`, −1
on foot) or the player human. Object pos ×32 converts 0x800/square to 16.16.

```
tgtX = obj.x*32 + K*sin[heading] + clamp(obj.vx*V, ±0x2af80)     # likewise Y with cos / vy
camX = (4*camX + tgtX) / 5
t    = |c*speed| + B − (obj.z >> 4)                               # = H_obj + |c·speed| + B − 1500
top-down:  t = clamp(t, 1, 3000);  camH = (2*camH + t) / 3
rotating:  t = min(t, 3000);       camH = (2*camH + t − R) / 3
```

Steady state: camera **B + |c·speed| h-units above the followed object**.

| case | K | V | B | c | R |
|---|---|---|---|---|---|
| on foot, top-down / rotating | 100 / 150 | 0x140 | 1000 / 1500 | 3 / 2 | – / 800 |
| driving, top-down / rotating | 150 / 225 | 200 | 800 / 1000 | 3 / 2 | – / 600 |
| other non-human player object | 200 / 300 | 0x140 | 1500 | 3 / 2 | – / 600 |

(K·sin with sin = ±256: K=100 ≈ 0.39 square look-ahead along the heading; the
velocity lead is capped at ±0x2af80 ≈ 2.7 squares.) Special cases: level intro starts
at camH 10000 and eases with 16/17 smoothing for 35 ticks; after death camH sinks
by 2·ticks/tick to 0; a destroyed vehicle holds camH = 2000 for 60 ticks.

**Map edges**: camX/camY clamped to [0, 0x7fffff]; the renderer clamps its square
ranges to 0…127 and draws nothing outside the map, so the framebuffer's old contents
show: black, CONFIRMED with a caveat. The software framebuffers
(`DAT_0114c7a4/7b0/7ac`) are zeroed only at level start (`FUN_01051800`,
`FUN_010692bc`, zero-fill `FUN_01111180`) and palette entry 0 is black; there is no
per-frame clear, so (INFERRED) sprites crossing the off-map area leave trails there and
the lightning palette flash brightens it too. **Visible range** (top-down): half-width `D·cx/(fx<<12) + 1` squares,
half-height `D·cy/(fy<<12) + 1` with `D = camH + 1500` (≈ 4×3 at camH 0, 11×9 at the
maximum camH 3000); no other far clip.

### Draw order (CONFIRMED, `FUN_01049374`) — painter's algorithm

```
for layer L in 0..min(2, DAT_0114c728):
    for each visible square, 4 quadrants, far → near the camera row/column:
        (L>0) moving objects standing on this layer; side faces of cube L   (FUN_010bb8f0)
    for each visible square, far → near:   top of cube L                  (FUN_010a3f1c)
    effects list L                                                          (FUN_010386e8)
    scenery/overlays of layer L                                             (FUN_010c9360 …)
effects list 3 (weather, explosions) last
```

### Rotating view — CONFIRMED (option "Rotating view", `DAT_0114c7dc`, saved in `S:Payback.config`)

Heading-aligned overhead camera: `yaw → (0xb40 − heading) mod 0x1680` (follows the
player, halving the difference each frame), pitch `→ clamp(0x167f − speed, 0x157c,
0x167f)` (straight down when slow, up to 260 units = 16.25° tilt at speed ≥ 259).
All depths are halved; projection (`FUN_01091f84`):

```
fwd = (cos[yaw]*(y>>8) − sin[yaw]*(x>>8)) >> 9
lat = (cos[yaw]*(x>>8) + sin[yaw]*(y>>8)) >> 2
dep = cos[p]*(Z>>1) + sin[p]*fwd ;  den = dep>>4  (cubes: den = 200 if dep < 0xc80; objects culled if dep ≤ 0x14f)
sx = cx + lat*fx/den ;  sy = cy + ((cos[p]*fwd − sin[p]*(Z>>1))>>1)*fy/den
```

With the trig tables at ±256 and x, y in 16.16, Z in h-units, this is a pinhole camera
at the camera position, tilted forward by `0x1680 − pitch`, with focal length `8·fx`
px (128 on 320×256): HFOV 102.7°, VFOV 90° — half the top-down focal length, at the
same depth Z (the earlier note "camera also half as far" was wrong: Z is not halved
overall, `Z >> 1` only pairs with the `>> 4` of the depth). Together with the lower
camera (B − R: 700 h-units above a standing player instead of 1000) the rotating view
shows about 1.4× the width of the top-down view. Heading `h` points up the screen
(yaw = 0xb40 − h: `fwd ∝ −(sin h·x + cos h·y)`, negative = ahead = up). Visible range
`D·cx/(fx<<10)+1` both ways (two side-by-side views: `3·D·cx/(fx<<11)+1`).

Yaw and pitch are per player (`DAT_012f5bd8[p]`, `DAT_0114b5dc[p]`) and updated by
the render loop `FUN_0105bd98` once per frame, before that player's view: the yaw moves
half-way to `(0xb40 − heading) mod 0x1680` the short way round, the pitch half-way to
`clamp(0x167f − speed, 0x157c, 0x167f)` (heading `+0x14` and speed `+0x3b0` of the
driven vehicle, else the player). While the player is dead (`DAT_012a65e0[p] > 0`) the
yaw turns `+0x20·ticks` and the pitch drops `4·ticks` to 0x157c. After the views both
globals are reset to 0.

Follow parameters in the rotating view (`FUN_010556e0`): on foot (human models,
`+0x4909` set) K 150, B 1500, c 2, R 800; other on-foot player objects K 300, B 1500,
R 600; driving (vehicle `+0x4909` clear) K 225, V 200, B 1000, R 600; other vehicles
K 150, V 0x140, B 1000, R 800; `t = min(t, 3000)` without the lower clamp. In the
original the camera state is simulation state (people spawn window, traffic), so the
option changes the game. **Port departure** (decision: the rotating view is not stored
in replays): the simulation's camera (`GameCam`/`ViewCams`) always follows with the
top-down parameters, and the views are drawn from separate render-only eyes
(`camera.rs` `ViewEyes`, the rotating parameters above) that lighting, chimney smoke and
the effect wrap also use. A replay plays back identically with the view on or off.
The camera step runs after the tick's simulation systems (as the original calls
`FUN_010556e0` from the render loop), so the spawn windows read a well-defined state.

**Replay "TV camera"** (Mac only, CONFIRMED, `DrawMap` @ 1005a260 prologue; same code
in the OS 9 build): when the rotating view is on, a replay plays back
(`EnablePlayback__Fv` flag) and there is one player, the city is drawn from a roaming
camera `(_DAT_1017d2cc, _DAT_1017d2c8)` (16.16 squares) instead of the followed one.
Per frame: position `+= velocity (_DAT_1017d2c4, _DAT_1017d2c0) · ticks`, clamped to
`0..0x7fffff`; the yaw (`_DAT_1017d2bc`) moves a quarter of the way (short way round)
to `0x167f − GetAngle(target − camera)` (`GetAngle__Fdd` @ 10072340 = heading + 0xb40,
so the target is up the screen; target = the driven vehicle or the player, or while
the player is dead the view's own camera); the pitch is 0x14b4 (28.75°); the height is
`1000 − (player.z >> 4)`; the visible range is `D·cx/(fx<<10) + 4` (+4 instead of +1).
If the camera is more than `range >> 2` whole squares from the followed camera in x or
y it re-spawns: `random3(4)` = 3 → velocity `random3(0x800) − 0x400` per axis; 2 →
`vel·64/3` of the target (`+0x7dc/+0x7e0`); else both added. Position = followed camera
+ `vel·0x800 + random3(range << 13) − range·0x1000`, clamped to `[(−range) >> 2,
range >> 2]` **in 16.16 units** (as compiled, so in effect it re-spawns at the
followed camera), and the yaw snaps to the target. The yaw and pitch globals are
restored after the frame. The water row formula clamps the negative pitch offset to 0.
Port: `camera.rs` `tv_camera`, random numbers from the cosmetic generator
(`VisualRng`, `random3`).

**Water in the rotating view** (software, `FUN_01081ba4` / `FUN_0108a2a0`): the
background is still read in screen space and does not turn with the view, but row
`sy / 2 + (pitch − 0x157c)·cy / 0x103` of the background is read instead of row `sy`
(cy = `DAT_0114c334`, 127): the lower half of the picture looking straight down, the
upper half at full tilt. (The Warp3D path rotates the reflection UVs with the yaw; the
port follows the software renderer.)

**Split-screen screen constants** (`FUN_0105bd98` at `0105c4a0`): two players (side
by side, 159×256): cx 80, cy 128, fx = (W/2+7)·16/240 = 11, fy = (H+8)·16/384 = 11;
three or four (159×127): cx 80, cy 64, fx = fy = 8. Vertical FOV `2·atan(cy/(16·fy))`:
72.1° for two players, 53.1° for three or four (rotating: 111.0° / 90°).

**Per view** (`FUN_0105bd98` loop): `FUN_010556e0(p)` (camera), `FUN_01036574` and
`FUN_010753e8` (dynamic lights), `FUN_01049374` (city, objects, effects, with this
view's camera; its 8 chimney random numbers (the Mac's per-chimney `random3`
draws, §10) and the alarm-box pulse `DAT_012e5adc += ticks` run once per view), `FUN_01034804` (this view's lens flares,
collected during its city pass; the list is cleared afterwards), then the HUD.

---

## 7. Effect textures (`EffectTextures/*`, 52 files, all 64×64 8-bit) — CONFIRMED unless noted

Names ending in two binary digits `xy` are quarters of a 128×128 sprite (`00` TL,
`10` TR, `01` BL, `11` BR); the leading digits are the set (Shockwave.1xx…4xx =
growth stages, LensFlare.2xx glow, 11xx/12xx rings). `3_4`, `6_7` are shared by two
flare elements.

**Effect records** (0x2c bytes, `DAT_0118ff2c`, 4 lists × 5000; spawn
`FUN_01035da0(type, tex, x, y, z, vx, vy, vz, size, list, blend)`): position in object
units, velocity per tick (given/3), half-size r (also the lifetime). Each tick
`pos += v·ticks`; types 2/0xB/0xE get gravity `vz += 17·ticks`; removed when r < 1
or z > 24000. Drawn by `FUN_010386e8` after layer `list` as **screen-aligned squares
of world half-size r, perspective-scaled**:

```
Dz = z + camH*16 − 256 (skip if ≤ 256)
sx = cx + fx*((x±r)*512 − camX*16)/(16*Dz)        (y likewise)
```

Blend = Alpha.256 table chosen by the record's blend offset (§3); texel 0 transparent
for all tables except 4 and 10. Single sprites are mirrored left-right
(`FUN_01033d50(tex,0,…)`). Flame.1–4 cycle one step per *frame*.

| type | dr/tick | use |
|---|---|---|
| 1 | −20 | flame/smoke puff; wheel dust (Dirt T3 or Dust T0) |
| 2 | −3, gravity | burning fireball (Flame, T0), spawns type 4 |
| 3 | −57 | dust cloud (Dust, T0, r 1500–2500) |
| 4 | −23 | flame; emits dark smoke while r 100–250 |
| 6 | −13 | smoke: Dust with T1 subtract (chimney scenery 0x11, explosions) |
| 7 | −66 | explosion fireball (Flame, r ≈ 2500+) |
| 8 | −1 | long-lived ground marks: **skidmarks**, scorch, blood splats, settled snow |
| 9 | −7 | ricochet sparks (Flare T0), blood spray (Blood T3), Gas |
| 10 | +300 | **shockwave**: starts r 300, set = r/2500 (Shockwave.1→4), deleted at r ≥ 10000 (~33 ticks), 4 quarters, T0 |
| 0xB | −5, gravity | explosion lens flare |
| 0xC/0xD/0xE | — | snow / rain / rain splash (§4) |
| 0xF | −2 | fire with looping Flames sound |

* **Skidmarks** (`FUN_010cc288` → `FUN_010cc010`): no decal buffer — a chain of type-8
  sprites (r = 230 cars / 100 bikes, spacing 7/8·r) per wheel from the previous to the
  current wheel position at the vehicle's z, shrinking 1/tick (~230 ticks). Texture by
  surface flag: 34/36 `DirtSkidmark` T3; 50–52 `Dust` T0 (light); else `Dust` T1
  (dark rubber). Intensity = 2·(|Δv|x+|Δv|y)/ticks, 0 breaks the trail.
* **Shadows** (`FUN_010c9f28`): people/small objects use `Shad`, drawn as a
  screen-aligned square with T1 (subtract), world half-size 1.5 × model[+0x48c4];
  vehicles use the *object* texture `shadow` on a 3×3 vertex mesh following ground
  heights, T1. All shadow points are offset by `(K − z_obj + z_ground)/4` in **+x and
  +y** (K = 0x200 cars, 0x80 people) → shadows fall toward the bottom-right (south-east)
  and grow with height.
* **Lens flares** (`AddFlare` `FUN_0103451c`, max 199/frame; drawn by
  `FUN_01034804` at **fixed screen size**): sources are model faces with mode 6
  (traffic lights, crossing beacons, alarm boxes, police lights, muzzle flashes,
  missiles — list in §10; size 300 → s = 18; table from face byte +0x152f, 3 → 0) and
  explosions (type 0xB, s = r/16, never occluded: AddFlare gets −size). Software
  occlusion: a marker pixel is written at the flare centre and the flare is dropped if
  it was overdrawn by the world. Option `DAT_0114c428` (`FUN_01035960`): 0 = no flares
  at all (AddFlare returns), 1 = glow only, 2 = everything (default). Main glow
  LensFlare.2xx (4 quarters) at the source, half-size (2/3)·s·fx/16; with option 2,
  ghosts on the line through the screen centre at `centre + f·d` (d = source −
  centre): f = 1.5 (.1, s/6), 0.75 (3_4, s/12), 2/3 (3_4, s/7), 0.5 (.5, s/9), 1/3
  (6_7, s/40), 0.1 (6_7, s/30), −0.1 (.8, s/4), −1/6 (.9, s/9), −1/3 (.10, s/6), −2/3
  (11xx quarters, s/2), −1 (12xx quarters, s); singles are not mirrored. Every element
  goes through the flare's table: 5–8 add only R / R+G / G / B, so traffic-light
  flares are red, amber and green. No sun flare.
* **Blood** → `Blood.dark` at night (`FUN_01035250`; port: `effects.rs` tile 47; the Mac has no `Blood.dark` and keeps its own `Blood` at night, see `mac_version.md`). `Bubble` is loaded but unused. FadeDarken*,
  HealthBar are HUD textures.

Lifetimes are in game ticks (`DAT_0114c6a0` = ticks elapsed this frame; a tick is
1/25 s on 68k (2 DateStamp ticks) and 1/30 s on OS4 and the Mac (the port's rate), CONFIRMED in `people.md` §1.1).

---

## 8. `.CubeTextureList` flags — CONFIRMED

Single reader `FUN_0106dfcc(tex)` → `*(int*)(tex+0x1040)` (4 if no texture), queried
for the top texture of the cube on the object's current layer (off-map = 4). Callers:
human movement (footsteps, in `FUN_0105f594`) and vehicle physics `FUN_010cc288`. The
value **is the footstep sound index** (`FUN_0100815c(flag, …)`; sound table
`FUN_01005864`) and selects vehicle handling:

| flag | textures | footstep | vehicle |
|---|---|---|---|
| 4 | pavement, road, buildings (665) | Footstep.stone | grip = base·4/5; dark rubber skids |
| 34 | grass, mud, path (70) | Footstep.grass | grip = base/2; speed > 450 loses 3/tick; DirtSkidmark + Dirt spray |
| 35 | fences, mesh, corrugated iron, `$Bars` (6) | Footstep.metal | as 4 |
| 36 | sand (9) | Footstep.sand | grip = base·2; speed > 350 loses 20/tick; dirt skids |
| 50 | snow (25) | (sand) + dust puff | grip fixed 4; speed > 200 loses 10/tick; Dust skids |
| 51 | slush roads (65) | (sand) + dust puff | grip = base·10/25; Dust skids |
| 52 | Ice, Ice2 | (sand) | grip = 1; Dust skids |
| 65 | `#Water`, Pond | Footstep.water | default |

"Grip" = per-tick limit on how fast the velocity converges to speed·(sin, cos)
(base = model+0x48e8). The flags are not used for collision, drivability or drawing
(water detection elsewhere uses face type 4). The leading `#`/`$` of a *file name*
(not the flag) marks animated water / night-remap exemption (§3, §4).

---

## 9. Checklist for `crates/payback/src/city.rs` (differences found)

1. Stack by `h1` for every cube, never `max(h1,h2)`.
2. Reflect flat tops sit at `base + h2`.
3. Sides: rectangle `base … base+h1` (clip to the slope edge, never above h1).
4. UVs per §2: flat top u mirrored (u=63 at west) unless flip; Vert/RVert v runs
   south→north; Horiz/RHoriz texture rotated (u along N→S, v along W→E); sides
   u = 63 at the x0/y0 end, v = 0 at the top; flip mirrors u only.
5. Alpha = 50 % blend (T4), texel 0 not transparent; Genlocked = texel 0 cut-out.
6. Water = screen-space sky (Clouds/Stars) displaced by the scrolling `#Water` texel
   values; no geometry reflection.
7. Camera: straight down, HFOV 64° at 4:5 aspect, height B + |c·speed| above the
   followed object, 1/5 positional and 1/3 height smoothing per tick.

Status (port): 1–7 are implemented (`world.rs`, `city.rs`, `water.rs`, `camera.rs`).
Also done: weather and the palette flash including explosions (§4), lens flares with
ghosts (§7), Gouraud lighting with the static lamps and the dynamic lights
(`lighting.rs`, §4), the night headlight polygon, Blood.dark, mode 3/4/5 object faces,
vehicle colour textures and the object details of §10. Still missing: the rotating
view.

---

## 10. Object rendering details (models, scenery, damage) — CONFIRMED unless noted

### Face slots and modes (loader `FUN_010bf8bc`, renderer `FUN_010c101c`)

Each `.Obj` face has two 9-byte slots at model `+0x1528 + face·0x12` (u0 v0 u1 v1 u2 v2,
mode, param, active; v values are halved and clamped to 2…125 at load, u stored raw).
The renderer picks **slot 0 or 1 by the sign of the projected winding** (the two slots
are the two sides of the face) and draws it only if its `active` byte is 1 — faces with
one active slot are single-sided, faces with none are invisible. Low nibble of `mode`:

| mode & 0xf | drawing |
|---|---|
| 1 | flat colour: `param` mapped through `General/64->256.dat` (64 palette indices, file name `64-_256.dat` on disk) at load |
| 2 | textured |
| 3 | textured, `FUN_0111e410` (CONFIRMED: texel 0 transparent — trees, radar dish, police light bars) |
| 4 | textured through the Alpha.256 table at `+0xff4` with `FUN_011113e0`: `DAT_0114c908` = **table 0, additive** for the world objects (`FUN_010c101c(obj, x, y, z, 0)`), table 4 (average) when the 5th argument is 100. Shine faces (`& 0x10`) with mode 4 use table 4, mode 0x28 table 9. Users: `Lights` (lamppost_dark light cones, the night headlight polygon), helicopter `Blades`, rocket-car `Flames`, `Phone`/`Red shade` arrows |
| 5 | a line between the first two vertices, colour `param` mapped through 64→256 like mode 1 (the loader maps `param` for every mode except 6, `FUN_010bf8bc`); bullets, pistol |
| 6 | **lens flare**: `AddFlare(vertex 0, DAT_0114c8dc, param << 16)`; tested on slot 0's mode even when slot 0 is inactive (most flare faces have no active slot, so only the flare shows) |

`mode & 0x10` selects the environment-mapped (`Shine`) path; `& 0x20` its variant.
`param` is the raw Alpha.256 table for mode 6, otherwise the 64→256 colour.
Without `& 0x10`, `& 0x20` sends the face down the mode-4 branch (CONFIRMED,
`FUN_010c101c`): mode 0x22, the four "White sand" leg faces of
PedMale1–4 (slot 1 active, texture "White sand", param 3), is drawn additively (the
low bits are not 1/2/3, so the software path falls into the mode-4 branch; Warp3D also
blends it, §11.6; port: the additive overlay). Bit 0x20 alone does nothing else: it
does not set the shine flag `+0x4906`. Mode 0x14 (`CD.obj`) is
environment-mapped through table 4 (50 %); the port still draws it opaque (remaining
gap). Mode 3: texel 0 transparent is CONFIRMED (`FUN_0111e410` jumps into an unrolled
span ending at 0x01128ae8; every pixel does `lbz; cmpwi 0; beq skip; stb`).

**Face depth sorting** (CONFIRMED, `FUN_010bf8bc`, `FUN_010c101c`, `FUN_010c4fec`,
`FUN_010c7218`): the `.Obj` header flag sets model `+0x490c = (flag == 1)`, which turns
on painter's sorting of the model's faces: moving objects insertion-sort by the sum of
the three vertex depths (re-sorted when roll/pitch changes), scenery sorts once by the
deepest vertex, the tumbling info sign every frame. Models with flag 0 draw their faces
in file order. Details and counts: `formats.md` Objects.

Mode-6 faces in the shipped models: `TrafficLight1` (table 5 = red), `TrafficLight2`
(5 red + 6 red+green = amber), `TrafficLight3` (7 green), `TrafficLight4` (6 amber),
`PedestrianCrossing_flash` (6), `AlarmBox_flash` (8 blue), `Police_red`/`PoliceVan_red`
(5), `Police_blue`/`PoliceVan_blue` (8), the `*_Fire` people poses (6, muzzle flash),
`Missile` (3 → drawn as 0), `RocketCar_Thrust1/2` (0), `Torch` (options screen).
`DAT_0114c8dc` is 300 (radius `300 >> 4` = 18 px) except while an alarm box is drawn.
Flares are only added for objects that are not burning (`obj+0x33c < 1`).

### Burnt and dented objects

* **Burnt** (`obj+0x33c` burn timer ≥ 1 — set to 100 by Ignite and left at 15 when the
  fire goes out, so a wreck stays burnt): every face's visible slot is drawn with the
  object texture `burnt` (`DAT_0114c91c`): mode 1 at the fixed UVs (2,2) (2,125)
  (125,125), mode 2 at its own UVs; faces of any other mode are not drawn at all.
* **Dented** (`obj+0x452 = 1`, set with the `Thud2` sound by wall hits ≥ 11 damage
  `FUN_010cd198` and object hits, never cleared): when the vertices are transformed each
  vertex n (0-based) is displaced by `FUN_0103ad20(3(n+1)+a, 160) − 80` on x, y, z
  (a = 0, 1, 2), i.e. `RandomNumbers[3(n+1)+a]·160/512 − 80` in ×16 model units
  (±5 model units). The table lookup is fixed, so the dent is a static deformation
  (same shape every frame and for every car of that model); environment-mapped faces
  then use a second set of normals (`+0x3c70` …).

### Scenery that changes (`FUN_01049374` scenery pass; 68k `FUN_000d1058`)

* **Traffic lights** (scenery 1): `k = special + DAT_0114c750 − 1`, `k −= 6` if > 5,
  where `DAT_0114c750` is the global light phase 0…5 (68k `DAT_0034c084`, +1 every 61
  ticks). Model: k < 2 `trafficlight1` (red), k = 2 `trafficlight2` (red+amber), k 3–4
  `trafficlight3` (green), k = 5 `trafficlight4` (amber). The loader stores the four
  models at `DAT_012f5ae0…aec`; scenery slot 1 points at `trafficlight1`. The same
  states (k ∈ {0,1,2,5}) stop traffic.
* **Pedestrian crossing** (15): `PedestrianCrossing` while the phase sub-counter
  (`DAT_0114c754`, 68k `DAT_0034c098`, 0…60) is < 25, else `PedestrianCrossing_flash`
  (amber flare).
* **Alarm box flashing** (22): `AlarmBox_flash` with flare size
  `|t − 10|·50 + 200` (200…700), t = `DAT_012e5adc` cycling 0…19 by ticks.
* **Spinning pickups**: models whose `+0x4908` code is ≥ 0 — bazooka 1, flamethrower 2,
  grenade 3, minigun 4, shotgun 5, pistol 6, bigbomb 7, radar 100, info 'P', key 'Q' —
  are drawn at heading `rotation·22.5 + DAT_0114c770`, a global angle advancing 60 per
  tick (one turn per 96 ticks; 68k `DAT_0034c09c`). The info sign ('P') goes through
  `FUN_010c7218` with two more angles (68k `DAT_0034c0a4` +30/tick, `DAT_0034c0a0`
  −45/tick), so it also tumbles. Axis order CONFIRMED (`FUN_010c7218`, call site
  0x0104cec4): rotate in the X–Z plane by `DAT_0114c76c`, then in the Y–Z plane by
  `DAT_0114c768`, then yaw by `rotation·22.5 + DAT_0114c770`. Per tick (0x0105fe08):
  c770 += 60, c76c += 30, c768 −= 45, each wrapping at 5760; initial values
  (0x01069954) c76c = 20, c768 = 5, c770 = 0. Snowman (0xfe) and the rest stay still.
* **Chimney** (17): while drawn, each frame with `rand(3) == 1` (one set of random
  numbers per frame for all chimneys) it adds per tick a type 6 `Dust2` smoke puff
  (subtractive) at the square centre + (rand(100)−50 + 100·i, rand(100)−50), z = its
  base z − 2500 + rand(300), v = (150+rand(30), rand(30)−15, rand(30)−250), size
  300+rand(200), in the list of the layer being drawn. The Mac (`DrawMap` @ 1005b0b8,
  CONFIRMED) draws `random3(3) == 1` and fresh numbers per chimney per tick (size, vz,
  vy, vx, z, y, x), so chimneys smoke independently; the OS4 drew the set from the game
  generator and restored its seed after the frame. Port (`objects.rs`
  `chimney_smoke`): the Mac's, from `sim::VisualRng`.

Port: `crates/payback/src/objects.rs` (slots/modes 1, 6 and inactive faces, burnt and
dented mesh variants, scenery animation, chimneys, flare sources); flares are drawn by
`effects.rs` (§7). Mode 3 uses the colour-0-transparent atlas half (alpha mask);
plain mode 4 faces, and the night headlight polygon, go into an additive overlay mesh
and mode 5 faces into a line-list overlay (1-pixel lines), drawn as child entities of
the object (`sync_overlays`); the main mesh keeps a degenerate triangle in their place.
Additive blending happens in linear colour rather than palette space.

### Mac models, face modes, doors and shadows (Mac `DrawObject` @ 10075880) — CONFIRMED unless noted

* **Mac models** (docs/re/mac_version.md decision 4). `DrawObject` places every model at
  its origin with no per-model offset (people: the same `Y + 0x5dc` when pitch is 0,
  depth `>> 10`, horizontal `/3` as OS4 `FUN_010c101c`). The Mac changed the data
  instead: people re-based by −max Y (MaleNormal 90, PedMale4 100, the rest 92) so the
  feet are at Y = 0; Pug, Pug Racer, Taxi and Vapour raised so the body bottom is at
  z = 0; `Doors/Bus` raised; shorter trees, phones and traffic lights; `Tree.Obj` the
  `tanne` cross-billboard; new menu props. Port (`objects.rs` `ObjectLibrary::load`):
  game models from the Mac data when present; scenery per `Settings::variants.scenery`
  (at the next level load), menu props per `variants.menu_props`. The simulation reads
  model extents (radius `+0x48c4`, front `+0x48c8`, back `+0x48cc`, ground mean: door
  points, contact circles, AI), so `ModelInfo` always comes from the Amiga file and the
  replays do not depend on the model set; the drawn model's own extents (`shape`) place
  the headlight polygon, door and shadows, as the Mac computes them from its models.
  Amiga people are drawn shifted by the Mac's per-model amounts, also before the lying
  +1500 (`sim/people.rs` `standing`/`lying`). Mac-only object textures (PC, Keyboard,
  Monitor, Disk, Moviereel, `tanne`) join the atlas.
* **Face modes.** Slot 0's mode byte decides before the winding test, whatever its
  `active` byte: 6 = lens flare only, 5 = a line (colour `param`; colour 0, black, in the
  burnt pass `DrawLine(…, 0)`); neither draws a triangle. Shine sub-modes (mode − 0x10
  or − 0x30): 1 file UV + CloudsReflect table 9, 2 opaque env map, 4 env map through
  table 4 in `DrawObject` but table 0 in `DrawScenery` @ 10079e90, anything else
  (0x10, 0x13, 0x15, 0x30 …) `Genlock` (texel 0 transparent) in software and `(7, 8)`
  in hardware. Hardware mode 3 is `(7, 8)` with the filtered A1R5G5B5 key (soft edges);
  port: a `keyed` overlay material, mask in software, alpha blend in hardware. No
  shipped model uses the Genlock shines or 0x34 (CD.obj's 0x14 is a menu prop).
* **Sand.** A person whose instance `+0x99c` (OS4 `+0x4b4`, the player number) is
  n ≥ 1 draws both `Red sand` and `Red dark sand` faces with the *light* sand n
  (`InitObjects` @ 10072d50: Red, Green, Yellow, Blue). Port: `build_person_mesh`.
* **Doors.** While `+0x418` ≠ 0 the tail of `DrawObject` creates the door model
  (`+0x8158`) at the hinge (`+0x8150` = −max|X|, `+0x8154` = door position) with heading
  + `+0x418`, the vehicle's depth, pitch and roll, paint (`+0x8a4/8a8`) and burn timer
  (`+0x81c`), not its dents. No offset: the Mac's raised cars keep the ±18 default
  door. Port: `objects.rs` `vehicle_doors` (child entity).
* **Shadows** (`DrawShadow` @ 1007f7d0, `DrawSceneryShadow` @ 1007ea20, called from the
  `DrawMap` square loops after each layer's effects; object shadows once per frame,
  flag `+0x816`): details and the port in `objects/shadows.rs`. Scenery shadows are
  skipped at night (`GetObjectNight`); object shadows are not. Objects in state 6 have
  none. Port approximations: depth-tested horizontal triangles 1 h-unit above the ground,
  linear-colour subtraction, no spinning-pickup shadows, the face texture's own key for
  the hardware `Tree2.clone`/`tanne.clone` (INFERRED same mask).
* Not done: debris (`DrawPart` @ 100794c0, the part list of `FUN_01035438`).

---

## 11. Hardware (Warp3D) renderer

The launcher (PaybackSetup, config field 19) starts the OS4 build with
`DAT_0114c7d8 != 0`: every renderer function then takes its Warp3D branch. Sources:
`analysis/ghidra_os4/decompiled.c`; the scratch extracts in `analysis/scratch/hw/`
(city, lighting) and `analysis/scratch/hw2/` (effects, objects, front end).

### 11.1 The Warp3D layer (`API_3D_*`, 0x01001eb8–0x01005000)

* `FUN_01001eb8` opens `Warp3D.library` v2 and its `main` interface (`DAT_0114c3dc`);
  `W3D_CheckDriver()` (+0x60) must report `W3D_DRIVER_3DHW` (bit 2), otherwise
  hardware rendering is off. Failures elsewhere ("Unable to load textures to 3D card",
  "Can't lock hardware", `FUN_01041864`) also fall back to software. CONFIRMED.
* Interface offsets used (CONFIRMED from the call shapes, names INFERRED from the
  Warp3D function order): +0x50 `CreateContext`, +0x54 `DestroyContext`, +0x5c
  `SetState(ctx, state, action)`, +0x60 `CheckDriver`, +0x64 `LockHardware`, +0x68
  `UnLockHardware`, +0x80 `AllocTexObj`, +0x84 `FreeTexObj`, +0x90 `SetFilter`, +0x9c
  `UpdateTexImage`, +0xa4 `DrawLine`, +0xa8 `DrawPoint`, +0xac `DrawTriangle`, +0xbc
  `SetBlendMode`, +0xc0 `SetDrawRegion`, +0x130 `SetCurrentColor` (W3D_Color, 4 floats).
  `SetTexEnv`, `SetFogParams`, `SetZCompareMode`/`AllocZBuffer` are never called.
* State bits (INFERRED from Warp3D.h; action 1 = enable, 2 = disable, deduced from the
  screen clear in `FUN_0101a7dc`, which disables 0x100 around a black flat quad):
  0x100 TEXMAPPING, 0x200 PERSPECTIVE, 0x400 GOURAUD, 0x2000 BLENDING, 0x4000 FOGGING,
  0x40000/0x80000 anti-aliasing/dithering. `API_3D_Init` (`FUN_01002024`) sets
  0x80000 1, 0x400 2, 0x100 1, 0x200 2, 0x40000 1, 0x4000 2, 0x2000 2, then
  `SetDrawRegion` and `SetBlendMode(2, 2)`. The renderer toggles GOURAUD, PERSPECTIVE,
  TEXMAPPING and BLENDING per primitive, caching them in `DAT_0114c3d8` / `3d6` / `3d4`.
  **No Z buffer** (state 0x800 is never touched; painter's order as in software), **no
  fog**, no gamma ramp.
* Blend modes: only `(2, 2)` = ONE/ONE (additive, the default) and `(7, 8)` =
  SRC_ALPHA/ONE_MINUS_SRC_ALPHA (set around alpha primitives, then restored).
  CONFIRMED (72 call sites).
* Triangles: `FUN_0100499c` (perspective), `FUN_01004b84` (affine), `FUN_01004d6c`
  (simple) clip to the screen and call `DrawTriangle`. W3D_Vertex is 0x40 bytes,
  x, y, z, w, u, v …, colour (r, g, b, a floats) at +0x20. **Perspective correction
  only for the cube side walls** (`FUN_010a4004` and the other wall functions) and the
  rotating-view radar; tops, objects and effects are affine. CONFIRMED.
* Textures (`FUN_010024fc`, cached by `FUN_01004398`, at most 3000): sizes are rounded
  up to powers of two ≤ 256 and resampled **nearest**; every texture gets
  `SetFilter(2, 2)` = LINEAR/LINEAR, i.e. **bilinear, no mipmaps**. The texture
  environment is never set (MODULATE, INFERRED: the Gouraud values below only make
  sense multiplied). Conversion flags (setters `FUN_01002478…010024f0`):

  | flag (setter) | format |
  |---|---|
  | none, power-of-two size | **A1R5G5B5**, palette RGB, index 0 transparent |
  | `c3be` (`FUN_010024cc`) | the same, forced (icons, HUD) |
  | none, other sizes / `c3c4` (`FUN_010024a8`) | ARGB32, alpha = max(R, G, B) |
  | `c3cc` (`FUN_01002478`) | black, alpha = max(R, G, B) (smoke `Dust`, shadows) |
  | `c3c8` (`FUN_01002490`) | each non-zero channel → 255, alpha = max (`Blood`) |
  | `c3c6` (`FUN_0100249c`) | pure green, alpha = max (gas) |
  | `c3c2` (`FUN_010024b4`) | inverted RGB, alpha = max / 2 (front end) |
  | `c3c0` (`FUN_010024c0`) | black, alpha 0x32, index 0 → 0 (tree shadows) |
  | `c3bc` (`FUN_010024d8`) | RGB, alpha 0x7f (cube textures `Ice2`, `RoofWindow`) |
  | `c3ba` (`FUN_010024e4`) | 24-bit RGB source (`.chunky2`, `TitleScreen.w3d`) |
  | `c3b8` (`FUN_010024f0`) | RGB − 64 per channel (night objects, `FUN_01003578`) |

  So the hardware look is **true colour, bilinear, 15-bit textures**: no palette, no
  `Alpha.256` / `Gouraud.256` tables, no final palette snap.

### 11.2 Options

* **Gouraud forced on**: LoadConfig `FUN_0104199c` sets `DAT_0114c2fc = 1` when
  `DAT_0114c7d8 != 0` (config field 19 read right after field 17, `DAT_0114c2fc`).
  CONFIRMED.
* **Motion blur refused**: the pause menu (`FUN_0101a7dc`, case 3) does not toggle it
  under Warp3D; `FUN_01042f00` only blends the two chunky buffers in software (the
  buffers are not even allocated, `FUN_0104feb0`). CONFIRMED.

### 11.3 City lighting (`FUN_010766c8`, `FUN_010920d8`)

* Base light (CONFIRMED, §4 table): day lit 0xbe, shadow 0x80, slope step 0x30; night
  lit R, G 0x20 / B 0x30, shadow R, G 0x0c / B 0x10, step 0x20. Lit Reflect tops 0xbe
  on all channels. The `.CubeTextureRemapList`, the voxel shadow test, the lamps
  (`FUN_0106c904`) and the dynamic lights are the same as in software, except that
  **the dynamic lights are not halved by day** (`FUN_0106ba08` halves only when
  `DAT_0114c84c == 0 && DAT_0114c7d8 == 0`). CONFIRMED.
* **Jitter** (hardware only, after the slopes, CONFIRMED): for every cube slot of
  squares x, y = 1…126, `d = FUN_0103ac94(16) − 8` is added to all three channels of
  top corner 0 of (x, y), 1 of (x−1, y), 2 of (x, y−1) and 3 of (x−1, y−1) — the four
  corners meeting at grid point (x, y), so the jitter is seamless — and a second `d`
  likewise to the bottom corners 4…7; negative results become 0. `FUN_0103ac94` is the
  shared game generator, so a hardware map load consumes 2·126·126·3 extra random
  numbers before play.
* Drawing (CONFIRMED, `FUN_010920d8` ~0x01092a00, walls `FUN_010a4004` etc.): vertex
  colour = `min(L, 255) · (1/256)` per channel (`DAT_0113457c` = 0.00390625), alpha
  1; the texture is modulated by it, so 0xbe ≈ 0.74 of the texture by day, the shadow
  0.5, and night 0.125 (lamps and lights brighten towards the plain texture). Each top
  is two triangles, (NE, NW, SW) and (NE, SW, SE); colours are interpolated linearly
  per triangle (no `& 0xF8` steps).
* Face types: Normal opaque (the A1R5G5B5 colour key is not tested); Genlocked and
  Alpha with BLENDING on and `(7, 8)` (genlocked: the filtered colour-key alpha; Alpha
  tops: `Ice2` / `RoofWindow` carry alpha 0x7f, a 50 % blend). No cube face is
  additive. CONFIRMED.
* **Night**: `FUN_0106ecf0` still darkens the chunky cube textures in place, but the
  card's copies are not reloaded, so on the card the cube textures stay undarkened and
  the night is only the low vertex colours (INFERRED from the upload order: cube
  textures go to the card at load, `FUN_0106b7a0` / `FUN_01071dac`). Objects: the
  darkened ones are re-uploaded with `c3b8` (RGB − 64), the `.dark` swaps as in
  software (`FUN_010bf310`). CONFIRMED.

### 11.4 Water (Reflect tops, non-rotating view; `FUN_010920d8` ~line 3020)

* Texture: the background `Clouds.chunky2` / `Stars.chunky2` (`FUN_0106e564` opens
  the name with the last character patched to '2'): 320×256 24-bit RGB, converted to a
  256×256 texture (nearest), bilinear. **No sky or background quad is drawn.**
* Per vertex (CONFIRMED; constants `DAT_01134568` = 8.0, `DAT_0113456c` = 128.0,
  `DAT_01134570` = 1/16384):

  ```
  k = sin[i]                                   # 5760-entry table, amplitude 256
  i = ((g·16 + scroll)·90) mod 5759            # #Water (scroll = DAT_0114c87c)
  i = (g·1440) mod 5759                        # other Reflect textures
  g = the corner's grid x + y
  u = 128 + 8·((cos·dx + sin·dy) >> 8)/fx + 255·k/16384
  v = 128 + 8·((cos·dy − sin·dx) >> 8)/fy + 255·k/16384
  colour = clamp(L + (k >> 3), 0, 255)/256     # L = the corner's Gouraud light
  ```

  dx, dy = the vertex's screen position minus the centre (`c330`, `c32c`), fx, fy as
  in §6; cos/sin are fixed table entries ≈ (256, 0) here (the rotating view uses the
  yaw, §6). So the reflection is the true-colour sky magnified about 2× (half the
  picture across the screen), gently wobbling, and lit like the city. Nothing else
  draws the background (INFERRED: no other `.chunky2` texture use was found).

### 11.5 Effects, flares, weather (`FUN_01033d50`, `FUN_01034804`, `FUN_010360fc`)

* Alpha.256 tables become blend modes (CONFIRMED): tables 0, 2, 4, 9, 10 → ONE/ONE
  with a white current colour (the 50 % "average" sprites are additive too); tables 1
  and 3 → `(7, 8)` with the converted textures (smoke / shadow black with alpha =
  max(rgb), blood saturated, gas green, dirt RGB with alpha = max); tables 5–8 → ONE/ONE
  with a red / (1, 1, 0) / green / blue current colour. Snow: additive `Dust`.
* Rain: `DrawLine` streaks and `DrawPoint` splashes, colour (1, 1, 1, 0.4) under
  `(7, 8)`. Palette flash (`FUN_0103ad6c`): a full-screen ONE/ONE quad of colour
  `flash/255` after the HUD. Fades: a black quad with alpha.
* **Lens flares are never occlusion-tested** (the software overdraw test reads the
  chunky screen). CONFIRMED.
* Shadows: people the black `Shad` sprite (alpha = max); vehicles 8 triangles of the
  `shadow` page, black, alpha = max, `(7, 8)`; model shadows black at alpha 0.3, tree
  faces through `Tree2.clone` / `tanne.clone` (`c3c0`, alpha 0x32, index 0
  transparent). CONFIRMED.

### 11.6 Objects (`FUN_010c101c`)

Never Gouraud-lit (GOURAUD off). Mode 1 flat colour (current colour), 2 opaque, 3
colour-keyed, 4 and the other non-0x10 modes ONE/ONE (headlight beams, lamp cones),
5 lines. Shine faces: 0x12 opaque, 0x14 / 0x28 ONE/ONE, 0x11 the opaque face plus
`CloudsReflect` ONE/ONE (software: table 9, `d + s − 32`), the others `(7, 8)`.
CONFIRMED.

### 11.7 Front end

`Icons/TitleScreen.w3d` is a raw 256×256 24-bit RGB picture (196 608 bytes), loaded
with `c3ba` by `FUN_010138dc` and drawn full-bright, stretched over the screen;
menus draw over a black clear, fades are alpha quads (`FUN_0101a7dc`). The port's
front end is `crates/payback/src/frontend/` (display options agent).

### 11.8 Port (`crates/payback/src/render_mode.rs`)

`RenderMode { Software, Hardware }` follows `Settings::hardware` (`Saves/Payback.cfg`
`hardware = 0|1`); `--hardware` / `--software` override. Switching at runtime relights
the city and rebuilds the chunks (like the Gouraud toggle), swaps the atlases and
materials. Implemented: Gouraud forced on; hardware base light, jitter and undimmed
day lights (`lighting.rs`); modulated, per-triangle Gouraud, A1R5G5B5 bilinear texels
repeated in their tile, undarkened night cube textures, blended genlocked faces
(`shaders/city.wgsl`); the water formulas above (`water.rs`, `shaders/water.wgsl`);
effect blend modes, converted effect atlas, unoccluded flares (`effects.rs`);
bilinear 15-bit object textures, night −64, additive CloudsReflect (`objects.rs`);
no palette snap and no motion blur (`postfx.rs`, the flash stays additive).
Approximate or not done:

* the jitter uses a local generator, not the game's, so replays stay valid across
  render modes (the original's hardware load shifts the game RNG);
* all blending runs in linear light on an sRGB target (Warp3D blends the 16-bit
  framebuffer values); the framebuffer is not quantised to 15/16 bits;
* effect and object atlases filter bilinearly across neighbouring atlas tiles at
  sprite edges (rain is blended `(7, 8)` at alpha 0.4, as the original);
* object shine modes 0x14 / 0x28 additive and the `(7, 8)` shine modes, the shadow
  textures (`Tree2.clone`, `shadow` page), fades: still drawn as in software;
* the rotating-view hardware water turns the UVs with the yaw (sign INFERRED); wall
  triangles split along the same diagonal as tops (INFERRED).
