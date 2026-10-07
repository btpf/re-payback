//! Payback reimplementation (Bevy).
//!
//! Usage: `payback [--data <dir>] [--map <n>] [--screenshot <out.png>]`
//!
//! Controls: WASD/arrows pan, Q/E rotate, mouse wheel or Z/X zoom,
//! R/F tilt.

mod city;
mod data;

use bevy::{
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use city::Pass;
use data::GameData;
use payback_formats::Map;

fn arg(name: &str) -> Option<String> {
    std::env::args().skip_while(|a| a != name).nth(1)
}

fn main() -> AppExit {
    let loaded = GameData::locate().and_then(|root| GameData::load(&root));
    let data = match loaded {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Payback".into(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(data)
        .insert_resource(ClearColor(Color::srgb(0.35, 0.55, 0.85)))
        .add_systems(Startup, (spawn_city, spawn_camera))
        .add_systems(Update, (camera_controls, screenshot_and_exit))
        .run()
}

fn spawn_city(
    mut commands: Commands,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let map_no = arg("--map").unwrap_or_else(|| "0".into());
    let path = format!("Maps/{map_no}.map");
    let map = match data
        .read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| Map::parse(&b).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            error!("{path}: {e}");
            return;
        }
    };
    info!(
        "loaded {path} (night: {}, variant: {})",
        map.night, map.variant
    );

    let atlas = images.add(city::build_cube_atlas(&data));
    for (pass, mesh) in city::build_city_meshes(&map) {
        let material = StandardMaterial {
            base_color_texture: Some(atlas.clone()),
            unlit: true,
            alpha_mode: match pass {
                Pass::Opaque => AlphaMode::Opaque,
                Pass::Masked => AlphaMode::Mask(0.5),
                Pass::Alpha | Pass::Reflect => AlphaMode::Blend,
            },
            base_color: match pass {
                Pass::Alpha => Color::srgba(1.0, 1.0, 1.0, 0.5),
                // Placeholder until the reflection effect is reimplemented.
                Pass::Reflect => Color::srgba(0.45, 0.6, 1.0, 0.85),
                _ => Color::WHITE,
            },
            ..default()
        };
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(material)),
        ));
    }
}

/// Camera orbiting a focus point on the ground.
#[derive(Component)]
struct CameraRig {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

impl CameraRig {
    fn transform(&self) -> Transform {
        let rot = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        Transform::from_translation(self.focus + rot * Vec3::new(0.0, 0.0, self.distance))
            .looking_at(self.focus, Vec3::Y)
    }
}

fn spawn_camera(mut commands: Commands) {
    let rig = CameraRig {
        focus: Vec3::new(
            arg("--x").and_then(|v| v.parse().ok()).unwrap_or(64.0),
            1.0,
            arg("--y").and_then(|v| v.parse().ok()).unwrap_or(64.0),
        ),
        distance: arg("--dist").and_then(|v| v.parse().ok()).unwrap_or(30.0),
        yaw: 0.0,
        pitch: arg("--pitch")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(60.0)
            .to_radians(),
    };
    commands.spawn((Camera3d::default(), rig.transform(), rig));
}

fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    rig: Single<(&mut CameraRig, &mut Transform)>,
) {
    let (mut rig, mut transform) = rig.into_inner();
    let dt = time.delta_secs();
    let pressed = |k: &[KeyCode]| keys.any_pressed(k.iter().copied());

    let forward = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
    let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
    let speed = rig.distance * dt;
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
    rig.focus += pan * speed;

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

/// With `--screenshot <path>`, capture a frame once rendering has settled
/// and exit. Used for automated visual checks.
fn screenshot_and_exit(mut commands: Commands, time: Res<Time>, mut taken: Local<bool>) {
    let Some(path) = arg("--screenshot") else {
        return;
    };
    // Render pipelines compile asynchronously; give them time to be ready.
    if !*taken && time.elapsed_secs() > 6.0 {
        *taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
}
