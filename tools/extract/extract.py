"""Unpack the original Payback CD image and updates into extracted/.

    extracted/iso      - the CD (Payback.iso)
    extracted/update7  - PaybackUpdate7.lha (last official update)
    extracted/os4      - paybackupdateOS4Native.lha (community OS4 PPC port)
    extracted/warp3d   - Warp3D-42.lha
    extracted/game     - merged install: CD Payback/ dir overlaid with Update 7

Needs 7z on PATH. Paths are relative to the repository root, whatever the
current directory.
"""
import os
import shutil
import subprocess

os.chdir(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

SRC = 'Payback Game + Updates'
OUT = 'extracted'

ARCHIVES = {
    'iso': f'{SRC}/Payback.iso',
    'update7': f'{SRC}/Updates/PaybackUpdate7.lha',
    'os4': f'{SRC}/Updates/paybackupdateOS4Native.lha',
    'warp3d': f'{SRC}/Updates/Warp3D-42.lha',
}

for name, archive in ARCHIVES.items():
    subprocess.run(['7z', 'x', '-y', f'-o{OUT}/{name}', archive],
                   check=True, stdout=subprocess.DEVNULL)

shutil.rmtree(f'{OUT}/game', ignore_errors=True)
shutil.copytree(f'{OUT}/iso/Payback', f'{OUT}/game')
shutil.copytree(f'{OUT}/update7/PaybackUpdate7/Files', f'{OUT}/game', dirs_exist_ok=True)
print(f'extracted to {OUT}/')
