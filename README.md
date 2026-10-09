# Payback reimplementation

A reverse-engineered reimplementation of
[Payback](https://en.wikipedia.org/wiki/Payback_(video_game)) (Apex Designs,
Amiga, 2001) in Rust with [Bevy](https://bevyengine.org), plus the research
behind it.

It is a **behavioural port**, not a matching decompilation: the game logic is
rewritten from the decompiled original (the Amiga Update 7 and AmigaOS 4 builds
and the later Mac OS release, whose OS X build kept its function names) and
checked against it function by function. The Mac release is treated as the
definitive version; where the Amiga version differs, a setting picks it.

**Status:** playable end to end. The six story levels, Rampage (with the Mac's
random phone missions), 2-4 player split screen, replays, saves, the front end,
HUD, sound and the Mac intro film are in, with software and hardware (Warp3D-style)
renderers and a WebAssembly build. Details and what is left:
[`docs/status.md`](docs/status.md).

## Legal

No original game data and no decompiled code are in this repository, and none
may be committed: you need your own copy of the game. Everything derived from it
(`extracted/`, `analysis/`, `out/`, `web/dist/`, the report pages) is git-ignored.
The notes in `docs/` describe the original's behaviour in their own words and
quote at most a few lines (see [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md)).

## Quick start

**Prerequisites:** Rust (edition 2024, stable); Python 3.11+ with `7z` on the
`PATH` (and `pip install machfs` for the Mac release); for the RE tools also
`pip install capstone numpy pillow markdown`; Docker for the decompiles; for the
web build see [`docs/web.md`](docs/web.md).

1. **Get the data.** Put your copies in `Payback Game + Updates/`:
   `Payback.iso`, `Updates/PaybackUpdate7.lha` (and optionally
   `Updates/paybackupdateOS4Native.lha`, `Updates/Warp3D-42.lha`, and the Mac
   release `PaybackMac.toast`). Then unpack them:

   ```sh
   python tools/extract/extract.py       # -> extracted/game (CD + Update 7), iso, os4, ...
   python tools/extract/extract_mac.py   # -> extracted/mac (optional, recommended)
   ```

2. **Build and run.**

   ```sh
   cargo run --release -p payback                  # menus
   cargo run --release -p payback -- --map 0       # straight into city 0, level 0
   ```

   `--data <dir>` / `PAYBACK_DATA` and `--mac-data <dir>` / `PAYBACK_MAC_DATA`
   point at other installs. `--screenshot out.png` saves a frame and exits; the
   full option list is at the top of `crates/payback/src/main.rs`, the controls
   in [`docs/controls.md`](docs/controls.md).

3. **Web build** (optional): `tools/web/build_web.sh`, then serve `web/dist/`
   ([`docs/web.md`](docs/web.md)).

## Verify

```sh
cargo test -p payback -p payback-formats             # data tests run when extracted/ exists
python tools/checks/replay_check.py --suite          # replay determinism: every case must MATCH
tools/checks/autoplay_all.sh                         # the bot plays the six story levels
```

The replay suite records a scripted run and plays it back with the same build;
it checks that the port is deterministic, not that it matches the original.
`tools/checks/` also has a frame-time check, a user-map smoke test and a
headless-browser test for the web build ([`tools/README.md`](tools/README.md)).

## Reverse engineering

```sh
docker build -t payback-re .         # Ghidra 12.1.4 workbench
tools/ghidra/decompile_all.sh        # fills analysis/ with every decompile
python tools/re/fn.py macx DrawMap   # print a function; --sym translates between builds
```

Then read [`docs/README.md`](docs/README.md), the index of the notes, and
[`docs/decompiling.md`](docs/decompiling.md).

## Layout

| Path | Contents |
|---|---|
| `crates/payback-formats/` | engine-independent parsers for the game's files |
| `crates/payback/` | the Bevy game |
| `crates/payback-mpeg/` | MPEG-1 playback for the Mac intro film (vendored `pl_mpeg`, MIT) |
| `docs/` | RE notes and project docs ([index](docs/README.md)) |
| `tools/` | extraction, Ghidra, RE helpers, checks, reports, web build ([index](tools/README.md)) |
| `web/` | the WebAssembly loader page |
| `Dockerfile` | RE workbench: Ghidra, radare2, amitools, FS-UAE, Rust |
| `extracted/`, `analysis/`, `out/`, `scratch/` | git-ignored: your game data, the decompiles, generated output, throwaway work |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).
