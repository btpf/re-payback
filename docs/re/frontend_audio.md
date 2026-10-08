# Front end, HUD and audio

Reverse-engineering notes for Payback's front end (title, menus, saves, replays), the
in-game HUD, the icon/font assets, sound/music and the intro video.

Addresses are in the AmigaOS4 PPC build (`analysis/ghidra_os4/decompiled.c`) unless
marked "68k". **CONFIRMED** = read in code and/or verified against every shipped file;
**INFERRED** = consistent with the data/code but not fully traced. Abbreviated (C) / (I).

Scripts written for this document (all in `analysis/`):

| Script | Purpose |
|---|---|
| `os4mem.py` | read bytes/strings from the OS4 ELF by virtual address |
| `render_icons.py` | renders every `Icons/*` file to `analysis/img/icons/*.png` + contact sheets |
| `fontmap.py`, `fontmap2.py` | dump the font glyph-slot tables built by `FUN_0100e088`/`FUN_0100f450` |
| `intro_video.py` | FLC + raw-PCM intro decoder (§5) |
| `scratch/replay_decode.py` | replay decoder (§3.8) |
| `scratch/hud_mock.py` | HUD mock-up at 320×256 (§2) |


## 1. Icons and fonts — `Icons/*`

### 1.1 File format (CONFIRMED)

Every file in `Icons/` (except `TitleScreen.w3d`) is **headerless 8-bit chunky,
palette-indexed** (game palette, `analysis/palette.bin`), row-major top-to-bottom.
Rows are padded to a **stride of `ceil(w/16)*16` bytes** (Amiga 16-pixel words), so
`filesize = stride × h`. **Colour 0 is transparent.** The dimensions are *not* in the
file; they are hard-coded at the call sites in `FUN_010138dc` (front-end/HUD asset
loader) and `FUN_0104feb0` (screen setup, `Loading`). Verified for every shipped icon:
all bytes beyond column `w` are zero (`render_icons.py`; output `analysis/img/icons/`).

`LoadIcon` = `FUN_0100d3c4(out, path, w, h, filesize)`: allocates `w*h`, reads
`filesize` bytes, `stride = filesize / h`, copies the `w×h` rectangle; returns a
16-byte struct `{u8 *pixels; i32 w; i32 h; W3D handle}`. A missing file prints
"LoadIcon: Unable to read %s..." and yields an all-zero (invisible) icon.
Each load ticks the progress bar by 2000 (`FUN_010434a0(2000)`).
When the screen is ≥ 640×400 (`DAT_0116e718 = 1`, set in `FUN_010138dc` when
`width > 0x27f && height > 399`) icons are **doubled to 2w×2h** on load; the in-between
pixels are blended with the 50 % table at `alpha_tables + 0x40000`
(`General/Alpha.256`, 11 tables of 64 KiB, indexed `[a*256+b]`).

`LoadIconShadow` = `FUN_01008ed4(out, path)` (used for `LeftArrow`/`RightArrow`): reads
160 bytes as 13×10 (stride 16) and produces a **14×11** icon where each opaque pixel
also writes colour **8** (black) at (+1,+1) — a drop shadow. `FUN_0100db84` is a generic
variant (w+1)×(h+1) with the same shadow rule.

`ScaleIcon` = `FUN_0100d9c0(icon, w, h)`: nearest-neighbour resample (used to stretch the
title screen to the actual screen size).

### 1.2 Dimension table (CONFIRMED, from `FUN_010138dc` unless noted)

| File(s) | w×h | stride | bytes | Notes |
|---|---|---|---|---|
| `Cursor` | 6×10 | 16 | 160 | menu pointer |
| `Heart.Small`, `Heart.Big` | 9×10 | 16 | 160 | health; two frames of a heartbeat |
| `Checkpoint.red`, `.blue` | 54×54 | 64 | 3456 | 4 green / 4 yellow arrow-heads pointing inwards (checkpoint marker) |
| `Big.<c>` | 23×22 | 32 | 704 | big font (font 0) — gold serif capitals |
| `You.<c>` | 6×10 | 16 | 160 | small font 1 (`FUN_0100f450("icons/you.",1,6,10,0xa0)`); `You.c_ap` is only 144 bytes (last row read as 0) |
| `Them.<c>` | 6×10 | 16 | 160 | small font 2, same glyph set as `You` minus `c0..c11` |
| `LeftArrow`, `RightArrow` | 13×10 → 14×11 | 16 | 160 | `LoadIconShadow` |
| `LoadingPiece`, `LoadingPiece2` | 6×16 | 16 | 256 | progress-bar segment (red bar) |
| `PaybackLogo` | 166×80 | 176 | 14080 | flaming logo |
| `Blob.0`–`.3` | 7×7 | 16 | 112 | minimap / radar blips |
| `Lights.0`–`.3` | 37×20 | 48 | 960 | race start lights |
| `Phone`, `Lips`, `Mobile`, `Pager`, `Info` | 25×38 | 32 | 1216 | message-source icons |
| `BigPager.0`–`.9` | 7×7 | 16 | 112 | pager LCD digits |
| `BigPager.image` | 38×38 | 48 | 1824 | big pager body |
| `Small.0`–`.9`, `Small.x`, `Small.Person` | 9×13 | 16 | 208 | small HUD digits, "×", person |
| `Siren.1`–`.10` | 9×13 | 16 | 208 | 10-frame spinning star (wanted level). `Siren` (no suffix) is the CD release's static wanted star (loaded as 9×13, 208 bytes, from `PROGDIR:icons/siren` at CD-exe file offset 0x6646), replaced by `Siren.1..10` in Oct 2001; not loaded by Update 7/OS4 (C) |
| `<Weapon>.Normal/.Bright` | 9×20 | 16 | 320 | Fist, Pistol, Bomb, Shotgun, Flamethrower, Grenade, Minigun, Bazooka; the slot number is drawn into the top of the icon (Fist 0, Pistol 1, Shotgun 2, Flamethrower 3, Grenade 4, Minigun 5, Bazooka 6, Bomb 7) |
| `Cup.None/.Bronze/.Silver/.Gold` | 15×20 | 16 | 320 | level rating cups |
| `Loading` | 94×35 | 96 | 3360 | "Loading" + empty bar; loaded in `FUN_0104feb0` |
| `TitleScreen` | 320×200 | 320 | 64000 | used when screen h ≤ 200 |
| `TitleScreen.medium` | 320×256 | 320 | 81920 | screen ≤ 320×256, h > 200 |
| `TitleScreen.big` | 640×480 | 640 | 307200 | any larger screen; then `ScaleIcon` to screen size |
| `TitleScreen.w3d` | 256×256 RGB24 | 768 | 196608 | Warp3D mode only (`DAT_0114c7d8 != 0`): loaded as a 24-bit texture (`FUN_010024e4(1)` = RGB mode) and stretched |

In software mode the title picture is copied into a back buffer and remapped through
`alpha_tables + 0x40000 + c` (i.e. the 50 % blend with colour 0 → **darkened**) for use as
the menu background (`FUN_010138dc`, end). `FadeDarken` / `FadeDarken2`
(`FUN_0103500c`) are effect tables created next to it.

Contact sheets: `analysis/img/icons/sheet_*.png`, `sheet_misc.png`.

### 1.3 Font naming and character mapping (CONFIRMED)

Fonts are loaded per character into a glyph table of 16-byte icon structs:
`glyph[font][code]` at `0x01174e8c + (font*0x100 + code)*16` (fonts 0 = Big, 1 = You,
2 = Them), plus a second "Latin-2" table at `0x01177e8c` with the same indexing.
Loader for Big: `FUN_0100e088(prefix, font, w, h, size)`; for You/Them:
`FUN_0100f450` (also loads lower case). File name = prefix + suffix (`"%s%c"` /
`"%s%s"`), opened case-insensitively by AmigaOS:

* `'0'..'9'` → own code. `'a'..'z'` → files `X.a`… are stored at the **upper-case**
  code (`c-0x20`); Big copies each to the lower-case slot as well (Big has capitals only).
  You/Them load lower case from `X.little_<c>`.
* **Numeric suffixes are ISO-8859-1 (Latin-1) codes**: `Big.161` = `¡`, `.191` = `¿`,
  `.192` = `À`, `.199` = `Ç`, `.223` = `ß`, `.224` = `à` … Big maps each upper-case
  accented file to both its upper- and lower-case Latin-1 code (e.g. `Big.193` → 0xC1 and
  0xE1). `Them.91`/`.93` = `[` `]`.
