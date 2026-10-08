//! 3D objects (`Objects/**/*.Obj`) and map scenery placement.
//!
//! Object space: x right, y forward, z down (negative = up), in game units.
//! World space: see `city.rs`; one unit = 256 game units.
//!
//! UVs are in texels of the 256x128 object texture, with v in half-texel
//! units (0-255 spans the 128 rows); verified on the police car light bar.
//!
//! Face slots (OS4 loader `FUN_010bf8bc`, renderer `FUN_010c101c`): each
//! face has two 9-byte slots, one per side (the renderer picks the slot by
//! the projected winding and draws it only if its `active` byte is 1); the
//! low nibble of `mode` selects 1 flat colour (`param` through
//! `General/64->256.dat`), 2 textured, 3 texel 0 transparent, 4
//! translucent, 5 line, 6 lens flare (slot 0 only, drawn even when the
//! slot is inactive, `param` = Alpha.256 table). Here the active slot is
//! drawn double-sided; faces with no active slot are degenerate.
//!
//! Also: traffic lights, crossing beacons and alarm boxes that change model
//! with the light phase, spinning pickups, chimney smoke, lens-flare sources
//! and the burnt / dented variants of vehicles.

use crate::camera::{GameCam, GameCamera};
use crate::data::GameData;
use crate::effects::{Blend, Effects, EffectsDraw, FlareSource, Flares, FxMode, Tex};
use crate::level::{AppState, LevelEntity};
use crate::sim::driving::Drive;
use crate::sim::people::{PEOPLE_MODELS, Person};
use crate::sim::traffic::LightPhase;
use crate::sim::{ModelInfo, Obj as SimObj, Rng};
use crate::vehicles::Vehicle;
use crate::world::{City, H_PER_SQUARE, heading_to_render, z_of_h};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{ImageSampler, ImageSamplerDescriptor},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, PrimitiveTopology, TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
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
/// The atlas holds every texture twice: with texel 0 transparent (top
/// half, for mode 3/4 faces) and opaque (bottom half, everything else; the
/// renderer only keys colour 0 for mode 3).
const ATLAS_FULL_H: u32 = 2 * ATLAS_H;
/// Atlas slot of the flat palette colours (mode 1 faces): colour c is the
/// 8x16 cell at (c % 32, c / 32).
const PALETTE_SLOT: u32 = ATLAS_COLS * ATLAS_ROWS - 1;
/// Fixed UVs of mode-1 faces drawn burnt (`FUN_010c101c`): (2,2), (2,125),
/// (125,125) in texels/rows, here in file units (v doubled).
const BURNT_FIXED_UV: [(i32, i32); 3] = [(2, 4), (2, 250), (125, 250)];
/// Lens-flare size of model faces (`DAT_0114c8dc`, default 300).
const FLARE_SIZE: i32 = 300;

/// Paint colours (`FUN_01047af4`; instance `+0x3c4` Shine, `+0x3c8`
/// Cardef): faces textured `Shine.All` / `Cardef` are drawn with the
/// vehicle's pair instead (`FUN_010c101c`). Index 3 pairs purple with orange.
const PAINTS: [(&str, &str); 8] = [
    ("shine.black", "cardef.black"),
    ("shine.blue", "cardef.blue"),
    ("shine.green", "cardef.green"),
    ("shine.purple", "cardef.orange"),
    ("shine.red", "cardef.red"),
    ("shine.silver", "cardef.silver"),
    ("shine.yellow", "cardef.yellow"),
    ("shine.white", "cardef.white"),
];

/// The object material: unlit atlas texture with environment-mapped faces
/// (`shaders/objects.wgsl`).
pub type ObjectMaterial = ExtendedMaterial<StandardMaterial, EnvMapped>;

/// Vertex-shader extension that maps "Shine" faces by their normal.
#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub struct EnvMapped {}

impl MaterialExtension for EnvMapped {
    fn vertex_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }
}

/// Reflection centres (texels, rows) of `FUN_010c101c`: per-vertex normal
/// mapping (mode & 0x20) and screen-space mapping (otherwise); see
/// `shaders/objects.wgsl`.
const ENV_CENTRE_SMOOTH: (f32, f32) = (127.0, 63.0);
const ENV_CENTRE_SCREEN: (f32, f32) = (88.0, 52.0);

/// How a "Shine" face (mode & 0x10) is drawn (`FUN_010c101c`; kinds as in
/// `shaders/objects.wgsl`): 0x32 normal-mapped slot texture, 0x12/0x14
/// screen-mapped slot texture, 0x31 / 0x11 the file-UV texture with
/// CloudsReflect (`DAT_0114c904`) blended over it, normal- / screen-mapped.
fn shine_kind(mode: i32) -> Option<u32> {
    if mode & 0x10 == 0 {
        return None;
    }
    let smooth = mode & 0x20 != 0;
    Some(match (mode & 0xf == 1, smooth) {
        (false, true) => 1,
        (false, false) => 2,
        (true, true) => 3,
        (true, false) => 4,
    })
}

/// How a model's faces are textured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Skin {
    Normal,
    /// Burning or burnt-out (`obj+0x33c` burn timer >= 1): mode-1 faces get
    /// the `burnt` texture at fixed UVs, mode-2 faces at their own UVs, and
    /// every other face is not drawn (`FUN_010c101c`).
    Burnt,
}

