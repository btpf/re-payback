//! Cameras: the game's overhead camera (docs/re/rendering.md §6, OS4
//! `FUN_010556e0`) and a free debug camera (toggle with F6; F2 is a
//! previous-weapon key on the Mac).
//!
//! Split screen (multiplayer, `FUN_0105bd98`): every player has a view
//! with its own camera state ([`ViewCams`]). Player 1 keeps the
//! [`GameCamera`]; players 2..4 get [`ExtraView`] cameras, each drawn into
//! its viewport; a full-window [`OverlayCamera`] draws the HUD on top.
//!
//! Rotating view (pause menu option, `DAT_0114c7dc`; [`RotatingView`]):
//! the camera turns with the followed object's heading and tilts forward
//! with its speed ([`ViewAngles`], updated once per frame by the render loop
//! `FUN_0105bd98`), follows with its own parameters (`FUN_010556e0`) and
//! projects with half the focal length (`FUN_01091f84`); rendering.md §6.
//! It is render-only here: the views are drawn from [`ViewEyes`] while the
//! simulation keeps the top-down [`GameCam`]. During single-player replay
//! playback the rotating view becomes the Mac's roaming "TV camera"
//! ([`TvCamera`], `DrawMap` @ 1005a260).
//!
//! The game views render in palette space (`postfx.rs`): HDR targets that
//! hold palette-encoded RGB, snapped to the palette, motion-blurred and
//! flashed by one final pass.

#[path = "postfx.rs"]
pub mod postfx;

use crate::level::{AppState, GameMode, LevelEntity};
use crate::sim::{Obj, TURN, Trig};
use crate::vehicles::Vehicle;
use crate::world::H_PER_SQUARE;
use bevy::{
    camera::{PerspectiveProjection, Viewport, visibility::RenderLayers},
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    window::PrimaryWindow,
};

/// Marks the object the game camera follows.
#[derive(Component)]
pub struct CameraTarget;

/// The original camera state: x, y in 16.16 squares, and `h` such that the
/// camera sits at height `h + 1500` h-units (a point at height H is
/// `h + 1500 - H` below it).
#[derive(Resource, Clone, Copy, Debug)]
pub struct GameCam {
    pub x: i64,
    pub y: i64,
    pub h: i32,
    pub initialised: bool,
}

impl Default for GameCam {
    fn default() -> Self {
        // The level intro starts high (camH 10000) and eases down.
        GameCam {
            x: 64 << 16,
            y: 64 << 16,
            h: 10000,
            initialised: false,
        }
    }
}

impl GameCam {
    /// Squares visible either side of the camera, plus margin: the
    /// spawn/despawn window half-size (people.md §7.2).
    pub fn window(&self) -> i32 {
        ((self.h + 1500) >> 8) + 5
    }

    pub fn square(&self) -> IVec2 {
        IVec2::new((self.x >> 16) as i32, (self.y >> 16) as i32)
    }

    /// Squares drawn either side of the camera square, (x, y), by the city
    /// renderer `FUN_01049374` with the screen constants of a layout of
    /// `players` views: top-down `D·cx / (fx << 12) + 1` and
    /// `D·cy / (fy << 12) + 1`, rotating `D·cx / (fx << 10) + 1` both ways
    /// (two side-by-side views: `3·D·cx / (fx << 11) + 1`), D = camH + 1500.
    pub fn visible_squares(&self, players: usize, rotating: bool) -> (i32, i32) {
        let d = self.h + 1500;
        let (cx, cy, fx, fy) = match players {
            0 | 1 => (160, 128, 16, 16),
            2 => (80, 128, 11, 11),
            _ => (80, 64, 8, 8),
        };
        if !rotating {
            (d * cx / (fx << 12) + 1, d * cy / (fy << 12) + 1)
        } else if players == 2 {
            let r = d * cx * 3 / (fx << 11) + 1;
            (r, r)
        } else {
            let r = d * cx / (fx << 10) + 1;
            (r, r)
        }
    }
}

/// Split screen: each player's camera state, by player number (empty
/// outside multiplayer; [`GameCam`] then holds player 1's copy).
#[derive(Resource, Default, Clone)]
pub struct ViewCams(pub Vec<GameCam>);

/// Render layer of view `p`'s own effect sprites and flares (the views
/// also see layer 0, the world).
pub fn view_layer(p: usize) -> usize {
    10 + p
}

/// The split-screen view camera of player `.0` (1..3; player 0 uses the
/// [`GameCamera`]).
#[derive(Component)]
pub struct ExtraView(pub u8);

/// Full-window camera drawing the UI over the split-screen views (it
/// renders no 3D: its only render layer is empty).
#[derive(Component)]
pub struct OverlayCamera;

/// The view size in 1/256 of the window (`DAT_0114c310`, config field
/// 12): 0xB0..0x100 in steps of 0x10, '=' grows and '-' shrinks it in game
/// (`FUN_01051940` / `FUN_01051a10`), software rendering and one player
/// only (refused and forced to 0x100 under Warp3D; `FUN_0105bd98` scales
/// the view's W and H by it). The view is centred, black around it, and
/// the HUD is drawn relative to it (`FUN_0101168c` moves the framebuffer
/// pointer).
pub fn view_size(settings: Option<&crate::frontend::progress::Settings>, players: usize) -> u32 {
    match settings {
        Some(s) if !s.hardware && players <= 1 => (s.view_size as u32).clamp(VIEW_SIZE_MIN, 0x100),
        _ => 0x100,
    }
}

/// Smallest view size (`DAT_0114c310`).
pub const VIEW_SIZE_MIN: u32 = 0xB0;

/// The single view's rectangle (x, y, w, h) in a `w`×`h` window at view
/// size `size` (/256), centred.
pub fn sized_view_rect(w: u32, h: u32, size: u32) -> (u32, u32, u32, u32) {
    let (vw, vh) = ((w * size / 256).max(1), (h * size / 256).max(1));
    ((w - vw.min(w)) / 2, (h - vh.min(h)) / 2, vw, vh)
}

