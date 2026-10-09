# Mac release reviews

The detailed comparison of the Mac OS 9 / OS X release with the Amiga builds
(2026-10-08). [`../mac_version.md`](../mac_version.md) is the summary: sources,
extraction, decompiling, the data differences and the decisions taken for the port.
These files hold the evidence behind it, area by area. Addresses are Mac OS X
unless marked; see [`../../CONVENTIONS.md`](../../CONVENTIONS.md).

| Note | Covers |
|---|---|
| [`rendering.md`](rendering.md) | renderer, lighting, OpenGL hardware path, effects, data that affects rendering |
| [`gameplay.md`](gameplay.md), [`gameplay_part2.md`](gameplay_part2.md) | tick rate, game loop, vehicles, people, missions engine (overview and part 2) |
| [`gameplay_vehicles.md`](gameplay_vehicles.md) | vehicle physics and vehicle AI, line by line |
| [`gameplay_people.md`](gameplay_people.md) | people on foot and collisions (the pavement wander, M1-M6) |
| [`gameplay_combat.md`](gameplay_combat.md) | combat, police, scoring, struct-offset map |
| [`gameplay_missions.md`](gameplay_missions.md) | trigger engine, script API, level scripts, random missions |
| [`followup.md`](followup.md) | the follow-up pass after the jump-table re-export |
| [`levels_00.md`](levels_00.md), [`levels_01_10.md`](levels_01_10.md), [`levels_11_20_21.md`](levels_11_20_21.md) | every recovered `ProcessTrigger_*` case per level, against `mission/levels/*.rs` |
| [`triggers.md`](triggers.md) | `SetTrigger_Long` (all 69 cases) and `DoKidnap` |
| [`input_movie.md`](input_movie.md) | weapon cycling, input and the replay ("movie") recorder |
| [`frontend.md`](frontend.md) | front end, HUD, input, files, languages and phrases |
| [`assets.md`](assets.md) | asset-by-asset data comparison (textures, models, icons, maps, phrases) |
| [`audio_platform.md`](audio_platform.md) | audio, music, the MPEG intro film, platform layer |

Some passages name the review's working files (`scratch_*/…`, `.c` extracts, renders
under `analysis/mac/`). Those were local and are not kept; see
[`../../CONVENTIONS.md`](../../CONVENTIONS.md) "Paths in the notes". The consolidated
local report `mac_differences.html` is built from these notes by
`python tools/reports/mac_differences.py`.
