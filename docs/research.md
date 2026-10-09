# Research notes

Payback (Apex Designs / James Daniels, Amiga 2001) is a top-down 3D
GTA-style game: a 128×128 voxel-like city rendered in software (or Warp3D),
missions driven by per-level scripts, vehicles, pedestrians and police.

## Source material

| Item | Use |
|---|---|
| `Payback.iso` | CD release; base data |
| `PaybackUpdate7.lha` | last official update; **the version we target** |
| `paybackupdateOS4Native.lha` | community AmigaOS 4 PPC port (ELF) |
| `Warp3D-42.lha` | Warp3D driver; not needed |

`python tools/extract.py` unpacks all of it into `extracted/` and builds the
merged install in `extracted/game/`.

## Binaries

All builds are compiled C (not hand-written assembly) and stripped of
symbols, but full of debug strings naming functions (`LoadMap`,
`Obj_GetX`, `ReloadPhrases`, `AddSound_script`, ...).

| Binary | Format | Notes |
|---|---|---|
| `Payback.exe` (Update 7) | 68k hunk, 25 hunks | matches our data; primary reference |
| `PaybackWOS.exe` | WarpOS PPC hunk | same code base, PPC |
| `PaybackOS4` | ELF32 PPC, static | same code base; best decompiler output |
| `Maps/*.script` | 68k hunk | mission logic, loaded per level |

Decompiled output (not committed; regenerate as below):

- `analysis/ghidra_os4/` — PaybackOS4: `decompiled.c`, `functions.tsv`,
  `strings.tsv` (each string lists its referencing functions).
- `analysis/ghidra_68k/` — Update 7 `Payback.exe`, flattened by
  `tools/hunk.py --flat` (hunks at 0x10000, 4 KiB aligned; layout in
  `analysis/flat/Payback68k.bin.map`).

```sh
docker build -t payback-re .
python tools/hunk.py --flat extracted/game/Payback.exe analysis/flat/Payback68k.bin
docker run --rm -v "$PWD:/work/impl" payback-re analyzeHeadless /tmp/p p68k \
  -import /work/impl/analysis/flat/Payback68k.bin -loader BinaryLoader \
  -loader-baseAddr 0x10000 -processor 68000:BE:32:MC68020 \
  -scriptPath /work/impl/tools/ghidra -preScript SeedFlat68k.java 10000 117000 \
  -postScript ExportAll.java /work/impl/analysis/ghidra_68k
```

### Known functions

| Address | Build | Role |
|---|---|---|
| `FUN_01079300` | OS4 | LoadMap |
| `FUN_0106b7a0` | OS4 | GetCubeTexture (lazy-loads `cubetextures/%s`) |
| `FUN_0104feb0` | OS4 | screen setup; loads `.CubeTextureList` |
| `FUN_010bdab0` | OS4 | loads `64->256.dat`, looks up named object textures |
| `FUN_010024fc` | OS4 | Warp3D texture conversion; reads palette at `0x011490b8` |
| `FUN_000d7874` | 68k | loads all object models; scenery table `0x348000` |

## Reverse-engineering notes

Facts are marked CONFIRMED (with the original function address) or INFERRED.

| Note | Covers |
|---|---|
| `docs/formats.md` | data file formats (maps, objects, textures, palette, phrases, sounds, companion files) |
| `docs/re/rendering.md` | city renderer, slopes, faces, camera, water, weather, lighting, effects, object face modes |
| `docs/re/vehicles.md` | vehicle table, physics, damage, AI drivers, traffic |
| `docs/re/people.md` | player and pedestrians on foot, `Human.motion`, collision, boarding, AI states |
| `docs/re/combat_police.md` | `.route` graph, police and wanted level, weapons, projectiles, explosions, scoring |
| `docs/re/frontend_menus.md` | title, menus, config, saves, replays, multiplayer, rampage, `.thumb`/`.mini` |
| `docs/re/frontend_audio.md` | fonts and icons, HUD layout, sound table and mixer, CD audio, intro video |
| `docs/re/script_api.md` | mission script loading, trigger types, script API |
| `docs/re/missions_00.md` … `missions_21.md` | the six level scripts, one note per level |
| `docs/re/open_questions.md` | every open or INFERRED item across the notes, with its status |

## Status (2026-10-08, `ffefe7e`)

The reimplementation is playable end to end. All six story levels are ported
from the 68k mission code and an automated bot (`--autoplay`) completes each
of them; replays are deterministic (`tools/replay_check.py --suite` reports
MATCH; it records and plays back with the same build, so it checks the port's
own determinism, not agreement with the original, whose state can't be
observed this way). `progress.html` (local, git-ignored) has the per-area estimates.

Replays stay deterministic only if the simulation (the fixed-tick schedules)
never depends on frame timing. The rules, after a hunt for frame-dependent
divergences:

- **Object order is the object table's.** Every `Obj` has a `slot` in
  `sim::ObjTable`, kept like the original's object array (CONFIRMED: Mac
  `NewObj` @ 100575b0 appends, `RemoveObj` @ 10057830 moves the last object
  into the freed index, `GetObject` @ 100530e0 scans from index 0). Sim loops
  whose result depends on the visiting order (random numbers per object,
  spawns and despawns, first-come picks, pairs, mission actor lookups) go
  through `sim::table_order`; ties in nearest-object picks break by slot.
  Neither the ECS query order (it follows the storage tables) nor `Entity`
  ids (render code spawns entities too) may decide anything.
- **Render code never adds or removes components on an `Obj` entity.**
  Bevy's `Aabb` and `PreviousGlobalTransform` and the object renderer's
  `OverlayChildren` / `DoorPanel` are required components of `Obj`; overlay
  parts and open doors are separate `Follow` entities, not children (a child
  would add `Children` to the object). `PAYBACK_DET_CHECK=1` logs any
  component change on an `Obj` between two ticks.
- **Sim state is written only in the fixed tick.** Map edits the simulation
  reads (phone flashing, scenery changes) run in `FixedUpdate`; no sim system
  reads `Time` or anything set in `Update`.
- **The fixed-tick schedules run single-threaded**, so systems without an
  explicit order still run in the same order every run.

Performance (`perf.rs`): `PAYBACK_PERF=1` logs every frame over 25 ms with
its fixed-tick count and main-app split (pre / fixed / update / wait for the
render thread and present), and every 10 s and at exit: frame-time
p50/p95/p99/max, the count of frames over 25 ms, ticks per frame, the tick
cadence (time between frames that ran a tick: ideal 33.3 ms; with smooth
motion off, spread there is visible judder) and the audio mixer (longest
gap between mixer renders, how far ahead it rendered, underruns, voice-lock
wait). `PAYBACK_PERF_EXIT=<s>` quits after s seconds; e.g. `--map 2 --level 0
--record x --script-input --ticks 1800 --seed 1234 --hardware`. Off, it
costs one atomic load per mixer block. `--perf-check [s]` (and
`tools/perf_check.sh [s] [game args]`, which runs it with the autoplay bot)
fails with exit code 1 when, after a 5 s warm-up, the frame p95 / max or
the main-app (CPU) p95 exceed 25 / 150 / 16 ms (`PAYBACK_PERF_MAX_*`).
Pitfalls found with it:

- `World::resource_scope` removes and re-inserts the resource (Bevy 0.19
  stores resources as entities), so the resource reads as *added* (and
  changed) afterwards. Do not scope a resource that systems watch with
  `is_added()` / `is_changed()`: traffic scoping `City` every tick made the
  night atlas rebuild in `lighting::sync_mode` (~70 ms) run every frame in
  hardware mode, and `water::sync_mode` reload the sky.
- A `&mut ResMut<T>` handed on as `&mut T` flags `T` changed even if
  nothing is written. The menu loop's `Ctx` borrows `Settings`, `Language`
  and `Profile` past change detection and flags them only on a real change
  (the phrase reload in `hud::load_global_texts` ran every frame).

Frame pacing (`frame.rs`). The originals draw one frame per game-loop
iteration after `clamp(elapsed ticks, 1, 5)` ticks (Mac `amiga_levelloop`
@ 10100f50), so at most 30 new pictures a second and no interpolation. The
port draws at the display rate and adds, all render-side:

- **Smooth motion** (Graphics options, on by default): every `Obj`'s
  transform is shown between its last two tick states (`frame::smooth_objects`,
  after the sync systems, before transform propagation) and so are the
  views' cameras (`camera::ViewEyes::prev`), one tick behind the
  simulation. Bookkeeping lives in a resource, never in components on `Obj`
  entities; the simulation never reads `Transform`, so replays are
  unaffected. Off shows the original's one picture per tick. Render code
  that draws an object from its `Obj` (the object shadows) adds
  `frame::SmoothOffsets`.
- Per-tick HUD counters (`hud::Canvas::frame`, the blinks and spins) and
  the motion blur's previous frame (`postfx::FrameParity`, swapped only when
  a tick has run) keep the original's per-frame speeds at any rate.
- Vsync and a frame limit (30/60/120/144) in the Graphics options; at most
  5 ticks per frame after a hitch (`Time<Virtual>::max_delta`); presses
  shorter than a tick are kept until a tick has seen them
  (`people::read_keyboard`, `multiplayer::read_devices`).

