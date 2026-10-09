"""Match functions between two builds: string seeds, call-graph propagation and
address-order gap filling (both PPC builds keep the source-file function order)."""
import collections, difflib, re
import load

SMALL = set(range(-16, 17))


def feats(f):
    nums = [n for n in f['nums'] if isinstance(n, float) or (n not in SMALL and not (0x01000000 <= n < 0x20000000) and n < 0xfff00000)]
    return dict(size=f['size'], ncalls=len(f['calls']), ncallees=len(set(f['calls'])),
                ncallers=len(set(f['callers'])), ifs=f['ifs'], loops=f['loops'], sw=f['switch'],
                nums=collections.Counter(nums), np=f['nparams'])


def ratio(x, y):
    if x == y:
        return 1.0
    return min(x, y) / max(x, y) if max(x, y) else 1.0


def shape_sim(A, B):
    s = 0.0; w = 0.0
    s += 3 * ratio(A['size'] + 40, B['size'] + 40); w += 3
    s += 2 * ratio(A['ncallees'] + 1, B['ncallees'] + 1); w += 2
    s += 1 * ratio(A['ifs'] + 1, B['ifs'] + 1); w += 1
    s += 1 * ratio(A['loops'] + 1, B['loops'] + 1); w += 1
    na, nb = A['nums'], B['nums']
    if na or nb:
        inter = sum((na & nb).values()); uni = sum((na | nb).values())
        s += 3 * (inter / uni if uni else 0); w += 3
    return s / w


def compat(ra, rb):
    if ra == 'libc' or rb == 'libc':
        return ra == rb
    return True


def mac_region(a):
    x = int(a, 16)
    return 'libc' if 0x1015d000 <= x < 0x1016f598 else 'g'


def os4_region(b):
    x = int(b, 16)
    if 0x010f8100 <= x < 0x01111180:
        return 'libc'
    return 'asm' if x >= 0x01111180 else 'g'


def mac_allowed(a, f):
    n = f['name']
    if int(a, 16) < 0x10051910:
        return False
    if re.match(r'^\.\w+__F', n):
        return True
    if n.startswith('FUN_') and (0x100fb000 <= int(a, 16) < 0x100fc000 or 0x1015d000 <= int(a, 16) < 0x1016f598):
        return True
    if n in ('.main',):
        return True
    return False


