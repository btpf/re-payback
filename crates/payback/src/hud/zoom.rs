//! The level-start zoom-in map (Mac `ZoomInMap__Fiii` @ 1009a9c0, OS4
//! `FUN_010247e4`; called by `amiga_levelloop` ~l.99925 once the camera
//! sits over the player at height 10000, in software rendering and only
//! when the flag `DAT_1017c334` is clear: story levels here, not Rampage,
//! split screen or replay playback (INFERRED for the last); hardware
//! rendering skips it, so the Mac OS X build normally never shows it).
//!
//! The game does not run meanwhile. Each frame (here each 1/30 s):
//!
//! - the city's 256×256 map picture (`Maps/<n>.thumb`, 4×4 tiles of 64,
//!   1 pixel = half a square = 4 map units) is drawn flat
//!   (`drawGroundFlat`) at depth `a`: map unit `u` at
//!   `cx + xs·u·0x3f80 / (16·a)` (y with `ys`), the map's left edge at
//!   the offset `off` (start −0x200, the map centred);
//! - over it, genlocked (colour 0 transparent), the first game frame (the
//!   city drawn from height 10000) shrunk to the same scale: half-sizes
//!   `((10000 + 1500)·320 + 0x20000) >> 14` by `(… ·200 …) >> 14` map
//!   units about the camera. The port shows the live 3-D view in that
//!   rectangle instead (`camera::layout_views`);
//! - for the first 60 frames the level-select line of the level (city
//!   name 2170+c centred, level name 2180+2c+l at x 20, gold and silver
//!   cups with the target times, the earned cup and the best time at
//!   x 150 / 166 / 205 / 221 / 260 / 276) slides down from y 206 / 220;
//! - then `a = (32a + 4000) / 33` and `off = (8·off − cam) / 9` (the map
//!   pans until the camera's square is centred), until `a ≤ 0x5c51`:
//!   from 0x1451f that is 45 frames, about 1.5 s.

use crate::frontend::progress::{Profile, TARGETS};
use crate::level::{AppState, GameMode, LevelSpec};
use bevy::prelude::*;

/// Start depth and the depth at which the zoom stops.
const START: i32 = 0x1451f;
const END: i32 = 0x5c51;
/// Camera height of the genlocked first frame.
const CAM_H: i32 = 10000;

/// The zoom in progress, if any.
#[derive(Resource, Default)]
pub struct ZoomIn {
    /// Armed at level start; starts once the camera is placed.
    pending: bool,
    pub zoom: Option<Zoom>,
}

pub struct Zoom {
    /// Depth.
    pub a: i32,
    /// Map offset (`local_68`, `iVar21`) and the camera (`cam >> 13`), map units.
    pub off: IVec2,
    pub cam: IVec2,
    /// Frames shown (`local_a4`).
    pub frame: i32,
    acc: f64,
    /// The game clock was paused by the zoom.
    paused: bool,
    pub city: usize,
    pub level: usize,
    /// `Maps/<city>.thumb`, 256×256 palette indices.
    pub thumb: Vec<u8>,
    /// `Cup.None/.Bronze/.Silver/.Gold`, lo-res and software hi-res.
    pub cups: Vec<super::icons::Icon>,
    pub cups2x: Vec<super::icons::Icon>,
}

impl Zoom {
    /// Map units to lo-res pixels from the centre (xs = 16: `0x3f80 / a`).
    fn scale(&self) -> f32 {
        0x3f80 as f32 / self.a as f32
    }

    /// The genlocked first frame's rectangle, as fractions of the view
    /// (x, y, w, h): where the live 3-D view is drawn.
    pub fn view_rect(&self) -> Rect {
        let s = self.scale();
        let hw = (((CAM_H + 1500) * 320 + 0x20000) >> 14) as f32 * s / 320.0;
        let hh = (((CAM_H + 1500) * 200 + 0x20000) >> 14) as f32 * s / 256.0;
        let c = Vec2::new(
            0.5 + (self.off.x + self.cam.x) as f32 * s / 320.0,
            0.5 + (self.off.y + self.cam.y) as f32 * s / 256.0,
        );
        Rect::from_center_half_size(c, Vec2::new(hw, hh))
    }

