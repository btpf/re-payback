//! Cameras: the game's overhead camera (docs/re/rendering.md §6, OS4
//! `FUN_010556e0`) and a free debug camera (toggle with F2).

use crate::sim::{Obj, Trig};
use crate::vehicles::Vehicle;
use crate::world::H_PER_SQUARE;
use bevy::{camera::PerspectiveProjection, input::mouse::AccumulatedMouseScroll, prelude::*};

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
            .add_systems(Startup, spawn_camera)
            .add_systems(FixedUpdate, follow_tick)
            .add_systems(
                PostUpdate,
                (toggle_mode, place_game_camera, free_controls)
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
        // Vertical FOV of the 320x256 original: 2·atan(128/256) = 53.1°.
        Projection::Perspective(PerspectiveProjection {
            fov: 2.0 * (128.0f32 / 256.0).atan(),
            near: 0.05,
            far: 400.0,
            ..default()
        }),
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

/// One game tick of the original camera motion.
fn follow_tick(
    trig: Res<Trig>,
    mut cam: ResMut<GameCam>,
    target: Query<(&Obj, Has<Vehicle>), With<CameraTarget>>,
) {
    let Ok((obj, driving)) = target.single() else {
        return;
    };
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

fn place_game_camera(game: Res<GameCam>, cam: Single<(&mut GameCamera, &mut Transform)>) {
    let (mut cam, mut transform) = cam.into_inner();
    let pos = Vec3::new(
        game.x as f32 / 65536.0,
        (game.h + 1500) as f32 / H_PER_SQUARE,
        game.y as f32 / 65536.0,
    );
    if cam.mode == CameraMode::Follow {
        // Straight down, north (-y) at the top of the screen.
        *transform = Transform::from_translation(pos).looking_at(pos - Vec3::Y, Vec3::NEG_Z);
        cam.free.focus = Vec3::new(pos.x, 1.0, pos.z);
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
