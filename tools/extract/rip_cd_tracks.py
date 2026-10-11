#!/usr/bin/env python3
"""Split the CD audio tracks out of a BIN/CUE image of the Payback CD.

The game plays the CD's Red Book tracks 2-16 as music (crates/payback/src/music.rs,
docs/re/frontend_audio.md §4.6). Neither Payback.iso nor PaybackMac.toast holds
them; a raw BIN/CUE rip of the disc does. This writes one file per audio track,
named `NN.wav` (or `NN.flac` with --flac, which needs ffmpeg), into the given
folder, usually the `Music/` folder next to payback.exe.

Usage:
  python tools/extract/rip_cd_tracks.py [SOURCE] [OUT_DIR] [--flac]

SOURCE is a .cue file or a .zip holding one (read in place, not unpacked).
Default: the first of "Payback Game + Updates/Payback (Europe).zip" (the
Redump set, one .bin per track) and "Payback Game + Updates/cd/Payback-CD.cue"
(one .bin for the whole disc) that exists. OUT_DIR defaults to
target/release/Music.

Any number of FILE ... BINARY entries with 2352-byte sectors. A track runs
from its INDEX 01 to the next track's INDEX 00 in the same file (or INDEX 01
when it has none), else to the end of its file; the gaps between tracks
(INDEX 00 to 01) are left out. PREGAP lines are not stored in the image.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import wave
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SECTOR = 2352
DEFAULT_SOURCES = [
    os.path.join(ROOT, "Payback Game + Updates", "Payback (Europe).zip"),
    os.path.join(ROOT, "Payback Game + Updates", "cd", "Payback-CD.cue"),
]


def msf(s):
    m, sec, f = (int(x) for x in s.split(":"))
    return (m * 60 + sec) * 75 + f


class Source:
    """The cue sheet and its .bin files, on disk or inside a zip."""

    def __init__(self, path):
        if path.lower().endswith(".zip"):
            self.zip = zipfile.ZipFile(path)
            cues = [n for n in self.zip.namelist() if n.lower().endswith(".cue")]
            if len(cues) != 1:
                sys.exit(f"{path}: expected one .cue, found {cues}")
            self.base = os.path.dirname(cues[0])
            self.cue = self.zip.read(cues[0]).decode("latin-1")
        else:
            self.zip = None
            self.base = os.path.dirname(path)
            with open(path, encoding="latin-1") as fh:
                self.cue = fh.read()

    def _name(self, name):
        return f"{self.base}/{name}" if self.zip and self.base else os.path.join(self.base, name)

    def sectors(self, name):
        if self.zip:
            return self.zip.getinfo(self._name(name)).file_size // SECTOR
        return os.path.getsize(self._name(name)) // SECTOR

    def read(self, name, start, end):
        with (self.zip.open(self._name(name)) if self.zip else open(self._name(name), "rb")) as fh:
            fh.seek(start * SECTOR)
            return fh.read((end - start) * SECTOR)


def parse_cue(text):
    tracks, cur, binfile = [], None, None
    for line in text.splitlines():
        line = line.strip()
        if m := re.match(r'FILE\s+"(.+)"\s+(\w+)', line):
            if m.group(2).upper() != "BINARY":
                sys.exit(f"unsupported file type {m.group(2)}")
            binfile = m.group(1)
        elif m := re.match(r"TRACK\s+(\d+)\s+(\S+)", line):
            cur = {"num": int(m.group(1)), "mode": m.group(2).upper(), "file": binfile, "idx": {}}
            tracks.append(cur)
        elif m := re.match(r"INDEX\s+(\d+)\s+(\S+)", line):
            cur["idx"][int(m.group(1))] = msf(m.group(2))
    return tracks


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("source", nargs="?", help=".cue or .zip (default: see the module doc)")
    ap.add_argument("out", nargs="?", default=os.path.join(ROOT, "target", "release", "Music"))
    ap.add_argument("--flac", action="store_true", help="encode FLAC with ffmpeg instead of WAV")
    args = ap.parse_args()

    if args.flac and not shutil.which("ffmpeg"):
        sys.exit("--flac needs ffmpeg on PATH")
    path = args.source or next((p for p in DEFAULT_SOURCES if os.path.exists(p)), None)
    if not path:
        sys.exit("no source given and none of the defaults exists:\n  " + "\n  ".join(DEFAULT_SOURCES))
    print(f"source: {path}")
    src = Source(path)
    tracks = parse_cue(src.cue)
    os.makedirs(args.out, exist_ok=True)

    for i, t in enumerate(tracks):
        if t["mode"] != "AUDIO":
            continue
        start = t["idx"][1]
        nxt = tracks[i + 1] if i + 1 < len(tracks) else None
        if nxt and nxt["file"] == t["file"]:
            end = nxt["idx"].get(0, nxt["idx"][1])
        else:
            end = src.sectors(t["file"])
        pcm = src.read(t["file"], start, end)  # 16-bit LE stereo 44.1 kHz
        out = os.path.join(args.out, f"{t['num']:02d}.wav")
        with wave.open(out, "wb") as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(44100)
            w.writeframes(pcm)
        if args.flac:
            flac = out[:-4] + ".flac"
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", out, flac], check=True)
            os.remove(out)
            out = flac
        secs = (end - start) / 75
        print(f"track {t['num']:2d}  {int(secs // 60)}:{secs % 60:05.2f}  {out}")


if __name__ == "__main__":
    main()