/// '=' / '-' in game: grow or shrink the view by 0x10 (`FUN_01051940` /
/// `FUN_01051a10`), refused under hardware rendering and in split screen.
fn view_size_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<GameMode>,
    time: Res<Time<Virtual>>,
    settings: Option<ResMut<crate::frontend::progress::Settings>>,
) {
    let Some(mut s) = settings else { return };
    if s.hardware || mode.is_multiplayer() || time.is_paused() {
        return;
    }
    let d: i32 = if keys.just_pressed(KeyCode::Equal) {
        0x10
    } else if keys.just_pressed(KeyCode::Minus) {
        -0x10
    } else {
        return;
    };
    let next = (s.view_size as i32 + d).clamp(VIEW_SIZE_MIN as i32, 0x100) as u16;
    if next != s.view_size {
        s.view_size = next;
    }
}

/// Render layer nothing is on (the overlay camera's).
const EMPTY_LAYER: usize = 30;

/// Split-screen view rectangles (x, y, w, h) in physical pixels
/// (`FUN_0105bd98` at `0105c4a0`): 2 players side by side, W/2-1 wide;
/// 3-4 in quadrants of H/2-1, the third centred at the bottom when there
/// are 3. Separators are 1 line of the 256-line screen.
pub fn view_rects(n: usize, w: u32, h: u32) -> Vec<(u32, u32, u32, u32)> {
    let sep = (h / 256).max(1);
    let vw = w.saturating_sub(sep) / 2;
    let vh = h.saturating_sub(sep) / 2;
    match n {
        0 | 1 => vec![(0, 0, w, h)],
        2 => vec![(0, 0, vw, h), (w - vw, 0, vw, h)],
        _ => {
            let mut v = vec![(0, 0, vw, vh), (w - vw, 0, vw, vh)];
            if n == 3 {
                v.push(((w - vw) / 2, h - vh, vw, vh));
            } else {
                v.push((0, h - vh, vw, vh));
                v.push((w - vw, h - vh, vw, vh));
            }
            v
        }
    }
}

