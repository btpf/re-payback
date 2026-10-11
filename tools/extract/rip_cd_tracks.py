#!/usr/bin/env python3
"""Split the CD audio tracks out of a BIN/CUE image of the Payback CD.

The game plays the CD's Red Book tracks 2-16 as music (crates/payback/src/music.rs,
docs/re/frontend_audio.md §4.6). Neither Payback.iso nor PaybackMac.toast holds
them; a raw BIN/CUE rip of the disc does. This writes one file per audio track,
named `NN.wav` (or `NN.flac` with --flac, which needs ffmpeg), into the given
folder, usually the `Music/` folder next to payback.exe.

Usage:
  python tools/extract/rip_cd_tracks.py [CUE] [OUT_DIR] [--flac]

Defaults: CUE = "Payback Game + Updates/cd/Payback-CD.cue",
OUT_DIR = target/release/Music.

Supports one FILE ... BINARY entry with 2352-byte sectors (the common
single-bin rip). A track runs from its INDEX 01 to the next track's INDEX 00
(or INDEX 01 when it has none), so the gaps stay with the track before them,
as a CD player would play them. PREGAP lines are not stored in the bin.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import wave

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SECTOR = 2352


def msf(s):
    m, sec, f = (int(x) for x in s.split(":"))
    return (m * 60 + sec) * 75 + f


def parse_cue(path):
    tracks, cur, binfile = [], None, None
    with open(path, encoding="latin-1") as fh:
        for line in fh:
            line = line.strip()
            if m := re.match(r'FILE\s+"(.+)"\s+(\w+)', line):
                if binfile is not None:
                    sys.exit("multi-file cue sheets are not supported")
                if m.group(2).upper() != "BINARY":
                    sys.exit(f"unsupported file type {m.group(2)}")
                binfile = os.path.join(os.path.dirname(path), m.group(1))
            elif m := re.match(r"TRACK\s+(\d+)\s+(\S+)", line):
                cur = {"num": int(m.group(1)), "mode": m.group(2).upper(), "idx": {}}
                tracks.append(cur)
            elif m := re.match(r"INDEX\s+(\d+)\s+(\S+)", line):
                cur["idx"][int(m.group(1))] = msf(m.group(2))
    return binfile, tracks


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cue", nargs="?", default=os.path.join(ROOT, "Payback Game + Updates", "cd", "Payback-CD.cue"))
    ap.add_argument("out", nargs="?", default=os.path.join(ROOT, "target", "release", "Music"))
    ap.add_argument("--flac", action="store_true", help="encode FLAC with ffmpeg instead of WAV")
    args = ap.parse_args()

    if args.flac and not shutil.which("ffmpeg"):
        sys.exit("--flac needs ffmpeg on PATH")
    binfile, tracks = parse_cue(args.cue)
    total = os.path.getsize(binfile) // SECTOR
    os.makedirs(args.out, exist_ok=True)

    with open(binfile, "rb") as fh:
        for i, t in enumerate(tracks):
            if t["mode"] != "AUDIO":
                continue
            start = t["idx"][1]
            if i + 1 < len(tracks):
                nxt = tracks[i + 1]["idx"]
                end = nxt.get(0, nxt[1])
            else:
                end = total
            fh.seek(start * SECTOR)
            pcm = fh.read((end - start) * SECTOR)  # 16-bit LE stereo 44.1 kHz
            wav = os.path.join(args.out, f"{t['num']:02d}.wav")
            with wave.open(wav, "wb") as w:
                w.setnchannels(2)
                w.setsampwidth(2)
                w.setframerate(44100)
                w.writeframes(pcm)
            secs = (end - start) / 75
            if args.flac:
                flac = wav[:-4] + ".flac"
                subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", wav, flac], check=True)
                os.remove(wav)
                wav = flac
            print(f"track {t['num']:2d}  {int(secs // 60)}:{secs % 60:05.2f}  {wav}")


if __name__ == "__main__":
    main()
