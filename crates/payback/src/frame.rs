//! Frame pacing and presentation (port-added; the originals render one
//! frame per game loop iteration, docs/port_engineering.md "Frame pacing").
//!
//! The Mac main loop (`amiga_levelloop__Fv` @ 10100f50) and OS4
//! (`FUN_0110761c`) run `clamp(elapsed 30 Hz ticks, 1, 5)` game ticks and
//! then draw one frame, so in game at most 30 frames a second are shown and
//! every frame shows a new state; there is no interpolation and no vsync
//! request (`aglSetInteger` is not imported; `docs/re/mac/audio_platform.md`
//! §4.1). The port draws at the display's rate, so it adds:
//!
//! - **Smooth motion** ([`Settings::smooth_motion`], Graphics options, on
//!   by default): render-only interpolation between the last two game
//!   ticks of every simulated object's transform ([`smooth_objects`]) and
//!   of the views' cameras (`camera::ViewEyes::prev`), one tick behind the
//!   simulation. It never touches simulation state: it rewrites `Transform`
//!   after the sync systems copied [`Obj`] into it (nothing in the
//!   simulation reads `Transform`), and keeps its bookkeeping in a resource
//!   (render code must not add components to `Obj` entities). Off gives the
//!   original's once-per-tick look. Replays are unaffected.
//! - **Present mode and frame limit** ([`Settings::vsync`],
//!   [`Settings::frame_limit`]): vsync on (`PresentMode::AutoVsync`) or off
//!   (`AutoNoVsync`), and an optional cap of 30/60/120/144 frames a second
//!   (30 = the original's in-game rate).
//! - **Catch-up clamp** ([`clamp_catch_up`]): at most 5 ticks per frame
//!   after a hitch, the original's `clamp(ticks, 1, 5)`.
//! - **Cursor** ([`cursor`]): hidden over the game window (the Mac hides it
//!   in fullscreen, `UDrawSprocket::Activate` @ 10157490 `HideCursor`; the
//!   mouse is used neither in game nor in the menus). Losing the focus
//!   does not pause the game, as on the Mac: `UDesktop::Suspend`
//!   @ 101591b0 only deactivates the PowerPlant windows and the game loop
//!   keeps running (Bevy releases the held keys when the focus goes).

use crate::frontend::progress::Settings;
use crate::sim::Obj;
use bevy::{
    platform::collections::HashMap,
    prelude::*,
    window::{CursorOptions, PresentMode, PrimaryWindow, WindowMode},
};
use bevy::platform::time::Instant;
use std::time::Duration;

/// The original's catch-up limit: at most 5 ticks per frame.
pub const MAX_TICKS_PER_FRAME: u32 = 5;

/// Frame limits offered by the Graphics options (0 = none).
pub const FRAME_LIMITS: [u16; 5] = [0, 30, 60, 120, 144];

/// Interpolation state of this frame.
#[derive(Resource, Default)]
pub struct Smooth {
    /// Smooth motion is on.
    pub on: bool,
    /// How far the display is between the previous tick's state (0) and
    /// the last tick's (1): `Time<Fixed>::overstep_fraction`; 1 when off.
    pub alpha: f32,
    /// Game ticks run so far (counted in `FixedLast`).
    ticks: u64,
    /// `ticks` at the previous frame.
    seen: u64,
    /// One or more ticks ran since the previous frame.
    pub new_tick: bool,
}

/// Render-space shift of each smoothed object this frame (shown minus its
/// last tick's translation), for render code that draws an object from
/// its `Obj` rather than its transform (the object shadows).
#[derive(Resource, Default)]
pub struct SmoothOffsets(pub HashMap<Entity, Vec3>);

impl SmoothOffsets {
    pub fn get(&self, e: Entity) -> Vec3 {
        self.0.get(&e).copied().unwrap_or(Vec3::ZERO)
    }
}

/// One object's last two tick states.
#[derive(Clone, Copy)]
struct Track {
    prev: Transform,
    curr: Transform,
    /// What was written last frame (to tell a sync system's new value from
    /// the interpolated one left in place).
    shown: Transform,
    /// Frame stamp, to forget despawned entities.
    frame: u32,
}

#[derive(Resource, Default)]
struct Tracks {
    map: HashMap<Entity, Track>,
    frame: u32,
}

/// Translations further apart than this (render units = squares) in one
/// tick are a teleport (respawn, boarding): shown without a sweep.
const SNAP: f32 = 4.0;

/// The interpolation runs in this set, in `PostUpdate` before transform
/// propagation (render code that places entities from an object's
/// transform, e.g. `objects::follow_owners`, runs after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SmoothSet;

pub struct FramePlugin;

impl Plugin for FramePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Smooth>()
            .init_resource::<SmoothOffsets>()
            .init_resource::<Tracks>()
            .add_systems(Startup, clamp_catch_up)
            .add_systems(FixedLast, |mut s: ResMut<Smooth>| s.ticks += 1)
            .add_systems(PreUpdate, (present_mode, cursor))
            .add_systems(
                PostUpdate,
                (begin, smooth_objects)
                    .chain()
                    .in_set(SmoothSet)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(Last, frame_limit);
    }
}

