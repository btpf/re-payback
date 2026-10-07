# City rendering (reverse-engineered)

Source of truth: the AmigaOS4 PPC build (`analysis/ghidra_os4/decompiled.c`, addresses
below are OS4 unless marked 68k). The OS4 port contains three renderers sharing one
code path: the original **software** (chunky, Amiga) renderer, a Warp3D **hardware**
path (`DAT_0114c7d8 != 0`) and the **rotating view** (`DAT_0114c7dc != 0`). The
software / non-rotating path is what the Amiga game shows by default and is the
reference here; the other two are mentioned where they clarify intent.

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
  CONFIRMED; software top-down path: the projected trapezoid degenerates/inverts when
  the camera is on the inner side and nothing is drawn, INFERRED).
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
texel), each entry = nearest palette colour (metric `25|ΔR|+36|ΔG|+16|ΔB|`, first
best wins, result 0 replaced by 8). Fitted against the palette (≥ 99 % match):

| k | offset | op (per channel) | used by |
|---|---|---|---|
| 0 | 0x00000 | min(d+s,255) additive | flares, flames, dust |
| 1 | 0x10000 | max(d−s,0) subtract | shadows, smoke, night remap (T1[c][0x67]) |
| 2 | 0x20000 | d−16 where s≠0 | |
| 3 | 0x30000 | s + d·(255−max(s))/256 | blood, dirt |
| 4 | 0x40000 | (d+s)/2 | **cube Alpha faces** |
| 5–8 | | add R / R+G / G / B only | coloured flares |
| 9 | | d+s−32 | |
| 10 | | pal[s] + d − 128 (d = intensity) | |

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
* Warp3D path (for reference): the background is a 320×256 texture sampled at
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
  `Lights` untouched; everything else (except `TankBits`) darkened through T1[c][0x67].
* Vehicles get an extra 4-vertex polygon textured `Lights` (INFERRED headlight beam;
  `FUN_0104e00c` → `FUN_010bf070`).

### Gouraud lighting (option; `DAT_0114c2fc`, "Gouraud shading" menu) — CONFIRMED

Off by default on the Amiga software renderer; when on, `FUN_010766c8` runs after
LoadMap:

1. **`.CubeTextureRemapList`** (one name per line, aligned with `.CubeTextureList`):
   every face's texture index i is replaced by the index of the texture named on line
   i. 295 lines differ and they map *baked-lighting variants back to plain textures*
   (DarkPavement→Pavement, RoadShadow1→Road, AltRoofShad2→NewRoof …): dynamic light
   replaces the painted-in shading.
2. Per cube 24 shorts at cube+0x24: three channel groups (R,G,B INFERRED) × 8:
   `[0..3]` top corners, `[4..7]` bottom corners (shared with the cube above).
   Corner order: **0 = NW (x0,y0), 1 = NE (x1,y0), 2 = SW (x0,y1), 3 = SE (x1,y1)**
   (CONFIRMED from the per-vertex assignments in the lit rotating paths of
   `FUN_010920d8`: vertex (x0,y0) takes `[0],[8],[0x10]`, (x1,y0) `[1],[9],[0x11]`, …).
3. Base light: lit 0x80 (night 0x60), shadow 0x30; sloped tops ± 0x40 (night 0x20)
   depending on h1 vs h2. Shadows: ray from each vertex through a 256×256×10 voxel
   grid (half-square XY resolution, z = H>>7) stepping (−x, −y, +z) — a fixed sun
   toward the north-west, ~55° elevation (INFERRED). Edge squares never shadowed.
4. Static lamps: scenery 33/36 (`lamppost_dark*`, I ≈ 800M/600M/400M R/G/B + random),
   35 (`lamp`, 500M/500M/400M); position = square centre, z = top of stack − 2000
   object units; each vertex adds `min(255, I/d²)` per channel, d in object units,
   radius `(isqrt(maxI)>>12)+1` squares (7 for lampposts). No occlusion test.
   Scenery 2 (`lamppost`, day) emits nothing.
5. Dynamic lights each frame (`FUN_010753e8`/`FUN_0106ba08`, removed again by
   `FUN_010729ec`): traffic lights (red/amber/green by phase), pedestrian-crossing
   beacons, fire/explosions (`FUN_01036574`).
6. Software shading: `out = Gouraud[(R&0xF8)<<15 | (G&0xF8)<<10 | (B&0xF8)<<5 | texel]`
   (`FUN_0112c6ec`); 128 = unchanged. `Gouraud.256` (not shipped, generated):
   32×32×32×256, value = nearest(clamp(pal[c] + (r,g,b) − 128)).

### Header `variant` = weather (`DAT_0114c854`, read via `FUN_0106e478`) — CONFIRMED

| value | maps | effect |
|---|---|---|
| 0 | 0, user maps | none |
| 1 | 1 | **snow**: 250 particles (type 0xC, `Dust`, additive, r 100–150) around the camera; settle as ground marks |
| 2 | 2 (night) | **rain + lightning**, **level 0 only** (`DAT_0114c558 == 0`; level 1 of map 2 is dry): 250 streaks (type 0xD, additive grey 0x67 lines) + splashes; looping `Rain` sound; random lightning (`rnd(1000) < ticks`) flashes the palette (adds to every RGB, decays ×3/4 per tick) with `Lightning` thunder |

