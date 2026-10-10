# Mac release vs Amiga: front end, HUD, input, files

Research notes comparing the Mac Payback (Virtual Programming, port by Alex Curylo, v1.0, 30 Oct 2002) with the Amiga builds. The scope is the front end and everything around it: menus, preferences, controls, HUD and text, languages and phrases, saves, replays, high scores, user maps, the intro, music, registration and debug features.

**Sources**
- Mac OS X build (PEF/Carbon, original C++ names): `analysis/ghidra_macx/`. Addresses below are from this build unless marked "OS 9".
- Mac OS 9 build: `analysis/ghidra_mac9/`.
- Resource forks: `extracted/mac/Payback/Payback (OS X).rsrc` and `(OS 9).rsrc`, dumped to `scratch_frontend/rsrc_macx.txt` and `rsrc_mac9.txt`.
- Manuals: `extracted/mac/Manuals/*.pdf`, as text in `scratch_frontend/manual_{English,French,German}.txt`. Also `extracted/mac/Payback Read Me`.
- Amiga: OS4 `analysis/ghidra_os4` (`FUN_01xxxxxx`), 68k `analysis/ghidra_68k`, our notes `docs/re/frontend_menus.md` and `docs/re/frontend_audio.md`.

**Tags:** C = CONFIRMED (read in code, disassembly or bytes). I = INFERRED.

**Each finding gives:** Mac function or resource → Amiga counterpart → what differs → impact on our implementation and a recommendation → confidence.

**Tools** (local working files in `analysis/mac/scratch_frontend/`, not kept; `tools/re/fn.py macx NAME`
replaces `mfn.sh`):
- `mfn.sh NAME [x|9]` prints a Mac function.
- `macdis2.py A B` is a PPC disassembler with symbol names, needed for functions Ghidra missed.
- `pidata.py` unpacks the PEF data section to `macx_data.bin`; a string at Ghidra address A sits at `A − 0x1017b440` in that file.
- TOC: r2 = 0x10183440, and `_DAT_1017cf0c` = 0x1018cec8 is the base of the front-end strings.

---

## 0. Summary

**Policy (from the lead):** the Mac release is treated as more definitive. The recommendations below default to **adopting** Mac behaviour and content wherever it differs. The exceptions are platform artefacts (PowerPlant/Carbon plumbing, the Apple menu, Finder specifics, DrawSprocket/HID/QuickTime mechanics, the CD-present check) and clear Mac bugs (§3.4, §6.3 phrase 1799, §7.3).

The Mac game is a close port of the OS4/Update 7 code, with the Amiga APIs emulated. Menus, HUD, fonts, save slots and the replay record words are essentially unchanged (the Mac replay body is a compressed bit stream). The differences fall into five groups.

**1. Platform shell (Mac-only).**
- A PowerPlant app wraps the game.
- A native "Payback Setup" dialog sets window or fullscreen, the size (640×480, 800×600 or 1024×768), the renderer (OS 9 only) and graphics toggles.
- Preferences replace `S:Payback.config` and are stored as a QuickTime atom container in the system Preferences folder.
- The menu bar has About, Quit and Setup (⌘;).
- The game will not launch unless the "PaybackMac" CD is mounted.

**2. Input.**
- Keyboard keys are Mac virtual key codes, with 0x7F meaning "none".
- Gamepads go through HID Utilities (`HID.bundle`) on OS X and InputSprocket on OS 9. Any binding may be a HID element.
- The control setup drops the PSX and CD32 columns: 5 columns instead of 11. Multiplayer devices become "Device 1–4" or Keyboard.

**3. Removed or changed.**
- No keyfile: the game is always registered.
- Only English, Deutsch and Francais ship.
- The FLC intro is replaced by a QuickTime MPEG-1 video (`payback_high_mpeg1.mpg`).
- CD audio plays the game CD's audio tracks through QuickTime; the menu entry is now called "Music setup".
- Replay version 0x3F3 has a variable-length map path.
- German language swaps the blood effect for "Gas".

**4. Mac bugs not to copy.**
- `SaveHiscores` writes uninitialised stack words for the scores and the checksum, so high scores never persist.
- The repeat-message control has no edge latch.
- Phrase 1799 is used but exists in no phrase file.

**5. Data.**
- 31 extra playable user maps; 13 of them use custom `.textures`, which our loader does not support yet.
- New title-screen art.
- 205 new and 136 changed English global phrases.
- `CubeTextureList.txt` replaces `.CubeTextureList`.
- The Replays and Saves folders are empty: the Amiga's two demo replays are not shipped.

There is no network play.

---

## 1. Platform shell, start-up and Mac-only UI

### 1.1 Start-up sequence: `CPaybackApp::PrepareToRun` (code at ~`1004ee00`; Ghidra did not create it, see `macdis2.py 1004eb64 1004f190`) — C

1. **Environment checks** (`Alert` 1000 with `STR# 1000` texts):
   - `HasFeature` (Sound Manager 3);
   - `Gestalt` system ≥ 0x710 (System 7.1);
   - QuickTime ≥ 3.0 (`ALQuickTimeVersion`).
2. **DrawSprocket.** `UDrawSprocket::Initialize`; on failure alert 1100 #2, "Payback requires DrawSprocket installed to run."
3. **Input.** `UInputSprocket::Initialize`: HID on OS X, InputSprocket on OS 9.
4. **Preferences.** `new CPaybackPreferences` (§2).
5. **CD.** `UCDAudio::Initialize`, `OpenDriver`, `SetVolume(200,200)`.
6. **CD check.** If `UCDAudio+0x34` is 0 (no mounted volume named **"PaybackMac"**), `LAlert` shows "The Payback CD must be inserted in order to play." (data `0x10188da8`) and the app quits.
7. **Setup dialog.** If the "show setup" preference (`prefs+0x2ac`) is set, **or Option (key code 0x3A) is held**, `EditPreferences(false)` runs. Cancel quits.
8. **Window and game.** `CreateWindow(PPob 129 "Payback Main", class 'AmWn' = CAmigaWindow)`, then `amiga_init` (`100ffc70`): `LoadConfig`, `InitAll`. Then `Run` (`1004f280`) runs `amiga_dotitlescreen` / `amiga_runloop`.

- **Amiga counterpart:** `FUN_010692bc` boot: config, keyfile, `FUN_010138dc`, then the main loop. Screen mode and renderer come from the separate `PaybackSetup` program.
- **Difference:** the native setup dialog and the CD check are Mac-only. The Amiga's CD use is optional (CDDA music, the intro fallback path).
- **Impact:** none needed. Our in-game Graphics options (`frontend/display.rs`) already cover what PaybackSetup and the Mac dialog do. Do not add a CD check.

### 1.2 "Payback Setup" dialog: `CPaybackPreferences::EditPreferences(bool live)` `10050c60`; PPob 2200 (OS 9) / 2210 (Carbon) — C

Controls, by PPob pane IDs:

| Group | Control | Pane | OS 9 (2200) | OS X (2210) |
|---|---|---|---|---|
| Display Setup | radio "Play in window" / "Play fullscreen" | `FlSc` (`Wind`/`Full`) | yes | yes |
| | popup "Size:" (MENU 135: 640 x 480 / 800 x 600 / 1024 x 768) | `Swch` | yes | yes |
| Rendering Setup | radio "Use OpenGL" / "Use QuickDraw" | `HwAc` (`OpGL`/`QDrw`) | yes | **no** (Carbon requires OpenGL) |
| | "Use rotating view" | `RtVw` | yes | yes |
| | "Use Gouraud shading" | `GdSh` | yes | no |
| | "Use motion blur" | `mb!!` | yes | no |
| | popup "FX:" (MENU 134: Off / Flares only / Flares and artifacts) | `le!!` | yes | yes |
| | "Don't show this dialog again" | `ShSt` | yes | yes |
| | caption "Hold down the Option key when launching to bring this dialog back." | | yes | yes |
| | OK / Cancel | `DFLT`/`CNCL` | | |

Behaviour (C):
- When hardware (OpenGL) is chosen, Gouraud is forced on and motion blur off.
- On OK with fullscreen, a DrawSprocket context of the chosen size is reserved: 8-bit for QuickDraw, 16-bit for OpenGL. Failure gives alert 1100 #3, "Sorry, cannot play fullscreen at that resolution."
- The fullscreen radio is disabled if DrawSprocket cannot switch.
- When `live` is true (opened from the menu during play), changed Gouraud, motion blur, FX and rotating view are applied immediately with `ToggleGouraudShading`, `ToggleMotionBlur`, `SetLensEffects` and `ToggleRotatingView`.
- **Defaults** (`ClearPreference` `100508c0`): fullscreen on, 640×480, OpenGL on, Gouraud on, rotating view off, FX 2, show-dialog on.

- **Amiga counterpart:** the `PaybackSetup` tool (screen mode IDs, Warp3D or software) plus the pause-menu toggles.
- **Difference:** only three fixed sizes are offered, windowed or fullscreen. The OS X build is OpenGL-only, so it always runs the "hardware" path.
- **Impact (adopt):** make 640×480, 800×600 and 1024×768 the first presets in our resolution list, and add a Mac-style "Size" choice next to window/fullscreen. Keep our extra sizes and borderless mode as additions. The renderer choice is a platform artefact: OS 9 still offers QuickDraw, so it is not a definitive "always hardware" signal, and we keep both paths. Adopt the dialog's live-apply behaviour: Gouraud, motion blur, FX and rotating view take effect immediately.
- **Confidence:** C for the dialog; I for "definitive look".

### 1.3 Window and fullscreen: `CAmigaWindow` (`10102400` ctor, `ResizeAndCenter` `10102570`, `MatchDSpContext` `10102a80`); WIND 129 — C

- **Windowed:** the window type is procID 4 (no grow box), so it cannot be resized. It is resized to the chosen size and centred. Screen W/H come from the window port (`ReadScreenParams` `10051fc0`, same formulas as Amiga `FUN_0104107c`).
- **Fullscreen:** DrawSprocket switches the display mode, with gamma fades.
- **Intro playback** hides the cursor and pauses GL (`PauseGL`/`ResumeGL`).
- **Impact:** none. Our free-resize window is a superset.

### 1.4 Menu bar: MENU/MBAR/Mcmd resources; `CPaybackApp::ObeyCommand` (`1004ec30`) and `FindCommandStatus` (`1004ecd0`) — C (resources and dispatch), I (Help)

- **MBAR 128** (classic): Apple ("About Payback…"), File ("Quit" ⌘Q), Edit (Undo/Cut/Copy/Paste/Clear/Select All, then **"Payback Setup..." ⌘;**, command 27 = cmd_Preferences).
- **MBAR 129** (Aqua): Apple and Edit (Aqua). Quit and Preferences come from the Aqua application menu.
- **ObeyCommand:** 27 → `EditPreferences(true)`; 11 is swallowed. Everything else goes to `LDocApplication`.
- **Help menu** (MENU 132): "Payback Help..." ⌘? and "Visit Payback Web Site". Its 'Help' and 'URL1' commands are enabled but not handled, and `STR# 1400` URLs is empty, so the items are no-ops (I).
- **About box:** PPob 2000, "Payback / Macintosh version by Alex Curylo", with the version string from `vers` 1 ("1.0.0, ©2002 VPL"). Opened by `ShowAboutBox` `1004ea30`.
- **Quit:** ⌘Q or Quit sets the app state to 2. Every front-end loop polls `MacHasQuit` (`1004f560`) and returns −1. `CPaybackApp::Run` then calls `amiga_cleanup`, which runs `SaveConfig`.
- **Debug:** MENU 600 "DEBUG" (PowerPlant debug menu: ZoneRanger, HeapBoss, QC, Eat Memory…) and submenus 225–234 are present but in no MBAR. This is PowerPlant boilerplate, unreachable (I).
- **Amiga counterpart:** none. The Amiga quits via main-menu "Quit game".
- **Impact (adopt):** save settings when the window closes or on quit, as the Mac does in `amiga_cleanup` → `SaveConfig`. The menu bar itself is a platform artefact; do not reproduce it.

### 1.5 Apple events: `aete` 0, `aedt` 128–131 — C

- Only the standard PowerPlant suites are present: Required, Core (close, count, data size, exists, get, make, open, print, save, set; window and document classes), Misc (revert, select) and PowerPlant SetTellTarget.
- There are no game-specific events. `Info.plist` (`plst` 0) declares the document type "Payback data file", OSType `PBdt`, role Viewer. Files on disk carry `PBDt`/`PAYB`, for example `Maps/0.scores`.
- There is no evidence that opening a document does anything game-specific (I).
- **Impact:** none.

### 1.6 Requirements and versions (Read Me, `vers`, `plst`) — C

- Mac OS 9.0+ or OS X, 333 MHz, 64 MB (96 MB recommended), DrawSprocket 1.7.6+, InputSprocket 1.7.3+, QuickTime 4.0+.
- Bundle `com.vpltd.payback`, version 1.0.
- `PAYB` 0 resource: "2002 Virtual Programming Ltd."

---

## 2. Configuration: preferences file instead of `S:Payback.config`

### 2.1 Storage: `ALPreferences` (`100fd280`, `InitPreferences` `100fd420`, `SavePreferences` `100fd6b0`); `CPaybackPreferences::WriteAtoms`/`LoadAtoms` (~`1004fa00`–`100503c0`, missed by Ghidra) — C

- The file lives in the system Preferences folder (`FindFolder(kOnSystemDisk,'pref')`); its name comes from `STR# 500` #1, "Payback Preferences".
- It is a **QuickTime atom container**, one atom per setting:

| Atom | Setting | Amiga config field (frontend_menus §6) |
|---|---|---|
| `vers` | version | 1 |
| `mb!!` | saved motion blur | 2 |
| `le!!` | FX 0..2 | 3 |
| `dvnm` | CD device name, 40 bytes (default ".AppleCD") | 4 |
| `dev#` | CD unit (default 1) | 5 |
| `cdMd` | music mode 0..3 (**default 1 = Radio**) | 6 |
| `diID` | software screen-mode ID (0) | 7 |
| `Lang` | language, 40 bytes (default "English") | 8 |
| `MPc1` / `MPc2` | 4 multiplayer devices / 4 key sets | 9 |
| `MPKN` | required kills (5) | 10 |
| `MPCt` | multiplayer/Rampage city (0) | 11 |
| `ScSS` | 3-D view size /256 (0x100) | 12 |
| `CAgg` | police aggressiveness / "Difficulty" (2) | 13 |
| `CtSt` | 11 × 11 bindings, 0x1E4 bytes | 14 |
| `Volm` | volume (**default 50**) | 15 |
| `HIDM` | one 0x24-byte `recSaveHID` per HID-bound control (`Get/SetHIDConfig` `10157bd0`/`10157c80`) | — (new) |
| `DMI3` | Warp3D mode ID (−1) | 16 |
| `GdSh` / `RtVw` / `HwAc` | Gouraud / rotating view / hardware | 17 / 18 / 19 |
| `ShSt` / `FlSc` / `Swch` / `DSpA` | show setup / fullscreen / size index 1..3 / DrawSprocket attributes (0x48) | — (new) |

- **Load and save:** `LoadConfig` (`10052be0`) and `SaveConfig` (`10052830`) copy between the in-memory `SPaybackPrefs` (0x13E8 bytes) and the game globals.
- **Amiga counterpart:** `FUN_0104199c` / `FUN_01041510`, a flat 656-byte file with a version check ("Config file too old").
- **Differences:**
  - The format and location are new.
  - There is no "too old" check.
  - Defaults differ: music Radio instead of Disabled; volume 50; P1 = Keyboard and P2 = Device 1 (Amiga: P1 = joystick 0, P2 = keyboard); key set P1 = 3.
  - `LoadConfig` forces hardware off when no GL context is possible, and still forces Gouraud on / view size 0x100 under hardware (same as the Amiga).
- **Impact (adopt):** keep our text `Saves/Payback.cfg` (the atom container is a platform artefact), but adopt the Mac **defaults**: music mode 1 (Radio), volume 50, P1 = Keyboard, P2 = gamepad 1, P1 key set 3, FX 2, Gouraud on, rotating view off.
- **Confidence:** C (atom codes from `WriteAtoms` disassembly; field mapping from `LoadConfig`/`SaveConfig`/`ClearPreference`).

---

## 3. Input and controls

### 3.1 Key tables and devices: `UpdateInputStatus` `100f8490`, `UpdateInputStatusNoKeys` `100f8090`, `ButtonFromKey` `100f7fe0` — C

- The 11 tables × 11 actions keep the Amiga action order (phrases 2457–2467).
- **Table 0 (keyboard)** holds **Mac virtual key codes**: Up 0x7E, Down 0x7D, Left 0x7B, Right 0x7C, Space 0x31, (none) 0x7F, Return 0x24, Z 0x06, Help 0x72, Fwd-Delete 0x75, M 0x2E. The Amiga uses raw keys 4C 4D 4F 4E 40 00 44 31 5F 46 37. "None" is 0x7F instead of 0.
- **Tables 1–10** default to 0x7F. Any entry > 0x10000 is a pointer to a HID element record. On first run the defaults are assigned with `UInputSprocket::AssignControl`:
  - table 1 Up/Down = Y axis −/+, Left/Right = X axis −/+;
  - shoot, in/out, brake, next weapon, previous weapon and repeat each get a button.
- **Single player:** keyboard events go through table 0 (`ButtonFromKey`). **Table 1 ("Device 1") is polled every frame** by `IsHIDControlPressed`, so a gamepad works alongside the keyboard.
- **Multiplayer:** player p polls table `device[p]+1`.
- **Amiga counterpart:** `FUN_0103f3d0` and the lowlevel `ReadJoyPort` / PSX port; tables 1, 3, 5, 7, 9 PSX and 2, 4, 6, 8, 10 CD32 with fixed button codes.
- **Differences:** there are no PSX/CD32 tables, and pad bindings are freely assigned HID elements (buttons or axis directions).
- **Impact (adopt):** adopt the Mac model, where a single player can use the keyboard and gamepad 1 at the same time (the Mac polls table 1 every frame). Our `bindings.rs` keyboard + gamepad model already fits; make sure both are active together in single player. Mac key codes and HID records are platform artefacts.

### 3.2 Control setup: `DoControlSetup` `100a1f80` (+ `ReadFromInputSource` `100f93d0`, `WriteControlName` `100f7890`, `ConfigureHIDControl` `101581f0`) — C

- **5 columns** instead of 11:

| Column | Title | Sub-title |
|---|---|---|
| 0 | "Single player" 2455 | "Keyboard" 2167 |
| 1 | "Single player" | "Device 1" |
| 2–4 | "Multiplayer" 2456 | "Device 2".."Device 4" |

  "Device %d" uses a hard-coded English "Device" (string `1018e185`) with "%s %d".
- **Selection** 0..54, Done = 55, Cancel = 56 (Amiga: 0..120, 0x79, 0x7A).
- **New:** a spinning, bobbing `controlsetup` 3-D model is drawn behind the screen (`CreateObject(model 5, 0, 600, 0)`, z wobble ±2/frame around 6000). The Amiga screen has no 3-D object.
- **Rebinding:**

| Key | Mac | Amiga |
|---|---|---|
| Esc (0x35) | cancel | cancel (raw 0x45) |
| "0" (0x1D) | disable (0x7F) | disable |
| 1–9, −, = (0x12–0x1C) | refused | refused |
| F1 (0x7A) | **refused** (reserved for "Press F1 to continue") | allowed; 'P' was refused instead |
| other keys | keyboard column only | keyboard column only |

  Device columns wait for any HID element via `ConfigureHIDControl`.
- **Key names:** `WriteControlName` uses a jump table for Mac key codes 0x24–0x7F. Some names are hard-coded English strings: "F1"–"F15", "Command", "Option", "R-Option", "Control", "R-Control", "Shift", "unknown". Others are phrases (1520–1523, 1529, 1532–1534 are still used). HID controls are named "device: element" by `WriteHIDControlName` `101579a0`.
- **Amiga counterpart:** `FUN_0102e75c`, `FUN_01040790`, `FUN_0103c690`.
- **Impact (adopt):** refuse F1 on the keyboard column, as the Mac does, and allow 'P'. Add the spinning `controlsetup` model behind the screen (§14.6). Our 10-column layout can stay, because the Mac's 5 columns reflect HID plumbing. Optionally rename the gamepad columns to the Mac's "Device n" style.
- **Confidence:** C.

### 3.3 Multiplayer devices: `DoMultiplayerSetup` `1009f5b0`, `ControlMethodExists` `100f8420`, `GetNextFreeControl1/2` `1009c730`/`1009c810` — C

- The device cycle is −1 "Off" 2168, 0–3 "Device n", 12 "Keyboard" 2167.
- PSX (4–7) and CD32 (8–11) are removed, and "Joystick n" 2166 is no longer used.
- Devices 0–3 are offered only when HID is available. Each device can be used by one player only. Key sets 0–3 are unique per player.
- Rows, kills, city, Play and Cancel are unchanged.
- **New:** four 3-D objects are drawn, the same as the Amiga (`FUN_0102b2c8` also makes 4).
- **Amiga counterpart:** `FUN_0102b2c8`.
- **Impact (adopt):** drop the PSX/CD32 device entries if they still exist in our list. Offer Off / Gamepad 1–4 / Keyboard, each device usable once, with Gamepad entries shown only when a pad is present.