/// Loaded object textures and meshes, shared by every instance.
#[derive(Resource)]
pub struct ObjectLibrary {
    /// Lower-cased texture name -> atlas slot.
    slots: HashMap<String, u32>,
    /// `General/64->256.dat`: flat-colour `param` -> palette index.
    flat_colours: [u8; 64],
    pub material: Handle<ObjectMaterial>,
    /// Mode 4 faces: drawn through Alpha.256 table 0 (`FUN_011113e0` with
    /// the table pointer `DAT_0114c908`), i.e. additive.
    additive: Handle<ObjectMaterial>,
    /// Night map: vehicles get the `Lights` headlight polygon.
    night: bool,
    /// Additive faces and mode-5 lines of each built mesh, drawn as child
    /// entities by [`sync_overlays`].
    overlays: HashMap<AssetId<Mesh>, Overlay>,
    by_path: HashMap<String, Option<usize>>,
    models: Vec<LoadedModel>,
}

/// The parts of a model mesh drawn with other pipelines.
#[derive(Clone, Default)]
struct Overlay {
    additive: Option<Handle<Mesh>>,
    lines: Option<Handle<Mesh>>,
}

/// Triangles (or line segments) under construction.
#[derive(Default)]
struct Tris {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    env: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
}

impl Tris {
    fn degenerate(&mut self, p: [f32; 3]) {
        self.positions.extend([p; 3]);
        self.uvs.extend([[0.0; 2]; 3]);
        self.env.extend([[-1.0; 2]; 3]);
        self.normals.extend([[0.0, 1.0, 0.0]; 3]);
    }

    fn mesh(self, topology: PrimitiveTopology) -> Mesh {
        Mesh::new(
            topology,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.env)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
    }
}

/// A model mesh and its overlay parts.
struct Parts {
    main: Mesh,
    additive: Option<Mesh>,
    lines: Option<Mesh>,
}

/// Vehicles that get the night headlight polygon (`FUN_0104e00c` runs
/// `FUN_010bf070` over the vehicle table `DAT_012a6364…63d0` and the police
/// variants; models with `+0x4928` set — tank, helicopter, boat — are
/// skipped).
fn has_headlights(path: &str) -> bool {
    path.starts_with("vehicles/")
        && !matches!(path, "vehicles/tank.obj" | "vehicles/helicopter.obj" | "vehicles/boat.obj")
}

