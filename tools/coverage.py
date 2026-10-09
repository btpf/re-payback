#!/usr/bin/env python3
"""Port coverage treemap for the Payback reimplementation.

Writes ``coverage.html`` in the repository root: a squarified treemap in the
style of the progress charts of matching-decompilation projects. Every
function of the Mac OS X build of Payback (``analysis/ghidra_macx``) is one
box, sized by its byte size and coloured by how far the port covers it.

This is a *behavioural* port, not a matching decompilation, so the colours
are evidence of attention, not proof of equivalence:

* green  - cited in the Rust sources (crates/**/*.rs) and no open gap,
           approximation or departure is noted for it;
* yellow - cited in the Rust sources, but a gap in remaining_gaps.html, a
           departure, or an "approximate / not reproduced / not done" note
           in the docs or code comments mentions it;
* blue   - cited only in the RE notes (docs/*.md, docs/re/*.md and the
           analysis/mac/*.md reviews): analysed, but nothing in the port
           points at it (maybe intentionally not needed);
* red    - game code that nothing cites;
* grey   - not applicable: Mac OS imports, PowerPlant, Mac platform classes
           (CD audio, DrawSprocket, InputSprocket, HID, preferences), the
           Amiga-API and Warp3D-on-OpenGL emulation layers, the Sound Manager
           backend and the MSL C/C++ runtime.

Citation forms recognised (in Rust comments, the docs and remaining_gaps.html):
Mac names (``DrawMap``, ``do_Person__Fv``, ``UCDAudio::SetVolume``), Mac
addresses (``@ 1013fbd0``; any address inside a function counts for it), OS4
``FUN_01xxxxxx`` / bare ``01xxxxxx`` and 68k ``FUN_000xxxxx`` / ``68k 0xa4a68``
addresses, mapped to Mac functions through ``analysis/mac/symbols.tsv`` (the
Amiga<->Mac symbol match) and the OS4/68k function tables.

Inputs (read at run time; analysis/ and remaining_gaps.html are git-ignored):
analysis/ghidra_macx/functions.tsv, analysis/ghidra_os4/functions.tsv,
analysis/ghidra_68k/functions.tsv, analysis/mac/symbols.tsv, crates/**/*.rs,
docs/*.md, docs/re/*.md, analysis/mac/*.md (except symbols.md) and
remaining_gaps.html. The output contains only function names, addresses,
sizes, file:line references and short status notes - no game data and no
decompiled code.

Usage: python tools/coverage.py [--out coverage.html]
"""

from __future__ import annotations

import argparse
import bisect
import datetime
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# --------------------------------------------------------------------------
# Areas
# --------------------------------------------------------------------------

AREAS = [
    ("render", "Rendering / city"),
    ("objects", "Objects / models"),
    ("effects", "Effects / weather"),
    ("hud", "Camera / HUD / minimap"),
    ("vehicles", "Vehicles / driving"),
    ("ai", "AI / traffic / police"),
    ("people", "People"),
    ("combat", "Combat / weapons"),
    ("missions", "Missions / triggers / scripts"),
    ("frontend", "Front end / menus"),
    ("audio", "Audio / music / CD"),
    ("files", "Replays / saves / files"),
    ("input", "Input"),
    ("util", "Maths / util"),
    ("core", "Core loop / game state"),
    ("platform", "Platform (Mac / Amiga API)"),
    ("runtime", "Runtime (C / C++)"),
    ("unknown", "Unknown"),
]
AREA_IDS = [a for a, _ in AREAS]

# CodeWarrior emits functions in source-file order, so the game code falls into
# contiguous per-file blocks. (start address, area); a block runs to the next.
AREA_RANGES = [
    (0x10051910, "core"),      # Payback.cpp accessors, config
    (0x10053320, "render"),    # ch2pl / c2p
    (0x10053510, "frontend"),  # loading progress bar
    (0x10054000, "combat"),    # CheckHitObject, Shockwave, Explode...
    (0x10055c70, "objects"),   # collide lists, object table
    (0x10057c60, "ai"),        # EstimateTime, AddThing, AddPolice
    (0x100593e0, "combat"),    # crates and weapon pickups
    (0x10059d00, "ai"),        # AddVehicles
    (0x1005a260, "render"),    # DrawMap, headlights
    (0x1005ffb0, "core"),      # InitStructures .. CloseAll
    (0x100648e0, "missions"),  # SetSeek
    (0x10064ad0, "input"),     # ReadInput
    (0x10065660, "people"),    # getting in and out of vehicles
    (0x100679a0, "missions"),
    (0x10067b50, "vehicles"),  # collisions, MoveObject_*
    (0x1006eb80, "render"),    # DrawView
    (0x10070c20, "combat"),    # DoVehicleFire
    (0x10071af0, "people"),
    (0x10072340, "util"),
    (0x10072cb0, "objects"),   # object.cpp: models, textures, DrawObject
    (0x10081080, "vehicles"),  # MoveUnit .. BounceUnit
    (0x10082ec0, "util"),      # arctan, random
    (0x10083620, "render"),    # brightness, colour tables
    (0x10084830, "util"),
    (0x100848a0, "files"),     # movie (replay) list
    (0x10084b30, "frontend"),  # user maps
    (0x100851d0, "hud"),       # display.cpp
    (0x10092dc0, "frontend"),  # options and menus
    (0x100a66c0, "effects"),
    (0x100acdb0, "objects"),   # Obj_*
    (0x100ad3c0, "files"),     # GetSampleNum, file tokenisers
    (0x100ad6d0, "effects"),   # weather
    (0x100ad730, "render"),    # background
    (0x100adad0, "missions"),  # restart points, start objects
    (0x100ae070, "render"),    # map: cubes, collision, lighting, voxels
    (0x100b0ba0, "files"),     # tokeniser, phrases
    (0x100b1250, "hud"),       # minimap
    (0x100b1520, "render"),
    (0x100f0f50, "ai"),        # routing
    (0x100f1670, "audio"),
    (0x100f43c0, "missions"),  # triggers
    (0x100f7820, "input"),
    (0x100f9700, "files"),     # replay bitstream and movie
    (0x100fae00, "audio"),     # CD
    (0x100fb130, "frontend"),  # intro animation
    (0x100fb770, "render"),    # chunky copy, asm blitters
    (0x100fbf10, "platform"),
    (0x100ffc70, "core"),      # amiga_* glue: Mac level loop
    (0x10102400, "platform"),
    (0x10104990, "missions"),  # script API
    (0x101089e0, "render"),    # ground rasterisers
    (0x10123fa0, "missions"),  # level scripts
    (0x1013e390, "combat"),    # do_Weapon
    (0x1013fbd0, "people"),    # do_Person
    (0x10146280, "vehicles"),  # do_Other
    (0x1014a670, "core"),      # MainLoop
    (0x1014abf0, "render"),    # API_3D_* (hardware renderer)
    (0x10153430, "platform"),  # Warp3D-on-OpenGL, CMS sound
    (0x1015a7b0, "render"),    # cube textures, light sourcing
    (0x1015d150, "runtime"),
]