### 3.4 Repeat last message: `UpdateInputStatusNoKeys` `100f8090` → `RepeatLastMessage` `1008efe0` — C

- The Mac calls `RepeatLastMessage` on every poll while control 10 is held, with **no edge latch**. Previous/next weapon do have a latch (`DAT_1017d300/301`). The queue can overflow ("Enqueue(): Too many entries").
- The Amiga is edge-triggered.
- **Impact:** this is a Mac bug. Keep our edge-triggered `Dialog::repeat`.

### 3.5 Text entry: `EnterString` `1008c960`, `MayGetChar` `100f9540` — C

- The allowed set is the same as the Amiga's `FUN_01012e38`: letters, digits, space `! ( ) , - . ; : _`. Characters come from the key event's char code (Mac keyboard layout).
- `EnterString2`/`EnterString3` are copies used for the CD device and unit prompts.
- **Impact:** none.

### 3.6 Manual's default controls (English manual p.5) — C

- Arrows, Space shoot or horn, Return in/out, Z brake, 0–7 or Del/Help change weapon, M repeat message.
- The German manual lists **Y** as brake because of the QWERTZ layout; the code uses key code 0x06, which is a physical position.
- **Impact:** if we document controls, bind by physical key (scancode), as the Mac does.

---

## 4. Menus and front-end screens

All menu screens keep the Amiga phrase numbers, layout constants (N/200 of H, `xs`/`ys`), sounds (0x20/0xEB move, 0x21/0x10E select, 0x2D/0xA0 refuse) and the rotating option objects. A per-screen phrase-set diff (`phrdiff.sh`) shows identical phrase use except where noted below. Every Mac loop also polls `MacHasQuit` and returns −1.

### 4.1 Main menu: `DoTitleScreen` `100a2e70` ↔ `FUN_0102fc44` — C

- Same 9 items and phrases (2090–2094, 2454 Control setup, 2132 Set Language, 2109 View intro, 2130 Quit), same slide constants (±0x5DC, swap at ±0x1194), arrows and title.
- **View intro works:** `OpenAnimation("intro")` → QuickTime movie. If the music mode is not 3, CD music stops first; if the movie is missing, the refuse sound plays. The OS4 build refuses this item; Update 7 68k and the CD release play it.
- Name entry moved into `GetPlayerName` `10097880` (phrase 2088, default "Travis", 25 characters), with the same reset of times and cups.
- The CD Audio Setup still opens automatically the first time, when the device name is empty.
- **New, no visible effect:** the option object gains spring/velocity fields (`obj[0x1f7..0x1f9]`, ±4/frame towards target x/y/z). The slide still moves x and the target together, so the velocity stays 0 (I: dormant).
- **Impact (adopt):** the intro already plays from the menu and at start-up; adopt the Mac rule that it stops music unless the mode is Disabled. Nothing else changes.

### 4.2 Game menu: `DoGameMenu` `100a05e0` ↔ `FUN_0102ca08` — C

- Identical items, phrases and the post-Rampage high-score entry (1611, 25 characters; `AddHiscore`, `SaveHiscores` — but see §7.3).
- **Impact:** none.

### 4.3 Pause menu: `ShowOptions` `10093240` ↔ `FUN_0101a7dc` — C

- Same 6 items: Return, Quit, FX, Motion blur, Volume, Graphics options with 1601 Gouraud / 1602 Rotating view.
- Same hardware rules: motion blur shows "Off" dimmed and is refused; Gouraud is dimmed and skipped.
- Volume uses `DrawMeter` `10085ef0` (`LoadingPiece` filled / `LoadingPiece2` empty, 8 px or 16 px in hi-res) plus a "%d" number, as on the Amiga.
- Gouraud and rotating view are also dimmed when unavailable (`_DAT_10188fc4` / `_DAT_10188fc8`).
- `SetFlareSize(800/500/300)` while browsing items (the flare on the item models).
- The pause also stops `InitProgress` from stopping the music (`*_DAT_1017c4f0`).
- **Impact:** none. Our pause menu is a superset.

### 4.4 Music (CD audio) setup: `DoCDAudioSetup` `100a1500` ↔ `FUN_0102dd30` — C

- The layout and phrases are unchanged (2131, 2136/2137, 2140/2141, 2142+mode, 2148). The English text is renamed "MUSIC SETUP" / "Music setup", and the label lines get extra leading spaces.
- **New:** a spinning `cd` model (model 4) behind the screen; the Amiga has no 3-D object here.
- The device and unit prompts remain, but **`CD_Init` `100fae00` ignores them**. `UCDAudio` finds the CD by itself (§10).
- **Impact (adopt):** rename to "Music setup" / "MUSIC SETUP" with the indented labels (2094, 2131, 2138–2145 Mac text), and add the spinning `cd` model. Device and unit are meaningless for us, but keep the rows for layout fidelity or hide them.

### 4.5 Language chooser: `DoSetLanguage` `1008c2b0`, `InitLanguages` `1008ce90` ↔ `FUN_0101267c` / `FUN_0101345c` — C

- Same scan of `Phrases/Global.#?.phrases` and the same hard-coded "Please choose your language".
- **New:** after a change, the blood effect texture is reloaded. Language "Deutsch" loads `EffectTextures/Gas` into the blood slot; anything else loads `Blood` (`ReloadEffect`). See §14.1.
- **Impact (adopt):** see §14.1. Reload the blood/gas texture when the language changes.

### 4.6 Level select, Level complete, Game over, Game complete, Results, Rampage setup, Load/Save game, replay list and info, Credits — C

All match their Amiga functions:
- `SelectLevel` `1009b620` ↔ `FUN_01025544`
- `DoLevelCompleted` `10099eb0` ↔ `FUN_0102395c`
- `DoGameOver` `10098150` ↔ `FUN_01021434`
- `DoGameComplete` `10098e10` ↔ `FUN_01022348`; its jitter text is factored into `DisplayTextWibble`
- `DoMultiInfo` `1009ccd0` ↔ `FUN_01027330`
- `DoRampageSetup` `1009ebc0` ↔ `FUN_0102a428`
- `DoLoadGame` / `DoSaveGame` `10096950` / `10096d80` ↔ `FUN_0101f1a4` / `FUN_0101f738`
- `SelectMovie` `10097290` ↔ `FUN_0101fdcc`
- `ShowReplayInfo` `100960a0` ↔ `FUN_0101e618`
- `DoScroller` `10097930` ↔ `FUN_010206bc`

Same phrases and logic; only code factoring differs:
- City names come from new helpers: `GetCityName` `10084f10` (2170–2172, user-map name, or 2039), `GetJustCityName` `10084e40` (new phrases 1840–1842, used by the Rampage script `InitScript_Random`), `GetCityFilename` `10084c90`, `GetCityNumFromFilename` `10084d20` and `GetNumCities` `10084c50`.
- Game Over creates 1 object (Amiga 2). The level-start zoom map `ZoomInMap` `1009a9c0` (↔ `FUN_010247e4`) is still skipped under hardware rendering.
- Phrase changes: Rampage "Police aggressiveness:" becomes **"Difficulty:"** (2055). The credits text changes (§6.3).
- **Impact (adopt):** use "Difficulty:" (2055) in Rampage, and phrases 1840–1842 wherever `GetJustCityName` is used (the Rampage script intro).