class Matcher:
    def __init__(self, ba, bb, allowA=None, allowB=None, size_scale=1.0, regA=None, regB=None):
        self.regA = regA or (lambda a: 'g'); self.regB = regB or (lambda b: 'g')
        self.ban = set()
        self.A = load.parse(ba)['funcs']; self.B = load.parse(bb)['funcs']
        self.ba, self.bb = ba, bb
        self.okA = {a for a, f in self.A.items() if (allowA is None or allowA(a, f))}
        self.okB = {b for b, f in self.B.items() if (allowB is None or allowB(b, f))}
        self.FA = {a: feats(f) for a, f in self.A.items()}
        self.FB = {b: feats(f) for b, f in self.B.items()}
        for v in self.FB.values():
            v['size'] = v['size'] * size_scale
        # call lists restricted to allowed functions
        self.cA = {a: [c for c in f['calls'] if c in self.okA] for a, f in self.A.items()}
        self.cB = {b: [c for c in f['calls'] if c in self.okB] for b, f in self.B.items()}
        self.rA = {a: sorted(set(c for c in f['callers'] if c in self.okA)) for a, f in self.A.items()}
        self.rB = {b: sorted(set(c for c in f['callers'] if c in self.okB)) for b, f in self.B.items()}
        self.m = {}; self.r = {}; self.ev = {}

    def add(self, a, b, conf, ev):
        if a in self.m or b in self.r or a not in self.okA or b not in self.okB:
            return False
        if (a, b) in self.ban or not compat(self.regA(a), self.regB(b)):
            return False
        self.m[a] = b; self.r[b] = a; self.ev[a] = (conf, ev)
        return True

    def drop(self, a):
        b = self.m.pop(a); del self.r[b]; del self.ev[a]

    def cg_sim(self, a, b):
        """agreement of already-matched callees/callers (fraction, count)."""
        agree = tot = 0
        cb = set(self.cB[b]); rb = set(self.rB[b])
        for c in set(self.cA[a]):
            if c in self.m:
                tot += 1; agree += self.m[c] in cb
        for c in self.rA[a]:
            if c in self.m:
                tot += 1; agree += self.m[c] in rb
        # B-side matched callees that A lacks
        ca = set(self.cA[a]); ra = set(self.rA[a])
        for c in cb:
            if c in self.r:
                tot += 1; agree += self.r[c] in ca
        for c in rb:
            if c in self.r:
                tot += 1; agree += self.r[c] in ra
        return (agree / tot if tot else None), tot

    def update_globals(self):
        c = collections.Counter(); na = collections.Counter(); nb = collections.Counter()
        for a, b in self.m.items():
            ga = set(self.A[a]['globs']); gb = set(self.B[b]['globs'])
            if len(ga) > 40 or len(gb) > 40:
                continue
            for g in ga:
                na[g] += 1
            for h in gb:
                nb[h] += 1
            for g in ga:
                for h in gb:
                    c[(g, h)] += 1
        gm = {}
        best = {}
        for (g, h), v in c.items():
            if v >= 2 and v >= 0.7 * na[g] and v >= 0.7 * nb[h]:
                if v > best.get(g, (0,))[0]:
                    best[g] = (v, h)
        self.gmap = {g: h for g, (v, h) in best.items()}
        self.gmapped = set(self.gmap.values())

    def gsim(self, a, b):
        if not hasattr(self, 'gmap'):
            return None, 0
        ga = {self.gmap[g] for g in self.A[a]['globs'] if g in self.gmap}
        gb = {h for h in self.B[b]['globs'] if h in self.gmapped}
        u = ga | gb
        if not u:
            return None, 0
        return len(ga & gb) / len(u), len(u)

    def sim(self, a, b):
        sh = shape_sim(self.FA[a], self.FB[b])
        parts = [(sh, 1.0)]
        cg, n = self.cg_sim(a, b)
        if cg is not None:
            parts.append((cg, min(n, 6) / 6 * 1.5))
        gj, k = self.gsim(a, b)
        if gj is not None:
            parts.append((gj, min(k, 4) / 4 * 1.0))
        return sum(x * w for x, w in parts) / sum(w for x, w in parts)

    # ---- seeds -----------------------------------------------------
    def strings(self):
        sa = collections.defaultdict(set); sb = collections.defaultdict(set)
        for a, f in self.A.items():
            if a in self.okA:
                for s in f['strings']:
                    sa[s].add(a)
        for b, f in self.B.items():
            if b in self.okB:
                for s in f['strings']:
                    sb[s].add(b)
        score = collections.defaultdict(float); nshared = collections.Counter()
        for s in set(sa) & set(sb):
            if len(sa[s]) > 4 or len(sb[s]) > 4:
                continue
            w = 1.0 / (len(sa[s]) * len(sb[s])) * min(1.0, len(s) / 8)
            for a in sa[s]:
                for b in sb[s]:
                    score[(a, b)] += w; nshared[(a, b)] += 1
        n = self._resolve(score, lambda a, b, v: (v >= 0.5, 'high' if v >= 0.99 and self.sim(a, b) > 0.5 else 'medium',
                                                    f'strings({nshared[(a, b)]})'), margin=0.6, minsim=0.3)
        return n

    def _resolve(self, score, judge, margin=0.7, minsim=0.45):
        byA = collections.defaultdict(list); byB = collections.defaultdict(list)
        for (x, y), v in score.items():
            if x in self.m or y in self.r:
                continue
            byA[x].append((v, y)); byB[y].append((v, x))
        n = 0
        for x, lst in sorted(byA.items(), key=lambda t: -max(t[1])[0]):
            lst.sort(reverse=True)
            v, y = lst[0]
            if x in self.m or y in self.r:
                continue
            lb = sorted(byB[y], reverse=True)
            if lb[0][1] != x:
                continue
            sec = max([l[0] for l in lst[1:2]] + [l[0] for l in lb[1:2]] + [0])
            if sec > v * margin:
                continue
            if self.sim(x, y) < minsim:
                continue
            ok, conf, ev = judge(x, y, v)
            if ok and self.add(x, y, conf, ev):
                n += 1
        return n

    def order_factor_fn(self, win=8):
        import bisect
        sA = sorted(self.okA); sB = sorted(self.okB)
        iA = {k: i for i, k in enumerate(sA)}; iB = {k: i for i, k in enumerate(sB)}
        ma = sorted(iA[a] for a in self.m)
        mapb = {iA[a]: iB[b] for a, b in self.m.items()}

        def f(x, y):
            ix = iA[x]; iy = iB[y]
            k = bisect.bisect_left(ma, ix)
            near = [ma[t] for t in (k - 1, k) if 0 <= t < len(ma)]
            good = 0
            for p in near:
                da = ix - p; db = iy - mapb[p]
                if (da > 0) == (db > 0) and abs(da - db) <= win:
                    good += 1
            return 1.5 if good == 2 else (1.15 if good == 1 else 0.7)
        return f

    # ---- propagation -------------------------------------------------
    def propagate(self, min_sim=0.45):
        cand = collections.defaultdict(float)
        for a, b in list(self.m.items()):
            fa, fb = self.A[a], self.B[b]
            fra = [c for c in fa['frefs'] if c in self.okA]
            frb = [c for c in fb['frefs'] if c in self.okB]
            for la, lb in ((self.cA[a], self.cB[b]), (fra, frb)):
                ta = [('M', self.m[c]) if c in self.m else ('U', c) for c in la]
                tb = [('M', c) if c in self.r else ('U', c) for c in lb]
                ka = [t if t[0] == 'M' else ('U',) for t in ta]
                kb = [t if t[0] == 'M' else ('U',) for t in tb]
                sm = difflib.SequenceMatcher(None, ka, kb, autojunk=False)
                for op, i1, i2, j1, j2 in sm.get_opcodes():
                    if op == 'equal':
                        for i, j in zip(range(i1, i2), range(j1, j2)):
                            if ta[i][0] == 'U':
                                cand[(ta[i][1], tb[j][1])] += 1.0
                    elif op == 'replace':
                        ua = [t[1] for t in ta[i1:i2] if t[0] == 'U']
                        ub = [t[1] for t in tb[j1:j2] if t[0] == 'U']
                        if len(set(ua)) == 1 and len(set(ub)) == 1:
                            cand[(ua[0], ub[0])] += 0.6
                        elif len(ua) == len(ub):
                            for x, y in zip(ua, ub):
                                cand[(x, y)] += 0.3
            ua = [c for c in self.rA[a] if c not in self.m]
            ub = [c for c in self.rB[b] if c not in self.r]
            if len(ua) == 1 and len(ub) == 1:
                cand[(ua[0], ub[0])] += 1.0
            elif ua and ub and len(ua) <= 6 and len(ub) <= 6:
                for x in ua:
                    for y in ub:
                        cand[(x, y)] += 0.15
        score = {}
        of = self.order_factor_fn()
        for (x, y), v in cand.items():
            if x in self.m or y in self.r:
                continue
            s = self.sim(x, y)
            if s < min_sim:
                continue
            score[(x, y)] = v * s * of(x, y)

        def judge(x, y, v):
            s = self.sim(x, y)
            c = 'high' if v >= 1.5 and s >= 0.7 else ('medium' if s >= 0.55 or v >= 1.0 else 'low')
            return v >= 0.4, c, f'callgraph(v={v:.2f},sim={s:.2f})'
        return self._resolve(score, judge)

    # ---- address-order gap filling ------------------------------------
    def chain(self):
        """Longest chain of matches increasing in both address spaces."""
        pairs = sorted((int(a, 16), int(b, 16), a, b) for a, b in self.m.items())
        import bisect
        tails = []; tails_idx = []; prev = [None] * len(pairs)
        for i, (_, bv, _, _) in enumerate(pairs):
            k = bisect.bisect_left(tails, bv)
            if k == len(tails):
                tails.append(bv); tails_idx.append(i)
            else:
                tails[k] = bv; tails_idx[k] = i
            prev[i] = tails_idx[k - 1] if k else None
        out = []; i = tails_idx[-1] if tails_idx else None
        while i is not None:
            out.append(pairs[i]); i = prev[i]
        return [(a, b) for _, _, a, b in reversed(out)]

    def gapfill(self, maxgap=60, thr=0.62):
        ch = self.chain()
        sA = sorted(self.okA); sB = sorted(self.okB)
        import bisect
        n = 0
        for (a1, b1), (a2, b2) in zip(ch, ch[1:]):
            ua = [x for x in sA[bisect.bisect_right(sA, a1):bisect.bisect_left(sA, a2)] if x not in self.m]
            ub = [y for y in sB[bisect.bisect_right(sB, b1):bisect.bisect_left(sB, b2)] if y not in self.r]
            if not ua or not ub or len(ua) > maxgap or len(ub) > maxgap:
                continue
            # DP alignment
            S = [[self.sim(x, y) for y in ub] for x in ua]
            na, nb = len(ua), len(ub)
            D = [[0.0] * (nb + 1) for _ in range(na + 1)]
            for i in range(na - 1, -1, -1):
                for j in range(nb - 1, -1, -1):
                    D[i][j] = max(D[i + 1][j], D[i][j + 1], D[i + 1][j + 1] + (S[i][j] - thr))
            i = j = 0
            while i < na and j < nb:
                if D[i][j] == D[i + 1][j + 1] + (S[i][j] - thr) and S[i][j] >= thr:
                    s = S[i][j]
                    eq = na == nb
                    c = 'medium' if (s >= 0.8 or (eq and s >= 0.7)) else 'low'
                    if self.add(ua[i], ub[j], c, f'order-gap(sim={s:.2f},gap={na}/{nb})'):
                        n += 1
                    i += 1; j += 1
                elif D[i][j] == D[i + 1][j]:
                    i += 1
                else:
                    j += 1
        return n

    def run(self, rounds=40):
        log = []
        for r in range(rounds):
            n1 = self.propagate()
            n2 = 0
            if n1 == 0:
                n2 = self.gapfill()
            log.append((n1, n2))
            if n1 == 0 and n2 == 0:
                break
        return log


