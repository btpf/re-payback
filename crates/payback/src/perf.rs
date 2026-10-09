//! `PAYBACK_PERF=1`: frame-time logger for hunting stutter.
//!
//! Logs every frame longer than 25 ms with the number of fixed (30 Hz) ticks
//! it ran and where the main-app time went (pre = `First`..fixed loop,
//! fixed = the fixed loop, upd = `Update`..`Last`, wait = the rest of the
//! frame: render extraction, waiting for the render thread, present).
//! Every 10 s and at exit it prints:
//! - frame time p50/p95/p99/max and the count of frames over 25 ms;
//! - frames per tick count (0, 1, 2, 3, 4+ ticks run in the frame);
//! - tick cadence: the time between successive frames that ran a tick,
//!   i.e. how far apart new game states reach the screen (ideal 33.3 ms;
//!   the sim is not interpolated, so spread here is visible judder);
//! - audio: the longest gap between mixer renders (how late voice changes
//!   are heard; the device buffer is ~10–50 ms), how far ahead of real time
//!   the mixer has rendered at most, underruns (the rendered total fell
//!   behind real time by more than 5 ms more than before) and the longest
//!   wait for the voice lock.
//!
//! `PAYBACK_PERF_EXIT=<s>` quits after that many seconds. When
//! `PAYBACK_PERF` is unset nothing is added except one relaxed atomic load
//! per mixer block ([`audio_block`]).
//!
//! `--perf-check [s]` (default 30 s) is the automatic check
//! (`tools/checks/perf_check.sh`): the logger runs, the game quits after `s`
//! seconds and the run fails (exit code 1, "perf-check: FAIL") when, after
//! a 5 s warm-up, the frames exceed the thresholds ([`Thresholds`],
//! overridable with `PAYBACK_PERF_MAX_P95`, `PAYBACK_PERF_MAX_FRAME`,
//! `PAYBACK_PERF_MAX_MAIN` in ms): frame time p95 and max, and the
//! main-app time (the CPU work of a frame, without waiting for the GPU or
//! vsync) at p95. A per-frame rebuild like the traffic `City` re-adding
//! (`5f0c78e`) shows in the main-app time.

use bevy::app::RunFixedMainLoopSystems;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use bevy::platform::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

/// Audio thread bookkeeping (µs since the first block, frames rendered).
static AUDIO_START: OnceLock<Instant> = OnceLock::new();
static AUDIO_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_LAST_US: AtomicU64 = AtomicU64::new(0);
static AUDIO_MAX_GAP_US: AtomicU64 = AtomicU64::new(0);
static AUDIO_MAX_LEAD: AtomicU64 = AtomicU64::new(0);
static AUDIO_MAX_DEFICIT: AtomicU64 = AtomicU64::new(0);
static AUDIO_UNDERRUNS: AtomicU64 = AtomicU64::new(0);
static AUDIO_MAX_LOCK_US: AtomicU64 = AtomicU64::new(0);

/// Called by the mixer (audio thread) for every rendered block of `frames`
/// frames at `rate` Hz, with the time spent waiting for the voice lock.
pub fn audio_block(frames: usize, rate: u32, lock_wait: std::time::Duration) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let start = *AUDIO_START.get_or_init(Instant::now);
    let now = start.elapsed().as_micros() as u64;
    let last = AUDIO_LAST_US.swap(now, Ordering::Relaxed);
    AUDIO_MAX_GAP_US.fetch_max(now - last, Ordering::Relaxed);
    let produced = AUDIO_FRAMES.fetch_add(frames as u64, Ordering::Relaxed);
    let consumed = now * rate as u64 / 1_000_000;
    AUDIO_MAX_LEAD.fetch_max(produced.saturating_sub(consumed), Ordering::Relaxed);
    let deficit = consumed.saturating_sub(produced);
    let max = AUDIO_MAX_DEFICIT.fetch_max(deficit, Ordering::Relaxed);
    if deficit > max + rate as u64 / 200 && produced > 0 {
        AUDIO_UNDERRUNS.fetch_add(1, Ordering::Relaxed);
    }
    AUDIO_MAX_LOCK_US.fetch_max(lock_wait.as_micros() as u64, Ordering::Relaxed);
}

pub struct PerfPlugin;

/// `--perf-check` limits (ms).
#[derive(Clone, Copy, Debug)]
struct Thresholds {
    p95: f32,
    max_frame: f32,
    main_p95: f32,
}

