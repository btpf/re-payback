"""Extract the Mac release (PaybackMac.toast, a classic HFS volume) to
extracted/mac/.

Each file's data fork is written under its HFS path; a non-empty resource
fork goes next to it as `<name>.rsrc`, and type/creator codes are listed in
extracted/mac/_finder_info.tsv. HFS names may contain characters Windows
does not allow; those are replaced with '_'.

Requires `pip install machfs`.
"""
import os
import sys

import machfs

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, 'Payback Game + Updates', 'PaybackMac.toast')
OUT = os.path.join(ROOT, 'extracted', 'mac')

BAD = '<>:"\\|?*'


def safe(name):
    name = ''.join('_' if c in BAD or ord(c) < 32 else c for c in name)
    return name.rstrip(' .') or '_'


def walk(folder, rel, info):
    for name, item in folder.items():
        path = os.path.join(rel, safe(name))
        full = os.path.join(OUT, path)
        if isinstance(item, machfs.Folder):
            os.makedirs(full, exist_ok=True)
            walk(item, path, info)
            continue
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, 'wb') as f:
            f.write(item.data)
        if item.rsrc:
            with open(full + '.rsrc', 'wb') as f:
                f.write(item.rsrc)
        info.append((path.replace(os.sep, '/'), item.type.decode('mac_roman'), item.creator.decode('mac_roman'),
                     len(item.data), len(item.rsrc)))


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else SRC
    vol = machfs.Volume()
    with open(src, 'rb') as f:
        vol.read(f.read())
    os.makedirs(OUT, exist_ok=True)
    info = []
    walk(vol, '', info)
    with open(os.path.join(OUT, '_finder_info.tsv'), 'w', encoding='utf-8', newline='\n') as f:
        f.write('path\ttype\tcreator\tdata\trsrc\n')
        for row in info:
            f.write('\t'.join(str(x) for x in row) + '\n')
    print(f'{len(info)} files -> {OUT}')


if __name__ == '__main__':
    main()
