#!/usr/bin/env python3
"""Replay determinism check (cross-platform; see also replay_check.sh).

Records a scripted run of the port (`--record --script-input`), plays the
recording back (`--play`) and compares the world hashes both runs log every
50 ticks (`replay.rs`, `debug_hash`). MATCH means the simulation is a pure
function of the seed and the recorded input words.

What this does NOT check: the port against the original game. The hash is
taken over the port's own object state (`sim::Obj`: position, velocity,
heading, health, state, model, god mode, generator index); the original has
no such dump, and its physics are not reproduced bit for bit, so neither a
replay recorded by the original nor stored reference hashes from it can be
compared this way. The check also has no stored baseline: the reference is
the recording run of the same build (or of `--play-exe`, to compare two
builds: record with one, play back with the other).

usage:
  tools/checks/replay_check.py [--exe PATH] [--play-exe PATH] [--out DIR] [--fast N]
                        NAME TICKS [record args...]
  tools/checks/replay_check.py [--exe PATH] ... --suite [CASE ...]

  --suite runs the standard cases (story, story2, rampage, multiplayer,
  cheats; all when none is named). Extra record args go to the recording
  run only: the replay header carries the level, mode and seed.

The executable defaults to $PAYBACK_EXE, else target/debug/payback(.exe).
It must find the game data from the repository root (PAYBACK_DATA, or
extracted/game). Exit status: 0 all MATCH, 1 a mismatch, 2 a run failed.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# name -> (ticks, record args). Seeds are fixed so a failure reproduces.
SUITE = {
    # Story level 00 (city 0): walking, shooting, a vehicle, the police.
    "story": (1000, ["--map", "0", "--level", "0", "--seed", "1234"]),
    # Story level 11 (city 1, second level).
    "story2": (600, ["--map", "1", "--level", "1", "--seed", "4321"]),
    # Rampage with police level 3 and its random phone missions.
    "rampage": (800, ["--map", "1", "--rampage", "3", "--seed", "77"]),
    # Split-screen deathmatch: one input word per player per tick.
    "multiplayer": (600, ["--map", "2", "--multiplayer", "3", "--kills", "5", "--seed", "5"]),
    # Cheats are replay events (`cheats.rs`): god modes, weapons, a vehicle.
    "cheats": (600, ["--map", "0", "--seed", "99",
                     "--cheat-script", "100:god,150:vgod,200:weapons,300:vehicle=5,450:nogod,500:novgod"]),
}

HASH = re.compile(r"replay hash: (.*)$")


def default_exe() -> Path:
    env = os.environ.get("PAYBACK_EXE")
    if env:
        return Path(env)
    name = "payback.exe" if os.name == "nt" else "payback"
    return ROOT / "target" / "debug" / name


def run(exe: Path, args: list[str]) -> list[str] | None:
    """Run the game, return its `replay hash` lines (None if it failed)."""
    p = subprocess.run([str(exe), *args], cwd=ROOT, capture_output=True,
                       text=True, errors="replace")
    lines = [m.group(1).strip() for line in (p.stdout + p.stderr).splitlines()
             if (m := HASH.search(line))]
    if p.returncode != 0:
        tail = "\n".join((p.stdout + p.stderr).splitlines()[-8:])
        print(f"  {exe.name} {' '.join(args)} exited {p.returncode}\n{tail}")
        return None
    return lines


def check(exe: Path, play_exe: Path, out: Path, name: str, ticks: int,
          extra: list[str], fast: str | None) -> int:
    out.mkdir(parents=True, exist_ok=True)
    rec = out / name
    speed = ["--fast", fast] if fast else []
    a = run(exe, ["--record", str(rec), "--script-input", "--ticks", str(ticks),
                  *speed, *extra])
    b = run(play_exe, ["--play", str(rec), "--ticks", str(ticks), *speed]) if a is not None else None
    if a is None or b is None:
        print(f"{name}: FAILED")
        return 2
    (out / f"{name}.rec.log").write_text("\n".join(a) + "\n")
    (out / f"{name}.play.log").write_text("\n".join(b) + "\n")
    if not a:
        print(f"{name}: FAILED (no hashes logged)")
        return 2
    if a == b:
        print(f"{name}: MATCH ({len(a)} checkpoints, last: {a[-1]})")
        return 0
    print(f"{name}: DIFFER")
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            print(f"  first difference at checkpoint {i}:\n  rec:  {x}\n  play: {y}")
            break
    else:
        print(f"  rec has {len(a)} checkpoints, play {len(b)}")
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--exe", type=Path, default=default_exe())
    ap.add_argument("--play-exe", type=Path, help="play back with another build")
    ap.add_argument("--out", type=Path, default=ROOT / "target" / "replay_check")
    ap.add_argument("--fast", help="run the game N times faster (both runs)")
    ap.add_argument("--suite", action="store_true", help="run the standard cases")
    ap.add_argument("rest", nargs=argparse.REMAINDER)
    o = ap.parse_args()
    exe = o.exe.resolve()
    play_exe = (o.play_exe or o.exe).resolve()
    if not exe.is_file():
        print(f"{exe}: not found (build it, or pass --exe / set PAYBACK_EXE)")
        return 2
    if o.suite:
        names = o.rest or list(SUITE)
        bad = [n for n in names if n not in SUITE]
        if bad:
            print(f"unknown case(s) {bad}; cases: {', '.join(SUITE)}")
            return 2
        worst = 0
        for n in names:
            ticks, extra = SUITE[n]
            worst = max(worst, check(exe, play_exe, o.out, n, ticks, extra, o.fast))
        return worst
    if len(o.rest) < 2:
        ap.error("NAME TICKS required (or --suite)")
    return check(exe, play_exe, o.out, o.rest[0], int(o.rest[1]), o.rest[2:], o.fast)


if __name__ == "__main__":
    sys.exit(main())
