# Payback reimplementation

A reverse-engineered reimplementation of
[Payback](https://en.wikipedia.org/wiki/Payback_(video_game)) (Apex Designs,
Amiga, 2001) in Rust with [Bevy](https://bevyengine.org).

No original game data is included. You need your own copy of the CD and the
official Update 7.

## Setup

1. Put `Payback.iso` and the updates in `Payback Game + Updates/` (see
   `tools/extract.py` for the expected names), then unpack them:

   ```sh
   python tools/extract.py
   ```

   This creates `extracted/game/`, the merged CD + Update 7 install.

2. Run:

   ```sh
   cargo run --release -p payback -- --map 0
   ```

   `--data <dir>` or `PAYBACK_DATA` point at a different install.
   Controls: WASD/arrows pan, Q/E rotate, R/F tilt, wheel or Z/X zoom.
   `--screenshot out.png` saves a frame and exits.

## Layout

| Path | Contents |
|---|---|
| `crates/payback-formats` | engine-independent parsers for the game's files |
| `crates/payback` | the Bevy game |
| `tools/` | extraction, Amiga hunk tools, Ghidra scripts |
| `docs/formats.md` | file format reference |
| `docs/research.md` | reverse-engineering notes, open questions, roadmap |
| `Dockerfile` | RE workbench: Ghidra, radare2, amitools, FS-UAE, Rust |

## Tests

```sh
cargo test
```

Tests in `crates/payback-formats/tests/real_data.rs` parse every original
file when `extracted/game` exists and are skipped otherwise.
