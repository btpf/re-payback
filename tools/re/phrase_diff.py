"""Diff Mac vs Amiga phrase files entry by entry (parser mirrors payback-formats phrases.rs).

usage: python tools/re/phrase_diff.py
Writes out/mac/phrase_diff_english.txt (Update 7 vs Mac), phrase_diff_english_vs_cd.txt
(CD vs Mac) and phrase_diff_de_fr.txt. The output quotes game text: keep it out of git.
"""
import os, sys
from common import EXTRACTED, OUT as OUTROOT
A = str(EXTRACTED / 'game' / 'Phrases'); M = str(EXTRACTED / 'mac' / 'Payback' / 'Phrases')
I = str(EXTRACTED / 'iso' / 'Payback' / 'Phrases')
OUT = str(OUTROOT / 'mac')

def parse(data):
    text = data.decode('latin-1').replace('\r', '')
    out = {}; order = []
    rest = text
    while rest:
        line, _, tail = rest.partition('\n')
        t = line.strip()
        if not t or t.startswith('#'):
            rest = tail; continue
        if '=' not in line:
            print('  !! no "=":', repr(line[:60]), file=sys.stderr); rest = tail; continue
        num, after = line.split('=', 1)
        num = int(num.strip()); after = after.lstrip()
        if after.startswith('"'):
            start = len(text) - len(rest) + (len(line) - len(after) + 1)
            alls = text[start:]
            end = alls.find('"')
            if end < 0:
                end = alls.find('\n') if '\n' in alls else len(alls)
            body = alls[:end]
            consumed = text[start + end:]
            rest = consumed.partition('\n')[2]
        else:
            body = after.rstrip().rstrip('"'); rest = tail
        if num in out: print('  dup', num, file=sys.stderr)
        out[num] = body; order.append(num)
    return out

def lst(d):
    return {f.lower(): os.path.join(d, f) for f in os.listdir(d)}

def diff(a, m, label, fh, summary):
    pa = parse(open(a, 'rb').read()); pm = parse(open(m, 'rb').read())
    ch = [n for n in sorted(set(pa) & set(pm)) if pa[n] != pm[n]]
    ad = sorted(set(pm) - set(pa)); rm = sorted(set(pa) - set(pm))
    summary.append((label, len(pa), len(pm), len(ch), len(ad), len(rm)))
    fh.write(f'\n==== {label}: amiga {len(pa)} entries, mac {len(pm)}; changed {len(ch)}, added {len(ad)}, removed {len(rm)}\n')
    for n in ch:
        fh.write(f'~ {n}\n    A: {pa[n]!r}\n    M: {pm[n]!r}\n')
    for n in ad:
        fh.write(f'+ {n}\n    M: {pm[n]!r}\n')
    for n in rm:
        fh.write(f'- {n}\n    A: {pa[n]!r}\n')

if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    al = lst(A); il = lst(I); ml = lst(M)
    for base, name in ((al, 'phrase_diff_english.txt'), (il, 'phrase_diff_english_vs_cd.txt')):
        summary = []
        with open(os.path.join(OUT, name), 'w', encoding='utf-8', newline='\n') as fh:
            for lv in ['global', '00', '01', '10', '11', '20', '21']:
                k = f'{lv}.english.phrases'
                diff(base[k], ml[k], k, fh, summary)
        print(name); [print('  ', s) for s in summary]
    # German / French
    summary = []
    with open(os.path.join(OUT, 'phrase_diff_de_fr.txt'), 'w', encoding='utf-8', newline='\n') as fh:
        for lang, alang in (('deutsch', 'deutsch'), ('francais', 'franτais')):
            for lv in ['global', '00', '01', '10', '11', '20', '21']:
                diff(al[f'{lv}.{alang}.phrases'], ml[f'{lv}.{lang}.phrases'], f'{lv}.{lang}', fh, summary)
    print('de/fr'); [print('  ', s) for s in summary]
