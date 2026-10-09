# Proposal: organising the RE and reimplementation work

Status: proposal, nothing moved yet. Surveyed 2026-10-09 at `a0b77ef`.
Sizes are `du -sh` on the working copy.

Short answer to "is `analysis/` unstructured?": **yes.** It holds five different
kinds of thing with different lifetimes: regenerable decompiler output, copies of the
original binaries, hand-written research notes that the committed code and docs cite,
about 330 one-off scripts, and 700 MB of throwaway agent scratch. The committed half
of the repo (`crates/`, `docs/`, `tools/`) is in good shape. The problem is that
`analysis/` is git-ignored as "generated output (reproducible from tools/)", and that
label is no longer true.

---

## 1. Inventory

### 1.1 Committed (165 files, `.git` = 15 MB)

| Path | What | Notes |
|---|---|---|
| `crates/payback-formats/` (16 files) | engine-agnostic parsers | tests read `extracted/` when it exists |
| `crates/payback/` (99 files) | the Bevy game | doc comments cite decompiled functions |
| `crates/payback-mpeg/` (11 files) | MPEG-1 intro playback, vendored `pl_mpeg` | MIT vendor code, fine to commit |
| `docs/formats.md`, `docs/research.md`, `docs/web.md` | format reference, status/roadmap, web build | `research.md` mixes sources, binaries, decompile how-to, status, roadmap |
| `docs/re/*.md` (16 files, ~12,000 lines, ~770 KB) | RE notes by subsystem and per mission | no index; CONFIRMED/INFERRED used throughout (open_questions.md alone: 189/73) |
| `tools/` (13 files) | extraction, hunk flattener, 3 Ghidra scripts, replay/perf checks, coverage report, web build | no README; no single "regenerate decompiles" command |
| `web/index.html`, `Dockerfile`, `README.md`, `CLAUDE.md` | | README predates the Mac release, the web build and `docs/re/` |

No original game data is committed. The only binaries are `pl_mpeg` sources. Several
committed docs quote short decompiled expressions inline (for example
`iVar41 = param_3 + 0x5dc`). There are no function bodies. That is fine, but the rule
is not written down anywhere.

### 1.2 Git-ignored, top level

| Path | Size | Kind | Contains game data / decompiled code? |
|---|---|---|---|
| `Payback Game + Updates/` | 198 MB | user's original media (`Payback.iso`, `PaybackMac.toast`, `Updates/`) | **yes, original** |
| `[A] PAYBACK 2025.7z` | 4.3 GB | ignore entirely | yes |
| `extracted/` (`game`, `iso`, `mac`, `os4`, `update7`, `warp3d`) | 244 MB | regenerable: `tools/extract.py`, `tools/extract_mac.py` | **yes, original** |
| `analysis/` | 1.0 GB | mixed (see 1.3) | **yes** |
| `target/` | **553 GB** | Cargo output: `debug`, `release`, `lead`, `check`, `fx`, `tmp`, and **~60 `agent_*` dirs of 10-20 GB each** | no |
| `web/dist/` | 171 MB | regenerable: `tools/build_web.sh`; `data/` = 125 MB packed game data | **yes (`data/`)** |
| `Saves/` | 6 KB | the game's local saves | no |
| `progress.html` | 27 KB | **hand-edited** `DATA` object | no |
| `remaining_gaps.html` | 85 KB | **hand-edited** `DATA` object (a copy of its script is in `analysis/scratch/gaps/check.js`) | no |
| `coverage.html` | 964 KB | generated: `tools/coverage.py` | no (names/addresses only) |
| `mac_differences.html` | 9.2 MB | generated: `analysis/mac/build_report.py` (**generator is git-ignored**) | embeds PNG renders of game art, so yes |

### 1.3 Inside `analysis/` (1,008 MB)

