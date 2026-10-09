# Mac release vs Amiga: rendering

Research notes comparing the rendering of the Mac release (`Payback (OS X)`, Carbon/PEF,
decompile `analysis/ghidra_macx/`; OS 9 build `analysis/ghidra_mac9/`) with the Amiga
builds (OS4 `analysis/ghidra_os4/`, 68k `analysis/ghidra_68k/`) and with our port
(`crates/payback/src/`). The Mac release is treated as the more definitive copy
(`docs/re/mac_version.md`).

Tags: **CONFIRMED** = traced in code (Mac and Amiga addresses given), **INFERRED** =
consistent with code/data but not fully traced. Mac addresses are OS X PEF addresses
(code at 0x10000000, data at 0x1017b440). Amiga addresses are OS4 unless marked 68k.

Recommendation keys. The Mac release is treated as definitive, so where it differs from
the Amiga the default is to adopt it:

* **adopt**: change the port to the Mac behaviour.
* **keep**: the port already matches, or Amiga = Mac.
* **ignore**: only for clear Mac platform artefacts (OpenGL/Carbon plumbing, file
  handling, stale build files).
* **option**: only for Amiga-only behaviour worth keeping as a non-default setting.

Every finding is also classified as a **bug fix**, a **Mac change** (deliberate
behaviour or data change) or a **platform artefact**. Mac changes that draw random
numbers are adopted with a render-only generator, so the simulation RNG order (and
replays) stays deterministic.

---

## 0. Summary

The Mac game code is the same C source as the Amiga OS4 build, recompiled with
CodeWarrior. The city renderer, the lighting pre-pass, the projection, the draw order,
the Alpha/Gouraud table maths and the screen constants match the OS4 build almost line
by line. They differ in two places:

* how the Amiga APIs are emulated (chunky buffer → `WritePixelArray8` → QuickDraw /
  DrawSprocket; Warp3D → OpenGL);
* the **data**: retouched cube textures, a new title screen and Blood sprite, a regenerated
  `Alpha.256` and a shipped (stale) `Gouraud.256`.

The main results:

| # | Area | Finding | Mac | Amiga | Port impact | Rec. | Conf. |
|---|---|---|---|---|---|---|---|
| L1 | Lighting | The shadow test for cube corners NE/SW/SE (top and bottom) casts **two rays** and a corner is shadowed only if **both** are blocked; port casts one ray | `DoLightSourcing__FP3Map` 1015b840, `CheckVoxels__Fiii` 100b3960 | `FUN_010766c8` (same paired tests, inlined) | `lighting.rs` `compute` / `Voxels::shadowed` | adopt | CONFIRMED Mac, INFERRED OS4 |
| L2 | Lighting | **Vert (slope 2) bug**: the lighting pass adds h1 twice: corner z NW/SW = base+2·h1, NE/SE = base+h1+h2, and the running base of later cubes in that square is +h1 too high | 1015bd18 | OS4 `FUN_010766c8` ~0x01076cf4 (identical) | `lighting.rs` `compute` (uses correct heights) | adopt (original behaviour) | CONFIRMED both |
| C1 | Camera | **Replay "TV camera"**: during replay playback in the rotating view (single player) the city is drawn from a roaming camera, tilted to pitch 0x14b4, that drifts and re-spawns randomly around the player | `DrawMap__FiiiisPiPiPii` 1005a260 (prologue) | not present in `FUN_01049374` | `camera.rs` (rotating view still missing) | adopt (Mac change) with the rotating view; render-only RNG | CONFIRMED |
| C2 | Effects | Chimney smoke: random numbers from a separate generator (`random3`), drawn per chimney per tick (independent puffs) | `DrawMap` 1005b0b8 | `FUN_01049374`: 8 numbers from the game generator once per view, shared by all chimneys | `objects.rs` `chimney_smoke` | adopt (Mac change), render-only RNG | CONFIRMED |
| B1 | Motion blur | Turning motion blur on clears both render buffers (`BlankChunky`): the first blended frame is averaged with black instead of with a stale frame from the last time blur was on | `ToggleMotionBlur__Fv` 10053210 | `FUN_01042308` (no clear) | `postfx.rs` (blends with the true previous frame) | adopt (bug fix): clear the previous-index buffer when blur is switched on | CONFIRMED |
| H1 | HW mode | OpenGL layer: GOURAUD state = SMOOTH + `GL_MODULATE`, off = FLAT + `GL_REPLACE`; per-vertex colour only sent with GOURAUD on | `W3D_SetState` 101535b0, `W3D_DrawTriangle` 10153b10 | Warp3D driver semantics (INFERRED same) | `shaders/city.wgsl`, `effects.rs` | keep | CONFIRMED |
| H2 | HW mode | Painter's order kept with a per-primitive depth counter (z from −1, +0.0001 per triangle/line/point, `GL_LESS`); > 20000 primitives in a frame would be clipped | `W3D_DrawTriangle`, `W3D_SetDrawRegion` 10153ec0, `W3D_CreateContextTags` 10153f80 | no Z buffer | none | keep / ignore | CONFIRMED |
| H3 | HW mode | Every frame is cleared to black (colour + depth) after the buffer swap; no stale framebuffer off the map edge | `W3D_SetDrawRegion` | Warp3D: `SetDrawRegion` on the other screen buffer and a viewport flip, no clear (`FUN_01042374` ~decompiled.c l.35030) | port clears | keep | CONFIRMED Mac, INFERRED Amiga |
| H4 | HW mode | Real triangle clipper against the view rectangle (u/v, colour, and for perspective w-correct u·w, v·w, w) before drawing, then offset by the view origin | `API_3D_DrawTriangle_Clipped*` 1014ce20…10151ec0 | `FUN_0100499c`/`01004b84`/`01004d6c`: reject-if-below + offset only, Warp3D clips to the screen | port uses viewports | keep | CONFIRMED |
| H5 | HW mode | `GL_POINT_SMOOTH`/`GL_LINE_SMOOTH` (state 0x40000) and `GL_DITHER` are on: rain lines/points anti-aliased | `W3D_SetState`, `API_3D_Init__FP12amiga_Bitmap` 1014ac60 | same states requested of Warp3D | `effects.rs` rain | ignore | CONFIRMED |
| D1 | Data | `Alpha.256` regenerated exactly from the current palette (100 % all 11 tables); Amiga Update 7 file was built from the old CD palette | `CalcAlpha__Fv` 10083ec0, `GetAlpha__Fv` 10084710 | `FUN_0103b540` | `postfx.rs` `alpha_tables`, `night_remap` | adopt (bug fix): compute, or use the Mac file | CONFIRMED |
| D2 | Data | `Gouraud.256` shipped but **stale** (built from the CD palette, 99.78 %) | `GetGouraudTable__Fv` 10084400 | generated `FUN_0103c080` | `postfx.rs` (computes it) | keep; the stale file is a build artefact, ignore it | CONFIRMED |
| D3 | Data | ~170 building-wall textures relit (baked top-centre spotlight), "Light" variants collapsed onto the normal ones, Dark variants darker, new Crowd/Wood/NewWall6/7/Fence/TentRoof pictures; `TentRoof.chunky` 63 rows | `CubeTextures/` | `extracted/game/CubeTextures` | `payback-formats` texture loader, data paths | adopt (Mac change): prefer the Mac textures | CONFIRMED |
| D4 | Data | No `Blood.dark` on the Mac; the darker Mac `Blood` is used day and night | `LoadEffect`/night code | `FUN_01035250` swaps to `Blood.dark` | `effects.rs` (night swap) | adopt (Mac change): no night swap, Mac `Blood` | CONFIRMED |
| D5 | Data | New title-screen artwork (all four files), same formats | `Icons/TitleScreen*` | idem | front end | adopt (Mac change) | CONFIRMED |
| U1 | Screen | Only 640×480 / 800×600 / 1024×768; always hi-res HUD; same fx/fy formula | `CAmigaWindow::CAmigaWindow` 10102400, `ReadScreenParams__Fv` 10051fc0 | `FUN_0104107c` | `frontend/display.rs`, `camera.rs`, `hud/` | adopt: 640×480 mode, hi-res HUD as default; the mode list itself is a platform artefact | CONFIRMED |