# Name -> area, for functions the address blocks put in the wrong place.
AREA_OVERRIDES = {
    "ScareDrones": "people", "ScareDrones2": "people",
    "SetPartialChunky": "render", "SetFullChunky": "render", "SetOutcome": "core",
    "GetRainSound": "audio", "PointOnScreen": "hud",
    "DrawLine": "render", "ShowRecord": "files", "ReadMotion": "files",
    "SetFlareSize": "effects", "IsJoystick": "input",
    "AddHeadlights": "render", "RemoveHeadlights": "render",
    "ReadSaveGame": "files", "WriteSaveGame": "files", "SaveGameExists": "files",
    "GetSlotName": "files", "LoadTex": "render", "Display_LoadTex": "render",
    "AddLanguage": "frontend", "GetLanguageNum": "frontend", "DoSetLanguage": "frontend",
    "InitLanguages": "frontend", "CloseLanguages": "frontend", "EnterString": "frontend",
    "InitGraphics": "core", "CloseGraphics": "core",
    "DrawCheckpoint": "hud", "InitLabels": "hud", "AddLabel": "hud",
    "AddLabelPercent": "hud", "CloseLabels": "hud", "DoLabels": "hud",
    "ShowReplayInfo": "files", "DoLoadGame": "files", "DoSaveGame": "files",
    "LoadHiscores": "files", "SaveHiscores": "files", "CalcHiscoreChecksum": "files",
    "drawGroundAlpha": "render", "MotionBlur": "render",
    "LoadMinimap": "hud", "CloseMinimap": "hud", "GetMinimap": "hud",
    "GetNumAltPhones": "missions", "GetSafePos": "missions", "GetPhoneCoords": "missions",
    "Input_OnMenu": "input", "Input_IsOnMenu": "input",
    "MayGetChar": "input", "WriteControlName": "input",
    "GetWeaponAmmo": "combat", "GetNextWeapon": "combat", "GetPrevWeapon": "combat",
    "SaveConfig": "files", "LoadConfig": "files", "ReadScreenParams": "files",
    "WriteHighestCube": "files", "ReadHighestCube": "files",
    "LoadHighestCube": "files",
    "AddPolice": "ai", "SetWantedCount": "ai",
    "GetPerson": "people", "GetVehicle": "vehicles",
    "AddPlScore": "combat", "IncScoreMultiplier": "combat",
    "AddHealth": "combat", "GetTrafficCounter": "ai", "GetTrafficFrame": "ai",
    "SetHardwareAcceleration": "render", "HWIsLocked": "render",
    "Lock3DHW": "render", "UnLock3DHW": "render",
    "EnablePlayback": "files", "DisablePlayback": "files",
    "GetViewPort": "hud", "ToggleMotionBlur": "render", "GetMotionBlur": "render",
    "ToggleGouraudShading": "render", "ToggleRotatingView": "hud",
    "DoScroll": "hud", "GetSampleNum": "audio",
    "object_MainLoop": "core", "MainLoop": "core",
    "amiga_finishlevel": "core",
    "AddSound_script": "missions", "DeleteSound_script": "missions",
    "ScreenAlpha": "render",
    "InitMovies": "files", "CloseMovies": "files", "AddMovie": "files",
}

# --------------------------------------------------------------------------
# Status
# --------------------------------------------------------------------------

STATUSES = ["green", "yellow", "blue", "red", "grey"]
STATUS_LABEL = {
    "green": "Ported",
    "yellow": "Ported, approximation / gap",
    "blue": "Documented only",
    "red": "Untouched game code",
    "grey": "Not applicable",
}

# symbols.tsv evidence categories that are not game code.
NA_CATEGORIES = {
    "import": ("platform", "Mac OS import stub"),
    "powerplant": ("platform", "PowerPlant framework"),
    "mac-class": ("platform", "Mac platform class (CD audio, DrawSprocket, InputSprocket, prefs, UI)"),
    "amiga-shim": ("platform", "Amiga API / Warp3D / Sound Manager emulation"),
    "msl-runtime": ("runtime", "MSL C/C++ runtime"),
}

# Functions in the game ranges that are Mac platform code all the same.
NA_OVERRIDES = {
    "RegisterClass_Payback_PPob": "PowerPlant class registration",
    "aglReportError": "OpenGL (AGL) error reporting",
    "uploadTexture": "Warp3D-on-OpenGL texture upload",
    "convert32BitTexture": "Warp3D-on-OpenGL texture conversion",
    "AllocateSoundSystem": "Mac Sound Manager backend",
    "PrepareChannel": "Mac Sound Manager backend",
    "StartSound": "Mac Sound Manager backend",
    "PlayMusicBlock": "Mac Sound Manager music streaming",
    "NewColorTableByEntries": "QuickDraw colour table",
    "GetMsgFromKeyboard": "Mac event to Amiga IntuiMessage glue",
    "AmigaPath2FSSpec": "Amiga path to Mac FSSpec",
    "NameOK": "Mac file name check",
    "StControlColorUPP": "PowerPlant control colour UPP",
    "LTimerTaskFunctor": "PowerPlant timer task",
    "UDrawSprocket::GetDSpContext": "DrawSprocket",
    "UDrawSprocket::SetQuickDraw": "DrawSprocket",
    "printf_special": "debug printf",
    "malloc_check": "allocation wrapper",
}

# Explicit status corrections: name -> (status, reason). Applied last.
_CITY_FACES = ("city.rs builds the cube tops and side faces from rendering.md "
               "§ Top surface / Side faces; it cites the doc, not these functions")
STATUS_OVERRIDES: dict[str, tuple[str, str]] = {
    **{f"drawCubeTop_{k}": ("green", _CITY_FACES)
       for k in ("None", "Vertical", "Horizontal", "RVertical", "RHorizontal")},
    **{f"drawCubeSide_{k}": ("green", _CITY_FACES) for k in ("North", "South", "West", "East")},
    "drawCubeTop": ("green", _CITY_FACES),
    "drawCubeSide": ("green", _CITY_FACES),
}

# Phrases that mark a gap / approximation / departure in a doc or code comment.
GAP_KW = re.compile(
    r"not reproduced|not implemented|not ported|approximat|port departure|departure|"
    r"\bnot done\b|the port (?:does not|doesn't|skips|ignores|leaves out)|simplif|"
    r"unlike the original|instead of the original",
    re.I,
)
# Doc lines whose gap note is known to be stale (remaining_gaps.html STALE list).
STALE_NOTE_LINES = {
    ("docs/re/rendering.md", 1088),
    ("docs/re/combat_police.md", 743),
}

CONF_C = re.compile(r"CONFIRMED|\[C\]|\(C\)|\*\*C\*\*")
CONF_I = re.compile(r"INFERRED|\[I\]|\(I\)|\*\*I\*\*")


# --------------------------------------------------------------------------
# Loading
# --------------------------------------------------------------------------

def read_tsv(path: Path) -> list[list[str]]:
    with open(path, encoding="utf-8", errors="replace") as f:
        return [line.rstrip("\n").split("\t") for line in f if line.strip()]


class FuncTable:
    """Sorted (start, size) table for "which function contains this address"."""

    def __init__(self, rows):
        rows = sorted(rows)
        self.starts = [r[0] for r in rows]
        self.sizes = [r[1] for r in rows]

    def containing(self, addr: int) -> int | None:
        i = bisect.bisect_right(self.starts, addr) - 1
        if i < 0:
            return None
        if addr < self.starts[i] + max(self.sizes[i], 1):
            return self.starts[i]
        return None


def load_amiga_table(name: str) -> FuncTable:
    path = ROOT / "analysis" / name / "functions.tsv"
    rows = []
    for r in read_tsv(path):
        try:
            rows.append((int(r[0], 16), int(r[2])))
        except (ValueError, IndexError):
            pass
    return FuncTable(rows)


def short_name(demangled: str) -> str:
    """`Foo::Bar(int, char*) const` -> `Foo::Bar`."""
    depth = 0
    for i, ch in enumerate(demangled):
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif ch == "(" and depth == 0 and i > 0:
            return demangled[:i]
    return demangled


def load_functions():
    funcs = {}
    for r in read_tsv(ROOT / "analysis/ghidra_macx/functions.tsv"):
        addr = int(r[0], 16)
        mangled = r[1].lstrip(".")
        funcs[addr] = {
            "addr": addr, "size": int(r[2]), "mangled": mangled,
            "demangled": mangled, "name": mangled, "cat": "game",
            "os4": None, "m68k": None, "match": "",
        }
    sym_rows = read_tsv(ROOT / "analysis/mac/symbols.tsv")
    header = sym_rows[0]
    col = {h: i for i, h in enumerate(header)}
    os4_to_mac, m68k_to_mac = {}, {}
    for r in sym_rows[1:]:
        r = r + [""] * (len(header) - len(r))
        if not r[col["mac_addr"]]:
            continue
        mac = int(r[col["mac_addr"]], 16)
        f = funcs.get(mac)
        if f is None:
            continue
        f["demangled"] = r[col["demangled"]] or f["mangled"]
        f["name"] = short_name(f["demangled"])
        ev = r[col["evidence"]]
        if ev.startswith("mac-only:"):
            f["cat"] = ev.split(":", 1)[1].split(";")[0].strip()
        if r[col["os4_addr"]]:
            a = int(r[col["os4_addr"]], 16)
            f["os4"] = a
            os4_to_mac.setdefault(a, mac)
        if r[col["m68k_addr"]]:
            a = int(r[col["m68k_addr"]], 16)
            f["m68k"] = a
            m68k_to_mac.setdefault(a, mac)
        if r[col["confidence"]]:
            f["match"] = r[col["confidence"]]
    return funcs, os4_to_mac, m68k_to_mac