def _idx(keys):
    s = sorted(keys)
    return s, {k: i for i, k in enumerate(s)}


def order_info(M, win=12):
    """For each match: is it consistent with its matched neighbours in address order?"""
    sA, iA = _idx(M.okA); sB, iB = _idx(M.okB)
    ma = sorted(M.m, key=lambda a: iA[a])
    res = {}
    for k, a in enumerate(ma):
        b = M.m[a]; ib = iB[b]
        good = 0; tot = 0
        for d in (-2, -1, 1, 2):
            if 0 <= k + d < len(ma):
                a2 = ma[k + d]; b2 = M.m[a2]
                da = iA[a2] - iA[a]; db = iB[b2] - ib
                tot += 1
                if (da > 0) == (db > 0) and abs(db) <= abs(da) + win and abs(da) <= abs(db) + win:
                    good += 1
        res[a] = (good, tot)
    return res


def prune(M):
    oi = order_info(M)
    n = 0
    for a, (g, t) in list(oi.items()):
        conf, ev = M.ev[a]
        if g == 0 and t > 0 and 'strings' not in ev and 'sole-' not in ev and not ev.startswith(('manual', 'triangle')):
            s = M.sim(a, M.m[a])
            v = float(re.search(r'v=([\d.]+)', ev).group(1)) if 'v=' in ev else 0
            strong = (v >= 3 and s >= 0.6) or (v >= 1.5 and s >= 0.8)
            if not strong:
                M.ban.add((a, M.m[a]))
                M.drop(a); n += 1
    return n


