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
| `Siren.1`–`.10` | 9×13 | 16 | 208 | 10-frame spinning star (wanted level). `Siren` (no suffix, a large star) is shipped but not loaded |
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
  red/yellow/blue/green buttons (`analysis/img/icons/sheet_you_c.png`; meaning INFERRED
  from the pictures).
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

<!-- SECTIONS -->

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
Frame period passed to the timer = speed×1000 µs × 0.5 (CONFIRMED); actual pacing is
≈15 fps, locked to the audio chunks (INFERRED). Length ≈ 30 s.

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
   shadow (29..32,4),(33,4),(34,4). INFERRED: a north pointer / player arrow.

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