#[derive(Component)]
pub struct GameCamera {
    pub mode: CameraMode,
    pub free: FreeRig,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraMode {
    Follow,
    Free,
}

/// Orbit rig for the free camera, in render units.
#[derive(Clone, Copy)]
pub struct FreeRig {
    pub focus: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl FreeRig {
    fn transform(&self) -> Transform {
        let rot = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        Transform::from_translation(self.focus + rot * Vec3::new(0.0, 0.0, self.distance))
            .looking_at(self.focus, Vec3::Y)
    }
}

/// Follow parameters (rendering.md §6): look-ahead K, velocity lead V,
/// base height B, speed factor c and, in the rotating view, the height
/// offset R (`camH = (2·camH + min(t, 3000) − R) / 3`, t not clamped below).
struct Follow {
    k: i64,
    v: i64,
    b: i32,
    c: i32,
    r: Option<i32>,
}

const ON_FOOT: Follow = Follow {
    k: 100,
    v: 0x140,
    b: 1000,
    c: 3,
    r: None,
};
const DRIVING: Follow = Follow {
    k: 150,
    v: 200,
    b: 800,
    c: 3,
    r: None,
};
/// The player's RC car, top-down (Mac `UpdateMapPosition` @ 10068cb0, the
/// `_DAT_1017c484 != −1` branch: lead `v·150`, `sin·225`, height
/// `clamp(|3·speed| + 800 − z/16, 1, 3000)`). With the rotating view it
/// uses `v·200`, `sin·225`, `min(|2·speed| + 1000 − z/16, 3000) − 600`,
/// i.e. [`DRIVING_ROTATING`].
const RC: Follow = Follow {
    k: 225,
    v: 150,
    b: 800,
    c: 3,
    r: None,
};
/// Rotating view, on foot (`FUN_010556e0`, model `+0x4909` set: people).
const ON_FOOT_ROTATING: Follow = Follow {
    k: 150,
    v: 0x140,
    b: 1500,
    c: 2,
    r: Some(800),
};
/// Rotating view, driving (vehicle model `+0x4909` clear).
const DRIVING_ROTATING: Follow = Follow {
    k: 225,
    v: 200,
    b: 1000,
    c: 2,
    r: Some(600),
};

/// The "Rotating view" option (`DAT_0114c7dc`, pause menu → Graphics
/// options; needs `DAT_0114c2f4`, initialised to 1). Follows
/// `Settings::rotating_view`; `--rotating-view [0|1]` or
/// `--no-rotating-view` wins. Off by default, as in the original (Mac
/// `ClearPreference` @ 100508c0 leaves the pref read by `LoadConfig` @
/// 10052be0 at +0x2a4 zero; Amiga config field 18). The tilted minimap
/// that goes with it in the original is a separate setting in the port
/// (`Settings::minimap_3d`, `hud::minimap_3d`).
///
/// Departure: in the original the rotating view's follow parameters move
/// the camera state the simulation sees (people spawn window, traffic). The
/// port keeps it render-only (it is not recorded in replays): the
/// simulation's [`GameCam`] / [`ViewCams`] always follow with the top-down
/// parameters, and the views are drawn from [`ViewEyes`].
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotatingView(pub bool);

/// Render-only camera state of every view, the position each view is drawn
/// from (`FUN_010556e0` with the rotating-view parameters when the option
/// is on, else the same as the simulation's cameras; the replay TV camera
/// replaces view 0's, [`TvCamera`]). Always at least one entry.
#[derive(Resource, Default, Clone, Debug)]
pub struct ViewEyes {
    /// The followed camera of each view, one game tick at a time.
    follow: Vec<GameCam>,
    /// `follow` one tick earlier: with smooth motion (`crate::frame`) the
    /// views are drawn between the two, as the objects are.
    prev: Vec<GameCam>,
    /// The camera each view is drawn from this frame.
    pub cams: Vec<GameCam>,
    /// View 0 is the replay TV camera (its visible range is 3 squares
    /// larger, `DrawMap` @ 1005a260).
    pub tv: bool,
}

impl ViewEyes {
    /// Squares drawn either side of view `i`'s camera
    /// ([`GameCam::visible_squares`], +3 for the TV camera).
    pub fn visible_squares(&self, i: usize, rotating: bool) -> (i32, i32) {
        let (x, y) = self.cams[i].visible_squares(self.cams.len(), rotating);
        let extra = if self.tv && i == 0 { 3 } else { 0 };
        (x + extra, y + extra)
    }
}

impl Default for RotatingView {
    fn default() -> Self {
        RotatingView(forced_rotating().unwrap_or(false))
    }
}

/// `--rotating-view [0|1]` or `--no-rotating-view`, overriding the setting.
fn forced_rotating() -> Option<bool> {
    if crate::args().any(|a| a == "--no-rotating-view") {
        return Some(false);
    }
    crate::args()
        .any(|a| a == "--rotating-view")
        .then(|| crate::arg("--rotating-view").is_none_or(|v| v != "0"))
}

fn follow_settings(
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mut rotating: ResMut<RotatingView>,
) {
    let Some(settings) = settings else { return };
    if settings.is_changed() && forced_rotating().is_none() && rotating.0 != settings.rotating_view {
        rotating.0 = settings.rotating_view;
    }
}

/// Rotating view: each view's yaw (`DAT_012f5bd8[p]`, copied to
/// `DAT_0114c7d0` for the projection) and pitch (`DAT_0114b5dc[p]`, copied
/// to `DAT_0114c2f0`), in 1/0x1680 turns. Render-only state, by player.
#[derive(Resource, Default, Clone, Debug)]
pub struct ViewAngles(pub Vec<ViewAngle>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewAngle {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for ViewAngle {
    fn default() -> Self {
        ViewAngle {
            yaw: 0.0,
            pitch: PITCH_DOWN as f32,
        }
    }
}

/// Yaw the 3-D minimap turns with (port-added, `Settings::minimap_3d`,
/// `hud::minimap_3d`), in 1/0x1680 turns: view 0's in the rotating view,
/// otherwise the yaw the rotating view would have (the followed object's
/// heading, eased as [`turn_views`] does) without turning the top-down
/// camera. Render-only.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct MinimapYaw(pub f32);

/// Pitch straight down (0x167f) and fully tilted (0x157c, 260 units =
/// 16.25° short of a turn).
pub const PITCH_DOWN: i32 = 0x167f;
pub const PITCH_TILTED: i32 = 0x157c;
const LEAD_CAP: i64 = 0x2af80;

/// Replay "TV camera" (Mac only, `DrawMap` @ 1005a260 prologue; Mac
/// rendering notes §2.3): with the rotating view on, while a replay plays
/// back in single player, the city is drawn from a roaming camera that
/// drifts on its own and turns to look at the followed object, re-spawning
/// when it strays. Render-only state; the yaw and pitch it draws with are
/// not stored in [`ViewAngles`] (the original restores them after the
/// frame).
#[derive(Resource, Clone, Debug)]
pub struct TvCamera {
    /// Position, 16.16 squares (`_DAT_1017d2cc`, `_DAT_1017d2c8`).
    x: f64,
    y: f64,
    /// Velocity per game tick, 16.16 squares (`_DAT_1017d2c4`, `_DAT_1017d2c0`).
    vx: i32,
    vy: i32,
    /// Yaw, 1/0x1680 turns (`_DAT_1017d2bc`).
    yaw: f32,
    /// Drawing from the TV camera this frame.
    active: bool,
}

impl Default for TvCamera {
    fn default() -> Self {
        TvCamera {
            x: 0.0,
            y: 0.0,
            vx: 0,
            vy: 0,
            yaw: 0.0,
            active: false,
        }
    }
}

/// Fixed pitch of the TV camera: 0x1cc units (28.75°) from straight down,
/// beyond the normal tilt range.
const TV_PITCH: i32 = 0x14b4;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameCam>()
            .init_resource::<ViewCams>()
            .init_resource::<RotatingView>()
            .init_resource::<ViewAngles>()
            .init_resource::<MinimapYaw>()
            .init_resource::<ViewEyes>()
            .init_resource::<TvCamera>()
            .add_plugins(postfx::PostFxPlugin)
            .add_systems(PreUpdate, follow_settings)
            .add_systems(Update, view_size_keys.run_if(in_state(AppState::Playing)))
            .add_systems(Startup, spawn_camera)
            .add_systems(
                OnEnter(AppState::Playing),
                |mut commands: Commands,
                 mut cam: ResMut<GameCam>,
                 mut views: ResMut<ViewCams>,
                 mut angles: ResMut<ViewAngles>,
                 mut eyes: ResMut<ViewEyes>,
                 mut tv: ResMut<TvCamera>,
                 mode: Res<GameMode>,
                 primary: Query<Entity, With<GameCamera>>| {
                    *cam = GameCam::default();
                    views.0.clear();
                    angles.0 = vec![ViewAngle::default(); mode.players().max(1)];
                    eyes.follow = vec![GameCam::default(); mode.players().max(1)];
                    eyes.cams = eyes.follow.clone();
                    eyes.prev = eyes.follow.clone();
                    eyes.tv = false;
                    *tv = TvCamera::default();
                    if mode.is_multiplayer() {
                        views.0 = vec![GameCam::default(); mode.players()];
                        spawn_views(&mut commands, mode.players(), primary.single().ok());
                    }
                },
            )
            .add_systems(
                OnExit(AppState::Playing),
                |mut commands: Commands, mut views: ResMut<ViewCams>, primary: Query<Entity, With<GameCamera>>| {
                    views.0.clear();
                    // The UI goes back to the game camera.
                    if let Ok(e) = primary.single() {
                        commands.entity(e).insert(IsDefaultUiCamera);
                    }
                },
            )
            // The camera moves after the game tick (the original calls
            // `FUN_010556e0` from the render loop), so every system that reads
            // it sees the same state: an unordered system here made the
            // spawn windows, and with them replays, depend on thread timing.
            .add_systems(
                FixedUpdate,
                follow_tick
                    .after(crate::sim::people::PeopleSet)
                    .after(crate::sim::traffic::TrafficSet)
                    .after(crate::sim::driving::DrivingSet)
                    .after(crate::sim::combat::CombatSet)
                    .after(crate::sim::vehicle_ai::VehicleAiSet),
            )
            .add_systems(
                PostUpdate,
                (
                    toggle_mode,
                    turn_views,
                    tv_camera,
                    place_game_camera,
                    place_extra_views,
                    free_controls,
                    solo_background,
                    layout_views,
                    separators,
                )
                    .chain()
                    // `tv_camera` blends the eyes by this frame's
                    // `Smooth::alpha`, set in `SmoothSet`: read before it,
                    // last frame's value put the camera out of step with
                    // the objects (shaking while driving).
                    .after(crate::frame::SmoothSet)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn arg_f32(name: &str) -> Option<f32> {
    crate::args()
        .skip_while(|a| a != name)
        .nth(1)
        .and_then(|v| v.parse().ok())
}

/// The game camera's projection: vertical FOV of the 320x256 original,
/// 2·atan(128/256) = 53.1°.
fn projection() -> Projection {
    Projection::Perspective(PerspectiveProjection {
        fov: view_fov(1, false),
        near: 0.05,
        far: 400.0,
        ..default()
    })
}

/// Vertical field of view of a view, from the screen constants of
/// `FUN_0104107c` / `FUN_0105bd98` (rendering.md §6): `2·atan(cy / f)` with
/// focal length `f = 16·fy` pixels top-down and `8·fy` in the rotating
/// view (`FUN_01091f84`). One view: cy 128, fy 16 (53.1°; rotating 90°).
/// Two (side by side, 159×256): cy 128, fy = 264·16/384 = 11 (72.1°;
/// rotating 111.0°). Three or four (159×127): cy 64, fy 8 (53.1°; 90°).
pub fn view_fov(players: usize, rotating: bool) -> f32 {
    let (cy, fy) = match players {
        0 | 1 => (128.0, 16.0),
        2 => (128.0, 11.0),
        _ => (64.0, 8.0),
    };
    let focal: f32 = if rotating { 8.0 * fy } else { 16.0 * fy };
    2.0 * (cy / focal).atan()
}

/// [`view_fov`] for a window of `size` physical pixels. With `original`
/// (`Settings::original_aspect`; off by default, a port choice) the horizontal field of view
/// is the original's at any size: `ReadScreenParams__Fv` @ 10051fc0 (OS4
/// `FUN_0104107c`) scales fx = (W+8)·16/320 with the width, so one view
/// always spans 2·atan(160/256) ≈ 64° (two side by side: fx =
/// ((W/2)+7)·16/240, 2·atan(80/176) ≈ 49°), and the vertical angle follows
/// the view's aspect (square pixels; the Mac at 640×480 instead stretches
/// the 10:8 picture 6.7 % wide). Without it the vertical angle is fixed
/// and the view widens with the window.
pub fn view_fov_for(players: usize, rotating: bool, original: bool, size: UVec2) -> f32 {
    let v = view_fov(players, rotating);
    if !original {
        return v;
    }
    // View aspect at 320×256 and in this window.
    let (w, h) = (size.x.max(1) as f32, size.y.max(1) as f32);
    let (a0, a) = match players {
        2 => (160.0 / 256.0, w / 2.0 / h),
        _ => (320.0 / 256.0, w / h),
    };
    2.0 * ((v / 2.0).tan() * a0 / a).atan()
}

/// Split screen: view cameras for players 2..n and the UI overlay, which
/// becomes the default UI camera (the HUD spans the whole window).
fn spawn_views(commands: &mut Commands, n: usize, primary: Option<Entity>) {
    for p in 1..n {
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: -10 + p as isize,
                ..default()
            },
            projection(),
            Transform::from_xyz(64.0, 30.0, 64.0).looking_at(Vec3::new(64.0, 0.0, 64.0), Vec3::NEG_Z),
            ExtraView(p as u8),
            RenderLayers::from_layers(&[0, view_layer(p)]),
            LevelEntity,
        ));
    }
    // After the views, before the menu camera (order 1).
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(EMPTY_LAYER),
        OverlayCamera,
        IsDefaultUiCamera,
        LevelEntity,
    ));
    if let Some(e) = primary {
        commands.entity(e).remove::<IsDefaultUiCamera>();
    }
}