impl ObjectLibrary {
    pub fn new(
        data: &GameData,
        images: &mut Assets<Image>,
        materials: &mut Assets<ObjectMaterial>,
        night: bool,
    ) -> ObjectLibrary {
        let mut rgba = vec![0u8; (ATLAS_W * ATLAS_FULL_H * 4) as usize];
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
            if slot >= PALETTE_SLOT {
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
        // Flat palette colours, unaffected by night darkening.
        let (px, py) = (
            PALETTE_SLOT % ATLAS_COLS * TEX_W,
            PALETTE_SLOT / ATLAS_COLS * TEX_H,
        );
        for row in 0..TEX_H {
            for col in 0..TEX_W {
                let c = (row / 16 * 32 + col / 8) as usize;
                let dst = (((py + row) * ATLAS_W + px + col) * 4) as usize;
                rgba[dst..dst + 3].copy_from_slice(&data.palette[c * 3..c * 3 + 3]);
                rgba[dst + 3] = 255;
            }
        }
        let mut flat_colours = [0u8; 64];
        match data
            .read("General/64->256.dat")
            .or_else(|_| data.read("General/64-_256.dat"))
        {
            Ok(b) if b.len() >= 64 => flat_colours.copy_from_slice(&b[..64]),
            _ => warn!("missing General/64->256.dat"),
        }
        // Opaque copy in the bottom half.
        let half = (ATLAS_W * ATLAS_H * 4) as usize;
        let (keyed, opaque) = rgba.split_at_mut(half);
        opaque.copy_from_slice(keyed);
        for px in opaque.chunks_mut(4) {
            px[3] = 255;
        }
        let mut atlas = Image::new(
            Extent3d {
                width: ATLAS_W,
                height: ATLAS_FULL_H,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        atlas.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
        let base = StandardMaterial {
            base_color_texture: Some(images.add(atlas)),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            // Faces are one-sided; see `face_sides`.
            double_sided: false,
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        };
        let material = materials.add(ExtendedMaterial {
            base: base.clone(),
            extension: EnvMapped::default(),
        });
        let additive = materials.add(ExtendedMaterial {
            base: StandardMaterial {
                alpha_mode: AlphaMode::Add,
                ..base
            },
            extension: EnvMapped::default(),
        });
        ObjectLibrary {
            slots,
            flat_colours,
            material,
            additive,
            night,
            overlays: HashMap::new(),
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
            let info = ModelInfo::from_obj(&key, &obj);
            let parts = self.build_parts(&obj, Skin::Normal, None, &info, |p, _| static_vertex(p));
            let mesh = self.add_parts(meshes, parts);
            let flares = flare_faces(&obj);
            self.models.push(LoadedModel {
                obj,
                mesh,
                info,
                flares,
                variants: HashMap::new(),
            });
            self.models.len() - 1
        });
        if index.is_none() {
            warn!("object {path} not found");
        }
        self.by_path.insert(key, index);
        index
    }

    /// Static mesh for `Objects/<path>`.
    #[allow(dead_code, reason = "public API; no caller yet")]
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

    /// Index of an already loaded model.
    pub fn loaded(&self, path: &str) -> Option<usize> {
        self.by_path.get(&path.to_lowercase()).copied().flatten()
    }

    /// Mesh of model `i` with `skin`, dented (`obj+0x452`) or not; built on
    /// first use. Dented vertices are displaced by fixed random offsets
    /// (`FUN_010c101c`: vertex n moves by `FUN_0103ad20(3(n+1)+axis, 160) − 80`
    /// on x, y, z, in 1/16 model units).
    pub fn variant_mesh(
        &mut self,
        meshes: &mut Assets<Mesh>,
        i: usize,
        skin: Skin,
        dent: Option<&Rng>,
        paint: Option<usize>,
    ) -> Handle<Mesh> {
        let key = (skin, dent.is_some(), paint);
        if key == (Skin::Normal, false, None) {
            return self.models[i].mesh.clone();
        }
        if let Some(h) = self.models[i].variants.get(&key) {
            return h.clone();
        }
        let obj = &self.models[i].obj;
        let info = &self.models[i].info;
        let offsets: Vec<[f32; 3]> = (0..obj.vertices.len())
            .map(|n| match dent {
                Some(rng) => {
                    let k = 3 * (n + 1);
                    [0, 1, 2].map(|a| (rng.hash(k + a, 160) - 80) as f32 / 16.0)
                }
                None => [0.0; 3],
            })
            .collect();
        let parts = self.build_parts(obj, skin, paint, info, |v, n| {
            let [dx, dy, dz] = offsets[n];
            let (sh, sv) = (2.0 / H_PER_SQUARE, 1.0 / H_PER_SQUARE);
            [
                (v.x as f32 + dx) * sh,
                -(v.z as f32 + dz) * sv,
                (v.y as f32 + dy) * sh,
            ]
        });
        let h = self.add_parts(meshes, parts);
        self.models[i].variants.insert(key, h.clone());
        h
    }

    /// Mesh of `obj` with each vertex placed by `place` (render units,
    /// object-local: +X left, +Y up, +Z forward). Vertices are emitted per
    /// face corner, in face order.
    pub fn build_mesh(&self, obj: &Obj, place: impl Fn(&Vertex) -> [f32; 3]) -> Mesh {
        self.build_mesh_skin(obj, Skin::Normal, None, |v, _| place(v))
    }

    /// Atlas UV of texel (u, v in file units) in slot `t`, from the keyed
    /// (colour 0 transparent) or the opaque half.
    fn atlas_uv(t: u32, u: i32, v: i32, keyed: bool) -> [f32; 2] {
        let ox = (t % ATLAS_COLS * TEX_W) as f32;
        let oy = (t / ATLAS_COLS * TEX_H + if keyed { 0 } else { ATLAS_H }) as f32;
        [
            (ox + u as f32 + 0.5) / ATLAS_W as f32,
            (oy + v as f32 / 2.0 + 0.5) / ATLAS_FULL_H as f32,
        ]
    }

    /// [`ObjectLibrary::build_mesh`] with a skin; `place` also gets the
    /// vertex index. Additive (mode 4) faces and lines (mode 5) are left out
    /// (degenerate); see [`ObjectLibrary::build_parts`].
    fn build_mesh_skin(
        &self,
        obj: &Obj,
        skin: Skin,
        paint: Option<usize>,
        place: impl Fn(&Vertex, usize) -> [f32; 3],
    ) -> Mesh {
        self.build_parts_with(obj, skin, paint, None, place).main
    }

    /// Add a mesh and remember its overlay parts.
    fn add_parts(&mut self, meshes: &mut Assets<Mesh>, parts: Parts) -> Handle<Mesh> {
        let h = meshes.add(parts.main);
        let overlay = Overlay {
            additive: parts.additive.map(|m| meshes.add(m)),
            lines: parts.lines.map(|m| meshes.add(m)),
        };
        if overlay.additive.is_some() || overlay.lines.is_some() {
            self.overlays.insert(h.id(), overlay);
        }
        h
    }

    /// The model's parts, with the night headlight polygon for vehicles.
    fn build_parts(
        &self,
        obj: &Obj,
        skin: Skin,
        paint: Option<usize>,
        info: &ModelInfo,
        place: impl Fn(&Vertex, usize) -> [f32; 3],
    ) -> Parts {
        let lights = if self.night && skin == Skin::Normal && has_headlights(&info.path) {
            self.slots.get("lights").map(|&t| (info, t))
        } else {
            None
        };
        self.build_parts_with(obj, skin, paint, lights, place)
    }

    fn build_parts_with(
        &self,
        obj: &Obj,
        skin: Skin,
        paint: Option<usize>,
        headlights: Option<(&ModelInfo, u32)>,
        place: impl Fn(&Vertex, usize) -> [f32; 3],
    ) -> Parts {
        let mut main = Tris::default();
        let mut add = Tris::default();
        let mut lines = Tris::default();
        let burnt = self.slots.get("burnt").copied();
        let reflect = self.slots.get("cloudsreflect").copied();
        // Smooth vertex normals: sum of the unit normals of the drawn faces
        // using the vertex (loader `FUN_010bf8bc`), each facing its visible side.
        let placed: Vec<Vec3> =
            obj.vertices.iter().enumerate().map(|(i, v)| Vec3::from(place(v, i))).collect();
        let mut smooth = vec![Vec3::ZERO; obj.vertices.len()];
        for (face, side) in face_sides(obj) {
            if let Some(side) = side {
                let [a, b, c] = side_order(side).map(|k| face.indices[k]);
                let n = (placed[b] - placed[a]).cross(placed[c] - placed[a]).normalize_or_zero();
                for i in [a, b, c] {
                    smooth[i] += n;
                }
            }
        }
        for (face, side) in face_sides(obj) {
            let p0 = || place(&obj.vertices[face.indices[0]], face.indices[0]);
            let Some(side) = side else {
                // Not drawn: a degenerate triangle keeps the layout.
                main.degenerate(p0());
                continue;
            };
            let slot = &face.slots[side];
            let mode = slot.mode & 0xf;
            if skin == Skin::Burnt && !matches!(mode, 1 | 2) {
                main.degenerate(p0());
                continue;
            }
            if mode == 5 {
                // A line between the first two vertices, colour `param`
                // through 64->256 like mode 1 (the loader maps every mode
                // but 6).
                main.degenerate(p0());
                let c = self.flat_colours[(slot.param & 63) as usize] as u32;
                let ox = (PALETTE_SLOT % ATLAS_COLS * TEX_W + c % 32 * 8 + 4) as f32;
                let oy = (PALETTE_SLOT / ATLAS_COLS * TEX_H + c / 32 * 16 + 8) as f32;
                for k in [0, 1] {
                    let vi = face.indices[k];
                    lines.positions.push(place(&obj.vertices[vi], vi));
                    lines.uvs.push([ox / ATLAS_W as f32, oy / ATLAS_FULL_H as f32]);
                    lines.env.push([-1.0; 2]);
                    lines.normals.push([0.0, 1.0, 0.0]);
                }
                continue;
            }
            // Plain mode 4 goes to the additive overlay; the main mesh keeps
            // a degenerate triangle in its place.
            let out = if mode == 4 && slot.mode & 0x10 == 0 {
                main.degenerate(p0());
                &mut add
            } else {
                &mut main
            };
            let order = side_order(side);
            let [a, b, c] = order.map(|k| face.indices[k]);
            let flat = (placed[b] - placed[a]).cross(placed[c] - placed[a]).normalize_or_zero();
            let shine = if skin == Skin::Normal { shine_kind(slot.mode) } else { None };
            let smooth_env = slot.mode & 0x20 != 0;
            let tex = match skin {
                Skin::Burnt => burnt,
                Skin::Normal => {
                    let name = slot.texture.to_lowercase();
                    let name = match (paint.and_then(|p| PAINTS.get(p)), name.as_str()) {
                        (Some(&(shine, _)), "shine.all") => shine,
                        (Some(&(_, cardef)), "cardef") => cardef,
                        _ => name.as_str(),
                    };
                    self.slots.get(name).copied()
                }
            };
            // Mode 3 keys colour 0 out; additive faces add nothing there.
            let keyed = skin == Skin::Normal && mode == 3;
            for k in order {
                let (vi, (u, v)) = (face.indices[k], slot.uv[k]);
                out.positions.push(place(&obj.vertices[vi], vi));
                let n = if shine.is_some() && smooth_env { smooth[vi].normalize_or_zero() } else { flat };
                out.normals.push(n.to_array());
                // Replacement kinds map the slot texture; overlays CloudsReflect.
                let source = match shine {
                    Some(1 | 2) => tex,
                    Some(_) => reflect,
                    None => None,
                };
                out.env.push(match (shine, source) {
                    (Some(kind), Some(t)) => {
                        let (cu, cv) = if smooth_env { ENV_CENTRE_SMOOTH } else { ENV_CENTRE_SCREEN };
                        let ox = (t % ATLAS_COLS * TEX_W) as f32 + cu + 0.5;
                        let oy = (t / ATLAS_COLS * TEX_H + ATLAS_H) as f32 + cv + 0.5;
                        [ox / ATLAS_W as f32 + 2.0 * kind as f32, oy / ATLAS_FULL_H as f32]
                    }
                    _ => [-1.0, -1.0],
                });
                out.uvs.push(match (skin, slot.mode, tex) {
                    (Skin::Burnt, _, Some(t)) if mode == 1 => {
                        let (u, v) = BURNT_FIXED_UV[k];
                        Self::atlas_uv(t, u, v, false)
                    }
                    // Flat colour: the centre of the palette cell.
                    (Skin::Normal, 1, _) => {
                        let c = self.flat_colours[(slot.param & 63) as usize] as u32;
                        let ox = (PALETTE_SLOT % ATLAS_COLS * TEX_W + c % 32 * 8 + 4) as f32;
                        let oy = (PALETTE_SLOT / ATLAS_COLS * TEX_H + c / 32 * 16 + 8) as f32;
                        [ox / ATLAS_W as f32, oy / ATLAS_FULL_H as f32]
                    }
                    (_, _, Some(t)) => Self::atlas_uv(t, u, v, keyed),
                    // Unknown/empty texture: sample a corner of slot 0.
                    (_, _, None) => [0.5 / ATLAS_W as f32, 0.5 / ATLAS_FULL_H as f32],
                });
            }
        }
        if let Some((info, tex)) = headlights {
            headlight_polygon(&mut add, info, tex);
        }
        let nonempty = |t: Tris, topology| (!t.positions.is_empty()).then(|| t.mesh(topology));
        Parts {
            main: main.mesh(PrimitiveTopology::TriangleList),
            additive: nonempty(add, PrimitiveTopology::TriangleList),
            lines: nonempty(lines, PrimitiveTopology::LineList),
        }
    }
}

/// Model-space vertex to render units for ordinary (non-people) models:
/// one model unit = 2 h-units horizontally; vertically the top-down view
/// draws 1 h-unit per model unit (rendering.md §0). Model z is downward.
pub fn static_vertex(p: &Vertex) -> [f32; 3] {
    let (sh, sv) = (2.0 / H_PER_SQUARE, 1.0 / H_PER_SQUARE);
    [p.x as f32 * sh, -p.z as f32 * sv, p.y as f32 * sh]
}

/// The night headlight polygon (`FUN_010bf070`): a rectangle on the
/// model's ground plane (z = 0) from `y0 = max_y − r/2` to `y1 = max_y +
/// 10r`, `x = ±2r` (r = `+0x48c4`, max |x|; `max_y` = `+0x48c8`), as two
/// one-sided triangles textured `Lights` at (77,3) (17,3) (77,82) and
/// (17,3) (17,82) (77,82) (texel, row), mode 4 (additive), param 2.
fn headlight_polygon(out: &mut Tris, info: &ModelInfo, tex: u32) {
    let r = info.radius as f32 / 16.0;
    let my = info.max_y as f32 / 16.0;
    let (y0, y1) = (my - r / 2.0, my + 10.0 * r);
    let sh = 2.0 / H_PER_SQUARE;
    let corner = |x: f32, y: f32| [x * sh, 0.0, y * sh];
    // (position, u texel, v row).
    let v0 = (corner(2.0 * r, y0), 77, 3);
    let v1 = (corner(-2.0 * r, y0), 17, 3);
    let v2 = (corner(2.0 * r, y1), 77, 82);
    let v3 = (corner(-2.0 * r, y1), 17, 82);
    for tri in [[v0, v1, v2], [v1, v3, v2]] {
        // Facing up: counter-clockwise seen from above.
        let [a, b, c] = tri.map(|t| Vec3::from(t.0));
        let tri = if (b - a).cross(c - a).y < 0.0 { [tri[0], tri[2], tri[1]] } else { tri };
        for (p, u, v) in tri {
            out.positions.push(p);
            // Rows are file units / 2.
            out.uvs.push(ObjectLibrary::atlas_uv(tex, u, v * 2, false));
            out.env.push([-1.0; 2]);
            out.normals.push([0.0, 1.0, 0.0]);
        }
    }
}

/// Overlay children of an object entity (see [`Overlay`]).
#[derive(Component)]
struct OverlayChildren(Vec<Entity>);

/// Keep each object's overlay children (additive faces, lines) in step
/// with its mesh.
#[allow(clippy::type_complexity)]
fn sync_overlays(
    mut commands: Commands,
    library: Option<Res<ObjectLibrary>>,
    q: Query<
        (Entity, &Mesh3d, Option<&OverlayChildren>),
        (With<MeshMaterial3d<ObjectMaterial>>, Changed<Mesh3d>),
    >,
) {
    let Some(library) = library else { return };
    for (entity, mesh, old) in &q {
        if let Some(old) = old {
            for &e in &old.0 {
                commands.entity(e).try_despawn();
            }
        }
        let Some(overlay) = library.overlays.get(&mesh.0.id()) else {
            if old.is_some() {
                commands.entity(entity).remove::<OverlayChildren>();
            }
            continue;
        };
        let mut children = Vec::new();
        for (part, material) in [
            (&overlay.additive, &library.additive),
            (&overlay.lines, &library.material),
        ] {
            if let Some(part) = part {
                let child = commands
                    .spawn((
                        Mesh3d(part.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::IDENTITY,
                        ChildOf(entity),
                    ))
                    .id();
                children.push(child);
            }
        }
        commands.entity(entity).insert(OverlayChildren(children));
    }
}

/// Positions in the order `build_mesh` emits them, for re-posing a mesh.
pub fn corner_positions(obj: &Obj, place: impl Fn(&Vertex) -> [f32; 3]) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for (f, side) in face_sides(obj) {
        match side {
            Some(side) => out.extend(side_order(side).map(|k| place(&obj.vertices[f.indices[k]]))),
            None => out.extend([place(&obj.vertices[f.indices[0]]); 3]),
        }
    }
    out
}

/// The triangles a model is drawn with: one per active slot (`None` for a
/// face with no active slot, kept as a degenerate triangle). The renderer
/// draws slot 0 when the face winds one way on screen and slot 1 when it
/// winds the other (`FUN_010c101c`), so each slot is a one-sided triangle
/// and slot 1 faces the opposite way.
fn face_sides(obj: &Obj) -> impl Iterator<Item = (&payback_formats::obj::Face, Option<usize>)> {
    obj.faces.iter().flat_map(|f| {
        let sides: Vec<Option<usize>> = match (f.slots[0].active, f.slots[1].active) {
            (false, false) => vec![None],
            (a, b) => [(a, 0), (b, 1)].into_iter().filter(|s| s.0).map(|s| Some(s.1)).collect(),
        };
        sides.into_iter().map(move |s| (f, s))
    })
}

/// Corner order of a slot's triangle (slot 1 reversed).
fn side_order(side: usize) -> [usize; 3] {
    if side == 0 { [0, 1, 2] } else { [0, 2, 1] }
}

/// A lens-flare face: mode 6 in slot 0 (`FUN_010c101c` calls
/// `AddFlare(vertex 0, DAT_0114c8dc, param << 16)`).
#[derive(Clone, Copy, Debug)]
pub struct FlareFace {
    /// Index of the face's first triangle in the mesh.
    pub face: usize,
    pub vertex: usize,
    pub table: u8,
}

fn flare_faces(obj: &Obj) -> Vec<FlareFace> {
    // First emitted triangle of each face; its corner 0 is the face's vertex 0.
    let mut first = vec![0; obj.faces.len()];
    let mut seen = 0;
    let mut tri = 0;
    for (f, _) in face_sides(obj) {
        let i = obj.faces.iter().position(|g| std::ptr::eq(g, f)).unwrap_or(0);
        if i >= seen {
            first[i] = tri;
            seen = i + 1;
        }
        tri += 1;
    }
    obj.faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.slots[0].mode == 6)
        .map(|(i, f)| FlareFace {
            face: first[i],
            vertex: f.indices[0],
            table: f.slots[0].param.clamp(0, 255) as u8,
        })
        .collect()
}

pub struct LoadedModel {
    pub obj: Obj,
    pub mesh: Handle<Mesh>,
    pub info: ModelInfo,
    /// Lens-flare faces.
    pub flares: Vec<FlareFace>,
    /// Burnt / dented / painted meshes, by (skin, dented, paint).
    variants: HashMap<(Skin, bool, Option<usize>), Handle<Mesh>>,
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

/// Scenery entity per map cell (sx, sy, cube level).
#[derive(Resource, Default)]
pub struct SceneryIndex(pub HashMap<(usize, usize, usize), Entity>);

/// Change the scenery object on a map cell (pickups, crates, scripts'
/// `SetScenery`). `special`/`rotation` are left unchanged when `None`.
#[derive(Message, Clone, Copy, Debug)]
pub struct SetScenery {
    pub sx: usize,
    pub sy: usize,
    pub level: usize,
    pub scenery: u8,
    pub special: Option<u8>,
    pub rotation: Option<u8>,
}

pub struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/objects.wgsl");
        app.add_plugins(MaterialPlugin::<ObjectMaterial>::default())
            .init_resource::<SceneryIndex>()
            .init_resource::<SceneryClock>()
            .add_message::<SetScenery>()
            .add_systems(FixedPostUpdate, apply_scenery_changes)
            .add_systems(
                FixedUpdate,
                (advance_clock, chimney_smoke).run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), |mut c: ResMut<SceneryClock>| {
                *c = SceneryClock::default();
            })
            .add_systems(Update, (animate_scenery, vehicle_skins))
            .add_systems(PostUpdate, sync_overlays.before(TransformSystems::Propagate))
            .add_systems(
                FixedUpdate,
                spawn_damage_check.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                PostUpdate,
                collect_flares
                    .before(EffectsDraw)
                    .after(TransformSystems::Propagate),
            );
    }
}