* Named suffixes: `FullStop` . , `Comma` , , `Colon` : , `SemiColon` ; , `Hyphen` - ,
  `Quotes` " , `Apostrophe` ' , `OpenBracket` ( , `CloseBracket` ) ,
  `ExclamationMark` ! , `QuestionMark` ? , `GreaterThan` > , `LessThan` < , `Slash` / ,
  `Underscore` _ , `Plus` + , `Dollar` $ , `Percent` % , `Pound` £ (0xA3);
  `ae` Æ(0xC6)/æ(0xE6), `o_line` Ø/ø (0xD8/0xF8), `a_circle` Å/å (0xC5/0xE5),
  `a_dots` Ä/ä (0xC4/0xE4), `o_dots` Ö/ö (0xD6/0xF6), `n_tilde` Ñ/ñ (0xD1/0xF1)
  (little_ variants for the lower-case codes in You/Them).
* `You.c0`..`You.c11` → control codes **1..12**: controller-button symbols for control
  hints — c0–c3 arrows left/up/right/down, c4–c7 PlayStation-style ✕ □ ○ △, c8–c11 CD32
  red/yellow/blue/green buttons (`analysis/img/icons/sheet_you_c.png`). CONFIRMED by the key-name
  code `FUN_0103c690` / `FUN_0103d7b0` (lowlevel JPF bits): PSX 0x106 → 5 ✕, 0x108 → 6 □,
  0x107 → 7 ○, 0x109 → 8 △; CD32 0x11D Red → 9, 0x120 Yellow → 10, 0x11F Blue → 11,
  0x11E Green → 12; Play/Reverse/Forward use phrases 1510–1512. Produced only for key names in
  the control setup; no phrase file contains these codes.
* **Latin-2 table** (`0x01177e8c`), selected in text by a `'^'` prefix byte
  (`FUN_0100a9e8`): `c_caron` Č 0xC8 & 0xC0, `little_c_caron` č 0xE8 & 0xE0,
  `s_caron` Š 0xA9 & 0xC3, `little_s_caron` š 0xB9 & 0xE3, `z_caron` Ž 0xAE & 0xC4,
  `little_z_caron` ž 0xBE & 0xE4, `d_line` Đ 0xE2, `little_d_line` đ 0xC2 (sic),
  `c_ap` Ć 0xC1, `little_c_ap` ć 0xE1. The first code of each pair is ISO-8859-2 (used by
  `Global.Slovensko.phrases`: `^\xE8` etc.), the second is the private code set used by
  `Global.Srpski.phrases` (`^\xE0` č, `^\xE3` š, `^\xE4` ž, `^\xE1` ć, `^\xE2` đ). Czech,
  Polish and Slovak phrase files are written without diacritics.
* A code with no glyph is drawn with the `'?'` glyph (`0x0117527c` = slot 0x3F).
* Small-font text (`FUN_0100a9e8(str, x, y, font)`) is **fixed pitch: 7 px per
  character and per space** (14 px at ≥ 640×400).

## 2. In-game HUD

Main routine `FUN_01019bd8` (68k `FUN_00024a3e`), called from the frame renderer
`FUN_0105bd98`. Mock-up of all elements at 320×256: `analysis/img/hud_mock_320x256.png`
(`analysis/scratch/hud_mock.py`); per-icon renders in `analysis/img/hud/`.

### 2.1 Coordinates, scaling, blending

* Globals (C, `FUN_0104107c`/`FUN_0105bd98`): `W=DAT_0114c348`, `H=DAT_0114c344`,
  `xmax=0114c340=W-1`, `ymax=0114c33c=H-1`, `cx=0114c330=((W-1)>>1)+1` (160 at 320),
  `cy=0114c32c`, `xs=0114c328=(W+8)*16/320`, `ys=0114c324=(H+8)*16>>8`, stride `0114c31c`.
  Front-end screens use a **virtual 320×256 grid** mapped by `v*xs>>4`, `v*ys>>4`; many
  front-end y positions are N/200 of H. (C)
* The HUD is anchored with constant offsets to the left/top, `xmax`/`ymax` or `cx`; it is
  **not scaled** except by the hi-res flag `DAT_01174e88` (W ≥ 640 and H ≥ 400), which
  doubles every offset and pitch (7→14, 10→20) and uses pre-doubled icons. (C)
* The view-size option `DAT_0114c310` (/256) shrinks the 3-D view, centred; the HUD is
  drawn relative to that view (framebuffer pointer moved by `FUN_0101168c`). (C)
* Blitters: `FUN_0100a53c(icon,x,y)` plain (0 transparent, clipped); `FUN_0100b3c8`
  "lighten" via Alpha table `0x30000`; `FUN_0100ce08` via `0x70000` (green-add); inner
  loop `FUN_01111280`: `out = table[dst*256+src]`. (C)
* `General/Alpha.256` = 11 tables × 64 KiB, index `dst*256+src`, result = nearest palette
  colour (weights 25R/36G/16B), 0 replaced by 8 (generator `FUN_0103b540`):
  0x00000 add, 0x10000 subtract, 0x20000 dst−16, 0x30000 lighten
  (`src + dst*(255-max(src))/255`), 0x40000 50 % average, 0x50000 red-add, 0x60000
  red+green add, 0x70000 green-add, 0x80000 blue-add, 0x90000 add−32, 0xA0000 brightness
  ±(row−128). (C; regenerated tables match the shipped file in ≥147/150 random samples.)
* `EffectTextures/FadeDarken`, `FadeDarken2`, `FadeDarken3` (64×64) are stretched over a
  rectangle through the **subtract** table: FadeDarken → dark blue panel (subtracts
  ≈(195,155,47)), FadeDarken2 → grey with lighter edges, FadeDarken3 → centred box (pause
  menu, loading). `FUN_0103500c`/`FUN_01036044`/`FUN_01033d50`; u += 0x3FFFFF/(w+1) per
  pixel, v += 0xFC0/(y1−y0) per row. (C)
* Game logic runs at **30 ticks/s**. HUD blink/spin animations step per *rendered frame*
  (counter `011746ac`, 0..3). (C)

### 2.2 Single-player layout at 320×256 (C unless marked)

| Element | Position (lo-res) | Hi-res | Details |
|---|---|---|---|
| Weapons `FUN_01019490` | fist (5,5); slots x = 15,25,…,75, y = 5 | x = 5,25,45… step 20 | order pistol, shotgun, flamethrower, grenade, minigun, bazooka, bomb (player +0x378,+0x364,+0x36c,+0x370,+0x374,+0x368,+0x37c/+0x380); a slot is drawn only if its ammo > 0. Current weapon (+0x360) alternates `.Normal`/`.Bright` every 2 frames (`011746ac & 2`) |
| Ammo | 5 digits `Small.N`, x = 5+10k, y = 26, lighten | y 47, step 20 | leading zeros; hidden for fist |
| Score / money `FUN_010180b0` | x = xmax−75, y = 5, 7 digits × 10 px, lighten | x−70, y 10 | player +0x384; > 9,999,999 → 8 digits from x−10; negative → 0 |
| Bonus multiplier (green) | `Small.x` at (xmax−75,19), digit at (xmax−65,19) | (xmax−150,38)/(xmax−130,38) | green-add table; value `DAT_0114c74c` (1–4, `FUN_010413ec`) multiplies score gains (`FUN_01041408`) |
| Lives | `Small.Person` plain (xmax−35,19), `Small.x` xmax−25, digit xmax−15 | −70/−50/−30, y 38 | `DAT_0114c6d0` (getter `FUN_01042078`): set to 3 (and `DAT_0114c728` = 3) at every level start (`FUN_010692bc` ~49232); death −1 (~49668); at 0 state 2 "Game over!" 2026; AddLives `FUN_01042084` / `FUN_010d14a8` (C) |
| Hearts `FUN_01018518` | 5 icons, x = cx−25+10i, y = 31, lighten | y 62, step 20 | health = player +0x34c (68k +0x34a); heart i = `Heart.Big` if health/20 ≥ i else `Heart.Small`; health < 1 → all small. No pulsing; the Heartbeat sample is never played |
| Wanted level | `Siren.1..10` stars, y = 16 | y 32 | n = player +0x330; pitch 11; x0 = cx − 11n/2; phase c++ per frame (reset > 29); star k uses p = c+2k (wrap at 30): `Siren.(p+1)` if p ≤ 9 else `Siren.1` → a ripple of spinning stars (disasm `0101a464`) |
| Countdown timer | `BigPager.image` at (5,45); 3 LCD digits `BigPager.N` at x = 13/21/29, y = 60 | (10,90); 26/42/58, 120 | shown if 0 ≤ t < 1000 s; t = `FUN_010f699c()` (script timer /30); TickTock each second |
| Start lights | `Lights.N` at (cx−19, H/4) | x cx−38 | if `DAT_0114c2b8 ≠ −1` (`FUN_01017f4c`): 0 both off, 1 red+off, 2 red+red (Beep1), 3 green+green (Beep2) |
| Lap counter | "Lap %d of %d", font 1, (5,45) | (10,90) | if lap `0114a910` > 0; lap ≥ 2 plays Beep3 (`FUN_01017fdc`) |
| Level-end blink | (5,40) | (10,80) | phrases 2023 "Level complete!" / 2049 "Press F1 to continue" alternate every 31 ticks; font flips 1↔2 every frame |
| Vehicle-name banner `FUN_01016224` | FadeDarken2 band (cx−80,0)–(cx+80,h); text x = cx−7·len/2 | ±160, h 32 | name phrase of the controlled object (+0x493c), set when switching object (`FUN_010168f0`). t<20: h=16t/20, text y=h−14; 20–129: h=16, y=2; 130–149 slides up; >149 hidden |
| Minimap | 64×64 at (xmax−70, 34) | (xmax−142, 66) | §2.4 |
| Checkpoint marker | projected target, centred (−27) | −54 | §2.5 |

