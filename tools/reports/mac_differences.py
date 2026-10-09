"""Build mac_differences.html: the consolidated Mac-vs-Amiga report.

Run from anywhere: python tools/reports/mac_differences.py
Reads the Mac research notes (docs/re/mac/*.md, docs/re/mac_version.md, docs/xref/), the
phrase diff (tools/re/phrase_diff.py, run on demand into out/mac/), the extracted data and a
handful of PNG renders from out/img/ (missing ones are shown as a warning), and writes a
single self-contained HTML file to the repository root. The output embeds renders of the
original game art: it is git-ignored and must never be committed. Needs `markdown` and `Pillow`.
"""
import base64
import collections
import datetime
import html
import io
import os
import re

import markdown
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
import subprocess
import sys
OUT = os.path.join(ROOT, 'mac_differences.html')


def rd(rel):
    with open(os.path.join(ROOT, rel), encoding='utf-8') as f:
        return f.read()


REP = {k: rd(f'docs/re/mac/{k}.md') for k in
       ('rendering', 'gameplay', 'gameplay_part2', 'frontend', 'assets', 'audio_platform')}
REP['symbols'] = rd('docs/xref/symbols.md')
MISSIONS = rd('docs/re/mac/gameplay_missions.md')
PEOPLE = rd('docs/re/mac/gameplay_people.md')
MACVER = rd('docs/re/mac_version.md')

# ----------------------------------------------------------------------------------------
# Markdown helpers
# ----------------------------------------------------------------------------------------
_LIST_RE = re.compile(r'^(\s*)([-*+]|\d+\.)\s')


def _prep(md):
    """Make GitHub-style markdown digestible by python-markdown: blank line before lists and
    tables, and 2-space nested indentation doubled to 4 (outside fenced code)."""
    out, fence, prev = [], False, ''
    for line in md.split('\n'):
        if line.lstrip().startswith('```'):
            # fences must start at column 0 for fenced_code
            if not fence and prev.strip():
                out.append('')
            fence = not fence
            out.append(line.lstrip())
            prev = line
            continue
        if fence:
            out.append(line)
            prev = line
            continue
        stripped = line.lstrip(' ')
        n = len(line) - len(stripped)
        if n and stripped:
            line = ' ' * (n * 2) + stripped
        is_list = bool(_LIST_RE.match(line)) and n == 0
        is_table = stripped.startswith('|') and n == 0
        prev_list = bool(_LIST_RE.match(prev)) or prev.startswith(' ')
        prev_table = prev.lstrip().startswith('|')
        if prev.strip() and ((is_list and not prev_list) or (is_table and not prev_table)):
            out.append('')
        out.append(line)
        prev = line
    return '\n'.join(out)


def md(text, shift=0):
    if shift:
        text = re.sub(r'^(#{1,6}) ', lambda m: '#' * min(6, len(m.group(1)) + shift) + ' ', text,
                      flags=re.M)
    h = markdown.markdown(_prep(text), extensions=['tables', 'fenced_code', 'sane_lists'])
    return h.replace('<table>', '<table class="md">')


def section(text, title_prefix, keep_heading=False):
    """Return the markdown of the section whose heading text starts with title_prefix."""
    lines = text.split('\n')
    start, level = None, None
    fence = False
    for i, l in enumerate(lines):
        if l.lstrip().startswith('```'):
            fence = not fence
        if fence:
            continue
        m = re.match(r'^(#{1,6}) (.*)$', l)
        if not m:
            continue
        if start is None and m.group(2).strip().startswith(title_prefix):
            start, level = i, len(m.group(1))
            continue
        if start is not None and len(m.group(1)) <= level:
            return '\n'.join(lines[start if keep_heading else start + 1:i]).strip()
    if start is None:
        raise KeyError(title_prefix)
    return '\n'.join(lines[start if keep_heading else start + 1:]).strip()


def il(s):
    """Inline mini-markdown for hand-written strings: `code`, **bold**, *em*."""
    s = html.escape(s, quote=False)
    s = re.sub(r'`([^`]+)`', r'<code>\1</code>', s)
    s = re.sub(r'\*\*([^*]+)\*\*', r'<b>\1</b>', s)
    s = re.sub(r'(?<![\w*])\*([^*\s][^*]*)\*(?![\w*])', r'<i>\1</i>', s)
    return s


# ----------------------------------------------------------------------------------------
# Images
# ----------------------------------------------------------------------------------------
IMG_BYTES = [0]


def img(rel, caption, max_w=1100, fmt='PNG', quality=82, nearest=False):
    p = os.path.join(ROOT, rel)
    if not os.path.exists(p):
        return f'<p class="warn">Missing image {html.escape(rel)}</p>'
    im = Image.open(p)
    if im.mode not in ('RGB', 'L', 'RGBA'):
        im = im.convert('RGBA' if 'A' in im.mode else 'RGB')
    if im.width > max_w:
        h = round(im.height * max_w / im.width)
        im = im.resize((max_w, h), Image.NEAREST if nearest else Image.LANCZOS)
    buf = io.BytesIO()
    if fmt == 'JPEG':
        im.convert('RGB').save(buf, 'JPEG', quality=quality, optimize=True)
        mime = 'image/jpeg'
    else:
        im.save(buf, 'PNG', optimize=True)
        mime = 'image/png'
    data = buf.getvalue()
    IMG_BYTES[0] += len(data)
    b64 = base64.b64encode(data).decode('ascii')
    return (f'<figure><img loading="lazy" src="data:{mime};base64,{b64}" alt="{html.escape(caption)}" '
            f'width="{im.width}" height="{im.height}"><figcaption>{il(caption)} '
            f'<span class="src">({html.escape(rel)})</span></figcaption></figure>')


# ----------------------------------------------------------------------------------------
# Chips
# ----------------------------------------------------------------------------------------
CHIP = {
    'bug fix': 'c-fix', 'Mac change': 'c-change', 'new feature': 'c-new',
    'platform artefact': 'c-plat', 'data change': 'c-data', 'Mac bug': 'c-bug',
    'adopt': 'c-adopt', 'optional': 'c-opt', 'ignore': 'c-ign', 'keep': 'c-keep',
    'done': 'c-keep', 'decide': 'c-opt',
    'CONFIRMED': 'c-conf', 'INFERRED': 'c-inf', 'MIXED': 'c-mixed',
    'high': 'c-bug', 'medium': 'c-opt', 'low': 'c-ign',
}


def chip(label):
    key = label
    if key not in CHIP:
        key = label.split(' ')[0]
    cls = CHIP.get(key, 'c-ign')
    return f'<span class="chip {cls}">{html.escape(label)}</span>'


# ----------------------------------------------------------------------------------------
# Consolidated "Mac differences" table
# area, mac, amiga, what, classification, recommendation, confidence, source
# ----------------------------------------------------------------------------------------
D = []


def diff(area, mac, amiga, what, cls, rec, conf, src):
    D.append(dict(area=area, mac=mac, amiga=amiga, what=what, cls=cls, rec=rec, conf=conf, src=src))


# Rendering and lighting
diff('Rendering', '`DrawMap` @ 1005a260 (prologue)', 'not in `FUN_01049374`',
     'Replay "TV camera": rotating view + replay playback + single player draws the city from a roaming, '
     'randomly re-spawning camera at pitch 0x14b4, range +4 squares (C1)',
     'new feature', 'adopt', 'CONFIRMED', 'rendering §2.3')
diff('Rendering', '`DrawMap` @ 1005b0b8', '`FUN_01049374`',
     'Chimney smoke uses `random3` per chimney per tick (independent puffs); Amiga draws 8 game-RNG numbers once per view (all chimneys in step) (C2/D5)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §2.4')
diff('Rendering', '`ToggleMotionBlur` @ 10053210', '`FUN_01042308`',
     'Turning motion blur on blanks both buffers (`BlankChunky`), so the first blended frame averages with black, not a stale frame (B1)',
     'bug fix', 'adopt', 'CONFIRMED', 'rendering §4')
diff('Rendering', '`CalcAlpha` @ 10083ec0, `General/Alpha.256`', '`FUN_0103b540`, U7 `Alpha.256`',
     'Alpha.256 regenerated exactly from the current (U7) palette; the Amiga U7 file was built from the CD palette (colour 8 = 7,7,7). 0.2-0.8 % of entries per table differ',
     'bug fix', 'adopt', 'CONFIRMED', 'rendering §4, assets §5.1')
diff('Rendering', '`GetGouraudTable` @ 10084400, `General/Gouraud.256`', '`FUN_0103c080` (generated at run time)',
     '8 MiB Gouraud table shipped, but stale: built from the CD palette (99.75-99.78 % vs a U7 computation)',
     'data change', 'decide', 'CONFIRMED', 'rendering §4 (ignore) vs assets §5.2 (read it)')
diff('Lighting', '`DoLightSourcing` @ 1015b840, `CheckVoxels` @ 100b3960', '`FUN_010766c8` (same, inlined)',
     'Paired shadow rays for NE/SW/SE corners: shadowed only if both rays are blocked. Same in both builds; the port casts one ray (L1)',
     'bug fix', 'adopt', 'CONFIRMED', 'rendering §3.2 (port fix, not a Mac change)')
diff('Lighting', '`DoLightSourcing` 1015bd18', '`FUN_010766c8` ~01076cf4',
     'Vert (slope 2) height bug: lighting adds h1 twice; present in both builds, port uses correct heights (L2)',
     'Mac bug', 'adopt', 'CONFIRMED', 'rendering §3.3 (original quirk, keep it)')
diff('Effects', '`UpdateEffects` @ 100aa1f0', '`FUN_010386e8`',
     'Snow dies at street level: flake removed at z > 0x4dc0, ground mark at 19904 (Amiga z > 24000, mark 0x57e4) (D1)',
     'bug fix', 'adopt', 'CONFIRMED', 'rendering §6.3')
diff('Effects', '`UpdateEffects`', '`FUN_010386e8`',
     'Weather wrap/delete rectangle = exact visible half-size + 1 square, not scaled with screen width; rotating and split-screen variants (D2)',
     'bug fix', 'adopt', 'CONFIRMED', 'rendering §6.3')
diff('Effects', '`amiga_levelloop` @ 10100f50', 'main loop after `FUN_010542b8`',
     'Lightning sets the flash to 100 + rand(150) instead of adding (no build-up on multi-flash strikes) (D3)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §6.3')
diff('Effects', '`amiga_startlevel` @ 100fff40', '`FUN_010692bc` (`FUN_0103ac94`)',
     'Level-start snow/rain spawn uses the visual generator `random3`, not the game RNG (D4)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §6.3')
diff('Effects', '`UpdateEffects`, `DoEffectsLighting` @ 100a82b0', '`FUN_010386e8`, `FUN_01036574`',
     'Random-argument evaluation order reversed in effect spawns and effect-light jitter (compiler order; visual only) (D8/D9)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §6.3')
diff('Effects', '`AddBrightness` @ 10083620', '`FUN_0103ad6c`',
     'Palette-flash quad does not reset GOURAUD state; may draw with a stale GL texenv (D10)',
     'platform artefact', 'ignore', 'CONFIRMED', 'rendering §6.3')
diff('Effects', 'effect loader, `EffectTextures/Blood`', '`FUN_01035250` (Blood.dark night swap)',
     'No `Blood.dark`; a smaller, darker `Blood` (1705 vs 3280 opaque texels) is used day and night (D4/D6)',
     'data change', 'adopt', 'CONFIRMED', 'rendering §8.2, assets §2')
diff('Effects', '`InitAll` ~l.23215, `DoSetLanguage` @ 1008c2b0', '`FUN_0104feb0` (Blood) - disputed',
     'German language loads `EffectTextures/Gas` into the blood slot (alpha mode 3); reloaded on language change (D7)',
     'Mac change', 'optional', 'CONFIRMED', 'frontend §14.1, rendering D7, assets §2')
diff('Hardware', '`W3D_*` 10153490-10154ba0', 'Warp3D driver',
     'Warp3D reimplemented on AGL/OpenGL: GOURAUD = GL_SMOOTH + MODULATE (off: FLAT + REPLACE), GL_LINEAR without mipmaps, GL_REPEAT, power-of-two textures capped at 256 (H1)',
     'platform artefact', 'keep', 'CONFIRMED', 'rendering §5')
diff('Hardware', '`W3D_DrawTriangle` @ 10153b10, `W3D_SetDrawRegion` @ 10153ec0', 'no Z buffer; no clear',
     'Painter\'s order kept with a per-primitive depth counter (>20000 primitives would clip); frame cleared to black each swap (H2/H3)',
     'platform artefact', 'ignore', 'CONFIRMED', 'rendering §5')
diff('Hardware', '`API_3D_DrawTriangle_Clipped*` 1014ce20-10151ec0', '`FUN_0100499c`/`01004b84`/`01004d6c`',
     'Real per-view triangle clipper (u/v, colour, perspective-correct): split-screen triangles cannot spill into the next view (H4)',
     'bug fix', 'keep', 'CONFIRMED', 'rendering §5, §9')
diff('Hardware', '`W3D_SetState`, `API_3D_Init` @ 1014ac60', 'same states requested of Warp3D',
     'GL_POINT_SMOOTH/GL_LINE_SMOOTH and GL_DITHER on: rain lines anti-aliased; 16-bit fullscreen colour (H5)',
     'platform artefact', 'ignore', 'CONFIRMED', 'rendering §5')
diff('Hardware', '`LoadConfig` @ 10052be0, prefs dialog', '`FUN_0104199c`, PaybackSetup',
     'OS X build is OpenGL-only (hardware is the default); OS 9 offers OpenGL or QuickDraw. Gouraud forced on and view size 0x100 under hardware (same as Amiga)',
     'platform artefact', 'ignore', 'CONFIRMED', 'rendering §5, frontend §1.2')
diff('UI scaling', '`CAmigaWindow` @ 10102400, `InitGraphics` @ 1008d110', '`FUN_010138dc`, any PaybackSetup mode',
     'Only 640x480 / 800x600 / 1024x768, window or DrawSprocket fullscreen. All three pass the W > 639 && H > 399 hi-res test, so the Mac always shows the 2x HUD (U1)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §7, frontend §5.1')
diff('UI scaling', '`ReadScreenParams` @ 10051fc0', '`FUN_0104107c`',
     'Same fx/fy/cx formulas; at 640x480 fx 32, fy 30 = the Amiga 320x256 framing on a 4:3 screen (HFOV 64 deg)',
     'Mac change', 'adopt', 'CONFIRMED', 'rendering §7')
diff('Objects', '`DrawObject` @ 10075880 etc.', '`FUN_010c101c` etc.',
     'Object renderer identical (modes, shine, dents, sorting, flares, headlights); objects never enable GOURAUD (GL_REPLACE), LF line endings accepted',
     'platform artefact', 'ignore', 'CONFIRMED', 'rendering §6.1')
# Data
diff('Textures', '`CubeTextures/`', '`extracted/game/CubeTextures`',
     '225 of 876 cube textures repainted: ~170 walls relit with a baked top-centre light pool, Light variants collapsed onto normals, Dark variants darker, new Crowd/Wood/Fence/TentRoof/NewWall6/7 art. Same 64x64 size and palette',
     'data change', 'adopt', 'CONFIRMED', 'assets §1, rendering §8.1')
diff('Textures', '`GetCubeTexture` @ 1015a9f0', 'same (fixed 0x1000 read)',
     '`TentRoof.chunky` truncated to 4032 bytes; the Mac reads 4096 into an uninitialised buffer (row 63 undefined)',
     'Mac bug', 'adopt', 'CONFIRMED', 'assets §1.2 (pad to 4096)')