# --------------------------------------------------------------------------
# Citation resolver
# --------------------------------------------------------------------------

RE_FUN = re.compile(r"\bFUN_([0-9a-fA-F]{8})\b")
RE_MAC = re.compile(r"(?<![\w.])(10[01][0-9a-f]{5})(?!\w)")
RE_OS4 = re.compile(r"(?<![\w.])(010[0-9a-f]{5})(?!\w)")
RE_68K = re.compile(r"68k[^\n.;]{0,24}?(?<!\w)(?:0x|FUN_000)([0-9a-f]{5})(?!\w)")
RE_HEX = re.compile(r"(?<![\w+])0x([0-9a-fA-F]{5,8})(?!\w)")
RE_CTX68K = re.compile(r"\b68k\b|Update 7", re.I)
RE_MANGLED = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*__[A-Za-z0-9_]+)")
RE_QUALIFIED = re.compile(r"\b([A-Za-z_]\w*::~?[A-Za-z_]\w*)")
RE_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


class Resolver:
    def __init__(self, funcs, os4_to_mac, m68k_to_mac, rust_idents):
        self.funcs = funcs
        self.mac = FuncTable([(a, f["size"]) for a, f in funcs.items()])
        self.os4 = load_amiga_table("ghidra_os4")
        self.m68k = load_amiga_table("ghidra_68k")
        self.os4_to_mac = os4_to_mac
        self.m68k_to_mac = m68k_to_mac
        self.by_mangled = {}
        self.by_base = defaultdict(list)
        self.by_qualified = defaultdict(list)
        for a, f in funcs.items():
            self.by_mangled[f["mangled"]] = a
            if f["name"].startswith("FUN_"):
                continue
            if "::" in f["name"]:
                self.by_qualified[f["name"]].append(a)
            else:
                self.by_base[f["name"]].append(a)
        # Bare names that are also Rust identifiers (types, enum variants) are
        # only taken in their mangled or `@ address` form in Rust comments.
        self.rust_collisions = {n for n in self.by_base if n in rust_idents}

    @staticmethod
    def distinctive(name: str) -> bool:
        caps = sum(1 for c in name if c.isupper())
        return len(name) >= 6 and (caps >= 2 or "_" in name)

    def mac_containing(self, addr):
        return self.mac.containing(addr)

    def resolve(self, text: str, rust: bool, context: str = "") -> dict[int, str]:
        """Mac function address -> how it was cited.

        `context` is the text just before `text` (previous lines); it decides
        whether a bare `0x` value that is a 68k function start is read as a
        68k address (other bare values need "68k" right before them).
        """
        out: dict[int, str] = {}

        def add(mac, kind):
            if mac is not None and mac not in out:
                out[mac] = kind

        for m in RE_FUN.finditer(text):
            h = m.group(1).lower()
            a = int(h, 16)
            if h.startswith("10"):
                add(self.mac_containing(a), "mac")
            elif h.startswith("01"):
                s = self.os4.containing(a)
                add(self.os4_to_mac.get(s if s is not None else a), "os4")
            elif h.startswith("000"):
                s = self.m68k.containing(a)
                add(self.m68k_to_mac.get(s if s is not None else a), "68k")
        for m in RE_MAC.finditer(text):
            if m.group(1) == "10000000":
                continue
            add(self.mac_containing(int(m.group(1), 16)), "mac")
        for m in RE_OS4.finditer(text):
            s = self.os4.containing(int(m.group(1), 16))
            if s is not None:
                add(self.os4_to_mac.get(s), "os4")
        for m in RE_68K.finditer(text):
            s = self.m68k.containing(int(m.group(1), 16))
            if s is not None:
                add(self.m68k_to_mac.get(s), "68k")
        is68k = bool(RE_CTX68K.search(context + " " + text))
        for m in RE_HEX.finditer(text):
            h = m.group(1).lower()
            a = int(h, 16)
            if len(h) == 8 and h.startswith("10"):
                if a != 0x10000000:
                    add(self.mac_containing(a), "mac")
            elif len(h) == 8 and h.startswith("010"):
                s = self.os4.containing(a)
                add(self.os4_to_mac.get(s), "os4")
            elif len(h) <= 6 and 0x10100 <= a < 0x120000:
                s = self.m68k.containing(a)
                if s == a and is68k:  # a function start, with 68k context
                    add(self.m68k_to_mac.get(s), "68k")
        for m in RE_MANGLED.finditer(text):
            a = self.by_mangled.get(m.group(1))
            if a is not None:
                add(a, "name")
        for m in RE_QUALIFIED.finditer(text):
            for a in self.by_qualified.get(m.group(1), ()):
                add(a, "name")
        for m in RE_IDENT.finditer(text):
            name = m.group(0)
            addrs = self.by_base.get(name)
            if not addrs:
                continue
            if rust and name in self.rust_collisions:
                continue
            before = text[m.start() - 1] if m.start() else ""
            after = text[m.end()] if m.end() < len(text) else ""
            if before == "[" or text[max(0, m.start() - 2):m.start()] == "[`":
                continue  # Rust intra-doc link
            ticked = before == "`" or after == "`"
            if not (ticked or self.distinctive(name)):
                continue
            for a in addrs:
                add(a, "name")
        return out


# --------------------------------------------------------------------------
# Scanning
# --------------------------------------------------------------------------

def rel(p: Path) -> str:
    return p.relative_to(ROOT).as_posix()


def rust_comment(line: str) -> str | None:
    i = line.find("//")
    if i < 0:
        return None
    if line.count('"', 0, i) % 2 == 1:  # inside a string literal
        return None
    return line[i:]


def snippet(text: str, n: int = 170) -> str:
    text = re.sub(r"^\s*(//[/!]?|[-*#>|]+)\s*", "", text.strip())
    text = re.sub(r"\s+", " ", text)
    return text if len(text) <= n else text[: n - 1] + "…"


RE_ITEM_START = re.compile(r"^\s*(?:[-*+]\s|\d+[.)]\s|\|)")
RE_SENTENCE_END = re.compile(r"[.;!?](?=\s|$)")


def gap_scopes(texts: list[str | None], i: int) -> list[tuple[str, str]]:
    """(text, context) pieces a gap keyword on line `i` applies to.

    Normally the sentence around the keyword (it may span lines of the same
    paragraph or comment block). A keyword line ending in ':' (e.g.
    "Not reproduced:") is a header: it covers the list that follows. Table
    rows stand alone.
    """
    line = texts[i]
    if line.lstrip().startswith("|"):
        return [(line, "")]
    if line.rstrip().endswith(":"):
        out, j = [(line, "")], i + 1
        while j < len(texts) and j <= i + 8 and texts[j] and texts[j].strip():
            out.append((texts[j], texts[j - 1]))
            j += 1
        return out

    def same_block(k):
        return 0 <= k < len(texts) and texts[k] is not None and texts[k].strip()             and not texts[k].lstrip().startswith("|")

    lo = i
    if not RE_ITEM_START.match(line):
        while lo > i - 3 and same_block(lo - 1):
            lo -= 1
            if RE_ITEM_START.match(texts[lo]):
                break
    hi = i
    while hi < i + 3 and same_block(hi + 1) and not RE_ITEM_START.match(texts[hi + 1]):
        hi += 1
    para = " ".join(t.strip() for t in texts[lo:hi + 1])
    out = []
    for m in GAP_KW.finditer(para):
        ends = [e.end() for e in RE_SENTENCE_END.finditer(para, 0, m.start())]
        start = ends[-1] if ends else 0
        e = RE_SENTENCE_END.search(para, m.end())
        out.append((para[start:e.end() if e else len(para)], para))
    return out


