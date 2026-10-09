"""Minimal PEF reader: unpack the pattern-initialised data section and apply the loader
relocations, so TOC entries (r2-relative globals) can be resolved to their targets."""
import struct, os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))


def arg(d, i):
    v = 0
    while True:
        b = d[i]; i += 1
        v = (v << 7) | (b & 0x7f)
        if not b & 0x80:
            return v, i


def unpack(p, total):
    out = bytearray(); i = 0
    while i < len(p):
        op = p[i] >> 5; cnt = p[i] & 0x1f; i += 1
        if cnt == 0:
            cnt, i = arg(p, i)
        if op == 0:
            out += b'\0' * cnt
        elif op == 1:
            out += p[i:i + cnt]; i += cnt
        elif op == 2:
            rep, i = arg(p, i)
            blk = p[i:i + cnt]; i += cnt
            out += blk * (rep + 1)
        elif op == 3:
            cs, i = arg(p, i); rc, i = arg(p, i)
            common = p[i:i + cnt]; i += cnt
            for _ in range(rc):
                out += common + p[i:i + cs]; i += cs
            out += common
        elif op == 4:
            cs, i = arg(p, i); rc, i = arg(p, i)
            for _ in range(rc):
                out += b'\0' * cnt + p[i:i + cs]; i += cs
            out += b'\0' * cnt
        else:
            raise ValueError(op)
    out += b'\0' * (total - len(out))
    return out


def load(path, bases):
    """bases: list of load addresses per instantiated section (code, data)."""
    d = open(path, 'rb').read()
    sc, isc = struct.unpack('>HH', d[32:36])
    secs = [struct.unpack('>iIIIIIBBBB', d[40 + i * 28:40 + (i + 1) * 28]) for i in range(sc)]
    mem = []
    loader = None
    for s in secs:
        kind = s[6]
        raw = d[s[5]:s[5] + s[4]]
        if kind == 0 or kind == 1:
            mem.append(bytearray(raw) + b'\0' * (s[2] - len(raw)))
        elif kind == 2:
            mem.append(unpack(raw, s[2]))
        elif kind == 4:
            loader = raw
    # loader header
    (mainS, mainO, initS, initO, termS, termO, nImpLib, nImpSym, nRelSec, relInstrOff,
     strOff, hashOff, hashPow, nExp) = struct.unpack('>iIiIiIIIIIIIII', loader[:56])
    p = 56 + nImpLib * 24 + nImpSym * 4
    for r in range(nRelSec):
        sidx, _, rcount, firstOff = struct.unpack('>HHII', loader[p + r * 12:p + r * 12 + 12])
        instr = loader[relInstrOff + firstOff: relInstrOff + firstOff + rcount * 2]
        apply_relocs(mem[sidx], instr, bases)
    return mem


def apply_relocs(sec, instr, bases):
    codeA, dataA = bases[0], bases[1]
    addr = 0; symi = 0
    i = 0
    words = [struct.unpack('>H', instr[k:k + 2])[0] for k in range(0, len(instr), 2)]

    def add(base):
        nonlocal addr
        v = struct.unpack('>I', sec[addr:addr + 4])[0]
        sec[addr:addr + 4] = struct.pack('>I', (v + base) & 0xffffffff)
        addr += 4
    rpt_stack = None
    while i < len(words):
        w = words[i]
        if w >> 14 == 0:  # RelocBySectDWithSkip
            skip = (w >> 6) & 0xff; cnt = w & 0x3f
            addr += skip * 4
            for _ in range(cnt):
                add(dataA)
            i += 1
        elif w >> 13 == 0b010:  # RelocRun group
            sub = (w >> 9) & 0xf; cnt = (w & 0x1ff) + 1
            for _ in range(cnt):
                if sub == 0: add(codeA)
                elif sub == 1: add(dataA)
                elif sub == 2: add(codeA); add(dataA); addr += 4
                elif sub == 3: add(codeA); add(dataA)
                elif sub == 4: add(dataA); addr += 4
                elif sub == 5: addr += 4; symi += 1  # import symbol, leave
                else: raise ValueError('run', sub)
            i += 1
        elif w >> 13 == 0b011:  # RelocSmIndex group
            sub = (w >> 9) & 0xf; idx = w & 0x1ff
            if sub == 0: addr += 4; symi = idx + 1
            elif sub == 1: pass  # set code section base
            elif sub == 2: pass
            elif sub == 3: pass
            else: raise ValueError('smidx', sub)
            i += 1
        elif w >> 12 == 0b1000:  # RelocIncrPosition
            addr += (w & 0xfff) + 1; i += 1
        elif w >> 12 == 0b1001:  # RelocSmRepeat
            blocks = ((w >> 8) & 0xf) + 1; rc = (w & 0xff) + 1
            # repeat previous `blocks` instructions rc times
            seq = words[i - blocks:i]
            j = 0
            # re-execute by inlining
            words[i:i + 1] = seq * rc
            continue
        elif w >> 10 == 0b101000:  # RelocSetPosition
            addr = ((w & 0x3ff) << 16) | words[i + 1]; i += 2
        elif w >> 10 == 0b101001:  # RelocLgByImport
            addr += 4; symi = (((w & 0x3ff) << 16) | words[i + 1]) + 1; i += 2
        elif w >> 10 == 0b101100:  # RelocLgRepeat
            blocks = ((w >> 6) & 0xf) + 1; rc = ((w & 0x3f) << 16) | words[i + 1]
            seq = words[i - blocks:i]
            words[i:i + 2] = seq * rc
            continue
        elif w >> 10 == 0b101101:  # RelocLgSetOrBySection
            i += 2
        else:
            raise ValueError(hex(w))


def mac_memory(name='Payback (OS X)', code=0x10000000, data=None):
    path = os.path.join(ROOT, 'extracted', 'mac', 'Payback', name)
    d = open(path, 'rb').read()
    secs = [struct.unpack('>iIIIIIBBBB', d[40 + i * 28:40 + (i + 1) * 28]) for i in range(2)]
    if data is None:
        data = code + secs[0][2]
    mem = load(path, [code, data])
    return {code: bytes(mem[0]), data: bytes(mem[1])}


if __name__ == '__main__':
    m = mac_memory()
    for b, v in m.items():
        print(hex(b), hex(len(v)))
    data = [v for b, v in m.items() if b != 0x10000000][0]
    db = [b for b in m if b != 0x10000000][0]
    i = data.find(b'Unable to malloc %d bytes')
    print('string at', hex(db + i))
    print('TOC 1017cefc ->', hex(struct.unpack('>I', data[0x1017cefc - db:0x1017cefc - db + 4])[0]))