/// A placed scenery object.
#[derive(Component, Clone, Copy, Debug)]
pub struct Scenery {
    /// Map scenery id (1-based).
    pub id: u8,
    /// Cube `special` (traffic lights: phase offset).
    pub special: u8,
    /// Cube rotation, 256 per turn.
    pub rotation: u8,
    /// Model currently shown.
    pub model: usize,
}

/// Scenery ids that animate (`FUN_01049374` scenery pass).
mod sid {
    pub const TRAFFIC_LIGHT: u8 = 1;
    pub const CROSSING: u8 = 15;
    pub const CHIMNEY: u8 = 17;
    pub const ALARM_FLASH: u8 = 22;
    pub const INFO: u8 = 24;
    /// Models whose `+0x4908` code is >= 0 (pickups, radar, info, key)
    /// spin (`FUN_0104e13c` sets the codes).
    pub const SPINNING: [u8; 10] = [9, 10, 11, 12, 13, 20, 24, 27, 28, 34];
}

/// Traffic light models by light state (`DAT_012f5ae0..aec`).
const TRAFFIC_LIGHTS: [&str; 4] = [
    "scenery/trafficlight1.obj",
    "scenery/trafficlight2.obj",
    "scenery/trafficlight3.obj",
    "scenery/trafficlight4.obj",
];