def scan_rust(resolver, cites):
    files = [p for p in sorted((ROOT / "crates").rglob("*.rs")) if "target" not in p.parts]
    for path in files:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        comments = [rust_comment(l) for l in lines]
        texts = [None if c is None else re.sub(r"^//[/!]?", "", c) for c in comments]
        for i, c in enumerate(comments):
            if c is None:
                continue
            context = " ".join(t for t in texts[max(0, i - 3):i] if t)
            refs = resolver.resolve(c, rust=True, context=context)
            for a, kind in refs.items():
                cites[a]["rust"].append(f"{rel(path)}:{i + 1}")
                cites[a]["rust_kinds"].add(kind)
            if GAP_KW.search(c):
                for text, ctx in gap_scopes(texts, i):
                    for a in resolver.resolve(text, rust=True, context=ctx):
                        cites[a]["notes"].append(("code", f"{rel(path)}:{i + 1}", snippet(text)))
    return len(files)


def doc_files():
    out = sorted((ROOT / "docs").glob("*.md")) + sorted((ROOT / "docs/re").glob("*.md"))
    out += [p for p in sorted((ROOT / "analysis/mac").glob("*.md")) if p.name != "symbols.md"]
    return out


def scan_docs(resolver, cites):
    files = doc_files()
    for path in files:
        r = rel(path)
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        heading = ""
        for i, line in enumerate(lines):
            if line.startswith("#"):
                heading = line.lstrip("#").strip()
            context = " ".join(lines[max(0, i - 3):i])
            refs = resolver.resolve(line, rust=False, context=context)
            conf = "C" if CONF_C.search(line) or CONF_C.search(heading) else (
                "I" if CONF_I.search(line) or CONF_I.search(heading) else "")
            for a in refs:
                cites[a]["docs"].append(f"{r}:{i + 1} § {heading[:70]}")
                if conf:
                    cites[a]["conf"][conf] += 1
            if GAP_KW.search(line) and (r, i + 1) not in STALE_NOTE_LINES:
                for text, ctx in gap_scopes(lines, i):
                    for a in resolver.resolve(text, rust=False, context=ctx):
                        cites[a]["notes"].append(("doc", f"{r}:{i + 1}", snippet(text)))
    return files


def js_strings(block: str) -> list[str]:
    return [json.loads(m.group(0)) for m in re.finditer(r'"(?:[^"\\]|\\.)*"', block)]


def scan_gaps(resolver, cites):
    path = ROOT / "remaining_gaps.html"
    if not path.exists():
        return 0
    text = path.read_text(encoding="utf-8", errors="replace")

    def block(name):
        i = text.find(f"const {name} = [")
        if i < 0:
            return ""
        j = text.find("\n];", i)
        return text[i:j]

    n = 0
    gaps = block("GAPS")
    for chunk in re.split(r'(?=\{ id: ")', gaps)[1:]:
        gid = re.search(r'id: "([^"]+)"', chunk).group(1)
        title = re.search(r'title: "((?:[^"\\]|\\.)*)"', chunk)
        sev = re.search(r'sev: "([^"]+)"', chunk)
        label = f"{gid} ({sev.group(1) if sev else '?'}): {json.loads(chr(34) + title.group(1) + chr(34)) if title else ''}"
        for a in resolver.resolve(chunk, rust=False):
            cites[a]["gaps"].append(label)
        n += 1
    for name, kind in (("DEPARTURES", "departure"), ("UNVERIFIED", "unverified"),
                       ("FAITHFUL", "faithful")):
        b = block(name)
        if not b:
            continue
        if name == "FAITHFUL":
            entries = [[s] for s in js_strings(b)]
        else:
            entries = [js_strings(m.group(0)) for m in re.finditer(r"\[\s*\"(?:[^\]\\]|\\.)*?\"\s*\]", b)]
        for e in entries:
            if not e:
                continue
            for a in resolver.resolve(" ".join(e), rust=False):
                cites[a]["notes"].append((kind, "remaining_gaps.html", snippet(e[0], 140)))
    return n


# --------------------------------------------------------------------------
# Classification
# --------------------------------------------------------------------------

def area_of(f) -> str:
    name, addr = f["name"], f["addr"]
    if name in AREA_OVERRIDES:
        return AREA_OVERRIDES[name]
    if f["cat"] in NA_CATEGORIES:
        return NA_CATEGORIES[f["cat"]][0]
    if f["cat"] == "mac-glue":
        return "core"
    if addr < AREA_RANGES[0][0]:
        return "platform"
    starts = [s for s, _ in AREA_RANGES]
    i = bisect.bisect_right(starts, addr) - 1
    return AREA_RANGES[i][1] if i >= 0 else "unknown"


def na_reason(f) -> str | None:
    if f["name"] in NA_OVERRIDES:
        return NA_OVERRIDES[f["name"]]
    if f["cat"] in NA_CATEGORIES:
        return NA_CATEGORIES[f["cat"]][1]
    if f["addr"] < AREA_RANGES[0][0]:
        return "PowerPlant framework"
    if f["addr"] >= 0x1015d150:
        return "MSL C/C++ runtime"
    if area_of(f) == "platform":
        return "Mac platform glue"
    return None


def classify(f, c):
    na = na_reason(f)
    yellow_notes = [n for n in c["notes"] if n[0] in ("code", "doc", "departure")]
    if na:
        status, why = "grey", na
    elif c["rust"]:
        if c["gaps"] or yellow_notes:
            status, why = "yellow", "cited in Rust; a gap / approximation note mentions it"
        else:
            status, why = "green", "cited in Rust; no gap noted"
    elif c["docs"] or c["gaps"]:
        status, why = "blue", "cited in the RE notes only"
    else:
        status, why = "red", "game code, not cited anywhere"
    if f["name"] in STATUS_OVERRIDES:
        status, why = STATUS_OVERRIDES[f["name"]]
        why = "override: " + why
    return status, why


# --------------------------------------------------------------------------
# Output
# --------------------------------------------------------------------------

def dedupe(seq):
    seen, out = set(), []
    for x in seq:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out


def git_head() -> str:
    try:
        return subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT,
                              capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "?"


def rust_identifiers() -> set[str]:
    idents = set()
    decl = re.compile(r"\b(?:struct|enum|trait|type|const|fn|mod)\s+([A-Za-z_]\w*)")
    variant = re.compile(r"^\s+([A-Z][A-Za-z0-9]*)\s*[,({]")
    for path in (ROOT / "crates").rglob("*.rs"):
        if "target" in path.parts:
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            idents.update(decl.findall(line))
            m = variant.match(line)
            if m:
                idents.add(m.group(1))
    return idents


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", default=str(ROOT / "coverage.html"))
    ap.add_argument("--json", help="also write the per-function data as JSON")
    args = ap.parse_args()

    funcs, os4_to_mac, m68k_to_mac = load_functions()
    resolver = Resolver(funcs, os4_to_mac, m68k_to_mac, rust_identifiers())
    cites = defaultdict(lambda: {"rust": [], "rust_kinds": set(), "docs": [], "notes": [],
                                 "gaps": [], "conf": {"C": 0, "I": 0}})
    n_rs = scan_rust(resolver, cites)
    docs = scan_docs(resolver, cites)
    n_gaps = scan_gaps(resolver, cites)

    rows = []
    for addr in sorted(funcs):
        f = funcs[addr]
        c = cites[addr]
        status, why = classify(f, c)
        rust = dedupe(c["rust"])
        docrefs = dedupe(c["docs"])
        notes = dedupe(c["notes"])
        rows.append({
            "a": f"{addr:08x}", "s": f["size"], "n": f["name"], "d": f["demangled"],
            "m": f["mangled"], "st": STATUSES.index(status), "ar": AREA_IDS.index(area_of(f)),
            "why": why,
            "os4": f"{f['os4']:08x}" if f["os4"] is not None else "",
            "k68": f"{f['m68k']:08x}" if f["m68k"] is not None else "",
            "mc": f["match"],
            "r": rust[:40], "rn": len(rust),
            "dr": docrefs[:40], "dn": len(docrefs),
            "g": dedupe(c["gaps"]),
            "no": [list(n) for n in notes[:12]],
            "cf": [c["conf"]["C"], c["conf"]["I"]],
        })

    meta = {
        "generated": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"),
        "head": git_head(),
        "nFuncs": len(rows), "nRust": n_rs, "nDocs": len(docs), "nGaps": n_gaps,
        "docs": [rel(p) for p in docs],
        "areas": [{"id": a, "name": n} for a, n in AREAS],
        "statuses": [{"id": s, "label": STATUS_LABEL[s]} for s in STATUSES],
        "naOverrides": NA_OVERRIDES,
        "statusOverrides": {k: list(v) for k, v in STATUS_OVERRIDES.items()},
        "areaOverrides": len(AREA_OVERRIDES),
    }
    payload = json.dumps({"meta": meta, "funcs": rows}, separators=(",", ":"), ensure_ascii=False)
    payload = payload.replace("</", "<\\/")
    html = TEMPLATE.replace("/*__DATA__*/null", payload)
    Path(args.out).write_text(html, encoding="utf-8", newline="\n")
    if args.json:
        Path(args.json).write_text(json.dumps(rows, indent=1), encoding="utf-8", newline="\n")

    # Console summary.
    tot = defaultdict(lambda: [0, 0])
    for r in rows:
        tot[STATUSES[r["st"]]][0] += 1
        tot[STATUSES[r["st"]]][1] += r["s"]
    game_b = sum(v[1] for k, v in tot.items() if k != "grey")
    game_n = sum(v[0] for k, v in tot.items() if k != "grey")
    print(f"wrote {args.out}: {len(rows)} functions, {n_rs} Rust files, {len(docs)} docs, {n_gaps} gaps")
    for s in STATUSES:
        n, b = tot[s]
        extra = "" if s == "grey" else f"  ({100 * b / game_b:.1f}% of game bytes, {100 * n / game_n:.1f}% of game functions)"
        print(f"  {s:6} {n:5} functions {b:8} bytes{extra}")
    return 0


TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Payback port coverage</title>
<style>
:root {
  color-scheme: light dark;
  --bg: #f6f6f3; --surface: #fcfcfb; --border: #deded8; --ink: #1d1d1b; --ink2: #55554f;
  --muted: #85857d; --chip: #ececE7; --accent: #2f6fb5;
  --s-green: #2e9e44; --s-yellow: #e8a917; --s-blue: #3b7dd8; --s-red: #d03b3b; --s-grey: #b9b9b1;
  --hdr: rgba(0,0,0,.62); --hdr-ink: #fff;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #141413; --surface: #1c1c1b; --border: #34342f; --ink: #ecece7; --ink2: #b5b5ad;
    --muted: #8a8a82; --chip: #2a2a27; --accent: #6aa2e0;
    --s-green: #3cb054; --s-yellow: #f0b429; --s-blue: #5591e6; --s-red: #e0514f; --s-grey: #55554f;
    --hdr: rgba(0,0,0,.7); --hdr-ink: #f2f2ee;
  }
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink);
  font: 14px/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
header, section { max-width: 1600px; margin: 0 auto; padding: 0 18px; }
header { padding-top: 18px; }
h1 { font-size: 23px; margin: 0 0 2px; }
h2 { font-size: 17px; margin: 26px 0 8px; }
.sub { color: var(--ink2); margin: 0 0 4px; }
.meta { color: var(--muted); font-size: 12.5px; }
code, .mono { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12.5px; }
.stats { display: grid; grid-template-columns: repeat(auto-fill, minmax(165px, 1fr)); gap: 10px; margin: 14px 0 6px; }
.stat { background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 10px 12px; }
.stat .v { font-size: 25px; font-weight: 650; font-variant-numeric: tabular-nums; line-height: 1.15; }
.stat .l { font-size: 12px; color: var(--ink2); }
.stat .x { font-size: 12px; color: var(--muted); font-variant-numeric: tabular-nums; }
.sw { display: inline-block; width: 11px; height: 11px; border-radius: 3px; vertical-align: -1px; margin-right: 6px; }
.bar { display: flex; height: 14px; border-radius: 4px; overflow: hidden; gap: 2px; background: var(--surface); }
.bar span { display: block; height: 100%; }
.controls { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; margin: 14px 0 8px; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; }
.chip { border: 1px solid var(--border); background: var(--surface); color: var(--ink); border-radius: 999px;
  padding: 3px 10px; font: inherit; font-size: 12.5px; cursor: pointer; display: inline-flex; align-items: center; }
.chip.off { opacity: .4; text-decoration: line-through; }
.chip:focus-visible, input:focus-visible, button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
input[type=search] { border: 1px solid var(--border); background: var(--surface); color: var(--ink); border-radius: 8px;
  padding: 5px 10px; font: inherit; width: 260px; }
select { border: 1px solid var(--border); background: var(--surface); color: var(--ink); border-radius: 8px; padding: 4px 8px; font: inherit; }
.main { display: grid; grid-template-columns: minmax(0, 1fr) 360px; gap: 14px; max-width: 1600px; margin: 0 auto; padding: 0 18px; }
#mapwrap { position: relative; height: 78vh; min-height: 520px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
canvas { display: block; width: 100%; height: 100%; cursor: crosshair; }
#panel { background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 12px 14px; height: 78vh;
  min-height: 520px; overflow: auto; font-size: 13px; }
#panel h3 { margin: 0 0 4px; font-size: 16px; word-break: break-all; }
#panel .kv { display: grid; grid-template-columns: 82px 1fr; gap: 2px 8px; margin: 8px 0; }
#panel .kv div:nth-child(odd) { color: var(--muted); }
#panel ul { margin: 4px 0 10px; padding-left: 18px; }
#panel li { margin: 1px 0; word-break: break-word; }
#panel .empty { color: var(--muted); }
.status-pill { display: inline-flex; align-items: center; gap: 6px; font-weight: 600; }
#tip { position: fixed; pointer-events: none; z-index: 10; max-width: 440px; background: var(--surface); color: var(--ink);
  border: 1px solid var(--border); border-radius: 8px; padding: 8px 10px; font-size: 12.5px;
  box-shadow: 0 6px 24px rgba(0,0,0,.18); display: none; }
