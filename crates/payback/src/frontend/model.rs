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

/// Camera-space units per model (file) unit: the loader stores vertices
/// ×16 (`FUN_010bf8bc`), and `FUN_010c101c(obj, 0, 0, 0, 0x65)` adds the
/// rotated in-memory vertex (8.8 trig, ×256) to the object position ×256
/// and divides by the depth ×16 with the screen constants (`xs`, `ys` =
/// 16 at 320×256: focal length 256, the menu camera's), skipping the ×2
/// the in-game modes apply to both. So one in-memory unit is one camera
/// unit, and a file unit 16. CONFIRMED.
pub(crate) const MODEL_UNIT: f32 = 16.0;

/// People models (`+0x4909 == 1`, here `people/`): the screen axes are
/// divided by 3 and the depth taken `>> 10` instead of `>> 8` (people.md
/// §2.3), after the rotation in the original, before it here. The
/// original also adds 0x5dc to Y on frames whose pitch angle is exactly 0
/// (one frame per turn of the pause menu's person); not reproduced.
fn model_scale(path: &str) -> Vec3 {
    if path.starts_with("people/") {
        Vec3::new(MODEL_UNIT / 3.0, MODEL_UNIT / 3.0, MODEL_UNIT / 4.0)
    } else {
        Vec3::splat(MODEL_UNIT)
    }
}

/// The models to show this frame (set by the menu loop): the option
/// object and an optional one behind it.
#[derive(Resource, Default)]
pub struct CurrentModel(pub Option<ModelView>, pub Option<ModelView>);

/// The front end's own object library (not the level's), kept for reuse.
#[derive(Resource, Default)]
pub struct ModelCache {
    lib: Option<ObjectLibrary>,
    meshes: HashMap<(&'static str, Option<u8>), Option<MenuMesh>>,
    /// The props were built from the Mac `Options/*` set
    /// (`Settings::variants.menu_props`).
    mac_props: bool,
}

impl ModelCache {
    /// Follow `Settings::variants.menu_props`: switching drops the built
    /// props so they are reloaded from the other set (the object atlas
    /// holds both sets' textures).
    fn follow(&mut self, settings: Option<&crate::frontend::progress::Settings>) {
        let Some(s) = settings else { return };
        crate::objects::set_model_variants(s.variants.scenery, s.variants.menu_props);
        if self.mac_props != s.variants.menu_props {
            self.mac_props = s.variants.menu_props;
            self.meshes.clear();
            if let Some(lib) = self.lib.as_mut() {
                lib.forget("options/");
            }
        }
    }
}

/// Every model the menus show, loaded at start-up (`crate::startup`).
const MENU_MODELS: [&str; 16] = [
    "options/joystick.obj",
    "options/joystick_multi.obj",
    "options/disk.obj",
    "options/moviereel.obj",
    "options/cup_bronze.obj",
    "options/cup_silver.obj",
    "options/cup_gold.obj",
    "options/cd.obj",
    "options/controlsetup.obj",
    "options/world.obj",
    "options/questionmarks.obj",
    "options/amiga.obj",
    "options/options.obj",
    "options/torch.obj",
    "options/cross.obj",
    "people/malenormal.obj",
];

/// Start-up: the menu models' object library (the object textures).
pub fn preload_library(world: &mut World) {
    world.resource_scope(|world, mut cache: Mut<ModelCache>| {
        if cache.lib.is_some() {
            return;
        }
        let data = world.resource::<GameData>().clone();
        world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let mut materials = world.resource_mut::<Assets<crate::objects::ObjectMaterial>>();
            cache.lib = Some(ObjectLibrary::new(&data, &mut images, &mut materials, false));
        });
    });
}

/// Start-up: build every menu model.
pub fn preload_models(world: &mut World) {
    preload_library(world);
    let data = world.resource::<GameData>().clone();
    world.resource_scope(|world, mut cache: Mut<ModelCache>| {
        cache.follow(world.get_resource::<crate::frontend::progress::Settings>());
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        for path in MENU_MODELS {
            cache.mesh(&data, &mut meshes, path, None);
        }
    });
}

impl ModelCache {
    /// The meshes of `path` in paint `paint` (built on first use): the
    /// model and its blended parts with their materials
    /// ([`ObjectLibrary::menu_parts_painted`]).
    fn mesh(
        &mut self,
        data: &GameData,
        meshes: &mut Assets<Mesh>,
        path: &'static str,
        paint: Option<u8>,
    ) -> Option<MenuMesh> {
        let lib = self.lib.as_mut()?;
        self.meshes
            .entry((path, paint))
            .or_insert_with(|| {
                let i = lib.load(data, meshes, path)?;
                let k = model_scale(path);
                let (main, overlays) = lib.menu_parts_painted(&lib.model(i).obj, paint.map(usize::from), |v| {
                    to_render(v.x as f32 * k.x, v.y as f32 * k.y, v.z as f32 * k.z).to_array()
                });
                Some(MenuMesh {
                    main: meshes.add(main),
                    overlays: overlays.into_iter().map(|(m, mat)| (meshes.add(m), mat)).collect(),
                })
            })
            .clone()
    }
}

/// A menu model's meshes: the main mesh (drawn with the library's
/// material) and its blended parts.
#[derive(Clone)]
struct MenuMesh {
    main: Handle<Mesh>,
    overlays: Vec<(Handle<Mesh>, Handle<crate::objects::ObjectMaterial>)>,
}

/// The blended parts of a menu model, children of its entity.
#[derive(Component)]
struct MenuOverlay;

#[derive(Component)]
pub struct MenuCamera;

/// A menu model entity: slot 0 = the option object, 1 = the background.
#[derive(Component)]
pub struct MenuModel(u8);

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
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mut model: Query<(Entity, &MenuModel, &Mesh3d, &mut Transform)>,
) {
    cache.follow(settings.as_deref());
    for (slot, view) in [current.0, current.1].into_iter().enumerate() {
        let existing = model.iter_mut().find(|(_, m, ..)| m.0 as usize == slot);
        let Some(view) = view else {
            if let Some((e, ..)) = existing {
                commands.entity(e).despawn();
            }
            continue;
        };
        if cache.lib.is_none() {
            cache.lib = Some(ObjectLibrary::new(&data, &mut images, &mut materials, false));
        }
        let Some(mesh) = cache.mesh(&data, &mut meshes, view.path, view.paint) else { continue };
        let material = cache.lib.as_ref().map(|l| l.material.clone()).unwrap_or_default();
        let transform = Transform::from_translation(to_render(
            view.pos[0] as f32,
            view.pos[1] as f32,
            view.pos[2] as f32,
        ))
        .with_rotation(rotation(view.rot));
        match existing {
            Some((_, _, m, mut t)) if m.0 == mesh.main => *t = transform,
            other => {
                if let Some((e, ..)) = other {
                    commands.entity(e).despawn();
                }
                commands
                    .spawn((
                        Mesh3d(mesh.main.clone()),
                        MeshMaterial3d(material),
                        transform,
                        RenderLayers::layer(LAYER),
                        MenuModel(slot as u8),
                    ))
                    .with_children(|p| {
                        for (part, mat) in &mesh.overlays {
                            p.spawn((
                                Mesh3d(part.clone()),
                                MeshMaterial3d(mat.clone()),
                                Transform::IDENTITY,
                                RenderLayers::layer(LAYER),
                                MenuOverlay,
                            ));
                        }
                    });
            }
        }
    }
}