The money display *is* the score (player +0x384).

### 2.3 Message box (Phone / Lips / Mobile / Pager / Info) — draw `FUN_01016424`, tick `FUN_01016d7c`

* Queue of 100 `{type, text, font, icon}`: 1 = open with icon (`FUN_01015d7c`), 2 = add
  text (`FUN_01015e18`), 3 = close. Icon 0 Phone, 1 Lips, 2 Mobile, 3 Pager, 4 Info
  (`0x0117b848 + 16*idx`). Font 1 (`You`, red) / font 2 (`Them`, blue) chosen by script
  calls `FUN_010d1614` / `FUN_010d1638` = `Dialog_Say1` / `Dialog_Say2` (red = the player speaking, matched to the 68k API `0xb40fe`/`0xb411c`; C). The
  death/arrest charge sheet uses Lips + font 2 (`FUN_010bcf10`). (C)
* Lo-res geometry: opening animates FadeDarken over (0, ymax−1−40t/15)–(xmax, ymax) for
  t = 0..15 ticks (closing reverses); open box (0, ymax−41)–(xmax, ymax), icon 25×38 at
  (2, ymax−39). Hi-res: top ymax−81, icon (2, ymax−78). (C)
* Text: plain, 7 px pitch, x = 29 (hi-res 58); up to 3 lines at y = ymax−34, −23, −12
  (line k at `ymax−34 + 11*min(lines,2) + scroll − 11k`). A 4th line scrolls: scroll = 10,
  −1 per tick; while scroll > 4 four lines at ymax−12/−23/−34/−45 + scroll. 4-line ring
  buffer `0117b780` (250 bytes each). (C)
* Wrap: chars per line = (W−30)/7 (41 at 320); a word fits if len < cap − strlen(line).
  Typing: whole words appear, the next after 2 ticks × word length; 20-tick pause after
  the last word, then the next queue entry. While typing SqueakyVoice (id 16) loops
  (255 @ 12000 Hz). (C)

### 2.4 Minimap `FUN_01018954`

* Source `.mini` (§6.2), cropped to 64×64, red (0xB0) north marker at the top centre.
  **No background**: only road (4) and shadow (8) pixels are drawn over the 3-D view;
  north up. Not doubled in software hi-res. (C)
* Single-player only; called with (X,Y) = (xmax−71, 33), image at **(xmax−70, 34)**
  (x 249..312 at 320). (C)
* Markers: map pixel = world >> 12 (square = 0x800 units → 1 px = 2 squares). Player:
  `Blob.0` (green) / `Blob.1` (orange) alternating every 2 frames at
  `((px>>12)+X−3, (py>>12)+Y−3)`; mission target (`DAT_0114c67c/678` ≥ 0): `Blob.2` (blue)
  / `Blob.3` (red). Warp3D hi-res: >> 11. (C)
* "Rotating view" (`DAT_0114c7dc`): X = xmax − (xs*72>>4), Y = 40 (80); map drawn as a
  camera-rotated, perspective-tilted quad (corners ±32) via `FUN_01018844`, cos/sin tables
  `012a676c`/`012ac16c` (scale 256, **5760 steps per turn**), tilt index 5040; software:
  two triangles with `FUN_0107fc64`. (C)
* Replay playback (`DAT_0114c6ce`, I) replaces it with `FUN_010176e0`: clock "H:MM:SS.cc"
  at (xmax−78, 43) and a progress bar on rows 37–41 (black frame, 0xB4 red, 0x78 green
  elapsed). (C)

### 2.5 Checkpoint marker `FUN_0101cf80`

Shown when `DAT_0114c6d8` is set and a target exists: target projected with
`sx = (tx − camx/32)*xs*32 / (camz*16 − 256 + h) + cx − 27` (y likewise with ys/cy;
h = ground height of the target square via `FUN_0106fdbc`, I), lighten blend, alternating
`Checkpoint.red`/`.blue` every 2 frames (green/yellow with this palette). (C)

### 2.6 Split screen (2–4 players; `DAT_0114c7c8`, count `DAT_0114c2e4`) — `FUN_0105bd98`

* 2 players: side by side, W/2−1 wide, full height. 3–4: quadrants W/2−1 × H/2−1; with
  3 the bottom view is centred and the side strips are filled with 0. 1-px separators of
  colour 0. (C)
* Per view: vehicle banner, weapons + ammo (same coordinates), hearts centred at
  xmax'−30 (hi-res −60), y = 10, kills box `FUN_010199ec` at (5, ymax'−13): FadeDarken
  (0,y−5)–(5+(len+3)*7+5, y+13) + "%s %d" with 2189 "Kill(s):" in font 2, big title text.
  **No** minimap, score, stars, lives, timer or message box. (C)

### 2.7 Text routines (C)

| Routine | Font | Blend | Pitch |
|---|---|---|---|
| `FUN_0100a9e8(s,x,y,font)` | any | plain | 7 |
| `FUN_0100bb94(s,x,y,font)` | any | lighten | 7 |
| `FUN_0100b8a8` / `FUN_0100ba18` | 2 | lighten | 10 |
| `FUN_0100c234` / `FUN_0100c3b0` | 2 / 1 | lighten | argument |
| `FUN_0100c52c(s,x,y,pitch)` | 0 (Big) | lighten | argument (×2 hi-res) |
| `FUN_0100bd24` / `FUN_0100bfac` | 2 | centred word wrap, (W−20)/7 chars, 12-px lines | 7 |

All fixed pitch; outlines are baked into the glyphs. **Big title** (`FUN_010169d0` set,
`FUN_01016a70` draw, animated in `FUN_01016d7c`): pitch starts 120, −2 per tick to 16;
fixed-point y from −0xF00, velocity 0x100, gravity +0x80/tick; ticks 300..151 fall and
bounce at 0xA000 (v → −v/2), 150..101 hold, < 101 fall off until y > 0x10900. Drawn at
x = cx − pitch*n/2, y = (y*ys>>12) − 11 (rests at y = 149 at 320×256). Split screen:
pitch−8, x offset n*pitch/3, y*ys*3>>13 (2 players) or >>12.

## 3. Front end

Main loop `FUN_010692bc`. Menu input struct (`FUN_0103f3d0`, shorts): +0 left, +2 right,
+4 up, +6 down, +8 fire, +10 Esc. `FUN_01072790(n)` returns phrase *n*. Menu sounds
(§4.5): `FUN_010053c4(0x20, 235)` move, `(0x21, 270)` select, `(0x2D, 160)` refuse.
CD track 13 plays in all menus.

### 3.1 Boot and flow (C)

1. Default key maps, `S:Payback.config` (`FUN_0104199c`), screen setup `FUN_0104feb0`
   (loads `Loading`/`LoadingPiece`, progress `startup.size`).
2. Keyfile `PROGDIR:General/Payback.key2` (path stored with each byte +1 at
   `0x01133b34`; 10 bytes, five XOR-ed u16 checks). Registered → full main menu
   `FUN_0102fc44`; missing → key entry `FUN_0101dfe4` (2190–2194); unregistered → demo
   menu `FUN_01031e24`/`FUN_0103141c` (2470–2472) and play stops after 9000 ticks
   ("Time out!" 2490). A reimplementation should behave as registered.
3. `FUN_010138dc`: scan `Replays/` (`FUN_01009b48`), user maps (`FUN_0100a1d4`), icons,
   option objects, thumbnails, `InitLanguages` `FUN_0101345c`, global phrases
   (`FUN_010726dc("global")`), 14 save-slot names (`FUN_01011da8`).
4. Main-menu result v: v ≥ 0 → city = v/2 (`DAT_0114c694`), level = v%2
   (`DAT_0114c558`), LoadMap; −1 quit; −2 play replay (`FUN_0104135c`).
