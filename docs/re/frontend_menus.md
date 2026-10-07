# Payback front end, menus, saves, replays, languages: report (AmigaOS4 PPC build unless stated)

The front-end flow is fully mapped, and both save formats are decoded: save slots and the config file. The two shipped replays decode cleanly but cannot be played by the build we target. They are version 3, an older format than Update 7 / OS4 accept.

Two things in the brief were wrong:
- `FUN_01009b48` is not InitLanguages. It scans the replays directory. InitLanguages is `FUN_0101345c`.
- `DAT_0114c2ec` is police aggressiveness (0..5), not the city.

Open items are listed at the end.

**Tags:** C = CONFIRMED (code address), I = INFERRED.

**Conventions:**
- Screen size is W = `DAT_0114c348`, H = `DAT_0114c344`.
- The layout uses a virtual 320×256 grid. Scale factors are `xs = DAT_0114c328 = (W+8)*16/320` and `ys = DAT_0114c324 = (H+8)*16/256`, so a virtual coordinate maps as `v*xs>>4` (C `01041510` area, around decompiled.c line 34268).
- `cx = DAT_0114c330`, `cy = DAT_0114c32c`.
- Hi-res flag `DAT_01174e88 = 1` when W ≥ 640 and H ≥ 400 (C `010138dc`).
- Many positions are given as N/200 of H.
- `FUN_01072790(n)` returns phrase n.
- Menu sounds: `FUN_010053c4(0x20,0xeb)` on move, `(0x21,0x10e)` on select, `(0x2d,0xa0)` on refuse/error (C).
- Input struct from `FUN_0103f3d0`, shorts: +0 left, +2 right, +4 up, +6 down, +8 fire, +10 Esc.

## 1. Boot and overall flow (C: `FUN_010692bc` main loop)

1. **Config.** Default key maps are set, then `S:Payback.config` is loaded (`FUN_0104199c`). The screen is set up (`FUN_0104feb0`), which loads the progress-panel icons `icons/Loading` and `LoadingPiece`.
2. **Keyfile.** The path `PROGDIR:General/Payback.key2` is stored in the binary obfuscated as each byte +1 at `0x01133b34` (C). The game reads 10 bytes and checks five XOR-ed u16 values.
   - Registered: `DAT_0114c680 = 1`, full main menu `FUN_0102fc44`.
   - Missing keyfile: key entry screen `FUN_0101dfe4` (phrases 2190–2194).
   - Unregistered: demo menu `FUN_01031e24`, demo text `FUN_0103141c` (2470–2472).
   - In unregistered builds the game stops after 9000 ticks (5 minutes at 30 Hz) with "Time out!" 2490. This applies in every mode (C around line 49830).
3. **Front-end init `FUN_010138dc`** runs: `FUN_01009b48` (scan replays), `FUN_0100a1d4` InitUserMap, all icons and option objects, the thumbnails, then `FUN_0101345c` InitLanguages, then `FUN_010726dc("global")`, then reads the 14 save-slot names.
4. **Main-menu return value v:** v ≥ 0 means city = v/2 (`DAT_0114c694`) and level = v%2 (`DAT_0114c558`), then LoadMap. −1 means quit. −2 means play a replay (`FUN_0104135c`). With multiplayer active (`DAT_0114c7c8`) it returns `DAT_0114c7c0*2`, the multiplayer city.
5. **After a level** (`DAT_0114c730`):
   - 3 → Level Complete `FUN_0102395c(city, level, minutes)`. This calls Game Complete `FUN_01022348` once all 6 levels have a cup.
   - 2 → Game Over `FUN_01021434(input, minutes)`.
   - Multiplayer → Results `FUN_01027330`.
   - Then main menu again. `DAT_0118ff28 = 1` makes it skip straight into the Game Menu `FUN_0102ca08(input, back, p3)`, with p3 = 1 when `DAT_0114c8a8 < 2` (I: replay state).

