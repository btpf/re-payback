# Front end, HUD and audio

Reverse-engineering notes for Payback's front end (title, menus, saves, replays), the
in-game HUD, the icon/font assets, sound/music and the intro video.

Addresses are in the AmigaOS4 PPC build (`analysis/ghidra_os4/decompiled.c`) unless
marked "68k". **CONFIRMED** = read in code and/or verified against every shipped file;
**INFERRED** = consistent with the data/code but not fully traced.

Scripts written for this document (all in `analysis/`):

| Script | Purpose |
|---|---|
| `os4mem.py` | read bytes/strings from the OS4 ELF by virtual address |
| `render_icons.py` | renders every `Icons/*` file to `analysis/img/icons/*.png` + contact sheets |
| `fontmap.py` | dumps the font glyph-slot table built by `FUN_0100e088`/`FUN_0100f450` |

<!-- SECTIONS -->