5. After a level (`DAT_0114c730`): 3 → Level Complete `FUN_0102395c` (→ Game Complete
   `FUN_01022348` once all 6 levels have a cup); 2 → Game Over `FUN_01021434`;
   multiplayer → Results `FUN_01027330`; then back into the Game menu (`DAT_0118ff28`).

**Title**: no separate "press fire" screen. The title picture (§1.2) slides in from
y = −H to 0 over 26 frames (`FUN_0100ab78(H*i/25)`, i = −25..0) while fading in from black
(`FUN_0104138c`), and slides/fades out the same way. Sub-screens use the darkened copy
`DAT_0117b6b4`. First run with no CD device configured goes to CD Audio Setup first.

### 3.2 Rotating 3-D option objects (C, `FUN_0102fc44`, `FUN_0102ca08`)

Model table `DAT_0117b6f8[17]` (`Objects/Options/*.Obj`):

| Idx | Model | Item |
|---|---|---|
| 0 | joystick | Start new game |
| 1 | disk | Load saved game |
| 2 | moviereel | View saved replay |
| 3 | cup_silver | Credits |
| 4 | cd | CD audio setup |
| 5 | controlsetup | Control setup |
| 6 | world | Set language |
| 7 | moviereel | View intro |
| 8 | amiga | Quit |
| 9 | joystick → cup_bronze/silver/gold once an overall rating exists | Play single player game |
| 10 | joystick_multi | Start multiplayer game |
| 11 | joystick | Single player rampage |
| 12 | disk | Save game |
| 13 | moviereel | View replay |
| 14 | moviereel | Save replay |
| 15 | amiga | End game |
| 16 | cross | Game Over screen object (`FUN_01021434`, z 0x898; C) |

One instance (`FUN_010c0c1c(model,x,y,heading)`, 0x4B8 bytes; [0..2] position, [3..5]
angles, [9] model) at **x = 0, y = 600, z = 6000**, drawn by
`FUN_010c101c(obj,0,0,0,0x65)`. Angles: 5760 (0x1680) per turn; per frame
`[3] −= 0x20, [4] += 0x40, [5] −= 0x10` (180/90/360 frames per turn). Changing item: the
object slides out at ±1500 units/frame; past ±4500 the model is swapped and it re-enters
from the other side to x = 0. Label centred at y = 70/200 (`FUN_0100bb94`, font 1);
`LeftArrow` at x = cx − (xs*0x60>>4) − 14 (28 hi-res), `RightArrow` at cx + (xs*0x60>>4);
screen title (Big font) at y = H/40. Lists flash the selected line on a 4-frame cycle.
`Cursor` is the text-entry caret (`FUN_01012e38`). Pause-menu models add `Torch.Obj`,
`QuestionMarks.Obj` and `People/malenormal.obj` (slide ±0x2EE, swap at 0x8CA).

### 3.3 Menus (C; phrase numbers from `Global.English.phrases`)

**Main menu `FUN_0102fc44`** — title 2085 "MAIN MENU", 9 items wrapping left/right,
label 2090+i except where noted: 0 Start new game 2090 (name entry 2088 "ENTER NAME",
default "Travis", 25 chars; resets times/cups → Game menu) · 1 Load saved game 2091
(`FUN_0101f1a4`) · 2 View saved replay 2092 (`FUN_0101fdcc`) · 3 Credits 2093
(`FUN_010206bc`) · 4 CD audio setup 2094 (`FUN_0102dd30`) · 5 Control setup 2454
(`FUN_0102e75c`) · 6 Set language 2132 (`FUN_0101267c`, reloads phrases) · 7 View intro
2109 · 8 Quit 2130 ("Are you sure?" 2070, Yes 2071 / No 2072).

View intro (C): **OS4** only plays the refuse sound; its video code (`FUN_01000978`, player
~0x01000e10) is compiled in but has no callers. **Update 7 68k** plays it: main menu
`FUN_00033192` item 7 → `FUN_00010364("intro")`, `FUN_000106c0` (play), `FUN_00011402`
(cleanup); refused only if the files are in neither `PROGDIR:Videos` nor
`PaybackCD:Payback/Videos`. 68k `FUN_000ec686` also plays it automatically at the end of the
startup load, after the CubeTextures and before CloseProgress (call at 0xeda52). The CD
release does the same (startup call at file offset 0x65caa, menu item index 6 at 0x14b70,
loader at 0x894c0).

**Game menu `FUN_0102ca08`** — title 2086 "GAME MENU", 7 items: 0 Play single player game
2134 (→ level select; with all cups also 2047 "GAME COMPLETE!" + rating 2073–2075) ·
1 Start multiplayer game 2133 (`FUN_0102b2c8`) · 2 Single player rampage 2197
(`FUN_0102a428`, `DAT_0114c7c6 = 1`) · 3 Save game 2096 (`FUN_0101f738`) · 4 View replay
2097 (`FUN_0101e618`) · 5 Save replay 2098 ("ENTER FILENAME" 2084, 30 chars, default
`"<name> - "`, `FUN_010bc3b0`, 2117 "REPLAY SAVED!") · 6 End game 2099. Items 4–5 hidden
until a game has been played; after Rampage the high-score entry runs first.

**Pause menu `FUN_0101a7dc`** (models `DAT_0117c944[]` via `FUN_0101a55c`; background
FadeDarken3 over virtual box 50..269 × 26..229):

| i | Model | Phrase | Action |
|---|---|---|---|
| 0 | joystick | Return to game 2050 | shows goal 2056 "Escape from jail" or 2058+N+2057 "Get N points", time elapsed 2059 (h:mm) |
| 1 | amiga | Quit to title screen 2051 | Yes/No |
| 2 | torch | FX: 2052 | lens flare 2060 Off / 2061 Flares only / 2062 Flares and artifacts |
| 3 | malenormal | Motion blur: 2053 | on/off; refused with Warp3D |
| 4 | – | Set sound volume 2054 | 0..100 step 10 (`FUN_01006558/64`), 10-piece LoadingPiece bar |
| 5 | questionmarks | Graphics options 1600 | Gouraud shading 1601 (`DAT_0114c2fc`), Rotating view 1602 (`DAT_0114c7dc`) |

**Credits** `FUN_010206bc(input, back, first=2200, speed=2, skippable=1, stopY=0x2E,
hold=100)` (demo nag: 2400, 3, 0, 0x46, 0x96). Line prefixes: ' ' big centred, '#' left
blue, '@' right red, '$' centred blue, '%' centred red, '?' PaybackLogo centred, 0xA3
half-screen gap, '!' end.

**Text entry** `FUN_01012e38(prompt, buf, maxlen, back, input)`: 1 = Enter, 0 = Esc;
hint 2119; allowed letters (folded), digits, space, `! ( ) , - . : ; _` (mask 0x0FFF7303
over 0x20..0x3B). Message box `FUN_0101dc60` + "Continue" 2110.

### 3.4 Single-player level select `FUN_01025544` (C)

* Title 2087 "CHOOSE LEVEL". The three city `.thumb`s sit on a virtual ring 320 wide at
  offsets +0x35, +0xA0, +0x10B (107 apart), scrolling ≤ 8 units/frame until the offset is
  0, 0x6B or 0xD6. Each is a textured square `FUN_010c3e10(tex, x*xs>>4, cy − (25*ys>>4),
  r, 0xE10)` with **r = max(20, 100 − |x−160|)** (centred city largest). Confirm runs a
  zoom counter 200 → 0 then fades.
* City name 2170+c at y = 0xB7 (virtual). Level rows at y = 199 and 221: name 2180+2c+l
  at x = 0x14; gold cup + time at x 0x96/0xA6; silver cup + time at 0xCD/0xDD; player's
  `Cup.*` + best time ("-:--" if none) at 0x104/0x114; level cursor = RightArrow at x = 6.
  Up/down = level, left/right = city. A level is refused if the previous level has no
  cup; default = first level without a cup.
* Target times `DAT_0117b6c8[city][level] = {gold, silver}` minutes (same in 68k
  `FUN_0001fcf8`); elapsed = `DAT_0114c738 / 0x708` (1800 ticks/min); shown "h:mm":

| City.Level | Name | Gold | Silver |
|---|---|---|---|
| 0.0 | Ready Or Not | 8 | 12 |
| 0.1 | Gangster Tripping | 10 | 14 |
| 1.0 | Going For Gold | 11 | 15 |
| 1.1 | Remote Control | 8 | 12 |
| 2.0 | Caught By The Fuzz | 4 | 7 |
| 2.1 | Born Slippy | 10 | 15 |

