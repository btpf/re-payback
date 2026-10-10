# Sources: media, versions and binaries

Payback (Apex Designs / James Daniels, Amiga 2001) is a top-down 3D
GTA-style game: a 128×128 voxel-like city rendered in software (or Warp3D),
missions driven by per-level scripts, vehicles, pedestrians and police.

## Media

| Item | Use |
|---|---|
| `Payback.iso` | CD release; base data |
| `PaybackUpdate7.lha` | last official update; **the version we target** |
| `paybackupdateOS4Native.lha` | community AmigaOS 4 PPC port (ELF) |
| `Warp3D-42.lha` | Warp3D driver; not needed |
| `PaybackMac.toast` | Mac OS 9 / OS X release (HFS image); the reference for behaviour; enough on its own to run the port |

`python tools/extract/extract.py` unpacks the Amiga media into `extracted/` and builds
the merged install in `extracted/game/`; `python tools/extract/extract_mac.py` reads
the Mac image into `extracted/mac/`.

## Amiga binaries

All builds are compiled C (not hand-written assembly) and stripped of
symbols, but full of debug strings naming functions (`LoadMap`,
`Obj_GetX`, `ReloadPhrases`, `AddSound_script`, ...).

| Binary | Format | Notes |
|---|---|---|
| `Payback.exe` (Update 7) | 68k hunk, 25 hunks | matches our data; primary reference |
| `PaybackWOS.exe` | WarpOS PPC hunk | same code base, PPC |
| `PaybackOS4` | ELF32 PPC, static | same code base; best decompiler output |

The Mac OS 9 / OS X release (`PaybackMac.toast`, extracted by
`tools/extract/extract_mac.py`) is described in [`re/mac_version.md`](re/mac_version.md);
it is treated as the more definitive copy. How the binaries are decompiled is in
[`decompiling.md`](decompiling.md).

## A few landmark functions

| Address | Build | Role |
|---|---|---|
| `FUN_01079300` | OS4 | LoadMap |
| `FUN_0106b7a0` | OS4 | GetCubeTexture (lazy-loads `cubetextures/%s`) |
| `FUN_0104feb0` | OS4 | screen setup; loads `.CubeTextureList` |
| `FUN_010bdab0` | OS4 | loads `64->256.dat`, looks up named object textures |
| `FUN_010024fc` | OS4 | Warp3D texture conversion; reads palette at `0x011490b8` |
| `FUN_000d7874` | 68k | loads all object models; scenery table `0x348000` |

The full Mac name ↔ OS4 ↔ 68k table is [`xref/symbols.tsv`](xref/symbols.tsv)
(`python tools/re/fn.py --sym NAME_OR_ADDRESS`).