def local_gapfill(M, maxgap=40, thr=0.62):
    sA, iA = _idx(M.okA); sB, iB = _idx(M.okB)
    ma = sorted(M.m, key=lambda a: iA[a])
    n = 0
    for a1, a2 in zip(ma, ma[1:]):
        b1, b2 = M.m[a1], M.m[a2]
        ia1, ia2, ib1, ib2 = iA[a1], iA[a2], iB[b1], iB[b2]
        if not (ib1 < ib2) or ia2 - ia1 > maxgap or ib2 - ib1 > maxgap:
            # section break: align the unmatched run after (a1,b1) and before (a2,b2)
            if ia2 - ia1 > 1:
                n += _edge(M, sA, sB, ia1, ib1, +1, thr + 0.08)
                n += _edge(M, sA, sB, ia2, ib2, -1, thr + 0.08)
            continue
        ua = [x for x in sA[ia1 + 1:ia2] if x not in M.m]
        ub = [y for y in sB[ib1 + 1:ib2] if y not in M.r]
        if not ua or not ub:
            continue
        S = [[M.sim(x, y) for y in ub] for x in ua]
        na, nb = len(ua), len(ub)
        D = [[0.0] * (nb + 1) for _ in range(na + 1)]
        for i in range(na - 1, -1, -1):
            for j in range(nb - 1, -1, -1):
                D[i][j] = max(D[i + 1][j], D[i][j + 1], D[i + 1][j + 1] + (S[i][j] - thr))
        i = j = 0
        while i < na and j < nb:
            if S[i][j] >= thr and D[i][j] == D[i + 1][j + 1] + (S[i][j] - thr):
                s = S[i][j]
                c = 'medium' if (s >= 0.8 or (na == nb and s >= 0.7)) else 'low'
                if M.add(ua[i], ub[j], c, f'order-gap(sim={s:.2f},gap={na}/{nb})'):
                    n += 1
                i += 1; j += 1
            elif D[i][j] == D[i + 1][j]:
                i += 1
            else:
                j += 1
    return n