Objects, effects, flares and weather: §6. The Mac-only effect changes D1–D10 are in
§6.3; the port gaps that both builds share are in §6.1 and §6.2.

---

## 1. Method and tools

* `tools/re/fn.py <macx|mac9|os4|68k> <name>` prints one
  decompiled function.
* `tools/re/symbols/pef.py` parses `extracted/mac/Payback/Payback (OS X)`:
  * it unpacks the PEF `pidata` data section (base **0x1017b440**, right after the
    code);
  * it decodes the loader relocations, so each data word is known to be code-, data- or
    import-relative;
  * it gives the TOC register **r2 = 0x10183440** (data + 0x8000). Ghidra's unresolved
    `*(local_xx + -0x62a8)` is the TOC slot at `0x10183440 − 0x62a8`;
  * `toc(off)`, `rptr(addr)`, `cf32/cf64` read the float pools (CodeWarrior keeps them in
    the code section, e.g. 0x10174694, 0x101746b0, 0x101741b8, 0x101741d0), and `dis()`
    disassembles with Capstone.
* Global map (Mac → OS4), confirmed from `ReadScreenParams` and use:

  | Mac | OS4 | meaning |
  |---|---|---|
  | `10188f74` / `f78` | `c348` / `c344` | W / H |
  | `10188f8c` / `f90` | `c330` / `c32c` | cx / cy |
  | `10188f84` / `f88` | `c338` / `c334` | cx−1 / cy−1 |
  | `10188f94` / `f98` | `c328` / `c324` | fx / fy |
  | `10188fc0` / `fc4` | `c2fc` / `c2f8` | Gouraud on / Gouraud available |
  | `10188fac` | `c310` | view-size scale (0xb0…0x100) |
  | `PTR_DAT_1017c5d8` → | `c7d8` | hardware (OpenGL / Warp3D) |
  | `PTR_DAT_1017c2b4` → | `c7dc` | rotating view |
  | `*_DAT_1017c5e4` | `c728` | layer limit |
  | `*_DAT_1017c514` | `c6c8` | motion blur |
  | `*_DAT_1017c44c` / `uRam10188fe0` | `c7d0` / `c2f0` | rotating yaw / pitch |
  | `*_DAT_1017c594` | — | replay playback (`EnablePlayback__Fv` 10052390) |
  | `*_DAT_1017c6f4` / `*_DAT_1017c6f0` | `c44a` / `c448` | game seed (`random__Fi` 100834e0) / second seed (`random3__Fi` 10083530) |

---

## 2. City rendering

### 2.1 Same as the Amiga (checked, no action)