/// Global scenery animation counters, advanced per game tick (68k
/// `FUN_000e3a58` tail, OS4 equivalents in brackets).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SceneryClock {
    /// Pickup spin (`DAT_0034c09c` / `DAT_0114c770`), +60 per tick.
    pub spin: i32,
    /// Info sign tumble (`DAT_0034c0a4`, +30; `DAT_0034c0a0`, −45).
    pub tumble: [i32; 2],
    /// Alarm-box pulse 0..19 (`DAT_012e5adc`, +1 per tick).
    pub pulse: i32,
}

fn advance_clock(mut c: ResMut<SceneryClock>) {
    c.spin = (c.spin + 60) % 5760;
    c.tumble = [(c.tumble[0] + 30) % 5760, (c.tumble[1] + 5760 - 45) % 5760];
    c.pulse = (c.pulse + 1) % 20;
}

/// Model of a traffic light (scenery 1) with phase offset `special`: the
/// state is `(special + phase − 1) mod 6`; 0–1 red, 2 red+amber, 3–4
/// green, 5 amber (`FUN_01049374`; the same states stop traffic in
/// `crate::sim::traffic`).
pub fn traffic_light_model(special: u8, phase: i32) -> usize {
    let mut k = special as i32 + phase - 1;
    if k > 5 {
        k -= 6;
    }
    match k {
        ..2 => 0,
        2 => 1,
        3 | 4 => 2,
        _ => 3,
    }
}

