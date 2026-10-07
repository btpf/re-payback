//! 3D objects (`Objects/**/*.Obj`) and map scenery placement.
//!
//! Object space: x right, y forward, z down (negative = up), in game units.
//! World space: see `city.rs`; one unit = 256 game units.
//!
//! UVs are in texels of the 256x128 object texture, with v in half-texel
//! units (0-255 spans the 128 rows); verified on the police car light bar.

use crate::data::GameData;
use crate::level::LevelEntity;
use crate::sim::ModelInfo;
use crate::world::{City, H_PER_SQUARE, heading_to_render};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
use payback_formats::{IndexedImage, Obj, map::MAP_SIZE, obj::Vertex};
use std::collections::HashMap;

/// Scenery models by map scenery id (1-based; 0 = none). Order recovered from
/// the object loader (68k `FUN_000d7874`, table at 0x348000).
pub const SCENERY: [&str; 36] = [
    "scenery/trafficlight1.obj",
    "scenery/lamppost.obj",
    "scenery/rock.obj",
    "scenery/apexlogo.obj",
    "scenery/tree.obj",
    "scenery/crate.obj",
    "scenery/newtree.obj",
    "scenery/newtree2.obj",
    "scenery/bazooka.obj",
    "scenery/flamethrower.obj",
    "scenery/largegrenade.obj",
    "scenery/minigun.obj",
    "scenery/shotgun.obj",
    "scenery/phone.obj",
    "scenery/PedestrianCrossing.obj",
    "scenery/PedestrianCrossing_flash.obj",
    "scenery/Chimney.obj",
    "scenery/Bench.obj",
    "scenery/phone_arrow.obj",
    "scenery/radar.obj",
    "scenery/AlarmBox.obj",
    "scenery/AlarmBox_flash.obj",
    "scenery/Briefcase.obj",
    "scenery/Info.obj",
    "scenery/newtree3.obj",
    "scenery/snowman.obj",
    "scenery/pistol.obj",
    "scenery/bigbomb.obj",
    "scenery/pole.obj",
    "scenery/bed.obj",
    "scenery/bedsidetable.obj",
    "scenery/potplant.obj",
    "scenery/lamppost_dark.obj",
    "scenery/key.obj",
    "scenery/lamp.obj",
    "scenery/lamppost_dark2.obj",
];

const TEX_W: u32 = 256;
const TEX_H: u32 = 128;
const ATLAS_COLS: u32 = 8;
const ATLAS_ROWS: u32 = 16;
const ATLAS_W: u32 = TEX_W * ATLAS_COLS;
const ATLAS_H: u32 = TEX_H * ATLAS_ROWS;

/// Loaded object textures and meshes, shared by every instance.
#[derive(Resource)]
pub struct ObjectLibrary {
    /// Lower-cased texture name -> atlas slot.
    slots: HashMap<String, u32>,
    pub material: Handle<StandardMaterial>,
    by_path: HashMap<String, Option<usize>>,
    models: Vec<LoadedModel>,
}

impl ObjectLibrary {
    pub fn new(
        data: &GameData,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        night: bool,
    ) -> ObjectLibrary {
        let mut rgba = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let mut slots = HashMap::new();
        let mut entries: Vec<_> = std::fs::read_dir(data.root.join("ObjectTextures"))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let slot = slots.len() as u32;
            if slot >= ATLAS_COLS * ATLAS_ROWS {
                warn!("object texture atlas full");
                break;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // Night (rendering.md §4, OS4 `FUN_010bf310`): Shine.* (not the
            // cups) and CloudsReflect swap in their `.dark` files; a few
            // textures are left alone; the rest are darkened.
            let swap = night
                && ((name.starts_with("shine.") && !name.starts_with("shine.cup"))
                    || name == "cloudsreflect")
                && !name.ends_with(".dark");
            let path = if swap {
                entry
                    .path()
                    .with_file_name(format!("{}.dark", entry.file_name().to_string_lossy()))
            } else {
                entry.path()
            };
            let read = |p: &std::path::Path| {
                std::fs::read(p)
                    .map_err(|e| e.to_string())
                    .and_then(|b| IndexedImage::object_texture(&b).map_err(|e| e.to_string()))
            };
            let Ok(img) = read(&path).or_else(|_| read(&entry.path())) else {
                continue;
            };
            let mut pixels = img.to_rgba(&data.palette, Some(0));
            let keep = ["joystick", "red shade", "lights", "tankbits"];
            if night && !swap && !name.ends_with(".dark") && !keep.contains(&name.as_str()) {
                for px in pixels.chunks_mut(4) {
                    for c in &mut px[..3] {
                        *c = c.saturating_sub(crate::city::NIGHT_DARKEN);
                    }
                }
            }
            let (sx, sy) = (slot % ATLAS_COLS * TEX_W, slot / ATLAS_COLS * TEX_H);
            for row in 0..TEX_H {
                let src = (row * TEX_W * 4) as usize;
                let dst = (((sy + row) * ATLAS_W + sx) * 4) as usize;
                rgba[dst..dst + (TEX_W * 4) as usize]
                    .copy_from_slice(&pixels[src..src + (TEX_W * 4) as usize]);
            }
            slots.insert(name, slot);
        }
        let mut atlas = Image::new(
            Extent3d {
                width: ATLAS_W,
                height: ATLAS_H,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        atlas.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(atlas)),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        ObjectLibrary {
            slots,
            material,
            by_path: HashMap::new(),
            models: Vec::new(),
        }
    }

    /// Load `Objects/<path>` (case-insensitive) on first use and return its
    /// model index. `None` if missing or unparsable.
    pub fn load(
        &mut self,
        data: &GameData,
        meshes: &mut Assets<Mesh>,
        path: &str,
    ) -> Option<usize> {
        let key = path.to_lowercase();
        if let Some(&i) = self.by_path.get(&key) {
            return i;
        }
        let parsed = find_case_insensitive(&data.root.join("Objects"), path)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| match Obj::parse(&b) {
                Ok(o) => Some(o),
                Err(e) => {
                    warn!("{path}: {e}");
                    None
                }
            });
        let index = parsed.map(|obj| {
            let mesh = meshes.add(self.build_mesh(&obj, |p| static_vertex(p)));
            let info = ModelInfo::from_obj(&key, &obj);
            self.models.push(LoadedModel { obj, mesh, info });
            self.models.len() - 1
        });
        if index.is_none() {
            warn!("object {path} not found");
        }
        self.by_path.insert(key, index);
        index
    }

