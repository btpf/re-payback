#!/usr/bin/env python3
"""Stage the game data the port reads next to an executable.

Writes <dir>/data/amiga (the Amiga data root, with Payback.exe) and
<dir>/data/mac (the Mac `Payback` folder, if there is one): the files chosen
by crates/payback/data_files.txt, hard-linked to the source where possible
(else copied). The sync is incremental: files whose size and modification
time match are left alone, files no longer selected are removed; nothing
outside <dir>/data/amiga and <dir>/data/mac is touched.

`cargo build -p payback` does the same into target/<profile>/ by itself
(crates/payback/build.rs); this script is for other folders, e.g. a copy of
the release executable. The output is your own copy of the original data:
never commit it.

usage: python tools/dist/stage_data.py <dir> [--game extracted/game]
           [--mac extracted/mac/Payback] [--copy]

Also the selection rules for tools/web/pack_web_data.py (`load_rules`,
`walk`).
"""

import argparse
import os
import shutil
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
RULES = os.path.join(ROOT, "crates", "payback", "data_files.txt")
KINDS = {"skip-dir", "skip-file", "skip-suffix", "skip-prefix", "film"}


def load_rules(path: str = RULES) -> dict[str, set[str]]:
    """The rules of data_files.txt by kind, names lower-cased."""
    rules: dict[str, set[str]] = {k: set() for k in KINDS}
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            kind, _, name = line.partition(" ")
            if kind not in KINDS or not name.strip():
                raise ValueError(f"{path}:{n}: bad rule {line!r}")
            rules[kind].add(name.strip().lower())
    return rules


def skip_file(name: str, rules: dict[str, set[str]], no_film: bool = False) -> bool:
    low = name.lower()
    return (
        low in rules["skip-file"]
        or any(low.endswith(s) for s in rules["skip-suffix"])
        or any(low.startswith(p) for p in rules["skip-prefix"])
        or (no_film and low in rules["film"])
    )


def walk(root: str, rules: dict[str, set[str]], no_film: bool = False):
    """(relative path with `/`, full path) of the selected files, sorted."""
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d.lower() not in rules["skip-dir"])
        for name in sorted(filenames):
            if skip_file(name, rules, no_film):
                continue
            full = os.path.join(dirpath, name)
            yield os.path.relpath(full, root).replace(os.sep, "/"), full


def sync(src: str, dst: str, rules: dict[str, set[str]], copy: bool = False) -> tuple[int, int, int, int]:
    """Mirrors the selected files of `src` into `dst`.
    Returns (files, bytes, updated, removed)."""
    wanted = set()
    files = size = updated = removed = 0
    for rel, full in walk(src, rules):
        wanted.add(rel.lower())
        st = os.stat(full)
        files += 1
        size += st.st_size
        out = os.path.join(dst, *rel.split("/"))
        try:
            ot = os.stat(out)
            if ot.st_size == st.st_size and ot.st_mtime_ns == st.st_mtime_ns:
                continue
            os.remove(out)
        except FileNotFoundError:
            pass
        os.makedirs(os.path.dirname(out), exist_ok=True)
        linked = False
        if not copy:
            try:
                os.link(full, out)
                linked = True
            except OSError:
                pass
        if not linked:
            shutil.copy2(full, out)
        updated += 1
    for dirpath, dirnames, filenames in os.walk(dst, topdown=False):
        for name in filenames:
            full = os.path.join(dirpath, name)
            if os.path.relpath(full, dst).replace(os.sep, "/").lower() not in wanted:
                os.remove(full)
                removed += 1
        if dirpath != dst and not os.listdir(dirpath):
            os.rmdir(dirpath)
    return files, size, updated, removed


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dir", help="the folder with payback.exe; the data goes into its data/")
    ap.add_argument("--game", default=os.environ.get("PAYBACK_DATA", os.path.join(ROOT, "extracted", "game")))
    ap.add_argument("--mac", default=os.environ.get("PAYBACK_MAC_DATA", os.path.join(ROOT, "extracted", "mac", "Payback")))
    ap.add_argument("--copy", action="store_true", help="copy the files instead of hard-linking them")
    a = ap.parse_args()

    # Either release alone is enough (crates/payback/src/data.rs).
    has_game = os.path.isfile(os.path.join(a.game, "Payback.exe"))
    has_mac = os.path.isfile(os.path.join(a.mac, "CubeTextures", "CubeTextureList.txt"))
    if not has_game and not has_mac:
        print(
            f"no game data: no Payback.exe in {a.game} and no Mac Payback folder at {a.mac}; "
            "run tools/extract/extract.py or extract_mac.py, or pass --game / --mac",
            file=sys.stderr,
        )
        return 1
    rules = load_rules()
    sources = []
    if has_game:
        sources.append((a.game, "amiga"))
    else:
        print(f"note: no Payback.exe in {a.game}; staging the Mac data only")
    if has_mac:
        sources.append((a.mac, "mac"))
    else:
        print(f"note: no Mac data at {a.mac}; staging the Amiga data only")
    for src, sub in sources:
        dst = os.path.join(a.dir, "data", sub)
        files, size, updated, removed = sync(src, dst, rules, a.copy)
        print(f"{dst}: {files} files, {size / 1e6:.1f} MB ({updated} updated, {removed} removed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
