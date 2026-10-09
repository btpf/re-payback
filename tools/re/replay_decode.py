"""Payback (2001) replay decoder.

Formats (all big-endian):
  v3     (ISO 68k Payback.exe Jan-2001, loader @file 0x7a9e2): 10 x i32 + char[100]          -> 144-byte header
  v10    (68k Payback.exe 2002 FUN_00094160 / WOS)              : 12 x i32 + char[100] + i32 -> 152-byte header
  v0x3F2 (AmigaOS4 FUN_010bc000)                                 : same layout as v10
Then <wordCount> u16 records; a record with bit15 set is followed by a NUL-terminated
comment string, padded with one byte so the stream stays 16-bit aligned.

Record word: b15 comment-follows | b14 turn-left | b13 turn-right | b12 accelerate |
             b11 reverse/brake | b10 shoot | b9 in/out vehicle | b8 brake-only |
             b7..b3 weapon-select (0..7, 10 = none) | b2..b0 game ticks (1/30 s) this frame.
0xFFFF = end-of-data sentinel (returned by the reader when the buffer is exhausted; not stored).
"""
import struct, sys, os, collections

CTRL = {-1: 'Off', 12: 'Keyboard'}
def ctrl_name(v):
    if v in CTRL: return CTRL[v]
    if 0 <= v < 4: return f'Joystick {v}'
    if 4 <= v < 8: return f'PSX Joypad {v-4}'
    if 8 <= v < 12: return f'CD32 Joypad {v-8}'
    return f'?{v}'

CITY = {0: 'Freedom City', 1: 'Los Francos City', 2: 'Corona City'}
LEVEL = {2180: 'Ready Or Not', 2181: 'Gangster Tripping', 2182: 'Going For Gold',
         2183: 'Remote Control', 2184: 'Caught By The Fuzz', 2185: 'Born Slippy'}
POLICE = ['Very low', 'Low', 'Average', 'Above average', 'High', 'Very high']
BITS = [(14, 'L'), (13, 'R'), (12, 'A'), (11, 'B'), (10, 'F'), (9, 'I'), (8, 'b')]

def map_index(name):
    for i in range(3):
        if name == f'PROGDIR:Maps/{i}': return i
    return None  # user map (index >= 3 depends on UserMaps list) or not found

def parse(data):
    i32 = lambda o: struct.unpack_from('>i', data, o)[0]
    magic = i32(0)
    h = {'magic': magic}
    if magic == 3:
        names = ['numPlayers', 'ctrl0', 'ctrl1', 'ctrl2', 'ctrl3', 'requiredKills',
                 'wordCount', 'totalTicks', 'rngSeed', 'levelCode']
        for k, n in enumerate(names): h[n] = i32(4 + 4 * k)
        raw = data[0x2C:0x90]; off = 0x90
        h['levelSub'] = h['levelCode'] & 1   # NB: v3 loader discards it (levelCode := mapIdx*2)
    elif magic in (10, 0x3F2):
        names = ['numPlayers', 'ctrl0', 'ctrl1', 'ctrl2', 'ctrl3', 'requiredKills',
                 'wordCount', 'totalTicks', 'rngSeed', 'rampage', 'policeAggr']
        for k, n in enumerate(names): h[n] = i32(4 + 4 * k)
        raw = data[0x30:0x94]; h['levelSub'] = i32(0x94); off = 0x98
    else:
        raise ValueError(f'unknown replay version {magic:#x}')
    h['mapName'] = raw.split(b'\0')[0].decode('latin-1')
    h['mapNameTailGarbage'] = raw[len(h['mapName']) + 1:].hex()
    recs, comments = [], []
    pos = 0x14  # game's byte counter DAT_0114c8c4 starts at 0x14
    for n in range(h['wordCount']):
        w = struct.unpack_from('>H', data, off)[0]; off += 2; pos += 2
        recs.append(w)
        if w & 0x8000:
            e = data.index(b'\0', off)
            s = data[off:e]; pos += e + 1 - off; off = e + 1
            if pos & 1: off += 1; pos += 1
            comments.append((n, s.decode('latin-1')))
    h['trailingBytes'] = len(data) - off
    return h, recs, comments

def decode_word(w):
    return {'keys': ''.join(c if w >> b & 1 else '.' for b, c in BITS),
            'weapon': (w >> 3) & 0x1F, 'ticks': w & 7, 'comment': bool(w & 0x8000)}

def report(path):
    data = open(path, 'rb').read()
    h, recs, comments = parse(data)
    print(f'=== {os.path.basename(path)}  ({len(data)} bytes)')
    for k, v in h.items(): print(f'  {k:20s} {v}')
    np = max(1, h['numPlayers'])
    mi = map_index(h['mapName'])
    code = (mi * 2 + h['levelSub']) if mi is not None else None
    print('  -> controllers     ', [ctrl_name(h[f"ctrl{i}"]) for i in range(4)], '(only meaningful if numPlayers>=2)')
    print('  -> city            ', CITY.get(mi, 'user map / not found'), f'(map index {mi})')
    if np < 2 and not h.get('rampage'):
        print('  -> level           ', code, LEVEL.get(2180 + (code or 0)))
    elif h.get('rampage'):
        print('  -> rampage, police ', POLICE[h['policeAggr']] if 0 <= h['policeAggr'] < 6 else h['policeAggr'])
    t = h['totalTicks']
    print(f'  -> duration         {t} ticks = {t/30:.2f} s ({t//1800}:{t%1800//30:02d}.{(t%30)*100//30:02d})')
    print(f'  -> avg framerate    {h["wordCount"]*30//(np*t)} fps (game formula words*30/(players*ticks)); exact {h["wordCount"]*30/(np*t):.2f}')
    ticks = sum(w & 7 for w in recs[::np])
    print(f'  check: sum(ticks of every {np}th word) = {ticks}  (header {t})  trailing={h["trailingBytes"]} comments={comments}')
    tick_hist = collections.Counter(w & 7 for w in recs)
    weap = collections.Counter((w >> 3) & 0x1F for w in recs)
    keys = collections.Counter()
    for w in recs:
        for b, c in BITS:
            if w >> b & 1: keys[c] += 1
    print('  tick histogram', dict(sorted(tick_hist.items())))
    print('  weapon-field histogram', dict(sorted(weap.items())))
    print('  key-held frame counts (L=left R=right A=accel B=reverse F=fire I=in/out b=brake)', dict(keys))
    print('  first 24 records:')
    for n, w in enumerate(recs[:24]):
        d = decode_word(w); print(f'    #{n:4d} {w:04x} keys={d["keys"]} weapon={d["weapon"]:2d} ticks={d["ticks"]}')
    # weapon change events
    ev = [(n, (w >> 3) & 0x1F) for n, w in enumerate(recs) if (w >> 3) & 0x1F != 10]
    print('  weapon-select events (record#, weapon):', ev[:20], '...' if len(ev) > 20 else '')
    print()

if __name__ == '__main__':
    from common import GAME
    paths = sys.argv[1:] or [os.path.join(GAME, 'Replays', f)
                             for f in ('Race for the prize', 'Tanks a lot')]
    for p in paths: report(p)