| Area | State | Code (`crates/payback/src/`) |
|---|---|---|
| Data formats | done; every shipped file parses in the tests | `payback-formats`: `map`, `obj`, `texture`, `palette`, `phrases`, `sound`, `motion`, `route`, `flc`, `replay`, `scores` |
| City rendering | done: cubes and slopes, face types, water, weather, lens flares, Gouraud lighting, night headlights, rotating view, motion blur, palette snap, per-view effects in split screen | `city`, `water`, `weather`, `lighting`, `camera`, `postfx`, `shaders/` |
| Objects and effects | done: scenery, pickups, animated scenery, particles, decals, parked vehicles (`.vehicles`) | `objects`, `effects`, `parked` |
| People | done: player and pedestrians, motion, collision, boarding, drowning, respawn | `sim/people`, `sim/collide`, `sim/boarding`, `sim/ai` |
| Vehicles | done: models, paint, liveries and shine, damage looks, physics, traffic, AI drivers | `vehicles`, `sim/driving`, `sim/contact`, `sim/traffic`, `sim/vehicle_ai` |
| Combat and police | done: weapons, projectiles, explosions, pickups, hit money, wanted level, route pursuit, roadblocks, scoring | `sim/combat`, `sim/police`, `sim/pursuit`, `sim/score` |
| Missions | done: trigger engine, script API, map edits, levels 00/01/10/11/20/21 | `mission/`, `mission/levels/`, `mapedit` |
| Front end and HUD | done except the startup loading bar: menus, level select, results, pause, saves, high scores, intro video | `frontend/`, `hud/`, `intro` |
| Audio | done except CD music (no tracks available; the disc images hold only data, so players rip the audio tracks into a `Music` folder, see `music.rs`): software mixer with pan and Doppler | `audio`, `music` |
| Rampage, replays, multiplayer | done: rampage with high scores, record/play replays, 2–4 player split screen | `rampage`, `replay`, `multiplayer` |
| Tooling | `--screenshot`, `--record`/`--play`, `--autoplay`, `--fast` | `main`, `replay`, `autoplay` |

Recent commits:

- `9801800` Graphics options and hardware render mode: a switchable
  hardware (Warp3D) render mode reverse-engineered from the OS4 build
  (`render_mode`, `rendering.md` §11: vertex-colour lighting, bilinear 15-bit
  textures, `.chunky2` water, effect blends; the sim is unaffected); graphics
  options for renderer, resolution and window mode in the pause menu and a
  new main-menu screen (`frontend/display`); key and gamepad redefinition
  (`frontend/bindings`); user maps in rampage and multiplayer
  (`frontend/usermaps`); staged level loading that fills the loading bar.
  Sim gap fixes: in-car arrest, script police dropping goto 0x15 for
  pursuit, 0x44 arrival triggers, multiplayer hijacking, `AddSound` z, the
  exit-side reset, roadblock squares testing the slope byte.
- `ffefe7e` RE docs: `open_questions.md` (inventory of 201 open or INFERRED
  items, 156 resolved), `formats.md` rewritten, `rendering.md` §11 (hardware
  renderer), and the resolved items applied across the notes. The findings
  that `open_questions.md` §1 held back have since been merged into
  `rendering.md`, `frontend_menus.md`, `vehicles.md`, `combat_police.md`
  and `people.md`.

## Roadmap

Done: formats; city and scenery rendering; player on foot; vehicles;
pedestrians and traffic; weapons, pickups, police and wanted level; the
mission script API and all six level scripts; front end, HUD, sound, saves,
replays; rampage and multiplayer; the rotating-view and motion-blur options;
the hardware render mode, graphics options, key redefinition and user maps; script `AddSound` z
(`script_api.md` §5).

Remaining, by priority:

1. Framebuffer blends (effects, alpha tops, additive faces) are done in linear
   light and snapped to the palette at the end; mode 0x14 (`CD.obj`, 50 %) is
   drawn opaque.
2. Front end: the startup loading bar (`startup.size`, 68k `FUN_000cc0c2`);
   the repeat-last-message action; the `.vehicles` city flags for traffic on
   user maps.
3. Close the remaining questions in `docs/re/open_questions.md`.

Not planned: CD music (no audio tracks are available); the shipped version-3
replays, which the original also rejects.

## Open questions

The full list, with status and the evidence still needed, is in
`docs/re/open_questions.md`. The questions this page used to list are resolved:

| Question | Answer |
|---|---|
| Sloped cubes: which edge `height1` belongs to | `rendering.md` §1 (CONFIRMED) |
| Texture orientation and the side-face flip bit | `rendering.md` §2 (CONFIRMED) |
| Square flag bits; header `variant` | `formats.md` maps (CONFIRMED except bit `0x80`); `variant` = weather |
| `.route`, `.mini`, `.vehicles`, `.textures`, `Human.motion` formats | `formats.md` companion files (CONFIRMED) |
| Object face `mode`/`param`; Obj header `flag` | `formats.md` objects, `rendering.md` §10 (CONFIRMED) |
| Sound sample rates, water reflection, night remapping | `frontend_audio.md` §4, `rendering.md` §3–4 |
| Mission script API and entry | `script_api.md` §1–5 (CONFIRMED) |
