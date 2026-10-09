"""Parse the Ghidra exports (decompiled.c / functions.tsv / strings.tsv) of each build
into per-function feature records, cached as pickle."""
import re, os, pickle, collections

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
ROOT = os.path.join(REPO, 'analysis')  # the Ghidra exports (tools/ghidra/decompile_all.sh)
# Pickled intermediate results (git-ignored scratch; delete to recompute).
CACHE = os.path.join(REPO, 'scratch', 'symbols_cache')
os.makedirs(CACHE, exist_ok=True)
BUILDS = {'mac': 'ghidra_macx', 'os4': 'ghidra_os4', 'm68k': 'ghidra_68k', 'mac9': 'ghidra_mac9'}

HDR = re.compile(r'^// ==== (\S+) @ ([0-9a-fA-F]+) size=(\d+)\n', re.M)
STRLIT = re.compile(r'"((?:[^"\\]|\\.)*)"')
SLABEL = re.compile(r'\b(?:PTR_)?s_\w*?_([0-9a-f]{8})\b')
DATREF = re.compile(r'\b(?:PTR_)?(?:DAT|PTR|LAB)_([0-9a-f]{8})\b')
NUM = re.compile(r'(?<![\w.])(0x[0-9a-fA-F]+|\d+\.\d+(?:e[-+]?\d+)?|\d+)(?![\w.])')
CALL = re.compile(r'([A-Za-z_.$][\w.$:<>,~]*)\s*\(')
IFACE = re.compile(r'\(\*\*?\(code \*\*?\)\(\*?\(?(?:int \*)?\)?\s*(DAT_[0-9a-f]+|[a-z]Var\d+|param_\d+)\s*\+\s*(0x[0-9a-f]+|\d+)\)')


def norm_str(s):
    s = s.replace('\\n', '\n').replace('\\t', '\t').replace('\\"', '"').replace('\\\\', '\\')
    return s.strip()


def load_strings(d):
    addr2s = {}
    refs = collections.defaultdict(set)  # funcname -> strings
    for line in open(os.path.join(d, 'strings.tsv'), encoding='utf-8', errors='replace'):
        line = line.rstrip('\n')
        p = line.split('\t')
        if len(p) < 3:
            continue
        a, xr, s = p[0], p[1], '\t'.join(p[2:])
        addr2s[a.lower()] = s
        for f in xr.split(','):
            f = f.strip()
            if f:
                refs[f].add(s.strip())
    return addr2s, refs


def callname(tok, build):
    if build in ('mac', 'mac9'):
        tok = tok.split('::')[-1]
    return tok


