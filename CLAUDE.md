# Payback reimplementation

Reverse engineering and Bevy reimplementation of Payback (Apex Designs, Amiga, 2001).

## Layout

- `crates/payback-formats/` — engine-agnostic parsers for the game's data files.
- `crates/payback/` — the Bevy 0.19 game.
- `docs/formats.md`, `docs/research.md`, `docs/re/*.md` — reverse-engineering notes. Mark facts CONFIRMED (with the original function address) or INFERRED.
- `tools/` — extraction scripts (`extract.py` for the Amiga CD and updates, `extract_mac.py` for the Mac release), `replay_check.py` (determinism suite), `perf_check.sh`, `coverage.py`, and the web build (`build_web.sh`, `pack_web_data.py`).
- `web/` — the WebAssembly loader (`index.html`); `web/dist/` is the git-ignored build output with the packed game data (`docs/web.md`).
- Mac release (`PaybackMac.toast`): treated as the more definitive copy; see `docs/re/mac_version.md`. Decompiles are in `analysis/ghidra_macx` (OS X build; functions keep their original C++ names) and `analysis/ghidra_mac9`, next to the Amiga ones (`analysis/ghidra_68k`, `analysis/ghidra_os4`), which stay as they are.
- Ignored: `Payback Game + Updates/` (the source data), `extracted/`, `analysis/`, `target/`, `Saves/`, `progress.html`, `mac_differences.html`, `remaining_gaps.html`, `coverage.html`, and `[A] PAYBACK 2025.7z` (ignore it entirely).
- Never commit original game data.

## Working conventions

- Keep the original's behaviour; cite decompiled functions in doc comments (`FUN_xxxxxxxx`, OS4 or 68k).
- Text files use LF line endings (`.gitattributes`).
- Verify visual changes with `./target/debug/payback.exe [--map N --level L] --screenshot <png>` and look at the image.
- Keep `cargo test -p payback -p payback-formats` passing. Keep replay determinism intact: `python tools/replay_check.py --suite` (story, rampage, multiplayer, cheats; `--exe` picks the build) or `tools/replay_check.sh <name> <ticks> --map 0 --seed 1234` must report MATCH; `cargo test -p payback --test replay_determinism -- --ignored` runs the same suite. It compares the port with itself, never with the original.
- Determinism rules (`docs/research.md`): render systems must not add/remove components on `Obj` entities; sim loops that draw rng, spawn, despawn or pick first-come iterate by object-table slot (`sim::table_order`), never by query order or `Entity`; sim state changes only in the fixed tick. `PAYBACK_DET_CHECK=1` reports violations of the first rule.

## Progress page

`progress.html` in the repository root is a self-contained page (no external assets) that shows the project's progress and what's left to do. It is git-ignored (local only). **Keep it up to date:** whenever a feature lands, a gap is found or fixed, or priorities change, update its `DATA` object.
