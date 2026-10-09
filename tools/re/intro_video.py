"""Decoder for Payback (2001) intro video/audio.

intro.video = Autodesk Animator Pro FLC (magic 0xAF12, little-endian), 320x200x8.
intro.audio = raw signed 8-bit mono PCM, played in 1470-byte chunks, one per
video frame (68k player FUN_000106c0, audio chunk via FUN_0001749e with Paula
period 163 -> ~21.7-22 kHz; nominal 22050 Hz = 1470 * 15 fps).

Usage: python tools/re/intro_video.py [--all | frame indices...]
Writes out/img/intro/intro_frame_XXXX.png, intro_audio.wav and intro_contact.png.
"""
import os
import struct
import sys
import wave

import numpy as np
from PIL import Image

from common import GAME, OUT as OUTROOT
VID = str(GAME / 'Videos' / 'intro.video')
AUD = str(GAME / 'Videos' / 'intro.audio')
OUT = str(OUTROOT / 'img' / 'intro')

AUDIO_CHUNK = 0x5BE        # 1470 bytes per video frame
AUDIO_LIMIT = 0x97D36      # player stops feeding audio once frame*1470 >= this
AUDIO_RATE = 22050         # nominal (Paula period 163: PAL 21760 Hz, NTSC 21961 Hz)


def parse_header(d):
    (size, magic, frames, w, h, depth, flags, speed, _res, created, creator,
     updated, updater, adx, ady) = struct.unpack_from('<IHHHHHHIHIIIIHH', d, 0)
    oframe1, oframe2 = struct.unpack_from('<II', d, 0x50)
    return dict(size=size, magic=magic, frames=frames, width=w, height=h,
                depth=depth, flags=flags, speed=speed, created=created,
                creator=struct.pack('<I', creator), updated=updated,
                updater=struct.pack('<I', updater), aspect=(adx, ady),
                oframe1=oframe1, oframe2=oframe2)


def s8(b):
    return b - 256 if b > 127 else b


def chunk_color(data, pal, shift):
    p = 0
    npk = struct.unpack_from('<H', data, p)[0]; p += 2
    idx = 0
    for _ in range(npk):
        idx += data[p]; cnt = data[p + 1] or 256; p += 2
        for _ in range(cnt):
            pal[idx & 0xFF] = [data[p] << shift & 0xFF, data[p + 1] << shift & 0xFF,
                               data[p + 2] << shift & 0xFF]
            p += 3; idx += 1


def chunk_brun(data, fb, w, h):
    p = 0
    for y in range(h):
        p += 1                      # obsolete packet-count byte (ignored by player)
        x = 0
        row = fb[y]
        while x < w:
            c = data[p]; p += 1
            if c < 0x80:            # replicate next byte c times
                row[x:x + c] = data[p]; p += 1; x += c
            else:                   # literal run of 256-c bytes
                n = 256 - c
                row[x:x + n] = np.frombuffer(data, np.uint8, n, p); p += n; x += n


def chunk_ss2(data, fb, w):
    """FLC DELTA_FLC (type 7), word-oriented."""
    flat = fb.reshape(-1)
    p = 0
    lines = struct.unpack_from('<H', data, p)[0]; p += 2
    y = 0
    for _ in range(lines):
        while True:
            op = struct.unpack_from('<H', data, p)[0]; p += 2
            if op & 0x8000 == 0:
                break
            if op & 0x4000:         # 0xC000..0xFFFF: skip -op lines
                y += 0x10000 - op
            else:                   # 0x8000..0xBFFF: last pixel of line = low byte
                flat[y * w + w - 1] = op & 0xFF
        x = 0
        for _ in range(op):        # op = packet count
            x += data[p]; c = s8(data[p + 1]); p += 2
            o = y * w + x
            if c >= 0:              # c literal words
                n = 2 * c
                flat[o:o + n] = np.frombuffer(data, np.uint8, n, p); p += n; x += n
            else:                   # repeat one word -c times
                n = -c
                flat[o:o + 2 * n] = np.tile(np.frombuffer(data, np.uint8, 2, p), n)
                p += 2; x += 2 * n
        y += 1


def frames(path=VID):
    d = open(path, 'rb').read()
    hd = parse_header(d)
    assert hd['magic'] in (0xAF11, 0xAF12), hex(hd['magic'])
    w, h = hd['width'], hd['height']
    fb = np.zeros((h, w), np.uint8)
    pal = np.zeros((256, 3), np.uint8)
    off = 0x80
    n = 0
    while n < hd['frames'] and off < len(d):        # ring frame (index frames) not played
        fsize, ftype, nch = struct.unpack_from('<IHH', d, off)
        if ftype == 0xF100:                         # prefix chunk: skipped, not a frame
            off += fsize
            continue
        assert ftype == 0xF1FA, (n, hex(off), hex(ftype))
        p = off + 16
        for _ in range(nch):
            csize, ctype = struct.unpack_from('<IH', d, p)
            body = d[p + 6:p + csize]
            if ctype == 4:
                chunk_color(body, pal, 0)
            elif ctype == 11:
                chunk_color(body, pal, 2)
            elif ctype == 7:
                chunk_ss2(body, fb, w)
            elif ctype == 13:
                fb[:] = 0
            elif ctype == 15:
                chunk_brun(body, fb, w, h)
            elif ctype == 16:
                fb[:] = np.frombuffer(body, np.uint8, w * h).reshape(h, w)
            elif ctype == 18:
                pass
            else:
                raise ValueError('Unrecognised chunk type %04x' % ctype)
            p += (csize + 1) & ~1
        yield n, fb, pal
        off += fsize
        n += 1


def write_audio(path=AUD, out=None):
    a = open(path, 'rb').read()
    pcm = (np.frombuffer(a, np.int8).astype(np.int16) + 128).astype(np.uint8)  # WAV 8-bit is unsigned
    out = out or os.path.join(OUT, 'intro_audio.wav')
    with wave.open(out, 'wb') as wv:
        wv.setnchannels(1); wv.setsampwidth(1); wv.setframerate(AUDIO_RATE)
        wv.writeframes(pcm.tobytes())
    return out, len(a)


def main(argv):
    os.makedirs(OUT, exist_ok=True)
    hd = parse_header(open(VID, 'rb').read(128))
    print({k: v for k, v in hd.items()})
    want = None
    if argv and argv[0] != '--all':
        want = set(int(x) for x in argv)
    thumbs = []
    for n, fb, pal in frames():
        img = Image.fromarray(pal[fb], 'RGB')
        if want is None or n in want:
            img.save(os.path.join(OUT, 'intro_frame_%04d.png' % n))
        if n % 25 == 0:
            thumbs.append(img.resize((160, 100)))
    sheet = Image.new('RGB', (160 * 6, 100 * ((len(thumbs) + 5) // 6)))
    for i, t in enumerate(thumbs):
        sheet.paste(t, ((i % 6) * 160, (i // 6) * 100))
    sheet.save(os.path.join(OUT, 'intro_contact.png'))
    print('decoded', n + 1, 'frames')
    print('audio', write_audio())


if __name__ == '__main__':
    main(sys.argv[1:])
