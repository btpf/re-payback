# Mac release: asset-by-asset comparison with the Amiga data

Mac data: `extracted/mac/Payback/` (HFS extraction of `PaybackMac.toast`, Read Me dated
**30 October 2002**, "Macintosh Edition, Version 1.0", port by Alex Curylo, published by
Virtual Programming). Amiga data: `extracted/game/` (CD + Update 7 merged). Where it
matters, the CD (`extracted/iso/Payback`) and Update 7 (`extracted/update7/.../Files`) were
also checked separately.

The review's scripts, intermediate tables and Rust check program were local working files
(`analysis/mac/scratch_assets/`, not kept; see [README](README.md)). The maintained data diffs
are `tools/re/mac_datadiff.py` and `tools/re/phrase_diff.py`:

| File | What it does |
|---|---|
| `listdiff.py` → `listdiff.txt` | full per-file diff (DIFF / MAC-only / AMI-only) |
| `provenance.py` → `provenance.tsv` | for each differing file: does the Mac copy equal the CD, U7 or OS4 copy? |
| `cubetex.py`, `shift.py`, `cube_stats.py` → `cubetex_*.tsv` | cube texture metrics |
| `objdiff.py` → `objdiff.txt` | `.Obj` parser and per-model diff |
| `render_*.py`, `titles.py`, `root_icons.py` | PNG renders in `out/img/mac/` |
| `alpha*.py` | Alpha.256 / Gouraud.256 regeneration fits |
| `phrases_diff.py` → `phrases_diff.txt` | per-entry phrase diff |
| `check/` (Rust, uses `payback-formats`) → `check_out.txt` | parses every Mac map, route, vehicles, scores, obj, phrases and motion file |
| `rsrc.py`, `pal.py` | resource fork and LoadRGB32 palette helpers |

Markers: CONFIRMED = byte-level fact or read from the Mac decompile
(`analysis/ghidra_macx`, function name + address); INFERRED = interpretation.

## 0. Summary

Policy (from the lead): the Mac release is **more definitive**. Wherever it differs, the
default is to **adopt the Mac data**. Only clearly Mac-platform changes (menu props that
replace the Amiga computer, publisher text) are marked optional. Where a Mac file is
broken, the table says what adopting it means in practice.