fn spawn_camera(mut commands: Commands) {
    let free_requested = crate::args().any(|a| a == "--free");
    let free = FreeRig {
        focus: Vec3::new(
            arg_f32("--x").unwrap_or(64.0),
            1.0,
            arg_f32("--y").unwrap_or(64.0),
        ),
        distance: arg_f32("--dist").unwrap_or(30.0),
        yaw: 0.0,
        pitch: arg_f32("--pitch").unwrap_or(60.0).to_radians(),
    };
    commands.spawn((
        Camera3d::default(),
        projection(),
        free.transform(),
        RenderLayers::from_layers(&[0, view_layer(0)]),
        GameCamera {
            mode: if free_requested {
                CameraMode::Free
            } else {
                CameraMode::Follow
            },
            free,
        },
    ));
}

fn toggle_mode(keys: Res<ButtonInput<KeyCode>>, mut cam: Single<&mut GameCamera>) {
    if keys.just_pressed(KeyCode::F6) {
        cam.mode = match cam.mode {
            CameraMode::Follow => CameraMode::Free,
            CameraMode::Free => CameraMode::Follow,
        };
    }
}

/// One game tick of the original camera motion (every player's view in
/// split screen; a dead player's view stays put until the respawn): the
/// simulation's cameras with the top-down parameters, and the views' eyes
/// ([`ViewEyes`]) with the rotating-view ones when that is on.
fn follow_tick(
    trig: Res<Trig>,
    rotating: Res<RotatingView>,
    mut cam: ResMut<GameCam>,
    mut views: ResMut<ViewCams>,
    mut eyes: ResMut<ViewEyes>,
    target: Query<(&Obj, Has<Vehicle>), With<CameraTarget>>,
    rc: Option<Res<crate::sim::rc_car::RcCars>>,
    objs: Query<&Obj>,
) {
    let n = views.0.len().max(1);
    if eyes.follow.len() != n {
        eyes.follow = vec![GameCam::default(); n];
    }
    let eyes = &mut *eyes;
    eyes.prev.clone_from(&eyes.follow);
    // A player with an RC car out watches it; after it blew up the view
    // holds still for 60 ticks (Mac `UpdateMapPosition` @ 10068cb0,
    // `_DAT_1017c484` / `_DAT_1017c364`; `sim::rc_car`).
    let watched = |obj: &'_ Obj, driving: bool| -> Option<(Obj, Mode)> {
        let p = obj.player.unwrap_or(0) as usize;
        let rc = rc.as_deref();
        if rc.is_some_and(|r| r.holding(p)) {
            return None;
        }
        match rc.and_then(|r| r.follow(p)).and_then(|e| objs.get(e).ok()) {
            Some(car) => Some((car.clone(), Mode::Rc)),
            None => Some((obj.clone(), if driving { Mode::Driving } else { Mode::OnFoot })),
        }
    };
    if views.0.is_empty() {
        let Ok((obj, driving)) = target.single() else {
            return;
        };
        let Some((obj, mode)) = watched(obj, driving) else { return };
        follow(&trig, &mut cam, &obj, mode, false);
        follow(&trig, &mut eyes.follow[0], &obj, mode, rotating.0);
        return;
    }
    for (obj, driving) in &target {
        let Some(p) = obj.player.map(|p| p as usize).filter(|&p| p < n) else { continue };
        let Some((obj, mode)) = watched(obj, driving) else { continue };
        follow(&trig, &mut views.0[p], &obj, mode, false);
        follow(&trig, &mut eyes.follow[p], &obj, mode, rotating.0);
    }
    *cam = views.0[0];
}

