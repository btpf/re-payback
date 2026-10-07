//! The player on foot.
//!
//! Constants marked INFERRED are placeholders until confirmed from the
//! decompiled code (docs/re/people.md).

use crate::camera::{CameraTarget, Tracked};
use crate::data::GameData;
use crate::level::StartPoints;
use crate::objects::ObjectLibrary;
use crate::world::{City, SQUARE, heading_to_render, to_render};
use bevy::prelude::*;

/// INFERRED: walking speed, game units per second.
const WALK_SPEED: f32 = 1.5 * SQUARE;
/// INFERRED: turn rate, radians per second.
const TURN_RATE: f32 = 4.0;
/// INFERRED: highest ledge a person can step up.
const STEP: f32 = 48.0;
/// INFERRED: body radius ("people now have volume", Update 1).
const RADIUS: f32 = 24.0;
const HEIGHT: f32 = 160.0;
const GRAVITY: f32 = 2000.0;
/// INFERRED: people models are drawn at this fraction of their file size.
pub const PEOPLE_SCALE: f32 = 0.5;

#[derive(Component)]
pub struct Player {
    pub heading: f32,
    pub vz: f32,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(FixedUpdate, move_player)
            .add_systems(Update, sync_player_transform);
    }
}

fn spawn_player(
    mut commands: Commands,
    data: Res<GameData>,
    starts: Option<Res<StartPoints>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(starts), Some(mut library)) = (starts, library) else {
        return;
    };
    let Some(pos) = starts.level_start.or_else(|| starts.list.first().copied()) else {
        warn!("map has no start points");
        return;
    };
    let mesh = library.mesh(&data, &mut meshes, "people/malenormal.obj");
    let mut e = commands.spawn((
        Player {
            heading: 0.0,
            vz: 0.0,
        },
        Tracked { pos, speed: 0.0 },
        CameraTarget,
        Transform::default(),
        Visibility::default(),
    ));
    if let Some(mesh) = mesh {
        // People models stand along -y in object space; rotate them upright.
        e.with_child((
            Mesh3d(mesh),
            MeshMaterial3d(library.material.clone()),
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(PEOPLE_SCALE))
                .with_translation(Vec3::Y * 90.0 * PEOPLE_SCALE / SQUARE),
        ));
    }
}

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    city: Option<Res<City>>,
    mut q: Query<(&mut Player, &mut Tracked)>,
) {
    let Some(city) = city else { return };
    let dt = time.delta_secs();
    for (mut p, mut t) in &mut q {
        let pressed = |k: &[KeyCode]| keys.any_pressed(k.iter().copied());
        if pressed(&[KeyCode::ArrowLeft, KeyCode::KeyA]) {
            p.heading -= TURN_RATE * dt;
        }
        if pressed(&[KeyCode::ArrowRight, KeyCode::KeyD]) {
            p.heading += TURN_RATE * dt;
        }
        let mut speed = 0.0;
        if pressed(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
            speed = WALK_SPEED;
        }
        if pressed(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
            speed = -WALK_SPEED * 0.5;
        }
        // Heading 0 = north (-y), clockwise.
        let dir = Vec2::new(p.heading.sin(), -p.heading.cos());
        let step = dir * speed * dt;
        let pos = t.pos;
        // Slide along walls: try each axis separately.
        let mut next = pos;
        for axis in [Vec2::new(step.x, 0.0), Vec2::new(0.0, step.y)] {
            let cand = next + axis.extend(0.0);
            let probe = cand + (axis.normalize_or_zero() * RADIUS).extend(0.0);
            if !city.blocked(probe.x, probe.y, next.z, HEIGHT, STEP) {
                next = cand;
            }
        }
        // Gravity onto the floor under us (stepping up small ledges).
        p.vz -= GRAVITY * dt;
        next.z += p.vz * dt;
        if let Some((floor, _)) = city.floor(next.x, next.y, pos.z, STEP) {
            if next.z <= floor {
                next.z = floor;
                p.vz = 0.0;
            }
        }
        t.speed = speed.abs();
        t.pos = next;
    }
}

fn sync_player_transform(mut q: Query<(&Player, &Tracked, &mut Transform)>) {
    for (p, t, mut tf) in &mut q {
        tf.translation = to_render(t.pos);
        tf.rotation = heading_to_render(p.heading);
    }
}
