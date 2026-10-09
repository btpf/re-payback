#!/usr/bin/env python3
# Linear 68k disassembly of a flat image, decoding pc-relative word jump tables.
# usage: python tools/re/dis68k.py <68k|scriptNN|flat.bin> <start_hex> <end_hex>
# The flat images (base 0x10000) come from tools/ghidra/decompile_all.sh.
import sys, re, capstone
from common import flat_path
d=open(flat_path(sys.argv[1]),'rb').read()
s,e=int(sys.argv[2],16),int(sys.argv[3],16)
md=capstone.Cs(capstone.CS_ARCH_M68K, capstone.CS_MODE_BIG_ENDIAN|capstone.CS_MODE_M68K_020)
w=lambda a: int.from_bytes(d[a-0x10000:a-0x10000+2],'big',signed=True)
a=s; hist=[]
while a<e:
    ins=next(md.disasm(d[a-0x10000:a-0x10000+12],a),None)
    if ins is None:
        print("%06x: %04x   .word"%(a,w(a)&0xffff)); a+=2; continue
    print("%06x: %-20s %s %s"%(ins.address,ins.bytes.hex(),ins.mnemonic,ins.op_str))
    hist.append(ins); a=ins.address+ins.size
    m=re.match(r'\$([0-9a-f]+)\(pc, d0\.w\)',ins.op_str)
    if ins.mnemonic=='jmp' and m:
        base=int(m.group(1),16); n=None
        cmpreg=None
        for h in reversed(hist[-6:]):
            mm=re.match(r'd0, (d\d)',h.op_str)
            if h.mnemonic=='cmp.l' and mm: cmpreg=mm.group(1)
            mm=re.match(r'#\$([0-9a-f]+), (d\d)',h.op_str)
            if h.mnemonic=='moveq' and mm and mm.group(2)==cmpreg: n=int(mm.group(1),16); break
        # find subtract bias
        bias=0
        for h in reversed(hist[-8:]):
            mm=re.match(r'#\$([0-9a-f]+), d0',h.op_str)
            if h.mnemonic in('subq.l','subi.l','sub.l') and mm: bias=int(mm.group(1),16);break
            if h.mnemonic=='addi.l' and mm: bias=(-int(mm.group(1),16))&0xffffffff;break
            if h.mnemonic=='moveq' and mm:
                v=int(mm.group(1),16); v= v-256 if v>127 else v; bias=-v; break
            mm2=re.match(r'#\$([0-9a-f]+), d0',h.op_str)
        for k in range(n+1):
            print("        case %d (ev %d / 0x%x) -> %06x"%(k,k+bias,k+bias,base+w(base+2*k)))
        a=base+2*(n+1)
