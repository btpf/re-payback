# Port engineering: determinism, performance, frame pacing

Notes on how the Bevy port is built to behave like the original. They are about
our code, not the original's; cite them from code comments as
`docs/port_engineering.md` "<section>".

## Determinism

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

The checks: `python tools/checks/replay_check.py --suite` (story, story2,
rampage, multiplayer, cheats) or `cargo test -p payback --test replay_determinism -- --ignored`
record a run and play it back with the same build; every checkpoint hash must
MATCH. They compare the port with itself, never with the original.

## Performance

`perf.rs`: `PAYBACK_PERF=1` logs every frame over 25 ms with
its fixed-tick count and main-app split (pre / fixed / update / wait for the
render thread and present), and every 10 s and at exit: frame-time
p50/p95/p99/max, the count of frames over 25 ms, ticks per frame, the tick
cadence (time between frames that ran a tick: ideal 33.3 ms; with smooth
motion off, spread there is visible judder) and the audio mixer (longest
gap between mixer renders, how far ahead it rendered, underruns, voice-lock
wait). `PAYBACK_PERF_EXIT=<s>` quits after s seconds; e.g. `--map 2 --level 0
--record x --script-input --ticks 1800 --seed 1234 --hardware`. Off, it
costs one atomic load per mixer block. `--perf-check [s]` (and
`tools/checks/perf_check.sh [s] [game args]`, which runs it with the autoplay bot)
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

## Frame pacing

`frame.rs`. The originals draw one frame per game-loop
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