/// What the camera follows.
#[derive(Clone, Copy)]
enum Mode {
    OnFoot,
    Driving,
    /// The player's RC car (`sim::rc_car`).
    Rc,
}

fn follow(trig: &Trig, cam: &mut GameCam, obj: &Obj, mode: Mode, rotating: bool) {
    let f = match (mode, rotating) {
        (Mode::OnFoot, false) => ON_FOOT,
        (Mode::Driving, false) => DRIVING,
        (Mode::Rc, false) => RC,
        (Mode::OnFoot, true) => ON_FOOT_ROTATING,
        (Mode::Driving | Mode::Rc, true) => DRIVING_ROTATING,
    };
    let lead = |v: i32| (v as i64 * f.v).clamp(-LEAD_CAP, LEAD_CAP);
    let tx = obj.pos.x as i64 * 32 + f.k * trig.sin(obj.heading) as i64 + lead(obj.vel.x);
    let ty = obj.pos.y as i64 * 32 + f.k * trig.cos(obj.heading) as i64 + lead(obj.vel.y);
    let t = (f.c * obj.speed).abs() + f.b - (obj.pos.z >> 4);
    if !cam.initialised {
        cam.x = tx;
        cam.y = ty;
        cam.initialised = true;
    }
    cam.x = (4 * cam.x + tx) / 5;
    cam.y = (4 * cam.y + ty) / 5;
    cam.h = match f.r {
        None => (2 * cam.h + t.clamp(1, 3000)) / 3,
        Some(r) => (2 * cam.h + t.min(3000) - r) / 3,
    };
}

/// Rotating view, once per frame per view (`FUN_0105bd98`): the yaw moves
/// half-way towards `(0xb40 − heading) mod 0x1680` the short way round and
/// the pitch half-way towards `clamp(0x167f − speed, 0x157c, 0x167f)` (the
/// followed object: the driven vehicle, else the player). While the player
/// is dead the yaw turns `0x20` and the pitch drops `4` per tick down to
/// 0x157c. The original halves once per rendered frame (about one tick);
/// here the step is scaled to the 30 Hz tick so the motion doesn't depend on
/// the frame rate.
///
/// Also keeps [`MinimapYaw`] (port-added): view 0's yaw in the rotating
/// view, else the yaw the rotating view would have, eased the same way.
fn turn_views(
    time: Res<Time>,
    rotating: Res<RotatingView>,
    mut angles: ResMut<ViewAngles>,
    mut minimap: ResMut<MinimapYaw>,
    target: Query<&Obj, With<CameraTarget>>,
) {
    if angles.0.is_empty() {
        angles.0.push(ViewAngle::default());
    }
    let ticks = time.delta_secs() * 30.0;
    // Fraction of the remaining difference closed this frame (1/2 per tick).
    let step = 1.0 - 0.5f32.powf(ticks);
    let single = angles.0.len() == 1;
    if !rotating.0 {
        let obj = target.iter().find(|o| single || o.player == Some(0));
        minimap.0 = ease_yaw(minimap.0, obj, step, ticks);
        return;
    }
    for (p, a) in angles.0.iter_mut().enumerate() {
        let obj = target.iter().find(|o| single || o.player == Some(p as u8));
        a.yaw = ease_yaw(a.yaw, obj, step, ticks);
        match obj {
            Some(obj) => {
                let want_pitch = (PITCH_DOWN - obj.speed).clamp(PITCH_TILTED, PITCH_DOWN) as f32;
                a.pitch += (want_pitch - a.pitch) * step;
            }
            None => a.pitch = (a.pitch - 4.0 * ticks).max(PITCH_TILTED as f32),
        }
    }
    minimap.0 = angles.0[0].yaw;
}

/// One frame of the rotating view's yaw ([`turn_views`]): `step` of the
/// way towards `0xb40 − heading` of the followed object, or `0x20` per
/// tick while there is none.
fn ease_yaw(yaw: f32, obj: Option<&Obj>, step: f32, ticks: f32) -> f32 {
    let turn = TURN as f32;
    match obj {
        Some(obj) => {
            let want = (0xb40 - obj.heading).rem_euclid(TURN) as f32;
            let d = (want - yaw + turn / 2.0).rem_euclid(turn) - turn / 2.0;
            (yaw + d * step).rem_euclid(turn)
        }
        None => (yaw + 0x20 as f32 * ticks).rem_euclid(turn),
    }
}

/// Camera orientation of a view: straight down with north up, or in the
/// rotating view with the heading up and tilted forward by
/// `0x1680 − pitch` units (rendering.md §6, `FUN_01091f84`).
fn view_transform(pos: Vec3, angle: Option<ViewAngle>) -> Transform {
    let Some(a) = angle else {
        return Transform::from_translation(pos).looking_at(pos - Vec3::Y, Vec3::NEG_Z);
    };
    // yaw = 0xb40 − heading: the heading (sin h, cos h) on the map points
    // up the screen.
    let h = (0xb40 as f32 - a.yaw) * std::f32::consts::TAU / TURN as f32;
    let ahead = Vec3::new(h.sin(), 0.0, h.cos());
    let tilt = (TURN as f32 - a.pitch) * std::f32::consts::TAU / TURN as f32;
    let look = Vec3::NEG_Y * tilt.cos() + ahead * tilt.sin();
    Transform::from_translation(pos).looking_to(look, ahead)
}

