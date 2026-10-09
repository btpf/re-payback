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
   - Then main menu again. `DAT_0118ff28 = 1` makes it skip straight into the Game Menu `FUN_0102ca08(input, back, p3)`, with p3 = 1 when `DAT_0114c8a8 < 2`: `DAT_0114c8a8` is the recorded game's player count (`FUN_010bba1c`: 1, or `DAT_0114c2e4` in multiplayer), and p3 lets the Game Menu offer the Rampage high-score entry (CONFIRMED, Mac `DoGameMenu` @ 100a05e0 called with `GetNumPlayersMovie() < 2`).

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
- The pause menu calls it with 0: "keep music running" (C). The receiver is the "Payback CDDA Thread" (entry 0x010087a0, from the start tags at 0x0112ee18; no Ghidra function). It waits on signal 0x1000 and switches on `DAT_0114c2b4 & 0xF000`: 0x2000 quit, 0x3000 stop, 0x4000+n play track n now (`FUN_010084bc`), 0x5000+n the policy at 0x010085cc (low 12 bits 0x64 = stop). Bit 0x10000 only toggles, so a repeated command counts as new.
- Policy(n) (C): a track is playing and its TOC length (start + frames/75) has not run out → nothing, whatever n is; it has run out → n > 0 replays n, n = 0 moves to the next track (the last wraps to the first); nothing playing → n = 0 starts a random track in first..last, n > 0 plays n. `FUN_01008cec` skips a repeat of the same n within one second (clock/50).
- The main loop polls the policy only in mode 2 (Continuous); the pause loop `FUN_0101a7dc` in any mode ≠ 0. Side effect (I): pausing in Radio mode on foot starts a random track.
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
| 16 | cross | the spinning object on the Game Over screen `FUN_01021434` (z 0x898) |

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
- 7 "View intro" (2109): only plays the refuse sound in the OS4 build (its video code `FUN_01000978`, player ~0x01000e10, has no callers). Update 7 68k plays it (C): main menu `FUN_00033192` item 7 → `FUN_00010364("intro")`, `FUN_000106c0` (play), `FUN_00011402` (cleanup); refused only if the files are in neither `PROGDIR:Videos` nor `PaybackCD:Payback/Videos`. 68k `FUN_000ec686` also plays it automatically at the end of the startup load (call at 0xeda52, after CubeTextures, before CloseProgress). The CD release does the same: startup call at file offset 0x65caa, menu item index 6 at 0x14b70, loader at 0x894c0 (offsets into `extracted/iso/Payback/Payback.exe`).
- 8 "Quit game" (2130): "Are you sure?" 2070 with Yes 2071 / No 2072.

