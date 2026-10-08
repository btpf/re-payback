# Mac release (PaybackMac.toast)

Working notes for the Mac OS 9 / Mac OS X release of Payback. It came out after
the last Amiga update (Update 7) and is treated as the more definitive copy: where
it differs from the Amiga builds, prefer its behaviour unless it is clearly a
Mac-platform artefact. The Amiga decompiles (`analysis/ghidra_68k`,
`analysis/ghidra_os4`) and all Amiga RE notes stay as they are; the Mac build is
an additional cross-reference.

## Source and extraction

- `Payback Game + Updates/PaybackMac.toast`: a classic **HFS** volume (`BD`
  signature at 0x400, volume name `PaybackMac`), not ISO 9660.
- `python tools/extract_mac.py` (needs `pip install machfs`) writes
  `extracted/mac/`: data forks under their HFS paths, resource forks as
  `<name>.rsrc`, and type/creator codes in `extracted/mac/_finder_info.tsv`.
- Layout: `Payback/` holds the game (`Payback (OS 9)`, `Payback (OS X)`, the
  map editors, and the data folders `Backgrounds`, `CubeTextures`,
  `EffectTextures`, `General`, `Icons`, `Maps`, `Motion`, `Objects`,
  `ObjectTextures`, `Phrases`, `Picture`, `Replays`, `Saves`, `Sounds`,
  `UserMaps`, `Videos`, `HID.bundle`); `Manuals/` holds the English, French
  and German PDF manuals.

## Executables

| File | Format | Notes |
|---|---|---|
| `Payback (OS X)` | PEF (CFM) PowerPC, Carbon | "Payback (Carbon version) requires OpenGL to run". CodeWarrior traceback names survive: 2248 of 2455 functions are named with mangled C++ signatures (e.g. `drawCubeTop_None__FP4Cubeiiii`, `do_Person__Fv`, `CheckForCollisions__Fi`). PowerPlant UI classes. |
| `Payback (OS 9)` | PEF (CFM) PowerPC, Classic | 609 of 2448 functions named (mostly imports). Same game code as OS X. |
| `MapEditor (OS X)`, `Map Editor (OS 9)` | PEF | The map editor. |

The Mac port keeps the Amiga code structure. It emulates the Amiga APIs it
used: `OpenScreenTags`, `LoadRGB32`, `MatchFirst`, `WritePixelArray8`,
`AllocMem`, and so on. Warp3D is reimplemented on OpenGL (`W3D_*`, `PauseGL`,
`convert16BitTextureARGB2RGBA`). It adds a sound library (`CMS*`), HID input
(`HID*`, `HID.bundle`), DrawSprocket and InputSprocket.

## Decompiling

Ghidra headless in the `payback-re` Docker image (see `Dockerfile`). The PEF
loader picks the wrong language by default (`PowerPC:BE:64:VLE`), so force
32-bit PowerPC:

```sh
docker run -d --rm -v "D:/Payback Reverse Engineered:/work/impl" payback-re sh -c \
  "mkdir -p /tmp/proj && analyzeHeadless /tmp/proj p_macx \
   -import '/work/impl/extracted/mac/Payback/Payback (OS X)' \
   -processor PowerPC:BE:32:default -scriptPath /work/impl/tools/ghidra \
   -postScript ExportAll.java /work/impl/analysis/ghidra_macx"
```

Output: `analysis/ghidra_macx/` and `analysis/ghidra_mac9/`, each with
`decompiled.c`, `functions.tsv` and `strings.tsv`, in the same format as the Amiga
exports.

## Data differences (first pass, `analysis/mac_datadiff.py`)

Compared against the merged Amiga data (`extracted/game`), by directory: same /
differ / Mac only / Amiga only.

| Directory | Same | Differ | Mac only | Notes |
|---|---|---|---|---|
| CubeTextures | 651 | 225 | 4 | Same sizes; retouched textures? Text copies of the texture lists |
| Objects | 81 | 49 | 7 | Changed models |
| ObjectTextures | 83 | 0 | 13 | New textures (e.g. `disk.back`/`disk.front`) |
| EffectTextures | 50 | 1 | 2 | `Blood` differs |
| General | 4 | 4 | 4 | `Alpha.256` differs; Mac ships `Gouraud.256` |
| Icons | 452 | 4 | 3 | Title screens differ |
| Phrases | 6 | 8 | 10 | Text changes; French phrases; fewer languages than Amiga |
| UserMaps | 4 | 3 | 163 | Many bundled user maps |
| Videos | 0 | 0 | 4 | MPEG-1/2 intro, high and low quality |
| Sounds | 74 | 0 | 2 | Samples identical |
| Maps | 14 | 1 | 4 | `2.thumb` differs; `.scores` files shipped |

## Icon

The game's exe and window icon is the Mac app's `icns` 128 family (128/48/32/16
with 8-bit masks; `payback_formats::icon::mac_app_icon`), embedded by
`crates/payback/build.rs` from `extracted/mac`. It falls back to the Amiga
`Payback.info`.

## Findings

The full comparison report is `mac_differences.html` in the repository root
(git-ignored, local only). Per-area research notes are in `analysis/mac/*.md`.
Confirmed facts get merged into the relevant `docs/re/*.md` notes, citing the
Mac function name and address (e.g. `drawCubeTop_None` @ `100c8160`).