impl Thresholds {
    fn from_env() -> Thresholds {
        let v = |k: &str, d: f32| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
        Thresholds {
            p95: v("PAYBACK_PERF_MAX_P95", 25.0),
            max_frame: v("PAYBACK_PERF_MAX_FRAME", 150.0),
            main_p95: v("PAYBACK_PERF_MAX_MAIN", 16.0),
        }
    }
}

/// Seconds at the start of a `--perf-check` run that are not judged
/// (start-up, level loading, shader compilation).
const WARMUP: f64 = 5.0;

fn perf_check() -> Option<f64> {
    crate::arg("--perf-check")
        .and_then(|s| s.parse().ok())
        .or_else(|| crate::args().any(|a| a == "--perf-check").then_some(30.0))
}

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("PAYBACK_PERF").is_none() && perf_check().is_none() {
            return;
        }
        ON.store(true, Ordering::Relaxed);
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.init_resource::<Perf>()
            .add_systems(First, frame_start)
            .add_systems(
                RunFixedMainLoop,
                (
                    mark_fixed_start.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
                    mark_fixed_end.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
                ),
            )
            .add_systems(FixedFirst, count_tick)
            .add_systems(Last, frame_end);
    }
}

/// Samples of one report window.
#[derive(Default, Clone)]
struct Stats {
    /// (frame ms, ticks run in it).
    frames: Vec<(f32, u32)>,
    /// Main-app ms of each frame (`First`..`Last`).
    main: Vec<f32>,
    /// ms between successive frames that ran a tick.
    cadence: Vec<f32>,
}

#[derive(Resource)]
struct Perf {
    begun: Instant,
    frame_start: Option<Instant>,
    fixed_start: Instant,
    fixed_end: Instant,
    ticks: u32,
    pending: Option<Spans>,
    last_tick_frame: Option<Instant>,
    window: Stats,
    all: Stats,
    /// `--perf-check`: the frames after the warm-up.
    judged: Stats,
    next_report: f64,
    exit_after: Option<f64>,
}

impl Default for Perf {
    fn default() -> Self {
        let now = Instant::now();
        Perf {
            begun: now,
            frame_start: None,
            fixed_start: now,
            fixed_end: now,
            ticks: 0,
            pending: None,
            last_tick_frame: None,
            window: Stats::default(),
            all: Stats::default(),
            judged: Stats::default(),
            next_report: 10.0,
            exit_after: perf_check().or_else(|| std::env::var("PAYBACK_PERF_EXIT").ok().and_then(|s| s.parse().ok())),
        }
    }
}

/// Main-app time of a frame (`First`..`Last`) by phase.
#[derive(Default)]
struct Spans {
    start: Option<Instant>,
    pre: f32,
    fixed: f32,
    upd: f32,
    ticks: u32,
}

fn ms(a: Instant, b: Instant) -> f32 {
    b.saturating_duration_since(a).as_secs_f32() * 1000.0
}

fn frame_start(mut perf: ResMut<Perf>) {
    let now = Instant::now();
    if let Some(spans) = perf.pending.take()
        && let Some(start) = spans.start
    {
        let frame = ms(start, now);
        if frame > 25.0 {
            let main = spans.pre + spans.fixed + spans.upd;
            info!(
                "perf: frame {frame:.1} ms, {} ticks (pre {:.1} fixed {:.1} upd {:.1} wait {:.1})",
                spans.ticks,
                spans.pre,
                spans.fixed,
                spans.upd,
                (frame - main).max(0.0)
            );
        }
        let main = spans.pre + spans.fixed + spans.upd;
        perf.window.frames.push((frame, spans.ticks));
        perf.all.frames.push((frame, spans.ticks));
        perf.all.main.push(main);
        if perf.begun.elapsed().as_secs_f64() > WARMUP {
            perf.judged.frames.push((frame, spans.ticks));
            perf.judged.main.push(main);
        }
        if spans.ticks > 0 {
            if let Some(prev) = perf.last_tick_frame {
                let gap = ms(prev, start);
                perf.window.cadence.push(gap);
                perf.all.cadence.push(gap);
            }
            perf.last_tick_frame = Some(start);
        }
    }
    perf.frame_start = Some(now);
}

fn mark_fixed_start(mut perf: ResMut<Perf>) {
    perf.fixed_start = Instant::now();
}

fn mark_fixed_end(mut perf: ResMut<Perf>) {
    perf.fixed_end = Instant::now();
}

fn count_tick(mut perf: ResMut<Perf>) {
    perf.ticks += 1;
}