def parse(build):
    cache = os.path.join(CACHE, f'cache_{build}.pkl')
    if os.path.exists(cache):
        return pickle.load(open(cache, 'rb'))
    d = os.path.join(ROOT, BUILDS[build])
    addr2s, srefs = load_strings(d)
    src = open(os.path.join(d, 'decompiled.c'), encoding='utf-8', errors='replace').read()
    hs = list(HDR.finditer(src))
    funcs = {}
    for i, m in enumerate(hs):
        name, addr, size = m.group(1), m.group(2).lower(), int(m.group(3))
        body = src[m.end(): hs[i + 1].start() if i + 1 < len(hs) else len(src)]
        funcs[addr] = dict(name=name, addr=addr, size=size, body=body)
    # name -> addr; mac call tokens look like _X for header .X
    byname = {}
    for a, f in funcs.items():
        n = f['name']
        byname[n] = a
        if n.startswith('.'):
            byname['_' + n[1:]] = a
            byname[n[1:]] = a
        if n.startswith('FUN_'):
            byname[n] = a
    for a, f in funcs.items():
        body = f['body']
        # signature line(s): text up to first '{'
        sig = body.split('{', 1)[0]
        f['nparams'] = len(re.findall(r'param_\d+', sig))
        b = body.split('{', 1)[1] if '{' in body else body
        strs = set(norm_str(x) for x in STRLIT.findall(b))
        for sa in SLABEL.findall(b):
            if sa in addr2s:
                strs.add(addr2s[sa].strip())
        strs |= set(x.strip() for x in srefs.get(f['name'], ()))
        f['strings'] = {s for s in strs if len(s) >= 3}
        calls = []
        ext = []
        for cm in CALL.finditer(b):
            tok = callname(cm.group(1), build)
            if tok in ('if', 'while', 'for', 'switch', 'return', 'sizeof') or tok.startswith('('):
                continue
            if tok in byname:
                calls.append(byname[tok])
            elif re.match(r'^[A-Za-z_][\w]*$', tok) and not re.match(r'^(FUN|DAT|SUB|LAB|CONCAT\d*|SUB\d*|ZEXT\d*|SEXT\d*|CARRY\d*|SBORROW\d*|POPCOUNT|trunc|abs|code)', tok):
                ext.append(tok)
        f['calls'] = [c for c in calls if c != a]
        f['ext'] = ext
        f['iface'] = [m.group(2) for m in IFACE.finditer(b)]
        nums = []
        for x in NUM.findall(b):
            try:
                v = float(x) if '.' in x else int(x, 0)
            except ValueError:
                continue
            nums.append(v)
        f['nums'] = nums
        f['lines'] = b.count('\n')
        f['ifs'] = len(re.findall(r'\bif \(', b))
        f['loops'] = len(re.findall(r'\b(while|for) \(', b)) + b.count('do {')
        f['switch'] = b.count('switch(') + b.count('switch (')
    # function pointer refs (non-call uses)
    for a, f in funcs.items():
        b = f['body'].split('{', 1)[-1]
        fr = []
        for m in re.finditer(r'\bFUN_([0-9a-f]{8})\b(?!\s*\()', b):
            if m.group(1) in funcs and m.group(1) != a:
                fr.append(m.group(1))
        f['frefs'] = fr
    if build in ('mac', 'mac9'):
        import struct, pef
        mem = pef.mac_memory('Payback (OS X)' if build == 'mac' else 'Payback (OS 9)')
        (cb, code), (db, data) = sorted(mem.items())
        def rd(x):
            if db <= x < db + len(data) - 3:
                return struct.unpack('>I', data[x - db:x - db + 4])[0]
            return None

        def cstr(x):
            if not (db <= x < db + len(data)):
                return None
            o = x - db
            e = data.find(bytes(1), o, o + 400)
            if e < 0 or e - o < 3:
                return None
            raw = data[o:e]
            if all(32 <= c < 127 or c in (9, 10, 13) for c in raw):
                return raw.decode('latin-1').strip()
            return None
        for a, f in funcs.items():
            b = f['body'].split('{', 1)[-1]
            for m in re.finditer(r'0x(10[0-9a-f]{6})\b', b):
                cs = addr2s.get(m.group(1)) or cstr(int(m.group(1), 16))
                if cs:
                    f['strings'].add(cs.strip())
            for m in re.finditer(r'\b_?(?:PTR_)?DAT_([0-9a-f]{8})\b(?:\s*\+\s*(0x[0-9a-f]+|\d+)(?![\d*]))?', b):
                t = rd(int(m.group(1), 16))
                if t is None:
                    continue
                if m.group(2):
                    cs = cstr(t + int(m.group(2), 0))
                    if cs:
                        f['strings'].add(cs)
                    continue
                ts = '%08x' % t
                if ts in addr2s:
                    f['strings'].add(addr2s[ts].strip())
                    continue
                cs = cstr(t)
                if cs:
                    f['strings'].add(cs)
                    continue
                t2 = rd(t)
                if t2 is not None and '%08x' % t2 in funcs:
                    f['frefs'].append('%08x' % t2)
                # PTR_s_ labels: the pointer itself already names a string
            # var = _DAT_x;  ... var + 0xNN  (string pool base kept in a register)
            for m in re.finditer(r'\b(\w+) = \(?\w*\s*\**\)?_?DAT_([0-9a-f]{8});', b):
                t = rd(int(m.group(2), 16))
                if t is None or not (db <= t < db + len(data)):
                    continue
                var = re.escape(m.group(1))
                for m2 in re.finditer(r'\b' + var + r'\s*\+\s*(0x[0-9a-f]+|\d+)(?![\d*])', b[m.end():]):
                    cs = cstr(t + int(m2.group(1), 0))
                    if cs:
                        f['strings'].add(cs)
            for m in re.finditer(r'\bPTR_s_\w*?_([0-9a-f]{8})\b', b):
                t = rd(int(m.group(1), 16))
                if t is not None and '%08x' % t in addr2s:
                    f['strings'].add(addr2s['%08x' % t].strip())
            f['strings'] = {s for s in f['strings'] if len(s) >= 3}
    # global data references
    for a, f in funcs.items():
        b = f['body'].split('{', 1)[-1]
        gs = []
        if build in ('mac', 'mac9'):
            for m in re.finditer(r'\b_?(?:PTR_)?DAT_([0-9a-f]{8})\b', b):
                x = int(m.group(1), 16)
                if 0x1017b440 <= x < 0x1017e000 or build == 'mac9':
                    t = rd(x)
                    if t is not None and db <= t < db + len(data) and '%08x' % t not in addr2s:
                        gs.append('%08x' % t)
        else:
            for m in re.finditer(r'\bDAT_([0-9a-f]{8})\b', b):
                if m.group(1) not in addr2s:
                    gs.append(m.group(1))
        f['globs'] = gs
    callers = collections.defaultdict(list)
    for a, f in funcs.items():
        for c in f['calls']:
            callers[c].append(a)
    for a, f in funcs.items():
        f['callers'] = callers.get(a, [])
    data = dict(funcs=funcs, byname=byname)
    pickle.dump(data, open(cache, 'wb'))
    return data


if __name__ == '__main__':
    for b in ('mac', 'os4', 'm68k'):
        d = parse(b)
        fs = d['funcs']
        print(b, len(fs), 'with strings', sum(1 for f in fs.values() if f['strings']),
              'calls', sum(len(f['calls']) for f in fs.values()),
              'ext', collections.Counter(e for f in fs.values() for e in f['ext']).most_common(15))