/// A camera state `t` of the way from `a` to `b` (smooth motion).
fn between(a: &GameCam, b: &GameCam, t: f32) -> GameCam {
    let t = t as f64;
    let mix = |a: f64, b: f64| (a + (b - a) * t).round();
    GameCam {
        x: mix(a.x as f64, b.x as f64) as i64,
        y: mix(a.y as f64, b.y as f64) as i64,
        h: mix(a.h as f64, b.h as f64) as i32,
        initialised: b.initialised,
    }
}

/// Render position of a camera state.
fn eye(game: &GameCam) -> Vec3 {
    Vec3::new(
        game.x as f32 / 65536.0,
        (game.h + 1500) as f32 / H_PER_SQUARE,
        game.y as f32 / 65536.0,
    )
}

/// Game ticks per second, for the per-frame easing.
const TICK_HZ: f32 = crate::mission::LevelClock::TICKS_PER_SECOND as f32;

/// `0x167f − GetAngle(dx, dy)` (`GetAngle__Fdd` @ 10072340: 0 along −y,
/// 0xb40 along +y, 0x10e0 along +x, i.e. heading + 0xb40; 0 for (0, 0)):
/// the yaw that puts the direction (dx, dy) up the screen.
fn yaw_towards(dx: f64, dy: f64) -> f32 {
    let get_angle = if dx == 0.0 && dy == 0.0 {
        0.0
    } else {
        let heading = dx.atan2(dy) / std::f64::consts::TAU * TURN as f64;
        (heading.round() + 0xb40 as f64).rem_euclid(TURN as f64)
    };
    (0x167f as f64 - get_angle) as f32
}

/// The eyes of this frame ([`ViewEyes`]) and the replay TV camera
/// (`DrawMap` @ 1005a260 prologue, active when the rotating view is on, a
/// replay plays back and there is one player). Each frame, as the
/// original's per-frame steps scaled to game ticks:
/// * the camera moves by its velocity·ticks, clamped to the map
///   (0..0x7fffff);
/// * its yaw eases a quarter of the way towards `0x167f − GetAngle(target −
///   camera)`, the target being the followed object (the view's own
///   camera while the player is dead); the pitch is fixed at 0x14b4;
/// * it is drawn at height `1000 − (z >> 4)` with the visible range 3
///   squares larger (`+4` instead of `+1`);
/// * when it is more than a quarter of that range (in whole squares) from
///   the view's followed camera, it re-spawns: `random3(4)` picks the new
///   velocity (3: random ±0x400 per axis; 2: `vel·64/3` of the followed
///   object; else both), the position becomes the followed camera plus
///   `vel·0x800 + random3(range << 13) − range·0x1000`, clamped to
///   `[(−range) >> 2, range >> 2]` (16.16 units, as in the Mac code: the
///   re-spawn is in effect at the followed camera), and the yaw snaps to the
///   target.
///
/// The random numbers come from the cosmetic generator
/// ([`crate::sim::VisualRng`], `random3__Fi`), never the simulation's.
/// `GetAngle` works on doubles; [`yaw_towards`] rounds the angle.
#[allow(clippy::too_many_arguments)]
fn tv_camera(
    time: Res<Time>,
    smooth: Res<crate::frame::Smooth>,
    rotating: Res<RotatingView>,
    mode: Res<GameMode>,
    replay: Option<Res<crate::replay::ReplayState>>,
    rng: Option<ResMut<crate::sim::VisualRng>>,
    mut tv: ResMut<TvCamera>,
    mut eyes: ResMut<ViewEyes>,
    target: Query<&Obj, With<CameraTarget>>,
) {
    let eyes = &mut *eyes;
    eyes.cams.clone_from(&eyes.follow);
    // Smooth motion: between the last two ticks' cameras (`crate::frame`).
    if smooth.alpha < 1.0 && eyes.prev.len() == eyes.cams.len() {
        for (c, p) in eyes.cams.iter_mut().zip(&eyes.prev) {
            if p.initialised {
                *c = between(p, c, smooth.alpha);
            }
        }
    }
    if eyes.cams.is_empty() {
        eyes.cams.push(GameCam::default());
    }
    let playing = replay.is_some_and(|r| r.playing());
    tv.active = rotating.0 && playing && !mode.is_multiplayer() && rng.is_some();
    eyes.tv = tv.active;
    let (Some(mut rng), true) = (rng, tv.active) else { return };
    let tv = &mut *tv;
    let base = eyes.cams[0];
    let ticks = (time.delta_secs() * TICK_HZ) as f64;
    const MAX: f64 = 0x7fffff as f64;
    tv.x = (tv.x + tv.vx as f64 * ticks).clamp(0.0, MAX);
    tv.y = (tv.y + tv.vy as f64 * ticks).clamp(0.0, MAX);
    let obj = target.iter().next();
    let look_at = match obj {
        Some(o) => (o.pos.x as f64 * 32.0, o.pos.y as f64 * 32.0),
        None => (base.x as f64, base.y as f64),
    };
    let turn = TURN as f32;
    let want = yaw_towards(look_at.0 - tv.x, look_at.1 - tv.y);
    let d = (want - tv.yaw + turn / 2.0).rem_euclid(turn) - turn / 2.0;
    tv.yaw = (tv.yaw + d * (1.0 - 0.75f32.powf(ticks as f32))).rem_euclid(turn);
    let h = 1000 - (obj.map_or(0, |o| o.pos.z) >> 4);
    let probe = GameCam {
        h,
        ..base
    };
    let range = probe.visible_squares(1, true).0 + 3;
    let quarter = range >> 2;
    let far = |a: f64, b: i64| ((a as i64 - b).abs() >> 16) as i32 > quarter;
    if far(tv.x, base.x) || far(tv.y, base.y) {
        let mut rnd = |n: i32| rng.rand(n);
        let vel = obj.map_or(IVec3::ZERO, |o| o.vel);
        match rnd(4) {
            3 => {
                tv.vx = rnd(0x800) - 0x400;
                tv.vy = rnd(0x800) - 0x400;
            }
            2 => {
                tv.vx = vel.x * 64 / 3;
                tv.vy = vel.y * 64 / 3;
            }
            _ => {
                tv.vx = vel.x * 64 / 3 + rnd(0x800) - 0x400;
                tv.vy = vel.y * 64 / 3 + rnd(0x800) - 0x400;
            }
        }
        let ox = (vel.x * 0x800 + rnd(range << 13) - range * 0x1000).min(quarter).max(-range >> 2);
        let oy = (vel.y * 0x800 + rnd(range << 13) - range * 0x1000).min(quarter).max(-range >> 2);
        tv.x = (base.x + ox as i64).clamp(0, 0x7fffff) as f64;
        tv.y = (base.y + oy as i64).clamp(0, 0x7fffff) as f64;
        tv.yaw = yaw_towards(look_at.0 - tv.x, look_at.1 - tv.y).rem_euclid(turn);
    }
    eyes.cams[0] = GameCam {
        x: tv.x as i64,
        y: tv.y as i64,
        h,
        initialised: true,
    };
}