def full(M, log=print):
    log('seeds', M.strings())
    M.update_globals()
    for it in range(8):
        steps = []
        while True:
            n = M.propagate()
            steps.append(n)
            if n == 0:
                break
        M.update_globals()
        p = prune(M)
        g = local_gapfill(M)
        g += cgpass(M)
        log('iter', it, 'prop', sum(steps), 'pruned', p, 'gap', g, 'total', len(M.m))
        if g == 0 and p == 0 and sum(steps) == 0:
            break
    uc = unique_children(M)
    rp = repair(M)
    log('repaired', rp, 'unique-children', uc)
    # final confidence adjust from order consistency
    oi = order_info(M)
    for a, (g, t) in oi.items():
        conf, ev = M.ev[a]
        s = M.sim(a, M.m[a])
        ev2 = re.sub(r';order=\d+/\d+', '', ev) + f';order={g}/{t}'
        if g >= 2 and conf == 'medium' and s >= 0.75:
            conf = 'high'
        elif g >= 2 and conf == 'low' and s >= 0.6:
            conf = 'medium'
        elif g == 0 and conf == 'high' and 'strings' not in ev and not ev.startswith(('manual', 'triangle')):
            conf = 'medium'
        elif g == 0 and conf == 'medium' and 'strings' not in ev and not ev.startswith(('manual', 'triangle')):
            conf = 'low'
        M.ev[a] = (conf, ev2)


def cgpass(M, minj=0.5, mincommon=3):
    """Unmatched pairs whose matched callees (mapped) overlap strongly."""
    def imgA(a):
        return {M.m[c] for c in set(M.cA[a]) if c in M.m}
    def setB(b):
        return {c for c in set(M.cB[b]) if c in M.r}
    score = {}
    for a in M.okA:
        if a in M.m:
            continue
        ia = imgA(a)
        if len(ia) < mincommon:
            continue
        cands = set()
        for c in ia:
            cands.update(x for x in M.rB[c] if x not in M.r)
        for b in cands:
            sb = setB(b)
            inter = len(ia & sb)
            if inter < mincommon:
                continue
            j = inter / len(ia | sb)
            if j >= minj:
                score[(a, b)] = j * (0.5 + 0.5 * shape_sim(M.FA[a], M.FB[b]))

    def judge(a, b, v):
        s = M.sim(a, b)
        return s >= 0.5, ('high' if v >= 0.7 and s >= 0.7 else 'medium'), f'callee-set(j={v:.2f},sim={s:.2f})'
    return M._resolve(score, judge, margin=0.85, minsim=0.4)