| Group | Paths | Size | Kind | Game data / decompiled code? |
|---|---|---|---|---|
| Ghidra exports (current) | `ghidra_68k`, `ghidra_os4`, `ghidra_macx`, `ghidra_mac9`, `ghidra_mapedx`, `ghidra_script00..21` (6) | ~30 MB, 11 dirs | regenerable (Docker + `tools/ghidra/*.java`), but the commands are spread over `docs/research.md`, `docs/re/mac_version.md` and `analysis/run_scripts_ghidra.sh` | **yes**: `decompiled.c`, `strings.tsv` |
| Ghidra exports (superseded) | `ghidra_macx_v1`, `ghidra_mac9_v1` | 9 MB | regenerable; same function lists as the current exports, older bodies (before the jump-table pass) | yes |
| Ghidra logs | `ghidra_*.log` (13) | <200 KB | regenerable | no |
| Ghidra inputs | `flat/*.bin` + `.map` (Payback68k + 6 scripts), `PaybackOS4` | 6 MB | regenerable (`tools/hunk.py --flat`; `PaybackOS4` is a **byte-identical copy** of `extracted/os4/.../PaybackOS4`) | **yes, original code** |
| Stray Ghidra script | `ghidra_scripts_vehicles/DecompOne.java`, output `vehicles_os4/main.c` | 0.3 MB | hand-made; not in `tools/ghidra/` | `main.c` yes |
| Mac research reviews | `mac/*.md` (10 reports, ~370 KB), `mac/symbols.tsv` (2,560 rows, Mac↔OS4↔68k), `mac/build_report.py` | 0.8 MB | **hand-made knowledge**; cited by 14 Rust doc comments and by `docs/re/*`; read by `tools/coverage.py` | `.md`: no; `symbols.tsv`: names and addresses only |
| Mac review scratch | `mac/scratch_{assets,audio,followup,frontend,gameplay,render,symbols}`, `mac/img` | 73 MB | mixed: 10 more `.md` reports (two are cited from code), the symbol-matching pipeline (`scratch_symbols/match.py`, `final_match.py`, `build_outputs.py`...), 217 `.c` excerpts, 61 MB of pickles/npy | `.c`, `.bin`, `.pkl`: yes |
| Per-topic RE work dirs | `combat/` (4 notes + `route_check.py`), `people/`, `people_ai/`, `m01/`, `m11/`, `missions_10/`, `missions21/`, `tools_mission/`, `smoke/`, `icon/` | ~3 MB | hand-made notes and helpers + decompile excerpts; cited by `docs/re/combat_police.md`, `missions_*.md`, `people.md` | `.c`/`.txt` excerpts: yes |
| Loose top-level scripts | ~40 files: `patch_*.py` (10), `fontmap*.py`, `veh_*.py`, `objfaces.py`, `intro_video.py`, `render_icons.py`, `mac_datadiff.py`, `os4mem.py`, `fn.sh`, `where.sh`, `run_scripts_ghidra.sh`, `*_staged.rs`, `main_*.rs`, `veh_special_*.c` | <1 MB | mix of useful tools (`intro_video.py`, `mac_datadiff.py`, `fn.sh`) and one-shot edit scripts (`patch_*.py`, `*_staged.rs`) | `.c`: yes |
| Images | `img/` (1,054 files: fonts, HUD, icons, 450 intro frames, `intro_audio.wav`), `shots/` (108 port screenshots) | 66 + 111 MB | regenerable / throwaway | **yes**: renders of original art and audio |
| Binary caches | `mem.pkl`, `palette.bin`, `palette_os4.bin`, `__pycache__` | 4 MB | regenerable | palette/mem: yes |
| Agent scratch | `scratch/` (149 entries, 11,417 files) | **712 MB** | throwaway, see below | yes (excerpts, screenshots) |

What is in `analysis/scratch/` (by content):

| Content | Files | Size |
|---|---|---|
| screenshots / audio | 350 | 294 MB |
| a headless Chrome profile (`web/profile/`, includes browser extensions) | 1,475 | 219 MB |
| a built game exe (`usermaps/payback_um.exe`) | 1 | 118 MB |
| replay recordings and per-tick hash dumps (`replay/` alone: 7,300 files) | 7,867 | 51 MB |
| logs and text dumps | 552 | 20 MB |
| copies of the crate source tree (`ingame/tree`, `usermaps/tree`, `perf_src`) | 324 | 6 MB |
| decompile excerpts (`fe_*.c`, `fx_*.c`, `hw/*.c`) | 116 | 4 MB |
| scripts: ~210, mostly one-shot source editors (`render_edit1..6.py`, `fe_patch_*.py`, `rotmap_patch_*.py`, `testsre/patch_docs*.py`) and a few real tools | 211 | <1 MB |
| notes (`doc_objects.md`, `doc_weather.md`, `hw/doc_section.md`) | 3 | <0.1 MB, all three already merged into `docs/re/rendering.md` §10-11 |

Helper for these counts: `analysis/scratch/organize/categorise.py`.

### 1.4 What the scratch scripts actually are

There are about 326 scripts under `analysis/`, and 122 of them hard-code
`D:/Payback Reverse Engineered`. They fall into four groups:

- **Real tools worth keeping** (≈20). `scratch/replay_decode.py` (replay format
  decoder, cited by two docs), `intro_video.py` (FLC + PCM decoder, cited),
  `render_icons.py`, `mac_datadiff.py` (cited), `mac/build_report.py` (cited), the
  `mac/scratch_symbols/` matcher (it produced `symbols.tsv`), `tools_mission/calls68k.py`,
  `missions21/trace68k.py`, `tools_mission/dis68k.py`, `scratch/ppcdis.py`,
  `combat/route_check.py` (cited), `scratch/callgraph.py`, `scratch/xref.py` +
  `os4mem.py`, `scratch/usermaps/run_all.sh` (user-map smoke test, cited),
  `scratch/web/cdp.mjs` (headless Chrome driver for the web build), `scratch/hud_mock.py`
  (cited), `scratch/buildgate.sh` (limits parallel cargo builds; agents use it).
- **Duplicates.** Five "print a decompiled function" helpers (`fn.sh`, `scratch/fn.sh`,
  `people/fn.py`, `tools_mission/fn.py`, `mac/scratch_render/fn.py`). Two copies of
  `os4mem.py`. `scratch/web/check.sh` and `scratch/agent_gaps/check.sh` are earlier
  copies of `tools/replay_check.sh`. Also `fontmap.py`/`fontmap2.py` and
  `aud_args.py`/`aud_args2.py`.
- **One-shot source/doc editors** (≈60): `patch_*.py`, `render_edit*.py`,
  `fe_patch_*.py`, `rotmap_patch_*.py`, `disp_edit_progress.py`, `testsre/patch_docs*.py`,
  `hudplat_lib.py`. Their effect is already in git history, so they have no value now.
- **Single-question probes** (the rest): `aud_dis.py`, `scen_count.py`, `night_check.py`,
  and similar.

---

## 2. Problems

1. **The `.gitignore` comment is wrong.** It calls `analysis/` "Generated RE output
   (reproducible from tools/)". Most of the folder is not reproducible: the
   `analysis/mac/*.md` reviews, the `combat/` notes, `symbols.tsv`, and about 20 useful
   scripts. Deleting `analysis/` today would lose research.
2. **Committed files point at ignored files.** 14 Rust doc comments cite
   `analysis/mac/{audio_platform,assets,frontend,rendering,driving_review}.md`,
   `analysis/mac/scratch_gameplay/people.md`, `analysis/mac/scratch_followup/input_movie.md`
   and `analysis/vehicles_os4/main.c`. About 76 references in `docs/` point into
   `analysis/` (scripts, notes, images). On a fresh clone all of these are dead links.
3. **Knowledge stuck in ignored notes.** `analysis/mac/*.md` (10 reports plus 10 more
   in `scratch_*`) is the whole Mac-vs-Amiga review. `docs/re/mac_version.md` only
   summarises its decisions. `tools/coverage.py` already counts these notes as
   documentation. `analysis/combat/notes_*.md` (≈1,000 lines) and
   `tools_mission/progression_findings.txt` are the same case.
4. **The Mac↔OS4↔68k symbol table is not committed.** `analysis/mac/symbols.tsv` is
   the Rosetta stone: it lets you translate any `FUN_01xxxxxx` into a readable Mac
   C++ name. It is hand-built, it lives in an ignored folder, and its generator is in
   `mac/scratch_symbols/`. A newcomer cannot get it.
5. **No single command regenerates the decompiles.** The recipes are spread over
   `docs/research.md` (68k), `docs/re/mac_version.md` (Mac OS X, OS 9, map editor;
   `docker run -d` with a Windows path), `analysis/run_scripts_ghidra.sh` (mission
   scripts, ignored) and nowhere at all (OS4, which reads the copy at
   `analysis/PaybackOS4`). `DecompOne.java` is not in `tools/ghidra/`.
6. **Report pages have no committed generator or data.** `progress.html` and
   `remaining_gaps.html` are hand-edited HTML with an embedded `DATA` object.
   `mac_differences.html`'s generator is ignored. `coverage.py` reads
   `remaining_gaps.html` as an input, so a hand-edited ignored page feeds a committed tool.
7. **Superseded and duplicate outputs.** `ghidra_*_v1` (9 MB) is superseded. There are
   five function-lookup helpers and three copies of the replay check script.
8. **Naming is inconsistent.** `missions21/` vs `missions_10/` vs `m01/` vs `m11/`;
   `people/` vs `people_ai/`; `scratch/` inside `analysis/` and again inside `analysis/mac/`.