fn place_game_camera(
    eyes: Res<ViewEyes>,
    rotating: Res<RotatingView>,
    angles: Res<ViewAngles>,
    tv: Res<TvCamera>,
    cam: Single<(&mut GameCamera, &mut Transform)>,
) {
    let (mut cam, mut transform) = cam.into_inner();
    let pos = eye(eyes.cams.first().unwrap_or(&GameCam::default()));
    if cam.mode == CameraMode::Follow {
        let angle = if tv.active {
            Some(ViewAngle {
                yaw: tv.yaw,
                pitch: TV_PITCH as f32,
            })
        } else {
            rotating.0.then(|| angles.0.first().copied().unwrap_or_default())
        };
        *transform = view_transform(pos, angle);
        cam.free.focus = Vec3::new(pos.x, 1.0, pos.z);
    }
}

fn place_extra_views(
    eyes: Res<ViewEyes>,
    rotating: Res<RotatingView>,
    angles: Res<ViewAngles>,
    mut q: Query<(&ExtraView, &mut Transform)>,
) {
    for (v, mut transform) in &mut q {
        if let Some(game) = eyes.cams.get(v.0 as usize) {
            let angle = rotating
                .0
                .then(|| angles.0.get(v.0 as usize).copied().unwrap_or_default());
            *transform = view_transform(eye(game), angle);
        }
    }
}

/// The camera behind one player's view when it does not fill the window
/// (view size below 0x100, the level-start zoom): it clears the window
/// black and shows the zoom picture (`hud::zoom::LAYER`). The UI stays on
/// the game camera, laid out in its viewport, so the HUD follows the view
/// and its subtract panels still act on the view's own target.
#[derive(Component)]
struct SoloBackground;

/// Spawn or drop [`SoloBackground`].
#[allow(clippy::too_many_arguments)]
fn solo_background(
    mut commands: Commands,
    mode: Res<GameMode>,
    state: Res<State<AppState>>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    zoom: Res<crate::hud::zoom::ZoomIn>,
    primary: Query<&GameCamera>,
    existing: Query<Entity, With<SoloBackground>>,
) {
    let Ok(game) = primary.single() else { return };
    let free = game.mode == CameraMode::Free;
    let want = *state.get() == AppState::Playing
        && !mode.is_multiplayer()
        && !free
        && (view_size(settings.as_deref(), mode.players()) < 0x100 || zoom.zoom.is_some());
    match (want, existing.single().ok()) {
        (true, None) => {
            commands.spawn((
                Camera2d,
                Camera {
                    order: -20,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..default()
                },
                RenderLayers::layer(crate::hud::zoom::LAYER),
                SoloBackground,
                LevelEntity,
            ));
        }
        (false, Some(e)) => {
            commands.entity(e).despawn();
        }
        _ => {}
    }
}

/// Black window areas between split-screen views (the separators, and the
/// sides of the centred third view), drawn on the overlay.
#[derive(Component)]
struct Separator;

/// Window areas no view covers, as fractions (x, y, w, h).
fn gaps(n: usize, w: u32, h: u32) -> Vec<(f32, f32, f32, f32)> {
    let rects = view_rects(n, w, h);
    if rects.len() < 2 {
        return Vec::new();
    }
    gaps_around(&rects, w, h)
}

/// Window areas none of `rects` covers, as fractions (x, y, w, h).
fn gaps_around(rects: &[(u32, u32, u32, u32)], w: u32, h: u32) -> Vec<(f32, f32, f32, f32)> {
    let (wf, hf) = (w as f32, h as f32);
    // Scan in bands between the views' edges; anything uncovered is a gap.
    let mut ys: Vec<u32> = rects.iter().flat_map(|r| [r.1, r.1 + r.3]).chain([0, h]).collect();
    ys.sort();
    ys.dedup();
    let mut out = Vec::new();
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        let mut xs: Vec<(u32, u32)> = rects
            .iter()
            .filter(|r| r.1 < y1 && r.1 + r.3 > y0)
            .map(|r| (r.0, r.0 + r.2))
            .collect();
        xs.sort();
        let mut x = 0;
        for (a, b) in xs.into_iter().chain([(w, w)]) {
            if a > x {
                out.push((x as f32 / wf, y0 as f32 / hf, (a - x) as f32 / wf, (y1 - y0) as f32 / hf));
            }
            x = x.max(b);
        }
    }
    out
}

