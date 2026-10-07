//! The spinning 3D option object (`FUN_010c0c1c` / `FUN_010c101c(obj, 0,
//! 0, 0, 0x65)`), drawn by a second camera between the two menu layers.

use super::ui::{ModelView, TURN};
use crate::data::GameData;
use crate::objects::ObjectLibrary;
use bevy::{camera::visibility::RenderLayers, prelude::*};
use std::collections::HashMap;

/// Render layer of the menu camera and its model.
pub const LAYER: usize = 7;

/// Original camera-space units per render unit.
const UNITS: f32 = 1000.0;

/// Camera-space units per model unit. INFERRED (sized by eye against the
/// menu text): the in-game scale (one model unit = 16 world units
/// horizontally, 8 vertically, rendering.md §0) with the vertical factor.
const MODEL_UNIT: f32 = 8.0;

/// The model to show this frame (set by the menu loop).
#[derive(Resource, Default)]
pub struct CurrentModel(pub Option<ModelView>);

/// The front end's own object library (not the level's), kept for reuse.
#[derive(Resource, Default)]
pub struct ModelCache {
    lib: Option<ObjectLibrary>,
    meshes: HashMap<&'static str, Option<Handle<Mesh>>>,
}

#[derive(Component)]
pub struct MenuCamera;

#[derive(Component)]
pub struct MenuModel;

/// Camera for the option models, drawn over the back layer (order 1, no
/// clear). Same projection as the game camera.
pub fn spawn_camera(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Camera3d::default(),
            Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::Perspective(PerspectiveProjection {
                fov: 2.0 * (128.0f32 / 256.0).atan(),
                near: 0.05,
                far: 100.0,
                ..default()
            }),
            RenderLayers::layer(LAYER),
            MenuCamera,
        ))
        .id()
}

/// Original camera space (x right, y down, z into the screen) to render
/// space (y up, looking down -z).
fn to_render(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, -y, -z) / UNITS
}

/// Angles [3]/[4]/[5] about the camera-space x, y and z axes.
fn rotation(rot: [i32; 3]) -> Quat {
    let a = |v: i32| v as f32 / TURN as f32 * std::f32::consts::TAU;
    Quat::from_rotation_z(-a(rot[2])) * Quat::from_rotation_y(-a(rot[1])) * Quat::from_rotation_x(a(rot[0]))
}

#[allow(clippy::too_many_arguments)]
pub fn sync_model(
    mut commands: Commands,
    current: Res<CurrentModel>,
    data: Res<GameData>,
    mut cache: ResMut<ModelCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<crate::objects::ObjectMaterial>>,
    mut model: Query<(Entity, &mut Mesh3d, &mut Transform), With<MenuModel>>,
) {
    let Some(view) = current.0 else {
        for (e, ..) in &model {
            commands.entity(e).despawn();
        }
        return;
    };
    let cache = &mut *cache;
    let lib = cache
        .lib
        .get_or_insert_with(|| ObjectLibrary::new(&data, &mut images, &mut materials, false));
    let mesh = cache
        .meshes
        .entry(view.path)
        .or_insert_with(|| {
            let i = lib.load(&data, &mut meshes, view.path)?;
            let m = lib.build_mesh(&lib.model(i).obj, |v| {
                let k = MODEL_UNIT;
                to_render(v.x as f32 * k, v.y as f32 * k, v.z as f32 * k).to_array()
            });
            Some(meshes.add(m))
        })
        .clone();
    let Some(mesh) = mesh else { return };
    let transform = Transform::from_translation(to_render(
        view.pos[0] as f32,
        view.pos[1] as f32,
        view.pos[2] as f32,
    ))
    .with_rotation(rotation(view.rot));
    match model.single_mut() {
        Ok((_, mut m, mut t)) => {
            if m.0 != mesh {
                m.0 = mesh;
            }
            *t = transform;
        }
        Err(_) => {
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(lib.material.clone()),
                transform,
                RenderLayers::layer(LAYER),
                MenuModel,
            ));
        }
    }
}
