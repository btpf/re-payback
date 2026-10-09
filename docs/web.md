# Web build (WebAssembly)

The game also builds for the browser (`wasm32-unknown-unknown`, Bevy's
WebGL2 renderer). The same simulation code runs; only file access, the
command line and the window differ.

## Building

Prerequisites (once):

```sh
rustup target add wasm32-unknown-unknown
# The CLI must match the wasm-bindgen crate in Cargo.lock (0.2.129 now):
cargo install -f wasm-bindgen-cli --version 0.2.129   # or: cargo binstall wasm-bindgen-cli@0.2.129
```

optionally `cargo binstall wasm-opt` (binaryen; shrinks the module), plus
clang and llvm-ar with the wasm32 target (the LLVM release for Windows
has it; `tools/build_web.sh` looks in `C:\Program Files\LLVM\bin`, else on
`PATH`, or set `LLVM_BIN`) for the vendored pl_mpeg, and Python 3 for the
data packer. `wasm-opt` (binaryen) is used when on `PATH`.

Build:

```sh
tools/build_web.sh            # release build + data pack -> web/dist/
tools/build_web.sh --no-data  # code only, keep web/dist/data
tools/build_web.sh --no-film  # pack without the Mac intro film (-46 MB)
CARGO_FLAGS="-j 4" CARGO_TARGET_DIR=target/web tools/build_web.sh
```

The build uses the `web` cargo profile (release with `opt-level = "s"`,
thin LTO, stripped; root `Cargo.toml`), then `wasm-opt -O2` when present.
A first build takes 15-30 minutes.

Output, all git-ignored:

| file | what |
|---|---|
| `web/dist/index.html` | copy of the committed loader `web/index.html` |
| `web/dist/payback.js`, `payback_bg.wasm` | wasm-bindgen output (about 48 MB, 12.7 MB gzipped) |
| `web/dist/data/payback.pak`, `payback.json` | **your** game data, packed by `tools/pack_web_data.py` |

The data pack is the original game's data and must never be committed or
published. It is made from the locally extracted copy (`extracted/game`
and `extracted/mac/Payback`, or `PAYBACK_DATA` / `PAYBACK_MAC_DATA`), and
a `Music/` folder of ripped CD tracks if there is one. Executables,
editors, docs, resource forks and the two Mac films the game never plays
are left out (about 120 MB remain, 46 MB of it the Mac intro film).

## Serving

Any static file server works; the files must come over HTTP (not
`file://`):

```sh
python -m http.server -d web/dist 8080
# open http://localhost:8080/
```

The page fetches the wasm, then the data pack (with a progress bar), then
waits for a click: browsers only start audio after a user gesture.
`?autostart` skips the click (sound may then stay muted until the first
click or key press).

Command-line options become query parameters: `?map=3&level=1`,
`?intro`, `?seed=1234`, `?rampage=2`, `?autoplay` ... (`key=value` is
`--key value`, a bare `key` is the flag `--key`; see `web::query_args`).

## How it works

- `crates/payback/src/vfs.rs`: all game-data and save file access goes
  through it. Natively it is `std::fs`. On the web the page hands the
  data pack to `vfs_load` (one blob plus a path index); the Amiga data is
  `data/game/...` and the Mac data `data/mac/...`, which
  `GameData::locate` / `locate_mac` pick up as their defaults. Directory
  listings (user maps, replays, phrases, object textures, music) come from
  the same index.
- Saves: `frontend::progress::saves_dir()` is `saves` on the web, and the
  vfs keeps every file under it in `localStorage` under the key
  `payback/saves/<file>`, base64-encoded, in exactly the native format:
  save slots (`PaybackSlotN.save`), settings (`Payback.cfg`, stored as
  base64 text too), high scores (`<city>.scores`, user maps'
  `UserMaps/<name>.scores`), replays saved by the player
  (`Replays/<name>`) and the loading-bar sizes. To export one, in the
  browser console: `atob(localStorage["payback/saves/Payback.cfg"])`
  (binary files: decode to bytes and save them into the native `Saves/`
  folder). `frontend::progress::{read_file, write_file}` is the one door
  for save IO.
- `crates/payback/src/web.rs`: `start()` (called by the page once the data
  is in) runs the same `run_game()` as the native `main`; the query string
  stands in for `std::env::args` (`crate::args()` everywhere); the window
  is the page's `#payback` canvas, resized with the page.
- Timing uses `bevy::platform::time::Instant` (std's panics on
  wasm32-unknown-unknown); the replay seed clock uses `Date.now()`.
- The Mac intro film: pl_mpeg is compiled with clang for wasm32 against a
  tiny libc shim (`crates/payback-mpeg/wasm-libc/`, with `malloc` / `free`
  / `realloc` in `src/wasm_libc.rs` over Rust's allocator).
- Renderer: WebGL2 (Bevy's default web backend), which every current
  browser has; none of the custom shaders use storage buffers or compute,
  which WebGL2 lacks. WebGPU would need Bevy's `webgpu` feature and a
  browser with WebGPU enabled.
- Bevy runs single-threaded on the web (no wasm atomics); the simulation
  is the same code, so replays recorded natively play back the same.

## Testing

Checked with headless Chrome (SwiftShader WebGL2) driven over the DevTools
protocol (`analysis/scratch/web/cdp.mjs`, a fixed `--user-data-dir`): the
Mac intro film plays, the language chooser, main menu and a level
(`?map=0`) render with no console errors; `Payback.cfg` and a save slot
written in one page load are read back in the next (the language is not
asked again; Load Game lists the slot). Chrome must be closed cleanly for
localStorage to reach the disk.

## Limitations

- No window settings: Resolution, Window mode and Vsync are shown dimmed
  in the Graphics options (the canvas fills the page; the browser paces
  frames, so the frame limit does nothing either).
- No user maps from outside the pack (no file system); maps in the game
  data's `UserMaps/` and the Mac's bundled ones work.
- localStorage holds about 5 MB per site: plenty for saves and scores, but
  a few hundred long replays would fill it (saving then fails with the
  refuse sound and a warning in the console).
- Saves are per browser profile and per origin (host and port): another
  port is another set of saves.
- Debug options that write files (`--record`, `--dump`, `--screenshot`)
  do nothing on the web.
- Performance is below the native build (single thread, WebGL2).
- WebGL2 has no `noperspective` interpolation: `city.wgsl` emulates it
  under the `WEBGL2` shader def (attribute times w, divided per fragment).
  New shaders must avoid storage buffers, compute and
  `@interpolate(linear)` (or add the same fallback).