diff('Textures', '`CubeTextureList.txt`, `CubeTextureRemapList.txt`', '`.CubeTextureList`, `.CubeTextureRemapList`',
     'Lists renamed (OS X hides dot-files); contents byte-identical, so flags, tiling and Gouraud remaps are unchanged',
     'platform artefact', 'adopt', 'CONFIRMED', 'assets §1.1, frontend §14.2 (read both names)')
diff('Textures', '`ObjectTextures/`', '83 identical',
     '10 new: Disk, PC, Monitor, Keyboard, Moviereel (menu props) and `tanne` / `tanne.clone` fir billboard for `Scenery/Tree.Obj`',
     'data change', 'adopt', 'CONFIRMED', 'assets §3')
diff('Models', '`Objects/People/*.Obj` (23)', 'Amiga people models',
     'Every vertex Y shifted by minus the model\'s max Y (90 MaleNormal, 92 most, 100 PedMale4): origin moved from the hips to the feet; bazooka/minigun +2 X. Mac `DrawObject` people transform unchanged, so this fixes "feet below floor" in data',
     'bug fix', 'adopt', 'CONFIRMED', 'assets §4.2, people.md M3')
diff('Models', 'Pug, Pug Racer, Taxi, Vapour', 'Amiga vehicles',
     'Shifted -18..-20 Z so the body bottom is at z = 0; Vapour loses 4 faces and its 40 `Shine.All` faces become 36 `Cardef` faces',
     'data change', 'decide', 'CONFIRMED', 'assets §4.2')
diff('Models', 'NewTree*, Phone*, TrafficLight*, Tree.Obj, Doors/bus.obj', 'Amiga scenery',
     'Lower crowns (242 to 202), phones 102 to 68, traffic lights 228 to 186, Tree.Obj redone as a `tanne` cross-billboard (411 to 244 tall), bus door quad moved up',
     'data change', 'adopt', 'CONFIRMED', 'assets §4.2')
diff('Models', '`Objects/Options/*`', 'Amiga menu props',
     'PC tower/monitor/keyboard replace the Amiga; gamepad replaces the joystick; "PAYBACK saved games" floppy (platform branding)',
     'data change', 'optional', 'CONFIRMED', 'assets §4.1')
diff('Models', 'Cup_Silver, World, QuestionMarks, Moviereel, CD', 'Amiga props',
     'Higher-poly or remodelled menu props (e.g. World 50 v to 422 v; CD now an upright disc)',
     'data change', 'adopt', 'CONFIRMED', 'assets §4.1')
diff('Icons', '`Icons/TitleScreen*` (4 files)', 'Amiga title screens',
     'New title art: desaturated zoom-blur, orange flaming PAYBACK logo, Apex Designs and Virtual Programming logos; same sizes',
     'data change', 'adopt', 'CONFIRMED', 'assets §6, frontend §5.4')
diff('Maps', '`Maps/2.thumb`', 'Amiga 2.thumb',
     'City 2 selection thumbnail re-rendered (green grass, correct day textures)',
     'bug fix', 'adopt', 'CONFIRMED', 'assets §7')
diff('Maps', '`Maps/0.scores`, `SaveHiscores` @ 1009eaa0', '`FUN_0102a0fc`',
     'Shipped scores file fails its checksum: `SaveHiscores` writes uninitialised stack words for scores and checksum, so Mac high scores never persist',
     'Mac bug', 'ignore', 'CONFIRMED', 'frontend §7.3, assets §7')
diff('Maps', '`UserMaps/`', '1 Amiga user map',
     '32 bundled community maps (31 playable; `test.map` skipped), several using custom `.textures`',
     'data change', 'adopt', 'CONFIRMED', 'frontend §8.2, assets §9')
# Gameplay
diff('Timing', '`amiga_levelloop` @ 10100f50', '68k `FUN_000ec686` (25 Hz), OS4 (30 Hz)',
     'Game tick 30 Hz (TickCount/2), ticks 1..5 per frame, 30 fps in game, 60 fps in menus, no interpolation (T1)',
     'Mac change', 'adopt', 'CONFIRMED', 'gameplay §1')
diff('Timing', '`amiga_levelloop`', '68k / OS4: 100 ticks',
     'Level-end fade counter 150 ticks (5 s) (T2)', 'Mac change', 'adopt', 'CONFIRMED', 'gameplay §1.3')
diff('Vehicle AI', '`do_Other` @ 10146280', '68k `FUN_000e3a58`',
     'Pursuit route-node arrival radius `300 + speed^2>>7` (Amiga `300 + abs(5*speed)`); flee unchanged (V1)',
     'Mac change', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Vehicle AI', '`MoveUnit` @ 10081080', '`FUN_000a143e` / `FUN_010cb7e4`',
     'Junction look-ahead `max(speed*1000/yaw, front)` (Amiga `5*speed^2/yaw`) (V2)',
     'bug fix', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Vehicle AI', '`do_Other`', '`FUN_000e3a58` ~124855',
     'Low-skill anti-spin no longer zeroes steering while braking (V3)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Traffic', '`AddThing` @ 10058350', '`FUN_000cf5cc`',
     'New traffic cars spawn with speed min(maxF, 100) (V4)', 'Mac change', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Tables', '`InitStructures` @ 1005ffb0', '`FUN_0104e13c` / `FUN_000d7874`',
     'Tank max reverse -220 (Amiga -100); all other 114 model rows identical (V5)', 'Mac change', 'adopt', 'CONFIRMED', 'gameplay §3.1')
diff('Collisions', '`CheckForCubeCollision` @ 100af980', '`FUN_00044f82` / `FUN_01070868`',
     'Map edge clamps vehicle probes and counts as a wall hit on the 3x3 (normal) sub-step (V6)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Vehicles', '`do_Other` tail', '68k ~125860',
     'Police light bar resets even if the siren never got a channel (V7)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay §4')
diff('Combat', '`DoCollideStuff` @ 10067b50', '`FUN_000dc5ac` / `FUN_010542b8`',
     'A destroyed remote bomb detonates (50000) and clears its owner\'s bomb slot (C1)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay §5.1')
diff('Combat', '`CheckForTriggers`, `CheckForCrate` @ 100593e0', '`FUN_000a4a68`, `FUN_01040ce4`',
     'Null check on the guard-sight watcher; crate bounds check moved into the callee', 'bug fix', 'ignore', 'CONFIRMED', 'gameplay_part2 §7')
diff('People', '`AddThing` (person branch)', 'not written',
     'Pavement peds store their spawn square centre (+0x7d4/+0x7d8) (P1): the anchor of the new pavement wander (reader found in people.md M1)', 'Mac change', 'adopt', 'CONFIRMED', 'gameplay §5.1, people.md M1')
diff('People', '`do_Person` @ 1013fbd0 (l.105953-106096)', '68k `FUN_000e3a58` ~122295-122400',
     'Pedestrians keep to the pavement: while the square and the square 512 ahead are pavement, re-aim every 3 ticks to heading +/-500 (1-in-50 go straight) and update the anchor; off pavement or at a wall, head back to the anchor every 20 ticks; keep walking slowly while turning. Amiga: walk straight 50 ticks, turn 90-270 deg at walls or off pavement',
     'Mac change', 'adopt', 'CONFIRMED', 'people.md M1')
diff('People', '`do_Person` (l.106712-106721)', '68k `FUN_000e3a58` ~123026',
     'On-foot velocity smoothed: `v = (v + (sin*speed>>7))/3` per frame (Amiga sets v directly), so car pushes and wall bounces carry over a few frames',
     'Mac change', 'adopt', 'CONFIRMED', 'people.md M2')
diff('People', '`do_Person` (l.107446-107453)', '68k ~123683',
     'Punch crate check rejects negative squares (only reachable near the west/north edge)',
     'bug fix', 'adopt', 'CONFIRMED', 'people.md M4')
diff('Missions', '`InitScript_21` @ 10129d20, `ProcessTrigger_21` @ 1013b2b0', '68k 0xb9aaa',
     'Level 21: new "car 50 destroyed" trigger fails mission 1 instead of soft-locking (M1)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay_part2 §6')
diff('Missions', '`SetTrigger_Long` @ 10106e50, `CheckForTriggers` @ 100f4a50', '`AddTriggerEx` 0xb2b12, 0xa4a68',
     'Actor triggers store the script id and resolve it every frame (no stale pointers; released actors stop matching) (M2)', 'bug fix', 'adopt', 'CONFIRMED', 'gameplay_part2 §6')
diff('Missions', '`amiga_levelloop`', '`FUN_000ec686` (no triggers in Rampage)',
     'Triggers run in Rampage when the map has a phone (M3)', 'new feature', 'adopt', 'CONFIRMED', 'gameplay_part2 §6')
diff('Missions', '`InitScript_Random` @ 10129e50, `ProcessTrigger_Random` @ 1012a0a0', 'none',
     'Random phone missions in Rampage: 7 generated jobs, scaled by police level, goal = top Rampage score (M4)', 'new feature', 'adopt', 'CONFIRMED', 'gameplay_part2 §6, missions.md §3')
diff('Missions', '`amiga_levelloop`', '`FUN_010692bc` ~49721 (2023)',
     'Rampage level end with trigger result 2 shows phrase 1799, which exists in no phrase file', 'Mac bug', 'ignore', 'CONFIRMED', 'frontend §6.3, gameplay_part2 §6')
# Audio
diff('Audio', '`DoAudio` @ 100f32f0, CMS library', '`FUN_01007290`, `FUN_01004f54`',
     'Same game-side voice logic; mixing handed to the Sound Manager (6 channels, `initNoInterp`), exact Hz (no PAL -1.3 %), continuous loops, 16-bit mixing',
     'platform artefact', 'keep', 'CONFIRMED', 'audio §1.1-1.3')
diff('Audio', '`DoAudio`, `ClearPreference` @ 100508c0', 'Paula volume FX*63/100',
     'Per-voice level `FX*A*65536/1e7` (0.653 at FX 100) with default FX 50 (= Amiga at 100)', 'Mac change', 'adopt', 'CONFIRMED', 'audio §1.5')
diff('Audio', '`DoAudio` group volumes', 'Paula group volume 31',
     'Quiet-voice doubling kept but the compensating group volume never applied: quiet voices +6 dB', 'platform artefact', 'ignore', 'CONFIRMED', 'audio §1.5')
diff('Audio', '`DoAudio` slot clock, `CMSQuietSound` @ 10155b10', 'same slot clock, chunked Paula output',
     'One-shots silenced after ~65 % of their length at 30 fps (slot clock runs (735t+399)/735t too fast)', 'Mac change', 'decide', 'CONFIRMED', 'audio §1.9')
diff('Audio', '`CMSPlayPaybackSFX` @ 10156a50', 'voice resumes at slot position',
     'Re-selected or swapped voices restart from the sample start; loops re-queue with a possible gap', 'platform artefact', 'ignore', 'CONFIRMED', 'audio §1.8')
diff('Audio', '`LoadSound` @ 100f1b40', '`FUN_01005748` (754 bytes)',
     'CarHorn loaded at its true 734 bytes (Amiga read 20 bytes past the end)', 'bug fix', 'done', 'CONFIRMED', 'audio §1.7')
diff('Audio', '`PlaySound` @ 100f1960', '`FUN_010053c4` (PAL 3,546,895)',
     'Menu sounds use the NTSC clock 3,579,545/period, volume FX*256/100, mixed with game voices', 'Mac change', 'adopt', 'CONFIRMED', 'audio §1.10')
diff('Audio', '`ShowOptions` @ 10093240 (`PauseAllSounds`)', '`FUN_0101a7dc` (Paula stopped)',
     'Pause menu pauses and resumes game voices', 'Mac change', 'adopt', 'CONFIRMED', 'audio §1.11')
diff('Music', '`SetSoundVolume` @ 100f24f0', '`FUN_01006564`',
     'One slider: SFX linear; CD music 200/256 up to FX 50, fading to silence at 100', 'Mac change', 'adopt', 'CONFIRMED', 'audio §1.6')
diff('Music', '`CD_SetVolume` @ 100fb100 (empty)', 'OS4: no music volume',
     'No "under cover" music duck (stub)', 'Mac change', 'adopt', 'CONFIRMED', 'audio §2.2')
diff('Music', '`UCDAudio` @ 101038f0, `CD_CheckForTrackEnd` @ 100fb000', 'CDDA thread / CD_Audio_Handler',
     'CD tracks played as QuickTime movies from the cddafs volume; continuous mode sequential with no random start and no wrap; default mode Radio', 'Mac change', 'adopt', 'MIXED', 'audio §2, frontend §10')
diff('Music', '`UCDAudio::Play`/`Next`', '-',
     'Continuous mode advances two catalogue entries per track (every other track), `n+1` index', 'platform artefact', 'ignore', 'INFERRED', 'audio §2.2')
diff('Intro', '`OpenAnimation` @ 100fb130, `PlayAnimation` @ 100fb1d0', '68k `FUN_00010364` (FLC)',
     'New 72 s MPEG-1 640x480 25 fps film (MP2 48 kHz), hard-coded `payback_high_mpeg1.mpg`, fit to window keeping 4:3, any key/click skips', 'new feature', 'adopt', 'CONFIRMED', 'audio §3, frontend §9, assets §10')
diff('Intro', '`PlayAnimation`', '-',
     'Skip polls held keys; 1-px black inset at 640 wide', 'platform artefact', 'ignore', 'MIXED', 'audio §3.2')
# Input / front end
diff('Input', '`GetMsgFromKeyboard` @ 100fc280, key table 0', 'raw keys, 0 = unbound',
     'Mac virtual key codes; "unbound" is 0x7F (the Amiga\'s 0 is the backtick key)', 'bug fix', 'adopt', 'MIXED', 'audio §4.2')
