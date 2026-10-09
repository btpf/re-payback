"""Step 2 of 3: write docs/xref/symbols.tsv and collect the data for symbols.md."""
import pickle, re, glob, os, collections, json
import load, match
from demangle import demangle

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
OUT = os.path.join(ROOT, 'docs', 'xref')

A = load.parse('mac')['funcs']; B = load.parse('os4')['funcs']; C = load.parse('m68k')['funcs']
F = pickle.load(open(os.path.join(load.CACHE, 'final.pkl'), 'rb'))
mo, eo = F['mo']; ok, ek = F['ok']; mk, emk = F['mk']

# manual additions (read from the decompiles / existing notes)
MANUAL = {
    '1014a670': ('0105f594', '000e3a58', 'medium',
                 'manual: OS4 0105f594 / 68k 000e3a58 are the per-object update with do_Person, do_Other '
                 'and do_Weapon inlined (docs/re/people.md); the Mac keeps them as separate functions'),
}
ROUTE = 'manual: route-table API, roles from docs/re/combat_police.md (OS4 and 68k equivalents listed there)'
MANUAL.update({
    '100f1020': ('010cd424', '000a4000', 'medium', ROUTE + '; per-node closest point'),
    '100f11d0': ('010cd590', '000a42ac', 'high', ROUTE + '; nearest node'),
    '100f1300': ('010cd684', '000a430a', 'high', ROUTE + '; farthest node'),
    '100f13b0': ('010cd72c', '000a438c', 'high', ROUTE + '; node x'),
    '100f1400': ('010cd744', '000a43ae', 'high', ROUTE + '; node y'),
    '100f1450': ('', '000a43d0', 'low', ROUTE + '; third per-node getter (OS4 has none; docs call it a flag byte)'),
    '100f14b0': ('010cd774', '000a43f2', 'medium', ROUTE + '; next hop'),
    '100f1560': ('010cd838', '000a4200', 'high', 'strings %s.route in all three builds'),
    '10052be0': ('0104199c', '000f043c', 'medium', 'manual: config loader (S:Payback.config, docs/re/frontend_menus.md 3.7), '
                 'next to SaveConfig in all builds; the Mac body reads Mac preferences instead'),
    '100f24f0': ('01006564', '00015d46', 'medium', 'manual: set FX volume (docs/re/frontend_audio.md), next to GetSoundVolume in both builds'),
    '100f3f40': ('0100812c', '', 'medium', 'manual: get slot sample id (docs/re/frontend_audio.md), Sound_GetSample/Sound_SetSample precede AddSound in both builds'),
    '100f3f80': ('01008144', '', 'medium', 'manual: set slot sample id (docs/re/frontend_audio.md), engine -> clonk swap'),
    '100a7750': ('0103517c', '', 'medium', 'manual: follows LoadEffect in both builds; OS4 strings '
                 '"PROGDIR:effecttextures/%s", "Unable to load effect %s..."'),
})
# OS4 <-> 68k pairs stated in docs/re (only applied where the matcher left both free)
DOC_OK = {'0103451c': '0003da7e', '01042084': '000efede', '01005864': '00012038',
          '010cbc7c': '000a1758'}  # PositionUnit: same slot between MoveUnit and SkidUnit in all three builds
for b, c in DOC_OK.items():
    if b not in ok and c not in set(ok.values()):
        ok[b] = c; ek[b] = ('medium', 'docs/re pairing')
for a, (b, c, conf, ev) in MANUAL.items():
    # drop conflicting automatic matches
    if a in mo:
        del mo[a]
    for x in [x for x, y in mo.items() if b and y == b]:
        del mo[x]
    if a in mk and (not c or mk[a] != c):
        del mk[a]
    if b:
        if c:
            for y in [y for y, z in ok.items() if z == c and y != b]:
                del ok[y]
            ok[b] = c; ek[b] = ('medium', 'manual')
        mo[a] = b; eo[a] = (conf, ev)
        for x in [x for x, y in mk.items() if y == c and x != a]:
            del mk[x]
    else:
        mk[a] = c; emk[a] = (conf, ev)