/// Swap traffic-light and crossing models with the light phase; spin
/// pickups (yaw + `spin`; the info sign also tumbles, `FUN_010c7218`).
#[allow(clippy::type_complexity)]
fn animate_scenery(
    data: Res<GameData>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    lights: Option<Res<LightPhase>>,
    clock: Res<SceneryClock>,
    mut q: Query<(&mut Scenery, &mut Mesh3d, &mut Transform)>,
) {
    let Some(mut library) = library else { return };
    let (phase, sub) = lights.map_or((0, 0), |l| (l.phase, l.sub));
    for (mut sc, mut mesh, mut tf) in &mut q {
        let path = match sc.id {
            sid::TRAFFIC_LIGHT => TRAFFIC_LIGHTS[traffic_light_model(sc.special, phase)],
            // The beacon flashes from tick 25 of each 61-tick phase.
            sid::CROSSING if sub >= 25 => SCENERY[15],
            sid::CROSSING => SCENERY[14],
            id if sid::SPINNING.contains(&id) => {
                let yaw = sc.rotation as i32 * 0x1680 / 256 + clock.spin;
                let mut rot = heading_to_render(yaw as f32);
                if id == sid::INFO {
                    let [a, b] = clock
                        .tumble
                        .map(|t| t as f32 / 5760.0 * std::f32::consts::TAU);
                    rot = rot * Quat::from_rotation_z(b) * Quat::from_rotation_x(a);
                }
                tf.rotation = rot;
                continue;
            }
            _ => continue,
        };
        let Some(model) = library.load(&data, &mut meshes, path) else {
            continue;
        };
        if model != sc.model {
            sc.model = model;
            mesh.0 = library.model(model).mesh.clone();
        }
    }
}