    /// Static mesh for `Objects/<path>`.
    pub fn mesh(
        &mut self,
        data: &GameData,
        meshes: &mut Assets<Mesh>,
        path: &str,
    ) -> Option<Handle<Mesh>> {
        self.load(data, meshes, path)
            .map(|i| self.models[i].mesh.clone())
    }

    pub fn model(&self, i: usize) -> &LoadedModel {
        &self.models[i]
    }

    pub fn model_mut(&mut self, i: usize) -> &mut LoadedModel {
        &mut self.models[i]
    }

    /// Mesh of `obj` with each vertex placed by `place` (render units,
    /// object-local: +X left, +Y up, +Z forward). Vertices are emitted per
    /// face corner, in face order.
    pub fn build_mesh(&self, obj: &Obj, place: impl Fn(&Vertex) -> [f32; 3]) -> Mesh {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for face in &obj.faces {
            let slot = face.active_slot();
            let tex = self.slots.get(&slot.texture.to_lowercase()).copied();
            for (&vi, &(u, v)) in face.indices.iter().zip(&slot.uv) {
                positions.push(place(&obj.vertices[vi]));
                uvs.push(match tex {
                    Some(t) => {
                        let ox = (t % ATLAS_COLS * TEX_W) as f32;
                        let oy = (t / ATLAS_COLS * TEX_H) as f32;
                        [
                            (ox + u as f32 + 0.5) / ATLAS_W as f32,
                            (oy + v as f32 / 2.0 + 0.5) / ATLAS_H as f32,
                        ]
                    }
                    // Unknown/empty texture: sample a corner of slot 0.
                    None => [0.5 / ATLAS_W as f32, 0.5 / ATLAS_H as f32],
                });
            }
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.compute_flat_normals();
        mesh
    }
}

/// Model-space vertex to render units for ordinary (non-people) models:
/// one model unit = 2 h-units horizontally; vertically the top-down view
/// draws 1 h-unit per model unit (rendering.md §0). Model z is downward.
pub fn static_vertex(p: &Vertex) -> [f32; 3] {
    let (sh, sv) = (2.0 / H_PER_SQUARE, 1.0 / H_PER_SQUARE);
    [p.x as f32 * sh, -p.z as f32 * sv, p.y as f32 * sh]
}

/// Positions in the order `build_mesh` emits them, for re-posing a mesh.
pub fn corner_positions(obj: &Obj, place: impl Fn(&Vertex) -> [f32; 3]) -> Vec<[f32; 3]> {
    obj.faces
        .iter()
        .flat_map(|f| f.indices.iter().map(|&vi| place(&obj.vertices[vi])))
        .collect()
}

pub struct LoadedModel {
    pub obj: Obj,
    pub mesh: Handle<Mesh>,
    pub info: ModelInfo,
}

/// AmigaOS paths are case-insensitive; the extracted files keep the CD's case.
fn find_case_insensitive(root: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    let mut dir = root.to_path_buf();
    for part in rel.split('/') {
        let exact = dir.join(part);
        dir = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&dir)
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?
                .path()
        };
    }
    Some(dir)
}

/// Spawn every scenery object placed in `map`.
pub fn spawn_scenery(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    city: &City,
) {
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let geoms = city.square_cubes(x, y);
            for (cube, geom) in city.map.square(x, y).cubes.iter().zip(geoms) {
                if cube.scenery == 0 {
                    continue;
                }
                let path = SCENERY.get(cube.scenery as usize - 1);
                let Some(mesh) = path.and_then(|p| library.mesh(data, meshes, p)) else {
                    warn!("square ({x},{y}): unknown scenery id {}", cube.scenery);
                    continue;
                };
                // Scenery stands on its cube's top; heading = rotation * 22.5
                // angle units (rendering.md §0).
                let h = geom.top_at(0.5, 0.5);
                commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(library.material.clone()),
                    Transform::from_xyz(x as f32 + 0.5, h / H_PER_SQUARE, y as f32 + 0.5)
                        .with_rotation(heading_to_render(cube.rotation as f32 * 22.5)),
                    LevelEntity,
                ));
            }
        }
    }
}
