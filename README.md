# Payback reimplementation

A reverse-engineered reimplementation of [Payback](https://en.wikipedia.org/wiki/Payback_(video_game)) (Apex Designs, Amiga, 2001) in Rust with [Bevy](https://bevyengine.org), plus the research behind it.

If there are any issues, please open an issue or PR with screenshot preferably and I would happily review the changes. 

![](https://private-user-images.githubusercontent.com/61168382/670853452-1c722d22-5d59-4d39-82ff-dae729981208.png?jwt=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJnaXRodWIuY29tIiwiYXVkIjoicmF3LmdpdGh1YnVzZXJjb250ZW50LmNvbSIsImtleSI6ImtleTUiLCJleHAiOjE3OTE3NDUxNzMsIm5iZiI6MTc5MTc0NDg3MywicGF0aCI6Ii82MTE2ODM4Mi82NzA4NTM0NTItMWM3MjJkMjItNWQ1OS00ZDM5LTgyZmYtZGFlNzI5OTgxMjA4LnBuZz9YLUFtei1BbGdvcml0aG09QVdTNC1ITUFDLVNIQTI1NiZYLUFtei1DcmVkZW50aWFsPUFLSUFWQ09EWUxTQTUzUFFLNFpBJTJGMjAyNjEwMTElMkZ1cy1lYXN0LTElMkZzMyUyRmF3czRfcmVxdWVzdCZYLUFtei1EYXRlPTIwMjYxMDExVDE4NTQzM1omWC1BbXotRXhwaXJlcz0zMDAmWC1BbXotU2lnbmF0dXJlPWUyNzY1N2RlMTU2ZWI4MmY3Njk3OTQ2YTNkZTk0NGE1NDE0NzAyODAwYmNjYjFmMWExMDU2NjE3ZTQyNGZkNmUmWC1BbXotU2lnbmVkSGVhZGVycz1ob3N0JnJlc3BvbnNlLWNvbnRlbnQtdHlwZT1pbWFnZSUyRnBuZyJ9.76XErXQMsCwFO921Fcbcs0kime2i8l1Se8DYzYqQalI)
### Features

- A from-scratch Rust and Bevy reimplementation of *Payback* (Apex Designs, 2001), built from reverse-engineering the original Amiga and Mac releases.
- Game logic follows the decompiled originals function by function: driving physics, collisions, AI traffic and police, pedestrians, combat, the wanted level and scoring. Code comments cite the original function and address.
- A toggle to switch between Amiga Assets and MacOS Release Assets
  - Both Intro videos
  - Both Textures
  - Both Models
- Quality Of Life Improvements
  - Smooth Motion Interpolation to any frame rate
  - A custom Cheat menu by pressing F12
  - Modern Controller Support
  - Ability to unlock all straight from the main menu

### Motivations

Payback was a GTA Clone for the Commodore Amiga and PPC Mac Computers. It was a childhood favorite of mine but as of 2026 is impossible to emulate at full speed and quality, and difficult to play without the original vintage hardware. 

I created this in the name of preservation and to test the reverse engineering capabilities of frontier AI Models.
The bulk of this game was recreated in ~3 days using opus 5.5 at medium effort and used around \$1500 worth of API Credits.

I had no prior knowledge of the original Motorola 68k Instruction Set, nor any ability to reverse engineer. This was all done with a handful of short high level prompts and some manual testing.



## AI Written & Maintained Notes Below

It is a **behavioural port**, not a matching decompilation: the game logic is
rewritten from the decompiled original (the Amiga Update 7 and AmigaOS 4 builds
and the later Mac OS release, whose OS X build kept its function names) and
checked against it function by function. The Mac release is treated as the
definitive version; where the Amiga version differs, a setting picks it.

**Status:** playable end to end. The six story levels, Rampage (with the Mac's
random phone missions), 2-4 player split screen, replays, saves, the front end,
HUD, sound and the Mac intro film are in, with software and hardware (Warp3D-style)
renderers and a WebAssembly build. Details and what is left:
[`docs/status.md`](docs/status.md).

## Legal

No original game data and no decompiled code are in this repository, and none
may be committed: you need your own copy of the game. Everything derived from it
(`extracted/`, `analysis/`, `out/`, `web/dist/`, the report pages) is git-ignored.
The notes in `docs/` describe the original's behaviour in their own words and
quote at most a few lines (see [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md)).

## Quick start

**Prerequisites:** Rust (edition 2024, stable); Python 3.11+ with `7z` on the
`PATH` (and `pip install machfs` for the Mac release); for the RE tools also
`pip install capstone numpy pillow markdown`; Docker for the decompiles; for the
web build see [`docs/web.md`](docs/web.md).

1. **Get the data.** Put your copies in `Payback Game + Updates/`:
   `Payback.iso`, `Updates/PaybackUpdate7.lha` (and optionally
   `Updates/paybackupdateOS4Native.lha`, `Updates/Warp3D-42.lha`, and the Mac
   release `PaybackMac.toast`). Then unpack them:

   ```sh
   python tools/extract/extract.py       # -> extracted/game (CD + Update 7), iso, os4, ...
   python tools/extract/extract_mac.py   # -> extracted/mac (optional, recommended)
   ```

   Either release alone also works: with only `PaybackMac.toast` the game
   runs from the Mac data (Mac content; English, Deutsch and Français; no
   Amiga variants, FLC intro or shipped replays). `--mac-only` tries that
   setup while the Amiga data is present.

2. **Build and run.**

   ```sh
   cargo run --release -p payback                  # menus
   cargo run --release -p payback -- --map 0       # straight into city 0, level 0
   ```

   The build copies the data files the game reads next to the executable
   (`target/<profile>/data/amiga` and `data/mac`, hard links where it can;
   the list is [`crates/payback/data_files.txt`](crates/payback/data_files.txt)),
   so `target/release/` is a self-contained folder: double-click
   `payback.exe`, or copy the folder elsewhere and run it there. The data is
   your own copy of the game: never commit or share it.

   Where the game looks for its data, first match wins: `PAYBACK_DATA` /
   `PAYBACK_MAC_DATA`, `--data <dir>` / `--mac-data <dir>`; `data/amiga` and
   `data/mac` next to the executable; the folder remembered in
   `payback-data.cfg` next to the executable; `extracted/game` and
   `extracted/mac/Payback` in the executable's folder or any folder above it,
   then in the working directory or above. If neither is found, a folder
   picker asks for one (the folder with `Payback.exe`, or the Mac CD's
   `Payback` folder) and remembers the choice. The log
   says which folders were used. `PAYBACK_NO_DIALOG=1` turns the picker and
   message boxes off (headless runs). `PAYBACK_STAGE_DATA=0` builds without
   staging the data (`=copy` copies instead of linking);
   `python tools/dist/stage_data.py <dir>` stages it next to an executable
   elsewhere.

   `--screenshot out.png` saves a frame and exits; the
   full option list is at the top of `crates/payback/src/main.rs`, the controls
   in [`docs/controls.md`](docs/controls.md).

3. **Web build** (optional): `tools/web/build_web.sh`, then serve `web/dist/`
   ([`docs/web.md`](docs/web.md)).

## Music

The game's music is the Red Book audio on the game CD; neither disc image
holds it. Rip the audio tracks from your CD (any of WAV, Ogg Vorbis, MP3,
FLAC) into a `Music/` folder next to `payback.exe` (beside `UserMaps/` and
`Saves/`; in a source checkout, `target/release/Music/`), one file per track,
named by its CD track number: `13.ogg`, `13 - anything.flac` or
`Track13.wav`. Track 1 is the data track: rip tracks 2–16. Without
tracks the Music setup shows the mode as Disabled.

As on the Mac, the music volume is linked to the sound volume by default:
full up to 50, then quieter, and silent at 100. "Music volume" on the
Music setup screen, or the Music line under the sound bar in the pause
menu (fire switches rows, up/down change the value), gives it a level of
its own.

From a raw BIN/CUE image of the CD (2352-byte sectors),
`python tools/extract/rip_cd_tracks.py <cue> <Music dir> [--flac]` splits
the tracks out (defaults: `Payback Game + Updates/cd/Payback-CD.cue` into
`target/release/Music`; `--flac` needs `ffmpeg`).

| CD track | Plays |
|---|---|
| 2 | radio: Limo, Hotrod |
| 3 | radio: Taxi |
| 4 | radio: Vapour |
| 5 | radio: Diblo, Pug Racer, Vapour GT |
| 6 | radio: Pug GTI |
| 7 | radio: Mundaneo, Fjord |
| 8 | Level / Game Complete; radio: Boat |
| 9 | radio: Evo |
| 10 | radio: Scooby |
| 11 | radio: Pug |
| 12 | radio: Truck, Rocket car |
| 13 | all menus |
| 14 | radio: Pickup truck |
| 15 | radio: Van |
| 16 | Game Over; radio: RC truck, Remote controlled car |

Bus, police vehicles, Tank, Ice cream van and Helicopter have no radio. In
Continuous mode every track plays in number order. The release names no
track titles (the credits list the artists only); details in
[`docs/re/frontend_audio.md`](docs/re/frontend_audio.md) §4.6.

## Verify

```sh
cargo test -p payback -p payback-formats             # data tests run when extracted/ exists
python tools/checks/replay_check.py --suite          # replay determinism: every case must MATCH
tools/checks/autoplay_all.sh                         # the bot plays the six story levels
```

The replay suite records a scripted run and plays it back with the same build;
it checks that the port is deterministic, not that it matches the original.
`tools/checks/` also has a frame-time check, a user-map smoke test and a
headless-browser test for the web build ([`tools/README.md`](tools/README.md)).

## Reverse engineering

```sh
docker build -t payback-re .         # Ghidra 12.1.4 workbench
tools/ghidra/decompile_all.sh        # fills analysis/ with every decompile
python tools/re/fn.py macx DrawMap   # print a function; --sym translates between builds
```

Then read [`docs/README.md`](docs/README.md), the index of the notes, and
[`docs/decompiling.md`](docs/decompiling.md).

## Layout

| Path | Contents |
|---|---|
| `crates/payback-formats/` | engine-independent parsers for the game's files |
| `crates/payback/` | the Bevy game |
| `crates/payback-mpeg/` | MPEG-1 playback for the Mac intro film (vendored `pl_mpeg`, MIT) |
| `docs/` | RE notes and project docs ([index](docs/README.md)) |
| `tools/` | extraction, Ghidra, RE helpers, checks, reports, web build ([index](tools/README.md)) |
| `web/` | the WebAssembly loader page |
| `Dockerfile` | RE workbench: Ghidra, radare2, amitools, FS-UAE, Rust |
| `target/<profile>/data/` | git-ignored: the game data staged next to the built executable |
| `extracted/`, `analysis/`, `out/`, `scratch/` | git-ignored: your game data, the decompiles, generated output, throwaway work |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).