9. **There is no entry point for a newcomer.** `README.md` does not mention the Mac
   release, `docs/re/`, the replay suite, the web build or the Docker RE workflow.
   `docs/` has no index, and nothing explains how a finding is written or cited.
10. **Disk use.** `target/` is 553 GB, almost all of it per-agent `CARGO_TARGET_DIR`s
    (`target/agent_*`) that were never cleaned up. Inside `analysis/scratch/` there is
    a 219 MB Chrome profile (which can hold browser state), a 118 MB exe and 7,300
    replay hash dumps.
11. **Legal exposure is spread out.** Original binaries (`analysis/PaybackOS4`,
    `analysis/flat/*.bin`), game art (`analysis/img`, `mac/img`, screenshots) and
    decompiled code (`*.c` excerpts in a dozen folders) sit next to notes that *should*
    be committed. A future "promote `analysis/` notes" commit could easily pick some of
    them up by accident.

---

## 3. Target layout

Design rule: **every ignored directory has exactly one lifetime.** Promote anything
hand-made and worth keeping into the committed tree. Anything regenerable goes under
one ignored root that a single command can rebuild. Anything else is scratch and can
be deleted at any time.

To keep churn low, the proposal **keeps `analysis/ghidra_*` and `extracted/` at their
current paths**. They are hard-coded in the game, the tests, `tools/coverage.py`,
`CLAUDE.md` and about 90 doc references. `analysis/` is narrowed to "decompiler output
only".

```
payback/
├── README.md              start here (rewritten, outline in 3.2)
├── CONTRIBUTING.md        how to add code or a finding; legal rules
├── CLAUDE.md
├── Cargo.toml  Cargo.lock  Dockerfile  .gitignore  .gitattributes
│
├── crates/                (a) COMMITTED source, unchanged
│   ├── payback-formats/
│   ├── payback-mpeg/
│   └── payback/
│
├── docs/                  (b) COMMITTED RE knowledge
│   ├── README.md          index: every doc, its subsystem, its primary binaries, status
│   ├── conventions.md     CONFIRMED/INFERRED, how to cite (see 3.3), quoting limits
│   ├── sources.md         the media, versions, binaries (from research.md "Source material", "Binaries")
│   ├── decompiling.md     how analysis/ is produced (merges the recipes from research.md + mac_version.md)
│   ├── status.md          status + roadmap (from research.md; or keep research.md for these)
│   ├── formats.md
│   ├── web.md
│   ├── xref/
│   │   ├── symbols.tsv    Mac name ↔ Mac addr ↔ OS4 addr ↔ 68k addr (from analysis/mac/symbols.tsv)
│   │   └── README.md      columns, confidence values, how it was matched
│   └── re/
│       ├── engine/        rendering, people, vehicles, combat_police, script_api,
│       │                  frontend_menus, frontend_audio  (moved as-is)
│       ├── missions/      missions_00..21, missions_random  (moved as-is)
│       ├── mac/           mac_version.md + promoted analysis/mac/*.md (see 4.2)
│       ├── reviews/       dated port-vs-original reviews (driving_review, ai_driving_review)
│       └── open_questions.md
│
├── tools/                 (c) COMMITTED tooling, with tools/README.md listing every entry
│   ├── extract/           extract.py, extract_mac.py, hunk.py
│   ├── ghidra/            ExportAll.java, MacJumpTables.java, SeedFlat68k.java, DecompOne.java,
│   │                      decompile_all.sh   ← ONE command: every analysis/ghidra_* export
│   ├── re/                fn.py (single function lookup, all builds), callgraph.py, xref.py,
│   │                      dis68k.py, calls68k.py, trace68k.py, ppcdis.py, os4mem.py,
│   │                      replay_decode.py, intro_video.py, render_icons.py, route_check.py,
│   │                      mac_datadiff.py, symbols/ (the Mac↔Amiga matcher)
│   ├── reports/           coverage.py, mac_differences.py, progress.py, gaps.py
│   │   └── data/          progress.json, gaps.json (committed DATA, no game content)
│   ├── check/             replay_check.py/.sh, perf_check.sh, usermaps_smoke.sh, web_smoke.mjs, buildgate.sh
│   └── web/               build_web.sh, pack_web_data.py
├── web/index.html
│
│   ── git-ignored below ──
├── Payback Game + Updates/   your original media (input; never touched by tools)
├── extracted/             (d) REGENERABLE: tools/extract/*.py           [game data]
├── analysis/              (d) REGENERABLE: tools/ghidra/decompile_all.sh  [decompiled code]
│   ├── flat/  PaybackOS4      inputs (or read PaybackOS4 straight from extracted/)
│   └── ghidra_{68k,os4,macx,mac9,mapedx,script00..21}/   + *.log
├── out/                   (d) REGENERABLE: reports, renders, web/dist-like outputs
│   ├── reports/           progress.html, remaining_gaps.html, coverage.html, mac_differences.html
│   └── img/               icon/font/intro renders (tools/re/render_icons.py, intro_video.py)
├── target/                Cargo; one shared dir (+ at most a few named ones)
├── web/dist/
├── Saves/
└── scratch/               (e) PERSONAL / AGENT SCRATCH: delete any time
    └── <YYYY-MM-DD>-<topic>/
```

