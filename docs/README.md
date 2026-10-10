# Documentation index

Start with [`status.md`](status.md) (what works, what is left) and
[`CONVENTIONS.md`](CONVENTIONS.md) (how findings are tagged and cited). Builds:
**Mac** = Mac OS X release (`analysis/ghidra_macx`, original names, the reference),
**OS4** = AmigaOS 4 port (`analysis/ghidra_os4`), **68k** = Amiga Update 7
(`analysis/ghidra_68k`). [`decompiling.md`](decompiling.md) explains how those
exports are made.

## Project

| Doc | Covers |
|---|---|
| [`status.md`](status.md) | state of the port per area, roadmap, where the open questions are |
| [`sources.md`](sources.md) | the original media, versions and binaries; landmark functions |
| [`decompiling.md`](decompiling.md) | the Ghidra workflow: `tools/ghidra/decompile_all.sh`, what `analysis/` holds, lookup tools |
| [`required_files.html`](required_files.html) | which original files are needed to compile, run and decompile (open in a browser) |
| [`CONVENTIONS.md`](CONVENTIONS.md) | CONFIRMED / INFERRED, citation format, quoting limits, adding a finding |
| [`port_engineering.md`](port_engineering.md) | the port's determinism rules, performance checks, frame pacing |
| [`controls.md`](controls.md) | keyboard and gamepad bindings |
| [`web.md`](web.md) | the WebAssembly build |
| [`research.md`](research.md) | the old entry page, now a pointer to the above |

## Reverse engineering: subsystems (`re/`)

| Note | Covers | Main builds |
|---|---|---|
| [`formats.md`](formats.md) | data file formats: maps, objects, textures, palette, phrases, sounds, companion files | data, all |
| [`re/rendering.md`](re/rendering.md) | city renderer, slopes, faces, camera, water, weather, lighting, effects, hardware (Warp3D) mode | OS4, 68k, Mac |
| [`re/vehicles.md`](re/vehicles.md) | vehicle table, physics, damage, AI drivers, traffic | OS4, 68k, Mac |
| [`re/people.md`](re/people.md) | player and pedestrians on foot, `Human.motion`, collision, boarding, AI | 68k, OS4, Mac |
| [`re/combat_police.md`](re/combat_police.md) | `.route` graph, police and wanted level, weapons, projectiles, explosions, scoring | 68k, OS4 |
| [`re/frontend_menus.md`](re/frontend_menus.md) | title, menus, config, saves, replays, multiplayer, rampage, `.thumb` / `.mini` | OS4, Mac |
| [`re/frontend_audio.md`](re/frontend_audio.md) | fonts and icons, HUD layout, sound table and mixer, CD audio, intro video | OS4, 68k, Mac |
| [`re/script_api.md`](re/script_api.md) | mission script loading, trigger types, script API | 68k, OS4 |
| [`re/open_questions.md`](re/open_questions.md) | every open or INFERRED item across the notes, with its status | all |

## Reverse engineering: missions (`re/`)

| Note | Level |
|---|---|
| [`re/missions_00.md`](re/missions_00.md) | city 0, level 0 |
| [`re/missions_01.md`](re/missions_01.md) | city 0, level 1 |
| [`re/missions_10.md`](re/missions_10.md) | city 1, level 0 |
| [`re/missions_11.md`](re/missions_11.md) | city 1, level 1 |
| [`re/missions_20.md`](re/missions_20.md) | city 2, level 0 ("Escape from jail") |
| [`re/missions_21.md`](re/missions_21.md) | city 2, level 1 |
| [`re/missions_random.md`](re/missions_random.md) | the Mac-only random phone missions in Rampage |

## Mac release (`re/mac/`)

[`re/mac_version.md`](re/mac_version.md) summarises the Mac release and the decisions
taken for the port. [`re/mac/README.md`](re/mac/README.md) indexes the detailed reviews:
[rendering](re/mac/rendering.md), [gameplay](re/mac/gameplay.md) and
[part 2](re/mac/gameplay_part2.md), [vehicles](re/mac/gameplay_vehicles.md),
[people](re/mac/gameplay_people.md), [combat](re/mac/gameplay_combat.md),
[missions](re/mac/gameplay_missions.md), [follow-up](re/mac/followup.md),
[level scripts 00](re/mac/levels_00.md), [01/10](re/mac/levels_01_10.md),
[11/20/21](re/mac/levels_11_20_21.md), [triggers](re/mac/triggers.md),
[input and replays](re/mac/input_movie.md), [front end](re/mac/frontend.md),
[assets](re/mac/assets.md), [audio and platform](re/mac/audio_platform.md).

## Reviews and working notes

| Doc | Covers |
|---|---|
| [`re/reviews/2026-10-08-driving.md`](re/reviews/2026-10-08-driving.md) | vehicle physics, original vs port, line by line |
| [`re/reviews/2026-10-08-ai-driving.md`](re/reviews/2026-10-08-ai-driving.md) | AI driving (traffic, police, scripted drives), Mac vs Amiga vs port |
| [`re/notes/combat_weapons.md`](re/notes/combat_weapons.md) | weapons and pickups (behind `combat_police.md`) |
| [`re/notes/combat_police.md`](re/notes/combat_police.md) | police, wanted level, roadblocks, arrest |
| [`re/notes/combat_effects.md`](re/notes/combat_effects.md) | explosions, damage, collide objects |
| [`re/notes/route_score_draft.md`](re/notes/route_score_draft.md) | first draft of the route graph and scoring |
| [`re/notes/progression_findings.md`](re/notes/progression_findings.md) | progression, saving and script API (Update 7) |

## Cross-reference

| File | Content |
|---|---|
| [`xref/symbols.tsv`](xref/symbols.tsv) | one row per Mac function: Mac address and name, demangled name, OS4 and 68k addresses, confidence, evidence (names and addresses only) |
| [`xref/symbols.md`](xref/symbols.md) | how the table was matched, the functions the notes cite with their Mac names, corrections |

Lookup: `python tools/re/fn.py --sym FUN_01079300` (or a Mac name, or a 68k address).
Regenerate: `tools/re/symbols/` (see `symbols.md` "Files").
