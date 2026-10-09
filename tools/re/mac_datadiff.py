"""Compare the Mac data files with the merged Amiga game data, per directory."""
import collections
import os

from common import EXTRACTED as BASE


def files(root):
    out = {}
    for dp, _, fn in os.walk(root):
        for f in fn:
            if f.endswith('.rsrc') or f.endswith('.info'):
                continue
            p = os.path.join(dp, f)
            out[os.path.relpath(p, root).replace(os.sep, '/').lower()] = p
    return out


mac = files(os.path.join(BASE, 'mac', 'Payback'))
ami = files(os.path.join(BASE, 'game'))
dirs = collections.defaultdict(lambda: [0, 0, 0, 0])
diff_examples = collections.defaultdict(list)
only_examples = collections.defaultdict(list)
for r, p in mac.items():
    d = r.split('/')[0] if '/' in r else '.'
    if r in ami:
        same = open(p, 'rb').read() == open(ami[r], 'rb').read()
        dirs[d][0 if same else 1] += 1
        if not same:
            diff_examples[d].append((r, os.path.getsize(ami[r]), os.path.getsize(p)))
    else:
        dirs[d][2] += 1
        only_examples[d].append(r)
for r in ami:
    if r not in mac:
        d = r.split('/')[0] if '/' in r else '.'
        dirs[d][3] += 1
print('dir: same, differ, mac-only, amiga-only')
for d, v in sorted(dirs.items()):
    print(f'{d:20} {v}  diff e.g. {diff_examples[d][:3]}  mac-only e.g. {only_examples[d][:4]}')