**Title screen / main-menu entry (C `0102fc44`):**
- There is no separate "press fire" title. The title picture slides in from y = −H to 0 over 26 frames (`FUN_0100ab78(H*i/25)`, i = −25..0) while fading from black to `0x1000000` (`FUN_0104138c`).
- On exit the reverse happens: it slides down and fades out over 26 frames.
- The first time, if no CD device is configured (`DAT_0114c7e0 == 0`), the CD Audio Setup `FUN_0102dd30` runs first.

**Title images** (`FUN_010138dc`, C):

| File | Used when | Format |
|---|---|---|
| `TitleScreen` | H ≤ 200 | 320×200 8bpp |
| `TitleScreen.medium` | ≤ 320×256 | 320×256 |
| `TitleScreen.big` | larger screens | 640×480 |
| `TitleScreen.w3d` | Warp3D | 256×256 24-bit RGB (0x30000 bytes) |

- The chosen image is scaled to the screen with `FUN_0100d9c0`.
- A darkened copy `DAT_0117b6b4` is the background for sub-screens: each pixel goes through the remap table `FUN_0104206c()+0x40000`.

**CD music** (`FUN_01008cec(track)` sends command 0x5000+track; C):
- Menus use track 13 (0xd).
- Level complete uses 8; game complete 8; game over 16 (0x10).
- The pause menu calls it with 0 (meaning not traced).
- It is only active when CD mode `DAT_0114c30c ≠ 0`.

## 2. Option 3D objects and how they are drawn (C)

**Model table `DAT_0117b6f8[17]`, built in `010138dc`:**

| Index | Model | Used for |
|---|---|---|
| 0 | joystick | Start new game |
| 1 | disk | Load saved game |
| 2 | moviereel | View saved replay |
| 3 | cup_silver | Credits |
| 4 | cd | CD audio setup |
| 5 | controlsetup | Control setup |
| 6 | world | Set Language |
| 7 | moviereel | View intro |
| 8 | amiga | Quit |
| 9 (`b71c`) | joystick, or a cup | Play single player game |
| 10 | joystick_multi | Start multiplayer game |
| 11 | joystick | Single player rampage |
| 12 | disk | Save game |
| 13 | moviereel | View replay |
| 14 | moviereel | Save replay |
| 15 | amiga | End game |
| 16 | cross | loaded, use not found |

Index 9 becomes cup_bronze/silver/gold (`DAT_0117bbf8[rating−1]`) once an overall rating exists.

**Object instance and drawing:**
- Instance created by `FUN_010c0c1c(model, x, y, heading)`, which allocates 0x4b8 bytes.
- Fields: [0] x, [1] y, [2] z (default 0x4dc0), [3]/[4]/[5] rotation angles, [9] model pointer.
- Angles are in units of 0x1680 = 5760 per full turn.
- Drawn with `FUN_010c101c(obj, 0, 0, 0, 0x65)`.

**Menu object constants (`0102fc44`, `0102ca08`):**
- One object at x = 0, y = 600, z = 6000.
- Each frame: [3] −= 0x20, [4] += 0x40, [5] −= 0x10, all modulo 0x1680. That is 180, 90 and 360 frames per turn.
- Changing item: the object slides out at ±0x5dc (1500) per frame. Past ±0x1194 (4500) the model is swapped and it re-enters from the other side to x = 0.
- The pause menu's malenormal person slides at 0x2ee and swaps at 0x8ca.

**Item label and arrows:**
- Label is centred at y = 70/200 (`FUN_0100bb94`, colour 1).
- LeftArrow at x = cx − (xs·0x60>>4) − 14 (28 in hi-res); RightArrow at cx + (xs·0x60>>4); same y.
- Screen title (big font) at y = H/40.
- Selection is shown by the label of the current item; lists flash the selected line on a 4-frame cycle (two colours).
- Arrow icons are loaded with the shadow loader `FUN_01008ed4`.