    /// One step (after drawing).
    fn step(&mut self) {
        self.a = (self.a * 32 + 4000) / 33;
        self.off = (self.off * 8 - self.cam) / 9;
        self.frame += 1;
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ZoomIn>()
        .add_systems(OnEnter(AppState::Playing), arm.in_set(crate::level::LevelLoaded))
        .add_systems(Update, run.run_if(in_state(AppState::Playing)))
        .add_systems(PostUpdate, paint)
        .add_systems(OnExit(AppState::Playing), |mut z: ResMut<ZoomIn>| *z = ZoomIn::default());
}

/// Debug: `--zoom-in` shows it whatever the flags below; `--zoom-frame n`
/// holds it at frame n (screenshots).
fn forced() -> bool {
    crate::args().any(|a| a == "--zoom-in") || crate::arg("--zoom-frame").is_some()
}

fn arm(
    mut z: ResMut<ZoomIn>,
    mode: Res<GameMode>,
    replay: Res<crate::replay::ReplayState>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
) {
    // Automated runs (replay checks, bots, perf checks) go straight in.
    let automated = ["--record", "--play", "--autoplay", "--perf-check", "--script-input"]
        .iter()
        .any(|f| crate::args().any(|a| a == *f));
    let software = !crate::render_mode::is_hardware();
    let story = *mode == GameMode::Story && !replay.playing();
    let on = settings.is_none_or(|s| s.zoom_in_map);
    z.pending = forced() || (software && story && on && !automated);
    z.zoom = None;
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut z: ResMut<ZoomIn>,
    data: Res<crate::data::GameData>,
    spec: Res<LevelSpec>,
    cam: Res<crate::camera::GameCam>,
    real: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    menu: Option<Res<crate::frontend::Layers>>,
) {
    if z.pending {
        // The camera is placed over the player by the first tick.
        if !cam.initialised {
            return;
        }
        z.pending = false;
        let city = spec.city().unwrap_or(0) as usize;
        let mut thumb = data.read_variant(format!("Maps/{city}.thumb"), true).unwrap_or_default();
        thumb.resize(256 * 256, 0);
        let cup = |n: &str| super::icons::Icon::load(&data, &format!("Cup.{n}"), 15, 20);
        let paused = !time.is_paused();
        time.pause();
        z.zoom = Some(Zoom {
            a: START,
            off: IVec2::splat(-0x200),
            cam: IVec2::new((cam.x >> 13) as i32, (cam.y >> 13) as i32),
            frame: 0,
            acc: 0.0,
            paused,
            city: city.min(2),
            level: (spec.level as usize).min(1),
            thumb,
            cups2x: {
                let avg = super::icons::Average::load(&data);
                ["None", "Bronze", "Silver", "Gold"].iter().map(|n| cup(n).doubled(&avg)).collect()
            },
            cups: ["None", "Bronze", "Silver", "Gold"].iter().map(|n| cup(n)).collect(),
        });
    }
    let Some(zoom) = z.zoom.as_mut() else { return };
    let hold = crate::arg("--zoom-frame").and_then(|v| v.parse::<i32>().ok());
    zoom.acc = (zoom.acc + real.delta_secs_f64() * 30.0).min(4.0);
    while zoom.acc >= 1.0 && zoom.a > END && hold.is_none_or(|h| zoom.frame < h) {
        zoom.acc -= 1.0;
        zoom.step();
    }
    if zoom.a <= END {
        // The pause menu, if opened meanwhile, keeps the clock stopped.
        if zoom.paused && menu.is_none() {
            time.unpause();
        }
        z.zoom = None;
    }
}

/// Render layer of the zoom picture, seen only by the background camera
/// behind the shrunk game view (`camera::SoloBackground`).
pub const LAYER: usize = 29;

/// The sprite showing the zoom picture.
#[derive(Component)]
struct ZoomSprite(Handle<Image>);

/// Paint the zoom into its own window-sized picture, shown by a sprite on
/// [`LAYER`] behind the game view (whose viewport is the genlocked frame's
/// rectangle, `Zoom::view_rect`); the HUD is not drawn meanwhile.
#[allow(clippy::too_many_arguments)]
fn paint(
    mut commands: Commands,
    z: Res<ZoomIn>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    sprite: Query<(Entity, &ZoomSprite, &mut Sprite)>,
    hud: (Option<Res<super::HudIcons>>, Option<Res<super::HudIcons2x>>, Option<Res<crate::frontend::progress::Settings>>),
    data: Res<crate::data::GameData>,
    texts: Res<super::Texts>,
    profile: Res<Profile>,
) {
    let Some(zoom) = &z.zoom else {
        for (e, ..) in &sprite {
            commands.entity(e).despawn();
        }
        return;
    };
    let (Some(lo), icons2x, settings) = hud else { return };
    let Ok(win) = window.single() else { return };
    let (w, h, hi) = super::hud_canvas(win.physical_size(), settings.as_ref().is_none_or(|s| s.hires_hud));
    let bilinear = hi && crate::render_mode::is_hardware();
    let set: &super::HudIcons = match &icons2x {
        Some(d) if hi && !bilinear => &d.0,
        _ => &lo,
    };
    let mut sprite = sprite;
    let handle = match sprite.iter_mut().next() {
        Some((_, s, mut sp)) => {
            if sp.custom_size != Some(win.size()) {
                sp.custom_size = Some(win.size());
            }
            s.0.clone()
        }
        None => {
            let handle = images.add(super::blank_sized(w, h));
            commands.spawn((
                Sprite { image: handle.clone(), custom_size: Some(win.size()), ..default() },
                Transform::default(),
                bevy::camera::visibility::RenderLayers::layer(LAYER),
                ZoomSprite(handle.clone()),
                crate::level::LevelEntity,
            ));
            handle
        }
    };
    let Some(mut img) = images.get_mut(&handle) else { return };
    if (img.width() as usize, img.height() as usize) != (w, h) {
        *img = super::blank_sized(w, h);
    }
    let Some(px) = img.data.as_mut() else { return };
    let mut p = super::Painter { px, w, pal: &data.palette };
    let mut sub = Vec::new();
    let mut pen = super::Pen { p: &mut p, sub: &mut sub, icons: set, hi, bilinear, hardware: crate::render_mode::is_hardware() };
    draw(&mut pen, zoom, &profile, &texts, w as i32, h as i32);
}

/// Draw the zoom (`w`×`h`, `k` = 2 in hi-res): the map, then the sliding
/// level line.
fn draw(pen: &mut super::Pen, zoom: &Zoom, profile: &Profile, texts: &super::Texts, w: i32, h: i32) {
    let xs = (w + 8) * 16 / 320;
    let ys = ((h + 8) * 16) >> 8;
    let (cx, cy) = (w / 2, h / 2);
    let pal = pen.p.pal;
    for sy in 0..h {
        let uy = ((sy - cy) as i64 * 16 * zoom.a as i64 / (ys as i64 * 0x3f80)) as i32 - zoom.off.y;
        for sx in 0..w {
            let ux = ((sx - cx) as i64 * 16 * zoom.a as i64 / (xs as i64 * 0x3f80)) as i32 - zoom.off.x;
            let (mx, my) = (ux.div_euclid(4), uy.div_euclid(4));
            let c = if (0..256).contains(&mx) && (0..256).contains(&my) {
                zoom.thumb[(my * 256 + mx) as usize] as usize
            } else {
                0
            };
            pen.p.put_rgb(sx, sy, [pal[c * 3], pal[c * 3 + 1], pal[c * 3 + 2]]);
        }
    }
    if zoom.frame >= 60 {
        return;
    }
    // The level-select line (`FrontendIcons` positions, xs/ys-scaled).
    let k = pen.k();
    let ic = pen.icons;
    let them = &ic.them;
    let you = &ic.you;
    let sx = |x: i32| xs * x >> 4;
    let y1 = (zoom.frame + 0xce) * ys >> 4;
    let y2 = (zoom.frame + 0xdc) * ys >> 4;
    let name = texts.global(2170 + zoom.city as u32);
    let x = cx - ((7 * k * super::text_len(&name)) >> 1);
    pen.text(them, &name, x, y1, 7, super::Blend::Lighten);
    let i = zoom.city * 2 + zoom.level;
    pen.text(them, &texts.global(2180 + i as u32), sx(20), y2, 7, super::Blend::Lighten);
    let [gold, silver] = TARGETS[zoom.city][zoom.level];
    let best = profile.best[i];
    let cup = profile.cups[i].clamp(0, 3) as usize;
    let best = if best < 0 { "-:--".to_string() } else { crate::frontend::ui::hmm(best) };
    for (icon, at, text) in [
        (3, 150, crate::frontend::ui::hmm(gold)),
        (2, 205, crate::frontend::ui::hmm(silver)),
        (cup, 260, best),
    ] {
        let cups = if pen.hi && !pen.bilinear { &zoom.cups2x } else { &zoom.cups };
        if let Some(c) = cups.get(icon) {
            pen.icon(c, sx(at), y2 - 3 * k, super::Blend::Plain);
        }
        pen.text(you, &text, sx(at + 16), y2, 7, super::Blend::Lighten);
    }
}