/// Chimneys in view smoke (`FUN_01049374`, scenery 0x11): once per frame
/// one set of random numbers; if `rand(3) == 1`, every visible chimney adds
/// a type 6 Dust2 puff 2500 z-units above its base.
fn chimney_smoke(
    cam: Res<GameCam>,
    rng: Option<ResMut<Rng>>,
    mut effects: ResMut<Effects>,
    city: Option<Res<City>>,
    q: Query<(&Scenery, &Transform)>,
) {
    let (Some(mut rng), Some(city)) = (rng, city) else {
        return;
    };
    let r = [3, 100, 100, 300, 30, 30, 30, 200].map(|n| rng.rand(n));
    if r[0] != 1 {
        return;
    }
    // Visible squares either side (rendering.md §6).
    let d = (cam.h + 1500) as f32;
    let (hw, hh) = (d * 160.0 / 65536.0 + 1.0, d * 128.0 / 65536.0 + 1.0);
    let (cx, cy) = (cam.x as f32 / 65536.0, cam.y as f32 / 65536.0);
    for (sc, tf) in &q {
        if sc.id != sid::CHIMNEY {
            continue;
        }
        let (x, y) = (tf.translation.x, tf.translation.z);
        if (x - cx).abs() > hw || (y - cy).abs() > hh {
            continue;
        }
        let level = city
            .floor(x * 2048.0, y * 2048.0, tf.translation.y * H_PER_SQUARE, 1.0)
            .map_or(0, |(_, l, _)| l);
        let base = z_of_h(tf.translation.y * H_PER_SQUARE);
        let pos = Vec3::new(
            x.floor() * 2048.0 + 0x400 as f32 + (r[1] - 50) as f32,
            y.floor() * 2048.0 + 0x400 as f32 + (r[2] - 50) as f32,
            base + (r[3] - 2500) as f32,
        );
        let vel = Vec3::new((r[4] + 150) as f32, (r[5] - 15) as f32, (r[6] - 250) as f32);
        let size = (r[7] + 300) as f32;
        effects.add(6, Tex::Dust2, pos, vel, size, level as u8, Blend::Subtract);
    }
}

/// Vehicles show the burnt skin from ignition on (the burn timer stays at
/// 15 once the fire is out) and the dented mesh after a hard hit.
/// `--damaged [burnt|clean]` forces them (`clean`: just park the cars),
/// for visual checks.
fn vehicle_skins(
    library: Option<ResMut<ObjectLibrary>>,
    rng: Option<Res<Rng>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&SimObj, &Vehicle, Option<&Drive>, &mut Mesh3d)>,
) {
    let (Some(mut library), Some(rng)) = (library, rng) else {
        return;
    };
    // `--damaged`: every vehicle dented, `--damaged burnt` also burnt (checks).
    let forced = std::env::args()
        .any(|a| a == "--damaged")
        .then(|| crate::arg("--damaged").unwrap_or_default());
    for (obj, vehicle, drive, mut mesh) in &mut q {
        let paint = usize::try_from(vehicle.colour).ok().filter(|&c| c < PAINTS.len());
        let burnt = obj.burn > 0 || forced.as_deref() == Some("burnt");
        let skin = if burnt { Skin::Burnt } else { Skin::Normal };
        let dented = drive.is_some_and(|d| d.dented) || forced.as_deref().is_some_and(|f| f != "clean");
        let dent = dented.then_some(&*rng);
        let h = library.variant_mesh(&mut meshes, obj.model, skin, dent, paint);
        if mesh.0 != h {
            mesh.0 = h;
        }
    }
}

/// Lens-flare sources this frame: mode-6 faces of scenery and objects
/// (traffic and crossing lights, alarm boxes, police lights, muzzle
/// flashes, missiles), at the face's first vertex, size 300 (alarm boxes
/// pulse 200..700). Burning objects have none. A flare is dropped when the
/// city is between it and the camera (the original tests whether its
/// centre pixel was overdrawn).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn collect_flares(
    mode: Res<FxMode>,
    library: Option<Res<ObjectLibrary>>,
    city: Option<Res<City>>,
    clock: Res<SceneryClock>,
    meshes: Res<Assets<Mesh>>,
    camera: Query<&GlobalTransform, With<GameCamera>>,
    scenery: Query<(&Scenery, &GlobalTransform, &Mesh3d)>,
    objects: Query<(&SimObj, &GlobalTransform, &Mesh3d, Option<&Person>)>,
    mut flares: ResMut<Flares>,
) {
    let (Some(library), Ok(cam)) = (library, camera.single()) else {
        return;
    };
    if mode.0 == 0 {
        return;
    }
    let eye = cam.translation();
    // Roughly the visible area around the camera.
    let near = |p: Vec3| (p.x - eye.x).abs() < eye.y + 2.0 && (p.z - eye.z).abs() < eye.y + 2.0;
    let mut add = |model: usize, tf: &GlobalTransform, mesh: &Mesh3d, size: i32| {
        let m = library.model(model);
        if m.flares.is_empty() {
            return;
        }
        let positions = meshes
            .get(&mesh.0)
            .and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION))
            .and_then(|a| a.as_float3());
        for f in &m.flares {
            let local = positions
                .and_then(|p| p.get(3 * f.face).copied())
                .unwrap_or_else(|| static_vertex(&m.obj.vertices[f.vertex]));
            let pos = tf.transform_point(Vec3::from_array(local));
            if city.as_ref().is_some_and(|c| occluded(c, pos, eye)) {
                continue;
            }
            flares.add(FlareSource {
                pos,
                size,
                table: f.table,
            });
        }
    };
    for (sc, tf, mesh) in &scenery {
        if !near(tf.translation()) {
            continue;
        }
        let size = if sc.id == sid::ALARM_FLASH {
            (clock.pulse - 10).abs() * 50 + 200
        } else {
            FLARE_SIZE
        };
        add(sc.model, tf, mesh, size);
    }
    for (obj, tf, mesh, person) in &objects {
        if obj.burn >= 1 {
            continue;
        }
        let model = match person {
            Some(p) => {
                let path = p.pose_model.unwrap_or(PEOPLE_MODELS[p.kind]);
                library.loaded(path).unwrap_or(obj.model)
            }
            None => obj.model,
        };
        add(model, tf, mesh, FLARE_SIZE);
    }
}