**Other icon uses:**
- `icons/cursor` (6×10) is the text-entry caret (`FUN_01012e38`, y = 0x5f virtual).
- Level complete shows a rotating cup at z = 4000 (`0102395c`).

## 3. Menus, items and phrases (C)

**Main menu `FUN_0102fc44`.** 9 items, wrapping left/right. Title 2085 "MAIN MENU". Item i shows phrase 2090+i except where noted; index = model index.
- 0 "Start new game" (2090): name entry 2088 ("ENTER NAME", default "Travis", 25 characters, `FUN_01012e38`). Resets best times to −1 and cups to 0, then goes to the Game Menu.
- 1 "Load saved game" (2091): `FUN_0101f1a4`, then Game Menu.
- 2 "View saved replay" (2092): `FUN_0101fdcc`, returns −2.
- 3 "Credits" (2093): `FUN_010206bc`.
- 4 "CD audio setup" (2094): `FUN_0102dd30`.
- 5 "Control setup" (2454): `FUN_0102e75c`.
- 6 "Set Language" (2132): `FUN_0101267c`, then reloads global phrases.
- 7 "View intro" (2109): only plays the refuse sound in this build.
- 8 "Quit game" (2130): "Are you sure?" 2070 with Yes 2071 / No 2072.

**Game menu `FUN_0102ca08`.** 7 items. Title 2086 "GAME MENU".
- 0 "Play single player game" (2134): goes to Choose Level `FUN_01025544`. Once all 6 levels have a cup it also shows "GAME COMPLETE!" 2047 and the overall rating 2073–2075.
- 1 "Start multiplayer game" (2133): `FUN_0102b2c8`.
- 2 "Single player rampage" (2197): `FUN_0102a428`, sets `DAT_0114c7c6 = 1`.
- 3 "Save game" (2096): `FUN_0101f738`.
- 4 "View replay" (2097): `FUN_0101e618`.
- 5 "Save replay" (2098): "ENTER FILENAME" 2084, 30 characters, default `"<name> - "`. Then `FUN_010bc3b0` (save replay), rescans the replay list and shows 2117 "REPLAY SAVED!".
- 6 "End game" (2099): Are you sure → returns −1.
- Items 4 and 5 are skipped when p3 == 0, i.e. no game has been played yet.
- When p3 ≠ 0 and Rampage was just played, high-score entry runs first (see §7).

**In-game pause menu `FUN_0101a7dc`.** Models `DAT_0117c944[i]`, loaded by `FUN_0101a55c`. The background is darkened with `FadeDarken3` over the box 50..269 × 26..229 (virtual).

| i | Model | Phrase | Behaviour |
|---|---|---|---|
| 0 | joystick | "Return to game" 2050 | Shows "Goal: Escape from jail" 2056 or "Goal: Get N points" (2058 + N + 2057), and "Time elapsed" 2059 as h:mm |
| 1 | amiga | "Quit to title screen" 2051 | Yes/No |
| 2 | torch | "FX: " 2052 | 2060 Off / 2061 Flares only / 2062 Flares and artifacts (`FUN_01035954`/`60`, cycles 0..2) |
| 3 | malenormal.obj | "Motion blur:" 2053 | On/Off; refused under hardware rendering |
| 4 | none | "Set sound volume" 2054 | Volume 0..100 in steps of 10 (`FUN_01006558`/`64`), drawn as a 10-piece LoadingPiece bar |
| 5 | questionmarks | "Graphics options" 1600 | Sub-items "Gouraud shading" 1601 (`DAT_0114c2fc`) and "Rotating view" 1602 (`DAT_0114c7dc`, needs `DAT_0114c2f4`); 1603 is not used |

**Credits `FUN_010206bc(input, back, first=2200, speed=2, skippable=1, stopY=0x2e, hold=100)`.**
- The demo nag uses (2400, 3, 0, 0x46, 0x96).
- Line markup:

| Prefix | Meaning |
|---|---|
| ' ' | big, centred |
| '#' | left, blue |
| '@' | right, red |
| '$' | centred, blue |
| '%' | centred, red |
| '?' | PaybackLogo (166×80), centred |
| 0xA3 | gap of half the screen height |
| '!' | end |

