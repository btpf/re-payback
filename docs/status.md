# Status and roadmap

State as of 2026-10-09 (`3382e77`); `git log` has everything since.

The reimplementation is playable end to end. All six story levels are ported
from the 68k mission code and an automated bot (`--autoplay`) completes each
of them; replays are deterministic (`tools/checks/replay_check.py --suite` reports
MATCH; it records and plays back with the same build, so it checks the port's
own determinism, not agreement with the original, whose state can't be
observed this way). The Mac release is the reference: its differences from
the Amiga version are ported, with a setting where the content differs
([`re/mac_version.md`](re/mac_version.md) "Decisions"). `docs/progress.html`
(local, git-ignored) has the per-area estimates and `docs/remaining_gaps.html`
(local) the itemised list of what still differs.

How the port keeps replays deterministic, and its performance and frame-pacing
notes, are in [`port_engineering.md`](port_engineering.md).

| Area | State | Code (`crates/payback/src/`) |
|---|---|---|
| Data formats | done; every shipped file parses in the tests; the palette is read from the Amiga `Payback.exe` or the Mac program's PEF data section | `payback-formats`: `map`, `obj`, `texture`, `palette`, `pef`, `phrases`, `sound`, `motion`, `route` (with the map editor's routing algorithm), `vehicles`, `flc`, `icon`, `replay`, `scores` |
| Data on disk | the build stages the files of `data_files.txt` next to the executable (`target/<profile>/data/amiga`, `data/mac`); search order and folder picker in `locate.rs`; either release alone runs (`--mac-only`) | `build.rs`, `locate`, `data`, `vfs` |
| City rendering | done: cubes and slopes, face types, water, weather, lens flares (occluded by the city and models), Gouraud lighting, night headlights, rotating view, motion blur, palette-space blending and snap, per-view effects in split screen; hardware (Warp3D-style) mode with 16-bit dithered output | `city`, `water`, `weather`, `lighting`, `camera`, `postfx`, `render_mode`, `shaders/` |
| Objects and effects | done: scenery (collision circles from the ground-level vertices), pickups, animated scenery, helicopter rotor and door, particles, decals, debris, floating labels, shadows, parked vehicles (`.vehicles`), direction arrows | `objects`, `objects/shadows`, `effects`, `debris`, `arrow`, `parked` |
| People | done: player and pedestrians, motion, collision, boarding, drowning, respawn, Mac pavement wander | `sim/people`, `sim/collide`, `sim/boarding`, `sim/ai` |
| Vehicles | done: models, paint, liveries and shine, damage looks, physics (driving accuracy pass against the Mac, OS4 and 68k code), barriers and cones, bomb truck RC car, traffic, AI drivers | `vehicles`, `sim/driving`, `sim/contact`, `sim/props`, `sim/rc_car`, `sim/traffic`, `sim/vehicle_ai` |
| Combat and police | done: weapons, punches, projectiles, explosions, pickups, hit money, ramming score, wanted level, route pursuit, roadblocks, scoring | `sim/combat`, `sim/police`, `sim/pursuit`, `sim/score` |
| Missions | done: trigger engine, script API, map edits, levels 00/01/10/11/20/21, the Mac's random phone missions in Rampage | `mission/`, `mission/levels/`, `mapedit` |
| Front end and HUD | done: menus, level select, results, pause, saves, high scores, loading bars, intro (Mac film or Amiga FLC), Graphics options, control setup, cheat menu (F12), "Unlock all levels" (main menu item 9), level-start zoom-in map, minimap (3D minimap option) | `frontend/`, `hud/`, `intro`, `startup`, `cheats` |
| Input | keyboard (Mac default keys) and gamepads (layout 3, rebindable, hot-plug, XInput cross-check on Windows; [`controls.md`](controls.md)) | `frontend/bindings`, `frontend/xinput` |
| Audio | done: software mixer with pan and Doppler; CD music from tracks the player rips into `Music/` next to the executable (tracks 2–16, README "Music"; the disc images hold only the data track, so none ship with the data; `tools/extract/rip_cd_tracks.py` splits them out of a BIN/CUE image of the CD) | `audio`, `music` |
| Rampage, replays, multiplayer | done: rampage with high scores and random missions, record/play replays (Amiga and Mac formats), 2–4 player split screen | `rampage`, `replay`, `multiplayer` |
| Platform | saves and settings in `Saves` next to the executable (`PAYBACK_SAVES`), smooth motion, vsync and frame limit, WebAssembly build ([`web.md`](web.md)) | `frontend/progress`, `frame`, `web`, `vfs` |
| Tooling | `--screenshot`, `--record`/`--play`, `--autoplay`, `--fast`, `--perf-check`, `--pad-sim`/`--pad-debug`; [`tools/README.md`](../tools/README.md) | `main`, `replay`, `autoplay`, `perf` |

