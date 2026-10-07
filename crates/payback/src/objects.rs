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
/// Atlas slot of the flat palette colours (mode 1 faces): colour c is the
/// 8x16 cell at (c % 32, c / 32).
const PALETTE_SLOT: u32 = ATLAS_COLS * ATLAS_ROWS - 1;
/// Fixed UVs of mode-1 faces drawn burnt (`FUN_010c101c`): (2,2), (2,125),
/// (125,125) in texels/rows, here in file units (v doubled).
const BURNT_FIXED_UV: [(i32, i32); 3] = [(2, 4), (2, 250), (125, 250)];
/// Lens-flare size of model faces (`DAT_0114c8dc`, default 300).
const FLARE_SIZE: i32 = 300;

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
            flat_colours,
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
    ) -> Handle<Mesh> {
        let key = (skin, dent.is_some());
        if key == (Skin::Normal, false) {
            return self.models[i].mesh.clone();
        }
        if let Some(h) = self.models[i].variants.get(&key) {
            return h.clone();
        }
        let obj = &self.models[i].obj;
        let offsets: Vec<[f32; 3]> = (0..obj.vertices.len())
            .map(|n| match dent {
                Some(rng) => {
                    let k = 3 * (n + 1);
                    [0, 1, 2].map(|a| (rng.hash(k + a, 160) - 80) as f32 / 16.0)
                }
                None => [0.0; 3],
            })
            .collect();
        let mesh = self.build_mesh_skin(obj, skin, |v, n| {
            let [dx, dy, dz] = offsets[n];
            let (sh, sv) = (2.0 / H_PER_SQUARE, 1.0 / H_PER_SQUARE);
            [
                (v.x as f32 + dx) * sh,
                -(v.z as f32 + dz) * sv,
                (v.y as f32 + dy) * sh,
            ]
        });
        let h = meshes.add(mesh);
        self.models[i].variants.insert(key, h.clone());
        h
    }

    /// Mesh of `obj` with each vertex placed by `place` (render units,
    /// object-local: +X left, +Y up, +Z forward). Vertices are emitted per
    /// face corner, in face order.
    pub fn build_mesh(&self, obj: &Obj, place: impl Fn(&Vertex) -> [f32; 3]) -> Mesh {
        self.build_mesh_skin(obj, Skin::Normal, |v, _| place(v))
    }

    /// Atlas UV of texel (u, v in file units) in slot `t`.
    fn atlas_uv(t: u32, u: i32, v: i32) -> [f32; 2] {
        let ox = (t % ATLAS_COLS * TEX_W) as f32;
        let oy = (t / ATLAS_COLS * TEX_H) as f32;
        [
            (ox + u as f32 + 0.5) / ATLAS_W as f32,
            (oy + v as f32 / 2.0 + 0.5) / ATLAS_H as f32,
        ]
    }

    /// [`ObjectLibrary::build_mesh`] with a skin; `place` also gets the
    /// vertex index.
    fn build_mesh_skin(
        &self,
        obj: &Obj,
        skin: Skin,
        place: impl Fn(&Vertex, usize) -> [f32; 3],
    ) -> Mesh {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let burnt = self.slots.get("burnt").copied();
        for face in &obj.faces {
            let slot = face.active_slot();
            let mode = slot.mode & 0xf;
            let drawn = face.slots.iter().any(|s| s.active)
                && (skin == Skin::Normal || matches!(mode, 1 | 2));
            if !drawn {
                // Not drawn: a degenerate triangle keeps the layout.
                let p = place(&obj.vertices[face.indices[0]], face.indices[0]);
                positions.extend([p; 3]);
                uvs.extend([[0.0; 2]; 3]);
                continue;
            }
            let tex = match skin {
                Skin::Burnt => burnt,
                Skin::Normal => self.slots.get(&slot.texture.to_lowercase()).copied(),
            };
            for (k, (&vi, &(u, v))) in face.indices.iter().zip(&slot.uv).enumerate() {
                positions.push(place(&obj.vertices[vi], vi));
                uvs.push(match (skin, slot.mode, tex) {
                    (Skin::Burnt, _, Some(t)) if mode == 1 => {
                        let (u, v) = BURNT_FIXED_UV[k];
                        Self::atlas_uv(t, u, v)
                    }
                    // Flat colour: the centre of the palette cell.
                    (Skin::Normal, 1, _) => {
                        let c = self.flat_colours[(slot.param & 63) as usize] as u32;
                        let ox = (PALETTE_SLOT % ATLAS_COLS * TEX_W + c % 32 * 8 + 4) as f32;
                        let oy = (PALETTE_SLOT / ATLAS_COLS * TEX_H + c / 32 * 16 + 8) as f32;
                        [ox / ATLAS_W as f32, oy / ATLAS_H as f32]
                    }
                    (_, _, Some(t)) => Self::atlas_uv(t, u, v),
                    // Unknown/empty texture: sample a corner of slot 0.
                    (_, _, None) => [0.5 / ATLAS_W as f32, 0.5 / ATLAS_H as f32],
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
    let place = &place;
    obj.faces
        .iter()
        .flat_map(|f| {
            let drawn = f.slots.iter().any(|s| s.active);
            f.indices.iter().map(move |&vi| {
                let vi = if drawn { vi } else { f.indices[0] };
                place(&obj.vertices[vi])
            })
        })
        .collect()
}

/// A lens-flare face: mode 6 in slot 0 (`FUN_010c101c` calls
/// `AddFlare(vertex 0, DAT_0114c8dc, param << 16)`).
#[derive(Clone, Copy, Debug)]
pub struct FlareFace {
    pub face: usize,
    pub vertex: usize,
    pub table: u8,
}

fn flare_faces(obj: &Obj) -> Vec<FlareFace> {
    obj.faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.slots[0].mode == 6)
        .map(|(i, f)| FlareFace {
            face: i,
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
    /// Burnt / dented meshes, by (skin, dented).
    variants: HashMap<(Skin, bool), Handle<Mesh>>,
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
        app.init_resource::<SceneryIndex>()
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
/// `--damaged [burnt]` forces them, for visual checks.
fn vehicle_skins(
    library: Option<ResMut<ObjectLibrary>>,
    rng: Option<Res<Rng>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&SimObj, Option<&Drive>, &mut Mesh3d), With<Vehicle>>,
) {
    let (Some(mut library), Some(rng)) = (library, rng) else {
        return;
    };
    // `--damaged`: every vehicle dented, `--damaged burnt` also burnt (checks).
    let forced = std::env::args()
        .any(|a| a == "--damaged")
        .then(|| crate::arg("--damaged").unwrap_or_default());
    for (obj, drive, mut mesh) in &mut q {
        let burnt = obj.burn > 0 || forced.as_deref() == Some("burnt");
        let skin = if burnt { Skin::Burnt } else { Skin::Normal };
        let dented = drive.is_some_and(|d| d.dented) || forced.is_some();
        let dent = dented.then_some(&*rng);
        if skin == Skin::Normal && dent.is_none() && mesh.0 == library.model(obj.model).mesh {
            continue;
        }
        let h = library.variant_mesh(&mut meshes, obj.model, skin, dent);
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

/// `--damaged`: park two cars (Mundaneo, Evo) beside the player once, to
/// show the damage skins.
fn spawn_damage_check(world: &mut World, mut done: Local<bool>) {
    if *done || !std::env::args().any(|a| a == "--damaged") {
        return;
    }
    let player = world
        .query_filtered::<&SimObj, With<crate::camera::CameraTarget>>()
        .iter(world)
        .next()
        .map(|o| o.pos);
    let Some(pos) = player else { return };
    *done = true;
    for (i, kind) in [0usize, 12].into_iter().enumerate() {
        let at = pos + IVec3::new(700 + 900 * i as i32, 0, 0);
        crate::vehicles::spawn_vehicle_world(world, kind as i32, at, 0, 0);
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