**Other screens:**
- Message box `FUN_0101dc60(input, back, phrase)`, with "Continue" 2110.
- Text entry `FUN_01012e38(prompt, buffer, maxlen, back, input)` returns 1 on Enter and 0 on Esc, with hint 2119.
  - Allowed characters: letters (folded), digits, space `! ( ) , - . : ; _`. The mask is 0x0fff7303 over 0x20..0x3b.

## 4. Single-player level select `FUN_01025544` (C)

**Carousel:**
- The three city thumbnails sit on a virtual ring 320 wide, at scroll offset +0x35, +0xa0 and +0x10b, i.e. 107 (0x6b) apart.
- They scroll 1 unit per step, up to 8 per frame, until the offset is 0, 0x6b or 0xd6.
- Each thumbnail is drawn by `FUN_010c3e10(tex, x*xs>>4, cy − 25*ys>>4, r, 0xe10)`. That draws a textured quad with corners at angle 0xe10 + k·0x5a0, i.e. an axis-aligned square.
- The radius is r = max(20, 100 − |x−160|), so the centred city is largest.
- On confirm, a zoom animation counter runs 200 → 0, then fades.

**Text and table:**
- City name 2170+c at y = 0xb7 (virtual); title 2087 "CHOOSE LEVEL".
- For each level: level name 2180 + 2c + l at x = 0x14.
- Gold cup icon plus gold time at x = 0x96/0xa6.
- Silver cup icon plus silver time at x = 0xcd/0xdd.
- The player's cup icon (`cup.none/bronze/silver/gold`, `DAT_0117bba8+cup*0x10`) plus best time ("-:--" if −1) at x = 0x104/0x114.
- Rows at y = 199 and 221; the level cursor is a RightArrow at x = 6.
- Up/down picks the level; left/right picks the city.
- A level is refused (refuse sound) if the previous level's cup is 0. The default selection is the first level without a cup.
- Returns city*2 + level, and also sets `DAT_0117f898` = city, `DAT_0117f89c` = level.

**Target-time table `DAT_0117b6c8[city][level] = {gold, silver}`, in minutes:**
- Initialised in `010138dc` and identical in the 68k build, `FUN_0001fcf8`.
- The time passed in is `DAT_0114c738/0x708`. `DAT_0114c738` counts 30 Hz ticks, so 1800 ticks = 1 minute (C line 50000; tick rate C ~49810).
- Display format is `"%d:%d"` / `"%d:0%d"`, i.e. h:mm.

| City.level | Level name | Gold (min) | Silver (min) |
|---|---|---|---|
| 0.0 | Ready Or Not | 8 | 12 |
| 0.1 | Gangster Tripping | 10 | 14 |
| 1.0 | Going For Gold | 11 | 15 |
| 1.1 | Remote Control | 8 | 12 |
| 2.0 | Caught By The Fuzz | 4 | 7 |
| 2.1 | Born Slippy | 10 | 15 |

**Cup per level (`FUN_0102395c`, C):**
- time ≤ gold → 3 (2043 "Gold trophy awarded!").
- time ≤ silver → 2 (2042).
- otherwise → 1 (2041).
- The best time and cup are stored only if the time beats the old best or the old best is −1.
- **Overall rating = min(cup over the 6 levels, capped at 3).** Game Complete shows 2047/2048, the rating 2072+r, and the hint 2075+r (2076/2077/2078).

## 5. Save games (C `FUN_0101182c` load, `FUN_01011a70` save, `FUN_01011cc8` exists, `FUN_01011da8` slot name)

- **Path:** `PROGDIR:Saves/PaybackSlot%d.save`, falling back to `S:PaybackSlot%d.save`. Slots 1..14.
- **Layout:** 152 bytes, big-endian.