### 4.7 Demo/registration screens: `DoDemoIntro` `100a4dc0`, `DoDemoOutro` `100a60c0`, `ShowDemoMessage` `100a4520` ↔ `FUN_01031e24` / `FUN_0103141c` — C (unreachable)

- The code is present but unreachable, because the game is always registered (§11).

---

## 5. HUD and text rendering (detail in `scratch_frontend/hud/`)

### 5.1 Layout and scaling at Mac resolutions — C

- `InitGraphics` `1008d110` uses the Amiga rule unchanged: hi-res (`_DAT_1017c890`) and icon doubling when W > 639 and H > 399. `ReadScreenParams` builds the same W/H/xmax/ymax/cx/cy/xs/ys.
- **Every Mac size (640×480, 800×600, 1024×768) is hi-res,** so the Mac always shows the 2× HUD.
- At 800×600 and 1024×768 nothing scales further. Elements stay 2× pixel size, anchored to left/top, xmax, ymax or cx, so the HUD gets relatively smaller.
- **Every element's offset equals the Amiga table** (frontend_audio §2.2):
  - `DrawWeapons` `10092480`, `DisplayAmmo` `10090490`, `DisplayScore` `10090f70`, `DisplayBonusMultiplier` `10091190`, `DisplayLives` `100910b0`
  - `DisplayHealth` `10091390`, `DisplayWanted` `10091270`, `DisplayTimer` `100914d0`, `DisplayLights` `10090db0`, `DisplayLap` `10090ea0`
  - `DisplayVehicleName` `1008f5f0`, `DisplayMinimap` `10091630`, `DisplayPlayback` `100909c0`, `DrawCheckpoint` `10095240`, `DrawAnnouncement` `1008fe10`
- Hi-res is not exactly 2× lo-res. Offsets hang off the real xmax/ymax: the score sits at xmax−145, not 2·(W/2−76).
- **Impact (adopt):** our HUD draws the lo-res layout on a 256-line canvas and stretches it. The Mac always shows the hi-res layout, so **adopt the hi-res HUD as the default whenever the window is ≥ 640×400**: a canvas of window/2 per 2× pixel, with the hi-res offsets above. Keep the lo-res layout for small windows, as the original does.
- **Doc nit:** the hi-res minimap image is at (xmax−141, 67); (xmax−142, 66) are the call arguments. Fix `frontend_audio.md` §2.2.

### 5.2 Message box, wrap, typing — C

- `DisplayMessage` `1008f840` and `UpdateAnnouncement` `10090010` are identical to `FUN_01016424` / `FUN_01016d7c`: geometry, (W−30)/7 wrap (hi-res (W−60)/14), word timing, scroll and SqueakyVoice.
- The only change is the repeat trigger (§3.4).

### 5.3 Text routines and fonts — C

- `DisplayText` / `DisplayTextAlpha` / `DisplayTextAlpha3` / `DisplayTextWibble` (`1008ecd0` / `1008bcd0` / `1008ee50` / `1008be50`) take `(s, x, y, pitch, font)` and double the pitch in hi-res.
- **New:** `DisplayTextAlpha3` / `DrawIconAlpha3` is a half-strength lighten (50% table against colour 0, then lighten). It is used only for dimmed menu items (pause, control setup, CD setup); the Amiga dims with `FUN_0100cc78`.
- **Glyph files and mapping are unchanged** (`LoadFont` `10089380`, `LoadFontPartial` `10087d80`; simulated slot tables equal the OS4 ones, `hud/big_slots.txt`, `small_slots.txt`). They are still Latin-1 numeric suffixes plus the `^` Latin-2 table. There are no MacRoman glyphs, and the Mac phrase files are still Latin-1 (§6.2).
- **No Mac system fonts are used in game.** The `Txtr` resources (Geneva, Monaco, "AudioReQuest LCD") belong to the dialogs.
- **Not Mac-specific (C):** no executable (OS4, 68k, Mac) loads the `Dollar`, `Percent` or `Pound` glyph files. "£" (0xA3) renders as "?" in the originals. Our `hud/icons.rs` `NAMED` loads all three. Recommendation: drop them for fidelity and fix the list in frontend_audio §1.3.
- **Not Mac-specific:** our banner centring uses byte length; the original uses `realstrlen`, which skips `^`.

### 5.4 Title screen and menu background — C

- The choice rule is the same: `TitleScreen` / `.medium` / `.big` / `.w3d`. On the Mac, W ≥ 640 always picks `TitleScreen.big` stretched, or `.w3d` in OpenGL.
- The four title files contain **new artwork**: a flaming PAYBACK logo with Apex Designs and Virtual Programming logos. The sizes are the same. See `scratch_frontend/data/title_*_amiga_vs_mac.png`.
- **Impact (adopt):** prefer the Mac title art (`TitleScreen*` from the Mac data) when available. The art is content, not a platform artefact. Include it as the default menu background when the Mac data is the source.

### 5.5 Split screen and kills box — C

- `DrawView` `1006eb80`, `DrawKills` `10092860` are identical to `FUN_0105bd98` / `FUN_010199ec`.

### 5.6 Progress bar — C

- `InitProgress` `10053510` and `DrawProgress` `100538c0` are the Amiga `FUN_01042ff8` / `FUN_010434a0` + `FUN_01042688` logic: blinking unknown-size mode, a hardware full redraw, and `CloseProgress` rewriting `.size`. `Loading` and its pieces are never doubled.
- The Mac `.size` values differ (§14.4).
- **Doc nit:** the Amiga `DrawProgress` path (2168 bytes) is bigger than frontend_menus §10 / frontend_audio §6.3 imply.

### 5.7 OpenGL HUD — C / I

- Icons are drawn as quads at k = 2 in hi-res, as on the Amiga Warp3D path.
- `W3D_SetFilter` `10154650` sets `GL_LINEAR`, so on OS X (always OpenGL) the 2× HUD and font glyphs are bilinearly smoothed (I). Software doubling uses the 50% blend.
- **Impact (adopt):** with the hi-res HUD, use linear sampling for 2× icons and glyphs in hardware mode (Mac OpenGL). Use blended doubling (`LoadIcon`) in software mode.

---

## 6. Languages and phrases (full diff: `scratch_frontend/data/phrase_diff_english.txt`, `phrase_diff_de_fr.txt`)

### 6.1 Languages — C

- **Amiga:** 14 languages (Czech, Deutsch, English, Español, Français, Italiano, Nederlands, Norsk, Polski, Slovak, Slovensko, Srpski, Suomi, Svenska).
- **Mac:** English, Deutsch and **Francais** (renamed to ASCII), 7 files each. A leftover `.FBCIndex` in `Phrases/` still lists all 14, so they were dropped for distribution.
- **Impact:** keep our language set, which is a superset. The Mac dropped 11 languages for distribution reasons (the Finder index proves they existed), so the reduction is not a design change. **Adopt the Mac texts for English, Deutsch and Français** as primary, and keep the other 11 Amiga languages as extras.

### 6.2 Encoding — C

