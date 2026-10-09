# Status and roadmap

State as of 2026-10-08 (`ffefe7e`); `git log` has everything since.

The reimplementation is playable end to end. All six story levels are ported
from the 68k mission code and an automated bot (`--autoplay`) completes each
of them; replays are deterministic (`tools/checks/replay_check.py --suite` reports
MATCH; it records and plays back with the same build, so it checks the port's
own determinism, not agreement with the original, whose state can't be
observed this way). `progress.html` (local, git-ignored) has the per-area estimates.

How the port keeps replays deterministic, and its performance and frame-pacing
notes, are in [`port_engineering.md`](port_engineering.md).

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