### 3.1 One rule per area

| Area | Committed? | Rule |
|---|---|---|
| `crates/` | yes | Source. Cite the original in doc comments (3.3). Never point at an ignored path, except `extracted/` as the default data location. |
| `docs/` | yes | Knowledge. Every statement is CONFIRMED (with an address) or INFERRED. Quote at most single expressions from decompiles, never whole functions. No game images. |
| `tools/` | yes | Anything run twice. Relative paths from the repo root (no `D:/...`), `--help`, an entry in `tools/README.md`. |
| `extracted/`, `analysis/`, `out/`, `web/dist/` | no | Regenerable only. Hand-made files are not allowed here. If it cannot be rebuilt by a `tools/` command, it does not belong here. |
| `scratch/` | no | Throwaway. Safe to `rm -rf`. If a doc or a code comment would need to cite it, promote it first. |
| `Payback Game + Updates/`, `Saves/`, `[A] PAYBACK 2025.7z` | no | User-owned inputs and state. Tools only read them. |

### 3.2 README / CONTRIBUTING outline (newcomer "start here")

**README.md**
1. What this is (behavioural port, not a matching decompilation; Mac release is the
   reference, Amiga Update 7 is the alternative via a setting).
2. Legal: no game data, no decompiled code in git. You need your own copy.
3. Prerequisites: Rust (edition 2024), Python 3.11 + `capstone`, `Pillow`, `numpy`,
   `markdown`; Docker for the RE workbench (Ghidra 12.1.4); optional Node for the web smoke test.
4. Get the data: put `Payback.iso`, `PaybackUpdate7.lha`, the OS4 port and/or
   `PaybackMac.toast` in `Payback Game + Updates/`, then run `python tools/extract/extract.py`
   and `python tools/extract/extract_mac.py`.
5. Build and run: `cargo run --release -p payback -- --map 0`; `--screenshot`; data flags.
6. Verify: `cargo test -p payback -p payback-formats`; `python tools/check/replay_check.py --suite`.
7. Reverse engineering: `docker build -t payback-re .`, then
   `tools/ghidra/decompile_all.sh` fills `analysis/`. Read `docs/README.md` next.
8. Layout table (section 3) and links to `docs/README.md`, `tools/README.md`, `CONTRIBUTING.md`.

**CONTRIBUTING.md**
1. Workflow: linear history on `main`; what a commit should contain.
2. Adding a finding: pick the subsystem doc via `docs/README.md`; mark it CONFIRMED with
   the function (3.3) or INFERRED with the reasoning; add a row to `docs/xref/symbols.tsv`
   when you name a function; add an entry to `docs/re/open_questions.md` when you leave
   something open.
3. Adding code: cite the original; follow the determinism rules (from `docs/research.md`);
   keep the replay suite at MATCH; check visuals with `--screenshot`.
4. Adding a tool: under `tools/<area>/`, with repo-relative paths, documented in `tools/README.md`.
5. Legal checklist before committing: no files from `extracted/`, `analysis/`, `out/`,
   `scratch/`; no PNGs of game art; no decompiled function bodies.

### 3.3 Citation convention (to put in `docs/conventions.md`)

The current docs use many forms (`FUN_01079300`, `0xb506e`, `68k 0xa4a68`,
`DrawMap @ 1013fbd0`, bare `01xxxxxx`). `tools/coverage.py` already parses all of them.
Proposed canonical form, with the most readable build first:

`Mac DrawMap @1013fbd0 / OS4 FUN_01079300 / 68k FUN_000a4a68` (give whichever builds
you checked). `docs/xref/symbols.tsv` is the lookup table. Existing citations stay
valid; there is no need to rewrite them.

