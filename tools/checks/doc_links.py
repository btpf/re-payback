"""Checks that the relative links in every committed Markdown file resolve, and that the
repository paths named in the entry docs exist. usage: python tools/checks/doc_links.py
(run before committing doc moves; exit status 1 on a broken link)."""
import re, os, subprocess, sys
os.chdir(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
files = [f for f in subprocess.run(['git','ls-files','-c'],capture_output=True,text=True).stdout.split('\n') if f.endswith('.md')]
bad = 0; n = 0
for f in files:
    s = open(f, encoding='utf-8').read()
    s = re.sub(r'```.*?```', '', s, flags=re.S)
    for m in re.finditer(r'\]\(([^)\s]+)\)', s):
        t = m.group(1)
        if re.match(r'[a-z]+://|mailto:|#', t): continue
        p = os.path.normpath(os.path.join(os.path.dirname(f), t.split('#')[0]))
        n += 1
        if not os.path.exists(p):
            bad += 1; print(f'BROKEN {f}: {t}')
# backticked repo paths in the entry docs
for f in ['README.md','CONTRIBUTING.md','CLAUDE.md','docs/README.md','tools/README.md','docs/CONVENTIONS.md','docs/decompiling.md']:
    s = open(f, encoding='utf-8').read()
    for m in re.finditer(r'`((?:docs|tools|crates|web)/[A-Za-z0-9_./-]+)`', s):
        t = m.group(1).rstrip('/')
        if '*' in t or '<' in t: continue
        n += 1
        if not (os.path.exists(t) or os.path.exists(os.path.join(os.path.dirname(f), t))): bad += 1; print(f'MISSING {f}: {t}')
print(f'{n} links checked, {bad} broken')
sys.exit(1 if bad else 0)