| Offset | Size | Field |
|---|---|---|
| 0x00 | u32 | version, always written as 1 (not checked on load) |
| 0x04 | char[50] | slot description |
| 0x36 | char[50] | player name (`DAT_0117aeac`) |
| 0x68 | 3 × 16 bytes | for city c: best time [2c], cup [2c], best time [2c+1], cup [2c+1] |

- Best times live in `DAT_0117aeb0[6]` as s32 minutes, −1 meaning none.
- Cups live in `DAT_0117aec8[6]` as s32: 0 none, 1 bronze, 2 silver, 3 gold.

**Load screen `FUN_0101f1a4`:**
- Title 2082 "LOAD GAME"; 14 slot lines at virtual y = 0x20 + 14i; "Cancel" 2114 at 0xeb.
- Results: 2111 "GAME LOADED!", or 2112 "SLOT EMPTY!" with the refuse sound.
- Slot names come from `DAT_0117bc04[15]`; an empty slot shows "Slot %d", and index 14 is "Cancel".

**Save screen `FUN_0101f738`:**
- Title 2089. For an empty slot the description defaults to `"%s - C%d, L%d"`: name, city+1, level 1 or 2 of the first level without a cup.
- Then edited with 2084 (max 40 characters), saved, and 2083 "GAME SAVED!" shown.

## 6. Config `S:Payback.config` (C load `FUN_0104199c`, save `FUN_01041510`)

The config is mostly written by `PaybackSetup`. Screen modes and resolutions are chosen there, not in game (I). Layout, all u32 big-endian unless noted:

| # | Field | Notes |
|---|---|---|
| 1 | version | 10; < 10 gives "Config file too old" |
| 2 | `DAT_0114c72c` | I: screen-mode related |
| 3 | lens-flare FX mode | 0..2 |
| 4 | char[40] CD device name | `DAT_0114c7e0` |
| 5 | CD unit | `DAT_0114c308`, default −1 |
| 6 | CD playback mode | `DAT_0114c30c`: 0 Off 2142, 1 Radio 2143, 2 Continuous 2144, 3 Disabled 2145 (default 3) |
| 7 | `DAT_0114c304` | screen mode ID (used by `0104feb0`) |
| 8 | char[40] language name | `DAT_012a63d4`; empty means ask at start |
| 9 | 4 × {device `DAT_0114b59c[p]`, key set `DAT_0114b5ac[p]`} | per multiplayer player |
| 10 | required kills | `DAT_0114c2e8` |
| 11 | multiplayer/rampage city | `DAT_0114c7c0` |
| 12 | `DAT_0114c310` | set to 0x100 under hardware rendering |
| 13 | police aggressiveness | `DAT_0114c2ec`, 0..5 |
| 14 | 11 × 11 key codes | `DAT_012a63fc` |
| 15 | volume | 0..100 |
| 16 | `DAT_0114c300` | second screen mode ID |
| 17 | Gouraud | `DAT_0114c2fc` |
| 18 | rotating view | `DAT_0114c7dc` |
| 19 | hardware (Warp3D) | `DAT_0114c7d8` |

Total 656 bytes.

**Key tables:** 11 actions per table, in the order of phrases 2457–2467: accelerate, reverse/brake, left, right, shoot, shoot alt, in/out, brake only, next weapon, prev weapon, repeat message.
- Table 0 (keyboard), Amiga raw keys: 4C 4D 4F 4E 40 00 44 31 5F 46 37 (up, down, left, right, Space, –, Return, Z, Help, Del, M).
- Odd tables 1,3,5,7,9 (PSX): 106 109 100 102 107 10C 108 10D 10B 10A 104.
- Even tables 2,4,6,8,10 (CD32): 11D 11E 118 11A 11F 0 120 122 121 0 0 (C; 68k `FUN_000ec686` builds the same alternating loop).

