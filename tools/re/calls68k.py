#!/usr/bin/env python3
# usage: python tools/re/calls68k.py <68k|scriptNN|flat.bin> <start_hex> <end_hex>
# Annotated 68k disassembly of mission code: argument pushes are folded into
# "CALL Name(args)" lines (args in C order), other instructions printed as-is.
# Stack/register tracking is linear and approximate (unknown values shown as '?').
# Names come from m68k_api.py; the flat images from tools/ghidra/decompile_all.sh.
import sys, re, capstone
from common import flat_path
from m68k_api import CALLS as NAMES
def s32(v): v&=0xffffffff; return v-(1<<32) if v&0x80000000 else v
def fmt(v):
    if isinstance(v,int):
        v=s32(v)
        return str(v) if -10<v<10 else ('-0x%x'%-v if v<0 else '0x%x'%v)
    return v
f,s,e=flat_path(sys.argv[1]),int(sys.argv[2],16),int(sys.argv[3],16)
d=open(f,'rb').read()
md=capstone.Cs(capstone.CS_ARCH_M68K,capstone.CS_MODE_BIG_ENDIAN|capstone.CS_MODE_M68K_020)
ins=[]; a=s; lastk=None; tables={}
while a<e:
    got=list(md.disasm(d[a-0x10000:e-0x10000],a,count=1))
    if not got:
        a+=2; continue
    i=got[0]; ins.append(i); a+=i.size
    m=re.match(r'#\$([0-9a-f]+), d\d',i.op_str)
    if i.mnemonic=='moveq' and m: lastk=int(m.group(1),16)
    m2=re.match(r'\$([0-9a-f]+)\(pc, d\d\.w\)',i.op_str)
    if i.mnemonic=='jmp' and m2 and lastk is not None:
        T=int(m2.group(1),16); n=lastk+1; ent=[]
        for k in range(n):
            w=int.from_bytes(d[T-0x10000+2*k:T-0x10000+2*k+2],'big')
            if w&0x8000: w-=0x10000
            ent.append(T+w)
        tables[i.address]=ent; a=T+2*n

targets=set()
for ent in tables.values(): targets.update(ent)
for i in ins:
    if i.mnemonic.startswith(('b','db','j')) and '$' in i.op_str:
        m=re.search(r'\$([0-9a-f]+)$',i.op_str)
        if m: targets.add(int(m.group(1),16))
stack=[]; regs={'a3':'ev'}; npush=0
def val(op,tag=False):
    op=op.strip()
    m=re.fullmatch(r'#\$([0-9a-f]+)',op)
    if m: return int(m.group(1),16)
    m=re.fullmatch(r'#(-?\d+)',op)
    if m: return int(m.group(1))
    if re.fullmatch(r'[ad]\d',op):
        v=regs.get(op,None)
        return (('%s{%s}'%(fmt(v),op)) if tag else v) if v is not None else '?'+op
    m=re.fullmatch(r'\$([0-9a-f]+)\.l',op)
    if m: return '['+m.group(1)+']'
    return '?('+op+')'
for i in ins:
    if i.address in targets:
        print('%06x:'%i.address)
        pass
    mn,op=i.mnemonic,i.op_str
    ops=[x.strip() for x in re.split(r',(?![^(]*\))',op)]
    if mn=='pea.l':
        m=re.fullmatch(r'\$([0-9a-f]+)\.(w|l)',op)
        if m:
            v=int(m.group(1),16)
            if m.group(2)=='w' and v&0x8000: v-=0x10000
            stack.append(v)
        else: stack.append('?('+op+')')
        npush+=1
        continue
    if mn in('move.l','move.w') and len(ops)==2 and ops[1] in('-(a7)','(a7)'):
        v=val(ops[0],True)
        if ops[1]=='-(a7)': stack.append(v); npush+=1
        else:
            if stack: stack[-1]=v
            else: stack.append(v)
            npush=max(npush,1)
        continue
    if mn=='clr.l' and op in('-(a7)','(a7)'):
        if op=='-(a7)': stack.append(0); npush+=1
        elif stack: stack[-1]=0
        else: stack.append(0)
        if op=='(a7)': npush=max(npush,1)
        continue
    if mn in('lea.l',) and ops[1]=='a7':
        m=re.fullmatch(r'\$([0-9a-f]+)\(a7\)',ops[0])
        if m:
            n=int(m.group(1),16)//4
            del stack[max(0,len(stack)-n):]
            continue
    if mn in('addq.l','addq.w','adda.w','adda.l') and ops[1]=='a7':
        n=int(ops[0][2:],16)//4 if ops[0].startswith('#$') else int(ops[0][1:])//4
        del stack[max(0,len(stack)-n):]
        continue
    if mn=='moveq' or (mn in('move.l','movea.l','lea.l') and len(ops)==2 and re.fullmatch(r'[ad]\d',ops[1])):
        src=ops[0]
        if mn=='lea.l':
            m=re.fullmatch(r'\$([0-9a-f]+)(\.l|\(pc\))',src); regs[ops[1]]=int(m.group(1),16) if m else '?'
        else: regs[ops[1]]=val(src)
        print('%06x:   %s %s'%(i.address,mn,op)); continue
    if mn=='jmp' and i.address in tables:
        print('%06x:   SWITCH %s'%(i.address,op))
        ent=tables[i.address]
        for k,t in enumerate(ent): print('           case idx %d -> %06x'%(k,t))
        continue
    if mn in('jsr','bsr.w','bsr.b','bsr.l'):
        m=re.fullmatch(r'\$([0-9a-f]+)(\.l)?',op)
        tgt=int(m.group(1),16) if m else regs.get(op.strip('()'),None)
        name,n=NAMES.get(tgt,('FUN_%x'%tgt if isinstance(tgt,int) else 'FUN_?',3)) if tgt is not None else ('FUN_?',3)
        if n is None: n=3
        args=[fmt(x) for x in reversed(stack[-n:])] if n else []
        if len(stack)<n: args+=['?']*(n-len(stack))
        print('%06x: CALL %s(%s)'%(i.address,name,', '.join(args)))
        for r in ('d0','d1','a0','a1'): regs.pop(r,None)
        npush=0
        continue
    extra=''
    if mn.startswith(('bra','jmp')) and npush>0:
        extra='   [PENDING ARGS (C order): %s]'%', '.join(fmt(x) for x in reversed(stack[-npush:]))
    print('%06x:   %s %s%s'%(i.address,mn,op,extra))
