#!/usr/bin/env python3
# Recursive-descent 68k disassembler with stack-argument tracking for mission handlers.
# usage: python tools/re/trace68k.py <68k|scriptNN|flat.bin> <func_start_hex> [<func_end_hex>]
# Names come from m68k_api.py; the flat images from tools/ghidra/decompile_all.sh.
import sys, re, capstone
from common import flat_path
from m68k_api import NAMES, NARGS
f = flat_path(sys.argv[1]); start = int(sys.argv[2], 16); end = int(sys.argv[3], 16) if len(sys.argv) > 3 else None
d = open(f, 'rb').read()
md = capstone.Cs(capstone.CS_ARCH_M68K, capstone.CS_MODE_BIG_ENDIAN | capstone.CS_MODE_M68K_020)

def ins_at(a):
    for i in md.disasm(d[a-0x10000:a-0x10000+12], a):
        return i
    return None
def s16(v):
    v &= 0xffff; return v-0x10000 if v & 0x8000 else v
def s32(v):
    v &= 0xffffffff; return v-(1 << 32) if v & 0x80000000 else v
def parse_imm(s):
    t = s.strip()
    if not t.startswith('#'): return None
    t = t[1:]
    try:
        return s32(int(t[1:], 16)) if t.startswith('$') else int(t)
    except ValueError:
        return None
def fmt(v):
    if isinstance(v, int):
        return str(v) if -100 < v < 100 else ('-0x%x' % (-v) if v < 0 else '0x%x' % v)
    return str(v)
def split_ops(s):
    parts = []; depth = 0; cur = ''
    for ch in s:
        if ch == '(': depth += 1
        if ch == ')': depth -= 1
        if ch == ',' and depth == 0:
            parts.append(cur.strip()); cur = ''
        else:
            cur += ch
    parts.append(cur.strip()); return parts