---

## 4. Migration plan

Every step is a separate commit or local move and can be undone. Steps 1-6 are
committed changes. Steps 7-10 are local, ignored-tree changes. **Back up `analysis/`
first** (it is 1 GB, mostly scratch; a zip of `analysis/` minus `scratch/` is about 300 MB).

### 4.1 Ordered steps

| # | Step | Reversible by | Risk |
|---|---|---|---|
| 0 | Back up: `analysis/` → archive outside the repo. | restore | none |
| 1 | **Docs index and conventions.** Add `docs/README.md` (one row per doc: subsystem, builds, status), `docs/conventions.md`. Move nothing yet. | `git revert` | none |
| 2 | **Promote the Mac reviews.** Copy the files in 4.2 into `docs/re/mac/` and `docs/re/reviews/`. Strip decompile excerpts and embedded images while doing it. Update the 14 Rust doc comments and the `docs/` references to the new paths. Leave the originals in place until step 9. | `git revert` | Low. Strip any pasted function bodies. `coverage.py` must read the new location (step 5). |
| 3 | **Commit the symbol table.** `analysis/mac/symbols.tsv` → `docs/xref/symbols.tsv` (names + addresses only, 248 KB), plus a README for its columns. | `git revert` | Legal judgement: original symbol names. Decompilation projects normally publish these. See question Q2. |
| 4 | **Promote tools** (4.3) into `tools/{re,ghidra,check,reports}/`. Make paths repo-relative, merge the five `fn` helpers into one `tools/re/fn.py`, and add `tools/ghidra/decompile_all.sh` (one Docker call per binary, same flags as today, plus `DecompOne.java`). Add `tools/README.md`. | `git revert` | Medium: re-running Ghidra must give the same function names. Run it into a temp dir and diff `functions.tsv` against the current exports before switching. |
| 5 | **Reports.** Move `analysis/mac/build_report.py` → `tools/reports/mac_differences.py`. Extract the `DATA` objects of `progress.html` and `remaining_gaps.html` into `tools/reports/data/*.json`, each with a small generator. Make `coverage.py` read `gaps.json` instead of the HTML. Optionally write all four pages to `out/reports/`. | `git revert` | Medium. `CLAUDE.md` says "update `progress.html`'s DATA"; the instruction becomes "edit `progress.json`, run `tools/reports/progress.py`". Decide about committing the JSON (Q3). |
| 6 | **Restructure `docs/re/`** into `engine/`, `missions/`, `mac/`, `reviews/`. Split `docs/research.md` into `sources.md`, `decompiling.md` and `status.md`, or keep it as the status page. Rewrite `README.md` and add `CONTRIBUTING.md`. Fix every link (`grep -rn "docs/re/"`). | `git revert` | Low. `tools/coverage.py` globs `docs/re/*.md`, so change it to `**/*.md`. |
| 7 | **Retire hard-coded `tools/` paths**: `tools/extract.py` → `tools/extract/extract.py` and so on. Update `CLAUDE.md`, `README.md`, `docs/web.md`, and the CI/agent prompts that call `tools/replay_check.py`. Optional: leave one-line forwarding stubs at the old paths for one release. | `git revert` | Medium: muscle memory and agent prompts. This step could be skipped and `tools/` kept flat. |
| 8 | **Make `analysis/` regenerable-only (local).** Delete `ghidra_*_v1` (check that `docs/re/mac_version.md` no longer needs them; say so in the doc). Point the OS4 import at `extracted/os4/.../PaybackOS4` and drop the copy. Move `img/` to `out/img/`. | archive from step 0 | Low |
| 9 | **Create top-level `scratch/` (ignored) and empty `analysis/` of everything that is not an export.** Move `analysis/scratch/` → `scratch/legacy-analysis-scratch/`. Move the per-topic dirs (`combat/`, `people*/`, `m01/`, `m11/`, `missions*/`, `tools_mission/`, `smoke/`, `icon/`, `shots/`), the loose scripts and `mac/` there too, once their content is promoted. | move back | Low. Before deleting anything, grep committed files for `analysis/` again: only `analysis/ghidra_*` (and `analysis/flat`) may remain. |
| 10 | **Clean up big local items** (user decision, Q5): the `target/agent_*` dirs (about 540 GB), `scratch/.../web/profile` (Chrome profile), `usermaps/payback_um.exe`, the replay hash dumps. | rebuild | none for correctness. Deleting `target/agent_*` costs rebuild time. |

