//! Cameras: the game's overhead camera (docs/re/rendering.md §6, OS4
//! `FUN_010556e0`) and a free debug camera (toggle with F2).
//!
//! Split screen (multiplayer, `FUN_0105bd98`): every player has a view
//! with its own camera state ([`ViewCams`]). Player 1 keeps the
//! [`GameCamera`]; players 2..4 get [`ExtraView`] cameras, each drawn into
//! its viewport; a full-window [`OverlayCamera`] draws the HUD on top.

use crate::level::{AppState, GameMode, LevelEntity};
use crate::sim::{Obj, Trig};
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
}

/// Split screen: each player's camera state, by player number (empty
/// outside multiplayer; [`GameCam`] then holds player 1's copy).
#[derive(Resource, Default, Clone)]
pub struct ViewCams(pub Vec<GameCam>);

/// The split-screen view camera of player `.0` (1..3; player 0 uses the
/// [`GameCamera`]).
#[derive(Component)]
pub struct ExtraView(pub u8);

/// Full-window camera drawing the UI over the split-screen views (it
/// renders no 3D: its only render layer is empty).
#[derive(Component)]
pub struct OverlayCamera;

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

/// Follow parameters (rendering.md §6, top-down view): look-ahead K,
/// velocity lead V, base height B, speed factor c.
struct Follow {
    k: i64,
    v: i64,
    b: i32,
    c: i32,
}

const ON_FOOT: Follow = Follow {
    k: 100,
    v: 0x140,
    b: 1000,
    c: 3,
};
const DRIVING: Follow = Follow {
    k: 150,
    v: 200,
    b: 800,
    c: 3,
};
const LEAD_CAP: i64 = 0x2af80;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameCam>()
            .init_resource::<ViewCams>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                OnEnter(AppState::Playing),
                |mut commands: Commands,
                 mut cam: ResMut<GameCam>,
                 mut views: ResMut<ViewCams>,
                 mode: Res<GameMode>,
                 primary: Query<Entity, With<GameCamera>>| {
                    *cam = GameCam::default();
                    views.0.clear();
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
            .add_systems(FixedUpdate, follow_tick)
            .add_systems(
                PostUpdate,
                (toggle_mode, place_game_camera, place_extra_views, free_controls, layout_views, separators)
                    .chain()
                    .before(TransformSystems::Propagate),
            );
    }
}

fn arg_f32(name: &str) -> Option<f32> {
    std::env::args()
        .skip_while(|a| a != name)
        .nth(1)
        .and_then(|v| v.parse().ok())
}

/// The game camera's projection: vertical FOV of the 320x256 original,
/// 2·atan(128/256) = 53.1°.
fn projection() -> Projection {
    Projection::Perspective(PerspectiveProjection {
        fov: 2.0 * (128.0f32 / 256.0).atan(),
        near: 0.05,
        far: 400.0,
        ..default()
    })
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
    let free_requested = std::env::args().any(|a| a == "--free");
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
    if keys.just_pressed(KeyCode::F2) {
        cam.mode = match cam.mode {
            CameraMode::Follow => CameraMode::Free,
            CameraMode::Free => CameraMode::Follow,
        };
    }
}

/// One game tick of the original camera motion (every player's view in
/// split screen; a dead player's view stays put until the respawn).
fn follow_tick(
    trig: Res<Trig>,
    mut cam: ResMut<GameCam>,
    mut views: ResMut<ViewCams>,
    target: Query<(&Obj, Has<Vehicle>), With<CameraTarget>>,
) {
    if views.0.is_empty() {
        let Ok((obj, driving)) = target.single() else {
            return;
        };
        follow(&trig, &mut cam, obj, driving);
        return;
    }
    for (obj, driving) in &target {
        if let Some(v) = obj.player.and_then(|p| views.0.get_mut(p as usize)) {
            follow(&trig, v, obj, driving);
        }
    }
    *cam = views.0[0];
}

fn follow(trig: &Trig, cam: &mut GameCam, obj: &Obj, driving: bool) {
    let f = if driving { DRIVING } else { ON_FOOT };
    let lead = |v: i32| (v as i64 * f.v).clamp(-LEAD_CAP, LEAD_CAP);
    let tx = obj.pos.x as i64 * 32 + f.k * trig.sin(obj.heading) as i64 + lead(obj.vel.x);
    let ty = obj.pos.y as i64 * 32 + f.k * trig.cos(obj.heading) as i64 + lead(obj.vel.y);
    let t = ((f.c * obj.speed).abs() + f.b - (obj.pos.z >> 4)).clamp(1, 3000);
    if !cam.initialised {
        cam.x = tx;
        cam.y = ty;
        cam.initialised = true;
    }
    cam.x = (4 * cam.x + tx) / 5;
    cam.y = (4 * cam.y + ty) / 5;
    cam.h = (2 * cam.h + t) / 3;
}

/// Render position of a camera state.
fn eye(game: &GameCam) -> Vec3 {
    Vec3::new(
        game.x as f32 / 65536.0,
        (game.h + 1500) as f32 / H_PER_SQUARE,
        game.y as f32 / 65536.0,
    )
}

fn place_game_camera(game: Res<GameCam>, cam: Single<(&mut GameCamera, &mut Transform)>) {
    let (mut cam, mut transform) = cam.into_inner();
    let pos = eye(&game);
    if cam.mode == CameraMode::Follow {
        // Straight down, north (-y) at the top of the screen.
        *transform = Transform::from_translation(pos).looking_at(pos - Vec3::Y, Vec3::NEG_Z);
        cam.free.focus = Vec3::new(pos.x, 1.0, pos.z);
    }
}

fn place_extra_views(views: Res<ViewCams>, mut q: Query<(&ExtraView, &mut Transform)>) {
    for (v, mut transform) in &mut q {
        if let Some(game) = views.0.get(v.0 as usize) {
            let pos = eye(game);
            *transform = Transform::from_translation(pos).looking_at(pos - Vec3::Y, Vec3::NEG_Z);
        }
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
fn layout_views(
    mode: Res<GameMode>,
    state: Res<State<AppState>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut primary: Query<&mut Camera, (With<GameCamera>, Without<ExtraView>)>,
    mut extras: Query<(&ExtraView, &mut Camera), Without<GameCamera>>,
) {
    let Ok(mut cam) = primary.single_mut() else { return };
    let split = mode.is_multiplayer() && *state.get() == AppState::Playing;
    let size = window.single().map(|w| w.physical_size()).unwrap_or(UVec2::new(1280, 800));
    let rects = view_rects(mode.players(), size.x.max(2), size.y.max(2));
    let viewport = |i: usize| {
        rects.get(i).map(|&(x, y, w, h)| Viewport {
            physical_position: UVec2::new(x, y),
            physical_size: UVec2::new(w.max(1), h.max(1)),
            ..default()
        })
    };
    let (order, clear, vp) = if split {
        (-10, ClearColorConfig::Default, viewport(0))
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
    for (v, mut cam) in &mut extras {
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
/// layout (none outside multiplayer).
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
    let want = if split { gaps(mode.players(), size.x.max(2), size.y.max(2)) } else { Vec::new() };
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
