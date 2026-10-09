#!/usr/bin/env python3
"""Pack the locally extracted game data for the web build (docs/web.md).

Writes <out>/payback.pak (the files back to back) and <out>/payback.json
({"files": [[path, offset, length], ...]}); the page fetches both and the
game reads them through `crates/payback/src/vfs.rs`. Paths are
`data/game/<rel>` for the Amiga data, `data/mac/<rel>` for the Mac data and
`Music/<rel>` for user-ripped CD music.

The output is the user's own copy of the original data: never commit it
(web/dist/ is git-ignored).

usage: python tools/web/pack_web_data.py [--game extracted/game]
           [--mac extracted/mac/Payback] [--music Music] [--no-film]
           [--out web/dist/data]
"""

import argparse
import json
import os
import sys

# Not read by the game: other executables, editors, docs, Finder junk, the
# extractor's resource forks, and the two Mac films it does not play
# (`GetPaybackMovie` @ 100fcbc0 opens payback_high_mpeg1.mpg only).
SKIP_NAMES = {
    ".ds_store", "mapeditor.exe", "paybackwos.exe", "payback (os 9)", "payback (os x)",
    "payback_high_mpeg2.mpg", "payback_low_mpeg1.mpg",
}
SKIP_DIRS = {"docs", "map editor (os 9)", "mapeditor (os x)"}
FILM = "payback_high_mpeg1.mpg"


def skip(name: str, no_film: bool) -> bool:
    low = name.lower()
    return (
        low in SKIP_NAMES
        or low.endswith(".rsrc")
        or low.startswith(".fbc")
        or (no_film and low == FILM)
    )


def walk(root: str, prefix: str, no_film: bool):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d.lower() not in SKIP_DIRS)
        for name in sorted(filenames):
            if skip(name, no_film):
                continue
            full = os.path.join(dirpath, name)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            yield f"{prefix}/{rel}", full


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game", default=os.environ.get("PAYBACK_DATA", "extracted/game"))
    ap.add_argument("--mac", default=os.environ.get("PAYBACK_MAC_DATA", "extracted/mac/Payback"))
    ap.add_argument("--music", default="Music")
    ap.add_argument("--no-film", action="store_true", help="leave out the Mac intro film (46 MB)")
    ap.add_argument("--out", default="web/dist/data")
    a = ap.parse_args()

    if not os.path.isfile(os.path.join(a.game, "Payback.exe")):
        print(f"no Payback.exe in {a.game}: run tools/extract/extract.py or pass --game", file=sys.stderr)
        return 1
    sources = [(a.game, "data/game")]
    if os.path.isdir(a.mac):
        sources.append((a.mac, "data/mac"))
    else:
        print(f"note: no Mac data at {a.mac}; packing the Amiga data only")
    if os.path.isdir(a.music):
        sources.append((a.music, "Music"))

    os.makedirs(a.out, exist_ok=True)
    files = []
    offset = 0
    with open(os.path.join(a.out, "payback.pak"), "wb") as pak:
        for root, prefix in sources:
            for path, full in walk(root, prefix, a.no_film):
                with open(full, "rb") as f:
                    b = f.read()
                pak.write(b)
                files.append([path, offset, len(b)])
                offset += len(b)
    with open(os.path.join(a.out, "payback.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"files": files}, f, ensure_ascii=False, separators=(",", ":"))
    print(f"packed {len(files)} files, {offset / 1e6:.1f} MB, into {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
