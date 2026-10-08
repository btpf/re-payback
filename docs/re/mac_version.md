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
| UserMaps | 4 | 3 | 163 | 163 files = 32 bundled community maps (31 playable; one is the editor's test map) |
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
(git-ignored, local only; regenerate with `python analysis/mac/build_report.py`).
Headlines:
- The Mac game code is the same C source as the Amiga OS4 build.
- There's no higher frame rate: a 30 Hz tick, a 30 fps cap in game and 60 fps
  menus.
- About 20 bug fixes.
- New random phone missions in Rampage.
- Pedestrians keep to the pavement.
- People models have their origin at the feet.
- 225 cube textures were repainted.
- `Alpha.256` is rebuilt from the final palette.
- A 72 s MPEG intro film.
- The sound samples are identical. Per-area research notes are in `analysis/mac/*.md`.
Confirmed facts get merged into the relevant `docs/re/*.md` notes, citing the
Mac function name and address (e.g. `drawCubeTop_None` @ `100c8160`).

## Decisions (2026-10-08)

How the port treats the Mac differences. These override the recommendations in
`mac_differences.html` where they disagree.

Principle: where the Mac and Amiga content differ and both are reasonable, offer
both through an in-game setting. Default to the Mac version, since it is the more
definitive copy. Graphics variants go in the Graphics menu (`frontend/display.rs`),
the sound variant in the sound options, and the random missions in the Rampage
setup. Settings are saved with the other options. Render-only and audio-only
settings are not stored in replays. Settings that change the simulation (random
missions) must be recorded in the replay header, or playback would diverge.

| Topic | Decision |
|---|---|
| Vert (slope 2) lighting height quirk (rendering §3.3, port fix L2) | **Not reproduced.** The original adds h1 twice for Vert cubes in the lighting pass (Amiga `FUN_010766c8` ~01076cf4, Mac `DoLightSourcing` @ 1015bd18). The port keeps the correct heights. The quirk stays documented as an intentional departure. |
| Payback Setup dialog (`CPaybackPreferences::EditPreferences`) | **Not implemented.** Everything it configures (window/fullscreen, resolution, renderer, rotating view, Gouraud, motion blur, FX) belongs in the in-game Graphics menu. |
| Blood effect texture | **Setting:** Mac (one smaller, darker `Blood`, day and night) or Amiga (`Blood` plus the `Blood.dark` night swap). Default: Mac. |
| Menu props (`Objects/Options/*`, PC vs Amiga, remodelled cup/world/CD) | **Setting:** Mac or Amiga props. Default: Mac. |
| Intro film | **Setting:** Mac MPEG film or Amiga FLC. Default: Mac when the film is present, otherwise FLC. |
| Title screens (`Icons/TitleScreen*`) | **Setting:** Mac or Amiga art. Default: Mac. |
| Shorter phones, traffic lights and tree crowns (scenery models) | **Setting:** Mac or Amiga scenery heights. Default: Mac. |
| 1. Continuous CD music wrap | **Stop after the last track, like the Mac.** |
| 2. One-shot sound cut-off (~65 %) | **Setting.** The cut is driven by recorded ticks, so replays sound the same. Default: off (play to the end). |
| 3. Quiet-voice +6 dB | **Ignored** as a platform artefact. |
| 4. Mac game models and placement | **Adopt the Mac models.** People use per-model offsets (origin at the feet). Vehicles and doors follow after their placement code is checked. |
| 5. Mac textures | **Mac cube textures are the default.** An "Amiga look" setting brings back the Amiga set. |
| 6. Mac MPEG intro (judgement call) | **Decode at run time** with the vendored `pl_mpeg` (single C file, MIT, built with `cc`). Players need no ffmpeg and the extractor stays pure Python. Falls back to the FLC when the film is missing. |
| 7. Ghidra re-export with jump-table overrides (judgement call) | **Yes.** Fold the SwitchOverride pass into `tools/ghidra/` and re-export `analysis/ghidra_macx` and `analysis/ghidra_mac9`, so the level scripts, `SetTrigger_Long`, input and movie code are in the export. The Amiga exports stay as they are. |
| 8. Random phone missions | **Setting** in the Rampage setup, recorded in replays. Default: on, the Mac behaviour. |
| 9. `Gouraud.256` (judgement call) | **Compute it** from the loaded palette, as the port does today, and ignore the shipped file. The file was built from the stale CD palette. Computing works for either texture set, and the file differs in only ~0.25 % of entries (all colour 8). |
| 10. German "Gas" in the blood slot | **Setting, default off.** |
| 11. Mac menu props and title art | **Offer both** (see the settings above). |
| 12. Phrase 1799 on a Rampage level end | **Keep phrase 2023 "Level complete!".** 1799 exists in no phrase file. |
| 13. Unchecked areas | **Follow-up pass after the re-export (7).** It covers effect-size arguments past the 8th, `GetNextWeapon`, input and movie code, and anything else in the recovered jump-table bodies. |