**Control setup screen `FUN_0102e75c`:**
- Title 2469. 11 columns, scrolled horizontally with easing `p = (7p + target) >> 3`.
- Columns 0..2: "Single player" 2455 with Keyboard 2167 / PSX 2164 / CD32 2165.
- Columns 3..10: "Multiplayer" 2456 with PSX/CD32 for (k−3)>>1.
- Action rows at y = (50 + 10i)/200, key names from `FUN_0103c690` (phrases 1500–1534). Redefine prompt 2468 (press 0 to disable). Done 2148 / Cancel 2192 at 180/200.

**CD Audio Setup `FUN_0102dd30`:**
- Title 2131. "Device:" 2140 + name (2138 if unknown) and "Unit:" 2141 (2139 if unknown) at H/4.
- Playback mode 2142+mode at y = 80/200, "Done" 2148 at 170/200.
- Entry prompts 2136/2137 (`FUN_0101d0b4`). The CDDA thread is `FUN_01008b10`, open is `FUN_010089dc(device, unit)`.

## 7. Multiplayer, Rampage, high scores (from the sub-agent; C unless marked)

**Multiplayer setup `FUN_0102b2c8`.** Title 2135.
- Rows 0/2/4/6 "Player k: device", at y = 30k/200.
- Device codes: −1 Off 2168; 0–3 Joystick n 2166; 4–7 PSX n 2164; 8–11 CD32 n 2165 (via lowlevel ReadJoyPort); 12 Keyboard 2167. Defaults P1 = 0, P2 = 12, P3/P4 = −1. Each device can be used by one player only.
- "CPU" 2169 is never used.
- Rows 1/3/5/7: key-set hint 2175–2178 for joystick players, one set per player.
- Row 8: City (2124 + 2170–2172, a user-map name, or 2039 "Not found!") at 149/200. Cities 1 and 2 are only offered if the previous city's first level has a cup (I).
- Row 9: "Required kills" 2329 (1..50, default 5) at 161/200.
- Row 10: "Play" 2095 at 173/200. It needs at least 2 active players, else 2179 is shown. It returns city*2 and sets `DAT_0114c7c0`, `DAT_0114c7c8 = 1` and `DAT_0114c2e4` = player count.
- Row 11: "Cancel" at 185/200.
- **Game types:** deathmatch is the only multiplayer mode. The replays named "Race…" and "Tanks…" are single-player recordings.

**Split screen (`FUN_0105bd98` at `0105c4a0`):**
- Up to 2 players: side by side, each view W/2−1 wide with a 1 px separator.
- 3–4 players: quadrants of height H/2−1. With 3 players the bottom view is centred.
- HUD per view: "Kill(s):" 2189.
- Kills are +1 per kill and −1 per suicide.
- When exactly one player reaches the target that player wins; several at once is a draw.

**Results `FUN_01027330(result, input)`:**
- Header: 2173 "Player %d won the match!", 2128 draw (−1), 2174 aborted (−2), nothing after a replay (−3).
- Title 2129 "RESULTS". Columns Won 2161 / Drawn 2163 / Lost 2162. Totals keep adding up until the setup screen is opened again.
- Menu: City, Play again 2198, View replay, Save replay, End game.

**Rampage setup `FUN_0102a428`:**
- Title 2196, "Top 5 high scores:" 1610.
- Rows: City at 130/200; Police aggressiveness 2055 + 2491+level at 145/200 (default 2); Play; Cancel.
- Score `DAT_0114c420`: civilian kill 1500×mult, cop kill 2500×mult, and every 11th cop kill +100000 ("Copkiller bonus!").
- Play ends at 0 of 3 lives ("Game over!" 2026). There is no time limit.

**High scores (`FUN_01029b3c` load, `FUN_0102a0fc` save):**
- File `PROGDIR:Maps/N.scores` for the built-in cities, or `<usermap>.scores`.
- 6 tables (one per police level) × 5 entries × {char name[30], s32 score} = 1020 bytes, then a u32 checksum = ~(0x5DC1DCD1 XOR 7 big-endian words of each name XOR all scores). Total 1024 bytes.
- In memory: scores `DAT_0118f8d4[6][5]`, names `DAT_0118f94c[6][5][50]`.
- If the file is missing or the checksum fails, defaults are used: name "Travis", score 500000·(table+1) for the top entry, halving down the list.
- A new score is entered after Rampage at the top of `0102ca08`: prompt 1611, then saved.