* Cup (`FUN_0102395c`): ≤ gold → 3 (2043), ≤ silver → 2 (2042), else 1 bronze (2041);
  stored only if better than the previous best. **Overall rating** = min cup over the 6
  levels (Game Complete shows 2047/2048, 2072+r, 2075+r). Level Complete shows a rotating
  cup object at z = 4000.

### 3.5 Multiplayer, results, Rampage, high scores (C)

* **Multiplayer setup** `FUN_0102b2c8` (title 2135): Player 1–4 device: −1 Off 2168,
  0–3 Joystick n 2166, 4–7 PSX n 2164, 8–11 CD32 n 2165 (lowlevel ReadJoyPort),
  12 Keyboard 2167 (defaults P1 = 0, P2 = 12, P3/P4 off; each device once); key-set hints
  2175–2178; City (2124 + 2170–2172 or a user map; 2039 "Not found!"); Required kills 2329
  (1..50, default 5); Play 2095 (needs ≥ 2 players, else 2179); Cancel. **Deathmatch is
  the only multiplayer mode**: kills +1, suicide −1; first to the target wins, ties draw.
* **Results** `FUN_01027330` (2129): 2173 "Player %d won the match!", 2128 draw, 2174
  aborted; Won 2161 / Drawn 2163 / Lost 2162 accumulate; menu City, Play again 2198,
  View replay, Save replay, End game.
* **Rampage** `FUN_0102a428` (2196, "Top 5 high scores:" 1610): City; Police
  aggressiveness 2055 + 2491+level (0..5, default 2); Play; Cancel. Score `DAT_0114c420`:
  civilian 1500×mult, cop 2500×mult, every 11th cop +100000 ("Copkiller bonus!"); ends
  at 0 of 3 lives ("Game over!" 2026); no time limit.
* **High scores** (`FUN_01029b3c` / `FUN_0102a0fc`): `PROGDIR:Maps/<n>.scores` or
  `<usermap>.scores`, 1024 bytes BE: 6 tables (police level) × 5 × {char name[30],
  s32 score}, then u32 checksum = ~(0x5DC1DCD1 XOR (7 BE words of each name) XOR every
  score). Missing/bad → "Travis", 500000×(table+1) halving down the list.

### 3.6 Save games — `Saves/PaybackSlot%d.save` (C: load `FUN_0101182c`, save `FUN_01011a70`)

`PROGDIR:Saves/PaybackSlot<n>.save` (fallback `S:`), slots 1..14, 152 bytes BE:

| Off | Size | Field |
|---|---|---|
| 0x00 | u32 | version = 1 (not checked) |
| 0x04 | char[50] | slot description |
| 0x36 | char[50] | player name |
| 0x68 | 3 × 16 | per city: s32 best time L0, s32 cup L0, s32 best time L1, s32 cup L1 |

