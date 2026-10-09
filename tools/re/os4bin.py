#!/usr/bin/env python3
"""Read the AmigaOS 4 executable (PaybackOS4, ELF32 PowerPC) by virtual address.

    python tools/re/os4bin.py str START [END]       C strings from START (to END)
    python tools/re/os4bin.py xref ADDR [ADDR ...]   every `bl` that calls ADDR
    python tools/re/os4bin.py dis ADDR COUNT         disassemble COUNT instructions (capstone)

Reads extracted/os4/PaybackUpdateOS4/install_files/PaybackOS4 (tools/extract/extract.py).
Importable: `from os4bin import rd, cstr, u32, i32, SEG, D`.
"""
import struct
import sys

from common import OS4_ELF

D = OS4_ELF.read_bytes()
_phoff, = struct.unpack(">I", D[28:32])
_phentsize, _phnum = struct.unpack(">HH", D[42:46])
SEG = []  # (vaddr, file offset, file size, memory size) of the PT_LOAD segments
for _i in range(_phnum):
    _p = D[_phoff + _i * _phentsize:_phoff + (_i + 1) * _phentsize]
    _t, _off, _va, _pa, _fs, _ms = struct.unpack(">6I", _p[:24])
    if _t == 1:
        SEG.append((_va, _off, _fs, _ms))


def rd(va, n):
    for s, o, fs, ms in SEG:
        if s <= va < s + ms:
            r = D[o + va - s:o + min(va - s + n, fs)]
            return r + b"\0" * (n - len(r))
    raise KeyError(hex(va))


def cstr(va):
    return rd(va, 512).split(b"\0")[0].decode("latin-1")


def u32(va):
    return struct.unpack(">I", rd(va, 4))[0]


def i32(va):
    return struct.unpack(">i", rd(va, 4))[0]


def xref(targets):
    for va, off, fs, _ in SEG:
        for i in range(0, fs - 3, 4):
            w = struct.unpack(">I", D[off + i:off + i + 4])[0]
            if (w >> 26) == 18 and (w & 3) == 1:  # bl
                li = w & 0x03FFFFFC
                if li & 0x02000000:
                    li -= 0x04000000
                t = (va + i + li) & 0xFFFFFFFF
                if t in targets:
                    print(hex(t), "<-", hex(va + i))


def dis(addr, count):
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_PPC, capstone.CS_MODE_32 | capstone.CS_MODE_BIG_ENDIAN)
    for ins in md.disasm(rd(addr, count * 4), addr):
        print(hex(ins.address), ins.mnemonic, ins.op_str)


def main(argv):
    if len(argv) < 2 or argv[0] not in ("str", "xref", "dis"):
        sys.exit(__doc__)
    cmd, args = argv[0], [int(x, 16) for x in argv[1:]]
    if cmd == "str":
        a = args[0]
        e = args[1] if len(args) > 1 else a + 1
        while a < e:
            s = cstr(a)
            print(hex(a), repr(s))
            a += max(4, (len(s) + 4) & ~3)
    elif cmd == "xref":
        xref(set(args))
    else:
        dis(args[0], args[1] if len(args) > 1 else 16)


if __name__ == "__main__":
    main(sys.argv[1:])