# complete triangles: Mac->68k direct + OS4<->68k gives a Mac->OS4 link
_r_ok = {c: b for b, c in ok.items()}
_r_mo = {b: a for a, b in mo.items()}
for a, c in list(mk.items()):
    if a in mo or not (0.2 <= C[c]['size'] / (max(A[a]['size'], 8) * 0.68) <= 5):
        continue
    b = _r_ok.get(c)
    if b and b not in _r_mo and 0.33 <= B[b]['size'] / max(A[a]['size'], 1) <= 3:
        mo[a] = b
        eo[a] = ('medium' if emk[a][0] == 'high' and ek[b][0] == 'high' else 'low',
                 f'via-68k: mac-68k ({emk[a][1]}) + os4-68k ({ek[b][1]})')
        _r_mo[b] = a

r_mo = {b: a for a, b in mo.items()}
r_ok = {c: b for b, c in ok.items()}
r_mk = {c: a for a, c in mk.items()}


def clean(ev):
    ev = ev.replace('triangle-anchor:', '')
    return ev


def sizeinfo(x, D):
    return D[x]['size'] if x in D else ''


# doc citations
cites = collections.defaultdict(set)
DOCS = os.path.join(ROOT, 'docs')
for f in sorted(glob.glob(os.path.join(DOCS, '**', '*.md'), recursive=True)):
    rel = os.path.relpath(f, DOCS).replace(os.sep, '/')
    if rel.startswith('xref/'):
        continue
    t = open(f, encoding='utf-8').read()
    # docs/re/<x>.md -> "<x>", docs/re/mac/<x>.md -> "mac/<x>", docs/<x>.md -> "<x>"
    key = rel[3:] if rel.startswith('re/') else rel
    for m in re.finditer(r'FUN_([0-9a-fA-F]{8})', t):
        cites[m.group(1).lower()].add(key)


def mac_category(a, f):
    n = f['name']; x = int(a, 16)
    if not n.startswith('.') and not n.startswith('FUN_'):
        return 'import'
    if n.startswith('FUN_') and 0x1015d000 <= x < 0x1016f598:
        return 'msl-runtime'
    if n.startswith('FUN_') and x >= 0x1015d000:
        return 'import'
    if x < 0x10051910:
        return 'powerplant'
    if re.match(r'^\.\w+__F', n):
        if n.startswith('.amiga_'):
            return 'mac-glue'
        return 'game'
    if re.match(r'^\.(__ct|__dt)?\w*__\d+[A-Za-z]', n) or '__Q' in n:
        return 'mac-class'
    if n.startswith('.'):
        return 'amiga-shim'
    return 'other'


rows = []
for a in sorted(A):
    f = A[a]
    cat = mac_category(a, f)
    b = mo.get(a, '')
    c = ''
    conf = ''
    evs = []
    if b:
        conf = eo[a][0]
        evs.append('os4:' + clean(eo[a][1]))
        tri = ok.get(b); d = mk.get(a)
        if eo[a][1].startswith(('manual', 'via-68k')):
            c = tri or ''
        elif tri and d and tri == d:
            c = tri; evs.append('68k:triangle-agrees')
            if conf == 'low':
                conf = 'medium'
            elif conf == 'medium' and 'swap' not in eo[a][1] and f['size'] > 24:
                conf = 'high'
        elif tri and d:
            c = tri; evs.append(f'68k:via-os4({clean(ek[b][1])}) disagrees with direct {d}')
            conf = 'medium' if conf == 'high' else 'low'
        elif tri:
            c = tri; evs.append(f'68k:via-os4({ek[b][0]},{clean(ek[b][1])})')
            if ek[b][0] == 'low' and conf == 'high':
                conf = 'medium'
        elif d:
            c = d; evs.append(f'68k:direct({emk[a][0]},{clean(emk[a][1])})')
    elif a in mk:
        c = mk[a]; conf = emk[a][0]
        evs.append(f'68k:direct({clean(emk[a][1])}); no OS4 match')
        if conf == 'high':
            conf = 'medium'
    else:
        evs.append(f'mac-only:{cat}')
    if cat == 'msl-runtime' and conf in ('high', 'medium'):
        conf = 'low'
        evs.append('MSL vs clib2 runtime: shape-only')
    # tiny bodies matched only by position: cap at medium
    if conf == 'high' and f['size'] <= 24 and 'strings' not in ' '.join(evs) and 'triangle-agrees' not in ' '.join(evs):
        conf = 'medium'
    rows.append(dict(mac_addr=a, mac_name=f['name'].lstrip('.'), demangled=demangle(f['name']),
                     os4_addr=b, m68k_addr=c, confidence=conf, evidence='; '.join(evs), cat=cat,
                     size=f['size']))