diff('Input', '`ReadFromInputSource` @ 100f93d0', '`FUN_01040790`',
     'Rebinding refuses F1 (reserved for "Press F1 to continue") and allows P', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §3.2, audio §4.2')
diff('Input', '`UpdateInputStatus` @ 100f8490, HID', '`FUN_0103f3d0`, lowlevel/PSX/CD32',
     'PSX and CD32 removed; up to 4 HID gamepads, digital only; single player polls keyboard and Device 1 together', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §3.1, audio §4.2')
diff('Input', 'fixed-key jump table 0x1018f760', 'Amiga hotkeys',
     'Extra weapon-cycle aliases: F3/F10/keypad+ next, Fwd-Delete/F2/F9/keypad- previous', 'Mac change', 'adopt', 'CONFIRMED', 'audio §4.2')
diff('Input', '`UpdateInputStatusNoKeys` @ 100f8090', 'edge-triggered',
     'Repeat-last-message fires on every poll while held (no edge latch); can overflow the queue', 'Mac bug', 'ignore', 'CONFIRMED', 'frontend §3.4')
diff('Front end', '`CPaybackPreferences::EditPreferences` @ 10050c60', '`PaybackSetup` tool',
     'Native "Payback Setup" dialog (window/fullscreen, size, renderer on OS 9, rotating view, Gouraud, motion blur, FX); Option key brings it back; live apply from the menu', 'new feature', 'adopt', 'CONFIRMED', 'frontend §1.2')
diff('Front end', '`ALPreferences`, `ClearPreference` @ 100508c0', '`FUN_0104199c` (`S:Payback.config`)',
     'Preferences in a QuickTime atom container; new defaults: music Radio, volume 50, P1 Keyboard, P2 Device 1, key set 3, FX 2', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §2.1')
diff('Front end', '`CPaybackApp::PrepareToRun` ~1004ee00', 'none',
     'Refuses to launch unless the "PaybackMac" CD is mounted', 'platform artefact', 'ignore', 'CONFIRMED', 'frontend §1.1')
diff('Front end', 'MBAR/MENU, `ShowAboutBox` @ 1004ea30, `amiga_cleanup`', 'main-menu Quit',
     'Menu bar with About (Alex Curylo), Quit, Payback Setup (Cmd-;); settings saved on quit', 'platform artefact', 'adopt', 'CONFIRMED', 'frontend §1.4 (adopt save-on-quit only)')
diff('Front end', '`amiga_init` @ 100ffc70', 'keyfile check in `FUN_010692bc`',
     'No keyfile: always registered; demo screens unreachable', 'Mac change', 'done', 'CONFIRMED', 'frontend §11')
diff('Front end', '`DoControlSetup` @ 100a1f80', '`FUN_0102e75c`',
     '5 columns (Keyboard, Device 1-4) instead of 11; spinning `controlsetup` 3-D model behind the screen', 'new feature', 'adopt', 'CONFIRMED', 'frontend §3.2, §14.6')
diff('Front end', '`DoCDAudioSetup` @ 100a1500', '`FUN_0102dd30`',
     'Renamed "Music setup", indented labels, spinning `cd` model behind; device/unit prompts ignored', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §4.4')
diff('Front end', '`DoTitleScreen` @ 100a2e70', '`FUN_0102fc44` (OS4 refuses View intro)',
     '"View intro" plays the QuickTime film and stops CD music unless music is disabled', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §4.1')
diff('Front end', '`DoRampageSetup` @ 1009ebc0, phrase 2055', '`FUN_0102a428`',
     '"Police aggressiveness:" renamed "Difficulty:"', 'data change', 'adopt', 'CONFIRMED', 'frontend §4.6')
diff('Front end', '`DisplayTextAlpha3` @ 1008ee50 / `DrawIconAlpha3`', '`FUN_0100cc78`',
     'New half-strength lighten used only for dimmed menu items', 'Mac change', 'optional', 'CONFIRMED', 'frontend §5.3')
diff('Replays', '`LoadMovie` @ 100fa7b0, `SaveMovie` @ 100fab30', '`FUN_010bc000` (0x3F2), 68k v10, CD v3',
     'Version 0x3F3: variable-length map path, then level, then u32 byte count; records unchanged; older versions rejected', 'Mac change', 'adopt', 'CONFIRMED', 'frontend §7.2')
diff('Phrases', '`Phrases/*.phrases`', '15 Amiga languages',
     'Only English, Deutsch, Francais ship (Finder index proves the rest existed)', 'platform artefact', 'ignore', 'CONFIRMED', 'frontend §6.1, assets §8')
diff('Phrases', '`Global.English.phrases`', '498 entries',
     '657 entries: +205, -46, ~131-136 changed (Difficulty, Music setup, credits, demo nag, random-mission texts 1400-1470/1800-1838, unused editor/key-name strings)', 'data change', 'adopt', 'CONFIRMED', 'frontend §6.3, assets §8')
diff('Phrases', 'level phrase files 00/11/21', 'Amiga English level files',
     'Control-neutral hints, "an RC" typo fix, "fecking"', 'bug fix', 'adopt', 'CONFIRMED', 'frontend §6.3, assets §8')
diff('Phrases', 'credits 2205-2281', 'credits to 2326',
     'Adds "Mac conversion / Alex Curylo"; 2002 Apex Designs / Virtual Programming copyright; localisation list trimmed', 'data change', 'adopt', 'CONFIRMED', 'frontend §6.3')
diff('Platform', '`amiga_init` ... `amiga_cleanup`', 'one function (`FUN_010692bc`)',
     'Main loop split into slices so PowerPlant\'s event loop can drive it; per-object update split into `object_MainLoop`/`do_Person`/`do_Other`/`do_Weapon`', 'platform artefact', 'ignore', 'CONFIRMED', 'symbols "New code"')
diff('Platform', 'struct layout', '68k cube 0x72 bytes',
     'Map cube 0x78 bytes, objects larger (+0x4e2..), models +0x385c: padding only', 'platform artefact', 'ignore', 'CONFIRMED', 'gameplay §3.3')
diff('Platform', 'no OpenTransport / NetSprocket imports', 'no network either',
     'No network play; same 2-4 player split screen', 'platform artefact', 'ignore', 'CONFIRMED', 'frontend §13')
diff('Platform', '`MapEditor (OS X)`, `Map Editor (OS 9)`', 'Amiga map editor',
     'Same key set; texture import via QuickTime formats instead of IFF', 'platform artefact', 'ignore', 'CONFIRMED', 'frontend §14.5')


# ----------------------------------------------------------------------------------------
# User decisions (2026-10-08, docs/re/mac_version.md "Decisions"): override the
# recommendation of matching rows. Key: substring of `what`.
# ----------------------------------------------------------------------------------------
DECIDED = {
    'Vert (slope 2) height bug': ('keep', 'Not reproduced: the port keeps the correct heights (quirk documented).'),
    'Native "Payback Setup" dialog': ('ignore', 'Not implemented: its options belong in the in-game Graphics menu.'),
    'No `Blood.dark`': ('optional', 'Setting: Mac or Amiga blood. Default Mac.'),
    'German language loads `EffectTextures/Gas`': ('optional', 'Setting, default off.'),
    'PC tower/monitor/keyboard replace the Amiga': ('optional', 'Setting: Mac or Amiga menu props. Default Mac.'),
    'Higher-poly or remodelled menu props': ('optional', 'Part of the menu-props setting. Default Mac.'),
    'New title art': ('optional', 'Setting: Mac or Amiga title screens. Default Mac.'),
    'New 72 s MPEG-1': ('optional', 'Setting: Mac film or Amiga FLC. Default Mac when present; decoded at run time with pl_mpeg.'),
    'Lower crowns (242 to 202)': ('optional', 'Setting: Mac or Amiga scenery heights. Default Mac.'),
    'Shifted -18..-20 Z so the body bottom': ('adopt', 'Adopt after checking the vehicle placement code.'),
    'One-shots silenced after ~65 %': ('optional', 'Setting, default off (play to the end).'),
    'Quiet-voice doubling': ('ignore', 'Ignored as an artefact.'),
    'no random start and no wrap': ('adopt', 'Stop after the last track, like the Mac.'),
    '8 MiB Gouraud table shipped': ('ignore', 'Compute from the palette; ignore the stale file.'),
}
for _x in D:
    for _k, (_rec, _note) in DECIDED.items():
        if _k in _x['what']:
            _x['rec'] = _rec
            _x['src'] = _x['src'] + ' | Decision: ' + _note

# ----------------------------------------------------------------------------------------
# Port fixes (port departs from BOTH originals)
# priority, area, fix, files, source
# ----------------------------------------------------------------------------------------
PF = [
    ('high', 'Lighting', 'Paired shadow rays for corners 1-3 and 5-7: shadow only if both rays (corner and half-square point) hit (L1). The port casts the second ray only, so it over-shadows NE/SW/SE corners on every map.', '`lighting.rs` `compute`, `Voxels::shadowed`', 'rendering §3.2'),
    ('high', 'RNG', 'Second RNG stream (`random3`) for effects, rendering and HUD wobble, so cosmetic draws stop shifting the gameplay sequence.', '`sim/mod.rs` `Rng`; `effects.rs`, `weather.rs`, `objects.rs`', 'gameplay §2, part2 §11.1'),
    ('high', 'Audio', 'Pan: float `r = dx/|dy|` and far side `min(A_raw/(|r|+1), 255)` with the unclamped amplitude (port uses integer r and clamps A first).', '`audio.rs` `pan`', 'audio §1.5'),
    ('high', 'Audio', 'Quantise every sound to 200 Hz steps (rate byte), not only Doppler sounds after their first frame.', '`audio.rs`', 'audio §1.4'),
    ('high', 'Vehicles', 'Heading deflection of +/-0x5a on wall hits that oppose the velocity (N7).', '`sim/collide.rs`, `sim/driving.rs`', 'gameplay §5.2, part2 §11.2'),
    ('high', 'Vehicles', 'Slope pitch and downhill speed `speed += pitch*ticks >> 8` (`PositionUnit`).', '`sim/driving.rs`', 'part2 §11.3'),
    ('high', 'Combat', 'Explosion damage `q/k` for people and vehicles, `(q>>4)/k` only for class-2 props, no push for class 2 (expect more chain reactions).', '`sim/combat.rs` `explosions()` ~1075', 'part2 §11.4'),
    ('high', 'Objects', 'Object shadows: people `Shad` sprite (T1), vehicle 3x3 `shadow` mesh, tree `DrawSceneryShadow`; offset K 0x200 cars, 0x80 people and bikes.', '`objects.rs`', 'rendering §6.1'),
    ('high', 'User maps', '`<map>.textures` loader for texture numbers >= 2000; 11-13 of the Mac user maps need it (a re-run of `usermaps_check.py` finds 12 maps with custom slots).', '`payback-formats` map/texture, `data.rs`, `frontend/usermaps.rs`', 'frontend §8.2, assets §9'),
    ('medium', 'Combat', 'Explosion score `min(6000, (q/T)/(k>>5)) x multiplier` for player-owned blasts.', '`sim/combat.rs`, `sim/score.rs`', 'part2 §11.5'),
    ('medium', 'Combat', 'Firearms crime counter: count shotgun, bazooka, grenade, bomb and tank cannon.', '`sim/combat.rs`, `sim/police.rs`', 'part2 §11.6'),
    ('medium', 'Police', 'Cop arrest distance to the target\'s door point without lead or cop radius; add the "player stunned" test.', '`sim/police.rs` `cops()`', 'part2 §11.7'),
    ('medium', 'Score', 'Copkiller bonus `100000 x multiplier`, not a flat 100000.', '`sim/score.rs`', 'part2 §11.8'),
    ('medium', 'Combat', 'Shotgun pellets: 3 sparks of size `rand(150)+250`.', '`sim/combat.rs`, `effects.rs`', 'part2 §11.9'),
    ('medium', 'Boarding', 'D1-D7: hijack resets WantedTimer; state-5 car with a driver; exit offset `(maxX+192)*6/5`; target pick extra -192; waypoint 5 walk-in with circle arrival; instant boat boarding; no boarding while burning; helicopter 3-press cycle.', '`sim/boarding.rs`', 'part2 §11.10-16'),
    ('medium', 'Collisions', 'Face type 5 must not block; `level_at` ORs layer bits; `resolve_person` push rules; glass breaks for class != 1 (sound 0x31); scenery circle collision (`AddCollideObj`); contact damage base incl. z; Irony bonus (phrase 2027, 10000).', '`sim/collide.rs`, `sim/contact.rs`', 'part2 §11.17-23'),
    ('medium', 'Objects', 'Shine modes 0x10/0x13/0x15 colour-keyed (7,8) in hardware, 0x34 via table 4; mode-5 lines from slot 0 (black when burnt); scenery mode 0x14 table 0 in `DrawScenery`; sand recolour; hardware mode 3 soft blend; debris (`DrawPart`).', '`objects.rs`, `effects.rs`', 'rendering §6.1'),
    ('medium', 'Lighting', 'Muzzle-flash dynamic lights at z-500: minigun (60M,60M,8M), pistol (40M,40M,10M), shotgun (60M,60M,20M).', '`lighting.rs` `collect_lights`', 'rendering §6.2'),
    ('medium', 'Weather', '`Rain` loop volume 0x7f when the visible-layer limit is not 3; hardware rain (7,8) blend at alpha 0.4, not additive.', '`weather.rs`, `effects.rs`', 'rendering §6.2'),
    ('medium', 'HUD', 'Hi-res HUD layout at >= 640x400 (both builds switch at W > 639 && H > 399): window/2 canvas, hi-res offsets, icon doubling through Alpha table 4 in software, bilinear in hardware.', '`hud/mod.rs`, `hud/icons.rs`, `hud/paint.rs`', 'rendering §7, frontend §5.1'),
    ('medium', 'Front end', 'Loading box (`Loading` 94x35) drawn at native size, never doubled.', '`frontend/progress.rs`, `startup.rs`', 'rendering §7, frontend §5.6'),
    ('medium', 'People', 'Fleeing peds: throttle only when the angle error is < 200 (the port accelerates while turning).', '`sim/people.rs` `ped_ai`', 'people.md §3'),
    ('medium', 'People', 'Initial wander target T = spawn heading (port starts at -1); moot if the Mac wander is ported.', '`sim/people.rs` `Person.wander`', 'people.md §3'),
    ('medium', 'People', 'Render people with a per-model offset (max Y: 92, PedMale4 100, MaleNormal 90) instead of a fixed `90 - y`; apply the same shift before the lying +1500 (or load the Mac `.Obj` files).', '`sim/people.rs` render helpers (~l.1071)', 'people.md M3'),
    ('low', 'People', 'Footsteps also when walking backwards (check frames 0/6 in the t < 0 branch).', '`sim/people.rs` `animate_people`', 'people.md §3'),
    ('low', 'People', 'Order: the original moves with last frame\'s velocity, then computes the new one (one tick more latency than the port).', '`sim/people.rs` `move_people`', 'people.md §3'),
    ('low', 'HUD', 'Do not load the `Dollar`/`Percent`/`Pound` glyph files (no executable does; "£" renders as "?").', '`hud/icons.rs` `NAMED`', 'frontend §5.3'),
    ('low', 'HUD', 'Banner centring with `realstrlen` (skip `^`), not byte length.', '`hud/mod.rs`', 'frontend §5.3'),
    ('low', 'Collisions', 'Minor `floor()` details.', '`sim/collide.rs`', 'part2 §11.23'),
]

# ----------------------------------------------------------------------------------------
# Adopt-from-Mac implementation plan
# phase, priority, item, touches, determinism note, source
# ----------------------------------------------------------------------------------------
AP = [
    ('1. Data and render only', 1, 'Mac data root: optional second root (`--mac-data`, `PAYBACK_MAC_DATA`, `extracted/mac/Payback`) read first; `.txt` texture-list names; zero-pad short chunky files (TentRoof); case-insensitive names; skip `.DS_Store`/`.FBC*`; palette still from the Amiga `Payback.exe`.', '`data.rs` `GameData`, `payback-formats` texture loader', 'None (asset loading only).', 'assets §14'),
    ('1. Data and render only', 2, 'Prefer Mac cube textures (as a whole set; "Amiga look" setting), title screens (setting), object textures (`tanne`), `Maps/2.thumb`, Mac `Blood` with no `Blood.dark` night swap (setting: Mac or Amiga blood; keep the swap with Amiga-only data).', '`data.rs`, `effects.rs` (~l.1294), `frontend/assets.rs`', 'None.', 'rendering §8, assets §1-7'),
    ('1. Data and render only', 3, 'Alpha tables computed with the exact `CalcAlpha` formulas (T3 lighten, T4 diagonal shortcut) or read from the Mac `Alpha.256`.', '`postfx.rs` `alpha_tables`, `night_remap`', 'None.', 'rendering §4'),
    ('1. Data and render only', 4, 'Render-only RNG (`random3`) and the Mac effect changes: per-chimney per-tick smoke, weather spawn, effect argument order, lightning sets the flash, snow at street level, view-sized weather rectangle.', '`sim/mod.rs` (second index), `objects.rs` `chimney_smoke`, `weather.rs`, `effects.rs`, `lighting.rs`', 'Moving cosmetic draws off the sim `Rng` changes the sim sequence once: re-record replay baselines in the same change. After that the render RNG must never feed the sim.', 'rendering §6.3, gameplay §2'),
    ('1. Data and render only', 5, 'Motion blur: clear the previous-frame buffer when blur is switched on.', '`postfx.rs`', 'None.', 'rendering §4 (B1)'),
    ('1. Data and render only', 6, 'Presentation: 640x480 preset first (then 800x600, 1024x768), hi-res HUD as default, fixed 64 deg HFOV ("original aspect") as default; lo-res HUD and free aspect as options.', '`frontend/display.rs`, `camera.rs` `view_fov`, `hud/`', 'None.', 'rendering §7, frontend §1.2'),
    ('1. Data and render only', 7, 'Audio: Mac gain law with FX default 50; menu sounds at the NTSC clock, mixed with game voices; pause/resume voices in the pause menu; single slider with the music crossfade; one-shot slot-clock cut as a setting (default off); CD continuous mode stops after the last track.', '`audio.rs`, `frontend/ui.rs`, `frontend/menus.rs`', 'Audio state only; drive the slot clock from recorded ticks so replays sound the same.', 'audio §1, §6'),
    ('1. Data and render only', 8, 'Front end: Mac defaults (music Radio, volume 50, P1 Keyboard, P2 Gamepad 1, FX 2, Gouraud on); "Music setup"/"Difficulty:" texts; rotating `controlsetup` and `cd` models; refuse F1 / allow P; devices Off/Gamepad 1-4/Keyboard; keyboard + pad 1 together; intro stops music unless Disabled; save settings on quit; settings apply live.', '`frontend/*.rs`, `frontend/bindings.rs`, `frontend/progress.rs`', 'Input mapping only; replays record inputs, so unaffected.', 'frontend §15'),
    ('1. Data and render only', 9, 'Mac phrases as the English/Deutsch/Francais baseline (global, level, credits); accept `Francais` and `Français`; keep the 11 other Amiga languages.', '`frontend/mod.rs`, `hud/mod.rs` phrase loading', 'None.', 'frontend §6, assets §8'),
    ('1. Data and render only', 10, 'German `Gas` in the blood slot when the language is Deutsch, as a setting (default off); reload on language change.', '`effects.rs`, `frontend/menus.rs`', 'None.', 'frontend §14.1'),
    ('2. Simulation (re-record baselines)', 11, '30 Hz tick: `Time::<Fixed>::from_hz(30.0)`, camera smoothing, replay tick units, `END_DELAY` 4.0 to 5.0 s (150 ticks); update `docs/re/people.md` §1.1.', '`main.rs` l.79, `camera.rs`, `replay.rs`, `frontend/mod.rs` l.66', 'Per-tick rules are unchanged, so simulated results per tick stay identical; only wall-clock speed changes. Re-record the replay baselines (`check.sh <name> <ticks> --map 0 --seed 1234` must report MATCH afterwards).', 'gameplay §1.4, audio §4.1'),
    ('2. Simulation (re-record baselines)', 12, 'Vehicle changes V1-V7: pursuit radius, junction look-ahead, low-skill steering, traffic spawn speed, tank reverse -220, map-edge wall hit, light-bar reset.', '`sim/vehicle_ai.rs`, `sim/pursuit.rs`, `sim/traffic.rs`, `vehicles.rs` l.217, `sim/collide.rs`, `sim/driving.rs`', 'Changes the sim: re-record baselines (one batch with item 11 is cheapest).', 'gameplay §4'),
    ('2. Simulation (re-record baselines)', 12.5, 'Pedestrians: Mac pavement wander (per-person re-aim timer +0x900 and anchor; anchor set in `spawn_peds`, (-1,-1) elsewhere), on-foot velocity smoothing `(v + (sin*speed>>7))/3` so contact pushes persist, punch crate bounds.', '`sim/people.rs` `ped_ai`, `move_people`, `Person`; `sim/contact.rs`; `docs/re/people.md` §7.3', 'Changes the sim RNG call pattern (`rand(50)`, `rand(1000)`) and movement: re-record replay baselines.', 'people.md M1, M2, M4'),
    ('2. Simulation (re-record baselines)', 13, 'Destroyed remote bomb detonates at 50000 and clears the owner\'s bomb slot (C1).', '`sim/combat.rs`, `sim/people.rs`', 'Sim change: re-record.', 'gameplay §5.1'),
    ('2. Simulation (re-record baselines)', 14, 'Level 21 "car 50 destroyed" trigger and failure (M1); trigger actor binding by current script id (M2).', '`mission/levels/level_21.rs`, `mission/api.rs`, `mission/eval.rs`; `docs/re/missions_21.md`', 'Level-21 replays and any replay with released actors change.', 'gameplay_part2 §6'),
    ('3. New features', 15, 'Mac model set with matching placement: people origin at the feet (per-model offsets; Mac render code unchanged), vehicles bottom at z = 0 after a placement check, door quads; settings for Mac/Amiga menu props and scenery heights (phones, traffic lights, trees).', '`objects.rs`, `world.rs`, people/vehicle placement code', 'Render only, unless any sim code reads model extents (check `+0x48c8` front extent etc.).', 'assets §4.2'),
    ('3. New features', 16, 'Random phone missions in Rampage (M4) with triggers in Rampage on maps with phones (M3), `GetSafePos`, `EstimateTime`, random phone, formatted say, Mac phrases 1400-1470 / 1800-1842.', 'new `mission/levels/random.rs`, `mission/mod.rs`, `mission/eval.rs`, `frontend/rampage.rs`, `rampage.rs`; a Rampage setup setting (default on) recorded in the replay header', 'Uses the game `random`: Rampage replays diverge from the current baselines; record new Rampage baselines.', 'gameplay_part2 §6, missions.md §3'),
    ('3. New features', 17, 'Replays: read and write version 0x3F3 (variable map path, level, u32 length); keep reading 3, 10, 0x3F2.', '`replay.rs`, `frontend/replays.rs`', 'Records unchanged: check.sh must still MATCH.', 'frontend §7.2'),
    ('3. New features', 18, 'Mac MPEG-1 intro as default when present, decoded at run time with vendored `pl_mpeg` (setting: Mac film or Amiga FLC; FLC fallback); skip on new key/click.', '`intro.rs`, `tools/extract/extract_mac.py`, Cargo deps', 'None.', 'audio §3, frontend §9'),
    ('3. New features', 19, 'Replay TV camera, when the rotating view lands.', '`camera.rs`', 'Render-only RNG required.', 'rendering §2.3'),
    ('3. New features', 20, 'Music module with the Mac modes (user-ripped tracks; menus 13, game over 16, complete 8; Radio per vehicle; Continuous sequential).', 'new music module, `audio.rs`, `frontend/menus.rs`', 'None.', 'audio §2'),
]

# ----------------------------------------------------------------------------------------
# Open questions and report disagreements
# ----------------------------------------------------------------------------------------
OPEN = [
    ('Continuous CD music wrap', 'The Mac `UCDAudio::Next` stops after the last audio track; the Amiga wraps (and picks a random first track). The no-wrap lives in OS X CD plumbing, so it may be an artefact.', 'Wrap like the Amiga, or stop like the Mac?', 'audio §2.2'),
    ('One-shot sound cut-off', 'The Mac silences one-shots after ~65 % of their length (game-code slot clock; e.g. MissionComplete cut at ~1.19 s of 1.84 s). The port plays them to the end.', 'Reproduce the cut (audio_platform recommends it), make it an option, or keep full playback?', 'audio §1.9'),
    ('Quiet-voice +6 dB', 'On the Mac, quiet voices play twice as loud when fewer than 4 loud voices play (lost Paula group volume).', 'Ignore as an artefact (recommended) or reproduce literally?', 'audio §1.5'),
    ('Mac game models and placement', 'Mac people have their origin at the feet (-90 Y), cars at the body bottom (-18..-20 Z). For people this is settled: the Mac `DrawObject` people transform is unchanged, so the data fixes "feet below floor" (people.md M3). For vehicles and doors the Mac placement code was not re-checked.', 'Adopt the Mac models (people with per-model offsets; vehicles after a placement check), or keep the Amiga models and only fix the people offset in code?', 'assets §4.2, people.md M3'),
    ('Mac textures as default vs optional', 'The Mac cube textures are a deliberate darker art pass with baked light pools (not higher resolution). All reports recommend them as the default when Mac data is present, with the Amiga set as fallback.', 'Default, or an "Amiga look" option only?', 'assets §1, rendering §8.1'),
    ('Mac MPEG intro', 'Needs an MPEG-1 + MP2 decoder (pl_mpeg bindings, a pure-Rust decoder, or an ffmpeg transcode at extraction time).', 'Which route: runtime dependency or extraction-time transcode?', 'audio §3, assets §10'),
    ('Re-export the Ghidra project with jump-table overrides', 'The current `analysis/ghidra_macx` export silently lacks most level-script, `SetTrigger_Long`, input and movie code (36 TOC jump tables). The recovery was done outside the repo.', 'Fold the SwitchOverride pass into `tools/ghidra/` and re-export now?', 'gameplay_part2 §6, missions.md §0'),
    ('Random missions: default or option', 'gameplay_part2.md says adopt (Mac behaviour); scratch missions.md says "implement as an option, since it exists only in the Mac release".', 'On by default in Rampage on maps with phones, or a setting?', 'part2 §6 M4, missions.md §3'),
    ('Gouraud.256: read or compute', 'rendering.md: the shipped file is stale (CD palette), ignore it and compute. assets.md: read it when present because that is what the Mac shows (~0.25 % of entries differ, all colour 8).', 'Compute (port today) or read the stale Mac file?', 'rendering §4, assets §5.2'),
    ('German "Gas" default', 'frontend: adopt with a switch (default on for Deutsch); assets: only if we want the regional variant.', 'Default on for Deutsch, or off?', 'frontend §14.1, assets §2'),
    ('Mac menu props and title art', 'PC tower/keyboard/gamepad/floppy props are platform branding; the title screens add the Virtual Programming logo.', 'Adopt the Mac props, keep the Amiga props, or offer both?', 'assets §4.1, §6'),
    ('Phrase 1799 in Rampage', 'Shown on a Rampage level end with trigger result 2; no phrase file has it. frontend calls it a Mac bug; gameplay leaves it unresolved.', 'Keep 2023 "Level complete!" (recommended)?', 'frontend §6.3, part2 §12'),
    ('Unchecked areas', '`do_Person` movement/animation (walk/run caps, swimming, frame advance, ped panic, stick handling) is now compared (people.md, see Gameplay). Still open: effect-size arguments past the 8th (hidden by the decompiler) and anything in the jump-table code missing from the export (`GetNextWeapon`, input, movie code).', 'Schedule a follow-up pass after the Ghidra re-export?', 'part2 §12, people.md'),
]

# Decisions on the open questions (2026-10-08), in OPEN order.
OPEN_DECIDED = [
    'Stop after the last track, like the Mac.',
    'Make it a setting (default off: play to the end); drive it from recorded ticks.',
    'Ignore as an artefact.',
    'Adopt the Mac models: people with per-model offsets; vehicles and doors after a placement check. Phone, traffic-light and tree heights also get a Mac/Amiga setting.',
    'Mac textures are the default; "Amiga look" is a setting.',
    'Judgement call: decode at run time with vendored `pl_mpeg` (MIT, one C file, built with `cc`); no ffmpeg needed; FLC fallback. Mac film / Amiga FLC is a setting.',
    'Judgement call: yes. Fold the SwitchOverride pass into `tools/ghidra/` and re-export `ghidra_macx` and `ghidra_mac9`; the Amiga exports stay as they are.',
    'A setting in the Rampage setup, default on, recorded in the replay header (it changes the simulation).',
    'Judgement call: compute from the loaded palette (port today); the shipped file was built from the stale CD palette, and the computation follows whichever palette is loaded.',
    'A setting, default off.',
    'Offer both: settings for menu props and title screens, default Mac.',
    'Keep phrase 2023 "Level complete!".',
    'Follow-up pass scheduled after the Ghidra re-export (7).',
]
assert len(OPEN_DECIDED) == len(OPEN)

DISAGREE = [
    ('Random-mission phrases', 'frontend.md §6.3 marks phrases 1400-1499 and 1800-1838 "not used by the Mac game" and says not to wire them up.',
     'gameplay_part2.md M4 and missions.md §3 show `ProcessTrigger_Random` using 1400-1470 and 1800-1838. The frontend phrase scan used the Ghidra export, which lacks the jump-table case bodies. **The gameplay reading is the correct one.**'),
    ('68k tick rate', 'frontend.md §14.3: "30 Hz logic, as on the Amiga".',
     'gameplay.md §1.1 and audio_platform.md §4.1: 68k Update 7 is 25 Hz (DateStamp/2), OS4 30 Hz. The frontend sentence is only true for OS4.'),
    ('Gouraud.256', 'rendering.md D2: keep computing, ignore the stale file.', 'assets.md §5.2: read the file when present (that is what the Mac shows).'),
    ('Alpha.256 table 3', 'rendering.md §4: the Mac file is exactly the `CalcAlpha` computation on all 11 tables, with T3 = (s*255 + (255 - max(sR,sG,sB))*d) >> 8.',
     'assets.md §5.1: T3 matches only ~96 % with the formula then documented in rendering.md, so it is "approximate". The new exact T3 from the Mac `CalcAlpha` resolves this; assets also found T4 65529/65536, which the T4 `s == d` shortcut explains.'),
    ('Gouraud.256 match', 'rendering.md: 99.78 % against the current palette.', 'assets.md: 99.75 % (20,000 random samples). Sampling difference.'),
    ('User maps with custom textures', 'frontend.md §8.2: 13 maps use custom textures.', 'assets.md §9: 11 maps. Re-running `usermaps_check.py` for this report shows 12 maps with custom slot references (max-custom-slot >= 0).'),
    ('Changed English global phrases', 'frontend.md: 136 changed (phrase_diff_english.txt header).', 'assets.md: "131 changed" (~). The diff file header says 136.'),
    ('Amiga language count', 'frontend.md §6.1: 14 Amiga languages.', 'assets.md §8: "the Amiga has 15". The list in frontend names 14.'),
    ('German Gas on the Amiga', 'rendering.md D7: "Gas replaces Blood ... as on the Amiga (INFERRED same)".', 'frontend.md §14.1: Amiga `FUN_0104feb0` always loads Blood; the swap is Mac-only.'),
    ('Random missions recommendation', 'gameplay_part2.md: adopt.', 'missions.md §3: implement as an option.'),
    ('Intro recommendation', 'rendering.md §7 table: intro "keep" (`intro.rs`).', 'frontend.md §9, assets.md §10, audio_platform.md §3: adopt the MPEG film as the default when present.'),
    ('"View intro" menu', 'frontend.md §4.1: main menu `DoTitleScreen`.', 'audio_platform.md §3.2: `DoGameMenu` option 7. Minor; both may offer it.'),
    ('Phrase 1799', 'frontend.md: a Mac bug, do not copy.', 'gameplay_part2.md §12: unresolved.'),
    ('`GetSafePos` callers', 'people.md M5: `GetSafePos` has no callers (dead code).', 'gameplay.md §5.1 / missions.md §3: used by the random missions. Its callers sit in the `ProcessTrigger_Random` jump-table bodies that the export lacks, so "no callers" reflects the export gap.'),
    ('Map-edge wall hit (V6) port impact', 'gameplay.md §4: adopt; `resolve_walls` must report a wall hit when the clamp moves a coordinate.', 'people.md M5: `collide::resolve_walls` already clamps and blocks off-map squares, nothing to do. Check whether the port reports the clamp as a hit (BounceUnit damage, sound, deflection).'),
    ('Ped spawn point (P1)', 'gameplay.md §5.1: reader not traced, no action.', 'people.md M1: it is the anchor of the new pavement wander. Resolved.'),
    ('`do_Person` movement', 'gameplay_part2.md §12: not compared line by line.', 'people.md: compared; differences M1-M4, the rest identical.'),
    ('docs/re/mac_version.md first-pass table', '"UserMaps: 163 Mac only", "Objects: 7 Mac only", "Icons: 3 Mac only".',
     'These are file counts that include Finder junk: 32 maps (31 playable), 2 new models, 0 new icons.'),
]

# ----------------------------------------------------------------------------------------
# Data diff (same logic as tools/re/mac_datadiff.py) and Finder info
# ----------------------------------------------------------------------------------------
JUNK = re.compile(r'(^|/)(\.ds_store|\.fbcindex|\.fbclockfolder(/.*)?|\.spotlight-v100.*|\.trashes.*)$')


def files(root):
    out = {}
    for dp, _, fn in os.walk(root):
        for f in fn:
            if f.endswith('.rsrc') or f.endswith('.info'):
                continue
            p = os.path.join(dp, f)
            out[os.path.relpath(p, root).replace(os.sep, '/').lower()] = p
    return out


def datadiff():
    mac = files(os.path.join(ROOT, 'extracted', 'mac', 'Payback'))
    ami = files(os.path.join(ROOT, 'extracted', 'game'))
    dirs = collections.defaultdict(lambda: [0, 0, 0, 0, 0])
    for r, p in mac.items():
        d = r.split('/')[0] if '/' in r else '.'
        if r in ami:
            same = open(p, 'rb').read() == open(ami[r], 'rb').read()
            dirs[d][0 if same else 1] += 1
        else:
            dirs[d][2] += 1
            if not JUNK.search(r):
                dirs[d][4] += 1
    for r in ami:
        if r not in mac:
            d = r.split('/')[0] if '/' in r else '.'
            dirs[d][3] += 1
    return dirs


def finder_info():
    p = os.path.join(ROOT, 'extracted', 'mac', '_finder_info.tsv')
    rows = [l.rstrip('\n').split('\t') for l in open(p, encoding='utf-8', errors='replace')][1:]
    c = collections.Counter()
    ex = {}
    for r in rows:
        if len(r) < 5:
            continue
        k = (r[1], r[2])
        c[k] += 1
        if k not in ex and r[1] != '????':
            ex[k] = r[0]
    return len(rows), c, ex


# ----------------------------------------------------------------------------------------
# Phrase diff (appendix)
# ----------------------------------------------------------------------------------------
def phrases():
    rel = 'out/mac/phrase_diff_english.txt'
    if not os.path.exists(os.path.join(ROOT, rel)):
        subprocess.run([sys.executable, os.path.join(ROOT, 'tools', 're', 'phrase_diff.py')], check=True)
    t = rd(rel)
    glob_part = t.split('\n==== 00.')[0]
    added, removed, changed = [], [], []
    cur = None
    for line in glob_part.split('\n'):
        m = re.match(r'^([+~-]) (\d+)$', line)
        if m:
            cur = (m.group(1), int(m.group(2)))
            continue
        m = re.match(r"^\s+M: (.*)$", line)
        if m and cur and cur[0] == '+':
            s = m.group(1)
            if s[:1] in '\'"':
                s = s[1:-1]
            added.append((cur[1], s))
        if cur and cur[0] == '-' and re.match(r'^\s+A:', line):
            removed.append(cur[1])
            cur = None
        if cur and cur[0] == '~' and re.match(r'^\s+M:', line):
            changed.append(cur[1])
            cur = None
    head = glob_part.split('\n')[1]
    return head, added, removed, changed


PHRASE_GROUPS = [
    (1400, 1499, 'Random phone missions (Rampage): briefs, results, "ANSWER THE PHONE", "NEW HIGH SCORE!"', 'used by `ProcessTrigger_Random`'),
    (1535, 1547, 'PC-style key names (Home, End, Insert, Page Up/Down, L/R-Ctrl, Pause, Menu, L/R-Start, Prnt Scrn)', 'unused'),
    (1800, 1839, 'Vehicle names with articles, compass areas of the city (`%s` arguments of 14xx)', 'used by the random missions'),
    (1840, 1842, 'City names alone (Freedom City, Los Francos City, Corona City)', '`GetJustCityName`, random-mission intro'),
    (1843, 1999, 'Map-editor UI, storage messages, "use mouse/stylus", "Lap %d of %d", "Gamma correction:"', 'unused (later Apex codebase?)'),
    (2000, 9999, '"Interference : Off/On" and others', 'unused'),
    (0, 1399, 'Other', '-'),
]


# ----------------------------------------------------------------------------------------
# User maps (appendix), straight from the extracted data
# ----------------------------------------------------------------------------------------
def usermaps():
    d = os.path.join(ROOT, 'extracted', 'mac', 'Payback', 'UserMaps')
    if not os.path.isdir(d):
        return []
    names = sorted({f[:-4] for f in os.listdir(d) if f.lower().endswith('.map')}, key=str.lower)
    allf = os.listdir(d)
    out = []
    for n in names:
        comps = sorted(f[len(n) + 1:].lower() for f in allf if f.lower().startswith(n.lower() + '.')
                       and f[len(n):len(n) + 1] == '.' and not f.endswith('.rsrc'))
        size = os.path.getsize(os.path.join(d, n + '.map'))
        tex = os.path.join(d, n + '.textures')
        tsz = os.path.getsize(tex) if os.path.exists(tex) else None
        out.append((n, comps, size, tsz))
    return out


# ----------------------------------------------------------------------------------------
# HTML building blocks
# ----------------------------------------------------------------------------------------
def table(headers, rows, cls='', tid=None, raw=False):
    t = [f'<table class="{cls}"' + (f' id="{tid}"' if tid else '') + '><thead><tr>']
    t += [f'<th>{h}</th>' for h in headers]
    t.append('</tr></thead><tbody>')
    for r in rows:
        attrs = ''
        if isinstance(r, dict):
            attrs = r.get('_attrs', '')
            r = r['cells']
        t.append(f'<tr{attrs}>' + ''.join(f'<td>{c if raw else il(str(c))}</td>' for c in r) + '</tr>')
    t.append('</tbody></table>')
    return ''.join(t)


def details(summary, body, open_=False):
    return f'<details{" open" if open_ else ""}><summary>{summary}</summary><div class="det">{body}</div></details>'


def diff_rows(filt=None):
    rows = []
    for x in D:
        if filt and not filt(x):
            continue
        rows.append({'_attrs': f' data-cls="{html.escape(x["cls"])}" data-rec="{html.escape(x["rec"])}"',
                     'cells': [html.escape(x['area']), il(x['mac']), il(x['amiga']), il(x['what']), chip(x['cls']),
                               chip(x['rec']), chip(x['conf']), html.escape(x['src'])]})
    return rows


DIFF_HEAD = ['Area', 'Mac function / file', 'Amiga counterpart', 'What differs', 'Classification',
             'Recommendation', 'Confidence', 'Source']


def diff_table(filt, tid=None):
    return table(DIFF_HEAD, diff_rows(filt), cls='data', tid=tid, raw=True)


def filter_bar(tid, with_selects=True):
    s = (f'<div class="filter"><input type="search" placeholder="Filter this table (all words must match)..." '
         f'data-target="{tid}" class="fbox">')
    if with_selects:
        s += (f'<select data-target="{tid}" data-attr="cls" class="fsel"><option value="">All classifications</option>'
              + ''.join(f'<option>{c}</option>' for c in ['bug fix', 'Mac change', 'new feature', 'platform artefact',
                                                           'data change', 'Mac bug'])
              + '</select>'
              f'<select data-target="{tid}" data-attr="rec" class="fsel"><option value="">All recommendations</option>'
              + ''.join(f'<option>{c}</option>' for c in ['adopt', 'optional', 'decide', 'keep', 'done', 'ignore'])
              + '</select>')
    s += f'<span class="fcount" data-target="{tid}"></span></div>'
    return s


def card(qid, title, verdict, body):
    return (f'<div class="card" id="{qid}"><h3>{title}</h3><p class="verdict">{verdict}</p>{body}</div>')


def ul(items):
    return '<ul>' + ''.join(f'<li>{il(i)}</li>' for i in items) + '</ul>'


# ----------------------------------------------------------------------------------------
# Content
# ----------------------------------------------------------------------------------------
def build():
    S = []  # (id, title, html, level)

    counts = collections.Counter(x['cls'] for x in D)
    recs = collections.Counter(x['rec'] for x in D)

    # ---------------- 1. Summary
    summary = f'''
<p>The Mac release of Payback (<b>Macintosh Edition 1.0, 30 October 2002</b>, port by Alex Curylo, published by
Virtual Programming) shipped on <code>PaybackMac.toast</code>, a classic HFS volume. It holds two PowerPC PEF
executables, <code>Payback (OS X)</code> (Carbon, OpenGL only) and <code>Payback (OS 9)</code> (Classic, OpenGL or
QuickDraw), the map editors, and the game data. It came out after Amiga Update 7, and this project treats it as
<b>more definitive</b> than the Amiga builds: where it differs, prefer it unless the difference is a Mac-platform
artefact.</p>
<p><b>How it relates to the Amiga builds.</b> The Mac game code is the <b>same C source as the Amiga OS4 build</b>
(Update 7 era), recompiled with CodeWarrior. The port emulates the Amiga APIs it used (<code>OpenScreenTags</code>,
<code>LoadRGB32</code>, <code>WritePixelArray8</code>, <code>MatchFirst</code>, <code>AllocMem</code> ...), reimplements
Warp3D on OpenGL, and replaces Paula, the CD thread, lowlevel input and the FLC player with the Sound Manager,
QuickTime, HID/InputSprocket and DrawSprocket. The OS X binary keeps 2248 of 2455 CodeWarrior function names, which
gave us real names for <b>648</b> Amiga functions (580 of 685 OS4 non-runtime functions).</p>
<div class="kpis">
 <div><b>{len(D)}</b><span>Mac differences catalogued</span></div>
 <div><b>{counts["bug fix"]}</b><span>bug fixes</span></div>
 <div><b>{counts["new feature"]}</b><span>new features</span></div>
 <div><b>{counts["Mac change"] + counts["data change"]}</b><span>Mac changes (code + data)</span></div>
 <div><b>{len(PF)}</b><span>port fixes (port vs both originals)</span></div>
 <div><b>{len(AP)}</b><span>adopt-from-Mac plan items</span></div>
</div>
<h3>Headline findings</h3>
{ul([
    '**No higher frame rate.** 30 Hz game tick (TickCount/2), at most 30 fps in game, 60 fps in menus, one render per simulation step, no interpolation, no vsync. The 68k Update 7 runs at 25 Hz; OS4 and Mac at 30 Hz.',
    '**Renderer and lighting are the OS4 code** almost line for line, in software and in hardware. Hardware mode is Warp3D emulated on OpenGL (bilinear, no mipmaps, painter’s order via a depth counter, per-view clipping).',
    '**Textures are not higher resolution.** Same 64x64 8-bit cube textures and palette; 225 of 876 were repainted (darker walls with a baked top-centre light pool, new Crowd/Wood/Fence/TentRoof art).',
    '**Models changed:** 49 changed + 2 new. People now have their origin at the feet, cars at the body bottom; trees, phones and traffic lights are shorter; menu props show a PC instead of an Amiga.',
    '**Sounds are byte-identical** (74 files). The engine is the same game-side logic on top of the Sound Manager: exact pitch and continuous loops, but also a +6 dB quiet-voice quirk and one-shots cut at ~65 %.',
    '**Pedestrians keep to the pavement** and on-foot velocity is smoothed (car pushes carry over): Mac AI changes in `do_Person`.',
    '**Real bug fixes:** map edge counts as a wall, destroyed remote bombs detonate, the level-21 soft-lock, stale trigger pointers, low-skill AI steering, quadratic junction radius, snow falling through streets, motion-blur first frame, Alpha.256 built from the wrong palette, CarHorn over-read.',
    '**New features:** random phone missions in Rampage, a native setup dialog, HID gamepads alongside the keyboard, a 72 s MPEG-1 intro film, 205 new English phrases, a replay "TV camera", replay format 0x3F3, rotating models on two more menu screens, 31 bundled user maps.',
    '**Mac bugs not to copy:** high scores never persist (`SaveHiscores`), repeat-message flood, phrase 1799, truncated `TentRoof.chunky`.',
    '**Our port also differs from both originals** in places the Mac comparison exposed: lighting shadow rays, audio pan and pitch steps, explosion damage, wall deflection, object shadows, the second RNG stream and more (' + str(len(PF)) + ' items).',
])}
'''
    S.append(('summary', 'Executive summary', summary, 2))

    # ---------------- 2. Questions answered
    q = []
    q.append(card('q-render', 'Rendering and lighting (CPU and hardware)',
                  'Same renderer and lighting as Amiga OS4; hardware mode is Warp3D emulated on OpenGL.',
                  ul([
                      '**CPU (software):** cube tops/sides, slopes, UVs and tiling, Alpha/Genlocked/Reflect faces, water, sky, projection, draw order and the off-map fill all match OS4 `FUN_01049374` and friends (rendering §2.1-2.2). Output is the 8-bit chunky buffer blitted 1:1 with `WritePixelArray8` to a QuickDraw GWorld / DrawSprocket CLUT.',
                      '**Lighting pre-pass:** `DoLightSourcing` @ 1015b840 = OS4 `FUN_010766c8`: same light tables (day/night, sw/hw), hardware jitter, voxel grid, lamps, dynamic lights and traffic-light beacons. Two facts apply to **both** builds and our port gets them wrong: paired shadow rays (L1) and the Vert height bug (L2).',
                      '**Hardware (OpenGL):** GOURAUD = `GL_SMOOTH` + `GL_MODULATE`; textures `GL_LINEAR`, no mipmaps, `GL_REPEAT`, power of two capped at 256; painter\'s order kept with a z counter; frame cleared each swap; a real per-view clipper; line/point smoothing and dither on. Night on the card is vertex colours only (confirms rendering.md §11.3 INFERRED). OS X is hardware-only by default; OS 9 offers QuickDraw.',
                      '**Mac-only render changes:** replay "TV camera" in the rotating view, chimney smoke on its own RNG per chimney, motion blur cleared on enable, snow dies at street level, weather rectangle sized to the view, lightning sets (not adds) the flash, weather spawn on the visual RNG.',
                  ])))
    q.append(card('q-bugs', 'Possible bug fixes in the Mac copy', f'{counts["bug fix"]} fixes found; several Mac bugs are new.',
                  ul([
                      '**Gameplay:** map edge counts as a wall (V6), destroyed remote bomb detonates and clears the owner slot (C1), level 21 soft-lock (M1), triggers bind by script id (M2), low-skill AI steers while braking (V3), junction look-ahead no longer quadratic (V2), light bar resets without a siren (V7), guard-sight null check.',
                      '**Rendering/data:** motion blur cleared when enabled, snow at street level, weather rectangle from the view size, per-view hardware clipping, `Alpha.256` rebuilt from the right palette, `2.thumb` re-rendered.',
                      '**Audio/input/text:** CarHorn length (734), exact pitch, continuous loops, unbound key 0x7F instead of the backtick key, typo "an RC", control-neutral hints.',
                      '**Not fixed (both builds):** Vert lighting heights, the last 32 pixels not motion-blurred, the `policevan_red` typo and every other documented quirk (gameplay_part2 §9).',
                      '**New Mac bugs:** high scores never persist, repeat-message flood, phrase 1799, truncated `TentRoof.chunky`, quiet voices +6 dB, one-shots cut at ~65 %.',
                  ])))
    q.append(card('q-physics', 'Physics and collisions', 'Unchanged except two collision fixes and some AI/table tweaks.',
                  ul([
                      'Identical: throttle/brake ramps, damage top-speed cap, J-turn, steering, lean, landing, splash/drowning, helicopter lift, `BounceUnit` damage, `SkidUnit` grip table, `PositionUnit`, `PositionAirborneUnit`, boarding, `GetCollideInfo`, `CheckForCollisions`, `DoCollideStuff`, `AddCollideObj`, combat, weapons, police, scoring.',
                      'Changed: **map edge = wall** on the full 3x3 sub-step (V6), **remote bomb** detonation (C1), pursuit arrival radius (V1), junction look-ahead (V2), low-skill steering (V3), traffic spawns moving (V4), **tank reverse -220** (V5).',
                      'Port gaps found on the way (both originals agree): wall heading deflection +/-0x5a (N7), slope pitch and downhill speed, explosion damage for vehicles, face type 5, layer OR, glass breaking, scenery circle collision.',
                      '**People on foot** (people.md): pedestrians now keep to the pavement (re-aim every 3 ticks, return to the last good pavement square) instead of "straight 50 ticks, turn 90-270 deg"; on-foot velocity is smoothed `(v + sin*speed>>7)/3`, so car pushes carry over; the punch crate check bounds negative squares. The rest of `do_Person` matches. The first two change the sim RNG sequence: re-record replays.',
                      'More people port gaps: fleeing peds accelerate while turning, initial wander target, no footsteps when walking backwards.',
                  ])))
    q.append(card('q-ui', 'How the UI was scaled at higher resolutions', 'Same rule as the Amiga: fixed 2x "hi-res" HUD, anchored to the edges, no further scaling.',
                  ul([
                      '`InitGraphics` @ 1008d110: W > 639 && H > 399 turns on hi-res (icons doubled, font pitch doubled). All three Mac sizes (640x480, 800x600, 1024x768) pass, so the Mac **always** shows the 2x HUD.',
                      'At 800x600 and 1024x768 nothing scales further: elements stay 2x and hang off left/top, xmax, ymax or cx, so the HUD gets relatively smaller. Offsets equal the Amiga hi-res table (e.g. score at xmax-145).',
                      'Software doubling blends each 2x2 block through Alpha table 4 (`LoadIcon`/`AverageVals`); OpenGL draws icons as 2x quads with `GL_LINEAR` (bilinear). The `Loading` box (94x35) is never doubled.',
                      'The 3-D view keeps the same angular framing: fx = (W+8)*16/320, fy = ((H+8)*16)>>8; at 640x480 that is the Amiga 320x256 look on a 4:3 screen (HFOV ~64 deg). Title screen: `TitleScreen.big` stretched, or `.w3d` under OpenGL.',
                  ])))
    q.append(card('q-fps', 'Was a higher FPS supported?', '<b>No.</b> 30 Hz tick, 30 fps cap in game, 60 fps menus.',
                  ul([
                      '`amiga_levelloop` @ 10100f50: `ticks = (TickCount() - last + 1) >> 1` (60 Hz Mac ticks halved), busy-wait until `last + ticks*2`, `ticks = clamp(1, 5)`; one simulation pass and one `DrawView` per frame.',
                      'No sub-tick timer (`Microseconds`/`UpTime` unused), no interpolation, no swap interval (`aglSetInteger` not imported), no frame-rate preference. A slow machine drops to 6 fps (5 ticks per frame) before the game slows.',
                      'Menus pace on `ch2pl` @ 10053320 at one Mac tick per frame = 60 fps.',
                      'The 735 samples per tick (22050/30) audio length suggests 30 Hz is the design rate (INFERRED); the 68k 25 Hz comes from the 50 Hz DateStamp.',
                  ])))
    q.append(card('q-tex', 'Textures: higher quality? Tiling and display?', 'Same size and palette; 225 repainted. Tiling unchanged.',
                  ul([
                      '**Resolution/format:** unchanged: 64x64 8-bit chunky cube textures, 256x128 object textures, same palette (proved by the regenerated `Alpha.256`). No texture is larger.',
                      '**Content:** 225 of 876 cube textures differ: ~170 building walls darkened with a baked top-centre "spotlight" (gain ~1.44 top centre, ~0.6 bottom corners), 37 Light groups collapsed onto the normal texture, Dark variants darker, 36 repainted (Crowd photo, Wood family, Fence, TentRoof, NewWall6/7, Uni/Fed brick toned down). 9.6-12.3 % of city faces use a retouched texture.',
                      '**Tiling/display:** lists byte-identical (renamed `.txt`), so flags, remaps and Gouraud substitution are the same. One copy per face, u/v 0..63, flip mirrors u, as on the Amiga. In OpenGL: `GL_REPEAT`, `GL_LINEAR`, no mipmaps, `RoundToExp2` to a power of two <= 256 (A1R5G5B5 to RGBA5551).',
                      '**Defect:** `TentRoof.chunky` is 4032 bytes (63 rows); pad to 4096.',
                  ])))
    q.append(card('q-models', 'Models: were any changed?', '<b>Yes:</b> 49 changed, 2 new (game models re-pivoted, menu props rebranded).',
                  ul([
                      '**People** (23 models): every vertex Y shifted by minus its max Y (90/92/100): origin moved from the hips to the feet. The Mac render code is unchanged, so this is a data fix for "feet below floor"; the port should use per-model offsets.',
                      '**Vehicles:** Pug, Pug Racer, Taxi, Vapour shifted -18..-20 Z (bottom at z = 0); Vapour uses `Cardef` paint instead of `Shine.All`.',
                      '**Scenery:** NewTree crowns lowered, phones 102 to 68 tall, traffic lights 228 to 186, `Tree.Obj` is a new `tanne` fir billboard; bus door quad moved up.',
                      '**Menu props:** PC tower/monitor/keyboard instead of the Amiga, gamepad joystick, "PAYBACK saved games" floppy; higher-poly silver cup, globe, question marks, film reel, upright CD.',
                      'For people the Mac placement code is confirmed unchanged (people.md M3); for vehicles it was not re-checked.',
                  ])))
    q.append(card('q-sound', 'Sounds: higher quality or a better engine?', 'Same samples; a slightly better but quirkier mixer.',
                  ul([
                      '**Samples:** all 74 files byte-identical (8-bit signed mono); nothing new; no `snd ` resources.',
                      '**Engine:** `DoAudio` is a line-for-line port of OS4 `FUN_01007290` (6 voices, same selection, pan, Doppler). Mixing moved to the Sound Manager: exact requested Hz (Amiga plays ~1.3 % flat), continuous loops (no 30 Hz chunk skipping), 16-bit mixing, still no interpolation (`initNoInterp`), no effects.',
                      '**Quirks:** quiet voices +6 dB (lost Paula group volume), one-shots silenced after ~65 % of their length, voices restart from the start when re-selected.',
                      '**Level/volume:** Mac gain law with default FX 50 (= Amiga at 100); one slider also crossfades CD music; menu sounds use the NTSC clock and mix with game voices; pause menu pauses voices.',
                      '**Music:** still Red Book CD audio (played as QuickTime movies from the cddafs volume); the Mac release ships no music data.',
                  ])))
    q.append(card('q-new', 'Additions and changes not in the Amiga version', f'{counts["new feature"]} new features plus many content changes.',
                  ul([
                      '**Random phone missions in Rampage** (`InitScript_Random`/`ProcessTrigger_Random`): 7 jobs (steal car, gang, checkpoints, kill cops, watchers, survival, points), scaled by police level 0-5, goal = top Rampage score; triggers now run in Rampage on maps with phones.',
                      '**Payback Setup dialog** (window/fullscreen, 640x480/800x600/1024x768, OpenGL/QuickDraw on OS 9, rotating view, Gouraud, motion blur, FX; Option key at launch; Cmd-; in game, live apply).',
                      '**HID input:** up to 4 gamepads via `HID.bundle` (InputSprocket on OS 9), digital only, any binding; single player uses keyboard and Device 1 together; PSX/CD32 removed.',
                      '**MPEG intro:** a different 72 s live-action/CG film, MPEG-1 640x480 25 fps with MP2 audio, instead of the 30 s in-engine FLC.',
                      '**205 new English phrases** (random missions, city/vehicle names, unused PC key names and map-editor strings), "Difficulty:", "Music setup", new credits.',
                      'Also: replay "TV camera", replay format 0x3F3, rotating `controlsetup` and `cd` models, 31 playable bundled user maps, German "Gas" blood, About box, always registered, CD-present launch check, new title art.',
                  ])))
    q.append(card('q-icon', 'The icon', '<b>Done</b> (commit 7077eee): the exe and window icon is the Mac app\'s icns 128 family.',
                  ul([
                      'Embedded by `crates/payback/build.rs` from `extracted/mac` (`payback_formats::icon::mac_app_icon`): 128/48/32/16 with 8-bit masks; falls back to the Amiga `Payback.info`.',
                  ]) + '<div class="iconrow">' + img('out/img/icon/exe_mac_icon_big.png', 'Mac app icon (used now)', 160, nearest=True)
                  + img('out/img/icon/exe_icon_big.png', 'previous Amiga-derived icon', 160, nearest=True) + '</div>'))
    q.append(card('q-else', 'Anything else that differs', 'Mostly platform plumbing and packaging.',
                  ul([
                      'Main loop split into `amiga_init` / `amiga_startlevel` / `amiga_levelloop` / ... for PowerPlant; per-object update split into `object_MainLoop`, `do_Person`, `do_Other`, `do_Weapon`.',
                      'Bigger structs (map cube 0x78 bytes, models +0x385c): padding only.',
                      'Preferences in a QuickTime atom container with new defaults (music Radio, volume 50, P1 Keyboard, P2 Device 1).',
                      'Only English, Deutsch and Francais ship; no keyfile; game refuses to start without the "PaybackMac" CD; no network play; replays and saves folders empty (Amiga demo replays not shipped).',
                      'Map editor imports QuickTime image formats. The CD root shows a mosaic of ~40 blank-named files whose custom icons spell a "payback" artwork.',
                      'Lots of Finder/Sherlock junk (`.DS_Store`, `.FBCIndex`, `.FBCLockFolder`) inflates the raw file-diff counts.',
                  ])))
    S.append(('questions', 'Questions answered', '<div class="cards">' + ''.join(q) + '</div>', 2))

    # ---------------- 3. Methodology
    meth = f'''
{ul([
    '**Extraction.** `tools/extract/extract_mac.py` (needs `machfs`) reads the HFS volume `PaybackMac.toast` into `extracted/mac/`: data forks under their HFS paths, resource forks as `<name>.rsrc`, type/creator codes in `extracted/mac/_finder_info.tsv`.',
    '**Decompile.** Ghidra headless in the `payback-re` Docker image with the language forced to `PowerPC:BE:32:default` (the PEF loader guesses VLE), exported with `ExportAll.java` to `analysis/ghidra_macx/` and `analysis/ghidra_mac9/` (`decompiled.c`, `functions.tsv`, `strings.tsv`), the same layout as the Amiga exports. PEF data and TOC (r2 = 0x10183440, data base 0x1017b440) are decoded by `scratch_render/pef.py`.',
    '**Known gap.** 36 TOC jump tables were not followed by Ghidra, so most level-script, `SetTrigger_Long`, input and movie code is missing from the export. The gameplay agent recovered them with SwitchOverride-style overrides in a temporary project outside the repo.',
    '**Name recovery.** Three pairwise matchers (Mac-OS4, OS4-68k, Mac-68k) using unique shared strings (TOC resolved), call-graph alignment, structural similarity and address order, then triangulation and manual anchors for the mission dispatchers. Result: 648 Mac functions matched (492 high, 98 medium, 58 low confidence); 306 of 343 OS4 and 84 of 101 68k addresses cited in `docs/` resolved.',
    '**Comparison.** Six research passes compared the Mac against OS4 and 68k and against our port: rendering, gameplay (two files), front end, assets and audio/platform. Data were compared byte for byte (`tools/re/mac_datadiff.py`, provenance against CD/U7/OS4), with renders of the differences. Tables like `InitStructures` were diffed field by field.',
    '**Policy.** The Mac is the more definitive release: default **adopt**, except platform artefacts (OpenGL/Carbon/QuickTime/Sound Manager plumbing) and clear Mac bugs.',
])}
<h3>Labels used in this report</h3>
<table class="data"><thead><tr><th>Label</th><th>Meaning</th></tr></thead><tbody>
<tr><td>{chip("CONFIRMED")}</td><td>Traced in code or bytes (Mac address and, where relevant, Amiga address given).</td></tr>
<tr><td>{chip("INFERRED")}</td><td>Consistent with code or data but not traced end to end.</td></tr>
<tr><td>{chip("MIXED")}</td><td>Part confirmed, part inferred (e.g. write confirmed, use inferred).</td></tr>
<tr><td>{chip("bug fix")} {chip("Mac change")} {chip("new feature")} {chip("data change")} {chip("platform artefact")} {chip("Mac bug")}</td><td>Classification of a difference.</td></tr>
<tr><td>{chip("adopt")} {chip("optional")} {chip("decide")} {chip("keep")} {chip("done")} {chip("ignore")}</td><td>Recommendation for our port. <i>decide</i> = the reports leave it to the user (see Open questions); <i>keep</i> = port already matches; <i>done</i> = already in the port.</td></tr>
</tbody></table>
<p>Addresses: Mac addresses are OS X PEF addresses (code at 0x10000000). <code>FUN_010xxxxx</code> are OS4, <code>FUN_000xxxxx</code> 68k Update 7.</p>
'''
    S.append(('method', 'Methodology', meth, 2))

    # ---------------- 4. Rendering
    r = REP['rendering']
    rend = f'''
<p>The Mac renderer is the OS4 C source; it differs in how the Amiga APIs are emulated and in the data.
The tables below are the Mac differences in this area; the full research note follows.</p>
<h3 id="r-sw">Software renderer and lighting</h3>
{diff_table(lambda x: x['area'] in ('Rendering', 'Lighting'))}
{details('What is the same as the Amiga (rendering.md §2.1)', md(section(r, '2.1 Same as the Amiga')))}
{details('Lighting details: paired rays and the Vert bug (rendering.md §3)', md(section(r, '3. Lighting'), shift=1))}
<h3 id="r-hw">Hardware mode: the OpenGL Warp3D layer</h3>
{md(section(r, '5. Hardware mode'), shift=1)}
<h3 id="r-fx">Effects, flares, weather</h3>
{diff_table(lambda x: x['area'] == 'Effects')}
{img('out/img/mac/effect_blood.png', 'Blood effect texture: Amiga Blood, Amiga Blood.dark, Mac Blood (smaller, darker; no Blood.dark)', 800, nearest=True)}
<h3 id="r-obj">Objects</h3>
{md(section(r, '6.1 Objects'), shift=1)}
<h3 id="r-ui">Screen setup, resolutions and UI scaling</h3>
{md(section(r, '7. Screen setup'))}
{details('Full research note: docs/re/mac/rendering.md', md(r, shift=2))}
'''
    S.append(('rendering', 'Rendering and lighting', rend, 2))

    # ---------------- 5. Gameplay
    g, g2 = REP['gameplay'], REP['gameplay_part2']
    game = f'''
<h3 id="g-timing">Timing and frame rate</h3>
{md(section(g, '1.1 The Mac frame loop'))}
{md(section(g, '1.2 Was a higher FPS'))}
<h3 id="g-diff">Gameplay differences</h3>
{diff_table(lambda x: x['area'] in ('Timing', 'Vehicle AI', 'Traffic', 'Tables', 'Collisions', 'Vehicles', 'Combat', 'People', 'Missions'))}
<h3 id="g-rng">Random number generator</h3>
{md(section(g, '2. Random number generator'))}
<h3 id="g-params">Parameter tables</h3>
{md(section(g, '3.1 Vehicle handling'))}
<h3 id="g-phys">Vehicle physics, AI and collisions</h3>
{details('Vehicle physics and AI (gameplay.md §4)', md(section(g, '4. Vehicle physics'), shift=1))}
{details('People, boarding and collisions (gameplay.md §5)', md(section(g, '5. People, boarding'), shift=1))}
<h3 id="g-people">People on foot</h3>
{md(section(PEOPLE, 'M1. Pedestrian wander'))}
{md(section(PEOPLE, 'M2. On-foot velocity'))}
{details('People models, punch fix, collisions, boarding (people.md M3-M6)', md(chr(10).join(section(PEOPLE, x, True) + chr(10) for x in ('M3. People models', 'M4. Punch crate', 'M5. Collisions', 'M6. Boarding')), shift=1))}
{details('Quirks shared by all builds and port deviations (people.md §2-3)', md(section(PEOPLE, '2. Quirks', True) + chr(10) + chr(10) + section(PEOPLE, '3. Port deviations', True), shift=1))}
<h3 id="g-missions">Missions, including the random phone missions</h3>
{md(section(g2, 'M4. Random missions'))}
{details('Random-mission job table (scratch_gameplay/missions.md §3)', md(section(MISSIONS, '3. NEW: Random missions')))}
{details('Level 21, trigger binding, Rampage triggers (gameplay_part2.md §6)', md(section(g2, '6. Mission engine'), shift=1))}
<h3 id="g-combat">Combat and police</h3>
{md(section(g2, '7. Combat, police'))}
<h3 id="g-quirks">Quirks the Mac did not fix (keep them)</h3>
{md(section(g2, '9. Documented quirks'))}
{details('Full research note: docs/re/mac/gameplay.md', md(g, shift=2))}
{details('Full research note: docs/re/mac/gameplay_part2.md', md(g2, shift=2))}
{details('Full sub-report: docs/re/mac/gameplay_people.md', md(PEOPLE, shift=2))}
'''
    S.append(('gameplay', 'Gameplay', game, 2))

    # ---------------- 6. Front end
    f = REP['frontend']
    fe = f'''
<h3 id="f-diff">Front-end, input and feature differences</h3>
{diff_table(lambda x: x['area'] in ('Front end', 'Input', 'Replays', 'Phrases'))}
<h3 id="f-setup">The Payback Setup dialog</h3>
{md(section(f, '1.2 "Payback Setup" dialog'))}
<h3 id="f-prefs">Preferences and defaults</h3>
{md(section(f, '2.1 Storage'))}
<h3 id="f-input">Input and controls</h3>
{md(section(f, '3.1 Key tables'))}
{details('Control setup, multiplayer devices, repeat message (frontend.md §3.2-3.6)', md((chr(10) * 2).join(section(f, s, True) for s in ('3.2 Control setup', '3.3 Multiplayer devices', '3.4 Repeat last message')), shift=1))}
<h3 id="f-hud">HUD and text</h3>
{md(section(f, '5.1 Layout and scaling'))}
<h3 id="f-saves">Saves, replays, high scores</h3>
{md(section(f, '7.2 Replays'))}
{md(section(f, '7.3 High scores'))}
{details('Full research note: docs/re/mac/frontend.md', md(f, shift=2))}
'''
    S.append(('frontend', 'Front end and features', fe, 2))

    # ---------------- 7. Assets
    a = REP['assets']
    asset = f'''
{md(section(a, '0. Summary'))}
<h3 id="a-tex">Cube textures</h3>
{md(section(a, '1.2 What changed in the pixels'))}
{img('out/img/mac/cubetex_highlights.png', '16 selected cube textures: Amiga | Mac | changed-texel mask (3x)', 1184)}
<div class="two">{img('out/img/mac/cubetex_gain_map.png', 'Mean Mac/Amiga gain map of the relit walls (bright = Mac brighter)', 256, nearest=True)}
{img('out/img/mac/cubetex_vignette.png', 'Average Mac/Amiga luma ratio and the "unchanged" mask: the baked light pool', 520)}</div>
{img('out/img/mac/cubetex_sheet_new.png', 'New Mac pictures (Crowd, Wood, NewWall6/7, Fence, TentRoof)', 616, nearest=True)}
{details('Top 24 most-changed cube textures', img('out/img/mac/cubetex_most_changed.png', 'Most changed cube textures (Amiga | Mac | mask)', 1200))}
{details('All 225 changed cube textures (large image)', img('out/img/mac/cubetex_all_changed.png', 'All 225 changed pairs at 1x', 1216))}
<h3 id="a-objtex">Object textures</h3>
{md(section(a, '3. ObjectTextures'))}
{img('out/img/mac/objecttextures_new.png', 'The 10 new Mac object textures', 900)}
<h3 id="a-models">Models</h3>
{md(section(a, '4.2 Game models'))}
{img('out/img/mac/objects_game.png', 'Game models, Amiga (left) vs Mac (right); blue line = model origin height', 1100)}
{md(section(a, '4.1 Menu props'))}
{details('Menu props render', img('out/img/mac/objects_options.png', 'Menu props, Amiga vs Mac', 1000))}
<h3 id="a-title">Title screens</h3>
{img('out/img/mac/icons_TitleScreen_big.png', 'TitleScreen.big (640x480): Amiga | Mac', 1100)}
{img('out/img/mac/icons_TitleScreen_w3d.png', 'TitleScreen.w3d (OpenGL, 256x256 RGB24): Amiga | Mac', 900)}
<h3 id="a-general">General files, maps, phrases</h3>
{md(section(a, '5. General'), shift=1)}
{img('out/img/mac/maps_2_thumb.png', 'Maps/2.thumb: Amiga | Mac (re-rendered)', 800)}
{md(section(a, '8. Phrases'), shift=1)}
<h3 id="a-usermaps">User maps</h3>
{img('out/img/mac/usermaps_thumbs.png', 'The bundled Mac user maps (thumbnails)', 1072)}
<p>Full list: Appendix D.</p>
<h3 id="a-other">Other</h3>
{img('out/img/mac/volume_root_icons.png', 'The CD root: ~40 blank-named files whose custom icons form a "payback" mosaic', 334)}
{details('Loading the Mac data (assets.md §14)', md(section(a, '14. Loading the Mac data')))}
{details('Full research note: docs/re/mac/assets.md', md(a, shift=2))}
'''
    S.append(('assets', 'Assets', asset, 2))

    # ---------------- 8. Audio and video
    au = REP['audio_platform']
    aud = f'''
{md(section(au, '0. Summary'))}
<h3 id="au-diff">Audio, music and intro differences</h3>
{diff_table(lambda x: x['area'] in ('Audio', 'Music', 'Intro'))}
<h3 id="au-engine">Is the Mac sound engine better?</h3>
{md(section(au, '1.3 Mixing rate'))}
<h3 id="au-music">Music</h3>
{md(section(au, '2.2 Track mapping'))}
<h3 id="au-intro">Intro video</h3>
{md(section(au, '3.1 Files'))}
{img('out/img/mac/video_mac_high_contact.png', 'Mac intro film, one frame every 3 s (payback_high_mpeg1.mpg)', 1200, fmt='JPEG')}
{md(section(au, '5. Bug fixes and new quirks'), shift=1)}
'''
    S.append(('audio', 'Audio and video', aud, 2))

    # ---------------- 9. Platform
    plat = f'''
{diff_table(lambda x: x['area'] in ('Platform', 'Hardware'))}
<h3 id="p-loop">Game loop, threads</h3>
{md(section(au, '4.1 Game loop'))}
<h3 id="p-input">Input</h3>
{details('Keyboard and gamepads (audio_platform.md §4.2)', md(section(au, '4.2 Input')))}
<h3 id="p-display">Display</h3>
{md(section(au, '4.3 Display'))}
<h3 id="p-shell">Start-up and the Mac shell</h3>
{md(section(f, '1.1 Start-up sequence'))}
{details('Full research note: docs/re/mac/audio_platform.md', md(au, shift=2))}
'''
    S.append(('platform', 'Platform layer', plat, 2))

    # ---------------- 10. Symbols
    sy = REP['symbols']
    KEY = ['amiga_levelloop', 'amiga_startlevel', 'DrawMap', 'DrawView', 'DoLightSourcing', 'CheckVoxels',
           'CheckForCubeCollision', 'CheckForCollisions', 'DoCollideStuff', 'MoveUnit', 'PositionUnit', 'SkidUnit',
           'BounceUnit', 'MoveObject_Vehicle', 'InitStructures', 'object_MainLoop', 'ReadInput', 'InitGraphics',
           'ReadScreenParams', 'DoAudio', 'AddSound', 'LoadMovie', 'CheckForTriggers', 'UpdateEffects',
           'GetCubeTexture', 'random3', 'CalcAlpha', 'GetGouraudTable', 'InitScript_Random', 'ProcessTrigger_Random']
    tsv = [l.rstrip('\n').split('\t') for l in open(os.path.join(ROOT, 'docs/xref/symbols.tsv'), encoding='utf-8')]
    hdr, rows = tsv[0], tsv[1:]
    by = {}
    for rrow in rows:
        nm = rrow[2].split('(')[0] if len(rrow) > 2 else ''
        if nm in KEY and nm not in by:
            by[nm] = rrow
    keyrows = []
    for k in KEY:
        rrow = by.get(k)
        if rrow:
            keyrows.append([f'`{rrow[2]}`', f'`{rrow[0]}`', f'`{rrow[3]}`' if rrow[3] else '-', f'`{rrow[4]}`' if rrow[4] else '-',
                            rrow[5] or '-', rrow[6] if len(rrow) > 6 else ''])
    ev = collections.Counter((rrow[6].split(':')[0] + ':' + rrow[6].split(':')[1].split('(')[0]) if rrow[6].startswith('mac-only') else 'matched'
                             for rrow in rows if len(rrow) > 6 and rrow[0])
    syms = f'''
{md(section(sy, 'Counts'))}
<h3 id="s-key">Key names</h3>
<p>A selection of names that matter for this report (full table in Appendix A, everything in <code>docs/xref/symbols.tsv</code>).</p>
{table(['Mac name', 'Mac addr', 'OS4', '68k', 'Conf.', 'Evidence'], keyrows, cls='data')}
<h3 id="s-tsv">symbols.tsv by kind</h3>
{table(['Kind', 'Rows'], sorted(ev.items(), key=lambda x: -x[1]), cls='data small')}
<h3 id="s-new">New code in the Mac build</h3>
{md(section(sy, 'New code in the Mac build'))}
<h3 id="s-removed">Amiga code missing from the Mac build</h3>
{md(section(sy, 'Amiga code missing from the Mac build'))}
<h3 id="s-docerr">Doc errors found</h3>
{md(section(sy, 'Corrections and doubts'))}
{md(section(g2, '10. Corrections to our notes'))}
<p>Further doc fixes from the rendering and front-end passes:</p>
{md(section(r, '10. Recommendations').split('12. **Doc fixes**')[1] if '12. **Doc fixes**' in r else '')}
{md(section(f, '15. Recommendations').split('**Doc fixes (not Mac-specific)**')[1])}
'''
    S.append(('symbols', 'Symbol cross-reference highlights', syms, 2))

    # ---------------- 11. Mac differences table
    ctab = ''.join(f'<li>{chip(k)} {v}</li>' for k, v in counts.most_common())
    rtab = ''.join(f'<li>{chip(k)} {v}</li>' for k, v in recs.most_common())
    dt = f'''
<p>Every difference between the Mac release and the Amiga builds found by the six research passes,
one row each. <b>{len(D)}</b> rows. Lighting rows L1/L2 are included because the Mac decompile settled them,
although they are the same in both builds (they are port fixes).</p>
<div class="two"><div><b>By classification</b><ul class="chips">{ctab}</ul></div><div><b>By recommendation</b><ul class="chips">{rtab}</ul></div></div>
{filter_bar('tbl-diff')}
{diff_table(None, tid='tbl-diff')}
'''
    S.append(('differences', 'Mac differences (consolidated)', dt, 2))

    # ---------------- 12. Port fixes
    pfrows = [{'_attrs': f' data-cls="" data-rec=""',
               'cells': [str(i + 1), chip(p), html.escape(ar), il(fx), il(fl), html.escape(src)]}
              for i, (p, ar, fx, fl, src) in enumerate(PF)]
    pf = f'''
<p>Places where <b>our port departs from both originals</b> (Mac and Amiga agree), found during the
Mac comparison. These apply whatever build is the reference. Ordered by priority. {len(PF)} items
({sum(1 for p in PF if p[0] == "high")} high, {sum(1 for p in PF if p[0] == "medium")} medium,
{sum(1 for p in PF if p[0] == "low")} low).</p>
{filter_bar('tbl-pf', False)}
{table(['#', 'Priority', 'Area', 'Fix', 'Files (crates/payback/src unless noted)', 'Source'], pfrows, cls='data', tid='tbl-pf', raw=True)}
<p class="note">Any simulation change here (vehicles, combat, boarding, collisions, RNG) needs the replay baselines
re-recorded: <code>tools/checks/replay_check.sh &lt;name&gt; &lt;ticks&gt; --map 0 --seed 1234</code> must report MATCH afterwards.</p>
'''
    S.append(('portfixes', 'Port fixes', pf, 2))

    # ---------------- 13. Adopt plan
    aprows = [{'_attrs': '', 'cells': [str(i + 1), html.escape(ph), il(it), il(tc), il(det), html.escape(src)]}
              for i, (ph, n, it, tc, det, src) in enumerate(sorted(AP, key=lambda x: x[1]))]
    ap = f'''
<p>What to adopt from the Mac, in order. Phase 1 changes only data, rendering, audio and the front end;
phase 2 changes the simulation and must be done together with re-recorded replay baselines; phase 3 adds features.</p>
<h3>Replay determinism rules</h3>
{ul([
    '**Render-only RNG.** The Mac draws all cosmetic randomness (`random3`: DrawMap 71 calls, UpdateEffects 64, start-level weather 12, text wobble ...) from a second counter over the same table. Give the port a second `Rng` index for effects, weather, chimneys, the TV camera and HUD wobble. Splitting it off changes the simulation sequence once (cosmetic draws stop consuming sim numbers): re-record the baselines in that change, then never let render code touch the sim `Rng`.',
    '**30 Hz tick.** The simulation consumes per-frame `ticks`, which replays store (bits 2..0). Per-tick rules are identical in all builds, so switching the fixed timestep to 30 Hz changes the wall-clock speed, not the per-tick results; but camera smoothing, `END_DELAY` and the tick-unit assumptions in `replay.rs` change, so re-record the replay baselines.',
    '**Simulation changes** (V1-V7, C1, M1, M2, pedestrian wander and velocity smoothing, random missions): batch them, re-record once, and keep `check.sh ... --map 0 --seed 1234` reporting MATCH.',
    '**Audio-only and data-only changes** (slot clock, gain law, textures, models without extents) do not touch the simulation.',
])}
{table(['#', 'Phase', 'Change', 'Touches', 'Determinism', 'Source'], aprows, cls='data', raw=True)}
'''
    S.append(('adopt', 'Adopt from Mac: implementation plan', ap, 2))

    # ---------------- 14. Open questions
    oq = ''.join(f'<div class="oq"><h4>{i + 1}. {html.escape(t)}</h4><p>{il(c)}</p><p class="ask"><b>Decision:</b> {il(qq)}</p>'
                 f'<p class="ask"><b>Decided:</b> {il(OPEN_DECIDED[i])}</p>'
                 f'<p class="src">Source: {html.escape(s)}</p></div>' for i, (t, c, qq, s) in enumerate(OPEN))
    dis = table(['Topic', 'One report says', 'Another says / resolution'],
                [[il(a_), il(b_), il(c_)] for a_, b_, c_ in DISAGREE], cls='data', raw=True)
    op = f'''
{oq}
<h3 id="disagreements">Where the reports disagree</h3>
<p>{len(DISAGREE)} disagreements between the research notes. Where one side is clearly right, it says so.</p>
{dis}
'''
    S.append(('open', 'Open questions and decisions', op, 2))

    # ---------------- 15. Appendices
    # A key names
    appA = details('Appendix A table: functions cited in docs/ with their Mac names (symbols.md)',
                   md(section(sy, 'Functions cited in `docs/`')))
    # B data diff
    dd = datadiff()
    ddrows = [[f'`{d}`', v[0], v[1], v[2], v[4], v[3]] for d, v in sorted(dd.items())]
    tot = [sum(v[i] for v in dd.values()) for i in range(5)]
    ddrows.append(['**total**', tot[0], tot[1], tot[2], tot[4], tot[3]])
    n_fi, fic, fiex = finder_info()
    firows = [[f'`{t}` / `{c}`', n, html.escape(fiex.get((t, c), '')) if (t, c) in fiex else '-'] for (t, c), n in fic.most_common()]
    appB = f'''
<p>Re-computed by this script with the logic of <code>tools/re/mac_datadiff.py</code> (Mac <code>extracted/mac/Payback</code>
vs merged Amiga <code>extracted/game</code>; resource forks and <code>.info</code> skipped). "Mac only (real)" drops
<code>.DS_Store</code>/<code>.FBC*</code>/Spotlight/Trashes junk.</p>
{table(['Directory', 'Same', 'Differ', 'Mac only (raw)', 'Mac only (real)', 'Amiga only'], ddrows, cls='data small')}
<h4>Finder type/creator codes (<code>extracted/mac/_finder_info.tsv</code>, {n_fi} entries)</h4>
{table(['Type / creator', 'Files', 'Example'], firows, cls='data small')}
<p>Notes: <code>APPL/PAYB</code> = the two games, <code>APPL/PMpE</code> = the map editors, <code>adrp/PAYB</code> =
aliases at the volume root, <code>PBDt/PAYB</code> = <code>Maps/0.scores</code>, <code>MPG /TVOD</code> = the intro films,
<code>PDF /CARO</code> = the manuals. Most data files have no type (<code>????</code>), including the ~40 blank-named
icon-mosaic files at the root (9134-byte resource forks).</p>
'''
    # C phrases
    head, added, removed, changed = phrases()
    groups = []
    for lo, hi, desc, use in PHRASE_GROUPS:
        ids = [n for n, _ in added if lo <= n <= hi]
        if lo == 0:
            ids = [n for n, _ in added if not any(a_ <= n <= b_ for a_, b_, *_ in PHRASE_GROUPS[:-1])]
        if ids:
            groups.append([f'{min(ids)}-{max(ids)}', len(ids), il(desc), il(use)])
    plist = table(['#', 'Mac English text'], [[n, html.escape(s)] for n, s in added], cls='data small', raw=True)
    appC = f'''
<p><code>{html.escape(head.strip("= "))}</code></p>
<p>Added: {len(added)}; changed: {len(changed)}; removed: {len(removed)} (removed: {html.escape(_ranges(removed))}).
Usage per frontend.md §6.3, corrected by gameplay_part2.md M4 for the random-mission ranges.</p>
{table(['Range', 'Count', 'Content', 'Used by'], groups, cls='data small', raw=True)}
{details(f'All {len(added)} new English global phrases', plist)}
{details('Changed English phrases (frontend.md §6.3 table)', md(section(f, '6.3 English changes')))}
'''
    # D user maps
    um = usermaps()
    umrows = [[f'`{n}`', ' '.join(c), f'{s:,}', '-' if t is None else (f'{t:,}' + (' (empty)' if t <= 4 else ''))] for n, c, s, t in um]
    appD = f'''
<p>{len(um)} <code>.map</code> files in <code>extracted/mac/Payback/UserMaps</code> ({len(um) - 1} playable; <code>test.map</code> is skipped by <code>InitUserMaps</code>).
Columns from the extracted files; per-map notes in assets.md §9.</p>
{table(['Map', 'Files', '.map bytes', '.textures bytes'], umrows, cls='data small')}
{details('assets.md §9 table with notes', md(section(a, '9. UserMaps')))}
'''
    S.append(('appendices', 'Appendices', '', 2))
    S.append(('app-a', 'A. Key names (symbols.md)', appA, 3))
    S.append(('app-b', 'B. File-level data diff counts', appB, 3))
    S.append(('app-c', 'C. New Mac phrases (summary)', appC, 3))
    S.append(('app-d', 'D. User maps', appD, 3))
    S.append(('app-e', 'E. docs/re/mac_version.md', md(MACVER, shift=2), 3))
    return S


def _ranges(nums):
    nums = sorted(nums)
    out, s, p = [], None, None
    for n in nums:
        if s is None:
            s = p = n
        elif n == p + 1:
            p = n
        else:
            out.append(f'{s}-{p}' if s != p else str(s))
            s = p = n
    if s is not None:
        out.append(f'{s}-{p}' if s != p else str(s))
    return ', '.join(out)


CSS = r'''
:root{--bg:#fbfbfa;--fg:#1d1f21;--muted:#5d6166;--card:#fff;--line:#d9dcdf;--code:#f0f1f2;--accent:#2557a7;
--th:#eef1f4;--row:#f7f8f9;--nav:#f3f4f5;
--fix:#1f7a3a;--fixbg:#dff3e5;--chg:#1d4f9c;--chgbg:#dde8fa;--new:#6b2fa3;--newbg:#ece0f8;--plat:#5a5f66;--platbg:#e6e8ea;
--data:#8a5a00;--databg:#fbeccc;--bug:#a12121;--bugbg:#f9dddd;--keep:#0c6b6b;--keepbg:#d7f0ef}
@media (prefers-color-scheme: dark){:root{--bg:#16181b;--fg:#e3e5e8;--muted:#9aa0a6;--card:#1e2125;--line:#33373c;--code:#2a2e33;
--accent:#7fb0ff;--th:#24282d;--row:#1b1e22;--nav:#1b1d20;
--fix:#7fdc9a;--fixbg:#1d3a26;--chg:#9cc2ff;--chgbg:#1d2c45;--new:#d4a8ff;--newbg:#33234a;--plat:#c3c7cc;--platbg:#2c3035;
--data:#ffcf73;--databg:#3d3018;--bug:#ff9b9b;--bugbg:#4a2222;--keep:#7fe0dc;--keepbg:#173a39}}
*{box-sizing:border-box}
html{scroll-behavior:smooth}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
a{color:var(--accent)}
code{background:var(--code);padding:0 .25em;border-radius:3px;font:.88em/1.4 ui-monospace,Consolas,monospace}
pre{background:var(--code);padding:.6em .8em;overflow:auto;border-radius:6px}
pre code{background:none;padding:0}
.layout{display:grid;grid-template-columns:260px minmax(0,1fr);gap:0}
nav{position:sticky;top:0;height:100vh;overflow:auto;background:var(--nav);border-right:1px solid var(--line);padding:14px 12px;font-size:13.5px}
nav h2{font-size:14px;margin:0 0 8px}
nav ol{padding-left:18px;margin:0}
nav li{margin:2px 0}
nav ol ol{padding-left:14px;font-size:12.5px}
main{padding:20px 36px 80px;max-width:1500px}
header.top{border-bottom:2px solid var(--line);margin-bottom:12px;padding-bottom:8px}
header.top h1{margin:.2em 0;font-size:28px}
header.top p{margin:.2em 0;color:var(--muted)}
section{scroll-margin-top:8px}
h2{border-bottom:1px solid var(--line);padding-bottom:4px;margin-top:2.2em}
h3{margin-top:1.6em}
table{border-collapse:collapse;margin:.6em 0 1em;width:100%;font-size:13.5px}
th,td{border:1px solid var(--line);padding:4px 7px;vertical-align:top;text-align:left}
th{background:var(--th);position:sticky;top:0;z-index:1}
tbody tr:nth-child(even){background:var(--row)}
table.small{width:auto}
details{border:1px solid var(--line);border-radius:6px;margin:.6em 0;background:var(--card)}
details>summary{cursor:pointer;padding:6px 10px;font-weight:600}
details[open]>summary{border-bottom:1px solid var(--line)}
.det{padding:4px 14px 10px;overflow-x:auto}
.chip{display:inline-block;padding:0 .5em;border-radius:999px;font-size:12px;font-weight:600;white-space:nowrap;border:1px solid transparent}
.c-fix,.c-adopt{color:var(--fix);background:var(--fixbg)}
.c-change{color:var(--chg);background:var(--chgbg)}
.c-new{color:var(--new);background:var(--newbg)}
.c-plat,.c-ign{color:var(--plat);background:var(--platbg)}
.c-data,.c-opt{color:var(--data);background:var(--databg)}
.c-bug{color:var(--bug);background:var(--bugbg)}
.c-keep{color:var(--keep);background:var(--keepbg)}
.c-conf{color:var(--fg);background:var(--platbg);border-color:var(--line)}
.c-inf{color:var(--muted);background:transparent;border:1px dashed var(--muted)}
.c-mixed{color:var(--muted);background:var(--platbg);border:1px dashed var(--muted)}
ul.chips{list-style:none;padding:0;display:flex;flex-wrap:wrap;gap:8px}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(440px,1fr));gap:14px}
.card{background:var(--card);border:1px solid var(--line);border-left:4px solid var(--accent);border-radius:8px;padding:6px 16px 10px}
.card h3{margin:.4em 0 .2em;font-size:17px}
.card .verdict{font-weight:600;margin:.2em 0 .4em}
.card ul{padding-left:18px;margin:.3em 0}
.card li{margin:.25em 0}
.kpis{display:flex;flex-wrap:wrap;gap:10px;margin:12px 0}
.kpis div{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:8px 14px;min-width:150px}
.kpis b{display:block;font-size:24px}
.kpis span{color:var(--muted);font-size:13px}
figure{margin:.8em 0;max-width:100%}
figure img{max-width:100%;height:auto;border:1px solid var(--line);image-rendering:auto}
figcaption{font-size:13px;color:var(--muted)}
figcaption .src{font-size:11.5px}
.two{display:flex;flex-wrap:wrap;gap:20px;align-items:flex-start}
.iconrow{display:flex;gap:20px}
.iconrow img{image-rendering:pixelated;width:128px}
.filter{display:flex;gap:8px;flex-wrap:wrap;align-items:center;margin:.6em 0;position:sticky;top:0;background:var(--bg);padding:6px 0;z-index:2}
.filter input{flex:1;min-width:260px;padding:6px 8px;border:1px solid var(--line);border-radius:6px;background:var(--card);color:var(--fg)}
.filter select{padding:6px;border:1px solid var(--line);border-radius:6px;background:var(--card);color:var(--fg)}
.fcount{color:var(--muted);font-size:13px}
.oq{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:4px 14px;margin:10px 0}
.oq h4{margin:.5em 0 .2em}
.oq .ask{border-left:3px solid var(--data);padding-left:8px}
.src,.note{color:var(--muted);font-size:13px}
.warn{color:var(--bug)}
@media (max-width:900px){.layout{grid-template-columns:1fr}nav{position:static;height:auto}main{padding:12px}}
@media print{nav{display:none}.layout{display:block}details{border:none}}
'''

JS = r'''
(function(){
 function apply(tid){
  var t=document.getElementById(tid); if(!t) return;
  var box=document.querySelector('.fbox[data-target="'+tid+'"]');
  var words=(box&&box.value||'').toLowerCase().split(/\s+/).filter(Boolean);
  var sels=document.querySelectorAll('.fsel[data-target="'+tid+'"]');
  var n=0,tot=0;
  t.querySelectorAll('tbody tr').forEach(function(tr){
   tot++;
   var txt=tr.textContent.toLowerCase(), ok=words.every(function(w){return txt.indexOf(w)>=0;});
   sels.forEach(function(s){ if(s.value && tr.getAttribute('data-'+s.getAttribute('data-attr'))!==s.value) ok=false; });
   tr.style.display=ok?'':'none'; if(ok) n++;
  });
  var c=document.querySelector('.fcount[data-target="'+tid+'"]'); if(c) c.textContent=n+' of '+tot+' rows';
 }
 document.querySelectorAll('.fbox,.fsel').forEach(function(el){
  var tid=el.getAttribute('data-target');
  el.addEventListener('input',function(){apply(tid);}); el.addEventListener('change',function(){apply(tid);});
  apply(tid);
 });
 // open a <details> when navigating to an anchor inside it
 function reveal(){ var id=location.hash.slice(1); if(!id) return; var el=document.getElementById(id);
  while(el){ if(el.tagName==='DETAILS') el.open=true; el=el.parentElement; } }
 window.addEventListener('hashchange',reveal); reveal();
})();
'''


def main():
    S = build()
    toc, body = [], []
    open_sub = False
    num = 0
    for sid, title, h, lvl in S:
        if lvl == 2:
            if open_sub:
                toc.append('</ol></li>')
                open_sub = False
            num += 1
            # sub-entries from h3 ids
            subs = re.findall(r'<h3 id="([^"]+)">(.*?)</h3>', h)
            if sid == 'appendices':
                toc.append(f'<li><a href="#{sid}">{html.escape(title)}</a><ol>')
                open_sub = True
            elif subs:
                toc.append(f'<li><a href="#{sid}">{html.escape(title)}</a><ol>'
                           + ''.join(f'<li><a href="#{a_}">{b_}</a></li>' for a_, b_ in subs) + '</ol></li>')
            else:
                toc.append(f'<li><a href="#{sid}">{html.escape(title)}</a></li>')
            body.append(f'<section id="{sid}"><h2>{num}. {html.escape(title)}</h2>{h}</section>')
        else:
            toc.append(f'<li><a href="#{sid}">{html.escape(title)}</a></li>')
            body.append(f'<section id="{sid}"><h3>{html.escape(title)}</h3>{h}</section>')
    if open_sub:
        toc.append('</ol></li>')
    today = datetime.date.today().isoformat()
    page = f'''<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>Payback: Mac release vs Amiga - differences report</title><style>{CSS}</style></head>
<body><div class="layout"><nav><h2>Contents</h2><ol>{''.join(toc)}</ol>
<p class="note">Generated {today} by <code>tools/reports/mac_differences.py</code>.</p></nav>
<main><header class="top"><h1>Payback: the Mac release compared with the Amiga builds</h1>
<p>Payback (Apex Designs, 2001) - Mac OS 9 / OS X edition 1.0 (Virtual Programming, 2002) vs Amiga 68k Update 7 and OS4, and vs our Bevy reimplementation.</p>
<p>Sources: <code>docs/xref/symbols.md</code>, <code>rendering.md</code>, <code>gameplay.md</code>, <code>gameplay_part2.md</code>, <code>frontend.md</code>, <code>assets.md</code>, <code>audio_platform.md</code>, <code>scratch_gameplay/people.md</code>, <code>scratch_gameplay/missions.md</code>; <code>docs/re/mac_version.md</code>.</p></header>
{''.join(body)}
</main></div><script>{JS}</script></body></html>'''
    with open(OUT, 'w', encoding='utf-8', newline='\n') as fh:
        fh.write(page)
    print(f'wrote {OUT}: {os.path.getsize(OUT) / 1e6:.2f} MB, images {IMG_BYTES[0] / 1e6:.2f} MB raw; '
          f'{len(D)} differences, {len(PF)} port fixes, {len(AP)} adopt items, {len(OPEN)} open questions, '
          f'{len(DISAGREE)} disagreements')


if __name__ == '__main__':
    main()
