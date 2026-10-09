"""Shared paths and helpers for the tools in tools/re/.

Every path is derived from the repository root (two levels above this file),
so the tools run from any working directory.
"""
from __future__ import annotations

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXTRACTED = ROOT / "extracted"
ANALYSIS = ROOT / "analysis"   # Ghidra exports (tools/ghidra/decompile_all.sh)
OUT = ROOT / "out"             # generated renders and reports (git-ignored)

GAME = Path(os.environ.get("PAYBACK_DATA", EXTRACTED / "game"))
MAC_GAME = Path(os.environ.get("PAYBACK_MAC_DATA", EXTRACTED / "mac" / "Payback"))
OS4_ELF = EXTRACTED / "os4" / "PaybackUpdateOS4" / "install_files" / "PaybackOS4"

# Short build names accepted by the tools -> Ghidra export directory.
BUILDS = ("os4", "68k", "macx", "mac9", "mapedx",
          "script00", "script01", "script10", "script11", "script20", "script21")


def decompile_path(build: str) -> Path:
    """`analysis/ghidra_<build>/decompiled.c`, or `build` itself if it is a file."""
    p = Path(build)
    if p.is_file():
        return p
    if build not in BUILDS:
        raise SystemExit(f"unknown build {build!r}; use one of {', '.join(BUILDS)} or a path")
    p = ANALYSIS / f"ghidra_{build}" / "decompiled.c"
    if not p.is_file():
        raise SystemExit(f"{p} is missing: run tools/ghidra/decompile_all.sh")
    return p


def flat_path(arg: str) -> Path:
    """A flattened 68k image: `68k` (Payback.exe), `scriptNN`, or a file path.

    The images come from `tools/extract/hunk.py --flat` (decompile_all.sh makes them).
    """
    if arg == "68k":
        p = ANALYSIS / "flat" / "Payback68k.bin"
    elif arg.startswith("script") and len(arg) == 8:
        p = ANALYSIS / "flat" / f"{arg}.bin"
    else:
        p = Path(arg)
    if not p.is_file():
        raise SystemExit(f"{p} is missing: run tools/ghidra/decompile_all.sh (it writes analysis/flat/)")
    return p


def game_palette(exe: Path | None = None) -> bytes:
    """The 256-colour game palette (768 bytes RGB) from the user's Payback.exe.

    Same search as `payback_formats::palette::find_in_executable`: the first
    `LoadRGB32` table (header 0x01000000, colour 0 black, zero terminator)
    with more than 64 distinct colours.
    """
    data = (exe or GAME / "Payback.exe").read_bytes()
    sig = bytes([1] + [0] * 15)
    table_len = 4 + 768 * 4 + 4
    start = 0
    while True:
        off = data.find(sig, start)
        if off < 0:
            raise SystemExit("no LoadRGB32 palette table in Payback.exe")
        table = data[off:off + table_len]
        if len(table) == table_len and table[-4:] == b"\0\0\0\0":
            pal = bytes(table[4 + i * 4] for i in range(768))
            if len({pal[i:i + 3] for i in range(0, 768, 3)}) > 64:
                return pal
        start = off + 1