/// Split-screen viewports from the window size; the whole window (order
/// 0) otherwise. The first view clears the window to the sky colour (as in
/// single player) and the separators are black overlay rectangles.
#[allow(clippy::type_complexity)]
fn layout_views(
    mode: Res<GameMode>,
    state: Res<State<AppState>>,
    rotating: Res<RotatingView>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    zoom: Res<crate::hud::zoom::ZoomIn>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut primary: Query<(&mut Camera, &mut Projection, &GameCamera), Without<ExtraView>>,
    mut extras: Query<(&ExtraView, &mut Camera, &mut Projection), Without<GameCamera>>,
) {
    let Ok((mut cam, mut projection, game_camera)) = primary.single_mut() else { return };
    let split = mode.is_multiplayer() && *state.get() == AppState::Playing;
    // Field of view of the original's screen constants for this layout.
    let free = game_camera.mode == CameraMode::Free;
    let size = window.single().map(|w| w.physical_size()).unwrap_or(UVec2::new(1280, 800));
    let original = settings.as_ref().is_none_or(|s| s.original_aspect);
    let fov = view_fov_for(if split { mode.players() } else { 1 }, rotating.0 && !free, original, size);
    let set_fov = |p: &mut Projection| {
        if let Projection::Perspective(pp) = p
            && pp.fov != fov
        {
            pp.fov = fov;
        }
    };
    set_fov(&mut projection);
    for (_, _, mut p) in &mut extras {
        set_fov(&mut p);
    }
    let rects = view_rects(mode.players(), size.x.max(2), size.y.max(2));
    let viewport = |i: usize| {
        rects.get(i).map(|&(x, y, w, h)| Viewport {
            physical_position: UVec2::new(x, y),
            physical_size: UVec2::new(w.max(1), h.max(1)),
            ..default()
        })
    };
    let sized = view_size(settings.as_deref(), mode.players());
    let zoom_rect = zoom.zoom.as_ref().map(|z| z.view_rect());
    let (order, clear, vp) = if split {
        (-10, ClearColorConfig::Default, viewport(0))
    } else if let Some(r) = zoom_rect.filter(|_| *state.get() == AppState::Playing && !free) {
        // The level-start zoom: the view shrunk to the genlocked frame
        // (`crate::hud::zoom`).
        let (w, h) = (size.x.max(2) as f32, size.y.max(2) as f32);
        let x0 = (r.min.x * w).clamp(0.0, w - 1.0);
        let y0 = (r.min.y * h).clamp(0.0, h - 1.0);
        let x1 = (r.max.x * w).clamp(x0 + 1.0, w);
        let y1 = (r.max.y * h).clamp(y0 + 1.0, h);
        let vp = Viewport {
            physical_position: UVec2::new(x0 as u32, y0 as u32),
            physical_size: UVec2::new((x1 - x0) as u32, (y1 - y0) as u32),
            ..default()
        };
        (-10, ClearColorConfig::Default, Some(vp))
    } else if sized < 0x100 && *state.get() == AppState::Playing && !free {
        let (x, y, w, h) = sized_view_rect(size.x.max(2), size.y.max(2), sized);
        let vp = Viewport {
            physical_position: UVec2::new(x, y),
            physical_size: UVec2::new(w, h),
            ..default()
        };
        (-10, ClearColorConfig::Default, Some(vp))
    } else {
        (0, ClearColorConfig::Default, None)
    };
    if cam.order != order {
        cam.order = order;
    }
    let same_clear = match (&cam.clear_color, &clear) {
        (ClearColorConfig::Default, ClearColorConfig::Default) | (ClearColorConfig::None, ClearColorConfig::None) => true,
        (ClearColorConfig::Custom(a), ClearColorConfig::Custom(b)) => a == b,
        _ => false,
    };
    if !same_clear {
        cam.clear_color = clear;
    }
    let key = |v: &Option<Viewport>| v.as_ref().map(|v| (v.physical_position, v.physical_size));
    if key(&cam.viewport) != key(&vp) {
        cam.viewport = vp;
    }
    for (v, mut cam, _) in &mut extras {
        let vp = viewport(v.0 as usize);
        if key(&cam.viewport) != key(&vp) {
            cam.viewport = vp;
        }
    }
}

fn free_controls(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    cam: Single<(&mut GameCamera, &mut Transform)>,
) {
    let (mut cam, mut transform) = cam.into_inner();
    if cam.mode != CameraMode::Free {
        return;
    }
    let rig = &mut cam.free;
    let dt = time.delta_secs();
    let pressed = |k: &[KeyCode]| keys.any_pressed(k.iter().copied());

    let forward = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
    let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
    let mut pan = Vec3::ZERO;
    if pressed(&[KeyCode::KeyW]) {
        pan += forward;
    }
    if pressed(&[KeyCode::KeyS]) {
        pan -= forward;
    }
    if pressed(&[KeyCode::KeyD]) {
        pan += right;
    }
    if pressed(&[KeyCode::KeyA]) {
        pan -= right;
    }
    rig.focus += pan * rig.distance * dt;
    if pressed(&[KeyCode::KeyQ]) {
        rig.yaw += 1.5 * dt;
    }
    if pressed(&[KeyCode::KeyE]) {
        rig.yaw -= 1.5 * dt;
    }
    if pressed(&[KeyCode::KeyR]) {
        rig.pitch = (rig.pitch + dt).min(1.55);
    }
    if pressed(&[KeyCode::KeyF]) {
        rig.pitch = (rig.pitch - dt).max(0.1);
    }
    let mut zoom = -scroll.delta.y * 0.1;
    if pressed(&[KeyCode::KeyZ]) {
        zoom -= dt;
    }
    if pressed(&[KeyCode::KeyX]) {
        zoom += dt;
    }
    rig.distance = (rig.distance * (1.0 + zoom)).clamp(2.0, 200.0);
    *transform = rig.transform();
}

/// Keep the black separator rectangles in step with the split-screen
/// layout (none outside multiplayer; a shrunk single view has the black
/// [`SoloBackground`] around it).
fn separators(
    mut commands: Commands,
    mode: Res<GameMode>,
    state: Res<State<AppState>>,
    window: Query<&Window, With<PrimaryWindow>>,
    existing: Query<Entity, With<Separator>>,
    mut last: Local<Vec<(f32, f32, f32, f32)>>,
) {
    let split = mode.is_multiplayer() && *state.get() == AppState::Playing;
    let size = window.single().map(|w| w.physical_size()).unwrap_or(UVec2::new(1280, 800));
    let (w, h) = (size.x.max(2), size.y.max(2));
    let want = if split { gaps(mode.players(), w, h) } else { Vec::new() };
    if *last == want && existing.iter().count() == want.len() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    for &(x, y, w, h) in &want {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(x * 100.0),
                top: Val::Percent(y * 100.0),
                width: Val::Percent(w * 100.0),
                height: Val::Percent(h * 100.0),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(-1),
            Separator,
            LevelEntity,
        ));
    }
    *last = want;
}
