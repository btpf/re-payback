#!/usr/bin/env python3
"""Look things up in the Ghidra exports (analysis/ghidra_<build>/decompiled.c).

    python tools/re/fn.py BUILD KEY [KEY ...]     print functions
    python tools/re/fn.py BUILD --grep TEXT       print every function whose header contains TEXT
    python tools/re/fn.py BUILD --refs REGEX      list functions whose body matches REGEX (with counts)
    python tools/re/fn.py BUILD --callers NAME    list the functions that call NAME
    python tools/re/fn.py BUILD --line N [N ...]  which function holds line N of decompiled.c
    python tools/re/fn.py --sym KEY [KEY ...]     translate between builds via docs/xref/symbols.tsv

BUILD is os4, 68k, macx, mac9, mapedx, script00..script21, or a path to a
decompiled.c. KEY is a function name (`FUN_` optional before a hex address,
Mac names with or without the leading '.', a bare Mac name also matches its
mangled form `name__...`) or an address: any address inside a function finds
that function. `--max N` caps the lines printed per function.

Replaces the older per-topic helpers (fn.sh, where.sh, fn.py, xref.py,
callgraph.py) that lived in the git-ignored analysis/ tree.
"""
from __future__ import annotations

import argparse
import bisect
import csv
import re
import sys

from common import ROOT, decompile_path

HEADER = re.compile(r"^// ==== (\.?)(\S+) @ ([0-9a-fA-F]+) size=(\d+)")


def load(build):
    """[(name, addr, size, first_line, lines)] in file order."""
    path = decompile_path(build)
    lines = path.read_text(encoding="utf-8", errors="replace").split("\n")
    funcs = []
    for i, line in enumerate(lines):
        m = HEADER.match(line)
        if m:
            funcs.append([m.group(2), int(m.group(3), 16), int(m.group(4)), i])
    out = []
    for n, f in enumerate(funcs):
        end = funcs[n + 1][3] if n + 1 < len(funcs) else len(lines)
        out.append((f[0], f[1], f[2], f[3], lines[f[3]:end]))
    return out


def find(funcs, key):
    k = key[1:] if key.startswith(".") else key
    hexkey = k[4:] if k.startswith("FUN_") else k[2:] if k.lower().startswith("0x") else k
    for f in funcs:
        if f[0] == k or f[0] == "FUN_" + k:
            return f
    for f in funcs:
        if f[0].startswith(k + "__"):
            return f
    if re.fullmatch(r"[0-9a-fA-F]{3,8}", hexkey):
        a = int(hexkey, 16)
        starts = [f[1] for f in funcs]
        i = bisect.bisect_right(starts, a) - 1
        if i >= 0 and a < funcs[i][1] + max(funcs[i][2], 1):
            return funcs[i]
    return None


def show(f, mx):
    body = f[4] if not mx else f[4][:mx]
    sys.stdout.write("\n".join(body).rstrip() + "\n\n")


def sym(keys):
    path = ROOT / "docs" / "xref" / "symbols.tsv"
    rows = list(csv.DictReader(path.open(encoding="utf-8"), delimiter="\t"))
    for key in keys:
        k = key.lower().removeprefix("fun_").removeprefix("0x").lstrip(".")
        a = int(k, 16) if re.fullmatch(r"[0-9a-f]{3,8}", k) else None
        hits = [r for r in rows if (a is not None and a in
                                    [int(r[c], 16) for c in ("mac_addr", "os4_addr", "m68k_addr") if r[c]])
                or key.lstrip(".") in (r["mac_name"], r["demangled"])
                or r["mac_name"].startswith(key + "__")]
        if not hits:
            print(f"{key}: no row in docs/xref/symbols.tsv")
        for r in hits:
            print(f"{r['demangled'] or r['mac_name']}  Mac @{r['mac_addr'] or '-'}  "
                  f"OS4 {r['os4_addr'] or '-'}  68k {r['m68k_addr'] or '-'}  ({r['confidence'] or '-'})")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("build", nargs="?")
    ap.add_argument("keys", nargs="*")
    ap.add_argument("--grep")
    ap.add_argument("--refs")
    ap.add_argument("--callers")
    ap.add_argument("--line", type=int, nargs="+")
    ap.add_argument("--sym", nargs="+")
    ap.add_argument("--max", type=int, default=0)
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    if a.sym:
        return sym(a.sym)
    if not a.build:
        ap.error("BUILD is required")
    funcs = load(a.build)
    if a.grep:
        for f in funcs:
            if a.grep in f[4][0]:
                show(f, a.max)
    elif a.refs:
        pat = re.compile(a.refs)
        for f in funcs:
            n = sum(1 for line in f[4][1:] if pat.search(line))
            if n:
                print(f"{f[0]}\t{f[1]:08x}\t{n}")
    elif a.callers:
        target = find(funcs, a.callers)
        name = target[0] if target else a.callers
        pat = re.compile(r"\b\.?" + re.escape(name) + r"\s*\(")
        for f in funcs:
            if f[0] != name and any(pat.search(line) for line in f[4][1:]):
                print(f"{f[0]}\t{f[1]:08x}")
    elif a.line:
        starts = [f[3] for f in funcs]
        for n in a.line:
            i = bisect.bisect_right(starts, n - 1) - 1
            print(n, funcs[i][0] if i >= 0 else "-")
    else:
        for k in a.keys:
            f = find(funcs, k)
            if f:
                show(f, a.max)
            else:
                print(f"// {k}: not found\n")


if __name__ == "__main__":
    main()