out = {}; seen = {}; labels = set()
work = [(start, [], {'a3': 'EV'}, None)]
joins = {}
BR = ('ra','eq','ne','gt','ge','lt','le','hi','ls','cc','cs','pl','mi','vc','vs')
while work:
    a, stack, regs, src = work.pop()
    joins.setdefault(a, []).append((src, [fmt(x) for x in stack[-3:]]))
    while True:
        if a in seen: break
        if end and not (start <= a < end): break
        i = ins_at(a)
        if i is None:
            out[a] = '??'; break
        seen[a] = 1
        mn = i.mnemonic; ops = split_ops(i.op_str) if i.op_str else []
        txt = '%s %s' % (mn, i.op_str); note = ''
        nxt = a + i.size
        try:
            if mn == 'pea.l':
                m = re.fullmatch(r'\$([0-9a-f]+)\.w', ops[0]); m2 = re.fullmatch(r'\$([0-9a-f]+)\.l', ops[0])
                if m: stack.append(s16(int(m.group(1), 16)))
                elif m2: stack.append(s32(int(m2.group(1), 16)))
                else: stack.append('<%s>' % ops[0])
            elif mn == 'clr.l' and ops[0] == '-(a7)': stack.append(0)
            elif mn == 'clr.l' and ops[0] == '(a7)':
                if stack: stack[-1] = 0
            elif mn.startswith('move') and len(ops) == 2 and (ops[1] in ('-(a7)', '(a7)') or re.fullmatch(r'\$[0-9a-f]+\(a7\)', ops[1])):
                src = ops[0]; v = parse_imm(src)
                if v is None:
                    if src in regs: v = regs[src]
                    else:
                        m = re.fullmatch(r'\$([0-9a-f]+)\.l', src)
                        v = '[0x%s]' % m.group(1) if m else '<%s>' % src
                if mn.endswith('.w') and isinstance(v, int): v = s16(v)
                if ops[1] == '-(a7)': stack.append(v)
                elif ops[1] == '(a7)':
                    if stack: stack[-1] = v
                else:
                    n = int(re.fullmatch(r'\$([0-9a-f]+)\(a7\)', ops[1]).group(1), 16)//4
                    if len(stack) > n: stack[-1-n] = v
            elif mn == 'lea.l' and re.fullmatch(r'\$([0-9a-f]+)\.l', ops[0]) and ops[1] != 'a7':
                regs[ops[1]] = ('fn', int(ops[0][1:-2], 16))
            elif mn == 'lea.l' and ops[1] == 'a7':
                m = re.fullmatch(r'\$([0-9a-f]+)\(a7\)', ops[0])
                if m:
                    n = int(m.group(1), 16)//4
                    del stack[len(stack)-n:]
            elif mn in ('addq.l','addq.w','adda.w','adda.l','add.l','lea.l') and len(ops) == 2 and ops[1] == 'a7':
                n = parse_imm(ops[0])//4; del stack[len(stack)-n:]
            elif mn == 'moveq' or (mn.startswith('move') and len(ops) == 2 and re.fullmatch(r'[ad][0-7]', ops[1])):
                v = parse_imm(ops[0])
                if v is None:
                    m = re.fullmatch(r'\$([0-9a-f]+)\.l', ops[0])
                    v = ('[0x%s]' % m.group(1)) if m else (regs.get(ops[0]) if ops[0] in regs else None)
                if v is None: regs.pop(ops[1], None)
                else: regs[ops[1]] = v
            elif len(ops) >= 2 and re.fullmatch(r'[ad][0-7]', ops[-1]):
                regs.pop(ops[-1], None)
            if mn in ('jsr', 'bsr.l', 'bsr.w', 'bsr.b', 'bsr.s', 'bsr'):
                m = re.search(r'\$([0-9a-f]+)', ops[0]); t = int(m.group(1), 16) if m else None
                mm = re.fullmatch(r'\((a[0-6])\)', ops[0])
                if mm and isinstance(regs.get(mm.group(1)), tuple): t = regs[mm.group(1)][1]
                nm = NAMES.get(t, 'FUN_%x' % t if t else ops[0])
                na = NARGS.get(nm, 4)
                args = [stack[-1-k] if len(stack) > k else '?' for k in range(na)]
                note = 'CALL %s(%s)' % (nm, ', '.join(fmt(x) for x in args))
                for r in ('d0', 'd1', 'a0', 'a1'): regs.pop(r, None)
        except Exception as e:
            note = 'ERR %s' % e
        out[a] = '%-44s %s' % (txt, ('; ' + note) if note else '')
        if mn in ('rts', 'rte'): break
        if mn.startswith('jmp'):
            m = re.fullmatch(r'\$([0-9a-f]+)\(pc, d0\.w\)', ops[0])
            if m:
                tb = int(m.group(1), 16)
                N = None; base = 0
                # walk back linearly over the last few instructions (from a known-good start)
                seq = []
                b = a - 40
                while b < a:
                    j = ins_at(b)
                    if j is None: b += 2; continue
                    if j.address + j.size > a: b += 2; continue
                    seq.append(j); b += j.size
                # keep only a chain that ends exactly at the move.w preceding jmp
                for j in seq[-8:]:
                    o = j.op_str.split(',')
                    if j.mnemonic == 'moveq' and len(o) == 2 and o[1].strip() != 'd0':
                        v = parse_imm(o[0]); N = v
                    if j.mnemonic == 'moveq' and len(o) == 2 and o[1].strip() == 'd0':
                        v = parse_imm(o[0]); v = v - 256 if v >= 128 else v; base = -v
                    if j.mnemonic in ('subq.l', 'subi.l', 'sub.l') and j.op_str.endswith('d0'):
                        base = parse_imm(o[0]) or 0
                    if j.mnemonic in ('addi.l', 'addq.l') and j.op_str.endswith('d0'):
                        base = -(parse_imm(o[0]) or 0)
                if N is None: N = 0
                tbl = []
                for k in range(N+1):
                    off = s16(int.from_bytes(d[tb-0x10000+2*k:tb-0x10000+2*k+2], 'big'))
                    tgt = tb + off; tbl.append(tgt); labels.add(tgt)
                    work.append((tgt, list(stack), dict(regs), a))
                out[a] += ' ; JUMPTABLE base=%d: ' % base + ', '.join('%d->%x' % (k+base, t) for k, t in enumerate(tbl))
            break
        if mn.startswith('b') and mn[1:3] in BR and not mn.startswith('bsr'):
            m = re.search(r'\$([0-9a-f]+)', ops[0]); t = int(m.group(1), 16); labels.add(t)
            work.append((t, list(stack), dict(regs), a))
            if mn.startswith('bra'): break
        if mn.startswith('db'):
            m = re.search(r'\$([0-9a-f]+)', ops[-1]); t = int(m.group(1), 16); labels.add(t); work.append((t, list(stack), dict(regs), a))
        a = nxt
prev = None
for a in sorted(out):
    if prev is not None and a != prev: print('        ...')
    if a in labels:
        print('L_%x:' % a)
        js = [j for j in joins.get(a, []) if j[0] is not None]
        if js: print('      ; from ' + '; '.join('%x top=%s' % (j[0], j[1][::-1]) for j in js))
    print('  %06x: %s' % (a, out[a]))
    i = ins_at(a); prev = a + (i.size if i else 2)