### 4.2 Notes to promote

| From (ignored) | To (committed) | Why / who cites it |
|---|---|---|
| `analysis/mac/audio_platform.md` | `docs/re/mac/audio_platform.md` | cited by `audio.rs`, `music.rs`, `intro.rs`, `frame.rs`, `docs/re/frontend_audio.md` |
| `analysis/mac/assets.md` | `docs/re/mac/assets.md` | `data.rs`, `frontend/assets.rs` |
| `analysis/mac/frontend.md` | `docs/re/mac/frontend.md` | `hud/mod.rs`, `hud/icons.rs`, `docs/re/frontend_menus.md` |
| `analysis/mac/rendering.md` | `docs/re/mac/rendering.md` | `postfx.rs`, `shadows.rs`, `effects.wgsl` |
| `analysis/mac/gameplay.md`, `gameplay_part2.md` | `docs/re/mac/gameplay.md` (merged) | `docs/re/people.md` |
| `analysis/mac/followup.md` | `docs/re/mac/followup_jumptables.md` | Decision 13 results |
| `analysis/mac/symbols.md` | `docs/xref/README.md` (the method part) | goes with `symbols.tsv` |
| `analysis/mac/scratch_followup/input_movie.md` | `docs/re/mac/input_replay.md` | `formats/replay.rs`, `docs/re/frontend_menus.md` |
| `analysis/mac/scratch_gameplay/people.md` | merge into `docs/re/engine/people.md` (or `docs/re/mac/people.md`) | `objects.rs`, `sim/people.rs` |
| `analysis/mac/scratch_followup/{level00,level01_10,level11_20_21,triggers}.md`, `scratch_gameplay/{combat,missions,vehicles}.md`, `scratch_symbols/part2.md` | review; merge the Mac-specific facts into the matching `docs/re/*` file | not cited, but findings |
| `analysis/mac/driving_review.md`, `ai_driving_review.md` | `docs/re/reviews/2026-10-08-driving.md`, `…-ai-driving.md` | `sim/driving.rs` |
| `analysis/combat/notes_{weapons,police,effects}.md`, `draft_route_score.md` | check against `docs/re/combat_police.md` (which cites them); promote what is missing as an appendix | cited |
| `analysis/tools_mission/progression_findings.txt` | check against `missions_*.md` and `script_api.md` | has CONFIRMED tags |
| `analysis/scratch/doc_objects.md`, `doc_weather.md`, `hw/doc_section.md` | **drop**: already `docs/re/rendering.md` §10, §4 (weather), §11 | |
| `analysis/vehicles_os4/main.c` (cited by `sim/driving.rs`) | **do not promote** (decompiled code); change the comment to cite the function address | |

### 4.3 Scripts: promote / drop

| Promote to | Source |
|---|---|
| `tools/ghidra/` | `analysis/ghidra_scripts_vehicles/DecompOne.java`, `analysis/run_scripts_ghidra.sh` (folded into `decompile_all.sh`) |
| `tools/re/fn.py` | merge `analysis/fn.sh`, `where.sh`, `people/fn.py`, `tools_mission/fn.py`, `mac/scratch_render/fn.py`, `scratch/phr_by_fn.py` |
| `tools/re/` | `scratch/replay_decode.py`, `intro_video.py`, `render_icons.py`, `mac_datadiff.py`, `os4mem.py`, `scratch/xref.py`, `scratch/callgraph.py`, `scratch/ppcdis.py`, `tools_mission/dis68k.py`, `tools_mission/calls68k.py` + `missions21/trace68k.py` (merge, same `NAMES` table), `combat/route_check.py`, `scratch/mapparse.py`, `scratch/hud_mock.py` |
| `tools/re/symbols/` | `mac/scratch_symbols/{load,demangle,pef,match,final_match,build_outputs,write_md}.py` (without the `.pkl` caches) |
| `tools/reports/` | `mac/build_report.py`; generators for progress and gaps |
| `tools/check/` | `scratch/usermaps/run_all.sh` → `usermaps_smoke.sh`; `scratch/web/cdp.mjs` → `web_smoke.mjs`; `scratch/buildgate.sh` (make the slot dir configurable, default `target/buildgate`) |
| **drop** | `patch_*.py` (10), `scratch/render_edit*.py`, `fe_patch_*.py`, `rotmap_patch_*.py`, `disp_edit_progress.py`, `testsre/patch_docs*.py`, `hudplat_lib.py`, `*_staged.rs`, `main_*.rs`, `fe_*_new.rs`, `scratch/{web,agent_gaps}/check.sh`, `fontmap.py` (keep `fontmap2.py` if needed), `aud_args.py`, all `__pycache__`, source-tree copies (`*/tree`, `perf_src`) |