/// The original runs `clamp(elapsed ticks, 1, 5)` ticks per frame
/// (`amiga_levelloop__Fv` @ 10100f50): cap the virtual clock's step at 5
/// ticks (Bevy's default is 250 ms, 7.5 ticks). `--fast` sets its own.
fn clamp_catch_up(mut time: ResMut<Time<Virtual>>) {
    if crate::arg("--fast").is_none() {
        time.set_max_delta(Duration::from_secs_f64(MAX_TICKS_PER_FRAME as f64 / 30.0));
    }
}

impl Smooth {
    /// Game ticks run so far.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
}

fn begin(mut smooth: ResMut<Smooth>, fixed: Res<Time<Fixed>>, settings: Option<Res<Settings>>) {
    smooth.on = settings.as_ref().is_none_or(|s| s.smooth_motion);
    smooth.new_tick = smooth.ticks != smooth.seen;
    smooth.seen = smooth.ticks;
    smooth.alpha = if smooth.on { fixed.overstep_fraction().clamp(0.0, 1.0) } else { 1.0 };
}

/// Blend two transforms (`t` = 0: `a`, 1: `b`).
pub fn blend(a: &Transform, b: &Transform, t: f32) -> Transform {
    Transform {
        translation: a.translation.lerp(b.translation, t),
        rotation: a.rotation.slerp(b.rotation, t),
        scale: a.scale.lerp(b.scale, t),
    }
}

/// Show every simulated object between its last two tick states.
///
/// The sync systems (`driving::sync_vehicles`, `people::pose_people`,
/// `combat::sync_projectiles`) write the last tick's transform every
/// frame; an object whose transform was not rewritten keeps the value
/// shown last frame, which is recognised and replaced by its tick state.
fn smooth_objects(
    smooth: Res<Smooth>,
    mut tracks: ResMut<Tracks>,
    mut offsets: ResMut<SmoothOffsets>,
    mut q: Query<(Entity, &mut Transform), With<Obj>>,
) {
    offsets.0.clear();
    if !smooth.on {
        if !tracks.map.is_empty() {
            // Leave the tick states in place, then forget them.
            for (e, mut tf) in &mut q {
                if let Some(t) = tracks.map.get(&e)
                    && *tf == t.shown
                {
                    *tf = t.curr;
                }
            }
            tracks.map.clear();
        }
        return;
    }
    let tracks = &mut *tracks;
    tracks.frame = tracks.frame.wrapping_add(1);
    let frame = tracks.frame;
    for (e, mut tf) in &mut q {
        let now = *tf;
        let t = tracks.map.entry(e).or_insert(Track {
            prev: now,
            curr: now,
            shown: now,
            frame,
        });
        let target = if now == t.shown { t.curr } else { now };
        if smooth.new_tick {
            t.prev = t.curr;
            t.curr = target;
        } else if target != t.curr {
            // Changed without a tick (spawned, re-posed by a menu): no sweep.
            t.prev = target;
            t.curr = target;
        }
        if t.prev.translation.distance(t.curr.translation) > SNAP {
            t.prev = t.curr;
        }
        t.frame = frame;
        let shown = blend(&t.prev, &t.curr, smooth.alpha);
        t.shown = shown;
        let d = shown.translation - t.curr.translation;
        if d != Vec3::ZERO {
            offsets.0.insert(e, d);
        }
        if *tf != shown {
            *tf = shown;
        }
    }
    if frame % 64 == 0 {
        tracks.map.retain(|_, t| t.frame == frame);
    }
}

/// Vsync on or off ([`Settings::vsync`]).
fn present_mode(settings: Option<Res<Settings>>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let (Some(s), Ok(mut w)) = (settings, window.single_mut()) else { return };
    let want = if s.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if w.present_mode != want {
        w.present_mode = want;
    }
}

/// Hide the cursor over the window in game, and always in fullscreen
/// (the Mac's `HideCursor` in `UDrawSprocket::Activate` @ 10157490).
fn cursor(
    state: Res<State<crate::level::AppState>>,
    mut q: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((w, mut c)) = q.single_mut() else { return };
    let fullscreen = !matches!(w.mode, WindowMode::Windowed);
    let visible = !fullscreen && *state.get() != crate::level::AppState::Playing;
    if c.visible != visible {
        c.visible = visible;
    }
}

/// Frame limit ([`Settings::frame_limit`]): sleep out the rest of the
/// frame (the last millisecond spun for accuracy).
fn frame_limit(settings: Option<Res<Settings>>, mut last: Local<Option<Instant>>) {
    // Web build: the browser paces the frames; blocking would stall the page.
    if cfg!(target_arch = "wasm32") {
        return;
    }
    let cap = settings.as_ref().map_or(0, |s| s.frame_limit);
    let now = Instant::now();
    if cap == 0 || crate::arg("--fast").is_some() {
        *last = Some(now);
        return;
    }
    let period = Duration::from_secs_f64(1.0 / cap as f64);
    if let Some(prev) = *last {
        let due = prev + period;
        if due > now {
            let left = due - now;
            // No sleeping on the web (the browser paces the frames).
            #[cfg(not(target_arch = "wasm32"))]
            if left > Duration::from_millis(2) {
                std::thread::sleep(left - Duration::from_millis(1));
            }
            while Instant::now() < due {
                std::hint::spin_loop();
            }
            // Keep the cadence; resynchronise after a slow frame.
            *last = Some(due);
            return;
        }
    }
    *last = Some(now);
}
