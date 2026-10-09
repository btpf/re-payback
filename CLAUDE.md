# Payback reimplementation

Reverse engineering and Bevy reimplementation of Payback (Apex Designs, Amiga, 2001).

## Layout

- `crates/payback-formats/` — engine-agnostic parsers for the game's data files.
- `crates/payback/` — the Bevy 0.19 game.
- `docs/formats.md`, `docs/research.md`, `docs/re/*.md` — reverse-engineering notes. Mark facts CONFIRMED (with the original function address) or INFERRED.
- `tools/` — extraction scripts (`extract.py` for the Amiga CD and updates, `extract_mac.py` for the Mac release).
- Mac release (`PaybackMac.toast`): treated as the more definitive copy; see `docs/re/mac_version.md`. Decompiles are in `analysis/ghidra_macx` (OS X build; functions keep their original C++ names) and `analysis/ghidra_mac9`, next to the Amiga ones (`analysis/ghidra_68k`, `analysis/ghidra_os4`), which stay as they are.
- Ignored: `Payback Game + Updates/` (the source data), `extracted/`, `analysis/`, `target/`, `Saves/`, `progress.html`, `mac_differences.html`, `remaining_gaps.html`, and `[A] PAYBACK 2025.7z` (ignore it entirely).
- Never commit original game data.

## Working conventions

- Keep the original's behaviour; cite decompiled functions in doc comments (`FUN_xxxxxxxx`, OS4 or 68k).
- Text files use LF line endings (`.gitattributes`).
- Verify visual changes with `./target/debug/payback.exe [--map N --level L] --screenshot <png>` and look at the image.
- Keep `cargo test -p payback -p payback-formats` passing. Keep replay determinism intact: `analysis/scratch/replay/check.sh <name> <ticks> --map 0 --seed 1234` must report MATCH.
- Determinism rules (`docs/research.md`): render systems must not add/remove components on `Obj` entities; sim loops that draw rng, spawn, despawn or pick first-come iterate by object-table slot (`sim::table_order`), never by query order or `Entity`; sim state changes only in the fixed tick. `PAYBACK_DET_CHECK=1` reports violations of the first rule.

## Progress page

`progress.html` in the repository root is a self-contained page (no external assets) that shows the project's progress and what's left to do. It is git-ignored (local only). **Keep it up to date:** whenever a feature lands, a gap is found or fixed, or priorities change, update its `DATA` object.