| Item | Mac | OS4 | Notes |
|---|---|---|---|
| Top dispatch by slope, return `base − h1` | `drawCubeTop__FP4Cubeiii` 100d9970 | `FUN_010a3f1c` | slope ≥ 5 → RHoriz in both |
| Side order N, S, W, E; skipped if slope 0 | `drawCubeSide__FP4Cubeiii` 100f0e80 | `FUN_010bb8f0` | |
| Flat top, software, flip → u = 63/0 | `drawCubeTop_None__FP4Cubeiiii` 100c8160 → `drawGroundFlat2__FPUcsslili` 100b4ca0 | `FUN_010920d8` → `FUN_0107a998` | y1 uses cy−1 (`f88` = `c334`) in both |
| Gouraud flat top: 12 corner values clamped to 255, per-pixel `(R&0xF8)<<15 | (G&0xF8)<<10 | (B&0xF8)<<5 | texel` | `drawGroundFlat2GouraudColour…` 100b5180, `GroundFlat2GouraudColour…` 10123dc0 | `FUN_0107a5d8` | the light is interpolated per row (edges) and per pixel (span), i.e. bilinear |
| Reflect top at `base + h2`; software: `#Water` scroll + background displacement | `drawCubeTop_None` (type 4), `drawGroundReflect__FPUcssiiii` 100b46c0 | `FUN_010920d8`, `FUN_0112ca44` | |
| Reflect under Gouraud: one grey light per corner = `min(255, (R+G+B)/3)` | `drawGroundReflectGouraud…` 100b4880 (call at 100ccb84) | `FUN_0107a02c` (`sum < 0x300 ? sum/3 : 255`) | identical result; rendering.md §3 "always lit 0x80" refers to the lighting pass, not the drawing |
| Alpha top (software): `drawGroundAlpha(tex, flip=1, …)` through table 4, x1 = cx−1 + …, y1 = cy−1 + …; **no Gouraud** | `Draw2DEffect__FPUciiiii` 100a8210 | `FUN_01036044` → `FUN_01033d50(tex,1,0,…)` | flip ignored in both |
| Genlocked top: `drawGroundGenlock` / `drawGroundGenlock2GouraudColour` (texel 0 skipped) | 100b54e0 / 100b4e30 | `FUN_0112c5cc` path | |
| Hardware tops: (NE, NW, SW) + (SE, NE, SW) triangles, colour `min(L,255)/256` (0.00390625 at 0x101741d0[8]) | `drawCubeTop_None` hw branch, `Set2DPoint2__Fiii` | `FUN_010920d8` hw branch | |
| Hardware water UVs: `128 + 8·rot(dx,dy)/f + k·255/16384`, `k = sin[((g·16+scroll)·90) mod 5759]` (#Water) or `sin[g·1440 mod 5759]` | `drawCubeTop_None` 819ff.; constants 0x101741d0 = {…, 128.0, 8.0, 16384.0, 1/256} | `FUN_010920d8` ~l.3020 | rendering.md §11.4 is correct for the Mac too |
| Hardware walls: perspective on (state 0x200), Gouraud walls through the colour-interpolating clipper | `drawCubeSide_North__FP4Cubeiiii` 100d9a50 | `FUN_010a4004` | |
| Rotating-view projection (`den` floor 200), water row `sy/2 + max(0,(pitch−0x157c)·(cy−1)/0x103)` | `Set2DPoint__FP6vertexiii` 100c7d60, `DrawAffineTriangleReflect__FP8triangle` 100ba910 | `FUN_01091d70`, `FUN_01081ba4` / `FUN_0108a2a0` | |
| Background: `.chunky` nearest-scaled to W×(H+10); hardware appends "2" (`.chunky2`, 320×256 RGB24, TrueColour upload) | `LoadBackground__FPCc` 100ad8b0, `ScaleBackground__FPUcii` 100ad730 | `FUN_0106e564` | the Mac appends a suffix, the Amiga patches the last character: same file |
| Night: cube textures darkened in place through `T1[c][0x67]` except `$`/`#` names; hardware copies on the card are not re-uploaded | `SetNight__Fs` 1015b5a0, `ReloadCubeTexture__FP11TextureInfo` 1015ac70 | `FUN_0106ecf0` | confirms rendering.md §11.3 INFERRED: with OpenGL the night is only the vertex colours |
| `Ice2` / `RoofWindow` uploaded "Averaged" (alpha 0x7f) | `GetCubeTexture__FUs` 1015a9f0 | `c3bc` | |
| No sky quad | — | — | §5 of rendering.md holds |

So cube tops and sides, slopes, UVs and tiling (one copy per face, u/v 0…63, flip
mirrors u), the face types, Alpha / Genlocked / Reflect, water and the sky are **unchanged
on the Mac**. Port: keep.

### 2.2 Draw order — same (CONFIRMED)

`DrawMap__FiiiisPiPiPii` 1005a260 has the same structure as `FUN_01049374`:

```
for layer L in 0..min(2, limit):
    4 quadrant loops, far → near:   (L>0) objects on this layer, scenery of layer L−1,
                                    side faces of cube L          (drawCubeSide)
    4 quadrant loops:               tops of cube L                (drawCubeTop)
    UpdateEffects(L) / UpdateEffectsMulti1/2(L)
    4 quadrant loops:               DrawSceneryShadow, DrawShadow (once per object, +0x816 flag)
upper pass (objects/scenery above the layer limit), then UpdateEffects(3)
```

The off-map fill (param 5: the top of the clamped edge cube drawn for negative square
coordinates on layer 0) is present in both builds. Port: keep.

### 2.3 Replay "TV camera" (Mac only) — CONFIRMED

`DrawMap` 1005a260 prologue, active when rotating view **and** replay playback
(`*_DAT_1017c594`, set by `EnablePlayback__Fv`) **and** single player
(`*_DAT_1017c4bc == 0`):

* A roaming camera position `(_DAT_1017d2cc, _DAT_1017d2c8)` (16.16 squares) moves each
  frame by a velocity `(_DAT_1017d2c4, _DAT_1017d2c0)·ticks`, clamped to the map.
* The yaw eases a quarter of the way per frame towards `0x167f − GetAngle(target − cam)`.
  The target is the followed vehicle or the player object (`+0`/`+4`, or the object
  table entry). The pitch is fixed at **0x14b4** (28.75° tilt, beyond the normal
  0x157c…0x167f range).
* The camera height is `1000 − (target.z >> 4)` (`iStack20`), and the visible range
  gets **+4 squares**.
* When the target leaves a quarter of the visible range, the camera jumps to a new
  random spot within the range around the target. `random3(4)` chooses the new velocity:
  * 3: random ±0x400;
  * 2: 1/3 of the target's velocity (`+0x7dc/+0x7e0`) × 64;
  * else: that plus random ±0x400.
* Yaw and pitch are restored after the frame.

The water-row formula clamps the negative pitch offset to 0, so water stays valid.
Port: replays exist but the rotating view is not implemented. Classification: Mac
change (new feature). **Adopt** as part of the rotating view, as the default behaviour
when playing back a replay with the rotating view on. Its `random3` numbers must come
from a render-only generator so the replay stays deterministic.

### 2.4 Chimney smoke (CONFIRMED)

| | Mac `DrawMap` 1005b0b8 | OS4 `FUN_01049374` start |
|---|---|---|
| generator | `random3__Fi` (seed `c6f0`, not the game seed) | `FUN_0103ac94` (game seed `c44a`), restored after the frame by `FUN_0105bd98` |
| when | per chimney, **per tick**: `if random3(3)==1` puff | once per view: 8 numbers; if `rand(3)==1` every chimney adds `ticks` puffs with the same numbers |
| puff | type 6 `Dust2`, x = centre + rnd(100)−50 + 100·i, y = centre + rnd(100)−50, z = base − 2500 + rnd(300), v = (150 + rnd 30, rnd 30 − 15, rnd 30 − 250), size 300 + rnd(200), list = layer, blend 0x10000 (T1) | same values |

The Mac chimneys smoke independently instead of in unison. Both builds restore the game
seed after rendering (Mac `DrawView__Fv` 1006eb80: `GetSeed` … `SetSeed`; OS4
`FUN_0103ad40`/`FUN_0103ad4c` in `FUN_0105bd98`), so neither affects the simulation.
Port (`objects.rs` `chimney_smoke`): follows OS4 (one set per view, all chimneys in
step). Classification: Mac change. **Adopt** the Mac scheme: per chimney, per tick, from
a render-only generator, which also keeps the port's simulation RNG out of rendering.

---

## 3. Lighting (Gouraud pre-pass and dynamic lights)

### 3.1 Same as the Amiga (CONFIRMED)

`DoLightSourcing__FP3Map` 1015b840 = OS4 `FUN_010766c8`:

* Base light table, identical including the hardware values: day sw 0x80/0x30/step 0x40;
  night sw 0x60/0x30/0x20; day hw 0xbe/0x80/0x30; night hw R,G 0x20 B 0x30 / shadow
  0x0c,0x0c,0x10 / 0x20.
* Reflect tops 0x80 (sw) / 0xbe (hw).
* Edge squares are not tested. The slope ±step applies only to corners still equal to
  the lit R value, with the same slope/direction table.
* Hardware jitter `random(16) − 8` (game generator `random__Fi`) on the four corners
  meeting at a grid point, top and bottom separately, clamped at 0.
* Voxel grid (`InitVoxels__FP3Map` 100b3590, `AddVoxels__Fiiiiii` 100b33d0): 256×256×10,
  boxes in half-squares, z = clamp(H>>7, 0, 9). Tops are marked per slope edge; Reflect
  tops at h2; sides as columns along their edge.
* Lamps: scenery `!` (33, renumbered `$`), `$` (36), `#` (35); positions and
  intensities as rendering.md §4; `AddLighting2__FP3Mapiiiiii` 100b2580.
* `.CubeTextureRemapList` applied in the same loop (through `GetCubeTexture`).
  Lighting is cached per map (`map+0x1e0000`) until `CloseLightSourcing`.
* Dynamic lights `AddLighting__FP3Mapiiiiii` 100b1520 / `SubLighting` 100b1d70:
  * a light is skipped when the Gouraud option is off or 1000 lights are active;
  * intensities are halved on day maps in software only;
  * box radius `(isqrt(max)>>13)+1`, clamped 1…126.
* Traffic lights and beacons: `ReadMapLighting__FP3Map` 100b2e10 builds the list (max
  1000), `DoMapLighting__Fv` 100b30b0 adds the light values each frame (same values as
  §4).
* Effects lighting: `DoEffectsLighting__Fv` 100a82b0.

### 3.2 L1 — paired shadow rays (CONFIRMED on the Mac)

`CheckVoxels__Fiii(px, py, z)` @ 100b3960 returns 0 (= shadow) if any cell
`(px−k, py−k, z+k)`, k = 1…, z+k ≤ 9, is marked. This is the same ray as rendering.md §4
(sun to the north-west). `DoLightSourcing` calls it per corner with (x, y = square;
base = this cube's base; corner z as per slope):

```c
c0 NW:  lit unless CheckVoxels(2x,   2y,   z0) == 0
c1 NE:  shadow only if CheckVoxels(2x+1, 2y,   z1) == 0 && CheckVoxels(2x+2, 2y,   z1) == 0
c2 SW:  shadow only if CheckVoxels(2x,   2y+1, z2) == 0 && CheckVoxels(2x,   2y+2, z2) == 0
c3 SE:  shadow only if CheckVoxels(2x+1, 2y+1, z3) == 0 && CheckVoxels(2x+2, 2y+2, z3) == 0
c4..c7: the same four tests with z = base >> 7 (bottom corners)
```

So the NE, SW and SE corners are lit if *either* of two parallel rays is clear: one from
the corner itself, one from the half-square point next to it, towards the NW corner.
The OS4 `FUN_010766c8` has the same paired, inlined tests (e.g. the `-0x9e2` /
`-0x13eb` pairs around decompiled l.512–560 of the dump in `scratch_render/city/os4_light.c`);
the doc's "INFERRED: the same diagonal" for the other corners is resolved by this.

**Port** (`lighting.rs` `compute`, `Voxels::shadowed`): uses one ray from
`(2x + 2(c&1), 2y + (c&2))` for every corner. That is the Mac's *second* ray only, so
the port puts more NE/SW/SE corners in shadow than the original. **Adopt**: for c ∈ {1,
2, 3, 5, 6, 7} test both points and shadow only if both rays hit.

### 3.3 L2 — Vert slope height bug (CONFIRMED in both builds)

Disassembly of the corner-height switch in `DoLightSourcing`:

```
1015bcec  lbz r0,8(r29)      ; slope
1015bcfc  cmpwi r0,2 ; beq 1015bd18   ; Vert
1015bd04  bge 1015bd20                ; Horiz (3)
1015bd18  lha r0,2(r29) ; add r26,r26,r0      ; base += h1        <-- extra
1015bd20  lha r0,6(r29) ; add r0,r26,r0 ; srawi r0,r0,7   ; zNE = zSE = (base+h2)>>7
1015bd40  lha r0,2(r29) ; add r26,r26,r0      ; base += h1
1015bd58  srawi r0,r26,7                      ; zNW = zSW = base>>7
```

Vert (slope 2) falls into the Horiz code after an extra `base += h1`. For a Vert cube the
shadow test therefore uses:

* zNW = zSW = (base + 2·h1) >> 7 and zNE = zSE = (base + h1 + h2) >> 7, instead of
  NW/NE = base + h1 and SW/SE = base + h2;
* a running base for the cubes stacked above it in that square that is h1 too high,
  which moves their corner and bottom z up.

OS4 `FUN_010766c8` at `LAB_01076cf4` does the same: `iVar20 = iVar6 + h1` (base + 2h1),
`iVar21 = (h2 + iVar6) >> 7`, and `iVar6 = iVar20` carries on. The voxel *marking*
(`InitVoxels`) is correct for Vert. Only the shadow test is wrong.

**Port**: `lighting.rs` uses the correct heights. **Adopt** to keep the original's
(both platforms') light pattern on Vert slopes: Vert corners are lit more often, and
cubes above a Vert cube are tested higher.

---

## 4. Palette, Alpha.256, Gouraud.256

* **Palette**: the Mac palette is a LoadRGB32 table in the data section, identical to
  the Update 7 `Payback.exe` palette (4 copies; the CD `Payback.exe` differs only in pal[8] =
  (7,7,7)). CONFIRMED (data pass). Software output: `LoadRGB32` @ 100fc000 →
  `SetColorTable` → `DSpContext_SetCLUTEntries`, 8-bit CLUT, 1:1 blit by
  `WritePixelArray8` @ 10102dd0.
* **ClosestMatch** `ClosestMatch__Fiii` @ 10083dc0: `25|ΔR| + 36|ΔG| + 16|ΔB|`, best
  starts at 20000, first best wins, early exit on 0, result 0 → 8. Same as OS4. Port
  (`postfx.rs` palette snap): keep.
* **Alpha.256** `GetAlpha__Fv` @ 10084710 loads `General/Alpha.256` (0xB0000) or
  computes `CalcAlpha__Fv` @ 10083ec0 and saves it. Exact formulas, d = dst
  (framebuffer), s = src (texel), per channel, clamped:

  | T | formula |
  |---|---|
  | 0 | d + s |
  | 1 | max(d − s, 0) |
  | 2 | s index ≠ 0 ? max(d − 16, 0) : d |
  | 3 | (s·255 + (255 − max(sR,sG,sB))·d) >> 8 |
  | 4 | s == d (index) ? s : nearest((s + d) >> 1) |
  | 5/6/7/8 | add R / R+G / G / B only |
  | 9 | d + s − 32 |
  | 10 | pal[s] + d_index − 128 |

  The Mac file is exactly this computation on the current palette (100 % on all
  tables). The Amiga Update 7 file is 99.2–99.8 % (it was built from the CD palette with
  pal[8] = 7,7,7). The OS4 generator lacks the T4 `s == d` shortcut (7 diagonal entries
  between duplicate colours, invisible). Night remap `T1[c][0x67]` differs at c = 155
  only. CONFIRMED.

  **Port** (`postfx.rs` `alpha_tables`, `night_remap`): **adopt**: compute the tables
  with the formulas above (or read the Mac file) instead of trusting the Amiga file.
  rendering.md §3 should get the exact T3 and T4 definitions.
* **Gouraud.256** `GetGouraudTable__Fv` @ 10084400: loads the file, or generates and
  saves it; `[r>>3][g>>3][b>>3][c]` = nearest(clamp(pal[c] + (r,g,b) − 128)), r,g,b =
  0, 8, …, 248, no +4 centre. The shipped Mac file is **stale** (CD palette, 99.78 %
  against the current palette). Port computes it exactly: keep, ignore the file.
* **Motion blur** `MotionBlur__FPUcPUcPUc` @ 100a6d50 → `ScreenAlpha__FPUcPUcPUcPUcUs`
  @ 10154eb0: identical to OS4 `FUN_0112cdb8`. Table 4, prev·256 + cur, 32 pixels per
  iteration, count `(W·H >> 5) − 1` (the last 32 pixels are not blended in either
  build), passed as an unsigned short on the Mac (W·H must stay < 2 M pixels). Software
  only: `c2p__Fv` @ 10053410 runs it only when not hardware; the pause menu refuses it
  under OpenGL, as on Warp3D. Difference B1: turning it on blanks both buffers on the
  Mac. Port: keep.

---

## 5. Hardware mode: the OpenGL Warp3D layer

The Mac implements the Warp3D calls the game uses on AGL/OpenGL
(`W3D_*` 10153490…10154ba0). `API_3D_Init` (1014ac60) makes the same state calls as OS4
`FUN_01002024`. The GL mapping decides what the "hardware look" is on the Mac.

| Warp3D | OpenGL on the Mac | Conf. |
|---|---|---|
| state 0x100 TEXMAPPING | `GL_TEXTURE_2D` | CONFIRMED |
| 0x200 PERSPECTIVE | only `glHint(GL_PERSPECTIVE_CORRECTION_HINT, NICEST/FASTEST)`; the real switch is `glTexCoord4f(u·w/texW, v·w/texH, 0, w)` with w = 1 when the cached flag `*0x10399eb2` is 0 | CONFIRMED |
| 0x400 GOURAUD | on: `glShadeModel(GL_SMOOTH)` + `glTexEnv(GL_MODULATE)`; off: `GL_FLAT` + `GL_REPLACE`. Per-vertex `glColor4f` only while the cached flag `*0x10399eb4` is set | CONFIRMED |
| 0x2000 BLENDING | `GL_BLEND` | CONFIRMED |
| 0x4000 FOGGING | `GL_FOG` (always off) | CONFIRMED |
| 0x40000 anti-aliasing | `GL_POINT_SMOOTH` + `GL_LINE_SMOOTH` (on) | CONFIRMED |
| 0x80000 dithering | `GL_DITHER` (on) | CONFIRMED |
| `SetBlendMode(a,b)` | jump table at 0x10192b58 → `glBlendFunc`; 2 = `GL_ONE`, 7 = `GL_SRC_ALPHA`, 8 = `GL_ONE_MINUS_SRC_ALPHA` | CONFIRMED |
| `SetFilter(2,2)` | `GL_LINEAR` min and mag, **no mipmaps** | CONFIRMED |
| texture wrap | `GL_REPEAT` s and t (set at upload) | CONFIRMED |
| `AllocTexObj` formats | A1R5G5B5 → in-place `convert16BitTextureARGB2RGBA` @ 10154ba0 → `GL_RGBA`/`GL_UNSIGNED_SHORT_5_5_5_1`; ARGB32 → `convert32BitTexture` (rotate) → `GL_RGBA8`; R8G8B8 → `GL_RGB` | CONFIRMED |
| texture sizes | `RoundToExp2__Fi` @ 1014b010: next power of two, capped at 256; larger textures nearest-downsampled, smaller non-power-of-two padded | CONFIRMED |
| `SetDrawRegion` | `glFinish`, `aglSwapBuffers`, clear colour and depth to 0, reset the depth counter to −1.0 | CONFIRMED |
| `DrawTriangle`/`DrawLine`/`DrawPoint` | immediate mode at `z = counter`, counter += 0.0001 per primitive; `glOrtho(0,W,H,0,−1,1)`, `GL_DEPTH_TEST` with `GL_LESS`; `glLineWidth`/`glPointSize` from the vertex | CONFIRMED |
| `CheckDriver` | 4 (`W3D_DRIVER_3DHW`) when `aglChoosePixelFormat` is linked | CONFIRMED |
| `LockHardware` | no-op | CONFIRMED |

Consequences:

* **Z buffer**: present but used only to keep submission order (each primitive is in
  front of all earlier ones), so drawing is painter's order exactly as on Warp3D (H2).
  The counter overflows into the near plane after 20 000 primitives per frame (all views
  together); later primitives (effects, flares, HUD) would vanish. That is a platform
  limit. Port: keep (the port sorts or depth-tests as designed), ignore.
* **Texture environment** (H1): with GOURAUD off, textured primitives are REPLACE
  (texture only). The current colour (`W3D_SetCurrentColor`) then tints only untextured
  primitives. With GOURAUD on, texture × vertex colour. The hardware city is always
  Gouraud (LoadConfig forces it, below), so city faces are modulated as in
  rendering.md §11.3. Which effect and object primitives get a tint depends on the
  GOURAUD state at that moment (see §6).
* **Clear** (H3): the hardware screen is black off the map edge and has no trails. Port
  already clears.
* **Clipping** (H4): the Mac clips every triangle in software against the current view
  (0…W, 0…H, the per-view W/H from `DrawView`) and then adds the view origin
  (`_DAT_1017c340`/`c33c`):
  * `API_3D_DrawTriangle_Clipped_Simple` @ 10151ec0 interpolates x, y, u, v;
  * `…_Affine` @ 101504a0 and the generic `…_Clipped` @ 1014ce20 also interpolate the
    RGBA colour;
  * `…_Perspective` @ 1014ec20 interpolates u·w, v·w and w, then divides back.

  OS4 `FUN_0100499c`/`FUN_01004b84`/`FUN_01004d6c` only reject triangles wholly below
  the screen, add the view offset and leave clipping to Warp3D (screen-wide). In split
  screen a Warp3D triangle could spill into the neighbouring view (INFERRED; no
  `SetScissor` call). The Mac fixes that. Port: per-view viewports, keep.
* **Smoothing/dither** (H5): rain lines and splash points are anti-aliased on the Mac;
  16-bit colour depth when OpenGL is on (DrawSprocket 16-bit). Ignore.
* **Gouraud forced on, view size 0x100**: `LoadConfig__Fv` @ 10052be0 sets
  `_DAT_10188fc0 = 1` and `_DAT_10188fac = 0x100` when hardware is on; Gouraud off if not
  available. Same as OS4 `FUN_0104199c`. Port (`render_mode.rs`): keep.
* **Upload conversions**: the `API_3D_Set_*` flags (@ 1014b070…1014b2b0) map 1:1 to
  the OS4 flag setters `FUN_01002478…010024f0`, and `API_3D_Internal_UploadTexture` @
  1014b2f0 has the same per-pixel chain as `FUN_010024fc` (details in §6).
* The software chunky renderer is unchanged; hardware is the **default** on OS X (no
  QuickDraw/OpenGL switch in the OS X preferences, U1). The OS 9 build links the same
  AGL/OpenGL and DrawSprocket imports (`analysis/ghidra_mac9/functions.tsv`:
  `aglChoosePixelFormat`, `glBlendFunc`, `DSpContext_SetCLUTEntries`) and offers "Use
  OpenGL / Use QuickDraw" in its setup dialog.

---

## 6. Objects, effects, flares, weather

Sources: the objects/effects passes; extracts in `analysis/mac/scratch_render/fx/obj/`
and `fx/eff/` (`mac_*.c`, `os4_*.c`, `dis_*.txt`), and the W3D notes in
`fx/w3d_notes.txt`.

### 6.1 Objects — same as the Amiga (CONFIRMED)

The object renderer is the same source, with a larger struct layout. Field map:

| Mac | OS4 | field |
|---|---|---|
| `+0x3cb8` | `+0x1528` | faces |
| `+0x8120` | `+0x48c4` | r |
| `+0x8162` | `+0x4906` | shine |
| `+0x8168` | `+0x490c` | face sorting |

Functions:

| Mac | OS4 |
|---|---|
| `DrawObject__FP6Objectiiis` 10075880 | `FUN_010c101c` |
| `DrawScenery` 10079e90 / `DrawScenery2` 1007c210 | `FUN_010c4fec` / `FUN_010c7218` |
| `DrawShadow` 1007f7d0 | `FUN_010c9f28` |
| `DrawSceneryShadow` 1007ea20 | `FUN_010c9360` |
| `DrawPart` 100794c0 | — |
| `LoadStructure` 10074630 | `FUN_010bf8bc` |
| `SetObjectNight` 10074210 | `FUN_010bf310` |
| `AddHeadlights` 10073f60 | `FUN_010bf070` |

These match OS4:

* modes 1–6 and the shine kinds;
* the burnt skin and the dent table;
* face sorting and the pickup spin rates;
* flare size 300 and the alarm-box pulse;
* the headlight polygon;
* night darkening and the `.dark` swaps;
* the upload conversion flags (`API_3D_Set_*` 1014b070…1014b2b0 = OS4
  `FUN_01002478…010024f0`; `API_3D_Internal_UploadTexture` 1014b2f0 has the same
  per-pixel chain as `FUN_010024fc`).

Cosmetic Mac-only details, all platform artefacts (ignore):

* objects never enable GOURAUD, so they draw with `GL_REPLACE`, which looks the same as
  Warp3D;
* `GL_LINE_SMOOTH` has no visible effect on mode-5 lines, because blending is off for them;
* `GetTexture` also accepts LF line endings.

**Port gaps, where both builds agree and the port differs** (`objects.rs`, `effects.rs`):

| Gap | Original | Port | Rec. |
|---|---|---|---|
| Object shadows missing | people: `Shad` sprite through T1; vehicles: 3×3 `shadow` mesh following the ground; trees: `DrawSceneryShadow` (`Tree2.clone` / `tanne.clone` in hardware, flat black at 0.3 otherwise). Offset K = 0x200 for cars, 0x80 for people **and bikes** (model `+0x8160`) | none | adopt |
| Mode-5 lines | tested on slot 0's mode byte regardless of `active`; drawn black when burnt | — | adopt |
| Other shine modes (0x10/0x13/0x15 …) | colour-keyed `(7,8)` in hardware, not opaque; 0x34 through table 4 | opaque | adopt |
| Scenery mode 0x14 | table 0 (additive) in `DrawScenery`, table 4 in `DrawObject` | one path | adopt |
| Sand recolour | object `+0x4b4`: "Red sand" / "Red dark sand" faces switched to the light variant | missing | adopt |
| Hardware mode 3 | soft-blended `(7,8)` (filtered colour-key alpha) | hard mask | adopt |
| Debris | `DrawPart` (with the 320×200 cull) | missing | adopt |

### 6.2 Effects, flares, weather — same as the Amiga (CONFIRMED)

Functions:

| Mac | OS4 |
|---|---|
| `UpdateEffects__Fiiiii` 100aa1f0 | `FUN_010386e8` |
| `AddEffect` 100a7f80 | `FUN_01035da0` |
| `AddFlare` 100a6fb0 | `FUN_0103451c` |
| `DrawFlares` 100a7340 | `FUN_01034804` |
| `drawGroundAlpha` 100a6720 | `FUN_01033d50` |
| `SkidUnit` 10081830 | `FUN_010cc288` |
| `AddBrightness` 10083620 / `SetBrightness` 100839e0 | `FUN_0103ad6c` |
| `DoEffectsLighting` 100a82b0 | `FUN_01036574` |
| `amiga_levelloop` 10100f50 / `amiga_startlevel` 100fff40 | main loop / `FUN_010692bc` |

These match OS4:

* the effect records, size/lifetime rates and gravity 17;
* the per-type Alpha tables and the hardware blend modes;
* the conversion flags;
* the flare ghost list and flare option default 2;
* no occlusion test in hardware (no `glReadPixels`);
* lightning timing and thunder;
* skidmarks and the shadow offset K;
* all dynamic-light constants.

Hardware blend factors come from `W3D_SetBlendMode`'s jump table (0x10192b58): 2 =
`GL_ONE`, 7 = `GL_SRC_ALPHA`, 8 = `GL_ONE_MINUS_SRC_ALPHA`, as on Warp3D.

### 6.3 Effects: Mac differences (CONFIRMED unless noted; Mac = definitive → adopt)

| # | Difference | Mac | Amiga | Kind | Port (file) | Recommendation |
|---|---|---|---|---|---|---|
| D1 | Snow dies at street level: a type 0xC flake with z > 0x4dc0 is removed and its ground mark placed at z = 19904 (Amiga: z > 24000, mark at 0x57e4) | `UpdateEffects` 100aa1f0 | `FUN_010386e8` | bug fix (flakes no longer fall through the streets) | `effects.rs` weather update and marks | adopt |
| D2 | The weather wrap/delete rectangle is the exact visible half-width/half-height of the view plus one square, so it no longer scales with the screen size as the Amiga's `((camH·16+0x2af2)·W+0x800)>>9` does; separate rotating and split-screen variants | `UpdateEffects` | `FUN_010386e8` | bug fix (hi-res screens) | `effects.rs` wrap | adopt |
| D3 | Lightning **sets** the flash to `100 + rand(150)` instead of adding, so a multi-flash strike doesn't build up | `amiga_levelloop` 10100f50 | main loop after `FUN_010542b8` | Mac change | `weather.rs` lightning | adopt |
| D4 | The level-start snow/rain spawn uses the visual generator `random3`, not the game generator | `amiga_startlevel` 100fff40 | `FUN_010692bc` (`FUN_0103ac94`) | Mac change (decouples weather from the simulation) | `weather.rs` spawn | adopt with a render-only RNG (keeps the sim deterministic) |
| D5 | Chimney smoke: `random3(3)==1` per chimney per tick, with fresh numbers each time (§2.4) | `DrawMap` 1005b0b8 | `FUN_01049374` | Mac change | `objects.rs` `chimney_smoke` | adopt, render-only RNG |
| D6 | No `Blood.dark`: the darker Mac `Blood` is used at night too | effect loader / night path | `FUN_01035250` | Mac change (data and code) | `effects.rs` night swap | adopt (keep the swap only when only Amiga data is installed) |
| D7 | German language: `Gas` replaces `Blood` | effect loader (decompiled.c ~23215) | INFERRED same | Mac change / localisation | `effects.rs` | adopt |
| D8/D9 | Random-argument evaluation order reversed in effect spawns and in the effect-light jitter (visual-only randoms) | `UpdateEffects`, `DoEffectsLighting` | `FUN_010386e8`, `FUN_01036574` | Mac change (compiler evaluation order) | `effects.rs`, `lighting.rs` | adopt the order on a render-only RNG; no sim impact |
| D10 | The palette-flash quad does not reset the GOURAUD state; on the Mac GL path it may draw with a stale texenv | `AddBrightness` 10083620 | `FUN_0103ad6c` | platform artefact | — | ignore (report only) |

**Port gaps where both builds agree** (adopt):

* muzzle-flash dynamic lights at z − 500, in `lighting.rs` `collect_lights`:
  * minigun (60M, 60M, 8M);
  * pistol (40M, 40M, 10M);
  * shotgun (60M, 60M, 20M);
* `Rain` loop volume 0x7f instead of 0xff when the visible-layer limit is not 3
  (`weather.rs`);
* hardware rain: `(7,8)` blend with alpha 0.4, not additive (`effects.rs`; already
  noted in rendering.md §11.8).

---

## 7. Screen setup, resolutions and UI scaling (CONFIRMED unless noted)

| Item | Mac | Amiga | Port | Rec. |
|---|---|---|---|---|
| Resolutions | MENU 135 "Size": 640×480, 800×600, 1024×768 only (`CAmigaWindow::CAmigaWindow` @ 10102400 from pref +0x2b0; `CPaybackPreferences::EditPreferences` @ 10050c60 asks DrawSprocket for 8-bit, or 16-bit with OpenGL) | any `PaybackSetup` mode via `OpenScreenTags`, typically 320×256 | `frontend/display.rs` lists 800×600, 1024×768 | adopt: add 640×480 (the Mac default) |
| Window / fullscreen | window or DrawSprocket fullscreen (`MatchDSpContext` @ 10102a80); defaults (`ClearPreference` @ 100508c0): fullscreen, 640×480, OpenGL on, Gouraud on, rotating off, motion blur off, FX = flares + artifacts | screen | windowed/borderless/exclusive | keep |
| Presentation | software: 8-bit GWorld + `WritePixelArray8` 1:1; OpenGL: viewport = port, pixel `glOrtho` | native mode, no doubling | renders at window size | keep |
| Screen constants | `ReadScreenParams__Fv` @ 10051fc0 = OS4 `FUN_0104107c` = 68k: fx = (W+8)·16/320, fy = ((H+8)·16)>>8, cx = ((W−1)>>1)+1 | same | `camera.rs` `view_fov`, `visible_squares` | keep |
| Field of view / aspect | 640×480 → fx 32, fy 30 (10:8 world area on square pixels, 6.7 % wider than tall = the Amiga 320×256 look on a 4:3 PAL screen); 800×600 → 40/38; 1024×768 → 51/48 | 320×256 → 16/16 | fixed vertical 53.1°, horizontal from the window aspect | adopt as default: HFOV fixed at 64° (2·atan(160/256)), stretched to the window, as the Mac does at 640×480; free aspect as an option |
| View-size option | `_DAT_10188fac` 0xb0…0x100 step 0x10, software only, `SetPartialChunky__Fv` @ 10064920, applied in `DrawView` | `DAT_0114c310`, `FUN_01051940/01051a10`, `FUN_01051800` | not implemented | option (low; software-only on both, never used by the OS X default) |
| Split-screen constants | `DrawView__Fv` @ 1006eb80: 2 players fx = ((W/2)+7)·16/240, fy = (H+8)·16/384 | same | `view_fov` | keep |
| Hi-res flag | `InitGraphics` @ 1008d110: W > 639 && H > 399 → icons scaled (`AreIconsScaled__Fv` @ 10085eb0); the Mac is therefore **always hi-res** | `FUN_010138dc` (same threshold) | 256-line HUD canvas (lo-res look) | adopt: hi-res HUD as default (the Mac is never lo-res); lo-res as option |
| Icon doubling (software) | `LoadIcon__FPciii` @ 10086f80 / `LoadIconShadow` @ 100878d0: 2×2 block = P, avg(P,right), avg(P,below), avg(P,diag) through Alpha table 4 (`AverageVals` @ 10086ef0), edge-clamped, colour 0 blended too | `FUN_0100d3c4` (same) | nearest | adopt with the hi-res HUD |
| Icons (hardware) | quads ×2 size, UVs 0…w, bilinear (`DrawIcon__FP4Iconii` @ 10085300 etc.) | `FUN_0100a53c` | — | adopt in hardware render mode (bilinear) |
| Fonts | `DisplayText*` double the pitch in hi-res | same | — | keep |
| HUD layout | `DrawGraphics` @ 100929e0, `DrawWeapons` @ 10092480, `DisplayMinimap` @ 10091630 …: the OS4 hi-res column. Hi-res minimap image at (xmax−141, 67) (doc says xmax−142, 66) | same | `hud/` | doc fix |
| Title screen | software: H ≤ 200 → `TitleScreen`, W ≤ 320 && H ≤ 256 → `.medium`, else `.big` 640×480, `ScaleIcon` nearest to W×H; OpenGL: `TitleScreen.w3d` stretched | same | front end | keep |
| Loading box | `DrawProgress__Fi` @ 100538c0: `Loading` 94×35, never doubled | same | drawn on the 256-line canvas (looks bigger) | adopt: draw it at native size |
| Intro | QuickTime `:Videos:payback_high_mpeg1.mpg` 640×480 25 fps, fit with aspect (`PlayAnimation` @ 100fb1d0); the mpeg2/low files are unused | 320×200 FLC | `intro.rs` | keep |
| Menus | frame-limited to `TickCount` (60 Hz) while on a menu (`ch2pl`) | VBlank | — | ignore (platform artefact) |

Extracts: `analysis/mac/scratch_render/ui/`.

---

## 8. Data differences that affect rendering (CONFIRMED; scripts and PNGs in `analysis/mac/scratch_render/data/`)

### 8.1 Cube textures

* Lists: the Mac reads `CubeTextureList.txt` / `CubeTextureRemapList.txt`,
  byte-identical to the Amiga `.CubeTextureList` / `.CubeTextureRemapList`. The flags
  are unchanged and there are no new flag values (4×665, 34×70, 51×65, 50×25, 36×9,
  35×6, 65×2, 52×2). `#Water.chunky` (4160 bytes) is identical.
* 225 textures differ. 182 differ from both the CD and Update 7 versions; 43 changed in
  Update 7 and again on the Mac. 184 of the 225 are used in the shipped maps (13 615
  faces). Kinds:
  1. **Baked "spotlight" relight** of about 170 building walls: the same picture times
     a grey gain map, ≈1.5 at the top centre, ≈1.0 on a U-shaped ring, ≈0.65–0.7 at the
     sides and bottom, mean 0.85 (`cubetex_gain_map.png`). The Uni series also changed
     from orange to beige.
  2. **"Light" variants replaced by copies of the normal texture** (37 groups, e.g.
     Hotel1 = LightHotel1): 159 distinct images instead of 197.
  3. **Dark variants darker** (≈0.70 of the Mac normal vs 0.84 on the Amiga). Some
     gained colour‑0 pixels (DarkHotel2 211→1110, NewWall6 0→142, Building2 0→49).
     Colour 0 is only transparent on Genlocked faces: shipped uses are Building2 (1
     side), NewWall6 (2), Fence (556, new picture), TentRoof (2 tops). User maps use
     more.
  4. **New pictures**: Crowd (photographic), the Wood family, NewWall6/7, Fence (taller,
     new mask), TentRoof (white star bunting) (`cubetex_sheet_new.png`).
  5. `greywall-51.dark`/`52.dark` now match their own names, but the remap list still
     swaps them (51.dark→52), so with Gouraud on they map to the other picture
     (harmless, both are "plain" textures).
  6. Small retouch of `RoofWindow` (Alpha tops).
* **`TentRoof.chunky` is 4032 bytes** (63 rows) on the Mac. `GetCubeTexture__FUs` freads
  0x1000 bytes into an uninitialised `new` buffer, so row 63 is undefined (rows 40–62 are
  colour 0; pad with 0). The port's `payback_formats::texture::IndexedImage::from_chunky`
  would reject it. **Adopt** (pad short chunky files) if Mac data is supported.
* Recommendation: these are deliberate art changes (Mac change). **Adopt**: prefer the
  Mac `CubeTextures/` when the Mac data is available, as a whole set; don't mix it with
  the Amiga set (the Light/Dark/remap relations differ). Code needed: accept the `.txt`
  list names and pad short chunky files (TentRoof). Keep the Amiga set as the fallback
  when only Amiga data is installed.

### 8.2 Other files

* `Alpha.256`, `Gouraud.256`: §4.
* `General/`: `64-_256.dat`, `RandomNumbers`, `map*.size` identical; the loading-bar
  `.size` files differ (irrelevant); Mac lacks `CD_Audio_Handler`, `led`, `Run`.
* **Blood**: the Mac sprite is smaller and darker (1705 non-zero texels vs 3280, mean red
  71 vs 175), and **`Blood.dark` does not exist**. Port (`effects.rs`, the night swap
  at l.~1294): **adopt** (Mac change): use the Mac `Blood` and drop the night swap.
  With Amiga data only, keep the swap, because the bright Amiga Blood needs it. "Gas" replaces Blood for the German language
  (as on the Amiga) (`effect_blood.png`).
* Title screens: new artwork in all four files (orange flaming logo, darker face,
  "apex designs" + "Virtual Programming"), same formats (`icons_TitleScreen*.png`).
  **Adopt** (Mac change) when the Mac data is available.
* ObjectTextures: 10 new front-end textures (Disk, Keyboard, Monitor, PC, Moviereel) and
  **`tanne`**, a top-down conifer now used by `Scenery/Tree.Obj`. `InitObjects` preloads
  `tanne` + `tanne.clone`; in hardware the clone gets the GenlockShadow conversion, like
  `Tree2.clone` (`objtex_mac_only.png`). **Adopt** with the Mac objects.
* Backgrounds `Clouds`/`Stars` `.chunky`/`.chunky2`: identical.

---

## 9. Bug fixes and behaviour changes relative to the Amiga

| Change | Where | Kind |
|---|---|---|
| Per-view triangle clipper in the hardware path (no spill between split-screen views) | `API_3D_DrawTriangle_Clipped*` | fix (platform layer) |
| Hardware frame cleared every frame | `W3D_SetDrawRegion` | platform |
| Motion blur buffers blanked when enabled | `ToggleMotionBlur__Fv` | fix |
| `Alpha.256` regenerated from the current palette, T4 diagonal shortcut | `CalcAlpha__Fv`, data | fix |
| Chimney smoke on its own generator, per chimney, per tick | `DrawMap` | change |
| Snow removed at street level (D1); weather rectangle from the true view size (D2) | `UpdateEffects` | fix |
| Lightning sets the flash instead of adding (D3); weather spawn on the visual RNG (D4) | `amiga_levelloop` / `amiga_startlevel` | change |
| Replay TV camera in the rotating view | `DrawMap` | new feature |
| `Blood.dark` removed (darker Blood day and night) | data, effect loader | change |
| Not fixed (present in both): Vert lighting heights (L2), last 32 pixels not motion-blurred, Alpha tops x1/y1 at cx−1/cy−1 | | — |

---

## 10. Recommendations for the port (priority order; Mac = definitive)

1. **`lighting.rs` `compute`**: paired shadow rays for corners 1–3 and 5–7 (L1). Code
   behaviour of both builds (the port is wrong); the pattern changes on every map. Adopt.
2. **`lighting.rs` `compute`**: reproduce the Vert slope height quirk (L2), in the
   lighting pass only:
   * Vert corner z = base + 2h1 (NW, SW) and base + h1 + h2 (NE, SE);
   * the next cube of that square starts from base + 2h1.

   This is an original bug present in both builds. Adopt.
3. **`postfx.rs`**: compute the 11 Alpha tables from `palette.bin` with the exact §4
   formulas (T3, T4 diagonal shortcut), or read the Mac `Alpha.256`. Bug fix: the
   Amiga file was built from the CD palette. Adopt.
4. **Effects D1–D9** (§6.3), Mac definitive. Adopt:
   * street-level snow death and marks;
   * the view-size weather rectangle;
   * lightning sets the flash;
   * weather, chimney and effect randoms on a render-only RNG;
   * no `Blood.dark`;
   * German `Gas`.
5. **Port gaps shared by both builds** (§6.1, §6.2). Adopt:
   * object, vehicle and tree shadows;
   * the shine modes;
   * sand recolour;
   * debris;
   * hardware mode-3 soft blend;
   * muzzle-flash lights;
   * the rain volume;
   * the hardware rain blend.
6. **Mac data as the preferred set** (Mac changes):
   * cube textures, with the `.txt` list names and short-file padding;
   * `Blood` without the `.dark` night swap;
   * title screens;
   * `tanne` tree and the new object textures.

   Fall back to the Amiga files only when the Mac data is absent. Adopt.
7. **`objects.rs` `chimney_smoke`**: per chimney, per tick `rand(3)==1`, render-only
   generator (Mac change). Adopt.
8. **Motion blur**: clear the previous-frame index buffer when blur is switched on (bug
   fix). Adopt.
9. **Rotating view** (when implemented): include the replay TV camera (Mac change),
   render-only RNG. Adopt.
10. **Presentation** (Mac behaviour; only the mode list is a platform artefact). Adopt:
   * 640×480 in the mode list;
   * hi-res HUD as default (2× canvas, Alpha-table 2× upscale in software, bilinear in
     hardware, hi-res offsets), with the loading box at native size;
   * fixed 64° HFOV ("original aspect").

   Keep the lo-res HUD and free aspect as options.
11. **Ignore** (platform artefacts):
   * the GL depth counter;
   * per-frame GL clear (the port already clears);
   * line/point smoothing and dithering;
   * the stale `Gouraud.256`;
   * DrawSprocket mode handling;
   * menu frame limiting.
12. **Doc fixes**:
    * rendering.md §3: exact T3/T4; water Gouraud = grey corner average;
    * rendering.md §4: paired rays, Vert quirk;
    * rendering.md §11.3: night on the card confirmed;
    * rendering.md §11: Mac GL mapping;
    * frontend_audio.md: hi-res minimap at (xmax−141, 67).
