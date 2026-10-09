#!/usr/bin/env python3
# Checks the .route next-hop tables of Maps/0..2 against plain BFS over the links
# (docs/re/combat_police.md). usage: python tools/re/route_check.py
import struct, heapq, math, collections, sys
from common import GAME
base = str(GAME / "Maps") + "/"
def load(n):
    d = open(base + n + ".route", "rb").read()
    assert len(d) == 1024 * 270
    nodes = {}
    for i in range(1024):
        r = d[i*270:(i+1)*270]
        links = struct.unpack(">4H", r[0:8])
        x, y = struct.unpack(">2H", r[8:12])
        flag = r[12]
        tab = r[13:269]
        pad = r[269]
        nodes[i] = dict(links=links, x=x, y=y, flag=flag, tab=tab, pad=pad)
    return nodes
def hop(n, dest):
    b = n['tab'][dest >> 2]
    return (b >> ((dest & 3) * 2)) & 3
for m in "012":
    N = load(m)
    used = [i for i in N if N[i]['flag'] != 0xff]
    flags = collections.Counter(N[i]['flag'] for i in N)
    pads = collections.Counter(N[i]['pad'] for i in N)
    print("map", m, "used", len(used), "max idx", max(used), "flags", dict(flags), "pads", dict(pads))
    # contiguity
    print(" contiguous:", used == list(range(len(used))))
    # unused records all zero except flag?
    nz = sum(1 for i in N if N[i]['flag']==0xff and (any(N[i]['links'] ) or N[i]['x'] or N[i]['y'] or any(N[i]['tab'])))
    print(" unused nonzero:", nz)
    lc = collections.Counter(sum(1 for l in N[i]['links'] if l != 9999) for i in used)
    print(" link counts", dict(lc))
    # link positions: are 9999 always trailing?
    nontrail = sum(1 for i in used if any(N[i]['links'][k]==9999 and N[i]['links'][k+1]!=9999 for k in range(3)))
    print(" non-trailing gaps", nontrail)
    bad = [(i,l) for i in used for l in N[i]['links'] if l!=9999 and (l>=1024 or N[l]['flag']==0xff)]
    print(" bad links", bad[:5])
    sym = sum(1 for i in used for l in N[i]['links'] if l!=9999 and i not in N[l]['links'])
    print(" one-way links", sym, "of", sum(1 for i in used for l in N[i]['links'] if l!=9999))
    xs=[N[i]['x'] for i in used]; ys=[N[i]['y'] for i in used]
    print(" x range", min(xs), max(xs), "y range", min(ys), max(ys), "x&0xff", collections.Counter(x&0xff for x in xs).most_common(5))
    # verify routing table: Dijkstra euclid & BFS
    def dist(a,b): return math.hypot(N[a]['x']-N[b]['x'], N[a]['y']-N[b]['y'])
    def manh(a,b): return abs(N[a]['x']-N[b]['x'])+abs(N[a]['y']-N[b]['y'])
    res = collections.Counter()
    for metric_name, metric in (("euclid",dist),("manh",manh),("hops",lambda a,b:1)):
        ok=bad_=selfv=unreach=0
        badex=[]
        for dst in used:
            # reverse dijkstra: distances to dst
            D={dst:0}; pq=[(0,dst)]
            rev=collections.defaultdict(list)
            for i in used:
                for l in N[i]['links']:
                    if l!=9999: rev[l].append(i)
            while pq:
                dd,u=heapq.heappop(pq)
                if dd>D[u]: continue
                for p in rev[u]:
                    nd=dd+metric(p,u)
                    if nd < D.get(p,1e18)-1e-9:
                        D[p]=nd; heapq.heappush(pq,(nd,p))
            for src in used:
                if src==dst: continue
                if src not in D: unreach+=1; continue
                k=hop(N[src],dst); l=N[src]['links'][k]
                if l==9999: bad_+=1; badex.append((src,dst,k)); continue
                if abs(metric(src,l)+D.get(l,1e18)-D[src])<1e-6*max(1,D[src])+1e-6: ok+=1
                else: bad_+=1; badex.append((src,dst,k))
        print("  ", metric_name, "ok", ok, "bad", bad_, "unreach", unreach, badex[:3])


print("---- detail of hop-count mismatches")
for m in "012":
    N=load(m); used=[i for i in N if N[i]['flag']!=0xff]
    rev=collections.defaultdict(list)
    for i in used:
        for l in N[i]['links']:
            if l!=9999: rev[l].append(i)
    for dst in used:
        D={dst:0}; q=collections.deque([dst])
        while q:
            u=q.popleft()
            for p in rev[u]:
                if p not in D: D[p]=D[u]+1; q.append(p)
        for src in used:
            if src==dst: continue
            k=hop(N[src],dst); l=N[src]['links'][k]
            if l==9999 or D.get(l,1e9)+1!=D[src]:
                print(m, "src",src,"dst",dst,"k",k,"link",l,"D[src]",D[src],"D[l]",D.get(l), "links", N[src]['links'])
