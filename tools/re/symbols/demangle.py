"""Small CodeWarrior (Metrowerks) C++ demangler, enough for the Payback symbols."""
import re

BASE = {'v': 'void', 'b': 'bool', 'c': 'char', 's': 'short', 'i': 'int', 'l': 'long',
        'x': 'long long', 'f': 'float', 'd': 'double', 'r': 'long double', 'w': 'wchar_t',
        'e': '...'}
OPS = {'__ct': None, '__dt': None, '__nw': 'operator new', '__dl': 'operator delete',
       '__nwa': 'operator new[]', '__dla': 'operator delete[]', '__as': 'operator=',
       '__eq': 'operator==', '__ne': 'operator!=', '__cl': 'operator()', '__vc': 'operator[]',
       '__lt': 'operator<', '__gt': 'operator>', '__pl': 'operator+', '__mi': 'operator-'}


class P:
    def __init__(self, s):
        self.s = s; self.i = 0

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ''

    def num(self):
        m = re.match(r'\d+', self.s[self.i:])
        if not m:
            raise ValueError
        self.i += len(m.group(0))
        return int(m.group(0))

    def name(self):
        n = self.num()
        v = self.s[self.i:self.i + n]
        self.i += n
        return v

    def qual(self):
        if self.peek() == 'Q':
            self.i += 1
            k = int(self.s[self.i]); self.i += 1
            return '::'.join(self.name() for _ in range(k))
        return self.name()

    def type(self):
        pre = []
        while self.peek() in ('C', 'V', 'P', 'R', 'U', 'S'):
            pre.append(self.peek()); self.i += 1
        c = self.peek()
        if c in BASE:
            self.i += 1; t = BASE[c]
        elif c.isdigit() or c == 'Q':
            t = self.qual()
        elif c == 'F':
            self.i += 1
            args = self.params()
            ret = 'void'
            if self.peek() == '_':
                self.i += 1; ret = self.type()
            t = f'{ret} (*)({args})'
            if pre and pre[-1] == 'P':
                pre = pre[:-1]
        elif c == 'A':
            self.i += 1; n = self.num()
            if self.peek() == '_':
                self.i += 1
            t = self.type() + f'[{n}]'
        else:
            raise ValueError(c)
        for p in reversed(pre):
            if p == 'C': t = 'const ' + t if not t.endswith(('*', '&')) else t + ' const'
            elif p == 'V': t = 'volatile ' + t
            elif p == 'P': t = t + '*'
            elif p == 'R': t = t + '&'
            elif p == 'U': t = 'unsigned ' + t
            elif p == 'S': t = 'signed ' + t
        return t

    def params(self):
        out = []
        while self.i < len(self.s) and self.peek() != '_':
            out.append(self.type())
        if out == ['void']:
            out = []
        return ', '.join(out)


def demangle(sym):
    s = sym.lstrip('.')
    if s.startswith('FUN_') or '__' not in s[1:]:
        return s
    start = 2 if s.startswith('__') else 0
    for m in re.finditer(r'__', s[start:]):
        k = start + m.start()
        fn, rest = s[:k], s[k + 2:]
        if not fn:
            continue
        try:
            p = P(rest)
            cls = None
            if rest[:1].isdigit() or rest[:1] == 'Q':
                cls = p.qual()
            const = False
            if p.peek() == 'C':
                const = True; p.i += 1
            if p.peek() != 'F':
                continue
            p.i += 1
            args = p.params()
            if p.i != len(rest):
                continue
        except (ValueError, IndexError):
            continue
        if fn in OPS:
            base = cls.split('::')[-1].split('<')[0] if cls else ''
            fname = base if fn == '__ct' else ('~' + base if fn == '__dt' else OPS[fn])
        else:
            fname = fn
        full = (cls + '::' if cls else '') + fname
        return f'{full}({args})' + (' const' if const else '')
    return s


if __name__ == '__main__':
    for t in ['.drawCubeTop_None__FP4Cubeiiii', '.do_Person__Fv', '.CheckForCollisions__Fi',
              '.__ct__7LUndoerFv', '.GetToken__FPQ23std5_FILEi', '.Resolve__14LModelDirectorFRC6AEDescR6AEDesc',
              '.__nwa__FUl', '.printf_special__FPCce', '.IsLazy__12LModelObjectCFv', '.EstimateTime__F11amiga_PointP9Structure',
              '.__dt__23TArray<P12LModelObject>Fv', '.StartSound__FP10SndChannelsssb', '.MessageThem_Parse__Fie']:
        print(t, '->', demangle(t))