- The Mac phrase files are still **Latin-1**: é 0xE9, ä 0xE4, ß 0xDF, £ 0xA3. They are not MacRoman, so the glyph mapping is unchanged.
- The German and French per-level files are byte-identical to the Amiga ones.
- 0xB0 "°" (French `00` #71) has no glyph on either platform and renders as "?".
- French lacks vehicle names 2330–2355 (the Amiga French file has them), so French vehicle banners are missing on the Mac (I).

### 6.3 English changes — C

**Per-level files:**
- `00` #0/#86: "Press RETURN/YELLOW/SQUARE to get in and out of vehicles." → "RETURN/YELLOW/SQUARE are the default controls to get in and out of vehicles."
- `11` #24: "Press return to take off and land." → "Use the same controls you use to get in and out of vehicles to take off and land."
- `11` #83: typo "launch ac RC" → "an RC".
- `21` #69: "f*cking" → "fecking".
- `01`, `10`, `20` only drop header comments.

**Global (498 → 657 entries; 136 changed, 205 added, 46 removed):**

| Change | Phrases | Used by the Mac game? |
|---|---|---|
| "Police aggressiveness:" → "Difficulty:" | 2055 | yes |
| "CD audio setup" / "CD AUDIO SETUP" → "Music setup" / "MUSIC SETUP" | 2094 / 2131 | yes |
| Indented CD labels | 2138–2145 | yes |
| Map-editor labels gain ":" | 1570/1571 | editor |
| Credits: "Mac conversion / Alex Curylo" added; localisation cut to co-ordinator + FR/DE; copyright "(c) 2002 Apex Designs / Published by Virtual Programming Ltd / www.vpltd.com"; ends at 2281 (2282–2326 removed) | 2205–2281 | yes (credits) |
| Demo nag rewrapped and de-Amiga'd ("Loads of huge cities", "High quality soundtrack", "English, German and French", vpltd.com; PSX/CD32, graphics-card and "support Amiga developers" lines removed; 2472 blanked; 2451 removed) | 2403–2472 | no (demo unreachable) |
| City names alone ("Freedom City"…) | **1840–1842 (new)** | yes (`GetJustCityName`, Rampage script) |
| Rampage phone-mission texts ("ANSWER THE PHONE", "NEW HIGH SCORE!"…) | 1400–1499 (new) | no |
| PC-style key names (Home, End, Insert, Page Up/Down, L/R-Ctrl, Pause, Menu, L/R-Start, Prnt Scrn) | 1535–1547 (new) | no |
| "a Mundaneo"…, compass areas | 1800–1838 (new) | no |
| Map editor, "Out of storage space", "use mouse/stylus", "Lap %d of %d", "Gamma correction" | 1845–1992 (new) | no |
| "Interference : Off/On" | 2146/2147 (new) | no |

- Unused new phrases look like they come from a later Apex codebase, perhaps a PC or handheld port, given "stylus" and "storage space" (I).
- **Still Amiga wording, used by the Mac:** 2049 "Press F1 to continue", 1522 "Help", 1528/1531 "L-/R-Amiga", 2164/2165 PSX/CD32, 2175–2178 key-set hints, 2136/2137 CD driver prompts.
- **Missing phrase (C):** `amiga_levelloop` `10100f50` shows phrase **1799** instead of 2023 "Level complete!" when a level ends with trigger result 2 **in Rampage mode** (`_DAT_1017c334`). 1799 is in no phrase file, so it would print "Phrase 1799 not found!". The Amiga (`FUN_010692bc` ~49721) always uses 2023. This is a Mac bug or unfinished feature; do not copy it.
- **Impact and recommendation (adopt):**
  - Use the **Mac English global and per-level texts as our English baseline**, and likewise the Mac Deutsch and Francais globals. That covers the control-neutral hints (`00` #0/#86, `11` #24), the "an RC" and "fecking" changes, "Difficulty:" (2055), "Music setup" (2094/2131) with the indented labels, and the new 1840–1842.
  - Credits: adopt the Mac credits (2205–2281), including "Mac conversion / Alex Curylo" and the 2002 copyright lines.
  - Keep phrases that are Amiga-hardware specific only where our UI still needs them; e.g. 2164/2165 PSX/CD32 become unused once those devices are gone (§3.3).
  - Do not wire up the unused new ranges (1400–1499, 1535–1547, 1800–1838, 1845–1992, 2146/2147): the Mac game never displays them.
  - Phrase 1799 is a bug (no text exists); keep 2023 at the end of a Rampage.
  - Our phrase loader reads Latin-1 bytes and is fine with both.

---

## 7. Saves, replays, high scores

### 7.1 Save slots: `ReadSaveGame` `1008b560`, `WriteSaveGame` `1008b7e0`, `SaveGameExists` `1008ba60`, `GetSlotName` `1008bb70` ↔ `FUN_0101182c` / `FUN_01011a70` — C

- The format is identical: 152 bytes, big-endian, version 1, `PROGDIR:Saves/PaybackSlot%d.save` then `S:` fallback, slots 1..14.
- Amiga and Mac save files are interchangeable.
- The Mac `Saves/` folder ships empty; only Finder index files mention a development `paybackslot1.save`.
- **Impact:** none.

### 7.2 Replays: `LoadMovie` `100fa7b0`, `SaveMovie` `100fab30`, `RecordMovie` `100f9ab0`, `InitMovies` `10084960` ↔ `FUN_010bc000` / `FUN_010bc3b0` / `FUN_01009b48` — C

- **Version 0x3F3** (OS4 0x3F2, 68k 10, CD release 3). Header fields in Amiga order:
  1. players, controller[4], kills, record count, ticks, seed, rampage, police;
  2. **the map path as a NUL-terminated string of variable length** (Amiga: fixed `char[100]`);
  3. level (city & 1);
  4. **u32 data byte count**, then the raw record buffer.
- The map is resolved by name (`GetCityNumFromFilename`). An unknown user map makes `LoadMovie` return 2.
- **Record words keep the Amiga bit layout** (`ReadInput` `10064bf0` builds them as `FUN_01051a10`), but the body is a run-length bit stream with 4-bit change codes and no comments (correction, `input_movie.md` §1).
- The Mac rejects versions 3, 10 and 0x3F2 (2116 "WRONG VERSION NUMBER"), so the Amiga replays cannot be played on the Mac.
- **Shipped replays:** none. `Replays/` holds only `.FBCIndex`/`.FBCLockFolder`, mentioning development names "test", "test2", "trav…". "Race for the prize" and "Tanks a lot" are not on the Mac CD.
- **Impact (adopt):** add read support for 0x3F3 (variable path, then level, then u32 length, then data) and **write 0x3F3** for new replays. Keep reading 3, 10 and 0x3F2. Check replay determinism (`tools/checks/replay_check.py --suite`) still matches (the port writes the stream losslessly).

### 7.3 High scores: `LoadHiscores` `1009e8a0`, `SaveHiscores` `1009eaa0`, `CalcHiscoreChecksum` `1009e7a0`, `InitHiscores` `1009e6a0` ↔ `FUN_01029b3c` / `FUN_0102a0fc` — C

- The format, defaults ("Travis", 500000·(t+1) halving) and checksum formula are identical. Paths: `<city file>.scores`, i.e. `PROGDIR:Maps/N.scores` or `PROGDIR:UserMaps/<name>.scores`.
- **Mac bug (C, disassembly):** `SaveHiscores` writes each name correctly but writes the stack slots `r1+0x40` (every score) and `r1+0x44` (the checksum), which it never fills.
  - On the next `LoadHiscores` the checksum fails and the defaults return, so **Mac Rampage high scores never persist**.
  - The shipped `Maps/0.scores` shows exactly this: every score is 0xBFFFEFA0 (an OS X stack address), the checksum is 0x006A5F60 against the expected 0xBA3B412C, and one name is "Matthew".
- **Impact:** do not copy. Our `payback-formats/src/scores.rs` should treat the Mac `0.scores` as invalid (the checksum fails, so defaults are used).

---

## 8. User maps

### 8.1 Loader: `InitUserMaps` `10084fe0`, `AddUserMap` `10084b30` ↔ `FUN_0100a1d4` / `FUN_01009e88` — C

- Same behaviour: `PROGDIR:UserMaps/#?.map`, skip `test.map`, at most 2000. The display name is the file name minus `.map`.
- **Impact:** none.

### 8.2 Bundled maps — C

- The "163" is a file count: Mac-only files make up 163 of 167 files, 20.3 MB.
- There are **32 `.map` files, 31 playable** (`test.map` is skipped).
- Companion files:

| Maps | Files |
|---|---|
| 17 | map, mini, thumb, route, textures, vehicles |
| 8 | map, mini, route, thumb |
| 3 | map, mini, textures, thumb, vehicles |
| 2 | map, mini, route, thumb, vehicles |
| 1 | map, mini, route, textures, thumb |
| 1 (`House Park 2 Night`) | map, mini, thumb |

- There are no `.scores` or `.objects` files.
- Four maps have no `.route` file (Kozie_Klefki, MultiplayerTournament, Tankwarfare, House Park 2 Night).
- Names are ASCII with spaces and underscores; the longest is 22 characters.
- Several maps are night, snow or rain maps.
- **13 maps use custom textures** (texture numbers ≥ 2000, `<map>.textures`, up to 112 slots).
- `The Killing Zone` is byte-identical to the Amiga copy.
- Per-map breakdown: `scratch_frontend/data/usermaps_check.py`.
- **Impact:** **we have no `.textures` loader** (`map.rs` documents texture numbers ≥ 2000, but nothing reads them). Those 13 maps would render with missing faces. Implement `<map>.textures` loading; the format is the Map Editor's, and the files parse exactly per the check script. Missing `.route` and `.vehicles` files are already handled.

---

## 9. Intro video

### 9.1 Playback: `OpenAnimation` `100fb130`, `PlayAnimation` `100fb1d0`, `CloseAnimation` `100fb5b0`, `GetPaybackMovie` `100fcbc0` ↔ 68k `FUN_00010364` / `FUN_000106c0` / `FUN_00011402` — C

- QuickTime opens `:Videos:payback_high_mpeg1.mpg`. This file name is the only one in both Mac binaries; `payback_high_mpeg2.mpg` and `payback_low_mpeg1.mpg` are never referenced.
- The movie is scaled to fit the window/screen (`ScaleRectToFit`). At width 640 a 1-px black border is painted and the box inset by 1.
- **Any key or mouse button skips** it. Events are flushed afterwards.
- It plays **automatically at start-up** after the cube-texture list (`InitAll`, line ~23299), as on 68k Update 7 and the CD release, and from main-menu "View intro".
- **Files:**

| File | Video | Bitrate |
|---|---|---|
| `payback_high_mpeg1.mpg` | MPEG-1, 640×480, 25 fps | 4.8 Mbit/s |
| `payback_high_mpeg2.mpg` | MPEG-2 | — |
| `payback_low_mpeg1.mpg` | 320×240 | 1.2 Mbit/s |

  All three: 1800 frames (72 s), MPEG-1 Layer II audio 48 kHz mono.
- The content is a **different, live-action/CG intro**, not the Amiga's 30 s 320×200 FLC (`data/mac_intro_contact.png`).
- **Impact (adopt):** the Mac intro is the newer content. **Play `Videos/payback_high_mpeg1.mpg` when present** (needs an MPEG-1 + MP2 decoder crate), skippable by any key or click and scaled to fit, and fall back to the Amiga FLC + PCM. This is medium priority because a dependency is needed.

---

## 10. Music (CD audio)

### 10.1 CD playback: `UCDAudio` (`101038f0`…; `FindFinderVolume` `10103c00`, `GetTOC` `10103e20`, `CountTracks` `10104520`, `Play` `10103fc0`); `CD_Init` `100fae00`, `CD_CheckForTrackEnd` `100fb000`, `CD_Play` `100faf00`, `CD_SetVolume` `100fb100` ↔ `FUN_01008cec` + "Payback CDDA Thread" — C

- **Finding the disc:** the Mac scans mounted volumes. A volume named "PaybackMac" sets the "CD present" flag (§1.1). A volume with file-system signature 0x4A48 (Apple CD audio file system) is used for music. Tracks are counted from `.TOC.plist` on OS X and played as QuickTime movies (`OpenMovieFile`/`NewMovieFromFile`/`StartMovie`).
- **Policy, simplified:**
  - `CD_CheckForTrackEnd(0)`: if nothing is playing, play the next track.
  - `CD_CheckForTrackEnd(-1)`: stop.
  - `CD_CheckForTrackEnd(n)`: if nothing is playing, play track n.

  This is the same effective behaviour as the Amiga policy. Menus still use track 13.
- `CD_Init` ignores the device name and unit. **`CD_SetVolume` is an empty stub**, so CD music volume is fixed (`SetVolume(200,200)` at start).
- Default mode is **Radio** (Amiga: Disabled).
- **Impact (adopt):** default `cd_mode` to 1 (Radio) instead of 3 (our default in `frontend/progress.rs`). The QuickTime/volume-scan plumbing is a platform artefact. Keep our music volume control (the empty Mac `CD_SetVolume` stub is an artefact, not intent).
- **Confidence:** C (code); I (intent).

---

## 11. Registration, keyfile, demo, copy protection

### 11.1 Keyfile: `amiga_init` `100ffc70` ↔ Amiga keyfile check in `FUN_010692bc`, `FUN_0101dfe4` — C

- The Mac sets the registered flag `*_DAT_1017c3b4 = 1` unconditionally after `InitAll`.
- There is no `Payback.key2` path, plain or +1-obfuscated, in either Mac binary, and no keyfile in the data.
- Phrases 2190–2194 (keyfile prompts) are unused.
- The demo time-out (9000 ticks, phrase 2490 "Time out!") is still in `amiga_levelloop` but is gated off by the flag.
- `Display_LoadTex` `1008e770` loads all three thumbnails only when registered.

### 11.2 Copy protection — C

- The game will not launch without the CD named "PaybackMac" mounted (§1.1).

- **Impact:** we already ignore the keyfile (registered behaviour). No CD check.

---

## 12. Cheats and debug options — C (absence), I (completeness)

- No cheat strings or key sequences were found. `chkKeyBoard` `1004d8b0` is just "first set byte" over a 16-byte key map.
- `printf_special` debug logging remains: "InitProgress(): Called more than once…", "Phrase %d not found!", "Phrase %d too long - must be 400 chars or less…".
- The PowerPlant DEBUG menu (MENU 600, submenus 225–234) and the Validate PPob / Eat Memory / LCommanderTree / LPaneTree windows (PPob 601–608) are compiled in but not in any MBAR, so they are unreachable.
- **Impact:** none.

---

## 13. Multiplayer and network — C

- There is no network play: no OpenTransport, NetSprocket, sockets or AppleTalk imports, and no network strings in either Mac build.
- Multiplayer is the same 2–4 player split-screen deathmatch (manual p.9). Devices are as in §3.3.
- **Impact:** none.

---

## 14. Other differences

### 14.1 German blood → Gas: `InitAll` (~line 23215) and `DoSetLanguage` ↔ Amiga `FUN_0104feb0` (always "Blood") — C

- When the configured language is "Deutsch", the blood effect slot loads `EffectTextures/Gas` (with `API_3D_Set_Alpha3`) instead of `Blood` (with `Set_Alpha2`). The language chooser reloads the matching texture.
- This is a German-market censorship artefact.
- **Impact (adopt, with a switch):** when the language is Deutsch, load `EffectTextures/Gas` into the blood slot (alpha mode 3) as the Mac does, and reload it on language change. This is market-specific content rather than platform plumbing, so adopt it, but give it a setting (default on for Deutsch) so it can be turned off.

### 14.2 Data file names — C

- The Mac opens `PROGDIR:CubeTextures/CubeTextureList.txt` and `CubeTextureRemapList.txt`; the Amiga uses `.CubeTextureList` and `.CubeTextureRemapList`. The content is identical.
- **Impact:** our `data.rs` reads `.CubeTextureList` only. Add a fallback to the `.txt` names so the Mac data folder works as a data source.

### 14.3 Timing — C

- `amiga_levelloop` paces frames with `TickCount` (60 Hz): it waits until `last + frames·2`, with frames clamped 1..5. That is 30 Hz logic, as on the Amiga.
- **Impact:** none.

### 14.4 General/ files — C

- `startup.size` 0xB6D29 → 0xAF5C3, `lighting.size` 0x2F80 → 0x21C4, `ReloadObjectTextures.size` 0x3519C → 0x395F8. These are self-calibrating, so there is no impact.
- `Gouraud.256` (8 MiB pregenerated cache) is new; the game regenerates it if missing.
- `Alpha.256` differs slightly.
- The Amiga-only `CD_Audio_Handler`, `led` and `Run` are absent.
- `Maps/*.script` files are not shipped. `2.thumb` is re-rendered.

### 14.5 Map Editor (`MapEditor (OS X)`, `Map Editor (OS 9)`; manual pp.11–18 vs Amiga `Docs/MapEditor-English.guide`) — C (docs)

- The key set is the same.
- Texture import takes QuickTime-readable files (PICT, GIFF, JPEG, BMPf, qtif, 8BPS, PNGf) instead of IFF.
- The manual says resolution can be chosen; the Amiga editor uses the game's screen mode.
- The editor still edits `UserMaps/Test.*` only.
- **Impact:** none for the game.

### 14.6 3-D objects on more screens — C

- The Mac adds rotating option models behind Control Setup (`controlsetup`) and Music Setup (`cd`). The Amiga draws none there.
- **Impact (adopt):** draw the `controlsetup` model behind Control Setup and the `cd` model behind Music Setup. Use the same rotation as the main menu, plus the Mac's z wobble (±2 per frame around 6000).

---

## 15. Recommendations for our implementation (adopt Mac by default; priority order)

**Adopt: content and data**
1. **`<map>.textures` loader** for custom user-map textures (§8.2). 13 of the 31 Mac maps need it. Bundle or support the Mac's 31 user maps as the default user-map set.
2. **Mac texts as the baseline** for English, Deutsch and Français: global, per-level and credits (§6.3). Keep the 11 extra Amiga languages as extras (§6.1).
3. **Mac title-screen art** as the default menu background when available (§5.4).
4. **Mac intro video** (`payback_high_mpeg1.mpg`) when present, with the FLC as fallback (§9.1).
5. **Data-folder compatibility:** read `CubeTextureList.txt` / `CubeTextureRemapList.txt` as fallbacks (§14.2). Treat the Mac `0.scores` as invalid (§7.3).

**Adopt: behaviour and options**
6. **Hi-res HUD layout as default** at ≥ 640×400, with linear sampling in hardware mode (§5.1, §5.7).
7. **Mac defaults:** music Radio, volume 50, P1 Keyboard, P2 Gamepad 1, FX 2, Gouraud on (§2.1, §10).
8. **Single-player keyboard + gamepad 1 simultaneously** (§3.1). **Multiplayer devices** Off / Gamepad 1–4 / Keyboard, without PSX/CD32 (§3.3).
9. **Control setup:** refuse F1 and allow 'P' (§3.2).
10. **Screens:** rename "Music setup", indented labels, "Difficulty:" (§4.4, §4.6). Add rotating models behind Control Setup and Music Setup (§14.6). The intro stops music unless mode 3 (§4.1).
11. **Resolution presets** 640×480 / 800×600 / 1024×768 first. Settings dialog changes apply live; save settings on quit (§1.2, §1.4).
12. **Replays:** read and write version 0x3F3, still reading 3/10/0x3F2. Verify replay determinism (§7.2).
13. **German blood → Gas** when the language is Deutsch, with an off switch (§14.1).

**Do not adopt (platform artefacts or Mac bugs)**
- the CD-present launch check (§1.1, §11.2);
- the native setup dialog and menu bar, Apple events, the PowerPlant DEBUG menu (§1.2–1.5, §12);
- the preferences atom container, Mac key codes and HID records (§2.1, §3.1);
- the OS X OpenGL-only renderer (OS 9 still has QuickDraw) (§1.2);
- the empty CD-volume stub (§10);
- the dropped languages (§6.1);
- the high-score save bug (§7.3), the repeat-message flood (§3.4), phrase 1799 (§6.3).

**Doc fixes (not Mac-specific)**
- the hi-res minimap position (§5.1);
- the unused Dollar/Percent/Pound glyphs (§5.3);
- banner centring with `realstrlen` (§5.3);
- the size of the Amiga progress code (§5.6).

## 16. Working files (`analysis/mac/scratch_frontend/`, local, not kept)

**Resources and manuals**
- `rsrc_macx.txt`, `rsrc_mac9.txt`: resource dumps (`dump_rsrc.py`)
- `manual_*.txt`: manual text

**Tools**
- `mfn.sh`, `phrdiff.sh`, `macdis.py`, `macdis2.py`, `mstr.py`, `pidata.py`, `macx_data.bin`

**Extracted functions:** `title.c`, `ctrl.c`, `mp.c`, `cd.c`, `pause.c`, `gm.c`, `amiga_title.c`, `amiga_ctrl.c`

**`data/`**
- `phrase_diff_english.txt`, `phrase_diff_de_fr.txt`, `phrase_diff_english_vs_cd.txt`
- `scores_decode.py`, `usermaps_check.py`, `mpeg_info.py`
- title, thumb and intro images

**`hud/`**
- `hud_r.c` (all Mac HUD functions with resolved strings), `initgraphics_r.c`, `progress_r.c`, `lf_r.c`, `lfp_r.c`
- `fontsim.py`, `big_slots.txt`, `small_slots.txt`
- `amiga_frame.c`, `amiga_initgfx.c`
