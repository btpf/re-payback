# Tools

Everything here runs from any working directory: paths are derived from the
repository root. Python 3.11+; the RE tools also want `capstone`, `numpy`,
`Pillow` (and `markdown` for the Mac report). Shell scripts are POSIX `sh`/`bash`
(Git Bash on Windows). Outputs that contain game data or renders go to the
git-ignored `out/`, `analysis/` or `extracted/`, never into git.

## `extract/`: unpack your own copy of the game

For playing, `payback-extract` (the Rust crate `crates/payback-extract`, built next to `payback.exe`) does the player's part without Python or 7-Zip: from `Payback Game + Updates/` next to the executable (`Amiga/`: `.iso`, `.cue` + `.bin`, `.zip`, `.rar`; `Amiga/Updates/`; `MacOS/`) it writes `data/amiga`, `data/mac` and `Music/*.flac` there. The scripts below unpack everything into `extracted/` for the build and the RE tools.

| Tool | Does |
|---|---|
| `extract.py` | unpacks `Payback Game + Updates/` (`Amiga/Payback.iso`, `Amiga/Updates/*.lha`; needs `7z`) into `extracted/`, and builds the merged CD + Update 7 install `extracted/game/` |
| `rip_cd_tracks.py` | splits the music (CD audio tracks 2–16) out of a BIN/CUE image of the CD, one `.bin` per disc or per track, or a `.zip` holding one (default: the first `.cue`, else `.zip`, in `Payback Game + Updates/Amiga/`) into `NN.wav`/`NN.flac` files, by default into `target/release/Music/` (`--flac` needs `ffmpeg`) |
| `extract_mac.py` | reads the HFS volume `Payback Game + Updates/MacOS/PaybackMac.toast` into `extracted/mac/` (needs `pip install machfs`) |
| `hunk.py` | AmigaOS hunk parser; `--flat EXE OUT.bin` lays the hunks out at 0x10000 for Ghidra |

## `ghidra/`: the decompiles in `analysis/`

| Tool | Does |
|---|---|
| `decompile_all.sh [TARGET...]` | regenerates every export (`68k os4 macx mac9 mapedx scripts`) with the `payback-re` Docker image; `--out`, `--jobs`. See `docs/decompiling.md` |
| `ExportAll.java` | writes `decompiled.c`, `functions.tsv`, `strings.tsv` |
| `SeedFlat68k.java` | creates functions in a flat 68k image (entry + every `LINK A5`) |
| `MacJumpTables.java` | recovers CodeWarrior switch tables in the Mac PEF builds |
| `DecompOne.java` | decompiles chosen functions with a long timeout |

## `re/`: reverse-engineering helpers

| Tool | Does |
|---|---|
| `fn.py BUILD KEY...` | prints functions from an export by name or any address inside; `--grep`, `--refs`, `--callers`, `--line`; `--sym KEY` translates between Mac / OS4 / 68k via `docs/xref/symbols.tsv` |
| `calls68k.py`, `trace68k.py`, `dis68k.py` | 68k disassembly of the flat images: call arguments folded into `CALL Name(args)` (linear / recursive with switch tables) and a plain listing with decoded jump tables |
| `m68k_api.py` | names and argument counts of the 68k mission-script API (shared table) |
| `os4bin.py` | the OS4 ELF by address: `str`, `xref` (callers via `bl`), `dis` |
| `common.py` | shared paths, build names, the game palette from `Payback.exe` |
| `replay_decode.py` | decodes Amiga replay files (`Replays/*`) |
| `intro_video.py` | FLC + PCM intro decoder → `out/img/intro/` |
| `render_icons.py` | renders `Icons/*` → `out/img/icons/` |
| `hud_mock.py` | HUD mock-up at 320×256 → `out/img/` |
| `route_check.py` | checks the `.route` next-hop tables against BFS |
| `mac_datadiff.py` | Mac vs Amiga data files, per directory |
| `phrase_diff.py` | Mac vs Amiga phrase files, per entry → `out/mac/` |
| `symbols/` | the Mac↔OS4↔68k function matcher: `final_match.py`, `build_outputs.py`, `write_md.py` regenerate `docs/xref/symbols.{tsv,md}` (cache in `scratch/symbols_cache/`) |

## `checks/`: verify the port

| Tool | Does |
|---|---|
| `replay_check.py [--exe PATH] --suite` | records and replays the standard cases; every case must MATCH (also `NAME TICKS [args]` for one run). `replay_check.sh` is the same for one case in `sh` |
| `autoplay_all.sh [EXE] [m.l ...]` | plays the story levels with the autoplay bot; logs in `out/autoplay/` |
| `perf_check.sh [s] [args]` | frame-time check with the autoplay bot (`--perf-check`) |
| `usermaps_smoke.sh [EXE]` | loads, screenshots and runs every user map; output in `out/usermaps/` |
| `contact_sheet.py DIR OUT.png` | contact sheet of a folder of screenshots |
| `doc_links.py` | every relative link in the committed Markdown resolves; paths named in the entry docs exist |
| `web_smoke.mjs URL PROFILE STEPS` | drives headless Chrome over the DevTools protocol against the web build |

## `reports/`: local report pages (git-ignored output)

| Tool | Writes |
|---|---|
| `coverage.py` | `docs/coverage.html`: every Mac function as a treemap box, coloured by how the port and the notes cite it |
| `mac_differences.py` | `docs/mac_differences.html`: the consolidated Mac-vs-Amiga report (embeds renders from `out/img/` when present; never commit it) |

## `web/`, `dist/` and `dev/`

| Tool | Does |
|---|---|
| `web/build_web.sh` | the WebAssembly build into `web/dist/` (`docs/web.md`); cargo cache in `target/web` |
| `web/pack_web_data.py` | packs your game data for the web build (the files of `crates/payback/data_files.txt`) |
| `dist/stage_data.py DIR` | stages the game data the port reads into `DIR/data/amiga` and `DIR/data/mac` (incremental; hard links, `--copy`); `cargo build` does the same into `target/<profile>/` (`crates/payback/build.rs`) |
| `dev/buildgate.sh CMD...` | runs CMD when one of `BUILDGATE_SLOTS` (3) build slots is free; for parallel agents |
