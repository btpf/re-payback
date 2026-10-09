"""Run the three pairwise matchers, triangulate, re-run seeded with agreed triples.

Step 1 of 3: python tools/re/symbols/final_match.py; then build_outputs.py; then write_md.py.
"""
import match, pickle, collections, os
import load
FINAL = os.path.join(load.CACHE, 'final.pkl')

MANUAL_MAC_OS4 = {
    # mission dispatchers read from InitScript__Fii (1013e050) vs OS4 010f42f4 and
    # ProcessTrigger__Fi (1013e2c0) vs OS4 010f45cc: city var x level var switch
    '101291c0': ('010eabc0', 'switch-dispatch: InitScript city0/level0'),
    '10129390': ('010ead34', 'switch-dispatch: InitScript city0/level1'),
    '10129650': ('010eaf98', 'switch-dispatch: InitScript city1/level0'),
    '10129880': ('010eb188', 'switch-dispatch: InitScript city1/level1'),
    '10129b00': ('010eb3d0', 'switch-dispatch: InitScript city2/level0'),
    '10129d20': ('010ebbe8', 'switch-dispatch: InitScript city2/level1'),
    '1012bae0': ('010d2b24', 'switch-dispatch: ProcessTrigger city0/level0'),
    '101315d0': ('010ebddc', 'switch-dispatch: ProcessTrigger city0/level1'),
    '10134800': ('010eef18', 'switch-dispatch: ProcessTrigger city1/level0'),
    '10137570': ('010f16ec', 'switch-dispatch: ProcessTrigger city1/level1'),
    '1013ab30': ('010de0b4', 'switch-dispatch: ProcessTrigger city2/level0 (OS4 splits it: also 010da308, 010d9ff4)'),
    '1013b2b0': ('010df934', 'switch-dispatch: ProcessTrigger city2/level1'),
    '1013e050': ('010f42f4', 'switch-dispatch parent: InitScript'),
    '1013e2c0': ('010f45cc', 'switch-dispatch parent: ProcessTrigger'),
}


def make(pair, seeds=None, manual=None):
    if pair == 'mo':
        M = match.Matcher('mac', 'os4', allowA=match.mac_allowed, regA=match.mac_region, regB=match.os4_region)
    elif pair == 'ok':
        M = match.Matcher('os4', 'm68k', regA=match.os4_region, regB=match.m68k_region, size_scale=1 / 0.68)
    else:
        M = match.Matcher('mac', 'm68k', allowA=match.mac_allowed, regA=match.mac_region, regB=match.m68k_region, size_scale=1 / 0.68)
    for a, (b, ev) in (manual or {}).items():
        M.add(a, b, 'high', 'manual:' + ev)
    for a, (b, ev) in (seeds or {}).items():
        M.add(a, b, 'high', ev)
    return M


def triangle(mo, ok, mk):
    agree = {}
    for a, b in mo.m.items():
        c = ok.m.get(b)
        if c and mk.m.get(a) == c:
            agree[a] = (b, c)
    return agree


def run():
    q = lambda *x: None
    mo = make('mo', manual=MANUAL_MAC_OS4); match.full(mo, log=q)
    ok = make('ok'); match.full(ok, log=q)
    mk = make('mk'); match.full(mk, log=q)
    for rnd in range(3):
        T = triangle(mo, ok, mk)
        print('round', rnd, 'triangle', len(T), 'mo', len(mo.m), 'ok', len(ok.m), 'mk', len(mk.m))
        s_mo = {a: (b, 'triangle-anchor:' + mo.ev[a][1].replace('triangle-anchor:', '')) for a, (b, c) in T.items() if a not in MANUAL_MAC_OS4}
        s_ok = {b: (c, 'triangle-anchor:' + ok.ev[b][1].replace('triangle-anchor:', '')) for a, (b, c) in T.items()}
        s_mk = {a: (c, 'triangle-anchor:' + mk.ev[a][1].replace('triangle-anchor:', '')) for a, (b, c) in T.items()}
        # carry over the manual anchors' 68k side if the os4-68k matcher knows it
        mo = make('mo', s_mo, MANUAL_MAC_OS4); match.full(mo, log=q)
        ok = make('ok', s_ok); match.full(ok, log=q)
        mk = make('mk', s_mk); match.full(mk, log=q)
    T = triangle(mo, ok, mk)
    print('final triangle', len(T), 'mo', len(mo.m), 'ok', len(ok.m), 'mk', len(mk.m))
    pickle.dump({'mo': (mo.m, mo.ev), 'ok': (ok.m, ok.ev), 'mk': (mk.m, mk.ev), 'T': T,
                 'gmap_mo': mo.gmap}, open(FINAL, 'wb'))


if __name__ == '__main__':
    run()


def resolve_disagreements(F):
    mo, eo = F['mo']; ok, ek = F['ok']; mk, emk = F['mk']
    rok = {c: b for b, c in ok.items()}
    changed = 0
    for it in range(3):
        r = {b: a for a, b in mo.items()}
        dis = {a for a, b in mo.items() if ok.get(b) and mk.get(a) and ok[b] != mk[a]}
        for a in sorted(dis):
            b = mo[a]
            if 'strings' in eo[a][1] or eo[a][1].startswith('manual'):
                continue
            bs = rok.get(mk[a])
            if not bs:
                continue
            holder = r.get(bs)
            if holder is None or holder in dis:
                if holder is not None:
                    # swap
                    mo[holder] = b; eo[holder] = ('medium', 'triangle-swap(was %s);' % bs + eo[holder][1])
                    r[b] = holder
                else:
                    del r[b]
                mo[a] = bs; eo[a] = ('medium', 'triangle-swap(was %s);' % b + eo[a][1])
                r[bs] = a
                changed += 1
                dis = {x for x, y in mo.items() if ok.get(y) and mk.get(x) and ok[y] != mk[x]}
    return changed


if __name__ == '__main__':
    F = pickle.load(open(FINAL, 'rb'))
    print('swapped', resolve_disagreements(F))
    pickle.dump(F, open(FINAL, 'wb'))
