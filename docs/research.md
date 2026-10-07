# Research notes

Payback (Apex Designs / James Daniels, Amiga 2001) is a top-down 3D
GTA-style game: a 128×128 voxel-like city rendered in software (or Warp3D),
missions driven by per-level scripts, vehicles, pedestrians and police.

## Source material

| Item | Use |
|---|---|
| `Payback.iso` | CD release; base data |
| `PaybackUpdate7.lha` | last official update; **the version we target** |
| `paybackupdateOS4Native.lha` | community AmigaOS 4 PPC port (ELF) |
| `Warp3D-42.lha` | Warp3D driver; not needed |

`python tools/extract.py` unpacks all of it into `extracted/` and builds the
merged install in `extracted/game/`.

## Binaries

All builds are compiled C (not hand-written assembly) and stripped of
symbols, but full of debug strings naming functions (`LoadMap`,
`Obj_GetX`, `ReloadPhrases`, `AddSound_script`, ...).

| Binary | Format | Notes |
|---|---|---|
| `Payback.exe` (Update 7) | 68k hunk, 25 hunks | matches our data; primary reference |
| `PaybackWOS.exe` | WarpOS PPC hunk | same code base, PPC |
| `PaybackOS4` | ELF32 PPC, static | same code base; best decompiler output |
| `Maps/*.script` | 68k hunk | mission logic, loaded per level |

Decompiled output (not committed; regenerate as below):

- `analysis/ghidra_os4/` — PaybackOS4: `decompiled.c`, `functions.tsv`,
  `strings.tsv` (each string lists its referencing functions).
- `analysis/ghidra_68k/` — Update 7 `Payback.exe`, flattened by
  `tools/hunk.py --flat` (hunks at 0x10000, 4 KiB aligned; layout in
  `analysis/flat/Payback68k.bin.map`).

```sh
docker build -t payback-re .
python tools/hunk.py --flat extracted/game/Payback.exe analysis/flat/Payback68k.bin
docker run --rm -v "$PWD:/work/impl" payback-re analyzeHeadless /tmp/p p68k \
  -import /work/impl/analysis/flat/Payback68k.bin -loader BinaryLoader \
  -loader-baseAddr 0x10000 -processor 68000:BE:32:MC68020 \
  -scriptPath /work/impl/tools/ghidra -preScript SeedFlat68k.java 10000 117000 \
  -postScript ExportAll.java /work/impl/analysis/ghidra_68k
```

### Known functions

| Address | Build | Role |
|---|---|---|
| `FUN_01079300` | OS4 | LoadMap |
| `FUN_0106b7a0` | OS4 | GetCubeTexture (lazy-loads `cubetextures/%s`) |
| `FUN_0104feb0` | OS4 | screen setup; loads `.CubeTextureList` |
| `FUN_010bdab0` | OS4 | loads `64->256.dat`, looks up named object textures |
| `FUN_010024fc` | OS4 | Warp3D texture conversion; reads palette at `0x011490b8` |
| `FUN_000d7874` | 68k | loads all object models; scenery table `0x348000` |

## Open questions

- Sloped cubes: which edge `height1` belongs to; how much they raise the
  cubes above. Current guess in `crates/payback/src/city.rs`.
- Texture orientation and the meaning of the flip bit on side faces.
- Square flags bits other than `0x20`; header `variant` field.
- `.route`, `.mini`, `.vehicles`, `.textures`, `Human.motion` formats.
- Object face `mode`/`param` (shading? environment mapping?).
- Sound sample rates; the reflection (water) effect; night remapping.
- The mission script API and how scripts are entered.

## Roadmap

1. ~~Formats: maps, objects, textures, palette, phrases~~
2. ~~Render the city and scenery in Bevy~~
3. Player on foot: `Human.motion` animation, movement, collision with cubes
4. Vehicles: physics (decompile handling code), entering/exiting, damage
5. Pedestrians and traffic (square flags, routing)
6. Weapons, pickups, police and wanted level
7. Mission script API; reimplement the six level scripts
8. Front end: menus, phrases/HUD, sound, CD audio, saves, replays
