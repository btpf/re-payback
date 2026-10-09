"""Minimal AmigaOS hunk executable parser.

Usage: python hunk.py <exe> [outdir]
Prints the hunk layout; if outdir is given, writes each hunk's raw bytes
(hunk_NN_TYPE.bin) and a JSON file with relocations and symbols.
"""
import json, os, struct, sys

HUNK_NAMES = {0x3E7: 'UNIT', 0x3E8: 'NAME', 0x3E9: 'CODE', 0x3EA: 'DATA', 0x3EB: 'BSS',
              0x3EC: 'RELOC32', 0x3ED: 'RELOC16', 0x3EE: 'RELOC8', 0x3EF: 'EXT',
              0x3F0: 'SYMBOL', 0x3F1: 'DEBUG', 0x3F2: 'END', 0x3F3: 'HEADER',
              0x3F7: 'DREL32', 0x3F8: 'DREL16', 0x3F9: 'DREL8', 0x3FC: 'RELOC32SHORT'}


class Reader:
    def __init__(self, data):
        self.d, self.p = data, 0

    def u32(self):
        v = struct.unpack_from('>I', self.d, self.p)[0]
        self.p += 4
        return v

    def u16(self):
        v = struct.unpack_from('>H', self.d, self.p)[0]
        self.p += 2
        return v

    def bytes(self, n):
        b = self.d[self.p:self.p + n]
        self.p += n
        return b

    def name(self, nlongs):
        return self.bytes(nlongs * 4).rstrip(b'\0').decode('latin-1')


def parse(data):
    r = Reader(data)
    assert r.u32() == 0x3F3, 'not a hunk executable'
    while r.u32():  # resident library names
        pass
    table_size, first, last = r.u32(), r.u32(), r.u32()
    sizes = [r.u32() for _ in range(last - first + 1)]
    hunks, cur = [], None
    while r.p < len(r.d):
        raw = r.u32()
        t = raw & 0x3FFFFFFF
        if t in (0x3E9, 0x3EA):
            n = r.u32() & 0x3FFFFFFF
            cur = {'type': HUNK_NAMES[t], 'mem': raw >> 30, 'size': n * 4,
                   'data': r.bytes(n * 4), 'relocs': {}, 'symbols': [], 'debug': []}
            hunks.append(cur)
        elif t == 0x3EB:
            n = r.u32() & 0x3FFFFFFF
            cur = {'type': 'BSS', 'mem': raw >> 30, 'size': n * 4, 'data': b'',
                   'relocs': {}, 'symbols': [], 'debug': []}
            hunks.append(cur)
        elif t in (0x3EC, 0x3F7):
            while True:
                n = r.u32()
                if not n:
                    break
                target = r.u32()
                cur['relocs'].setdefault(target, []).extend(r.u32() for _ in range(n))
        elif t == 0x3FC:
            while True:
                n = r.u16()
                if not n:
                    break
                target = r.u16()
                cur['relocs'].setdefault(target, []).extend(r.u16() for _ in range(n))
            if r.p % 4:
                r.p += 2
        elif t == 0x3F0:
            while True:
                n = r.u32()
                if not n:
                    break
                nm = r.name(n)
                cur['symbols'].append((nm, r.u32()))
        elif t == 0x3F1:
            n = r.u32()
            cur['debug'].append(r.bytes(n * 4))
        elif t == 0x3E8:
            r.name(r.u32())
        elif t == 0x3F2:
            pass
        else:
            raise ValueError(f'unhandled hunk type {raw:#x} at {r.p - 4:#x}')
    return sizes, hunks


def flatten(hunks, base):
    """Lay hunks out contiguously from `base` (4 KiB aligned), zero-fill BSS
    and apply RELOC32. Returns (image bytes, list of hunk start addresses)."""
    starts, addr = [], base
    for h in hunks:
        starts.append(addr)
        addr += (h['size'] + 0xFFF) & ~0xFFF
    img = bytearray(addr - base)
    for h, s in zip(hunks, starts):
        img[s - base:s - base + len(h['data'])] = h['data']
    for h, s in zip(hunks, starts):
        for target, offsets in h['relocs'].items():
            for off in offsets:
                p = s - base + off
                v = struct.unpack_from('>I', img, p)[0]
                struct.pack_into('>I', img, p, (v + starts[target]) & 0xFFFFFFFF)
    return bytes(img), starts


def main():
    if sys.argv[1] == '--flat':
        # python hunk.py --flat <exe> <out.bin> [base]  -> also writes <out.bin>.map
        path, out = sys.argv[2], sys.argv[3]
        base = int(sys.argv[4], 0) if len(sys.argv) > 4 else 0x10000
        _, hunks = parse(open(path, 'rb').read())
        img, starts = flatten(hunks, base)
        open(out, 'wb').write(img)
        with open(out + '.map', 'w') as f:
            for i, (h, s) in enumerate(zip(hunks, starts)):
                f.write(f"{i}\t{h['type']}\t{s:#010x}\t{h['size']:#x}\n")
        print(f'{out}: {len(img)} bytes at {base:#x}, entry {starts[0]:#x}')
        return
    path = sys.argv[1]
    sizes, hunks = parse(open(path, 'rb').read())
    for i, h in enumerate(hunks):
        nrel = sum(len(v) for v in h['relocs'].values())
        print(f"{i:2d} {h['type']:5s} size={h['size']:8d} relocs={nrel:6d} "
              f"syms={len(h['symbols']):5d} debug={len(h['debug'])}")
    if len(sys.argv) > 2:
        out = sys.argv[2]
        os.makedirs(out, exist_ok=True)
        meta = []
        for i, h in enumerate(hunks):
            with open(os.path.join(out, f"hunk_{i:02d}_{h['type']}.bin"), 'wb') as f:
                f.write(h['data'])
            meta.append({'index': i, 'type': h['type'], 'size': h['size'],
                         'relocs': {str(k): v for k, v in h['relocs'].items()},
                         'symbols': h['symbols']})
        json.dump(meta, open(os.path.join(out, 'hunks.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
