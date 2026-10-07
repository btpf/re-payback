//! 3D objects (`Objects/**/*.Obj`) and map scenery placement.
//!
//! Object space: x right, y forward, z down (negative = up), in game units.
//! World space: see `city.rs`; one unit = 256 game units.
//!
//! UVs are in texels of the 256x128 object texture, with v in half-texel
//! units (0-255 spans the 128 rows); verified on the police car light bar.

use crate::city::{UNITS_PER_SQUARE, stack_height};
use crate::data::GameData;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
use payback_formats::{
    IndexedImage, Obj,
    map::{MAP_SIZE, Map},
};
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
    meshes: HashMap<String, Option<Handle<Mesh>>>,
}

impl ObjectLibrary {
    pub fn new(
        data: &GameData,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
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
            let Ok(img) = std::fs::read(entry.path())
                .map_err(|e| e.to_string())
                .and_then(|b| IndexedImage::object_texture(&b).map_err(|e| e.to_string()))
            else {
                continue;
            };
            let pixels = img.to_rgba(&data.palette, Some(0));
            let (sx, sy) = (slot % ATLAS_COLS * TEX_W, slot / ATLAS_COLS * TEX_H);
            for row in 0..TEX_H {
                let src = (row * TEX_W * 4) as usize;
                let dst = (((sy + row) * ATLAS_W + sx) * 4) as usize;
                rgba[dst..dst + (TEX_W * 4) as usize]
                    .copy_from_slice(&pixels[src..src + (TEX_W * 4) as usize]);
            }
            slots.insert(entry.file_name().to_string_lossy().to_lowercase(), slot);
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
            meshes: HashMap::new(),
        }
    }

    /// Mesh for `Objects/<path>`, loaded on first use. `None` if missing.
    pub fn mesh(
        &mut self,
        data: &GameData,
        meshes: &mut Assets<Mesh>,
        path: &str,
    ) -> Option<Handle<Mesh>> {
        if let Some(m) = self.meshes.get(path) {
            return m.clone();
        }
        let built = find_case_insensitive(&data.root.join("Objects"), path)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| match Obj::parse(&b) {
                Ok(o) => Some(o),
                Err(e) => {
                    warn!("{path}: {e}");
                    None
                }
            })
            .map(|o| meshes.add(self.build_mesh(&o)));
        if built.is_none() {
            warn!("object {path} not found");
        }
        self.meshes.insert(path.to_string(), built.clone());
        built
    }

    fn build_mesh(&self, obj: &Obj) -> Mesh {
        let s = 1.0 / UNITS_PER_SQUARE;
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for face in &obj.faces {
            let slot = face.active_slot();
            let tex = self.slots.get(&slot.texture.to_lowercase()).copied();
            for (&vi, &(u, v)) in face.indices.iter().zip(&slot.uv) {
                let p = obj.vertices[vi];
                positions.push([p.x as f32 * s, -p.z as f32 * s, p.y as f32 * s]);
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
    map: &Map,
) {
    let s = 1.0 / UNITS_PER_SQUARE;
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let mut base = 0.0;
            for cube in &map.square(x, y).cubes {
                let top = base + cube.height1.max(cube.height2) as f32;
                if cube.scenery != 0 {
                    let path = SCENERY.get(cube.scenery as usize - 1);
                    if let Some(mesh) = path.and_then(|p| library.mesh(data, meshes, p)) {
                        let angle = cube.rotation as f32 / 256.0 * std::f32::consts::TAU;
                        commands.spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(library.material.clone()),
                            Transform::from_xyz(x as f32 + 0.5, top * s, y as f32 + 0.5)
                                .with_rotation(Quat::from_rotation_y(-angle)),
                        ));
                    } else {
                        warn!("square ({x},{y}): unknown scenery id {}", cube.scenery);
                    }
                }
                base += stack_height(cube);
            }
        }
    }
}