All header values verified in the files: `Maps/0.map` 0, `1.map` 1, `2.map` 2 (night 1),
`UserMaps/*.map` 0. Weather runs only in single player (`DAT_0114c7c8 == 0`).

**Level start** (main loop `FUN_010692bc`, ~0x1069390, after the cameras are placed;
random numbers from `FUN_0103ac94`, call order x, y, z, vx, vy, vz, size):

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
  per update, x then y) instead of being deleted as other types are.
* Removal at z > 24000 (H = 0, i.e. *below* the street level of 256; drops fall through
  roofs and streets alike):
  * snow: a **mark** `AddEffect(8, Dust, x, y, 0x57e4, 0,0,0, 200+rand(50), list 0,
    blend 0)` (additive, lives ~225 ticks) and a new flake in list 3 at
    `x = cam.x + rand(0x10000) − 0x8000` (±16 squares), `y = cam.y + rand(0xa000) −
    0x5000` (±10), **z = 0x100 − camH·16** (the camera plane), same velocities/size as
    at level start;
  * rain: a new drop (±8 × ±5 squares, z = 0x100 − camH·16) and **3 splashes**
    `AddEffect(0xE, Dust, x, y, 0x5780, rand(100)−50, rand(100)−50, −200−rand(200), 1,
    list 3)` at the old drop's x, y. These random numbers come from `FUN_0103acc4`.
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
    if counter even: flash += 100 + rand(150)
    counter -= 1                                   # per frame, not per tick
```

So a strike is 1–3 flashes on alternate frames, with thunder at the strike (no delay).

**Palette flash** `DAT_0114c748` (also `+= power/250000` per explosion,
`FUN_0104405c`): the frame renderer `FUN_0105bd98` (end) calls
`FUN_0103ad6c(screen, flash)` while it is > 0, and once with 0 when it has just run out
(`DAT_0114c744` = last frame's value, stored by the main loop). Software: every palette entry becomes `min(255, rgb + flash)` per
channel (from the unflashed copy at `DAT_012a510c/550c/590c`) and is loaded with
`LoadRGB32`; Warp3D: a full-screen quad of colour `flash/255` (clamped to 1) drawn
additively. Nothing is darkened or recoloured per weather apart from this (the night
remap is the header's night flag, not the weather).

Port: `crates/payback/src/weather.rs` (spawn, lightning), `effects.rs` (wrap, respawn,
drawing, `ScreenFlash`; the flash is an additive full-screen quad, i.e. added in linear
light rather than to palette entries). The snow marks are put on the top of cube 0
under the flake instead of at z 22500 (the port depth-tests list 0).

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
ranges to 0…127 and draws nothing outside the map (framebuffer clear colour shows;
INFERRED black). **Visible range** (top-down): half-width `D·cx/(fx<<12) + 1` squares,
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

Focal length 128 px → HFOV 102.7°, VFOV 90° (camera also half as far, so the ground
under the player has the same scale as top-down). Visible range `D·cx/(fx<<10)+1`.

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
* **Blood** → `Blood.dark` at night. `Bubble` is loaded but unused. FadeDarken*,
  HealthBar are HUD textures.

Lifetimes are in game ticks (`DAT_0114c6a0` = ticks elapsed this frame; tick length
not traced here).

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
Also done: weather and the palette flash (§4), lens flares with ghosts (§7), and the
object details of §10. Still missing: Gouraud lighting and the dynamic lights (§4,
option, off by default on the Amiga), the night headlight polygon (`Lights`,
`FUN_010bf070`), Blood.dark, the rotating view, mode 3/4/5 object faces (drawn as
plain textured faces), object textures substituted per vehicle colour (`Cardef.*`), and
the explosion contribution to the flash (`ScreenFlash += power/250000` belongs in the
port's Explosion, `sim/combat.rs`).

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
| 3 | textured, `FUN_0111e410` (INFERRED: texel 0 transparent — trees, radar dish) |
| 4 | textured, `FUN_011113e0` (INFERRED: translucent/additive — `Lights`, blades, flames) |
| 5 | a line between the first two vertices, colour `param` |
| 6 | **lens flare**: `AddFlare(vertex 0, DAT_0114c8dc, param << 16)`; tested on slot 0's mode even when slot 0 is inactive (most flare faces have no active slot, so only the flare shows) |

`mode & 0x10` selects the environment-mapped (`Shine`) path; `& 0x20` its variant.
`param` is the raw Alpha.256 table for mode 6, otherwise the 64→256 colour.

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
  −45/tick), so it also tumbles (axis order INFERRED). Snowman (0xfe) and the rest stay
  still.
* **Chimney** (17): while drawn, each frame with `rand(3) == 1` (one set of random
  numbers per frame for all chimneys) it adds per tick a type 6 `Dust2` smoke puff
  (subtractive) at the square centre + (rand(100)−50 + 100·i, rand(100)−50), z = its
  base z − 2500 + rand(300), v = (150+rand(30), rand(30)−15, rand(30)−250), size
  300+rand(200), in the list of the layer being drawn.

Port: `crates/payback/src/objects.rs` (slots/modes 1, 6 and inactive faces, burnt and
dented mesh variants, scenery animation, chimneys, flare sources); flares are drawn by
`effects.rs` (§7).