Mac release changes (CONFIRMED, docs/re/mac_version.md; `analysis/mac/frontend.md` §3–4), adopted in the port:
- "View intro" plays the film (`DoTitleScreen` @ 100a2e70 → `OpenAnimation("intro")`), stopping the CD music first unless the mode is Disabled; refuse sound when the film is missing.
- Control setup (`DoControlSetup` @ 100a1f80): 5 columns, "Single player" Keyboard / Device 1 and "Multiplayer" Device 2..4 (no PSX/CD32), selection 0..54, Done 55, Cancel 56. F1 (key code 0x7A) is refused when rebinding and `P` is allowed. A `controlsetup` model (model 5) spins behind the screen: `CreateObject(model, 0, 600, 0)`, z = 6000, the option-object spin (−0x20, +0x40, −0x10), y halved every frame, z += v with v += 2 while z ≤ 6000 else v −= 2 (v from 100). Port: the Mac's 5 columns first (gamepad tables per pad, pad 1 shared by single player and multiplayer), then the 4 multiplayer keyboard key sets.
- CD audio setup, renamed "Music setup" in the Mac phrases (2094 / 2131, labels 2138–2145 indented so the colons line up; `DoCDAudioSetup` @ 100a1500): labels from x = 30/320 at y = 50, 61 and 80/200, Done centred at 170/200, starting on Done; the `cd` model (model 4) bobs behind it as above. Disabled (no CD) is drawn dimmed and skipped. Port: an added row "One-shot cut : On/Off" at 91/200 (`Variants::oneshot_cut`).
- Rampage setup: 2055 reads "Difficulty:" in the Mac phrases. Port: an added row "Random missions: On/Off" at 0xa0/200 (`Variants::random_missions`, default on), with Play and Cancel moved to 0xb0 and 0xbf.
- Multiplayer devices (`DoMultiplayerSetup` @ 1009f5b0, `GetNextFreeControl1` @ 1009c730, `ControlMethodExists` @ 100f8420): −1 Off, 0..3 Device n (only when HID input exists), 12 Keyboard, each used once. Port: Off, Joystick 1..4 (only while a gamepad is connected), Keyboard (first free key set).
- Defaults (`ClearPreference` @ 100508c0): music Radio, volume 50, player 1 Keyboard, player 2 device 1, FX 2, Gouraud on.
- Settings are saved on quit (`MacHasQuit` @ 1004f560 → `amiga_cleanup` → `SaveConfig` @ 10052830); the port saves them on any exit when they differ from the file.
- Not copied (Mac bugs): `SaveHiscores` @ 1009eaa0 writes uninitialised scores and checksum, so Mac high scores never persist; `UpdateInputStatusNoKeys` @ 100f8090 repeats the last message every poll while the key is held (the port keeps the Amiga's edge trigger).

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

Graphics options details (C, `FUN_0101a7dc`):
- Sub-items 1601 at y = 0x6e/200 and 1602 at 0x79/200, each phrase + On 2081 / Off 2080. Up/down moves the sub-selection `local_5c`.
- Under hardware rendering (`DAT_0114c7d8 != 0`) the sub-selection starts on "Rotating view" (`local_5c = DAT_0114c7d8 != 0`) and moving up from it leaves the sub-items, so Gouraud cannot be selected.
- Gouraud is drawn dimmed with `FUN_0100cc78` (each glyph pixel through the 50 % blend with colour 0, `alpha + 0x40000`) when `DAT_0114c2f8 == 0` (Gouraud not available) or under hardware rendering; otherwise with `FUN_0100bb94`.
- Turning Gouraud on when `DAT_0114c2f8 == 0` is refused (sound 0x2d). On: `FUN_01042060`, `FUN_010792dc` (relight). Off: `FUN_01042060`, `FUN_01074cbc`, `FUN_01079c54`.
- Rotating view can only be turned on when `DAT_0114c2f4 != 0`, else refused.
- Item 3 (Motion blur): under hardware rendering the toggle is refused (sound 0x2d); otherwise `FUN_01042308`.
- LoadConfig `FUN_0104199c` forces Gouraud on (`DAT_0114c2fc = 1`, `DAT_0114c310 = 0x100`) when field 19 (hardware) is set.

Reimplementation (`crates/payback/src/frontend/display.rs`). The original chose the renderer and the screen mode in PaybackSetup; here they are menu items:
- The pause menu's items 0..5 and their order are the original's. Item 5 keeps the original's two sub-items, 1601 Gouraud shading at 0x6e/200 and 1602 Rotating view at 0x79/200, and adds a third line, "More graphics options" (English, no phrase), 11/200 below. Fire steps Gouraud or Rotating view; fire on the added line opens the Graphics options screen below (the same item list, labels, greying and scrolling as the front end's), drawn like the pause menu: over the frozen game picture with a widened FadeDarken3 panel (under hardware rendering too, see below), `options.obj` turning behind, the music kept going (`CD_CheckForTrackEnd(0)`). Esc or Done saves and returns to the pause menu.
- In a level the settings apply at once: resolution, window mode, hardware, rotating view, 3D minimap, Gouraud (relight), motion blur, FX, HUD, aspect, blood and German gas (the effect atlases are rebuilt), textures (the cube atlases and the water's index atlas are rebuilt in place, `lighting::follow_textures`; render-only, the surface flags come from `.CubeTextureList`) and menu props (the pause menu's models). "Scenery" reads "(next level)": its models are built at level load and swapping them would reshape the level's objects. Title screens and intro are listed too; they show in the menus.
- Sound parity: the original's pause menu has only "Set sound volume" (item 4; the same in the Mac `ShowOptions` @ 10093240). The music mode and the one-shot cut stay on the front end's CD Audio Setup screen.
- Hardware on forces Gouraud on and motion blur off; Gouraud is then dimmed and skipped. Motion blur is refused under hardware. The resolution is dimmed and skipped in borderless fullscreen, which uses the desktop size.
- Resolution cycles through the Mac's three sizes first (640×480, 800×600, 1024×768: MENU 135 "Size", `CAmigaWindow::CAmigaWindow` @ 10102400), then the monitor's video modes, common window sizes that fit on the monitor (640×512 … 2560×1440), the window's current size and the saved size, smallest first. In Windowed it sets the window's physical size; in Fullscreen it picks the monitor's video mode of that size with the highest refresh rate.
- The main menu has an added item 8 "Graphics options" (1600, questionmarks model, before "Quit game"). It opens the Graphics options screen laid out like CD Audio Setup: title 1600, the items from y = 36/200 12 px apart (scrolling, with dots for hidden lines), "Done" 2148 at 170/200. Fire or right steps an item, left steps it back, Esc or Done saves. It replaces the Mac's "Payback Setup" dialog (`CPaybackPreferences::EditPreferences` @ 10050c60, not implemented by decision, `mac_version.md`): Resolution, Window mode, Hardware acceleration, Rotating view, "3D minimap: On / Off" (port-added: the rotating view's tilted minimap as its own setting, off by default and forced on (dimmed) while the rotating view is on, at the user's request, tilted and turned with the heading even when the view does not rotate; the original draws it only in the rotating view, `frontend_audio.md` §2.4), Gouraud, Motion blur 2053, Lens effects 2052, HUD, Aspect, then the Mac/Amiga content choices "Textures: Mac / Amiga look", "Blood", "Scenery", "Menu props", "Title screens", "Intro: Mac film / Amiga" and "German gas: On / Off", and last "Unlock all levels: On / Off" (§5). A Mac choice is dimmed, skipped and reads "Amiga" when the Mac data (or, for the intro, the film) is missing.
- Under hardware rendering (`RenderMode`, which follows `hardware` unless `--hardware`/`--software` force it), the front end also follows the original's Warp3D path:
  - The original pause menu (`FUN_0101a7dc`; Mac `ShowOptions` @ 10093240) does not restore the saved frozen picture under hardware (`DAT_0114c7d8 != 0`): it calls `FUN_010418b4` and clears the Warp3D screen every frame, so there is no game picture and no FadeDarken3 panel (CONFIRMED, OS4). **The port departs from this at the user's request:** under hardware rendering the pause menu and its Graphics options screen show the frozen game picture with the same darkened panel as software (a translucent overlay on the menu's back layer).
  - The Motion blur label is "Motion blur: " + 0x820 ("Off") regardless of state, drawn dimmed (`FUN_0100cc78`).
  - Menu backgrounds use `TitleScreen.w3d` (256×256 raw RGB, inferred R,G,B order) stretched over the screen at full brightness, also on sub-screens. Software uses the 50 % copy `DAT_0117b6b4` there, which hardware never builds.
  - The level-start screen `FUN_010247e4`, which hardware skips, has no equivalent in the front end.