| Category | Differences | Kind | Adopt? |
|---|---|---|---|
| CubeTextures | 225 of 876 retouched (lists byte-identical, only renamed to `*.txt`) | Art pass: darkened walls with a baked top-centre light pool, 12+ repainted textures | **Yes** (pad the truncated `TentRoof` to 4096 B) |
| EffectTextures | `Blood` redrawn smaller/darker; `Blood.dark` dropped | Art change | **Yes**: use the Mac `Blood` day and night (no `Blood.dark`) |
| ObjectTextures | 10 new (PC, monitor, keyboard, disk, movie reel, fir tree) | Menu props + 1 scenery tree | **Yes** (needed by the Mac models) |
| Objects | 49 changed, 2 new: menu props replaced; **people, cars and scenery re-pivoted / resized** | Platform branding + engine-coupled geometry change | Game models **yes**, with matching placement changes (feet/wheels at the origin); Mac menu props optional (branding) |
| General/Alpha.256 | 0.2–0.8 % of entries per table | **Fix**: regenerated with the final U7 palette | **Yes** |
| General/Gouraud.256 | new 8 MiB file | Precomputed cache (built with the CD palette) | **Yes**: read it when present, otherwise compute; differs from a U7-palette build in about 0.25 % of entries (colour 8) |
| General/*.size | 3 of 5 values differ | Self-calibrating progress-bar values | Yes (harmless; the game rewrites them anyway) |
| Icons | 4 title screens repainted (Apex + Virtual Programming logos); the "3 new icons" are Finder junk | Final title art, includes the publisher logo | **Yes** (the logo is branding; an Amiga-title option is optional) |
| Maps | `.map/.route/.vehicles/.mini` identical; `2.thumb` regenerated; `0.scores` shipped but has a bad checksum; no `.script` files | Fix (thumb) / broken placeholder (scores) | **Yes**: thumb. Scores: read it like the Mac does; it fails the checksum, so the result is the defaults |
| Phrases | English/German/French only; 5 level-text edits; Global +205 / −46 / ~131 changed entries | Text fixes + new strings for Mac features | **Yes** (all three languages; other Amiga languages stay as fallbacks) |
| UserMaps | 32 community maps (163 files), all parse | Bonus content | Yes (just data) |
| Videos | new 72 s MPEG trailer (640×480 and 320×240), Amiga FLC dropped | Different video | **Yes** if an MPEG-1 decoder is added; FLC as fallback |
| Sounds, Backgrounds, Motion | identical | — | Nothing to do |
| Replays | Mac ships none (Amiga: 2 demos) | Omitted | Keep the Amiga demos (nothing to adopt) |
| Picture, Saves | empty folders | — | No |
| Junk | `.DS_Store`, `.FBCIndex`, `.FBCLockFolder/` in most folders, `.Spotlight-V100`, `.Trashes`, `TheVolumeSettingsFolder` | Mac OS / Sherlock indexing junk | Ignore |

Provenance (CONFIRMED, `provenance.tsv`): **no** differing Mac file equals any Amiga
generation (CD, Update 7 or OS4). Every difference is new Mac-era content; nothing reverts
to the CD. 182 of the 225 changed cube textures and 48 of the 49 changed models had never
been touched by an Amiga update.

---

## 1. CubeTextures

### 1.1 Texture lists (CONFIRMED)

- `CubeTextureList.txt` and `CubeTextureRemapList.txt` are **byte-identical** to the Amiga
  `.CubeTextureList` / `.CubeTextureRemapList` (LF line ends, same 844 entries, same flags).
  The Mac renamed them because Mac OS X hides dot-files. The Mac game opens
  `PROGDIR:CubeTextures/CubeTextureList.txt` and `.../CubeTextureRemapList.txt` (strings
  `1018a4f9`, `10193015`).
- No flags, no ordering and no remap entries changed, so footstep/grip classes, tiling and
  the Gouraud substitution are as on the Amiga.
- The Amiga dot-files are absent on the Mac.

### 1.2 What changed in the pixels

225 files differ; all are 4096 bytes on both, except `TentRoof.chunky`, which is **4032
bytes on the Mac** (63 rows; see below). The CSV with per-file numbers is
`scratch_assets/cubetex_diff.tsv` (changed texels, RGB distance, colour counts) and
`cubetex_class.tsv` / `cubetex_corr.tsv`.

- **Changed texels:** 817,814 in total. The median file has 3,735 of 4,096 texels changed
  (min 1,937 `Bank5/Bank6`, max 4,091 `Crowd`). 223 of the 225 have more than 2,048
  texels changed.
- **Not a transform:** no flip, rotation, transpose or cyclic shift reproduces any of them
  (`shift.py`). It is also not a palette-index remap: the same Amiga index maps to many
  Mac indices.
- **The palette is unchanged** (see §5: the Mac's `Alpha.256` regenerates exactly from the
  Update 7 palette), so the textures are repainted, not reinterpreted.
- **Systematic darkening with a baked light pool.** 217 of 225 are darker (mean luma −20 on
  0–255) and only 5 are lighter. Averaged over the 189 textures that keep their artwork
  (luma correlation ≥ 0.5), the Mac/Amiga brightness ratio is about **1.44 at the top centre,
  0.95 in the middle, 0.69 at the bottom centre and 0.61 in the bottom corners** (column
  means 0.60 → 0.98 → 0.69; row means 1.07 → 0.67). The texels inside an ellipse hanging
  from the top edge are often unchanged. The effect is a wall lit by a lamp above the
  middle of each tile, with darker edges and base. It shows in nearly every building
  facade (`Building*`, `greywall-*`, `brickwall*`, `Fed*`, `Uni*`, `Hotel*`, `Shop*`,
  `Store*`, `Bank*`).
  - INFERRED: this is an art pass that bakes ambient-occlusion-style shading into the
    facades. It assumes row 0 is the top of a wall face, as on the Amiga. Whether the
    OpenGL path flips V has not been checked in the Mac renderer.
- **Repainted artwork** (luma correlation < 0.5): 36 files. The clearest ones are:
  - `Crowd`: random coloured blobs → a photographic stadium crowd.
  - `Wood`, `WoodDark`, `WoodLight`, `WoodShade`, `WoodSide`: plain vertical boards → a
    weathered plank pattern.
  - `WoodWindow`, `WoodWindowLight`, `WoodWindowDark`: green-framed windows on boards → a
    corrugated shed with dark windows and a planter.
  - `NewWall6`: a white-trim stone window → a brown facade with a decorated frieze.
  - `Fence`: a short picket fence along the bottom → a full-height wooden fence.
  - `TentRoof`: a green scalloped awning → a beige scalloped awning with green stars.
  - `Building4*`, `Building5*`: a new lighting gradient.
  - The `Uni*` / `Fed*` facades: orange brick toned down to brown/grey.
- **Pre-lit variants vs Gouraud targets** (from the remap list): 105 changed files are
  Light/Dark pre-lit variants that are replaced when Gouraud is on; 63 are remap targets
  (the neutral textures Gouraud uses); 34 are mapped to themselves; 23 (`greywall-01..23`)
  are not in the main list at all. So the Mac pass touches both the software (non-Gouraud)
  and the Gouraud look.
- **Usage by the shipped cities:** 9.6 % (map 0), 11.6 % (map 1) and 12.3 % (map 2) of all
  map faces use a retouched texture.
- `TentRoof.chunky` is truncated to 4032 bytes. The Mac loader reads a fixed 0x1000 bytes
  with no size check (`ReloadCubeTexture__FP11TextureInfo` @ `1015ac70`,
  `GetCubeTexture__FUs` @ `1015a9f0`; `#`-prefixed textures read 0x1040). The last row is
  whatever was in the buffer: a Mac data bug. A loader for the Mac data must zero-pad.

Renders:
- `img/cubetex_highlights.png`: 16 selected pairs (Amiga | Mac | changed-texel mask, 3×).
- `img/cubetex_most_changed.png` / `img/cubetex_least_changed.png`: the top 24 and bottom 12
  by changed texels.
- `img/cubetex_all_changed.png`: all 225 pairs at 1×.
- `img/cubetex_vignette.png`: the average Mac/Amiga luma ratio and the "unchanged" mask,
  which show the light pool.

**Verdict.** This is a deliberate art pass, not a technical fix. It adds detail in a few
places (crowd, wood) but makes the whole city darker and moodier. It counts as new content
that the Mac publisher shipped, not a correction. Because the Mac is the definitive release,
**adopt it as the default**: read the Mac `CubeTextures/` when present. Pad
`TentRoof.chunky` to 4096 bytes; the missing last row is a Mac bug, so either zero it or
take that one row from the Amiga file. Keep the Amiga set as a fallback or option.

## 2. EffectTextures

- `Blood` differs (CONFIRMED): the Amiga version is a large bright-red splat (colours
  176–189 plus 0/2/8/45–47; 816 transparent texels). The Mac version is about 35 % smaller
  and much darker, uses only indices {0, 2, 8, 45, 47, 189} and has 2391 transparent texels.
  It is not the Amiga `Blood.dark` either (3220 texels differ). Render:
  `img/effect_blood.png`.
- `Blood.dark` (the Amiga night variant, our `effects.rs` swaps it in on night maps) is
  **absent on the Mac**. The Mac `InitAll__Fv` @ `100632a0` loads only `Blood`, and for the
  German language it loads `Gas` in its place: green-blood censorship, CONFIRMED from the
  string order `Deutsch`, `Gas`, `Blood` at `1018a4e7..1018a4f3`. The darker single `Blood`
  INFERRED serves day and night.
- The other 50 effect textures are identical. The "2 new" files are `.FBCIndex` and
  `.FBCLockFolder` junk.
- Adopt: **yes**. Use the Mac `Blood` for day and night: the Mac has no night swap, so
  `effects.rs` must not require `Blood.dark` (fall back to `Blood`). The German
  `Gas`-for-blood swap is a regional censorship rule; implement it only if we want the
  regional variant.

## 3. ObjectTextures

83 identical, **10 new**. The other 3 "new" files are junk: `.DS_Store`, `.FBCIndex` and
`.FBCLockFolder`. All are 256×128 8-bit (32768 B). Render: `img/objecttextures_new.png`.

| Texture | Content | Used by (CONFIRMED by grepping the Mac `.Obj` files) |
|---|---|---|
| `Disk.back`, `Disk.front` | blue 3.5" floppy, label "PAYBACK saved games" | `Options/Disk.Obj` (load/save menu) |
| `PC.front`, `PC.back` | beige tower PC, front panel and back with fans/ports | `Options/Amiga.Obj`, `Options/Amiganew.Obj` |
| `Monitor.plastics`, `Monitor.screen` | beige monitor shell; screen shows title-screen art | `Amiga.Obj`, `Amiganew.Obj` |
| `Keyboard` | beige PC keyboard | `Amiga.Obj`, `Amiganew.Obj`, `Options/ControlSetup.obj` |
| `Moviereel` | grey film reel | `Options/Moviereel.obj` (replays menu) |
| `tanne`, `tanne.clone` (identical) | fir-tree billboard ("Tanne" = fir) | `Scenery/Tree.Obj` |

Adopt: **yes**. They are additive and needed by the adopted Mac models (`Tree.Obj` uses `tanne`).

## 4. Objects

132 Mac models: 81 identical, 49 changed and 2 new (`Options/Amiganew.Obj`,
`Options/Joystickold.Obj`). The other 5 "new" files are `.DS_Store`/`.FBC*` junk. All 132
parse with our `payback_formats::obj` parser (`check_out.txt`). Full diff:
`scratch_assets/objdiff.txt`. Renders (a crude software rasteriser, no culling or game
lighting): `img/objects_options.png` (menu props) and `img/objects_game.png` (game models,
blue line = model origin height). Only 3 file names differ in case:
`ControlSetup.obj`, `Moviereel.obj` and `World.obj` (Amiga `.Obj`).

### 4.1 Menu props (`Objects/Options/`): platform branding and remodelling

| Model | Amiga | Mac |
|---|---|---|
| `Amiga.Obj` | Amiga desktop + monitor + keyboard (62 v / 92 f; `Amiga*`, `Display` textures, mixed modes) | **beige PC tower + monitor + keyboard** (128 v / 118 f; `PC.*`, `Monitor.*`, `Keyboard`, modes 17/2 only), smaller |
| `Amiganew.Obj` (new) | — | the same PC model at a larger scale (unused draft?) |
| `CD.obj` | flat disc, shine + `Truck.new` (56 v / 72 f) | upright disc, 48 v / 48 f, `Truck.new` only (menu item renamed "Music setup", §8) |
| `Disk.Obj` | sand-textured floppy | "PAYBACK saved games" floppy (`Disk.*`) |
| `Joystick.obj`, `Joystick_multi.obj` | Amiga stick (45 v / 86 f) | red shiny gamepad-style model (149 v / 242 f; ×4 for multi) |
| `Joystickold.Obj` (new) | — | same as the new Joystick at another scale (leftover) |
| `ControlSetup.obj` | Amiga keyboard bits (`AmigaKeyboard`) | PC keyboard (`Keyboard`) |
| `Cup_Silver.obj` | 128 v / 176 f | 324 v / 622 f (smoother; gold and bronze cups are unchanged) |
| `Moviereel.obj` | shine-textured reel | textured `Moviereel` reel |
| `QuestionMarks.obj` | 136 v / 240 f | 180 v / 310 f, extra silver piece and mode-6 faces |
| `World.obj` | 50 v / 80 f globe, flag 0 | 422 v / 840 f smooth globe, flag 1 |

Kind:
- `Amiga.Obj` (PC replaces the Amiga), `ControlSetup.obj` (PC keyboard), `Disk.Obj` and
  `Joystick*` are **platform branding**: adoption is **optional**.
- `Cup_Silver`, `World`, `QuestionMarks`, `Moviereel` and `CD` are higher-poly or
  remodelled props with no platform meaning: **adopt**. "CD" now stands for music rather
  than CD audio, which matches the Mac phrase "Music setup".

Suggested default: adopt everything except the computer/keyboard/joystick/disk props, and
keep those as an option.

### 4.2 Game models: geometry re-pivoted and resized (engine-coupled)

All CONFIRMED by vertex diffs. Topology, UVs and textures are unchanged unless noted.

- **People** (all 23 `People/*.Obj`: `MaleNormal*`, `PedMale1..7*`, `PoliceMale1*`): every
  vertex is moved by a constant −90 in **Y** (−92 for the KKK hood and bazooka models, −100
  for `PedMale4`; the bazooka and minigun models also +2 X). The Amiga Y range −88..+90
  (origin at the hips) becomes −178..0: **the origin moves to the feet**. This matches the
  open "feet-below-floor" issue in `docs/re/people.md` §2.3 (the Amiga draws the feet about
  6.5 h-units below the floor). INFERRED: the Mac fixes that in data.
- **Vehicles**: `Pug`, `Pug Racer` and `Taxi` move −18..−20 in Z, so the bottom sits at
  z = 0 instead of 18–20 below the origin. `Vapour` gets the same shift, loses 4 faces
  (54 → 50) and its 40 `Shine.All` faces become 36 `Cardef` faces (mode 2/3). So the Vapour
  now uses the normal car-paint texture, INFERRED to be recolourable like other cars.
- **Scenery** (lower):
  - `NewTree`, `NewTree2`, `NewTree3`: the crown is lowered by 40–42 (height 242 → 202).
  - `Phone`, `Phone_arrow`: height 102 → 68, plus slightly thinner and new arrow UVs.
  - `TrafficLight`, `TrafficLight1..4`: height 228 → 186 (and new UVs for the base model).
  - `Tree.Obj`: redone as a 20 v / 14 f cross-billboard fir (`tanne`, mode 3 = colour-0
    cut-out). The Amiga had 10 v / 8 f `Leaves`/`Dark leaves`/`Bark`, 411 tall; the Mac
    version is 244 tall.
- **Doors**: `Doors/bus.obj` moves the door quad up (z −26..+28 → −92..0).

INFERRED: these offsets only look right if the Mac engine places models the same way as
the Amiga engine. The people change in particular is the kind of thing a port fixes in
data. I did not find the Mac `.Obj` loader in the decompile (`Obj_Load__FPc` @ `100ad180`
is an unrelated settings loader), so whether the Mac renderer also changed its offsets is
**open**. Check it with the render-agent work before adopting.

Adopt: **yes** (Mac is definitive), with matching placement code. Otherwise people
float about 90 file units and cars about 20.
- **People:** the Mac model origin is at the feet. Remove any hip offset; this also
  resolves people.md §2.3 (feet below the floor) in the Mac's favour.
- **Vehicles:** the origin is at the bottom of the body. Drop any +18/+20 lowering we
  apply, or place the model at its ride height.
- **Scenery:** the shorter trees, phones and traffic lights need no code change.
- **Before switching,** confirm against the Mac renderer that it did not also change its
  placement offsets. If it did, apply exactly the Mac offsets.

## 5. General

| File | Amiga | Mac | Notes |
|---|---|---|---|
| `Alpha.256` | 720896 B (U7, 22 Oct 2001) | 720896 B, differs | **regenerated with the final palette** (below) |
| `Gouraud.256` | not shipped (built at runtime) | 8388608 B | cache of the coloured Gouraud table |
| `64->256.dat`, `RandomNumbers` | — | identical | — |
| `lighting.size` | 0x2F80 | 0x21C4 | progress-bar calibration, rewritten by the game |
| `ReloadObjectTextures.size` | 0x3519C | 0x395F8 | ditto |
| `startup.size` | 0xB6D29 | 0xAF5C3 | ditto |
| `map.size`, `map_reload.size` | — | identical | — |
| `CD_Audio_Handler`, `led`, `Run` | Amiga binaries | absent | platform |

### 5.1 Alpha.256 (CONFIRMED)

The changes are small: 124–510 entries per table (0.2–0.8 %), mostly involving palette
indices 7, 8 and 0. Per table: T0 409, T1 509, T2 510, T3 145, T4 305, T5 162, T6 309,
T7 124, T8 142, T9 343, T10 264 changed entries.

The explanation is exhaustive (`alpha_fit.py`, `alpha_fit2.py`), using the generator from
`docs/re/rendering.md` §3 (nearest colour, metric `25|ΔR|+36|ΔG|+16|ΔB|`, first best wins,
0 → 8):

- The CD palette and the Update 7 palette differ in exactly **one colour: index 8 =
  (7,7,7) on the CD, (0,0,0) in U7**.
- The **Amiga U7 `Alpha.256`** regenerates **65536/65536** for tables 0, 1, 2, 4, 5–8, 9
  and 10 from the **CD** palette. Update 7 shipped a table built with the old palette.
- The **Mac `Alpha.256`** regenerates **65536/65536** for tables 0, 1, 2, 5–8, 9 and 10
  (65529 for T4) from the **U7** palette. Same metric, same 0 → 8 rule, so the metric did
  not change; only the palette input did.
- Table 3 (lighten) matches neither exactly with the documented formula (about 96 % for
  both), so the formula in rendering.md is approximate. Both builds use the same rule.
- Table 10 = `nearest(pal[src] + (dst − 128))` is exact (dst = brightness row). This
  sharpens the rendering.md wording.
- The Mac game palette equals the U7 palette, including colour 65 (CONFIRMED byte for
  byte, §15; first inferred from the regenerated entries). The Mac app's `clut` 128
  resource ("8-bit Amiga") differs from U7 only at index 65 (0xB37B4B vs 0xB37347); it is
  a resource, not the game palette. The game palette sits in the PEF's pattern-initialized
  data section, so it shows only after unpacking that section (§15).

Kind: **fix** (consistency with the shipped palette). Adopt: **yes**. Either read the
Mac `Alpha.256` or regenerate the tables from the game palette. The port already reads
table 1 for the night remap (rendering.md §3), so the Mac file removes the stale colour-8
entries.

### 5.2 Gouraud.256 (CONFIRMED)

- Layout `[R/8][G/8][B/8][colour]` = 32×32×32×256 bytes:
  `entry = nearest(pal[c] + 8·i − 128 per channel)`. The generator and loader are
  `GetGouraudTable__Fv` @ `10084400`: it allocates 0x800000, reads
  `PROGDIR:General/Gouraud.256`, otherwise computes it with `ClosestMatch__Fiii`
  ("Unable to open …, calculating...") and writes it back.
- It reproduces 100 % (20,000 random samples) with the **CD** palette and 99.75 % with the
  U7 palette (all mismatches involve colour 8). So the shipped file is a stale cache from an
  older build, unlike `Alpha.256`.
- Adopt: **yes**. Read `General/Gouraud.256` when present, as the Mac does, and compute it
  only when it is missing. Note that it was built with the CD palette (colour 8 = 7,7,7),
  so about 0.25 % of entries differ from a U7-palette computation; that is what the Mac
  game shows. Our `postfx.rs` palette snap currently computes from the game palette.

### 5.3 `.size` files

The progress bar rewrites these when the measured byte count differs ("Wrote updated
file sizes", `frontend_audio.md`). The Mac values are the port's own measurements: a
platform artefact. Adopt: yes. They only set the loading-bar scale, and the original rewrites them after one run anyway.

## 6. Icons

- 452 identical, 4 differ: all four title screens. The "3 new icons" are `.DS_Store`,
  `.FBCIndex` and `.FBCLockFolder` junk.
- Title screens (CONFIRMED; 95 % or more of bytes differ): new artwork. The image is
  desaturated with a radial zoom blur and an orange flaming "PAYBACK" logo instead of the
  blue one, and adds an **Apex Designs** logo bottom-left and a **"10110 VP" Virtual
  Programming** logo bottom-right. Same formats and sizes (320×200, 320×256, 640×480 8-bit;
  `.w3d` 256×256 RGB24). Renders:
  - `img/icons_TitleScreen.png`
  - `img/icons_TitleScreen_medium.png`
  - `img/icons_TitleScreen_big.png`
  - `img/icons_TitleScreen_w3d.png`
- Kind: final title art, with the Virtual Programming logo as publisher branding. Adopt:
  **yes** by default (read the Mac `Icons/TitleScreen*`). An "Amiga title" option is
  optional.

## 7. Maps

- `0/1/2.map`, `.route`, `.vehicles` and `.mini` are **identical**. They parse with our
  parsers (1024 route nodes; vehicles: city flags and 4/4/5 parked).
- `2.thumb` (city 2 selection picture) is regenerated: 32507 of 65536 texels differ. The
  Amiga thumbnail shows the park/grass areas as red/black; the Mac one shows green grass
  and the correct day textures. Render: `img/maps_2_thumb.png`. Kind: fix. Adopt: yes
  (cosmetic).
- `0.scores` (Mac only, 1024 B): 30 entries "Travis" with score 0xBFFFEFA0 (−1,073,745)
  and stored checksum 0x006A5F60. **It fails the checksum.** Our
  `payback_formats::scores::HighScores::parse` returns `None`; the correct sum would be
  0xBA3B412C. The Mac algorithm is the same as the Amiga one (`CalcHiscoreChecksum__Fv` @
  `1009e7a0`, seed 0x5DC1DCD1, 7 name words + score, inverted; `LoadHiscores__Fi` @
  `1009e8a0` falls back to `InitHiscores__Fv` @ `1009e6a0`, "Travis" 500000·(t+1) halving).
  So the Mac game ignores the file and uses the defaults. It is a stray development file.
  Adopt: **yes, in the Mac way**. Read `Maps/<n>.scores`, reject a bad checksum (our
  parser already does) and fall back to the defaults. The visible result is the default
  table. Do not "repair" the checksum: that would show −1,073,745 scores.
- **No `Maps/*.script` on the Mac.** The level logic is compiled into the executable
  (`InitScript_00__Fv` … `InitScript_21__Fv`, `InitScript_Random__Fv` @ `101291c0..10129e50`).
  This agrees with `script_api.md`: the U7 scripts are built in and the shipped `.script`
  files are dead data. `InitScript_Random` is Mac-only and goes with the new rampage
  phrases (§8).

## 8. Phrases (data diff; the UI meaning is covered elsewhere)

Languages: the Mac ships **English, Deutsch and Francais** only (the Amiga has 15). The
file names use ASCII `Francais` instead of Latin-1 `Français`.

- The French level files (00–21) are byte-identical to the Amiga ones.
- The German level files are identical.
- The English level files differ as follows. Level 01, 10 and 20 only lose their
  `# Language Codes` comment header; no entries change.

**English level phrases, entry changes (CONFIRMED, `phrases_diff.txt`):**

| File | # | Amiga | Mac |
|---|---|---|---|
| 00 | 0, 86 | `<Press RETURN/YELLOW/SQUARE to get in and out of vehicles.>` | `<RETURN/YELLOW/SQUARE are the default controls to get in and out of vehicles.>` |
| 11 | 24 | `<Press return to take off and land.>` | `<Use the same controls you use to get in and out of vehicles to take off and land.>` |
| 11 | 83 | "…to launch **ac** RC…" | "…to launch **an** RC…" (typo fix) |
| 21 | 69 | "…do your **f\*cking** shopping first." | "…do your **fecking** shopping first." |

**Global.English.phrases:** 498 → 657 entries; +205, −46, 131 changed. The German file
changes in parallel (368 differing numbers). By number range:

- **Added 1400–1499**: rampage/phone jobs ("ANSWER THE PHONE", "Welcome to %s. Prepare to
  rampage!", steal %s in %s, gang kills, race "Go!", cop cull, "SURVIVAL!", "NEW HIGH
  SCORE!"). None of these strings exist in the Amiga data or executables.
- **Added 1535–1547**: extra key names (Home, End, Insert, Page Up/Down, L/R-Ctrl, Shift,
  Pause, Menu, L-/R-Start, Prnt Scrn).
- **Added 1800–1838**: vehicle names with articles ("a Mundaneo" … "a Vapour GT"), city
  areas ("the North-East of the city"), city names (Freedom City, Los Francos City, Corona
  City). These are the `%s` arguments of 14xx.
- **Added 1840–1992**: map-editor UI (Surface/Time/Weather/Slope, Load/Save/New/Quit,
  Cube/Face/Route/Object, scenery names "Traffic Light" … "Lamp", the special-parameter
  list "0: Empty … 25: Undefined", "Overwrite map: Are you sure?") and storage messages
  ("Out of storage space!", "Choose file to delete"). Also "use mouse/stylus to choose
  option below", "Lap %d of %d", "CHOOSE TEXTURE" and "Gamma correction:".
- **Added 2146–2147**: "Interference : Off/On".
- **Changed**:
  - 2055 "Police aggressiveness:" → "Difficulty:".
  - 2094/2131 "CD audio setup" → "Music setup".
  - 2138–2145: re-indented device/playback lines.
  - 1570/1571: colon added.
- **Credits block** 2206–2281 rewritten: "@Alex Curylo" added, the localisation list
  trimmed to French/German translators, ending "(c) 2002 Apex Designs / Published by
  Virtual Programming Ltd / www.vpltd.com". The Amiga's 2282–2326 (Polish, Serbian,
  Slovenian, Spanish, Swedish and Slovak translators, "(c) 1998-2001") are **removed**.
- **Demo nag** 2403–2472 rewritten for the Mac: "Loads of huge cities", "English, German
  and French language support", "High quality soundtrack", buy at www.vpltd.com, "call
  your local Mac games supplier". The Blittersoft / "Please support Amiga developers!"
  lines are gone.

Kind:
- Level text: fixes (control-neutral hints, typo, profanity softened).
- Global: new strings for features the Mac phrases file knows about. INFERRED: the
  "stylus" and "storage space" strings suggest a codebase shared with a handheld/console
  port; whether the Mac binary uses all of them is for the UI agent.
- Credits and demo: platform/publisher.

Adopt: **yes**. Read the Mac `Phrases/` for English, Deutsch and Francais, level and
Global files. Keep the Amiga-only languages (Czech, Español, Italiano, Nederlands, Norsk,
Polski, Slovak, Slovensko, Srpski, Suomi, Svenska) as extra choices from the Amiga data.
- Their Global files lack the 14xx/18xx/19xx numbers, so anything we build on those
  numbers must fall back to English.
- Visible menu changes: "Difficulty:" (2055) and "Music setup" (2094/2131).
- The Mac credits replace the Amiga credits.

## 9. UserMaps

163 files = **32 maps** (plus junk). Every `.map` parses with `payback_formats::map::Map`;
every `.route` parses (1024 nodes) and every `.vehicles` parses (`check_out.txt`).
Thumbnail sheet: `img/usermaps_thumbs.png`.

| Map | Files | `.map` bytes | Notes |
|---|---|---|---|
| AlabastraCity | map mini thumb route vehicles textures(4 B) | 232013 | variant 1 |
| AllegroCity | all six | 299295 | |
| Amigaliens_Metro | no vehicles | 259061 | |
| Amigaliens_Metro_night | map mini thumb route | 259255 | night |
| Amirace | all six (textures 464580 B) | 350535 | 8086 custom-texture faces |
| D-Town / D-Town_Night | all six | 292291 / 305205 | custom textures |
| Death_Match | no textures | 213933 | |
| Desert_Deathmatch | all six (207404 B tex) | 243801 | |
| DestraCity / DestraCity_Night | all six | 342273 / 341629 | |
| End_of_the_World | all six | 293033 | |
| Gods_Island | no textures | 317269 | variant 2 |
| House Park 2 Night | map mini thumb (no route) | 271049 | night |
| Hyrtola offroad | map mini thumb route | 242217 | |
| Isla Nublar | all six | 255085 | |
| Kozie_Klefki | no route | 260453 | |
| Little_Chicago, Little_Indy, Little_Paris | map mini thumb route | 297819 / 249171 / 303647 | |
| MaluchRejs | all six | 217121 | night, variant 2 |
| MultiplayerTournament | no route | 253129 | variant 2 |
| Real Park | all six | 271897 | |
| RocketCarHeaven | all six | 239529 | "AMIGA" logo track |
| Tankwarfare | no route | 275967 | |
| The Killing Zone | map mini thumb route | 214829 | also on the Amiga (identical) |
| TheBigTrack | all six | 301601 | |
| ThumbCity, Tribe_City, VerbalCity | all six | 238625 / 291057 / 273479 | |
| Woods | map mini thumb route | 295779 | |
| test | map mini thumb route | 214829 | = Amiga `The Killing Zone.map` byte for byte; the Amiga `test.map` differs in 40 bytes, and its `.thumb`/`.mini` differ too |

Custom textures (`.textures`, texture numbers ≥ 2000) are used by 11 maps. A 4-byte
`.textures` is an empty set.

Kind: bundled community content (several are Amiga-community maps). Adopt: **yes**. They are
plain data in formats we parse. The user-map menu just needs to list them; our parser
handles them unchanged.

## 10. Videos

| | Amiga `intro.video` + `.audio` | Mac `payback_high_mpeg1.mpg` | `payback_high_mpeg2.mpg` | `payback_low_mpeg1.mpg` |
|---|---|---|---|---|
| Codec | FLC (Animator Pro) + raw 8-bit PCM | MPEG-1 video + MP2 audio | MPEG-2 video (4:2:2) + MP2 | MPEG-1 + MP2 |
| Size | 23.6 MB + 0.6 MB | 45.8 MB | 38.4 MB | 12.8 MB |
| Resolution | 320×200, 8-bit palette | 640×480 | 640×480 | 320×240 |
| Frames / rate | 450 at 66 ms ≈ 29.7 s | 1800 frames, 25 fps effective (stream says 50), 71.98 s | 25 fps, 71.98 s | 71.98 s |
| Video bitrate | — | 4.8 Mbit/s | (VBR) | 1.2 Mbit/s |
| Audio | 8-bit mono | 48 kHz mono 192 kbit/s | same | same |

- Content (renders: `img/video_mac_high_contact.png`, one frame every 3 s; Amiga:
  `out/img/intro/intro_contact.png`): a **different film**.
  - The Amiga intro is 30 s of in-engine city fly-through ending in an explosion and the
    flaming logo.
  - The Mac video is a 72 s live-action / composited trailer: night road, headlights, a
    time-coded camera feed, glitchy "PAYBACK" lettering, shockwave and explosion, ending
    on the same orange-fire logo as the new title screen.
- Only `:Videos:payback_high_mpeg1.mpg` is referenced by the Mac binaries (Pascal string
  next to `1018faf6`, played through QuickTime: `UQuickTime`, `InitMovie__Fv`). The
  `mpeg2` and `low` files are unreferenced alternatives. The Mac has no `intro.video` or
  `intro.audio`.
- Kind: new content in a higher-resolution format. Adopt: **yes**: play
  `Videos/payback_high_mpeg1.mpg`, the only file the Mac game references, when present.
  This needs an MPEG-1 video + MP2 audio decoder (e.g. an ffmpeg dependency or a pure-Rust
  MPEG-1 decoder) next to `payback_formats::flc`. Fall back to the FLC intro when the
  Mac file or the decoder is unavailable.

## 11. Sounds, Backgrounds, Motion

- `Sounds/`: all 74 samples **identical**; nothing new (the 2 "new" files are
  `.FBCIndex` / `.FBCLockFolder`).
- `Backgrounds/`: `Clouds`/`Stars` `.chunky` and `.chunky2` identical.
- `Motion/Human.motion`: identical; parses with `payback_formats::motion`.

## 12. Replays, Picture, Saves

- `Replays/`: Mac ships **none**; the Amiga ships two demo replays ("Race for the prize",
  "Tanks a lot"). The Mac `InitMovies__Fv` @ `10084960` scans `PROGDIR:replays` the same
  way.
- `Picture/`: an empty folder (only `.DS_Store`); no binary references it.
- `Saves/`: empty on both.

## 13. Other files

- `Payback Read Me` (TEXT/ttxt, CR line ends, Mac Roman):
  - "Payback, Macintosh Edition, Version 1.0, October 30, 2002".
  - Requirements: Mac OS 9.0+ or OS X; 333 MHz; 64 MB RAM; DrawSprocket 1.7.6;
    InputSprocket 1.7.3; QuickTime 4.0.
  - Joystick troubleshooting mentions `HID.bundle`.
  - Credits: Mac programming by Alex Curylo; thanks to Paul Lesurf, Mark Hinton and James
    Daniels.
- `Manuals/Payback English|French|German.pdf`: manuals (not compared here).
- Volume root: about 40 files whose names are made only of tabs and spaces, with empty
  data forks and a 9134-byte resource fork holding a custom icon (`icns`/`icl8`/`ICN#`
  -16455, Finder flag 0x400). They sit on a 32-pixel grid and together form a blue
  "payback" artwork mosaic in the CD window (`img/volume_root_icons.png`, assembled from the
  HFS Finder positions with `root_icons.py`). Root `Payback (OS 9)` and `Payback (OS X)` are
  aliases (`adrp`/`PAYB`). Decorative; ignore.
- Junk to ignore everywhere: `.DS_Store`, `.FBCIndex` and `.FBCLockFolder/.FBCSemaphoreFile`
  (Sherlock find-by-content index, up to 2.6 MB in CubeTextures), `.Spotlight-V100`,
  `.Trashes`, `TheVolumeSettingsFolder`, `____HFS+ Private Data`.

## 14. Loading the Mac data (preferring the Mac files)

The plan this section was written as; the port has since implemented it
(`crates/payback/src/data.rs`, `locate.rs`). Code reads relative paths through
`GameData::read`.

1. **Two roots.** An Amiga root (`Payback.exe`) and a Mac root, found via
   `PAYBACK_DATA` / `PAYBACK_MAC_DATA`, `--data` / `--mac-data <dir>`, `data/amiga` /
   `data/mac` next to the exe, `extracted/game` / `extracted/mac/Payback`. The Mac root is
   detected by `CubeTextures/CubeTextureList.txt` (or a Mac application next to
   `CubeTextures`; `data::is_mac_root`). Either root alone is enough: without the Amiga
   data the Mac folder becomes the main root and every Mac/Amiga setting resolves to the
   Mac content (`GameData::mac_variant`); `--mac-only` ignores the Amiga data.
2. **Palette:** the Mac game palette is the U7 palette (§15). The port takes it from the
   Amiga `Payback.exe` when present, else from `Payback (OS X)` / `Payback (OS 9)`
   (`palette::find_in_executable` unpacks the PEF data section, `pef.rs`).
3. **Renamed lists:** map `CubeTextures/.CubeTextureList` → `CubeTextureList.txt` and
   `.CubeTextureRemapList` → `CubeTextureRemapList.txt` (identical contents).
4. **Mac files to prefer** (they differ from the Amiga ones):
   - `CubeTextures/*`, `EffectTextures/Blood`, `ObjectTextures/*` (10 new)
   - `Objects/**` (with the placement changes in §4.2; the branding props optional)
   - `General/Alpha.256`, `General/Gouraud.256`, `General/*.size`
   - `Icons/TitleScreen*`
   - `Maps/2.thumb`, `Maps/0.scores` (rejected by checksum → defaults)
   - `Phrases/*.{English,Deutsch,Francais}.phrases`
   - `UserMaps/*`
   - `Videos/payback_high_mpeg1.mpg`
5. **Fallbacks when the Mac lacks a file:**
   - `EffectTextures/Blood.dark` → Mac `Blood`. Do not fall through to the Amiga
     `Blood.dark`, so the Mac look stays consistent.
   - `Videos/intro.*` → the MPEG, or the Amiga FLC when no decoder is available.
   - `Replays/*`, the Amiga-only languages and `General/CD_Audio_Handler` → Amiga root.
   - Language names: accept `Francais` and `Français`. The scan in `frontend/mod.rs` /
     `hud/mod.rs` should merge both roots and deduplicate by language.
6. **Short reads:** pad `TentRoof.chunky` (4032 B) to 4096. The loader should pad any
   cube texture shorter than 4096 (0x1040 for `#` textures).
7. **Model placement:** switch people, vehicle and door placement to the Mac pivots (§4.2)
   when the Mac objects are loaded. If both sets can be active, key this off which root
   the model came from.
8. **Case and junk:** match names case-insensitively (`.obj` vs `.Obj`). Skip `.DS_Store`,
   `.FBC*`, `.Spotlight-V100`, `.Trashes` and similar when listing directories (UserMaps,
   Replays, Phrases).

Platform-specific items that stay optional: the PC/keyboard/joystick/disk menu props and
the Virtual Programming credits and demo text. A "classic Amiga look" switch could restore
the Amiga textures, models and title screens.

## 15. The game palette in the Mac binaries

CONFIRMED (`Payback (OS X)`, `analysis/ghidra_macx`): the palette is a static
graphics.library `LoadRGB32` table compiled into the program, as on the Amiga, not a file
or a resource. Nothing builds it at start-up.

- TOC slot `0x1017cef0` points at the table: data-section offset `0x10030`, i.e.
  `0x1018b470` with the data section at `0x1017b440` (directly after the 0x17b440-byte code
  section at `0x10000000`). Layout as on the Amiga: `0x01000000` (256 colours from 0),
  256 × 3 longwords with the component in the top byte, a 0 terminator.
- `DoSetupColourLookup` @ 10084640 (called once from `InitAll` @ 100632a0) copies the
  table into the per-channel int arrays (TOC `0x1017c6d0` / `d4` / `d8`, read as B / G /
  R) and the packed `0xffRRGGBB` array (`0x1017c6cc`). `ClosestMatch` @ 10083dc0,
  `CalcAlpha` @ 10083ec0 and the `Gouraud.256` builder read those arrays.
- `AddBrightness` @ 10083620 (68k `FUN_0003e7e6`) and `SetBrightness` @ 100839e0
  (68k `FUN_0003eb0c`) rewrite the table's top bytes from the channel arrays plus the
  flash or brightness amount and pass it to the emulated `_LoadRGB32` @ 100fc000. These
  are the only two `LoadRGB32` callers; no code writes the base colours.
- The table lives in the PEF's data section, which is stored **pattern-initialized**
  (section kind 2, 79985 bytes in the file expanding to 102698). The expansion
  byte-code splits the table into runs, so no search of the raw file (raw RGB, LoadRGB32
  longwords, ColorSpec) finds it. Unpacked, the section holds **four identical copies**
  (offsets `0xf160`, `0x10030`, `0x10e08`, `0x16760`); `Payback (OS 9)` likewise
  (`0xf278`, `0x10148`, `0x10f20`, `0x16898`). The low 24 bits of each component vary
  (`1f ff ff ff`, `17 77 77 77`, ...), as in the Amiga copies.
- **All 256 colours equal Update 7's `Payback.exe` table** in both Mac builds. The CD
  `Payback.exe` differs from both only at colour 8 (7,7,7 instead of 0,0,0; §5). The
  map editors' tables (`MapEditor (OS X)` at `0xdbb8` / `0xed58`) are a different
  palette; the game does not use them.

Port: `crates/payback-formats/src/pef.rs` unpacks PEF sections;
`palette::find_in_executable` searches them when given a PEF, so the Mac application
serves as the palette source when the Amiga data is missing (`data.rs`;
`tests/real_data.rs` `mac_palette_matches_update7`). Since both sources are identical,
the Amiga `Payback.exe` stays the source when present.
