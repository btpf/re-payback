#!/usr/bin/env python3
"""Pack the locally extracted game data for the web build (docs/web.md).

Writes <out>/payback.pak (the files back to back) and <out>/payback.json
({"files": [[path, offset, length], ...]}); the page fetches both and the
game reads them through `crates/payback/src/vfs.rs`. Paths are
`data/game/<rel>` for the Amiga data, `data/mac/<rel>` for the Mac data and
`Music/<rel>` for user-ripped CD music. The files are those the game reads,
chosen by crates/payback/data_files.txt (shared with the native build's
data staging, tools/dist/stage_data.py).

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

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "dist"))
import stage_data  # noqa: E402  (the shared file selection, crates/payback/data_files.txt)


def walk(root: str, prefix: str, no_film: bool):
    rules = stage_data.load_rules()
    for rel, full in stage_data.walk(root, rules, no_film):
        yield f"{prefix}/{rel}", full


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game", default=os.environ.get("PAYBACK_DATA", "extracted/game"))
    ap.add_argument("--mac", default=os.environ.get("PAYBACK_MAC_DATA", "extracted/mac/Payback"))
    ap.add_argument("--music", default="Music")
    ap.add_argument("--no-film", action="store_true", help="leave out the Mac intro film (46 MB)")
    ap.add_argument("--out", default="web/dist/data")
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
    sources = []
    if has_game:
        sources.append((a.game, "data/game"))
    else:
        print(f"note: no Payback.exe in {a.game}; packing the Mac data only")
    if has_mac:
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