Recent work (2026-10-08/09; one line each in `docs/progress.html`):

- Mac comparison: the Mac release extracted and decompiled, its differences
  ported (30 Hz tick, AI and pedestrians, lighting, MPEG intro, 0x3F3
  replays, random missions), Mac/Amiga variant settings; the port runs from
  the Mac release alone (palette from the PEF data section).
- Determinism (object-table order, render code never reshapes sim entities)
  and a driving-accuracy pass; most gaps of the 2026-10-09 gap report closed
  (`bbe27cb`): punches, ramming, RC car, props, palette-space blending,
  smooth motion, replay clock, zoom-in map, the `.route` algorithm.
- Gamepads (layout 3, Steam Input fixes), vehicle god mode and cheat-menu
  colours and wanted level, saves next to the executable, data staged by the
  build, defaults of free aspect and hardware rendering, 3D minimap off by
  default (forced by the rotating view), helicopter rotor and door, scenery
  collision sizes, headlight depth, web build with cache-busting.
- Repository reorganised: research in `docs/`, tools by area, one decompile
  command (`tools/ghidra/decompile_all.sh`).

## Roadmap

Remaining, by priority (details and evidence in `docs/remaining_gaps.html`):

1. Small rule fixes: the burning-car blast is centred 500 above the car
   instead of below it (`sim/contact.rs`, Mac `Explode` @ 10055570); bullet
   and pellet hits test "shooter is the player" rather than the owner side
   (`combat_police.md` §5.3).
2. Decisions: blast crimes at zero damage (`BLAST_CRIME_AT_ZERO_DAMAGE`, off);
   the Rampage "Random missions" row without the Mac phrases.
3. Verification by a person: level 11 M9 with the RC car, smooth motion on a
   high-refresh display, more gamepads, the web build in browsers with a GPU,
   screenshots of the new rendering against the original.
4. Tests: unit tests for the newer combat and vehicle rules; the bot still
   teleports on foot and in some cases.
5. Rendering leftovers: one palette snap per frame instead of per blend, HUD
   panels in linear light, effect sprites don't hide flares, painter's sort
   fixed at mesh build, affine tops in the rotating view, hardware fades,
   shadow draw order, the zoom-in map's genlocked first frame.
6. The per-object AI interleave (the port runs each phase over all objects);
   only needed for frame-exact parity with original recordings.
7. A map editor (not started).
8. Close the remaining questions in [`re/open_questions.md`](re/open_questions.md).

Not planned: the shipped version-3 replays, which the original also rejects.

## Open questions

The full list, with status and the evidence still needed, is in
`docs/re/open_questions.md`. The questions this page used to list are resolved:

| Question | Answer |
|---|---|
| Sloped cubes: which edge `height1` belongs to | `rendering.md` §1 (CONFIRMED) |
| Texture orientation and the side-face flip bit | `rendering.md` §2 (CONFIRMED) |
| Square flag bits; header `variant` | `formats.md` maps (CONFIRMED; `0x80` unused); `variant` = weather |
| `.route`, `.mini`, `.vehicles`, `.textures`, `Human.motion` formats | `formats.md` companion files (CONFIRMED) |
| Object face `mode`/`param`; Obj header `flag` | `formats.md` objects, `rendering.md` §10 (CONFIRMED) |
| Sound sample rates, water reflection, night remapping | `frontend_audio.md` §4, `rendering.md` §3–4 |
| Mission script API and entry | `script_api.md` §1–5 (CONFIRMED) |