fn frame_end(mut perf: ResMut<Perf>, mut commands: Commands, exiting: MessageReader<AppExit>) {
    let now = Instant::now();
    let Some(start) = perf.frame_start else { return };
    // Logged by the next `frame_start`, once the frame's length is known.
    perf.pending = Some(Spans {
        start: Some(start),
        pre: ms(start, perf.fixed_start),
        fixed: ms(perf.fixed_start, perf.fixed_end),
        upd: ms(perf.fixed_end, now),
        ticks: std::mem::take(&mut perf.ticks),
    });
    let t = perf.begun.elapsed().as_secs_f64();
    let quit = perf.exit_after.is_some_and(|l| t >= l);
    if t >= perf.next_report || quit || !exiting.is_empty() {
        perf.next_report = t + 10.0;
        let w = std::mem::take(&mut perf.window);
        report("last 10 s", &w);
        if quit || !exiting.is_empty() {
            report("whole run", &perf.all);
        }
    }
    if quit {
        perf.exit_after = None;
        let ok = perf_check().is_none() || judge(&perf.judged, Thresholds::from_env());
        commands.write_message(if ok { AppExit::Success } else { AppExit::error() });
    }
}

/// `--perf-check`: the judged frames against the limits; logs the verdict.
fn judge(s: &Stats, t: Thresholds) -> bool {
    let (Some(f), Some(m)) = (percentiles(s.frames.iter().map(|f| f.0).collect()), percentiles(s.main.clone())) else {
        error!("perf-check: FAIL, no frames after the {WARMUP} s warm-up");
        return false;
    };
    let mut fails = Vec::new();
    if f[2] > t.p95 {
        fails.push(format!("frame p95 {:.1} ms > {}", f[2], t.p95));
    }
    if f[4] > t.max_frame {
        fails.push(format!("frame max {:.1} ms > {}", f[4], t.max_frame));
    }
    if m[2] > t.main_p95 {
        fails.push(format!("main-app p95 {:.1} ms > {}", m[2], t.main_p95));
    }
    let summary = format!(
        "{} frames, frame p95 {:.1} max {:.1} ms, main-app p50 {:.1} p95 {:.1} max {:.1} ms",
        s.frames.len(),
        f[2],
        f[4],
        m[1],
        m[2],
        m[4]
    );
    if fails.is_empty() {
        info!("perf-check: PASS ({summary})");
        println!("perf-check: PASS ({summary})");
        true
    } else {
        error!("perf-check: FAIL: {} ({summary})", fails.join(", "));
        println!("perf-check: FAIL: {} ({summary})", fails.join(", "));
        false
    }
}

fn percentiles(mut v: Vec<f32>) -> Option<[f32; 5]> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let p = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
    Some([p(0.01), p(0.5), p(0.95), p(0.99), v[v.len() - 1]])
}

fn report(what: &str, s: &Stats) {
    let Some(f) = percentiles(s.frames.iter().map(|f| f.0).collect()) else { return };
    let long = s.frames.iter().filter(|f| f.0 > 25.0).count();
    let mut hist = [0usize; 5];
    for f in &s.frames {
        hist[(f.1 as usize).min(4)] += 1;
    }
    let c = percentiles(s.cadence.clone()).unwrap_or_default();
    let off = s.cadence.iter().filter(|&&g| (g - 100.0 / 3.0).abs() > 5.0).count();
    let a = |v: &AtomicU64| v.load(Ordering::Relaxed);
    info!(
        "perf {what}: {} frames, frame ms p50 {:.1} p95 {:.1} p99 {:.1} max {:.1}, >25 ms: {long}; \
         ticks/frame 0:{} 1:{} 2:{} 3:{} 4+:{}; tick cadence ms p1 {:.1} p50 {:.1} p99 {:.1} max {:.1}, \
         off by >5 ms: {off}/{}; audio max gap {:.1} ms, max lead {:.1} ms, underruns {}, max lock wait {} us",
        s.frames.len(),
        f[1],
        f[2],
        f[3],
        f[4],
        hist[0],
        hist[1],
        hist[2],
        hist[3],
        hist[4],
        c[0],
        c[1],
        c[3],
        c[4],
        s.cadence.len(),
        a(&AUDIO_MAX_GAP_US) as f32 / 1000.0,
        a(&AUDIO_MAX_LEAD) as f32 * 1000.0 / crate::audio::MIX_RATE as f32,
        a(&AUDIO_UNDERRUNS),
        a(&AUDIO_MAX_LOCK_US),
    );
}