#tip b { font-size: 13.5px; }
#tip .dim { color: var(--muted); }
#tip ul { margin: 3px 0 0; padding-left: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
th, td { text-align: left; padding: 5px 8px; border-bottom: 1px solid var(--border); vertical-align: middle; }
th { color: var(--ink2); font-weight: 600; font-size: 12px; }
td.n, th.n { text-align: right; font-variant-numeric: tabular-nums; }
tr.click { cursor: pointer; }
tr.click:hover td { background: var(--chip); }
.two { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }
@media (max-width: 1100px) { .main, .two { grid-template-columns: 1fr; } #panel { height: auto; } }
.rules li { margin: 3px 0; }
.btn { border: 1px solid var(--border); background: var(--chip); color: var(--ink); border-radius: 6px; padding: 1px 8px; font: inherit; font-size: 12px; cursor: pointer; }
footer { max-width: 1600px; margin: 30px auto 40px; padding: 0 18px; color: var(--muted); font-size: 12.5px; }
</style>
</head>
<body>
<header>
  <h1>Payback port coverage</h1>
  <p class="sub">Every function of the original Mac OS X build of Payback, one box each, sized by its bytes and coloured by how far the Rust / Bevy port covers it.</p>
  <p class="meta" id="meta"></p>
  <div class="stats" id="stats"></div>
  <div class="bar" id="mainbar" aria-hidden="true"></div>
  <p class="meta">Coverage figures exclude grey (not applicable) functions unless noted. The port is behavioural, not a matching decompilation: <b>green means "cited by the port and believed ported", not "proven identical"</b>.</p>
</header>
<section>
  <div class="controls">
    <div class="chips" id="statusChips" role="group" aria-label="Status filter"></div>
    <label>Area <select id="areaSel"><option value="">All areas</option></select></label>
    <input type="search" id="search" placeholder="Search name or address (Enter pins)" aria-label="Search">
    <span class="meta" id="shown"></span>
  </div>
</section>
<div class="main">
  <div id="mapwrap"><canvas id="map" aria-label="Treemap of original functions"></canvas></div>
  <aside id="panel"><p class="empty">Hover a box for details; click to pin it here.</p></aside>
</div>
<div id="tip"></div>
<section>
  <div class="two">
    <div>
      <h2>By area</h2>
      <table id="areaTable"></table>
    </div>
    <div>
      <h2>Biggest untouched game code (red)</h2>
      <table id="redTable"></table>
    </div>
  </div>
  <h2>How the colours are decided</h2>
  <ol class="rules" id="rules"></ol>
  <h2>Caveats</h2>
  <ul class="rules" id="caveats"></ul>
</section>
<footer id="foot"></footer>
<script>
const DATA = /*__DATA__*/null;
const M = DATA.meta, F = DATA.funcs;
const ST = M.statuses.map(s => s.id), AREAS = M.areas;
const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
let COL = {};
function readColours() { ST.forEach(s => COL[s] = css('--s-' + s)); COL.hdr = css('--hdr'); COL.hdrInk = css('--hdr-ink'); COL.surface = css('--surface'); COL.ink = css('--ink'); }
readColours();
const fmtB = b => b >= 1024 ? (b / 1024).toFixed(b >= 10240 ? 0 : 1) + ' KB' : b + ' B';
const pct = (a, b) => b ? (100 * a / b).toFixed(1) + '%' : '-';
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const TOTAL = F.reduce((a, f) => a + f.s, 0);
const GAME = F.filter(f => ST[f.st] !== 'grey');
const GAMEB = GAME.reduce((a, f) => a + f.s, 0);

// ---------------------------------------------------------------- summary
function tally(list) {
  const t = {}; ST.forEach(s => t[s] = {n: 0, b: 0});
  list.forEach(f => { t[ST[f.st]].n++; t[ST[f.st]].b += f.s; });
  return t;
}
const T = tally(F);
document.getElementById('meta').textContent =
  `Generated ${M.generated} from HEAD ${M.head} (working tree) by tools/coverage.py: ${M.nFuncs} Mac OS X functions, ` +
  `${M.nRust} Rust files, ${M.nDocs} notes files, ${M.nGaps} entries of remaining_gaps.html.`;
(function stats() {
  const ported = T.green.b + T.yellow.b, portedN = T.green.n + T.yellow.n;
  const cards = [
    ['Ported (green + yellow)', pct(ported, GAMEB), `${portedN} of ${GAME.length} game functions (${pct(portedN, GAME.length)}); ${fmtB(ported)} of ${fmtB(GAMEB)}`, null],
    ['Ported or documented', pct(ported + T.blue.b, GAMEB), `${portedN + T.blue.n} functions (${pct(portedN + T.blue.n, GAME.length)})`, null],
  ];
  ST.forEach(s => {
    const base = s === 'grey' ? TOTAL : GAMEB, baseN = s === 'grey' ? F.length : GAME.length;
    cards.push([M.statuses[ST.indexOf(s)].label, pct(T[s].b, base),
      `${T[s].n} functions (${pct(T[s].n, baseN)}), ${fmtB(T[s].b)}` + (s === 'grey' ? ' of all bytes' : ''), s]);
  });
  cards.push(['All bytes, grey included', pct(ported, TOTAL), `ported share of the whole binary (${fmtB(TOTAL)}, ${F.length} functions)`, null]);
  document.getElementById('stats').innerHTML = cards.map(([l, v, x, s]) =>
    `<div class="stat"><div class="l">${s ? `<span class="sw" style="background:var(--s-${s})"></span>` : ''}${esc(l)}</div><div class="v">${v}</div><div class="x">${esc(x)}</div></div>`).join('');
  document.getElementById('mainbar').innerHTML = ST.filter(s => s !== 'grey').map(s =>
    `<span title="${esc(M.statuses[ST.indexOf(s)].label)}" style="width:${100 * T[s].b / GAMEB}%;background:var(--s-${s})"></span>`).join('');
})();

// ---------------------------------------------------------------- filters
const statusOn = {}; ST.forEach(s => statusOn[s] = true);
const sc = document.getElementById('statusChips');
ST.forEach((s, i) => {
  const b = document.createElement('button');
  b.className = 'chip'; b.type = 'button'; b.setAttribute('aria-pressed', 'true');
  b.innerHTML = `<span class="sw" style="background:var(--s-${s})"></span>${esc(M.statuses[i].label)} <span class="meta" style="margin-left:6px">${T[s].n}</span>`;
  b.onclick = () => { statusOn[s] = !statusOn[s]; b.classList.toggle('off', !statusOn[s]); b.setAttribute('aria-pressed', statusOn[s]); layout(); };
  sc.appendChild(b);
});
const areaSel = document.getElementById('areaSel');
AREAS.forEach((a, i) => { if (F.some(f => f.ar === i)) areaSel.insertAdjacentHTML('beforeend', `<option value="${i}">${esc(a.name)}</option>`); });
areaSel.onchange = layout;
const search = document.getElementById('search');
let query = '';
function matches(f) {
  if (!query) return true;
  return f.n.toLowerCase().includes(query) || f.m.toLowerCase().includes(query) || f.a.includes(query.replace(/^0x/, '')) ||
    (f.os4 && f.os4.includes(query.replace(/^(fun_|0x)/, ''))) || (f.k68 && f.k68.includes(query.replace(/^(fun_|0x)/, '')));
}
search.oninput = () => { query = search.value.trim().toLowerCase(); draw(); };
search.onkeydown = e => { if (e.key === 'Enter') { const hit = rects.find(r => matches(r.f)); if (hit) pin(hit.f); } };

// ---------------------------------------------------------------- treemap
function worst(row, side, sum) {
  let mx = 0, mn = Infinity;
  for (const r of row) { mx = Math.max(mx, r.v); mn = Math.min(mn, r.v); }
  const s2 = sum * sum, w2 = side * side;
  return Math.max(w2 * mx / s2, s2 / (w2 * mn));
}
// Squarified treemap (Bruls, Huizing, van Wijk). items: [{v}] sorted descending, v in area units.
function squarify(items, x, y, w, h, out) {
  let i = 0;
  while (i < items.length) {
    const side = Math.min(w, h);
    if (side <= 0) break;
    const row = [items[i]]; let sum = items[i].v; i++;
    while (i < items.length) {
      const next = items[i];
      if (worst(row.concat(next), side, sum + next.v) <= worst(row, side, sum)) { row.push(next); sum += next.v; i++; }
      else break;
    }
    if (w >= h) {
      const rw = sum / h; let yy = y;
      for (const r of row) { const rh = r.v / rw; out.push([r, x, yy, rw, rh]); yy += rh; }
      x += rw; w -= rw;
    } else {
      const rh = sum / w; let xx = x;
      for (const r of row) { const rw = r.v / rh; out.push([r, xx, y, rw, rh]); xx += rw; }
      y += rh; h -= rh;
    }
  }
}
const canvas = document.getElementById('map'), ctx = canvas.getContext('2d');
let rects = [], groups = [], W = 0, H = 0, hover = null, pinned = null;
const HDR = 17;
function layout() {
  const wrap = document.getElementById('mapwrap');
  W = wrap.clientWidth; H = wrap.clientHeight;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(W * dpr); canvas.height = Math.round(H * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const area = areaSel.value === '' ? null : +areaSel.value;
  const vis = F.filter(f => statusOn[ST[f.st]] && (area === null || f.ar === area) && f.s > 0);
  const byArea = new Map();
  vis.forEach(f => { if (!byArea.has(f.ar)) byArea.set(f.ar, []); byArea.get(f.ar).push(f); });
  const gl = [...byArea.entries()].map(([ar, fs]) => ({ar, fs, v: fs.reduce((a, f) => a + f.s, 0)})).sort((a, b) => b.v - a.v);
  const tot = gl.reduce((a, g) => a + g.v, 0);
  const out = [];
  if (tot > 0) { gl.forEach(g => g.v = g.v / tot * W * H); squarify(gl, 0, 0, W, H, out); }
  rects = []; groups = [];
  for (const [g, gx, gy, gw, gh] of out) {
    const hdr = gh > 34 && gw > 50 ? HDR : 0, pad = 1;
    const ix = gx + pad, iy = gy + hdr + pad, iw = Math.max(0, gw - 2 * pad), ih = Math.max(0, gh - hdr - 2 * pad);
    groups.push({g, x: gx, y: gy, w: gw, h: gh, hdr});
    const sum = g.v / (W * H) * tot;
    const items = g.fs.map(f => ({f, v: f.s / sum * iw * ih})).sort((a, b) => b.v - a.v);
    const o2 = []; squarify(items, ix, iy, iw, ih, o2);
    for (const [it, x, y, w, h] of o2) rects.push({f: it.f, x, y, w, h});
  }
  document.getElementById('shown').textContent = `${vis.length} functions shown, ${fmtB(vis.reduce((a, f) => a + f.s, 0))}`;
  draw();
}
function draw() {
  ctx.fillStyle = COL.surface; ctx.fillRect(0, 0, W, H);
  const q = !!query;
  for (const r of rects) {
    const m = matches(r.f);
    ctx.globalAlpha = q && !m ? 0.18 : 1;
    ctx.fillStyle = COL[ST[r.f.st]];
    const gap = r.w > 3 && r.h > 3 ? 1 : 0;
    ctx.fillRect(r.x + gap * .5, r.y + gap * .5, Math.max(r.w - gap, .5), Math.max(r.h - gap, .5));
  }
  ctx.globalAlpha = 1;
  ctx.font = '11px system-ui, sans-serif'; ctx.textBaseline = 'middle';
  for (const r of rects) {
    if (r.w < 46 || r.h < 15) continue;
    if (q && !matches(r.f)) continue;
    ctx.save(); ctx.beginPath(); ctx.rect(r.x + 2, r.y, r.w - 4, r.h); ctx.clip();
    const st = ST[r.f.st], dark = matchMedia('(prefers-color-scheme: dark)').matches;
    ctx.fillStyle = st === 'yellow' || (st === 'grey' && !dark) ? 'rgba(0,0,0,.78)' : 'rgba(255,255,255,.92)';
    ctx.fillText(r.f.n, r.x + 3, r.y + 8);
    ctx.restore();
  }
  ctx.font = '600 11.5px system-ui, sans-serif';
  for (const g of groups) {
    ctx.strokeStyle = COL.surface; ctx.lineWidth = 2; ctx.strokeRect(g.x, g.y, g.w, g.h);
    if (!g.hdr) continue;
    ctx.fillStyle = COL.hdr; ctx.fillRect(g.x + 1, g.y + 1, g.w - 2, g.hdr - 1);
    ctx.save(); ctx.beginPath(); ctx.rect(g.x + 1, g.y, g.w - 6, g.hdr); ctx.clip();
    const fs = g.g.fs, b = fs.reduce((a, f) => a + f.s, 0), gb = fs.filter(f => ST[f.st] !== 'grey').reduce((a, f) => a + f.s, 0);
    const pb = fs.filter(f => f.st <= 1).reduce((a, f) => a + f.s, 0);
    ctx.fillStyle = COL.hdrInk;
    ctx.fillText(`${AREAS[g.g.ar].name}  ${fmtB(b)}` + (gb ? `  · ${pct(pb, gb)} ported` : ''), g.x + 5, g.y + g.hdr / 2 + 1);
    ctx.restore();
  }
  const hl = (f, col, lw) => { const r = rects.find(r => r.f === f); if (!r) return;
    ctx.strokeStyle = col; ctx.lineWidth = lw; ctx.strokeRect(r.x + 1, r.y + 1, Math.max(r.w - 2, 1), Math.max(r.h - 2, 1)); };
  if (pinned) hl(pinned, COL.ink, 2.5);
  if (hover && hover !== pinned) hl(hover, COL.ink, 1.5);
}
function hit(x, y) {
  for (const r of rects) if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) return r.f;
  return null;
}
const tip = document.getElementById('tip');
function tipHtml(f) {
  const s = ST[f.st];
  const list = (arr, n) => arr.slice(0, n).map(x => `<li>${esc(x)}</li>`).join('') + (arr.length > n ? `<li class="dim">… ${arr.length - n} more</li>` : '');
  let h = `<b>${esc(f.n)}</b><div class="dim mono">${esc(f.m)}</div>`;
  h += `<div><span class="mono">${f.a}</span> · ${fmtB(f.s)} (${f.s} B) · ${pct(f.s, TOTAL)} of all` + (s !== 'grey' ? `, ${pct(f.s, GAMEB)} of game code` : '') + `</div>`;
  h += `<div><span class="sw" style="background:var(--s-${s})"></span><b>${esc(M.statuses[f.st].label)}</b> <span class="dim">— ${esc(f.why)}</span></div>`;
  h += `<div class="dim">${esc(AREAS[f.ar].name)}${f.os4 ? ' · OS4 ' + f.os4 : ''}${f.k68 ? ' · 68k ' + f.k68 : ''}</div>`;
  if (f.rn) h += `<div>Rust (${f.rn}):<ul>${list(f.r, 5)}</ul></div>`;
  if (f.dn) h += `<div>Docs (${f.dn}):<ul>${list(f.dr, 4)}</ul></div>`;
  if (f.g.length) h += `<div>Gaps:<ul>${list(f.g, 3)}</ul></div>`;
  if (f.no.length) h += `<div>Notes:<ul>${list(f.no.map(n => `${n[0]}: ${n[2]}`), 3)}</ul></div>`;
  return h;
}
canvas.addEventListener('mousemove', e => {
  const b = canvas.getBoundingClientRect();
  const f = hit(e.clientX - b.left, e.clientY - b.top);
  if (f !== hover) { hover = f; draw(); }
  if (!f) { tip.style.display = 'none'; return; }
  tip.innerHTML = tipHtml(f); tip.style.display = 'block';
  const tw = tip.offsetWidth, th = tip.offsetHeight;
  let x = e.clientX + 14, y = e.clientY + 14;
  if (x + tw > innerWidth - 8) x = e.clientX - tw - 14;
  if (y + th > innerHeight - 8) y = Math.max(8, innerHeight - th - 8);
  tip.style.left = x + 'px'; tip.style.top = y + 'px';
});
canvas.addEventListener('mouseleave', () => { hover = null; tip.style.display = 'none'; draw(); });
canvas.addEventListener('click', e => { const b = canvas.getBoundingClientRect(); const f = hit(e.clientX - b.left, e.clientY - b.top); if (f) pin(f); });
function pin(f) {
  pinned = f; draw();
  const s = ST[f.st];
  const ul = (arr, total) => arr.length ? `<ul>${arr.map(x => `<li>${esc(x)}</li>`).join('')}${total > arr.length ? `<li class="empty">… ${total - arr.length} more</li>` : ''}</ul>` : '<p class="empty">none</p>';
  const p = document.getElementById('panel');
  p.innerHTML = `<h3>${esc(f.n)}</h3><div class="mono" style="color:var(--muted);word-break:break-all">${esc(f.d)}</div>
    <div class="kv">
      <div>Address</div><div><span class="mono" id="addr">${f.a}</span> <button class="btn" id="copy">Copy</button></div>
      <div>Mangled</div><div class="mono" style="word-break:break-all">${esc(f.m)}</div>
      <div>Size</div><div>${f.s} bytes (${pct(f.s, TOTAL)} of all${s !== 'grey' ? ', ' + pct(f.s, GAMEB) + ' of game code' : ''})</div>
      <div>Status</div><div><span class="status-pill"><span class="sw" style="background:var(--s-${s})"></span>${esc(M.statuses[f.st].label)}</span><br><span style="color:var(--muted)">${esc(f.why)}</span></div>
      <div>Area</div><div>${esc(AREAS[f.ar].name)}</div>
      <div>Amiga</div><div class="mono">${f.os4 ? 'OS4 FUN_' + f.os4 : ''}${f.k68 ? (f.os4 ? '<br>' : '') + '68k FUN_' + f.k68 : ''}${!f.os4 && !f.k68 ? '<span class="empty">no match</span>' : ''}${f.mc ? ' <span style="color:var(--muted)">(' + esc(f.mc) + ')</span>' : ''}</div>
      <div>Doc tags</div><div>${f.cf[0] || f.cf[1] ? `CONFIRMED ×${f.cf[0]}, INFERRED ×${f.cf[1]} (lines citing it)` : '<span class="empty">none</span>'}</div>
    </div>
    <b>Rust citations (${f.rn})</b>${ul(f.r, f.rn)}
    <b>Doc references (${f.dn})</b>${ul(f.dr, f.dn)}
    <b>Gaps (remaining_gaps.html)</b>${ul(f.g, f.g.length)}
    <b>Notes</b>${ul(f.no.map(n => `[${n[0]}] ${n[1]}: ${n[2]}`), f.no.length)}`;
  document.getElementById('copy').onclick = () => {
    const done = () => document.getElementById('copy').textContent = 'Copied';
    if (navigator.clipboard) navigator.clipboard.writeText(f.a).then(done, () => selectAddr());
    else selectAddr();
  };
}
function selectAddr() { const r = document.createRange(); r.selectNodeContents(document.getElementById('addr')); const s = getSelection(); s.removeAllRanges(); s.addRange(r); }

// ---------------------------------------------------------------- tables
(function areaTable() {
  const rows = AREAS.map((a, i) => ({a, i, fs: F.filter(f => f.ar === i)})).filter(r => r.fs.length);
  rows.forEach(r => { r.t = tally(r.fs); r.gb = r.fs.filter(f => ST[f.st] !== 'grey').reduce((x, f) => x + f.s, 0); r.b = r.fs.reduce((x, f) => x + f.s, 0); });
  rows.sort((x, y) => y.gb - x.gb || y.b - x.b);
  let h = `<tr><th>Area</th><th class="n">Funcs</th><th class="n">Bytes</th><th style="width:34%">Status (bytes)</th><th class="n">Ported</th><th class="n">Ported (count)</th></tr>`;
  for (const r of rows) {
    const gn = r.fs.filter(f => ST[f.st] !== 'grey').length;
    const bar = ST.map(s => r.t[s].b ? `<span style="width:${100 * r.t[s].b / r.b}%;background:var(--s-${s})" title="${s}: ${fmtB(r.t[s].b)}"></span>` : '').join('');
    h += `<tr class="click" data-a="${r.i}"><td>${esc(r.a.name)}</td><td class="n">${r.fs.length}</td><td class="n">${fmtB(r.b)}</td><td><div class="bar">${bar}</div></td>` +
      `<td class="n">${r.gb ? pct(r.t.green.b + r.t.yellow.b, r.gb) : 'n/a'}</td><td class="n">${gn ? pct(r.t.green.n + r.t.yellow.n, gn) : 'n/a'}</td></tr>`;
  }
  const t = document.getElementById('areaTable'); t.innerHTML = h;
  t.querySelectorAll('tr.click').forEach(tr => tr.onclick = () => { areaSel.value = tr.dataset.a; layout(); document.getElementById('mapwrap').scrollIntoView({behavior: 'smooth'}); });
})();
(function redTable() {
  const red = F.filter(f => ST[f.st] === 'red').sort((a, b) => b.s - a.s).slice(0, 25);
  let h = `<tr><th>Function</th><th>Address</th><th>Area</th><th class="n">Bytes</th></tr>`;
  red.forEach((f, i) => h += `<tr class="click" data-i="${F.indexOf(f)}"><td>${esc(f.n)}</td><td class="mono">${f.a}</td><td>${esc(AREAS[f.ar].name)}</td><td class="n">${f.s}</td></tr>`);
  const t = document.getElementById('redTable'); t.innerHTML = h;
  t.querySelectorAll('tr.click').forEach(tr => tr.onclick = () => { pin(F[+tr.dataset.i]); document.getElementById('panel').scrollIntoView({behavior: 'smooth', block: 'nearest'}); });
})();
document.getElementById('rules').innerHTML = [
  `<b>Boxes</b>: the ${M.nFuncs} functions of the Mac OS X build (<code>analysis/ghidra_macx/functions.tsv</code>), sized by Ghidra's byte size, grouped by area. Readable names come from <code>analysis/mac/symbols.tsv</code> (demangled CodeWarrior names).`,
  `<b>Citations</b> are found in Rust comments (<code>crates/**/*.rs</code>), the notes (${M.docs.map(d => '<code>' + esc(d) + '</code>').join(', ')}) and <code>remaining_gaps.html</code>. Recognised forms: Mac names (bare names only in backticks unless distinctive, e.g. <code>DisplayMinimap</code>; names that are also Rust types such as <code>Explosion</code> count in Rust only with an address or the mangled form), mangled names, <code>Class::Method</code>, Mac addresses (any address inside a function counts), OS4 <code>FUN_01xxxxxx</code> / bare <code>01xxxxxx</code> and 68k <code>FUN_000xxxxx</code> / <code>68k 0x…</code> addresses mapped through the Amiga↔Mac symbol match.`,
  `<span class="sw" style="background:var(--s-grey)"></span><b>Grey, not applicable</b>: Mac OS imports, PowerPlant, Mac platform classes (CD audio, DrawSprocket, InputSprocket/HID, preferences, UI), the Amiga-API, Warp3D-on-OpenGL and Sound Manager emulation layers and the MSL runtime (symbols.tsv categories), plus a few named overrides (${Object.keys(M.naOverrides).length}). Grey functions keep their citations in the tooltip but never count towards coverage.`,
  `<span class="sw" style="background:var(--s-green)"></span><b>Green, ported</b>: cited in Rust, and no gap, departure or approximation note mentions it.`,
  `<span class="sw" style="background:var(--s-yellow)"></span><b>Yellow, ported with a known approximation / gap</b>: cited in Rust, and mentioned by a <code>remaining_gaps.html</code> gap or departure, or by a doc / code-comment line saying "not reproduced", "not implemented", "approximate", "departure", "not done" or similar (that line plus the rest of its paragraph or comment block).`,
  `<span class="sw" style="background:var(--s-blue)"></span><b>Blue, documented only</b>: not cited in Rust, but analysed in the notes (or named by a gap). Often deliberately unneeded (software rasterisers, loaders replaced by the formats crate), sometimes still to do.`,
  `<span class="sw" style="background:var(--s-red)"></span><b>Red, untouched</b>: game code nothing cites.`,
  `<b>Status overrides</b> (script table <code>STATUS_OVERRIDES</code>): ` + (Object.keys(M.statusOverrides).length ?
    Object.entries(M.statusOverrides).map(([n, [s, why]]) => `<code>${esc(n)}</code> → ${esc(s)}`).join(', ') + '. Reasons: ' +
    [...new Set(Object.values(M.statusOverrides).map(v => v[1]))].map(esc).join('; ') + '.' : 'none.'),
  `<b>Areas</b> follow the source-file blocks (CodeWarrior emits functions in source order), with ${M.areaOverrides} name overrides in the script.`,
  `<b>Percentages</b>: the main figure is (green + yellow) bytes over all non-grey bytes; counts and the all-bytes figure are shown next to it.`
].map(x => `<li>${x}</li>`).join('');
document.getElementById('caveats').innerHTML = [
  'Green is "the port cites this function and nobody has noted a difference", not a proof of equivalence: a function may be cited for a single constant while its other branches are not reproduced.',
  'Citations through Amiga addresses depend on the Amiga↔Mac symbol match, which includes medium- and low-confidence pairs (shown in the detail panel).',
  'Name matching is textual; a common word that is also a function name can produce a false citation, and a function only described in prose without its name or address is missed (it shows red or blue).',
  'Gap notes are matched by keywords per line and paragraph, so a yellow may come from a note about a neighbouring function in the same paragraph, and notes that went stale after a fix still count unless listed in the script.',
  'analysis/mac/*.md are local, git-ignored review notes; they are counted as documentation.',
  'Only the Mac OS X build is drawn: the 105 OS4 functions without a Mac counterpart (Amiga-only code) are not boxes.'
].map(x => `<li>${esc(x)}</li>`).join('');
document.getElementById('foot').textContent = 'Self-contained page; regenerate with python tools/coverage.py.';
window.addEventListener('resize', () => layout());
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { readColours(); draw(); });
layout();
</script>
</body>
</html>
"""

if __name__ == "__main__":
    sys.exit(main())