def m68k_region(c):
    x = int(c, 16)
    return 'libc' if x >= 0x0010d000 else 'g'


def _edge(M, sA, sB, ia, ib, d, thr, k=12):
    """Walk away from an anchor in direction d on both sides while functions are unmatched."""
    ua = []; ub = []
    i = ia + d
    while 0 <= i < len(sA) and sA[i] not in M.m and len(ua) < k:
        ua.append(sA[i]); i += d
    j = ib + d
    while 0 <= j < len(sB) and sB[j] not in M.r and len(ub) < k:
        ub.append(sB[j]); j += d
    if not ua or not ub:
        return 0
    S = [[M.sim(x, y) for y in ub] for x in ua]
    na, nb = len(ua), len(ub)
    D = [[0.0] * (nb + 1) for _ in range(na + 1)]
    for i in range(na - 1, -1, -1):
        for j in range(nb - 1, -1, -1):
            D[i][j] = max(D[i + 1][j], D[i][j + 1], D[i + 1][j + 1] + (S[i][j] - thr))
    i = j = 0; n = 0
    while i < na and j < nb:
        if S[i][j] >= thr and D[i][j] == D[i + 1][j + 1] + (S[i][j] - thr):
            s = S[i][j]
            if M.add(ua[i], ub[j], 'medium' if s >= 0.85 else 'low', f'order-edge(sim={s:.2f})'):
                n += 1
            i += 1; j += 1
        elif D[i][j] == D[i + 1][j]:
            i += 1
        else:
            j += 1
    return n


def repair(M):
    """Re-point weak, order-inconsistent matches to an unmatched, order-consistent twin."""
    import bisect
    oi = order_info(M)
    sB = sorted(M.okB); iB = {k: i for i, k in enumerate(sB)}
    sA = sorted(M.okA); iA = {k: i for i, k in enumerate(sA)}
    n = 0
    for a, (g, t) in list(oi.items()):
        conf, ev = M.ev[a]
        if g >= 2 or 'strings' in ev or 'sole-' in ev or ev.startswith(('manual', 'triangle')):
            continue
        b = M.m[a]
        s0 = M.sim(a, b)
        # expected position from neighbours
        k = bisect.bisect_left(sorted(iA[x] for x in M.m), iA[a])
        ma = sorted(M.m, key=lambda x: iA[x])
        idx = ma.index(a)
        exp = []
        for d in (-1, 1):
            if 0 <= idx + d < len(ma):
                a2 = ma[idx + d]
                exp.append(iB[M.m[a2]] + (iA[a] - iA[a2]))
        best = None
        for e in exp:
            for j in range(e - 3, e + 4):
                if 0 <= j < len(sB):
                    y = sB[j]
                    if y in M.r:
                        continue
                    sy = M.sim(a, y)
                    if sy >= s0 - 0.05 and sy >= 0.6 and (best is None or sy > best[0]):
                        best = (sy, y)
        if best:
            M.drop(a)
            M.add(a, best[1], 'medium', f'order-repair(sim={best[0]:.2f},was {b})')
            n += 1
    return n


def unique_children(M, minsim=0.45):
    """Matched parents whose only unmatched callee (or caller) is unique on both sides."""
    total = 0
    while True:
        n = 0
        for a, b in list(M.m.items()):
            for la, lb, kind in ((M.cA[a], M.cB[b], 'sole-callee'), (M.rA[a], M.rB[b], 'sole-caller')):
                ua = {c for c in la if c not in M.m}
                ub = {c for c in lb if c not in M.r}
                if len(ua) == 1 and len(ub) == 1:
                    x = ua.pop(); y = ub.pop()
                    s = M.sim(x, y)
                    if s >= minsim:
                        M.ban.discard((x, y))
                        if M.add(x, y, 'medium' if s < 0.75 else 'high', f'{kind}(of {a},sim={s:.2f})'):
                            n += 1
        total += n
        if n == 0:
            return total
