//! Cameras: an overhead camera that follows the player (the game's view),
//! and a free debug camera (toggle with F2).

use crate::world::{SQUARE, to_render};
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*};

/// Marks the entity the game camera follows (its [`Tracked`] state).
#[derive(Component)]
pub struct CameraTarget;

/// Position (game units) and planar speed of a simulated entity, read by the
/// camera. Systems that move things keep this up to date.
#[derive(Component, Default, Clone, Copy)]
pub struct Tracked {
    pub pos: Vec3,
    pub speed: f32,
}

#[derive(Component)]
pub struct GameCamera {
    pub mode: CameraMode,
    /// Smoothed height above the target, game units.
    pub height: f32,
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

/// Camera height above the player on foot / at rest, game units. Rises with
/// speed so you can see further ahead when driving fast.
/// INFERRED pending docs/re/rendering.md.
const BASE_HEIGHT: f32 = 6.0 * SQUARE;
const HEIGHT_PER_SPEED: f32 = 0.25;
const MAX_HEIGHT: f32 = 16.0 * SQUARE;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera).add_systems(
            PostUpdate,
            (toggle_mode, follow, free_controls)
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
        free.transform(),
        GameCamera {
            mode: if free_requested {
                CameraMode::Free
            } else {
                CameraMode::Follow
            },
            height: BASE_HEIGHT,
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

fn follow(
    time: Res<Time>,
    target: Option<Single<&Tracked, With<CameraTarget>>>,
    cam: Single<(&mut GameCamera, &mut Transform)>,
) {
    let (mut cam, mut transform) = cam.into_inner();
    if cam.mode != CameraMode::Follow {
        return;
    }
    let Some(target) = target else { return };
    let wanted = (BASE_HEIGHT + target.speed * HEIGHT_PER_SPEED).min(MAX_HEIGHT);
    let k = 1.0 - (-2.0 * time.delta_secs()).exp();
    cam.height += (wanted - cam.height) * k;
    let focus = to_render(target.pos);
    // Looking straight down, north at the top of the screen.
    *transform = Transform::from_translation(focus + Vec3::Y * cam.height / SQUARE)
        .looking_at(focus, Vec3::NEG_Z);
    cam.free.focus = focus;
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
    if pressed(&[KeyCode::KeyW, KeyCode::ArrowUp]) {
        pan += forward;
    }
    if pressed(&[KeyCode::KeyS, KeyCode::ArrowDown]) {
        pan -= forward;
    }
    if pressed(&[KeyCode::KeyD, KeyCode::ArrowRight]) {
        pan += right;
    }
    if pressed(&[KeyCode::KeyA, KeyCode::ArrowLeft]) {
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