## 8. Replays (from the sub-agent; decoder `analysis/scratch/replay_decode.py`, run from the repo root)

**Code:**
- `FUN_01009b48` lists `PROGDIR:replays/#?` into `DAT_0116e8c4[]` (count `DAT_0116e8c0`, max 2000). It reuses the "AddMovie" and "InitLanguages" debug strings by copy-paste.
- `FUN_010bc000` = load, `FUN_010bc3b0` = save, both on `PROGDIR:Replays/%s`.
- Pick list `FUN_0101fdcc` ("SELECT REPLAY" 2113).
- Info screen `FUN_0101e618`: shows 2122–2126 and 2149; the frame rate shown = words·30/(players·ticks).

**Header (big-endian):**
- OS4 version 0x3F2 and 68k Update 7 version 10, 152 bytes: version, number of players, controller code[4], required kills, record count, duration in ticks, random seed, rampage flag, police level, char map path[100], level-within-city. Records start at 0x98.
- Version 3 (CD release `Payback.exe`, load code at file offset 0x7a9e2), 144 bytes: no rampage or police fields; the level code is a single field before the map path. Records start at 0x90.

**Records** (`FUN_01051a10`): one u16 per active player per frame, no compression.

| Bit(s) | Meaning |
|---|---|
| 15 | a NUL-terminated comment follows (padded to even length) |
| 14 | turn left |
| 13 | turn right |
| 12 | accelerate |
| 11 | reverse/brake |
| 10 | shoot |
| 9 | in/out of vehicle |
| 8 | brake only |
| 7..3 | weapon selected this frame, 10 = no change |
| 2..0 | 30 Hz ticks in that frame (1..5) |

- The recording buffer is 150,000 bytes and silently stops filling when full.
- Playback reuses the stored seed and the per-frame tick counts. It ends when the reader returns 0xFFFF (data exhausted) or Esc is pressed.

**Shipped files — the version is not supported:** both are version 3. The OS4 and Update 7 loaders would reject them with "WRONG VERSION NUMBER" (2116).

| File | Map | City / level | Records | Ticks | Duration | Seed |
|---|---|---|---|---|---|---|
| Race for the prize | `PROGDIR:Maps/1` | 1, level code 2 | 1211 | 5337 | 2:57.9 | 26009 |
| Tanks a lot | `PROGDIR:Maps/0` | 0, level code 0 | 1650 | 7586 | 4:12.9 | 412 |

Both are single-player. In each, the tick values add up exactly to the header duration and no bytes are left over.

## 9. Languages

**InitLanguages `FUN_0101345c` (C):**
- Scans `Phrases/Global.#?.phrases` and keeps the middle token of each name (between the dots).
- Up to 500 languages in `DAT_0117aee4[]` (count `DAT_0117aee0`); `AddLanguage` is `FUN_01012438`.
- If the configured language `DAT_012a63d4` is empty, it runs the chooser.

**Chooser `FUN_0101267c` (C):**
- Title "Please choose your language" is hard-coded in English.
- A vertical list scrolled with easing `y = (16·target + 8·old)/9`; the selected entry flashes.
- On select: the name is copied to `DAT_012a63d4` and `FUN_0103da68` is called (I: input reset). The config is written by `FUN_01041510`.
- The index lookup `FUN_01012504` falls back to "English".
- Phrases are reloaded with `FUN_0107234c("global")`. Level phrases load from LoadMap using the "%d%d" level name (e.g. "00"), except in Rampage and multiplayer.

## 10. Loading / progress bar (C)