# size sanity: wildly different sizes without strings/manual evidence
for r in rows:
    e = r['evidence']
    if 'manual' in e or 'strings' in e or 'triangle-agrees' in e or 'sole-' in e:
        continue
    ms = max(r['size'], 8)
    if r['os4_addr']:
        q = B[r['os4_addr']]['size'] / ms
        if not (0.2 <= q <= 5) and r['confidence'] != 'low' and B[r['os4_addr']]['size'] > 8:
            r['confidence'] = 'low'; r['evidence'] += f'; size ratio os4/mac {q:.1f}'
    if r['m68k_addr'] and not r['os4_addr']:
        q = C[r['m68k_addr']]['size'] / (ms * 0.68)
        if not (0.2 <= q <= 5):
            r['evidence'] += f'; 68k {r["m68k_addr"]} dropped (size ratio {q:.1f})'
            r['m68k_addr'] = ''; r['confidence'] = ''

# one Mac function per 68k address: keep the best-supported row
RANK = {'high': 3, 'medium': 2, 'low': 1, '': 0}
by68 = collections.defaultdict(list)
for r in rows:
    if r['m68k_addr']:
        by68[r['m68k_addr']].append(r)
for c, rs in by68.items():
    if len(rs) < 2:
        continue
    def key(r):
        e = r['evidence']
        return ('manual' in e, 'triangle-agrees' in e, 'via-os4' in e, RANK[r['confidence']])
    rs.sort(key=key, reverse=True)
    for r in rs[1:]:
        r['evidence'] += f'; 68k {c} dropped (also claimed by {rs[0]["mac_name"]})'
        r['m68k_addr'] = ''
        if not r['os4_addr']:
            r['confidence'] = ''

# Amiga-only rows (OS4 game functions with no Mac counterpart)
amiga_only = []
for b in sorted(B):
    if b in r_mo or match.os4_region(b) == 'libc':
        continue
    c = ok.get(b, '')
    used68 = {r['m68k_addr'] for r in rows if r['m68k_addr']}
    if c in used68:
        c = ''
    ev = 'amiga-only: no Mac match' + (f'; 68k via os4-68k ({ek[b][0]}, {clean(ek[b][1])})' if c else '')
    amiga_only.append(dict(mac_addr='', mac_name='', demangled='', os4_addr=b, m68k_addr=c,
                           confidence=ek[b][0] if c else '', evidence=ev, cat='amiga-only', size=B[b]['size']))

cols = ['mac_addr', 'mac_name', 'demangled', 'os4_addr', 'm68k_addr', 'confidence', 'evidence']
with open(os.path.join(OUT, 'symbols.tsv'), 'w', encoding='utf-8', newline='\n') as fo:
    fo.write('\t'.join(cols) + '\n')
    for r in rows + amiga_only:
        fo.write('\t'.join(str(r[k]).replace('\t', ' ') for k in cols) + '\n')

pickle.dump(dict(rows=rows, amiga_only=amiga_only, cites={k: sorted(v) for k, v in cites.items()},
                 mo=mo, ok=ok, mk=mk), open(os.path.join(load.CACHE, 'outputs.pkl'), 'wb'))

# summary numbers
cnt = collections.Counter(r['cat'] for r in rows)
matched = [r for r in rows if r['os4_addr'] or r['m68k_addr']]
print('mac functions', len(rows), dict(cnt))
print('matched to any amiga', len(matched), 'os4', sum(1 for r in rows if r['os4_addr']),
      '68k', sum(1 for r in rows if r['m68k_addr']))
print('conf', collections.Counter(r['confidence'] for r in matched))
print('game matched', sum(1 for r in matched if r['cat'] == 'game'), 'of', cnt['game'])
print('amiga-only os4 rows', len(amiga_only))