- Saved in `Payback.cfg` (saves folder) as `hardware = 0|1`, `resolution = <w>x<h>` and `window_mode = windowed|borderless|fullscreen`. They are applied live, and at start-up the window is created from them; exclusive fullscreen is created hidden and shown directly in the chosen video mode once the monitor's modes are listed (no switch through the desktop mode). `--resolution <w>x<h>` and `--window-mode <mode>` override the saved values without saving them.
- Port-added presentation items on the same screen (`crate::frame`): "Vsync", "Frame limit" (off / 30 / 60 / 120 / 144), "Smooth motion" (render-only interpolation between the last two ticks, on by default; off = the original's one picture per tick) and "Zoom-in map" (the software level-start zoom, `hud::zoom`, dimmed under hardware). The mouse cursor is hidden in game and in fullscreen (Mac `UDrawSprocket::Activate` @ 10157490 `HideCursor`); losing the focus does not pause, as on the Mac (`UDesktop::Suspend` @ 101591b0 only deactivates the PowerPlant windows).
- View size (`DAT_0114c310`, below): '=' and '-' in game grow and shrink the single view in software rendering (`camera::view_size`, saved as `view_size`), centred with a black border, the HUD drawn inside it.
- The Mac's About box (`ShowAboutBox__11CPaybackAppFl` @ 1004ea30, PPob 2000: "Payback", "Macintosh version by Alex Curylo", `vers` 1 "1.0.0, ©2002 VPL") has no menu bar here: its lines end the main menu's credits (`misc::ABOUT`).

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
- Title 2089. Slot lines at (0x19 + 11i)/200 of the height, "Cancel" at 0xb8/200 (C; the load screen's are virtual 0x20 + 14i and 0xeb).
- For an empty slot (`FUN_01011cc8` = file missing) the description defaults to `"%s - C%d, L%d"`: name, city+1, level 1 or 2 of the first level without a cup; once all six have a cup it reads C3, L1 (C: the chain of `DAT_0117aec8..aedc` tests ends with level 1 when `aedc != 0`; Mac `DoSaveGame` @ 10096d80 has the same loop). A used slot offers its old description.
- Then edited with 2084 (max 40 characters), saved, and 2083 "GAME SAVED!" shown. Esc keeps the old line ("Slot %d" for an empty slot).

**Mac (CONFIRMED, `ReadSaveGame` @ 1008b560, `WriteSaveGame` @ 1008b7e0, `SaveGameExists` @ 1008ba60, `GetSlotName` @ 1008bb70, `DoLoadGame` @ 10096950, `DoSaveGame` @ 10096d80):** the same paths and the same 152 bytes (PPC, big-endian), so Mac and Amiga slots are interchangeable. Load and save screens are laid out as the OS4 ones; a failed load shows 2112 with the refuse sound. No Mac save files ship (`extracted/mac/Payback/Saves` holds only Finder files).

**What a slot does not hold:** money, score, the current level, police level or any setting. Loading restores the name, best times and cups only; the level select then opens on its default (§4).

**Level-select default (C, `FUN_01025544`):** the first level without a cup; once all have one, the first level with the lowest cup (capped at 3); all gold → city 3 level 2.

**Reimplementation (`frontend/progress.rs`, `misc.rs`):**
- `PaybackSlot<n>.save` in the saves folder (`progress::saves_dir`: `$PAYBACK_SAVES`, else the per-user data directory, `%APPDATA%\Payback` on Windows, `~/Library/Application Support/Payback` on macOS, `$XDG_DATA_HOME/payback` elsewhere; on first use an older `./Saves`, from the working directory or next to the executable, is copied there), same 152 bytes; written through a temporary file and a rename. A file shorter than 152 bytes counts as an empty slot (the original would read what is there and keep the rest of the old profile); a negative best time reads as −1 and cups are clamped to 0..3. The version word is not checked, as in the original.
- After a load the replay items of the game menu stay hidden until a level is played.
- Debug: `--autoplay-save <n>` (with `--autoplay`) awards the completed level's cup and writes slot n; `--load-slot <n>` loads slot n at start-up (e.g. with `--frontend-screen levelselect`).

**Port-added "Unlock all levels" (`Settings::unlock_all`, `unlock_all = 0|1` in `Saves/Payback.cfg`, default off):** the last line of the Graphics options screen (main menu item 8, also reached from the pause menu). On, every story level (level select), every built-in city (Rampage and multiplayer city rows) is selectable whatever the cups; user maps are always offered anyway. It only changes the checks (`progress::level_open`, `progress::city_open`): the profile, its cups, the save files and the Game Complete / rating logic are untouched, so turning it off restores the original gating. It does not touch the simulation (the Rampage random missions read the real profile, §8).

## 6. Config `S:Payback.config` (C load `FUN_0104199c`, save `FUN_01041510`)

The config is mostly written by `PaybackSetup`. Screen modes and resolutions are chosen there, not in game (I). The reimplementation offers them in game instead (§3, Graphics options). Layout, all u32 big-endian unless noted:

| # | Field | Notes |
|---|---|---|
| 1 | version | 10; < 10 gives "Config file too old" |
| 2 | `DAT_0114c72c` | saved motion blur (C, see below) |
| 3 | lens-flare FX mode | 0..2 |
| 4 | char[40] CD device name | `DAT_0114c7e0` |
| 5 | CD unit | `DAT_0114c308`, default −1 |
| 6 | CD playback mode | `DAT_0114c30c`: 0 Off 2142, 1 Radio 2143, 2 Continuous 2144, 3 Disabled 2145 (default 3) |
| 7 | `DAT_0114c304` | software-render screen ModeID (C, see below) |
| 8 | char[40] language name | `DAT_012a63d4`; empty means ask at start |
| 9 | 4 × {device `DAT_0114b59c[p]`, key set `DAT_0114b5ac[p]`} | per multiplayer player |
| 10 | required kills | `DAT_0114c2e8` |
| 11 | multiplayer/rampage city | `DAT_0114c7c0` |
| 12 | `DAT_0114c310` | 3-D view size in 1/256 (C, see below); set to 0x100 under hardware rendering |
| 13 | police aggressiveness | `DAT_0114c2ec`, 0..5 |
| 14 | 11 × 11 key codes | `DAT_012a63fc` |
| 15 | volume | 0..100 |
| 16 | `DAT_0114c300` | Warp3D screen ModeID (C, see below) |
| 17 | Gouraud | `DAT_0114c2fc` |
| 18 | rotating view | `DAT_0114c7dc` |
| 19 | hardware (Warp3D) | `DAT_0114c7d8` |

Total 656 bytes.

**Fields 2, 7, 12, 16 (C; full text in `frontend_audio.md` §3.7):**
- `DAT_0114c72c` = saved motion blur. The live flag is `DAT_0114c6c8`, toggled by `FUN_01042308` / pause item 3; level start (`FUN_010692bc` ~49442) turns blur on if c72c is set, level end (~49887) stores c6c8 back and turns blur off.
- `DAT_0114c304` = software-render screen ModeID: `FUN_0104107c` gets W/H from it via GetDisplayInfoData and sets `DAT_0114c2e0` = native non-RTG (8 bitplanes + c2p `FUN_0112d1d0`); OpenScreen SA_DisplayID at 0x0104ff88 and 0x010509c4.
- `DAT_0114c300` = Warp3D screen ModeID: OpenScreen at 0x0105013c, W3D_CC_ModeID in `FUN_01002024`, and `FUN_0104107c` when `DAT_0114c7d8 != 0`.
- `DAT_0114c310` = 3-D view size in 1/256 of the screen, 0xB0..0x100 step 0x10, keys '=' (raw 0x0C) and '-' (0x0B) in `FUN_01051940` / `FUN_01051a10`, refused under Warp3D; `FUN_01051800` clears the buffers and sets `DAT_0114c6c4`; forced to 0x100 under Warp3D.

**Key tables:** 11 actions per table, in the order of phrases 2457–2467: accelerate, reverse/brake, left, right, shoot, shoot alt, in/out, brake only, next weapon, prev weapon, repeat message.
- Table 0 (keyboard), Amiga raw keys: 4C 4D 4F 4E 40 00 44 31 5F 46 37 (up, down, left, right, Space, –, Return, Z, Help, Del, M).
- Odd tables 1,3,5,7,9 (PSX): 106 109 100 102 107 10C 108 10D 10B 10A 104.
- Even tables 2,4,6,8,10 (CD32): 11D 11E 118 11A 11F 0 120 122 121 0 0 (C; 68k `FUN_000ec686` builds the same alternating loop).

**Control setup screen `FUN_0102e75c`:**
- Title 2469. 11 columns, scrolled horizontally with easing `p = (7p + target) >> 3`.
- Columns 0..2: "Single player" 2455 with Keyboard 2167 / PSX 2164 / CD32 2165.
- Columns 3..10: "Multiplayer" 2456 with PSX/CD32 for (k−3)>>1.
- Action rows at y = (50 + 10i)/200, key names from `FUN_0103c690` (phrases 1500–1534; 0 = "-?-"). Redefine prompt 2468 (press 0 to disable). Done 2148 at 170/200, Cancel 2192 at 180/200 (C).
- Action labels right-aligned at x = 130. The columns are drawn into a view from x = 135 (0x87) to the right edge, column k centred at 90 + p + 100k in it (C).
- Selection (C): `column*11 + action` (0..120), Done 0x79, Cancel 0x7a (the start). Up: from Done to 10 (column 0, last action), from Cancel to Done, from an action's first row to Cancel, else −1. Down: from an action's last row to Done, Done → Cancel, Cancel → 0, else +1. Left/right: ±11, wrapping over the columns (not on Done/Cancel). Scroll target −100·column, 0 on Done/Cancel. Edits go to a local copy of the 11×11 table; Done copies it back, Cancel drops it.
- Fire on an action: the screen is replaced by 2468 (wrapped at 80/200) and 2119 "(Press escape to cancel.)" at 110/200 − 5, the fire key must be released, then `FUN_01040790(column)` reads the binding (C): raw key 0x45 Esc → cancel (select sound); raw 0x0A ('0') → 0 (disabled); raw 1–9, 0x0B ('-'), 0x0C ('='), 0x19 ('P') → refused (sound 0x2d); any other raw key < 0x80 is accepted for the keyboard column only; pad columns wait for a pad button (lowlevel `ReadJoyPort` / PSX port).
- Reimplementation: 9 columns (single-player keyboard, gamepads 1–4, multiplayer keyboard key sets 1–4), `crates/payback/src/frontend/bindings.rs`; saved as `key.*`, `pad.*`, `pad<n>.*`, `mp<n>.key.*` in `Payback.cfg`. The single-player keyboard defaults are the Mac's (`ClearPreference` @ 100508c0: arrows, Space, shoot-alt unbound, Return, Z brake, Help→Insert next weapon, Forward-Delete→Delete previous weapon, M), with the Mac's fixed weapon-cycle keys F3/F10/keypad+ (next) and Delete/F2/F9/keypad− (previous) (`UpdateInputStatus` @ 100f8490); the debug free camera moved from F2 to F6. Pad bindings take buttons or axis ends (`PadInput::Axis`, pressed past 60 %, the Mac's `IsHIDControlPressed` @ 10158300); gamepad numbers stay with their device across unplugging (`PadSlots`). Character keys are named by the keyboard layout once pressed (`bindings::key_label`).

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
- Row 8: City (2124 + 2170–2172, a user-map name, or 2039 "Not found!") at 149/200. The row skips built-in city c+1 while city c's last level (`DAT_0117aecc[c*2]`) has no cup; user maps are always offered (C, §11).
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
- Menu: City, Play again 2198, View replay, Save replay, End game (items 0..4).
- `FUN_01027330` returns void. "Play again" sets `DAT_0114c7c0` = the chosen city, slides out and returns with `DAT_0114c7c8` still 1; the caller (`FUN_010692bc` ~50007) calls `FUN_0102fc44`, which with c7c8 set skips the title and returns `c7c0*2` (LAB_01030208), so LoadMap starts the next match at once: same players and kill target, chosen city, level 0 (C). "End game" ("Are you sure") sets `DAT_0114c7c8 = 0` instead.

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
- Mac version 0x3F3 (CONFIRMED, `SaveMovie` @ 100fab30, `LoadMovie` @ 100fa7b0): the same 12 words (rampage written as `x != 0`, read as `== 1`), then the map path as a NUL-terminated string of any length (read up to 0x100 bytes; `GetCityFilename` @ 10084c90: `PROGDIR:Maps/0..2` or `PROGDIR:UserMaps/<name>` from `AddUserMap` @ 10084b30), level (city & 1), a u32 byte count, then the records as a bit stream (below). The loader ignores anything after the records and rejects every other version (2116); an unknown map (`GetCityNumFromFilename` @ 10084d20 = −1) returns 2.
- Mac record stream (CONFIRMED, `AddFrame` @ 100fa5d0, `GetAbbreviatedVals` @ 100fa2b0, `GetNextFrame` @ 100f9ea0, `WriteBits` / `ReadBits` @ 100f9700 / 100f97b0, `TidyMovie` @ 100faaa0; `analysis/mac/scratch_followup/input_movie.md` §1): the words below, without comments, written LSB first. Per player the coder keeps the last word (0xFFFFFFFF) and the last selected weapon (0); one run counter is shared by the interleaved players. An entry is `1` + a 16-bit literal or `0` + a 4-bit code applied to the last word (0/1 steering ±1 cyclic, 2/3 pedal ±1 cyclic, 4/5/6 toggle fire / in-out / brake, 7..13 weapon = last selected + code − 6 mod 8, 14/15 ticks ±1 cyclic in 1..5), then its run: 4-bit counts, each 0 adding 15, ended by a nonzero one. `TidyMovie` writes the pending run, the literal 0xFFFF and count 1. The byte count includes the partial last byte. The Mac encoder is lossy in four cases (a no-change word after a weapon select is merged into a repeat, so the select is played again; both directions held decode as one; a weapon change with a tick change is coded as the tick change). Recording stops at 99,998 bytes (buffer 100,000).
- Port: writes 0x3F3 with the Mac stream, encoded without the lossy shortcuts (no merge; a literal whenever a code would not decode back to the word), and reads 3, 10, 0x3F2 and 0x3F3, decoding Mac streams exactly, losses included (the replay list accepts 10, 0x3F2 and 0x3F3, as OS4 plus the Mac). After the records it appends `PBrm` and a u32 of flags: bit 0 = random phone missions in Rampage (`Variants::random_missions`, which changes the simulation; `crate::replay::RandomMissions` holds the level's value, from the settings when recording and from the header when playing), bit 1 = the records are the Mac stream. Bit 2 = two more i32 follow, the random missions' outside inputs (`RandomSetup`): the money goal (top of the Rampage high-score table for the police level, `GetHighestScore` @ 100a05a0) and the story progress (`GetHighestLevelCompleted` @ 1009b580, which limits the vehicles the jobs draw, so it changes the RNG draws). Both are read from files outside the replay (high scores, profile), so they are recorded and the recorded values are used on playback (`crate::replay::random_setup`); files without bit 2 play with the current values. A 0x3F3 file without the trailer is a Mac recording: stream, random missions on. A trailer without bit 1 marks the port's first 0x3F3 files, with raw words. Not tested on a real Mac file (none ships). Versions 3, 10 and 0x3F2 come from the Amiga builds, which have none. Rendering and audio settings are not stored.

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

- Weapon-select values (C, `FUN_01051a10`, 68k `FUN_000da2e0`): 0 fist (`+0x360` id 0), 1 pistol (id 0xD, ammo `+0x378`), 2 shotgun (5, `+0x364`), 3 flamethrower (2, `+0x36c`), 4 grenade (3, `+0x370`), 5 minigun (4, `+0x374`), 6 bazooka (1, `+0x368`), 7 remote bomb (0xE, `+0x37c > 0` or a placed bomb `+0x380`); 10 = no change; 8 and 9 unused. A change happens only if the player has that weapon and is not already holding it. Same order as the HUD weapon icons; not the trigger 0x22 numbering.
- Version-3 records (CD release) use the same bit layout (C): encoder at file offset 0x4773c..0x477de (bits 14..8 flags, weapon << 3 or 0x50, ticks separate), decoder 0x473cc (`(w & 0xF8) >> 3`, 10 = none, 0xFFFF = end, `w & 7` = ticks), reader 0x7a7a0 (0xFFFF at end of data), comment skip 0x7a7d2 (bit 15 as `FUN_010bbcf4`). Only the header differs.

- The recording buffer is 150,000 bytes and silently stops filling when full.
- Playback reuses the stored seed and the per-frame tick counts. It ends when the reader returns 0xFFFF (data exhausted) or Esc or P is pressed (both set the same status flag: OS4 `FUN_0103f3d0` raw 0x45 / 0x19, Mac `ReadPlaybackInput` @ 10064ad0; CONFIRMED).
- Bit 9 is the in/out key's *held* state (`FUN_01051a10` records `psVar4[6]`, Mac `ReadInput` @ 10064bf0 `status[6]`); `GetInOutVehicles` (Mac @ 10066140) takes the edge itself. The port records one-tick presses and plays bit 9 back as a press only on its rising edge, so both kinds of file play the same.
- Digits 0..7 select weapons for every keyboard player in multiplayer too (OS4 `FUN_0103f3d0`, Mac `UpdateInputStatus` @ 100f8490; port `multiplayer::read_devices`).

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
- On select: the name is copied to `DAT_012a63d4` and `FUN_0103da68` is called: the pad half of the input poll, which clears the Esc/fire words and sets `DAT_0114c460` so the next `FUN_0103f3d0` starts from a zeroed struct, i.e. an input reset (CONFIRMED; the Mac `DoSetLanguage` @ 1008c2b0 does not do it). The config is written by `FUN_01041510`.
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
- Reimplementation: a level started from the menus loads in stages, one per frame (`level::LevelLoad`), with `map.size` as the unit and microseconds of work as DrawProgress units; the calibrated value is written to `Saves/map.size` (read before `General/map.size`). `--load-delay <ms>` (debug) idles between stages.
- **68k startup Loading screen** (C): InitProgress = `FUN_000cc0c2("startup.size")`, called from `FUN_000ec686` at 0xed4fe right after the fade (`FUN_0003eb0c`). Software path: clears the box with `FUN_000365ea` (cx−0x36..cx+0x35, cy−0x16..cy+0x17), draws `icons/Loading` with `FUN_00018a9e` at (cx−0x2e, cy−0x11) and shows it with `FUN_000cbf20`. DrawProgress = `FUN_000cc456`; CloseProgress is inline at the end of `FUN_000ec686`. A duplicate sequence in `FUN_000f0a72` has no callers.
- Start-up (C): 68k `FUN_000ec686` calls InitProgress `FUN_000cc0c2("startup.size")` at 0xed4fe after the fade; the loaders (sounds, effect textures, object textures `FUN_000952c2`, objects `FUN_000d7874`, cube textures) call DrawProgress `FUN_000cc456`; the intro plays at 0xeda52, then CloseProgress runs inline. Reimplementation (`startup.rs`): booting into the menus, the start-up asset loads run one per frame behind the panel, calibrating `Saves/startup.size`; the intro follows, then the menus.
- `FUN_0100b288(x, y, filled, total)` is a separate segment-bar helper (`LoadingPiece2` = empty piece) with no callers in OS4. The pause-menu volume bar draws its own 10 pieces inline.

## 11. `.thumb`, `.mini`, icon format (C)

**`.thumb`:**
- 256×256 8bpp, raw, 65536 bytes; a top-down picture of the city.
- `maps/0..2.thumb` are preloaded by `LoadTex` `FUN_01011698` (64 KB + a texture handle at +0x10000).
- `LoadCityPreview(n)` `FUN_01027164`: n < 3 → `PROGDIR:Maps/n.thumb`; otherwise user map `DAT_01170808[n−3]` + ".thumb" into `DAT_0117f8d0`. Used by the multiplayer and Rampage city rows.

**User maps (`FUN_0100a1d4` / `FUN_01009e88`):**
- `UserMaps/*.map`, skipping `test.map`, max 2000 ("AddUserMap(): Too many user maps...").
- Paths in `DAT_01170808` (`PROGDIR:UserMaps/<name>` without `.map`), display names in `DAT_01172748` (`<name>`), count `DAT_01170804`.
- `FUN_01009ff8(city)`: 0..2 → `PROGDIR:Maps/0..2`, 3.. → user map city−3 (C). `FUN_0100a064(path)` is the reverse (−1 if unknown).
- The Rampage and multiplayer city rows (`FUN_0102a428`, `FUN_0102b2c8`) cycle 0 .. count+2: a built-in city c+1 is skipped while city c's last level (`DAT_0117aecc[c*2]`) has no cup; user maps are always offered. A configured city ≥ 3 that no longer exists becomes 0 (C). The row shows the user map's name.
- The shipped data has one: `UserMaps/The Killing Zone` (.map/.mini/.route/.thumb, no `.vehicles`), plus `test.map`.
- Mac (CONFIRMED): `InitUserMaps` @ 10084fe0 matches `PROGDIR:UserMaps/#?.map`; `AddUserMap` @ 10084b30 skips `test.map` (string `1018cf24`), keeps at most 2000 and strips the last 4 characters. `DoRampageSetup` @ 1009ebc0 cycles the same city list (built-in city c ≥ 1 skipped while `cups[2c−1]`, the previous city's last level, is 0; user maps always offered), resets a city whose `GetCityFilename` is NULL to 0, and returns `city*2`; the caller sets the Rampage flag `DAT_1017c334`. User maps are played in Rampage and in multiplayer only (story levels are the built-in cities).
- Missing companions (Mac CONFIRMED): no `.route` → `LoadRouting` @ 100f1560 calls `InitRouting` @ 100f0f50 (every node's byte 12 = 0xFF, i.e. unused; police drive without the graph); no `.vehicles` (or version ≠ 1) → `Obj_Load` @ 100ad180 calls `Obj_Init` @ 100ad130 (traffic flags 0, no parked vehicles); no `.textures` → no custom slots (`LoadUserTextures` @ 1015b1d0). `LoadMap` @ 100b3a60 loads the textures and `.vehicles` before the squares, then the route, then `<map>.mini`; level phrases only for cities 0..2 outside Rampage and multiplayer.
- Rampage start (CONFIRMED, all builds): `amiga_startlevel` @ 100fff40 places the first player with `StartObject` @ 100adb70 (a random special-1 start square, avoiding the last 4, as a respawn) when the Rampage flag is set, else with `InitialStartObject` @ 100ade40 (the level's special level+2 square). 68k `FUN_000ec686` ~0xee31a (`FUN_00043c96` vs `FUN_00043f0e`), OS4 `FUN_010692bc` ~49288 (`FUN_0106ef6c` vs `FUN_0106f1ac`). 11 of the 31 Mac maps have no special-2 square. Port: `sim/people.rs` `spawn_player` / `random_start`.
- Mac bundle check (2026-10-09): all 31 maps plus `test.map` parse (version 1, 128×128, no trailing bytes); every texture ≥ 2000 they use has a present `.textures` slot; 4 have no `.route` (House Park 2 Night, Kozie_Klefki, MultiplayerTournament, Tankwarfare), 10 no `.vehicles`. Night: Amigaliens_Metro_night, D-Town_Night, DestraCity_Night, House Park 2 Night, MaluchRejs; weather 1: AlabastraCity, D-Town, VerbalCity; weather 2: D-Town_Night, Gods_Island, MaluchRejs, MultiplayerTournament. Smoke test: `analysis/scratch/usermaps/run_all.sh` (screenshot + 2000 Rampage ticks per map from a scratch working directory, `contact.py` for the sheet).
- Reimplementation (`frontend/usermaps.rs`): searched in the game data's `UserMaps/`, next to the executable and in the working directory; `LevelSpec.map` holds the path for a user map; scores go to `Saves/UserMaps/<name>.scores` for maps inside the game data, next to the map otherwise.

**`.mini`:**
- 65×65 8bpp. Loaded at the end of LoadMap as `FUN_010727f0("<map>.mini")`.
- Cropped to 64×64. Pixels at x 29..34 (0x1d..0x22), rows 0..4, are overwritten (C, `FUN_010727f0`, stores at 0x010728c0): a red (0xB0) up-pointing arrowhead with a black (0x08) shadow row (picture in `frontend_audio.md` §6.2). Baked into the texture, so it turns with the map in rotating view: a north pointer (I).
- Stored at `DAT_0130e254` as the HUD radar texture (C: the minimap `FUN_01018954` draws it, `frontend_audio.md` §2.4).

**Icons:**
- Raw 8bpp chunky; the row stride is the width rounded up to 16. Examples: cursor 6×10 is a 160-byte file; cup icons 15×20 are 320 bytes; PaybackLogo 166×80 is 14080 bytes.
- Loader `FUN_0100d3c4(dst, path, w, h, bytes)`.

## 12. Cheat menu (port addition, not in the original)

Not in any release: added at the user's request (`crates/payback/src/cheats.rs`, screens in `crates/payback/src/frontend/cheats.rs`).

- **Opening:** F12 in a single-player level (story or rampage), not in multiplayer and not while a replay plays. F12 is refused in the control setup's redefine prompt (`bindings::refused_key`), so it is never a game key. The game pauses (virtual time) as for the pause menu; the music keeps going (`CD_CheckForTrackEnd(0)`).
- **Look:** the pause menu's: the frozen game behind its darkened panel (virtual 50..269 × 26..229, software and hardware rendering; without the pause menu's `options.obj`, which reads "OPTIONS"), the wide title font for "CHEATS", small-font lines with the selected one flashing (4-frame cycle), the menu sounds (move 0x20, select 0x21). English text (no phrases), except On/Off (2081/2080) and the vehicle names (2330–2355).
- **Items** (up/down, Enter/Space or fire; Esc back to the game):
  - "God mode: On/Off": Enter (or left/right) toggles; the menu stays open. The player's object (on foot, or the vehicle being driven) takes no damage: punches, bullets, flames, explosions, run-overs and crashes leave its health alone (`Obj::god`, checked where damage is applied in `sim::combat`, `sim::contact`, `sim::driving`). So the player can't be killed or wrecked by damage. Deaths that are not damage still happen: drowning, being busted, mission scripts that kill the player, the RC car dying in water. Health already lost is not restored.
  - "All weapons": every weapon slot 1..7 gets at least 999 rounds (HUD 00999); closes the menu.
  - "Clear wanted", "Wanted up", "Wanted down": the player's wanted level (`Obj::wanted`, the original's `+0x32e`) to 0, or one up / down within 0..6 (the HUD's six sirens, the top of the crime tables). A change restarts the decay timer (`WantedTimer`, `DAT_0030c3b8`) as a crime that changes the level does (68k `FUN_000ccc96`). The menu stays open and shows "Wanted level: n/6" as it will be; the HUD sirens follow when the game resumes. The police react as to any other change of the level (pursuit and cops on foot from 3, police cars in traffic from 4, roadblocks from 5; below 3 pursuers return to traffic).
  - "Spawn vehicle": the vehicle selection screen: title "SPAWN VEHICLE", the vehicle's name at y = 0x46/200 between the arrows, "n/26" below it, and the vehicle's model turning on its vertical axis behind the panel (seen from about 24° above, sized by its bounding box). Left/right change the vehicle (wrapping), the model sliding out and the next one in like the option objects (3 + 3 frames). Up/down change its paint colour (wrapping), shown on the turning model and as "Colour n/m" under the number. The colours are the original's paint pairs (`objects::PAINTS`: 0 black, 1 blue, 2 green, 3 purple shine / orange body, 4 red, 5 silver, 6 yellow, 7 white; the original gives every traffic car `random(8)`, Mac `AddThing` @ 10058350, and scripts choose one in `CreateActor`). Only models with paintable faces (`Cardef` / `Shine.All`) have 8: Mundaneo, Pug, Diblo, Vapour, Pug GTI, Fjord, Van, Limo, Scooby, Evo, Pickup, Hot rod, Pug racer, Rocket car, Vapour racer; the rest keep their fixed livery (police cars and vans, taxi, bus, tank, ice-cream van, truck, helicopter, bomb truck, RC car, boat) and show "Colour 1/1". The list is every drivable type in `vehicles::TYPES` order: 0..24 and 27 (Mundaneo … Rocket car, Vapour racer, with the tank, bus, helicopter, RC car and boat), without the rocket car's thrust frames 25 and 26. Enter closes the menus and spawns it; Esc goes back to the cheat menu.
- **Placement:** a parked vehicle of that type, facing the player's heading, on the nearest free spot straight ahead: from just clear of the player out to 8 squares in steps of 256 units; a spot is free when the floor under the centre is within a step (64 h-units) of the player's height (water up to two squares below, for a boat off a quay), the floor under all nine box points is within a step of that, the box meets no wall on the way from the player (`collide::vehicle_walls` with the centre path from the player), and it is clear of every other vehicle. If nothing ahead is free, the nearest free spot in 16 directions around. A boat prefers water and the others dry ground, falling back to any free spot (so a boat can be spawned on land and a car into water, where it sinks). No free spot: nothing is spawned.
- **Determinism:** the menu only queues the action (`Request::Cheat` → `Cheats::queue`). The fixed tick applies the queue right after the replay input (`cheats::apply`, chained after `replay::debug_hash`), records each action in the replay at that tick, and the god-mode flag is copied onto the player objects there. Vehicles are spawned by the simulation (`vehicles::spawn_vehicle_world`, next object-table slot); the placement search draws no random numbers. Playback applies the recorded actions at their ticks and ignores the menu (which is disabled anyway). God mode is reset at level start and is in the replay hash.
- **Replay format:** trailer flag bit 3 (`FLAG_CHEATS`) and a block after the flags and the random setup: a u32 count, then per event u32 tick, u32 kind (1 god mode, argument 1/0; 2 all weapons; 3 spawn vehicle, argument the type | paint << 8, so older files with the type alone spawn paint 0 as they did; 4 wanted level, argument +1/−1; 5 clear wanted), i32 argument (`payback_formats::replay::CheatEvent`). Files without cheats are written exactly as before; older builds ignore the block (and so would replay such a game without its cheats).
- **Debug:** `--cheat-menu [--menu-item <n>]` and `--cheat-vehicle <type> [--cheat-colour <paint>]` open the menus 3 s into a `--map` level; `--cheat-script <tick>:<cheat>,...` (`god`, `nogod`, `weapons`, `vehicle=<type>[:<paint 0..7>]`, `wanted+`, `wanted-`, `wanted0`) queues cheats at those ticks of a recorded game, e.g. for the replay check.

## Open items
Resolved in the 2026-10-08 pass (see `open_questions.md`): pause-menu CD command (§1), View intro (§3), config fields (§6), Play again (§7), version-3 records and weapon-select values (§8), the 68k startup Loading screen (§10), the `.mini` glyph (§11), model 16 `cross` (§2), the city-row unlock rule (§7 now follows §11). Resolved on 2026-10-09: §1 Game Menu p3 (single-player recording: high-score entry allowed) and §9 `FUN_0103da68` (pad reader used as an input reset). Nothing is open.

Scripts are in `D:\Payback Reverse Engineered\analysis\`:
- `fn.sh` / `where.sh` (decompile helpers)
- `scratch\ppcdis.py` (PPC disassembly)
- `scratch\findbl.py` (call-site scan)
- `scratch\phr_by_fn.py` (phrases used per function)
- `scratch\replay_decode.py` (replay decoder)
- `scratch\fe_imgs.py` (renders the images)

Rendered front-end images are in `D:\Payback Reverse Engineered\analysis\img\frontend\`: the four title screens, `0..2.thumb`, `0..2.mini`, Loading, the pieces, the logo, the cursor and the arrows.