**OS4 functions:**
- InitProgress(name) `FUN_01042ff8`: reads a u32 "expected units" from `PROGDIR:General/<name>`. It clears a box (cx−0x36, cy−0x16)–(cx+0x35, cy+0x17), with cx = (W−1)/2 and cy = (H−1)/2, and draws `icons/Loading` (94×35, stride 96; it already shows the empty slots) at (cx−0x2e, cy−0x11).
- DrawProgress(n) `FUN_010434a0` → `FUN_01042688`: adds n to the total. Pieces = min(12, total/expected). It draws `LoadingPiece` (6×16) at x = cx−0x2e+8i, y = cy+2.
- CloseProgress `FUN_010434d4`: writes total/12 back to the file if it changed, so the bar calibrates itself.

**Size files:**

| File | Value |
|---|---|
| `startup.size` | 0xB6D29 |
| `map.size` | 0x12179 |
| `map_reload.size` | 0xF681 |
| `lighting.size` | 0x2F80 |
| `ReloadObjectTextures.size` | 0x3519C |

- LoadMap calls DrawProgress(50000) and (30000).
- `FUN_0100b288(x, y, filled, total)` is a separate segment-bar helper (`LoadingPiece2` = empty piece) with no callers in OS4. The pause-menu volume bar draws its own 10 pieces inline.

## 11. `.thumb`, `.mini`, icon format (C)

**`.thumb`:**
- 256×256 8bpp, raw, 65536 bytes; a top-down picture of the city.
- `maps/0..2.thumb` are preloaded by `LoadTex` `FUN_01011698` (64 KB + a texture handle at +0x10000).
- `LoadCityPreview(n)` `FUN_01027164`: n < 3 → `PROGDIR:Maps/n.thumb`; otherwise user map `DAT_01170808[n−3]` + ".thumb" into `DAT_0117f8d0`. Used by the multiplayer and Rampage city rows.

**User maps (`FUN_0100a1d4` / `FUN_01009e88`):**
- `UserMaps/*.map`, skipping `test.map`, max 2000.
- Paths in `DAT_01170808`, display names in `DAT_01172748`, count `DAT_01170804`.

**`.mini`:**
- 65×65 8bpp. Loaded at the end of LoadMap as `FUN_010727f0("<map>.mini")`.
- Cropped to 64×64. A few pixels near (0x1d..0x22, rows 0..4) are overwritten with colours 0xB0 and 0x08 (I: a north/marker glyph).
- Stored at `DAT_0130e254` as the HUD radar texture (I).

**Icons:**
- Raw 8bpp chunky; the row stride is the width rounded up to 16. Examples: cursor 6×10 is a 160-byte file; cup icons 15×20 are 320 bytes; PaybackLogo 166×80 is 14080 bytes.
- Loader `FUN_0100d3c4(dst, path, w, h, bytes)`.

## Open items
- Which action the pause-menu call `FUN_01008cec(0)` performs (pause the CD?).
- What "View intro" was meant to do; in this build it only plays the refuse sound.
- Exact meaning of `DAT_0114c72c` and `DAT_0114c304`/`DAT_0114c300` in the config, which PaybackSetup writes.
- What the main menu does after "Play again" in multiplayer (I: the next match starts straight away).
- Whether the version-3 replay records use the same bit layout (I: the tick totals match).
- Names for weapon-select values 0..7.
- The `.mini` overlay glyph.
- Where the 68k build draws its startup Loading screen (it loads the icons in `FUN_000ec686`).

Scripts are in `D:\Payback Reverse Engineered\analysis\`:
- `fn.sh` / `where.sh` (decompile helpers)
- `scratch\ppcdis.py` (PPC disassembly)
- `scratch\findbl.py` (call-site scan)
- `scratch\phr_by_fn.py` (phrases used per function)
- `scratch\replay_decode.py` (replay decoder)
- `scratch\fe_imgs.py` (renders the images)

Rendered front-end images are in `D:\Payback Reverse Engineered\analysis\img\frontend\`: the four title screens, `0..2.thumb`, `0..2.mini`, Loading, the pieces, the logo, the cursor and the arrows.