### 4.4 What must stay where it is (or move with care)

| Path | Hard-coded in |
|---|---|
| `extracted/game`, `extracted/mac/Payback` | `crates/payback/src/data.rs`, `crates/payback/build.rs`, `payback-formats/tests/real_data.rs`, `payback/tests/replay_determinism.rs`, `payback-mpeg/tests/film.rs`, `tools/pack_web_data.py`, `tools/build_web.sh` |
| `analysis/ghidra_{macx,os4,68k}/functions.tsv` | `tools/coverage.py` |
| `analysis/mac/symbols.tsv`, `analysis/mac/*.md`, `remaining_gaps.html` (repo root) | `tools/coverage.py` (change in the same commit as steps 2, 3 and 5) |
| `analysis/ghidra_*` | `CLAUDE.md`, the Docker recipes in `docs/research.md` / `docs/re/mac_version.md` (`/work/impl/analysis/...`), about 90 doc references |
| `target/debug/payback(.exe)`, `target/replay_check` | `tools/replay_check.*`, `tools/perf_check.sh`, `CLAUDE.md` |
| `analysis/scratch/buildgate/` | `scratch/buildgate.sh` and probably agent prompts (**a `slot0` is present now, so a build may be holding it**) |
| `web/dist/` | `tools/build_web.sh`, `docs/web.md` |
| `Saves/` | the game (an old `./Saves` is copied to `%APPDATA%\Payback`) |

### 4.5 CLAUDE.md changes

- Layout: add `CONTRIBUTING.md`, `docs/README.md` (index), `docs/xref/symbols.tsv`, and the
  `tools/` subfolders. State the rule: "`analysis/` = Ghidra exports only, regenerated by
  `tools/ghidra/decompile_all.sh`; `out/` = generated reports; `scratch/` = throwaway,
  never cited".
- Add: "Promote before you cite. Committed code and docs must not reference `scratch/`
  or any ignored file except `extracted/` and `analysis/ghidra_*`."
- Add: "Scripts you will run twice go into `tools/` with repo-relative paths. One-off
  edit scripts go into `scratch/<date>-<topic>/`."
- Citing: allow Mac names ("`FUN_xxxxxxxx`, OS4 or 68k" becomes "Mac name @addr, OS4
  or 68k `FUN_`; see `docs/conventions.md`").
- Progress page: "edit `tools/reports/data/progress.json`, then run
  `python tools/reports/progress.py`" (if Q3 = yes). Same for gaps.
- Agents: use the shared `target/` (or `target/agent_<name>` only through `buildgate`),
  and delete per-agent target dirs when done.
- Ignored list: replace the list of four root HTML files with `out/`, and add `scratch/`.

---

## 5. Open questions

1. **Directory names.** Keep `analysis/` for the exports (minimal churn, recommended),
   or rename it to `out/decomp/` so that all regenerable output sits under one root?
2. **Publish `docs/xref/symbols.tsv`?** It holds function names and addresses only (no
   code). Matching-decomp projects publish symbol maps, but the Mac names are the
   original developer's identifiers. Commit it, or keep it local with a generator?
3. **Commit the report data** (`progress.json`, `gaps.json`)? They contain no game
   content. Committing them makes "what's left" visible to newcomers, which is the
   point of this exercise. Today they are local-only by choice.
4. **`mac_differences.html` embeds renders of game art.** Keep generating it locally
   only (recommended), or change it to link images from `out/img/`?
5. **Disk clean-up.** OK to delete the `target/agent_*` dirs (~540 GB), the Chrome
   profile, `payback_um.exe` and the replay dumps after the backup?
6. **Flat `tools/` or subfolders?** Subfolders scale better but change the paths
   used in `CLAUDE.md` and agent prompts (step 7). The alternative is to keep the
   current tools at their paths and put only new ones in subfolders.
7. **The `_v1` Mac exports.** `docs/re/mac_version.md` says they are "kept". Are they
   still needed to compare before and after the jump-table pass, or can
   `decompile_all.sh` drop them (rebuildable by skipping `MacJumpTables.java`)?
8. **Split `docs/research.md`?** It is the file the memory and `CLAUDE.md` point to
   first. Splitting it is cleaner, but it would then turn into a short index page.