Times in minutes (−1 none); cup 0 none/1 bronze/2 silver/3 gold. Load screen 2082
(14 lines at virtual y = 0x20+14i, Cancel 2114 at 0xEB; 2111 "GAME LOADED!" / 2112 "SLOT
EMPTY!"). Save screen 2089: default description `"%s - C%d, L%d"`, 40 chars, 2083
"GAME SAVED!".

### 3.7 Config — `S:Payback.config` (C: load `FUN_0104199c`, save `FUN_01041510`)

656 bytes, u32 BE unless noted, in order: version 10 (< 10 "Config file too old") ·
`DAT_0114c72c` saved motion blur · lens-flare mode 0..2 · char[40] CD device · CD unit
(−1) · CD music mode (0 Off 2142, 1 Radio 2143, 2 Continuous 2144, 3 Disabled 2145;
default 3) · software screen ModeID `DAT_0114c304` · char[40] language (empty → ask) ·
4 × {device, key set} · required kills · multiplayer/rampage city · view size
`DAT_0114c310` · police aggressiveness · 11 × 11 key codes · FX volume 0..100 · Warp3D
screen ModeID `DAT_0114c300` · Gouraud · rotating view · Warp3D. Resolution/screen mode
are picked in `PaybackSetup`, not in game (I).

Field meanings (C):
* `DAT_0114c72c` = saved motion-blur setting. The live flag is `DAT_0114c6c8` (toggled by
  `FUN_01042308` and pause item 3; the software path blends frames with `FUN_0112cdb8`). Level
  start (`FUN_010692bc` ~49442): blur on if c72c is set; level end (~49887): c72c = c6c8, then
  blur off for the menus.
* `DAT_0114c304` = software-render screen ModeID: `FUN_0104107c` gets W/H from it via
  GetDisplayInfoData and sets `DAT_0114c2e0` = native non-RTG (8 bitplanes + c2p
  `FUN_0112d1d0`); it is the OpenScreen SA_DisplayID in both software branches (0x0104ff88,
  0x010509c4).
* `DAT_0114c300` = Warp3D screen ModeID: the Warp3D OpenScreen (0x0105013c), W3D_CC_ModeID in
  `FUN_01002024`, and `FUN_0104107c` when `DAT_0114c7d8 != 0`.
* `DAT_0114c310` = 3-D view size in 1/256 of the screen, 0xB0..0x100 step 0x10: raw key 0x0C
  ('=') grows, 0x0B ('-') shrinks it (`FUN_01051940` / `FUN_01051a10`), refused under Warp3D;
  `FUN_01051800` clears the buffers and sets the partial-view flag `DAT_0114c6c4`;
  `FUN_0105bd98` scales W/H by it; forced to 0x100 when Warp3D is on.

Keys: 11 actions (phrases 2457–2467: accelerate, reverse/brake, left, right, shoot,
shoot alt, in/out, brake only, next weapon, prev weapon, repeat message). Keyboard raw
keys `4C 4D 4F 4E 40 00 44 31 5F 46 37` (cursor up/down/left/right, Space, –, Return, Z,
Help, Del, M). Repeat message (C): the message queue (`DAT_0117bc40`, 100 × 16-byte entries:
type 1 Open / 2 Text / 3 Close, count `DAT_011746a4`) is mirrored on every enqueue into a
last-message list (`DAT_0117c280`, count `DAT_011746a8`; `FUN_01015d7c` Open restarts it,
`FUN_01015e18` Text, `FUN_01015f68` Close, `FUN_01015b2c` any type; full → "LastMessageEnqueue():
Too many entries"). `FUN_01015a10` copies that list back into the queue; it is called from the
keyboard reader `FUN_0103f3d0` on the bound key's release (raw `| 0x80`) and from the pad reader
`FUN_0103da68` when the button goes down. Odd tables PSX `106 109 100 102 107 10C 108 10D 10B 10A 104`; even tables
CD32 `11D 11E 118 11A 11F 0 120 122 121 0 0`. Control setup `FUN_0102e75c` (2469):
11 columns (Single player 2455: Keyboard/PSX/CD32; Multiplayer 2456: PSX/CD32 per
player), eased scroll `p = (7p+target)>>3`, rows y = (50+10i)/200, key names 1500–1534,
prompt 2468 (0 = disable), Done 2148 / Cancel 2192. CD Audio Setup `FUN_0102dd30` (2131):
Device 2140, Unit 2141, mode 2142+m at 80/200, Done at 170/200.

### 3.8 Replays — `Replays/*` (C; decoder `analysis/scratch/replay_decode.py`)

List `FUN_01009b48` (`PROGDIR:replays/#?`, ≤ 2000), load `FUN_010bc000`, save
`FUN_010bc3b0`, picker `FUN_0101fdcc` (2113 "SELECT REPLAY"), info `FUN_0101e618`.
Header BE, **version 10 (68k Update 7) / 0x3F2 (OS4), 152 bytes**: version, players,
controller code[4], required kills, record count, duration (ticks), random seed, rampage
flag, police level, char map path[100], level-in-city; records at 0x98. Version 3 (CD
release): 144 bytes without rampage/police; records at 0x90.

Records (`FUN_01051a10`): one u16 per active player per rendered frame, uncompressed —
bit 15 comment follows (NUL-terminated, even-padded), 14 left, 13 right, 12 accelerate,
11 reverse/brake, 10 shoot, 9 in/out, 8 brake only, 7..3 weapon selected (10 = none),
2..0 ticks this frame (1..5). Buffer 150,000 bytes (recording stops when full). Playback
reuses the seed and per-frame tick counts → the simulation must be **deterministic**.
Ends on 0xFFFF or Esc.

Shipped `Replays/Race` ("Race for the prize", map 1, 1211 records, 5337 ticks = 2:57.9,
seed 26009) and `Replays/Tanks` ("Tanks a lot", map 0, 1650 records, 7586 ticks = 4:12.9,
seed 412) are **version 3**; Update 7/OS4 reject them ("WRONG VERSION NUMBER" 2116).

### 3.9 Languages (C)

`InitLanguages` = `FUN_0101345c`: scans `Phrases/Global.#?.phrases`, keeps the middle
token, ≤ 500 (`AddLanguage` `FUN_01012438`). Empty configured language → chooser
`FUN_0101267c` ("Please choose your language", hard-coded English; eased list
`y = (16*target + 8*old)/9`); choice saved to the config; lookup `FUN_01012504` falls
back to "English". Global phrases `FUN_010726dc("global")`; level phrases `"%d%d"`
(e.g. `00`) loaded by LoadMap except in Rampage/multiplayer. Text uses §1.3 (Latin-1,
`^` → Latin-2 glyphs, codes 1–12 = button symbols).

## 4. Audio

A software mixer feeds 4 Paula channels; samples do **not** have per-sound periods —
each AddSound call passes a playback rate in Hz. 68k equivalents: InitSound
`FUN_00012038`, AddSound `FUN_000170ce`, ModifySound `FUN_000172e8`, DeleteSound
`FUN_000173c8` (same ids and constants).

| Address | Role |
|---|---|
| `FUN_0100622c` | InitAudio: two 88,200-byte chip buffers (double-buffered), volume + step tables, 500 slots, then InitSound |
| `FUN_01005864` | InitSound: audio.device, 5 IOAudio requests (masks 1,2,4,8,0xF; pri 0x7F); loads 77 samples |
| `FUN_01005748(path,len)` | LoadSample → `{ptr,len}` at `0x01166648 + id*8`; ids 0x20, 0x21, 0x2D, 0x42 in chip RAM |
| `FUN_0100815c(id,loop,vol,hz,x,y,z)` | AddSound → slot (−2 "Too many sounds") |
| `FUN_010082bc(slot,loop,vol,hz,x,y,z)` | ModifySound (−1 = unchanged) |
| `FUN_01008398(slot)` | DeleteSound |
| `FUN_0100812c` / `FUN_01008144` | get/set slot sample id (engine → clonk swap) |
| `FUN_01007290` / `FUN_01006750` | UpdateSound, 1 listener / 4 listeners (split screen) |
| `FUN_01004f54` / `FUN_01007d10` | mix inner loop / submit + buffer flip |
| `FUN_010053c4(id,period)` | direct one-shot on channels 0+1 (menus) |
| `FUN_01005548` / `FUN_01005294` | intro-audio stream start / stop |
| `FUN_010050a4` | stop everything; `FUN_01006564`/`FUN_01006558` set/get FX volume |
| `FUN_010cda74` / `FUN_010cdae4` | `AddSound_script(ref 0..9, id, loop, vol, hz, x, y, z)` / `DeleteSound_script`; handles at `0x01359e9c[10]` |

### 4.1 Sound ids = load order (C)

Samples are headerless **signed 8-bit mono**; the loaded length equals the file size
except CarHorn (754 bytes loaded from a 734-byte file — 20 garbage bytes).

| id | file | id | file | id | file |
|---|---|---|---|---|---|
| 0 | Explosion | 26 | MissionComplete | 52 | Footstep.sand (ice) |
| 1 | Engine | 27 | TankEngine | 53 | Pistol |
| 2 | Skid | 28 | BigGunFire | 54 | HelicopterBlades |
| 3 | CarHorn | 29 | Ricochet | 55 | Beep1 |
| 4 | Footstep.stone | 30 | HitScenery | 56 | Beep2 |
| 5 | Thud | 31 | ThumpingMusic | 57 | Beep3 |
| 6 | ShipHorn | 32 | ChangeSelection | 58 | Hiss |
| 7 | Burp | 33 | ChangeSelection2 | 59 | Punch_Hit |
| 8 | Ambulance | 34 | Footstep.grass | 60 | RCEngine |
| 9 | CarStart | 35 | Footstep.metal | 61 | Rain |
| 10 | Heartbeat | 36 | Footstep.sand | 62 | Lightning |
| 11 | Siren | 37–39 | Ow1–Ow3 | 63 | BoatEngine |
| 12 | Squelch | 40 | Doorslam | 64 | Alarm2 |
| 13 | Flames | 41 | Flamethrower | 65 | Footstep.water |
| 14 | TelephoneRing | 42 | Click | 66 | Boom |
| 15 | LongAaah | 43 | Splash | 67 | Engine_904 |
| 16 | SqueakyVoice | 44 | IceCream | 68 | Engine_911Turbo |
| 17 | FireMissile | 45 | ChangeSelection3 | 69 | Engine_935 |
| 18 | GrenadeBounce | 46 | Engine_clonk | 70 | Engine_Bus |
| 19 | Minigun | 47 | ObjectAppear | 71–74 | Engine_904/911Turbo/935/Bus `_clonk` |
| 20 | PickupWeapon | 48 | Alarm | 75 | Thud2 |
| 21 | ShotgunFire | 49 | BrokenGlass | 76 | Screams |
| 22 | GarageDoor | 50 | Footstep.sand (snow) | | |
| 23 | Bleep | 51 | Footstep.sand (slush) | | |
| 24 | TickTock | | | | |
| 25 | MissionFail | | | | |

**Never played by the executable** (all 201 AddSound/ModifySound/DeleteSound call sites
scanned): ShipHorn, Burp, Ambulance, CarStart, **Heartbeat**, **ThumpingMusic** (C). The
scripts do not play them either (C): in the CD scripts AddSound is command 0x20 (CD DoScript
at file 0x4d6e6); levels 00 and 01 use 0x30 Alarm, 10 uses 0x3A Hiss, 20 uses 0x40 Alarm2, 11
and 21 have none. `Siren` is the police-car siren, not
a wanted-level sound.

### 4.2 Mixer (C, `FUN_01007290` / `FUN_01004f54` / `FUN_01007d10`)

* Output: 4 Paula channels at period **163** (PAL 21,760 Hz) while the mixer assumes
  **22,050 Hz** (0x5622). Buffer areas of 22,050 bytes: +0 ch0 left (group A), +0x5622
  ch1 right (A), +0xAC44 ch3 left (B), +0x10266 ch2 right (B). `CMD_WRITE`,
  `ADIOF_PERVOL`, cycles 0; previous request aborted each frame.
* Frame length `len = ticks*735 − 1 + 400` (735 = 22050/30, ticks 1..5); afterwards
  `pos += len*rate*200/22050` (quirk: includes the 400-sample overlap).
* **≤ 6 audible voices**, 3 per stereo group, summed without clipping; volume table
  `vt[v][s] = (int8)s * v / 768` (v 0..255). Hardware volume `master*V/100`, V = 31 or 63
  per group (louder voices with A > 127 → a 63 group; quieter ones doubled in a 31 group, I).
* Pitch byte `rate = Hz/200` (1..255, 200 Hz steps); step-table rows at `base+0x10000`
  approximate `r*200/22050`.
* Loop flag 1 → wrap; otherwise the slot frees itself when `pos > len`.
* Slots: 500 × 64 bytes at `0x0115e948`: +0 id (−1 free), +4 vol, +8 Hz/200, +C/+10/+14
  x,y,z (world>>4; y = −1 → non-positional), +18 position, +1C..+28 previous distance per
  listener, +2C squared distance, +3C loop.
* With FX volume 0, AddSound refuses non-looping sounds.

### 4.3 Positional audio (C, `FUN_01007290`)

* Listener = player or the driven vehicle (camera `(camX>>5, camY>>5, 22000)` during
  camera timers). Coordinates >> 4 (2048 world units per square → 128).
* `d² = dx²+dy²+dz²` (≥ 1); **`A = vol / d²`** clamped to 255; audible only if A > 19 and
  among the 6 largest. Non-positional: A = vol (255 typical). E.g. vol 10,000,000 is full
  within ≈198 units (1.5 squares), silent beyond ≈725 (5.7 squares).
* **Pan**: `r = dx'/|dy|` (dx' rotated by camera angle in Rotating-view mode, tables
  `012a676c`/`012ac16c`); r > 0: R = A, L = A/(r+1); r < 0: L = A, R = A/(|r|+1); r = 0 or
  dy = 0: L = R = A.
* **Doppler**: `step = rate + ((prevDist − dist)*184)/len`, clamped 1..255.
* Split screen: priorities per (sound, listener), best 6 overall (`FUN_01006750`).

### 4.4 Gameplay sounds (C; vol, Hz)

* **Engines**: on spawn/enter `AddSound(engine, loop, 10,000,000, 20000, x, y, z*16+100)`
  (handle obj+0x42C); skid loop at vol 0 (obj+0x434). Pitch each frame (`FUN_0105f594`):
  `Hz = |speed| * 20000 / clamp(maxSpd*health/50, maxSpd/4, maxSpd) + 6000` (tank
  `*10000 … + 4000`). Health ≤ 49 swaps to the `_clonk` sample (1→46, 67→71, 68→72,
  69→73, 70→74). Skid (`FUN_010cc288`): `s = clamp((|vx−v·cos|+|vy−v·sin|)*2/dt, 0, 255)`
  → vol `s*40000`, Hz `s*10+10000`.
* **Per vehicle** (`FUN_0104e13c`; engine / horn / reverse beep / CD radio track):
  mundaneo, fjord Engine/CarHorn/–/7 · taxi Engine/CarHorn/–/3 · policevan(_red/_blue)
  Engine/Siren · pug 935/CarHorn/–/11 · pug GTI 935/6 · pug racer 935/5 · diblo 904/5 ·
  vapour 904/4 · vapour racer 904/5 · hotrod 904/2 · police(_red/_blue) 911Turbo/Siren ·
  scooby 911Turbo/10 · evo 911Turbo/9 · bus Bus/CarHorn/Bleep/– · van Bus/15 · limo Bus/2 ·
  pickup Bus/14 · truck Bus/Bleep/12 (I) · bombtruck Bus/Bleep/16 · icecreamvan
  Bus/IceCream · tank TankEngine (horn key fires BigGunFire 15M @ 9000) · helicopter
  HelicopterBlades · remotecontrolledcar RCEngine/16 · boat BoatEngine/8 · rocketcar
  Flamethrower/12.
* **Horn** (`FUN_0105e02c`): police Siren loop 25,000,000 @ 10000; others loop 20,000,000
  @ `clamp(16,000,000/mass, 3000, 20000)` (mass = type+0x48EC); deleted on release (I).
  Reverse beep while speed < 0: loop 10,000,000 @ 22050.
* **Collisions** (`FUN_010cd198`): impact = v²>>14; < 11 Thud (speed*10000 @ 12000), else
  Thud2 @ 8000 (+ Screams 255 @ 15000+rand(2000) if the player is in the bus).
* **Footsteps**: id = the cube texture's first `.CubeTextureList` column (4 stone,
  34 grass, 35 metal, 36 sand, 50 snow, 51 slush, 52 ice, 65 water; default 4) — i.e. that
  column *is* a sound id; square flags&0x7F == 4 → water @ 15000. One-shot 3,000,000 @
  19000, z+200.
* **Weapons/combat**: Pistol 53 4M @ 14000; ShotgunFire 21 4M @ 14000; Minigun 19 4M @
  16000; FireMissile 17 5M @ 17000; Flamethrower 41 loop @ 12500; Punch_Hit 59 4M @
  22050; out of ammo Click 42 3M @ 16000; GrenadeBounce 18 4M @ 5000/8000; Ricochet 29
  2304+rand @ 11500+rand; bullet on wall HitScenery 30 2M @ 8500; bump scenery HitScenery
  speed·20000 @ 15000; pain Ow1–3 (37+rand 3) 5M @ 8000; Doorslam 40 5M @ 20500 (enter/exit
  vehicle); Splash 43 @ 8350; pedestrian run over Squelch 12 15M @ 18000; Explosion
  (`FUN_0104405c`) 50M @ `12000 − power/20000` (5000 if power ≥ 140M), z+100.
* **Fire** (`FUN_010542b8`): burning person LongAaah 10M @ 6000; Flames loop 500k @ 19000
  → `size*10000 @ 20000` → `*5000 @ 19000`; fire particles Flames 1.5M @ 19000.
* **World/mission**: phone TelephoneRing loop 10M @ 17000 at the phone square centre
  (`FUN_010cfa80`); BrokenGlass 20M @ 7000; ObjectAppear 20M @ 8000; PickupWeapon 1M @
  14000 (255 non-positional for HUD pickups); GarageDoor 8M @ 10500; countdown TickTock
  255 @ 8400 every 30 ticks; MissionComplete 255 @ 11025 with "Mission/Level complete!"
  (2022/2023); MissionFail 255 @ 11025 with "Mission failed!" (2028); message typing
  SqueakyVoice loop 255 @ 12000; message source / start lights / laps Beep1 @ 4182,
  Beep2 @ 8363, Beep3 @ 8363; Rain loop 255 @ 14000 (weather 2; scripts set 127/255 via
  `FUN_010cf9f0`); thunder (`FUN_010692bc`): each frame if rand(1000) < ticks, Lightning
  255 @ 6500+rand(500) / 240 @ 7500+rand(500) / 160 @ 8000+rand(500) with a screen flash
  (+rand(150)+100, decays ×3/4 per tick); script sounds Alarm2 loop 1e9 @ 11025, Alarm loop
  1e8 @ 16000, Hiss loop 4e6 @ 8300.

### 4.5 Menu sounds (C, `FUN_010053c4`)

Direct one-shot on channels 0+1 (mixer channels stopped first), length −8, volume
`master*63/100`, rate = 3,546,895/period (PAL): ChangeSelection (0x20) period 235
(15,093 Hz) cursor move; ChangeSelection2 (0x21) period 270 (13,137 Hz) select/confirm;
ChangeSelection3 (0x2D) period 160 (22,168 Hz) refuse; Boom (0x42) period 448 (7,917 Hz)
on the Game Complete screen.

### 4.6 Music = CD audio (C)

ThumpingMusic is unused; all music is Red Book CD audio. CD release: external
`General/CD_Audio_Handler`, launched `run >nil: CD_Audio_Handler <dev> <unit>`, message
ports `pbmain2`/`pbcdaudio` (CD_Init/CD_Play/CD_SetVolume/CD_Stop; message format not
decoded). Update 7/OS4: in-process "Payback CDDA Thread" (`FUN_010089dc`, entry
`0x010087a0`), commands in `DAT_0114c2b4` + Signal(0x1000): 0x2000 init/quit, 0x3000 stop
(`FUN_01008c84`), 0x4000+n play n (`FUN_01008bb0`), 0x5000+n ensure n playing
(`FUN_01008cec`, ≤ 1/s; restarts the track past its TOC length; n = 0 → random first
track, then sequential with wrap). SCSI play `FUN_010084bc` (opcode 0x48, then 0xA5,
then 0x45).

| Situation | Track |
|---|---|
| All front-end menus | 13 |
| Game Over | 16 |
| Level Complete, Game Complete | 8 |
| In game, mode Continuous (and pause menu) | 0 → random, then sequential |
| In game, mode Radio | the vehicle's radio track while in it; stopped on exit |

Music mode `DAT_0114c30c`: 0 Off, 1 Radio, 2 Continuous, 3 Disabled (forced if the CD
thread fails). **No music volume**; FX volume `DAT_0114c2a0` 0..100 step 10 (default
100) scales hardware volume. `extracted/iso` is data-only, so track contents are unknown.


## 5. Intro video — `Videos/intro.video` + `intro.audio`

Decoder: `analysis/intro_video.py` (`python intro_video.py --all` from `analysis/`) →
`analysis/img/intro_frame_0000..0449.png`, `intro_contact.png`, `intro_audio.wav`.
**Verified bit-exact**: decoding the trailing loop frame (index 450) on top of frame 449
reproduces frame 0 with 0/64000 differing pixels, so every chunk in the file decodes
correctly.

Player code is in the 68k build: `FUN_00010364` (68k) loads `PROGDIR:Videos/%s.audio`
whole and builds the `.video` path (= OS4 `FUN_01000978`); `FUN_000106c0` (68k) is the
player (header, timing, frame/chunk loop, palette, per-frame audio); `FUN_0001021c` (68k)
the word-delta decoder; `FUN_000104d4`/`FUN_00010556` (68k) the Warp3D path (frame →
RGB 256×256 texture keeping 4 of every 5 columns, full-screen quad). In the OS4 binary
the player body (≈0x01000e0c–0x01001eb8) is not recognised as a function by Ghidra.

### 5.1 Video = Autodesk Animator Pro FLC, little-endian (CONFIRMED)

128-byte header (the player byte-swaps it):

| Off | Type | Field | Value |
|---|---|---|---|
| 0x00 | u32 | file size | 23,630,325 |
| 0x04 | u16 | magic | 0xAF12 (FLC; 0xAF11 FLI also accepted) |
| 0x06 | u16 | frames | 450 |
| 0x08 | u16 | width | 320 |
| 0x0A | u16 | height | 200 |
| 0x0C | u16 | depth | 8 |
| 0x0E | u16 | flags | 3 |
| 0x10 | u32 | speed | 66 ms/frame (FLI: 1/70 s units) |
| 0x16/0x1E | u32 | created/updated | 951984562 / 951984617 (≈ 2 Mar 2000) |
| 0x1A/0x22 | 4 B | creator/updater | "MERP" (= "PREM", Adobe Premiere — INFERRED) |
| 0x26/0x28 | u16 | aspect | 1/1 |
| 0x50 | u32 | offset frame 1 | 0x80 |
| 0x54 | u32 | offset frame 2 | garbage, ignored |

Frames start at 0x80: 450 shown + 1 ring (loop) frame never shown. Frame header 16 bytes:
`u32 size, u16 0xF1FA, u16 nchunks, 8 reserved`; type 0xF100 (prefix) frames are skipped.
Chunk header `u32 size, u16 type`; next chunk at `(size+1)&~1`.

| Chunk | Count in file | Meaning |
|---|---|---|
| 4 COLOR_256 | 1 (frame 0) | u16 npackets; each: skip byte, count byte (0 = 256), count×RGB (8-bit). Palette is constant for the whole video and replaces the game palette (`LoadRGB32`) |
| 11 COLOR_64 | 0 | same, 6-bit components (<<2) |
| 15 BYTE_RUN | 1 (frame 0) | per row: ignored packet-count byte; then until 320 px: `c<0x80` → repeat next byte c times; else copy `256-c` literals |
| 7 DELTA_FLC | 380 | u16 nlines; per line u16 opcodes: `0xC000..0xFFFF` skip `0x10000-op` lines; `0x8000..0xBFFF` set last pixel of the line to low byte; else = packet count; packet: column-skip byte, signed count: `>0` copy count words, `<0` repeat next word `-count` times |
| 16 COPY | 65 | raw 64,000 bytes |
| 13 BLACK | 0 | fill colour 0 |
| 18 PSTAMP | 0 | ignored |

Unknown chunks print "Unrecognised chunk type %04x" (OS4 string 0x0112e0c4).
Frame period = the header speed (66 ms) converted to E-clock ticks and rounded:
`round(speed_ms·1000 / µs_per_tick)` with IEEE `+ 0.5` (68k `FUN_000106c0`, 0x10bd2..0x10c36;
CONFIRMED). So ≈15.15 fps, matching the 1470-byte audio chunks. Length ≈ 30 s.

### 5.2 Audio = raw signed 8-bit mono PCM (CONFIRMED)

The whole 623,348-byte (0x982F4) file is read into memory. For video frame *n*, if
`n*1470 < 0x97D36` the player plays bytes `[n*1470, n*1470+1470)` on audio slot 4 (aborting
the previous chunk), else stops slot 4 — so frames 0–423 have sound, 424–449 are silent,
and the last 68 bytes are never played. Paula period **163**, volume **63**, one-shot:
21,760 Hz PAL / 21,961 Hz NTSC; nominal rate **22,050 Hz** = 1470 × 15 fps (INFERRED).
The reimplementation should play the file at 22,050 Hz in sync with a 15 fps video.

## 6. `.mini`, `.thumb` and progress bar

### 6.1 `<map>.thumb` — level-select preview (CONFIRMED)

256×256 headerless 8bpp chunky, game palette, **row = map y, column = map x, 2×2 pixels
per map square**, north (−y) at the top (verified: map-0 water squares vs. blue thumb
pixels correlate at r = 0.997; transposed r = 0.03). Pre-rendered top-down picture of
the city. Loaded by `LoadTex` = `FUN_01011698(path)`: allocates 0x10004 bytes, reads
0x10000, stores the W3D texture handle at +0x10000. `FUN_010138dc` loads
`PROGDIR:maps/0.thumb`..`2.thumb` at start-up into `DAT_0117bbec/f0/f4`; user maps load
`"%s.thumb"` in `FUN_01027164`. (Rendering: `analysis/img/thumb0_check.png`.)

### 6.2 `<map>.mini` — in-game minimap (CONFIRMED format; drawing see §2)

65×65 headerless 8bpp (4225 bytes, stride 65), same orientation as `.thumb`, **one
pixel = 2×2 map squares** (correlation with `.thumb` road pixels r ≈ 0.5–0.75 vs. ≈ 0.05
transposed). Only three values occur in all shipped files: **0** transparent, **4** white
(road), **8** black (drop shadow of the road pixels, offset +1,+1). Rendering:
`analysis/img/minis.png`.

Load: end of LoadMap (`FUN_01079300`) builds `"%s.mini"` from the map base name
(`FUN_01009ff8`: `PROGDIR:Maps/0..2` or user-map name) and calls `FUN_010727f0`:

1. `LoadIcon(path, 65, 65, 0x1081)`.
2. Copy into a **64×64** buffer (row 64 and column 64 dropped) at `DAT_0130e254`
   (icon struct `{pixels, 64, 64, w3d}`); a 64×64 W3D texture in hardware mode.
3. Stamp a small marker (colour **0xB0**, shadow **0x08**) at the top centre — pixel
   writes (x,y): (31,0); (30,1),(31,1),(32,1); (29..32,2),(33,2); (29..32,3),(33,3);
   shadow (29..32,4),(33,4),(34,4) (CONFIRMED, `FUN_010727f0` stores at 0x010728c0). As a
   picture (x 29..34, B = 0xB0 pure red, S = 0x08 black):
   ```
   y0  . . B . . .
   y1  . B B B . .
   y2  B B B B B .
   y3  B B B B B .
   y4  S S S S S S
   ```
   A red up-pointing arrowhead with a shadow at the map's north edge, baked into the texture
   (so it turns with the map in rotating view): a north pointer (INFERRED).

### 6.3 Loading progress bar (CONFIRMED)

`InitProgress(name)` = `FUN_01042ff8`: reads `PROGDIR:General/<name>` (a single
big-endian u32 = *units per bar segment*; files `startup.size` 0xB6D29,
`map.size` 0x12179, `map_reload.size` 0xF681, `lighting.size` 0x2F80,
`ReloadObjectTextures.size` 0x3519C), clears a box `(cx-54, cy-22)–(cx+53, cy+23)` and
draws `Loading` at `(cx-46, cy-17)` (`cx,cy` = screen centre `DAT_0114c338/334`).
`DrawProgress(n)` = `FUN_010434a0` → `FUN_01042688`: `acc += n`;
`segments = clamp(acc / unit, 0, 12)`; draws `LoadingPiece` (6×16) for each new segment
*i* at `(cx-46 + 8*i, cy+2)`. `CloseProgress` = `FUN_010434d4`: `acc /= 12`; if it
differs from the file value it **rewrites the .size file** ("Wrote updated file sizes").
Loaders call `DrawProgress(2000)` per icon, `(50000)` per texture, `(30000)` for phrases,
etc. — units are arbitrary "work" counts, self-calibrating.


## 7. Open questions

* Original-CD `CD_Audio_Handler` message format and the CD audio track list
  (`extracted/iso` holds no audio tracks).
* §4.4: the water test "square flags & 0x7F == 4" is probably the top face type (cube +9 = 4
  Reflect), as in the traffic spawner (`FUN_000cf5cc`); not verified.

Resolved (C):
* ~~Scripts playing Heartbeat / ThumpingMusic / CarStart~~ None do (§4.4).
* ~~Pause-menu `FUN_01008cec(0)`~~ The receiver is the "Payback CDDA Thread" (entry
  0x010087a0; no Ghidra function), which waits on signal 0x1000 and switches on
  `DAT_0114c2b4 & 0xF000`: 0x2000 quit, 0x3000 stop, 0x4000+n play track n now
  (`FUN_010084bc`), 0x5000+n the policy at 0x010085cc (low 12 bits 0x64 = stop); bit 0x10000
  only toggles so a repeat counts as new. Policy(n): a track is playing and its TOC length has
  not run out → nothing; it has run out → n > 0 replays n, n = 0 advances to the next track
  (wrapping); nothing playing → n = 0 starts a random track, n > 0 plays n. `FUN_01008cec`
  skips a repeat of the same n within one second. So command 0 = "keep music running"; the
  pause loop `FUN_0101a7dc` polls it in any mode ≠ 0, the main loop only in mode 2 (side
  effect, I: pausing in Radio mode on foot starts a random track).
* ~~View intro~~ Update 7 and the CD release play it (menu and startup); only OS4 refuses (§3.3).
* ~~Config fields~~ §3.7.
* ~~Version-3 replay records~~ Same bit layout as version 10: CD encoder at file 0x4773c..0x477de,
  decoder 0x473cc (`(w&0xF8)>>3` weapon, 10 = none, 0xFFFF = end, `w&7` ticks), reader 0x7a7a0,
  comment skip 0x7a7d2 (bit 15 as `FUN_010bbcf4`). Only the header differs.
* ~~Unused assets~~ `Icons/Siren` is the CD release's wanted star (§1.2); `Cross.Obj` (option
  model 16) is the spinning object on the Game Over screen `FUN_01021434` (z 0x898).