/// Whether a solid cube lies on the segment from `from` to the camera
/// `eye` (render space), sampled every 1/8 square.
fn occluded(city: &City, from: Vec3, eye: Vec3) -> bool {
    let d = eye - from;
    let n = (d.length() * 8.0).ceil().min(400.0) as usize;
    (1..n).any(|i| {
        let p = from + d * (i as f32 / n as f32);
        let h = p.y * H_PER_SQUARE;
        city.blocked(p.x * 2048.0, p.z * 2048.0, h, 1.0, 0.0)
    })
}

fn apply_scenery_changes(
    mut commands: Commands,
    mut changes: MessageReader<SetScenery>,
    data: Res<GameData>,
    city: Option<ResMut<City>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut index: ResMut<SceneryIndex>,
) {
    let (Some(mut city), Some(mut library)) = (city, library) else {
        changes.clear();
        return;
    };
    for c in changes.read() {
        if c.sx >= MAP_SIZE || c.sy >= MAP_SIZE || c.level > 2 {
            continue;
        }
        let cube = &mut city.map.squares[c.sx * MAP_SIZE + c.sy].cubes[c.level];
        cube.scenery = c.scenery;
        if let Some(s) = c.special {
            cube.special = s;
        }
        if let Some(r) = c.rotation {
            cube.rotation = r;
        }
        if let Some(e) = index.0.remove(&(c.sx, c.sy, c.level)) {
            commands.entity(e).despawn();
        }
        spawn_one(
            &mut commands,
            &data,
            &mut library,
            &mut meshes,
            &city,
            &mut index,
            c.sx,
            c.sy,
            c.level,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_one(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    city: &City,
    index: &mut SceneryIndex,
    x: usize,
    y: usize,
    level: usize,
) {
    let cube = &city.map.square(x, y).cubes[level];
    if cube.scenery == 0 {
        return;
    }
    let path = SCENERY.get(cube.scenery as usize - 1);
    let Some(model) = path.and_then(|p| library.load(data, meshes, p)) else {
        warn!("square ({x},{y}): unknown scenery id {}", cube.scenery);
        return;
    };
    let mesh = library.model(model).mesh.clone();
    // Scenery stands on its cube's top; heading = rotation * 22.5 angle
    // units (rendering.md §0).
    let h = city.square_cubes(x, y)[level].top_at(0.5, 0.5);
    let e = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(library.material.clone()),
            Transform::from_xyz(x as f32 + 0.5, h / H_PER_SQUARE, y as f32 + 0.5)
                .with_rotation(heading_to_render(cube.rotation as f32 * 22.5)),
            Scenery {
                id: cube.scenery,
                special: cube.special,
                rotation: cube.rotation,
                model,
            },
            LevelEntity,
        ))
        .id();
    index.0.insert((x, y, level), e);
}

/// Spawn every scenery object placed in the city.
pub fn spawn_scenery(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    city: &City,
    index: &mut SceneryIndex,
) {
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for level in 0..3 {
                spawn_one(commands, data, library, meshes, city, index, x, y, level);
            }
        }
    }
}

/// `--damaged` / `--park <kinds>`: park vehicles beside the player once,
/// for visual checks of skins and damage.
fn spawn_damage_check(world: &mut World, mut done: Local<bool>) {
    let park = crate::arg("--park");
    if *done || (park.is_none() && !std::env::args().any(|a| a == "--damaged")) {
        return;
    }
    let player = world
        .query_filtered::<&SimObj, With<crate::camera::CameraTarget>>()
        .iter(world)
        .next()
        .map(|o| o.pos);
    let Some(pos) = player else { return };
    *done = true;
    // `--park 13,9,3`: those vehicle types in a row (wide spacing for
    // buses); otherwise a red Mundaneo and a blue Evo.
    let kinds: Vec<i32> = park
        .map(|p| p.split(',').filter_map(|k| k.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![0, 12]);
    for (i, &kind) in kinds.iter().enumerate() {
        let at = pos + IVec3::new(700 + 1100 * i as i32, 0, 0);
        crate::vehicles::spawn_vehicle_world(world, kind, at, 0, [4, 1][i % 2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_light_states() {
        // special 1: red, red, red+amber, green, green, amber over phases 0..5.
        let seq: Vec<usize> = (0..6).map(|p| traffic_light_model(1, p)).collect();
        assert_eq!(seq, [0, 0, 1, 2, 2, 3]);
        assert_eq!(traffic_light_model(4, 0), 2);
    }
}
