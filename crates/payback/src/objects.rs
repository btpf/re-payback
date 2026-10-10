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
//! slot is inactive, `param` = Alpha.256 table). Here each active slot is
//! a triangle facing its side (slot 1 with the reversed winding,
//! `side_order`) and back faces are culled, so the GPU's facing test picks
//! the slot as `FUN_010c101c`'s projected winding does; faces with no
//! active slot are degenerate. Shine
//! mode 0x14 (`CD.obj`) is averaged with the background (Alpha.256 table
//! 4) in software and added in hardware.
//!
//! The Mac release's models are drawn where adopted (`prefer_mac`; the
//! simulation's extents stay the Amiga ones), with open doors
//! (`vehicle_doors`) and shadows (`shadows`); docs/re/rendering.md §10
//! "Mac models, face modes, doors and shadows".
//!
//! Also: traffic lights, crossing beacons and alarm boxes that change model
//! with the light phase, spinning pickups, chimney smoke, lens-flare sources
//! and the burnt / dented variants of vehicles.
//!
//! Hardware renderer (docs/re/rendering.md "Hardware (Warp3D) renderer"):
//! objects are never Gouraud-lit there either; the textures are A1R5G5B5
//! (5 bits per channel, colour 0 transparent), bilinear filtered; at night
//! the darkened textures lose 64 per channel (`FUN_010bf310` re-uploads them
//! with flag `DAT_0114c3b8`) instead of going through `T1[c][0x67]`; the
//! CloudsReflect overlay of mode 0x11 faces is added ONE/ONE instead of
//! table 9.

mod shadows;

use crate::camera::{ExtraView, GameCamera, RotatingView, ViewCams};
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
use std::sync::atomic::{AtomicBool, Ordering};

/// Mirrors of `Settings::variants.scenery` / `.menu_props` for the object
/// loader, which has no world access (kept in step by [`follow_settings`]).
static MAC_SCENERY: AtomicBool = AtomicBool::new(true);
static MAC_MENU_PROPS: AtomicBool = AtomicBool::new(true);

/// Set which `.Obj` set the scenery and the menu props come from (true =
/// the Mac's). Takes effect for models loaded afterwards: the scenery at
/// the next level load.
pub fn set_model_variants(scenery: bool, menu_props: bool) {
    MAC_SCENERY.store(scenery, Ordering::Relaxed);
    MAC_MENU_PROPS.store(menu_props, Ordering::Relaxed);
}

/// Whether `Objects/<path>` (lower case) is taken from the Mac data when
/// it has the file (docs/re/mac_version.md, decisions): the game models
/// (people, vehicles, doors, weapons) always (decision 4), the scenery
/// (shorter phones, traffic lights and tree crowns, the `tanne` fir) and
/// the menu props (`Options/*`) per setting.
fn prefer_mac(path: &str) -> bool {
    if path.starts_with("scenery/") {
        MAC_SCENERY.load(Ordering::Relaxed)
    } else if path.starts_with("options/") {
        MAC_MENU_PROPS.load(Ordering::Relaxed)
    } else {
        true
    }
}

/// Y of the feet of an Amiga people model (the model's max Y; Amiga
/// people have their origin at the hips). The Mac release re-based every
/// `People/*.Obj` by exactly these amounts so the feet sit at Y = 0
/// (vertex diff, `docs/re/mac/gameplay_people.md` M3); its
/// `DrawObject` @ 10075880 kept the Amiga people transform. Rendering
/// either set with this shift gives the Mac picture.
fn amiga_foot_y(path: &str) -> i32 {
    match path {
        "people/malenormal.obj" => 90,
        "people/pedmale4.obj" => 100,
        _ => 92,
    }
}

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

/// Number of paint colours (`random(8)` for traffic, `AddThing` @ 10058350).
pub const PAINT_COUNT: usize = PAINTS.len();

/// The model has faces the paint recolours (`Shine.All` / `Cardef`), so its
/// [`PAINT_COUNT`] colours look different; the others (police, taxi, bus,
/// ...) have a fixed livery.
pub fn paintable(obj: &Obj) -> bool {
    obj.faces.iter().flat_map(|f| f.slots.iter()).any(|s| {
        s.active && (s.texture.eq_ignore_ascii_case("shine.all") || s.texture.eq_ignore_ascii_case("cardef"))
    })
}

/// The object material: unlit atlas texture with environment-mapped faces
/// (`shaders/objects.wgsl`).
pub type ObjectMaterial = ExtendedMaterial<StandardMaterial, EnvMapped>;

/// Vertex-shader extension that maps "Shine" faces by their normal.
#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub struct EnvMapped {
    /// x = 1 for the hardware (Warp3D) look; y = 1 for the game's library,
    /// whose views hold gamma-encoded palette RGB ([`mark_gamma`]).
    #[uniform(100)]
    pub params: Vec4,
}

impl MaterialExtension for EnvMapped {
    fn vertex_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/objects.wgsl".into()
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        gamma_def(descriptor, key.mesh_key);
        Ok(())
    }
}

/// A view whose target holds gamma-encoded palette RGB: the game views'
/// Rgba16Float (HDR) targets (`crate::camera::postfx::GammaWorld`).
pub fn gamma_view(key: bevy::pbr::MeshPipelineKey) -> bool {
    key.target_format() == TextureFormat::Rgba16Float
}

/// `GAMMA_TARGET` for a view whose target holds gamma-encoded palette RGB
/// (the HDR game views, `crate::camera::postfx::GammaWorld`); the shader
/// also checks `params.y`, as the menu-model camera draws into the same
/// HDR texture in linear light.
pub fn gamma_def(descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor, key: bevy::pbr::MeshPipelineKey) {
    if gamma_view(key)
        && let Some(fragment) = descriptor.fragment.as_mut()
    {
        fragment.shader_defs.push("GAMMA_TARGET".into());
    }
}

/// The game's object library draws in the game views: its materials write
/// gamma-encoded palette RGB (`params.y`). The menu's own library
/// (`crate::frontend::model`) is not a resource and stays linear.
fn mark_gamma(library: Option<Res<ObjectLibrary>>, mut materials: ResMut<Assets<ObjectMaterial>>) {
    let Some(library) = library else { return };
    if !library.is_changed() || crate::camera::postfx::linear_blend() {
        return;
    }
    for handle in [&library.material, &library.additive, &library.half, &library.keyed] {
        if materials.get(handle).is_some_and(|m| m.extension.params.y != 1.0)
            && let Some(mut m) = materials.get_mut(handle)
        {
            m.extension.params.y = 1.0;
        }
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Skin {
    #[default]
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
    /// Mode 0x14 faces (`CD.obj`): the environment-mapped slot texture
    /// through Alpha.256 table 4, the 50 % average (`FUN_010c101c` sets
    /// `+0xff4 = DAT_0114c908 + 0x40000` before `FUN_011113e0`); Warp3D
    /// adds them ONE/ONE (rendering.md §11.6). See [`half_material`].
    half: Handle<ObjectMaterial>,
    /// Mode 3 and the Genlock shine faces: texel 0 cut out (software
    /// `Genlock`), blended by the filtered key on the card. See
    /// [`keyed_material`].
    keyed: Handle<ObjectMaterial>,
    /// Night map: vehicles get the `Lights` headlight polygon.
    night: bool,
    /// The atlas holds the hardware conversion of the textures.
    hardware: bool,
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
    half: Option<Handle<Mesh>>,
    keyed: Option<Handle<Mesh>>,
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
    /// Reorder the triangles back to front for the overhead camera, as
    /// the painter's sort of flagged models does (CONFIRMED keys: moving
    /// objects the sum of the three vertex depths, insertion-sorted;
    /// scenery once by the deepest vertex). Depth is taken along the
    /// vertical of the unturned model (render −y), so the order is the
    /// top-down one; the original re-sorts objects when they pitch or roll
    /// and sorts by true camera depth in the rotating view (not done).
    /// Stable, as the insertion sort.
    fn painter_sort(&mut self, scenery: bool) {
        let n = self.positions.len() / 3;
        if n < 2 {
            return;
        }
        let key = |t: usize| -> f32 {
            let y = [0, 1, 2].map(|k| self.positions[3 * t + k][1]);
            if scenery { -y[0].min(y[1]).min(y[2]) } else { -(y[0] + y[1] + y[2]) }
        };
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| key(b).total_cmp(&key(a)));
        let pick = |v: &Vec<[f32; 3]>| order.iter().flat_map(|&t| [v[3 * t], v[3 * t + 1], v[3 * t + 2]]).collect();
        let pick2 = |v: &Vec<[f32; 2]>| order.iter().flat_map(|&t| [v[3 * t], v[3 * t + 1], v[3 * t + 2]]).collect();
        self.positions = pick(&self.positions);
        self.normals = pick(&self.normals);
        self.uvs = pick2(&self.uvs);
        self.env = pick2(&self.env);
    }

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
    half: Option<Mesh>,
    keyed: Option<Mesh>,
    lines: Option<Mesh>,
}

/// The mode-0x14 material's blend: software averages (Alpha.256 table 4,
/// as alpha 0.5), hardware adds ONE/ONE.
fn half_material(m: &mut ObjectMaterial, hardware: bool) {
    let a = if hardware { 1.0 } else { 0.5 };
    m.base.base_color = Color::srgba(1.0, 1.0, 1.0, a);
    m.base.alpha_mode = if hardware { AlphaMode::Add } else { AlphaMode::Blend };
}

/// The colour-keyed faces' blend: a hard cut at texel 0 in software
/// (`Genlock`); in hardware `W3D_SetBlendMode(SRC_ALPHA,
/// ONE_MINUS_SRC_ALPHA)` with the A1R5G5B5 key bilinear filtered, so the
/// edges fade (Mac `DrawObject` @ 10075880 mode 3 and Genlock shines).
fn keyed_material(m: &mut ObjectMaterial, hardware: bool) {
    m.base.alpha_mode = if hardware { AlphaMode::Blend } else { AlphaMode::Mask(0.5) };
}

/// The object texture atlas (`ObjectTextures/*`, 256x128 each, file-name
/// order; the flat palette colours in the last slot; an opaque copy in the
/// bottom half) and the slot of every lower-cased name. Night: `Shine.*`
/// (not the cups) and `CloudsReflect` swap in their `.dark` files, a few
/// textures are left alone and the rest are darkened (rendering.md §4,
/// OS4 `FUN_010bf310`): through `T1[c][0x67]` in software, by 64 per
/// channel on the Warp3D card. `hardware` also cuts the channels to the 5
/// bits of A1R5G5B5 and filters bilinearly.
fn build_atlas(data: &GameData, night: bool, hardware: bool) -> (Image, HashMap<String, u32>) {
    let mut rgba = vec![0u8; (ATLAS_W * ATLAS_FULL_H * 4) as usize];
    let mut slots = HashMap::new();
    // The Amiga set plus the Mac-only textures (the PC, keyboard, monitor,
    // disk and movie-reel props and the `tanne` fir of the Mac `Tree.Obj`);
    // the names both ship are byte-identical.
    let mut entries = data.list_dir("ObjectTextures", true, false);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut night_remap: Option<[u8; 256]> = None;
    for (file, entry) in entries {
        let slot = slots.len() as u32;
        if slot >= PALETTE_SLOT {
            warn!("object texture atlas full");
            break;
        }
        let name = file.to_lowercase();
        // Night (rendering.md §4, OS4 `FUN_010bf310`): Shine.* (not the
        // cups) and CloudsReflect swap in their `.dark` files; a few
        // textures are left alone; the rest are darkened.
        let swap = night
            && ((name.starts_with("shine.") && !name.starts_with("shine.cup"))
                || name == "cloudsreflect")
            && !name.ends_with(".dark");
        let path = if swap {
            entry.with_file_name(format!("{file}.dark"))
        } else {
            entry.clone()
        };
        let read = |p: &std::path::Path| {
            crate::vfs::read(p)
                .map_err(|e| e.to_string())
                .and_then(|b| IndexedImage::object_texture(&b).map_err(|e| e.to_string()))
        };
        let Ok(mut img) = read(&path).or_else(|_| read(&entry)) else {
            continue;
        };
        // Everything else through T1[c][0x67] (colour 0 unchanged); on the
        // card 64 less per channel.
        let keep = ["joystick", "red shade", "lights", "tankbits"];
        let darken = night && !swap && !name.ends_with(".dark") && !keep.contains(&name.as_str());
        if darken && !hardware {
            let remap = night_remap.get_or_insert_with(|| crate::camera::postfx::night_remap(data));
            for c in &mut img.pixels {
                *c = remap[*c as usize];
            }
        }
        let mut pixels = img.to_rgba(&data.palette, Some(0));
        if hardware {
            for px in pixels.chunks_mut(4) {
                for v in &mut px[..3] {
                    if darken {
                        *v = v.saturating_sub(64);
                    }
                    *v = (*v & 0xf8) | (*v >> 5);
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
    atlas.sampler = ImageSampler::Descriptor(if hardware {
        ImageSamplerDescriptor::linear()
    } else {
        ImageSamplerDescriptor::nearest()
    });
    (atlas, slots)
}

/// Keep the loader's model-set switches in step with the settings.
fn follow_settings(settings: Option<Res<crate::frontend::progress::Settings>>) {
    if let Some(s) = settings.filter(|s| s.is_changed()) {
        set_model_variants(s.variants.scenery, s.variants.menu_props);
    }
}

/// Follow the renderer: rebuild the object atlas and set the shader flag.
fn sync_mode(
    mode: Res<crate::render_mode::RenderMode>,
    data: Res<GameData>,
    library: Option<ResMut<ObjectLibrary>>,
    mut materials: ResMut<Assets<ObjectMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut library) = library else { return };
    let hardware = mode.hardware();
    if library.hardware == hardware {
        return;
    }
    library.hardware = hardware;
    let (atlas, _) = build_atlas(&data, library.night, hardware);
    let flag = if hardware { 1.0 } else { 0.0 };
    for handle in [
        library.material.clone(),
        library.additive.clone(),
        library.half.clone(),
        library.keyed.clone(),
    ] {
        let Some(mut m) = materials.get_mut(&handle) else { continue };
        m.extension.params.x = flag;
        if handle == library.half {
            half_material(&mut *m, hardware);
        }
        if handle == library.keyed {
            keyed_material(&mut *m, hardware);
        }
        if let Some(image) = m.base.base_color_texture.clone()
            && let Some(mut slot) = images.get_mut(&image)
        {
            *slot = atlas.clone();
        }
    }
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
        let hardware = crate::render_mode::is_hardware();
        let (atlas, slots) = build_atlas(data, night, hardware);
        let mut flat_colours = [0u8; 64];
        match data
            .read("General/64->256.dat")
            .or_else(|_| data.read("General/64-_256.dat"))
        {
            Ok(b) if b.len() >= 64 => flat_colours.copy_from_slice(&b[..64]),
            _ => warn!("missing General/64->256.dat"),
        }
        let base = StandardMaterial {
            base_color_texture: Some(images.add(atlas)),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            // Faces are one-sided; see `face_sides`.
            double_sided: false,
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        };
        let extension = EnvMapped {
            params: Vec4::new(if hardware { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
        };
        let material = materials.add(ExtendedMaterial {
            base: base.clone(),
            extension: extension.clone(),
        });
        let additive = materials.add(ExtendedMaterial {
            base: StandardMaterial {
                alpha_mode: AlphaMode::Add,
                ..base.clone()
            },
            extension: extension.clone(),
        });
        let mut keyed = ExtendedMaterial {
            base: base.clone(),
            extension: extension.clone(),
        };
        keyed_material(&mut keyed, hardware);
        let keyed = materials.add(keyed);
        let mut half = ExtendedMaterial { base, extension };
        half_material(&mut half, hardware);
        let half = materials.add(half);
        ObjectLibrary {
            slots,
            flat_colours,
            material,
            additive,
            half,
            keyed,
            night,
            hardware,
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
        let rel = format!("Objects/{path}");
        let parse = |b: Vec<u8>| match Obj::parse(&b) {
            Ok(o) => Some(o),
            Err(e) => {
                warn!("{path}: {e}");
                None
            }
        };
        // The Amiga file (or the Mac one for the Mac-only props) and the
        // Mac file drawn instead of it (`prefer_mac`).
        let amiga = data.read(&rel).ok().and_then(parse);
        let mac = data
            .mac_path(&rel)
            .filter(|_| prefer_mac(&key))
            .and_then(|p| crate::vfs::read(p).ok())
            .and_then(parse);
        let index = amiga.clone().or_else(|| mac.clone()).map(|base| {
            // The simulation reads the model extents (radius, front and back
            // extents, ground mean: collisions, doors, AI), so they always
            // come from the Amiga file: the Mac re-pivoted people and four
            // cars, which would move them and break replays.
            let info = ModelInfo::from_obj(&key, &base);
            let from_mac = mac.is_some();
            let obj = mac.unwrap_or(base);
            let shape = ModelInfo::from_obj(&key, &obj);
            let foot = if key.starts_with("people/") && !from_mac { amiga_foot_y(&key) } else { 0 };
            let parts = self.build_parts(&obj, Skin::Normal, None, &shape, |p, _| static_vertex(p));
            let mesh = self.add_parts(meshes, parts);
            let flares = flare_faces(&obj);
            self.models.push(LoadedModel {
                obj,
                mesh,
                info,
                shape,
                foot,
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

    /// Make the next [`ObjectLibrary::load`] of the paths starting with
    /// `prefix` read the file again (the menu props after the
    /// Mac/Amiga switch). Indices handed out before stay valid.
    pub fn forget(&mut self, prefix: &str) {
        self.by_path.retain(|k, _| !k.starts_with(prefix));
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
        let info = &self.models[i].shape;
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
        self.build_person_mesh(obj, 0, place)
    }

    /// [`ObjectLibrary::build_mesh`] for player `sand`'s person (0 = red,
    /// the model's own; see `SANDS`).
    pub fn build_person_mesh(&self, obj: &Obj, sand: usize, place: impl Fn(&Vertex) -> [f32; 3]) -> Mesh {
        let look = Look {
            sand,
            keep_keyed: true,
            ..Look::default()
        };
        self.build_parts_with(obj, look, None, |v, _| place(v)).main
    }

    /// A menu model (`frontend::model`): the main mesh and the blended
    /// parts (additive, mode 0x14, lines) with their materials. The front
    /// end draws objects with `FUN_010c101c(obj, 0, 0, 0, 0x65)`, so mode 4
    /// is additive there too (table 0).
    pub fn menu_parts(
        &self,
        obj: &Obj,
        place: impl Fn(&Vertex) -> [f32; 3],
    ) -> (Mesh, Vec<(Mesh, Handle<ObjectMaterial>)>) {
        self.menu_parts_painted(obj, None, place)
    }

    /// [`Self::menu_parts`] with paint `paint` (index into [`PAINTS`]).
    pub fn menu_parts_painted(
        &self,
        obj: &Obj,
        paint: Option<usize>,
        place: impl Fn(&Vertex) -> [f32; 3],
    ) -> (Mesh, Vec<(Mesh, Handle<ObjectMaterial>)>) {
        let look = Look { paint: paint.filter(|&p| p < PAINTS.len()), ..Look::default() };
        let parts = self.build_parts_with(obj, look, None, |v, _| place(v));
        let overlays = [
            (parts.additive, &self.additive),
            (parts.half, &self.half),
            (parts.keyed, &self.keyed),
            (parts.lines, &self.material),
        ]
        .into_iter()
        .filter_map(|(m, mat)| m.map(|m| (m, mat.clone())))
        .collect();
        (parts.main, overlays)
    }

    /// Atlas UV of palette colour `c` in the flat-colour slot (mode 1 and
    /// mode 5 faces).
    fn palette_uv(c: u32) -> [f32; 2] {
        let ox = (PALETTE_SLOT % ATLAS_COLS * TEX_W + c % 32 * 8 + 4) as f32;
        let oy = (PALETTE_SLOT / ATLAS_COLS * TEX_H + c / 32 * 16 + 8) as f32;
        [ox / ATLAS_W as f32, oy / ATLAS_FULL_H as f32]
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

    /// Add a mesh and remember its overlay parts.
    fn add_parts(&mut self, meshes: &mut Assets<Mesh>, parts: Parts) -> Handle<Mesh> {
        let h = meshes.add(parts.main);
        let overlay = Overlay {
            additive: parts.additive.map(|m| meshes.add(m)),
            half: parts.half.map(|m| meshes.add(m)),
            keyed: parts.keyed.map(|m| meshes.add(m)),
            lines: parts.lines.map(|m| meshes.add(m)),
        };
        if overlay.additive.is_some()
            || overlay.half.is_some()
            || overlay.keyed.is_some()
            || overlay.lines.is_some()
        {
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
        let look = Look {
            skin,
            paint,
            scenery: info.path.starts_with("scenery/"),
            ..Look::default()
        };
        self.build_parts_with(obj, look, lights, place)
    }

    fn build_parts_with(
        &self,
        obj: &Obj,
        look: Look,
        headlights: Option<(&ModelInfo, u32)>,
        place: impl Fn(&Vertex, usize) -> [f32; 3],
    ) -> Parts {
        let Look {
            skin,
            paint,
            scenery,
            sand,
            keep_keyed,
        } = look;
        let mut main = Tris::default();
        let mut add = Tris::default();
        let mut halves = Tris::default();
        let mut keyed_tris = Tris::default();
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
        let palette_uv = |c: u32| {
            let ox = (PALETTE_SLOT % ATLAS_COLS * TEX_W + c % 32 * 8 + 4) as f32;
            let oy = (PALETTE_SLOT / ATLAS_COLS * TEX_H + c / 32 * 16 + 8) as f32;
            [ox / ATLAS_W as f32, oy / ATLAS_FULL_H as f32]
        };
        for (face, side) in face_sides(obj) {
            let p0 = || place(&obj.vertices[face.indices[0]], face.indices[0]);
            let Some(side) = side else {
                // Not drawn: a degenerate triangle keeps the layout.
                main.degenerate(p0());
                // Slot 0 mode 5, whatever its `active` byte: a line between
                // the first two vertices, colour `param` through 64->256
                // like mode 1, black when burnt (Mac `DrawObject` @ 10075880:
                // `DrawLine(…, param)`, burnt pass `DrawLine(…, 0)`).
                let slot = &face.slots[0];
                if slot.mode == 5 {
                    let c = match skin {
                        Skin::Normal => self.flat_colours[(slot.param & 63) as usize] as u32,
                        Skin::Burnt => 0,
                    };
                    for k in [0, 1] {
                        let vi = face.indices[k];
                        lines.positions.push(place(&obj.vertices[vi], vi));
                        lines.uvs.push(palette_uv(c));
                        lines.env.push([-1.0; 2]);
                        lines.normals.push([0.0, 1.0, 0.0]);
                    }
                }
                continue;
            };
            let slot = &face.slots[side];
            let mode = slot.mode & 0xf;
            if skin == Skin::Burnt && !matches!(mode, 1 | 2) {
                main.degenerate(p0());
                continue;
            }
            let shine = if skin == Skin::Normal { shine_kind(slot.mode) } else { None };
            // Shine modes other than 1, 2 and 4 (0x10, 0x13, 0x15, 0x30 …)
            // go through `Genlock` (texel 0 transparent) in software and
            // `(7, 8)` in hardware (`DrawObject`'s shine branches).
            let genlock = shine.is_some() && !matches!(mode, 1 | 2 | 4);
            // Shine mode 4 (0x14, 0x34; `uVar50 == 4` in `FUN_010c101c`).
            let quarter = shine.is_some() && mode == 4;
            // Plain mode 4 goes to the additive overlay; the main mesh keeps
            // a degenerate triangle in its place. Without the 0x10 bit, 0x20
            // also takes the software renderer's mode-4 branch (`FUN_010c101c`:
            // mode 0x22, the "White sand" leg faces of PedMale1-4).
            let additive = mode == 4 || slot.mode & 0x20 != 0;
            // Mode 3 and the Genlock shines key colour 0 out.
            let keyed = skin == Skin::Normal && (slot.mode == 3 || genlock);
            let out = if additive && shine.is_none() && skin == Skin::Normal {
                main.degenerate(p0());
                &mut add
            } else if quarter && scenery {
                // `DrawScenery` @ 10079e90 draws shine mode 4 through table 0
                // (additive), `DrawObject` through table 4.
                main.degenerate(p0());
                &mut add
            } else if quarter {
                main.degenerate(p0());
                &mut halves
            } else if keyed && !keep_keyed {
                // A hard mask in software; on the card the filtered colour
                // key is blended `(7, 8)` (soft edges), see `keyed_material`.
                main.degenerate(p0());
                &mut keyed_tris
            } else {
                &mut main
            };
            let order = side_order(side);
            let [a, b, c] = order.map(|k| face.indices[k]);
            let flat = (placed[b] - placed[a]).cross(placed[c] - placed[a]).normalize_or_zero();
            let smooth_env = slot.mode & 0x20 != 0;
            let tex = match skin {
                Skin::Burnt => burnt,
                Skin::Normal => {
                    let name = slot.texture.to_lowercase();
                    let name = match (paint.and_then(|p| PAINTS.get(p)), name.as_str()) {
                        (Some(&(shine, _)), "shine.all") => shine,
                        (Some(&(_, cardef)), "cardef") => cardef,
                        // Player n's people (instance `+0x99c`, OS4 `+0x4b4`)
                        // wear sand n: both red sands become its light one
                        // (`DrawObject`: `_DAT_1017c6a4[n]` for both).
                        (_, "red sand" | "red dark sand") if sand > 0 && sand < SANDS.len() => SANDS[sand],
                        _ => name.as_str(),
                    };
                    self.slots.get(name).copied()
                }
            };
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
                        // Genlock shines sample the keyed copy.
                        let half = if genlock { 0 } else { ATLAS_H };
                        let ox = (t % ATLAS_COLS * TEX_W) as f32 + cu + 0.5;
                        let oy = (t / ATLAS_COLS * TEX_H + half) as f32 + cv + 0.5;
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
                    (Skin::Normal, 1, _) => palette_uv(self.flat_colours[(slot.param & 63) as usize] as u32),
                    (_, _, Some(t)) => Self::atlas_uv(t, u, v, keyed),
                    // Unknown/empty texture: sample a corner of slot 0.
                    (_, _, None) => [0.5 / ATLAS_W as f32, 0.5 / ATLAS_FULL_H as f32],
                });
            }
        }
        if let Some((info, tex)) = headlights {
            headlight_polygon(&mut add, info, tex);
        }
        // `.Obj` flag 1 (model `+0x490c`, loader `FUN_010bf8bc`): the faces
        // are drawn in painter's order, deepest first (`FUN_010c101c`,
        // `FUN_010c4fec`, `FUN_010c7218`). With the depth test only the
        // blended overlays depend on the order (the 50 % faces and, in
        // hardware, the alpha-blended colour-keyed faces such as tree
        // canopies, which do not write depth); flag 0 keeps file order.
        if obj.flag == 1 {
            halves.painter_sort(scenery);
            keyed_tris.painter_sort(scenery);
        }
        let nonempty = |t: Tris, topology| (!t.positions.is_empty()).then(|| t.mesh(topology));
        Parts {
            main: main.mesh(PrimitiveTopology::TriangleList),
            additive: nonempty(add, PrimitiveTopology::TriangleList),
            half: nonempty(halves, PrimitiveTopology::TriangleList),
            keyed: nonempty(keyed_tris, PrimitiveTopology::TriangleList),
            lines: nonempty(lines, PrimitiveTopology::LineList),
        }
    }
}

/// How a model mesh is built.
#[derive(Clone, Copy, Debug, Default)]
struct Look {
    skin: Skin,
    /// Index into [`PAINTS`] for `Shine.All` / `Cardef` faces.
    paint: Option<usize>,
    /// Drawn by `DrawScenery` (shine mode 4 additive).
    scenery: bool,
    /// Player number of a person (`SANDS`).
    sand: usize,
    /// Keep the colour-keyed faces in the main mesh (people: re-posing only
    /// moves the main mesh).
    keep_keyed: bool,
}

/// The light sand of player n's people (`InitObjects` @ 10072d50 loads
/// `_DAT_1017c6a4[0..4]`); player 0 keeps red.
const SANDS: [&str; 4] = ["red sand", "green sand", "yellow sand", "blue sand"];

/// Model-space vertex to render units for ordinary (non-people) models:
/// one model unit = 2 h-units horizontally; vertically the top-down view
/// draws 1 h-unit per model unit (rendering.md §0). Model z is downward.
pub fn static_vertex(p: &Vertex) -> [f32; 3] {
    let (sh, sv) = (2.0 / H_PER_SQUARE, 1.0 / H_PER_SQUARE);
    [p.x as f32 * sh, -p.z as f32 * sv, p.y as f32 * sh]
}

/// `uv_b` of the headlight beam's vertices: `objects.wgsl` gives the
/// points below the model's origin plane the depth of that plane (the
/// original has no z-buffer, so the beam is never cut by the road when the
/// body leans in a turn or dips under the throttle).
const BEAM_ENV: [f32; 2] = [-2.0, -2.0];

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
            // Marks the beam for the shader's depth (`objects.wgsl`).
            out.env.push(BEAM_ENV);
            out.normals.push([0.0, 1.0, 0.0]);
        }
    }
}

/// Overlay parts of an entity (see [`Overlay`]): render-only entities that
/// follow it ([`Follow`]). On every [`SimObj`] from the spawn (required
/// there; render code must not change a simulated object's components),
/// inserted on scenery when first needed.
#[derive(Component, Default)]
pub(crate) struct OverlayChildren(Vec<Entity>);

/// A render-only entity drawn at its owner's transform times `local`, with
/// the owner's visibility. Used instead of `ChildOf` for parts of simulated
/// objects: a child would give the object a `Children` component at a
/// frame-dependent moment (and Bevy removes it again with the last child).
/// Despawned once the owner is gone.
#[derive(Component)]
pub(crate) struct Follow {
    owner: Entity,
    local: Transform,
}

/// Place [`Follow`] entities (before transform propagation).
#[allow(clippy::type_complexity)]
fn follow_owners(
    mut commands: Commands,
    owners: Query<(&Transform, Option<&Visibility>), Without<Follow>>,
    mut q: Query<(Entity, &Follow, &mut Transform, &mut Visibility)>,
) {
    for (e, f, mut tf, mut vis) in &mut q {
        let Ok((otf, ovis)) = owners.get(f.owner) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let want = otf.mul_transform(f.local);
        if *tf != want {
            *tf = want;
        }
        let v = ovis.copied().unwrap_or_default();
        if *vis != v {
            *vis = v;
        }
    }
}

/// Vertical scale of the object meshes relative to the horizontal
/// ([`static_vertex`]: 1 h-unit per model unit up, 2 across).
const MODEL_SQUASH: Vec3 = Vec3::new(1.0, 0.5, 1.0);

/// World matrix of a tilted model: the original turns the vertices in
/// uniform model units (roll, pitch, heading) and only then maps model z
/// at 1 h-unit per unit (Mac `DrawObject` @ 10075880: depth `+0x08 +
/// rotated z`, x/y `pos·256 + rotated·256`; rendering.md §0). The meshes
/// carry that squash already, so the rotation is applied as
/// `squash · R · squash⁻¹` instead of `R`; a pure heading commutes with
/// the squash and is unchanged. The sim's pitch and roll are
/// `arctan(Δz / length)` with z at 16 per h-unit, twice the h-scale of
/// x/y, so a plain `R` tilts a car twice as steeply as the slope it
/// stands on and buries its downhill end.
fn tilted_affine(tf: &Transform) -> bevy::math::Affine3A {
    use bevy::math::Affine3A;
    Affine3A::from_translation(tf.translation)
        * Affine3A::from_scale(MODEL_SQUASH)
        * Affine3A::from_quat(tf.rotation)
        * Affine3A::from_scale(MODEL_SQUASH.recip())
        * Affine3A::from_scale(tf.scale)
}

/// Replace the propagated world matrix of vehicles and props (the objects
/// whose pitch and roll come from the ground) and of their [`Follow`]
/// parts with [`tilted_affine`]. Render-only (after transform
/// propagation; writes `GlobalTransform` values only).
#[allow(clippy::type_complexity)]
fn tilt_squashed_models(
    mut owners: Query<
        (&Transform, &mut GlobalTransform),
        (Or<(With<Vehicle>, With<crate::vehicles::Prop>)>, Without<Follow>),
    >,
    mut parts: Query<(&Follow, &mut GlobalTransform), Without<Vehicle>>,
) {
    for (tf, mut g) in &mut owners {
        let want = GlobalTransform::from(tilted_affine(tf));
        if *g != want {
            *g = want;
        }
    }
    for (f, mut g) in &mut parts {
        let Ok((otf, _)) = owners.get(f.owner) else { continue };
        let want = GlobalTransform::from(tilted_affine(otf) * f.local.compute_affine());
        if *g != want {
            *g = want;
        }
    }
}

/// Keep each object's overlay parts (additive faces, lines) in step with
/// its mesh.
#[allow(clippy::type_complexity)]
fn sync_overlays(
    mut commands: Commands,
    library: Option<Res<ObjectLibrary>>,
    mut q: Query<
        (Entity, &Mesh3d, Option<&mut OverlayChildren>),
        (With<MeshMaterial3d<ObjectMaterial>>, Changed<Mesh3d>),
    >,
) {
    let Some(library) = library else { return };
    for (entity, mesh, mut old) in &mut q {
        if let Some(old) = old.as_deref_mut() {
            for e in old.0.drain(..) {
                commands.entity(e).try_despawn();
            }
        }
        let Some(overlay) = library.overlays.get(&mesh.0.id()) else {
            continue;
        };
        let mut children = Vec::new();
        for (part, material) in [
            (&overlay.additive, &library.additive),
            (&overlay.half, &library.half),
            (&overlay.keyed, &library.keyed),
            (&overlay.lines, &library.material),
        ] {
            if let Some(part) = part {
                let child = commands
                    .spawn((
                        Mesh3d(part.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::IDENTITY,
                        Follow {
                            owner: entity,
                            local: Transform::IDENTITY,
                        },
                    ))
                    .id();
                children.push(child);
            }
        }
        match old {
            Some(mut old) => old.0 = children,
            None => {
                commands.entity(entity).insert(OverlayChildren(children));
            }
        }
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
            // Slot 0 mode 5 / 6 makes a line / a lens flare instead of a
            // triangle (Mac `DrawObject` @ 10075880).
            _ if matches!(f.slots[0].mode, 5 | 6) => vec![None],
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
    /// The model as drawn (the Mac file where adopted, see `prefer_mac`).
    pub obj: Obj,
    pub mesh: Handle<Mesh>,
    /// Extents and constants the simulation uses (from the Amiga file).
    pub info: ModelInfo,
    /// Extents of the drawn model (`obj`), for drawing: the headlight
    /// polygon, door hinges and shadows of the original are computed from
    /// the loaded model.
    pub shape: ModelInfo,
    /// Model Y of the feet of a person (`amiga_foot_y`; 0 for the Mac
    /// people, whose origin is at the feet).
    pub foot: i32,
    /// Lens-flare faces.
    pub flares: Vec<FlareFace>,
    /// Burnt / dented / painted meshes, by (skin, dented, paint).
    variants: HashMap<(Skin, bool, Option<usize>), Handle<Mesh>>,
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
        embedded_asset!(app, "shaders/shadows.wgsl");
        shadows::plugin(app);
        app.add_plugins(MaterialPlugin::<ObjectMaterial>::default())
            .init_resource::<SceneryIndex>()
            .init_resource::<FlareOccluders>()
            .init_resource::<SceneryClock>()
            .init_resource::<RotorPhases>()
            .add_message::<SetScenery>()
            .add_systems(FixedPostUpdate, apply_scenery_changes)
            .add_systems(
                FixedUpdate,
                (advance_clock, advance_rotors, chimney_smoke).run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                OnExit(AppState::Playing),
                |mut c: ResMut<SceneryClock>, mut r: ResMut<RotorPhases>| {
                    *c = SceneryClock::default();
                    r.0.clear();
                },
            )
            .add_systems(
                Update,
                (follow_settings, animate_scenery, (vehicle_skins, vehicle_doors).chain(), sync_mode, mark_gamma),
            )
            .add_systems(
                PostUpdate,
                (sync_overlays, pose_rotors, follow_owners)
                    .chain()
                    .after(crate::frame::SmoothSet)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                FixedUpdate,
                spawn_damage_check.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                PostUpdate,
                tilt_squashed_models
                    .after(TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds)
                    .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
                    .before(collect_flares),
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
    /// Render height of the cube top it stands on (the model origin).
    pub ground: f32,
}

/// How far (render units) scenery turned by `rot` must be raised so that
/// no vertex is below its origin. The original draws scenery at its cube
/// top, after that cube, in painter's order (Mac `DrawMap` @ 1005a260
/// passes `height << 4` to `DrawScenery`, no offset), so a model reaching
/// below its origin still shows whole over the ground: the tumbling `Info`
/// sign (z ±8, y ±28, centred on the origin) and the `Briefcase` (z ±6).
/// The port's depth test would cut those parts off; lifting by the depth
/// they reach keeps them visible (port approximation: a slightly nearer
/// projection than the original's).
fn sunk_lift(model: &LoadedModel, rot: Quat) -> f32 {
    let low = model
        .obj
        .vertices
        .iter()
        .map(|v| (rot * Vec3::from_array(static_vertex(v))).y)
        .fold(0.0f32, f32::min);
    -low
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
#[derive(Resource, Clone, Copy, Debug)]
pub struct SceneryClock {
    /// Pickup spin (`DAT_0034c09c` / `DAT_0114c770`), +60 per tick.
    pub spin: i32,
    /// Info sign tumble: the X–Z-plane angle (`DAT_0034c0a4` / OS4
    /// `DAT_0114c76c`, +30 per tick, initially 20) and the Y–Z-plane angle
    /// (`DAT_0034c0a0` / `DAT_0114c768`, −45, initially 5).
    pub tumble: [i32; 2],
    /// Alarm-box pulse 0..19 (`DAT_012e5adc`, +1 per tick).
    pub pulse: i32,
}

impl Default for SceneryClock {
    fn default() -> Self {
        SceneryClock {
            spin: 0,
            tumble: [20, 5],
            pulse: 0,
        }
    }
}

/// The alarm pulse advances in every view's city pass (`FUN_01049374`
/// adds the frame's ticks to `DAT_012e5adc`), so once per view per tick.
fn advance_clock(mut c: ResMut<SceneryClock>, views: Res<ViewCams>) {
    c.spin = (c.spin + 60) % 5760;
    c.tumble = [(c.tumble[0] + 30) % 5760, (c.tumble[1] + 5760 - 45) % 5760];
    c.pulse = (c.pulse + views.0.len().max(1) as i32) % 20;
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
                    // `FUN_010c7218`: the X–Z plane (about the model's y =
                    // render z) by `c76c` first, then the Y–Z plane (about
                    // x) by `c768`, then the yaw.
                    let [xz, yz] = clock
                        .tumble
                        .map(|t| t as f32 / 5760.0 * std::f32::consts::TAU);
                    rot = rot * Quat::from_rotation_x(yz) * Quat::from_rotation_z(xz);
                    tf.translation.y = sc.ground + sunk_lift(library.model(sc.model), rot);
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

/// Chimneys in view smoke (scenery 0x11): every view's city pass (once per
/// view in split screen) gives each chimney in that view's square range
/// (`ViewEyes::visible_squares`) a chance `random3(3) == 1` per tick of a
/// type 6 Dust2 puff 2500 z-units above its base, with its own numbers
/// drawn last argument first (Mac `DrawMap` @ 1005b0b8, rendering C2/D5).
/// OS4 `FUN_01049374` drew one set from the game generator per view and
/// puffed every chimney in step; the Mac's come from the cosmetic
/// generator, so the chimneys smoke independently.
fn chimney_smoke(
    eyes: Res<crate::camera::ViewEyes>,
    rotating: Res<RotatingView>,
    rng: Option<ResMut<crate::sim::VisualRng>>,
    mut effects: ResMut<Effects>,
    city: Option<Res<City>>,
    q: Query<(&Scenery, &Transform)>,
) {
    let (Some(mut rng), Some(city)) = (rng, city) else {
        return;
    };
    // The views' eyes (rotating view, replay TV camera; camera.rs).
    for (i, view) in eyes.cams.iter().enumerate() {
        let (hw, hh) = eyes.visible_squares(i, rotating.0);
        let centre = view.square();
        for (sc, tf) in &q {
            if sc.id != sid::CHIMNEY {
                continue;
            }
            let (x, y) = (tf.translation.x, tf.translation.z);
            let (sx, sy) = (x.floor() as i32, y.floor() as i32);
            if (sx - centre.x).abs() > hw || (sy - centre.y).abs() > hh {
                continue;
            }
            if rng.rand(3) != 1 {
                continue;
            }
            let size = (rng.rand(200) + 300) as f32;
            let vz = rng.rand(30) - 250;
            let vy = rng.rand(30) - 15;
            let vx = rng.rand(30) + 150;
            let dz = rng.rand(300) - 2500;
            let dy = rng.rand(100) - 50;
            let dx = rng.rand(100) - 50;
            let level = city
                .floor(x * 2048.0, y * 2048.0, tf.translation.y * H_PER_SQUARE, 1.0)
                .map_or(0, |(_, l, _)| l);
            let base = z_of_h(tf.translation.y * H_PER_SQUARE);
            let pos = Vec3::new(
                x.floor() * 2048.0 + 0x400 as f32 + dx as f32,
                y.floor() * 2048.0 + 0x400 as f32 + dy as f32,
                base + dz as f32,
            );
            let vel = Vec3::new(vx as f32, vy as f32, vz as f32);
            effects.add(6, Tex::Dust2, pos, vel, size, level as u8, Blend::Subtract);
        }
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
    let forced = crate::args()
        .any(|a| a == "--damaged")
        .then(|| crate::arg("--damaged").unwrap_or_default());
    for (obj, vehicle, drive, mut mesh) in &mut q {
        let paint = usize::try_from(vehicle.colour).ok().filter(|&c| c < PAINTS.len());
        let burnt = obj.burn > 0 || forced.as_deref() == Some("burnt");
        let skin = if burnt { Skin::Burnt } else { Skin::Normal };
        // The helicopter always takes the animated vertex path (its rotor
        // phase is never -1 once `do_Other` has run), and `DrawObject`
        // displaces dented vertices only on the static path (Mac
        // `DrawObject` @ 10075880, `+0x93a` test): it never looks dented.
        let dented = (drive.is_some_and(|d| d.dented) || forced.as_deref().is_some_and(|f| f != "clean"))
            && vehicle.kind != crate::vehicles::kind::HELICOPTER;
        let dent = dented.then_some(&*rng);
        let h = library.variant_mesh(&mut meshes, obj.model, skin, dent, paint);
        if mesh.0 != h {
            mesh.0 = h;
        }
    }
}

/// Door model of a vehicle type (`+0x8158`, OS4 `+0x48fc`; the loader's
/// table, people.md §6.2).
fn door_model(kind: usize) -> Option<&'static str> {
    use crate::vehicles::Door;
    Some(match crate::vehicles::TYPES.get(kind)?.door {
        Door::Default => "doors/default.obj",
        Door::Police => "doors/police.obj",
        Door::Bus => "doors/bus.obj",
        Door::Taxi => "doors/taxi.obj",
        Door::Without => return None,
    })
}

/// Lateral distance of the door hinge from the centre line, ×16 units
/// (`+0x8150` = −that). `LoadStructure` @ 10074630 sets −max|X|;
/// `InitStructures` @ 1005ffb0 (OS4 `FUN_0104e13c`) divides the
/// helicopter's max|X| (the rotor square's corner) by 3 first, so its door
/// hangs at the cabin, not out at the rotor tip.
fn door_hinge(kind: usize, radius: i32) -> i32 {
    if kind == crate::vehicles::kind::HELICOPTER { radius / 3 } else { radius }
}

/// The open door of a vehicle (a [`Follow`] entity), if one is drawn. On
/// every [`SimObj`] from the spawn (see [`OverlayChildren`]).
#[derive(Component, Default)]
pub(crate) struct DoorPanel(Option<Entity>);

/// An open driver's door (`Drive::door` != 0) is drawn as its own object
/// (Mac `DrawObject` @ 10075880 tail = OS4 `FUN_010c101c` tail): the door
/// model created at the hinge (`+0x8150`, `+0x8154`: −max|X| on the right
/// side, at the door's longitudinal position) turned by heading + door
/// angle, with the vehicle's depth, pitch and roll, paint and burn timer
/// (not its dents). The Mac draws the models where they are, with no
/// placement offset; its raised Pug, Pug Racer, Taxi and Vapour keep the
/// unchanged ±18 doors, so their doors hang lower than the body there too.
#[allow(clippy::type_complexity)]
fn vehicle_doors(
    mut commands: Commands,
    data: Res<GameData>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut vehicles: Query<(Entity, &SimObj, &Vehicle, &Drive, &mut DoorPanel)>,
    mut panels: Query<(&mut Follow, &mut Mesh3d), Without<SimObj>>,
) {
    let Some(mut library) = library else { return };
    // `--open-doors`: every door fully open (visual checks).
    let forced = crate::args().any(|a| a == "--open-doors");
    for (e, obj, vehicle, drive, mut panel) in &mut vehicles {
        let angle = if forced { 0x370 } else { drive.door };
        let door = (angle != 0)
            .then(|| door_model(vehicle.kind))
            .flatten()
            .and_then(|p| library.load(&data, &mut meshes, p));
        let Some(door) = door else {
            if let Some(p) = panel.0.take() {
                commands.entity(p).try_despawn();
            }
            continue;
        };
        let shape = &library.model(obj.model).shape;
        let lat = -door_hinge(vehicle.kind, shape.radius) as f32 / 16.0;
        let fwd = crate::sim::boarding::door_y(vehicle.kind, shape.max_y) as f32 / 16.0;
        let sh = 2.0 / H_PER_SQUARE;
        let tf = Transform::from_xyz(lat * sh, 0.0, fwd * sh)
            .with_rotation(heading_to_render(angle as f32));
        let paint = usize::try_from(vehicle.colour).ok().filter(|&c| c < PAINTS.len());
        let skin = if obj.burn > 0 { Skin::Burnt } else { Skin::Normal };
        let mesh = library.variant_mesh(&mut meshes, door, skin, None, paint);
        match panel.0.and_then(|p| panels.get_mut(p).ok()) {
            Some((mut f, mut m)) => {
                f.local = tf;
                if m.0 != mesh {
                    m.0 = mesh;
                }
            }
            None => {
                let child = commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(library.material.clone()),
                        Transform::IDENTITY,
                        Follow { owner: e, local: tf },
                    ))
                    .id();
                panel.0 = Some(child);
            }
        }
    }
}

/// Helicopter rotor phase 0..11 per helicopter (the object's animation
/// frame N, Mac `+0x18`, OS4 `[6]`; frame P `+0x1c` is held at 0). Only
/// `DrawObject` reads it, so the port keeps it render-side.
#[derive(Resource, Default)]
pub struct RotorPhases(HashMap<Entity, i32>);

/// `do_Other` @ 10146280 (every vehicle not in state 6, each frame):
/// a helicopter's N −= 2·ticks with the engine on (`+0x984`, the port's
/// `Vehicle::flying`), else (ticks >> 1) + 1, wrapped into 0..11. Per
/// tick that is 2 and 1 (exact at 1–2 ticks a frame), so even a parked,
/// empty helicopter turns its rotor slowly.
fn advance_rotors(mut phases: ResMut<RotorPhases>, q: Query<(Entity, &SimObj, &Vehicle)>) {
    phases.0.retain(|e, _| q.contains(*e));
    for (e, obj, v) in &q {
        if v.kind != crate::vehicles::kind::HELICOPTER || obj.state == crate::sim::State::Removed {
            continue;
        }
        let n = phases.0.entry(e).or_insert(0);
        *n = (*n - if v.flying { 2 } else { 1 }).rem_euclid(12);
    }
}

/// Turn the helicopter's rotor (Mac `DrawObject` @ 10075880 with
/// `+0x8186` set, OS4 `FUN_010c101c` `+0x492a`): each blade-square corner
/// of limb l = 1..4 is placed at angle a = N·0x78 + (4 − l)·0x5a0 (a
/// quarter turn per 12 frames; the square's 4-fold symmetry makes it
/// seamless), x = sin a·16, y = cos a·16 in ×16 units, i.e. 256 model
/// units out, its own z kept; burning (`+0x81c` ≥ 1) puts every corner at
/// the centre and the rotor vanishes. The `Blades` square is the only
/// limb-tagged part and the only additive face of the model, so this is
/// a rotation and horizontal scale of the model's additive overlay.
fn pose_rotors(
    library: Option<Res<ObjectLibrary>>,
    phases: Res<RotorPhases>,
    helis: Query<(Entity, &SimObj, &Vehicle, &OverlayChildren)>,
    mut parts: Query<(&mut Follow, &MeshMaterial3d<ObjectMaterial>)>,
) {
    let Some(library) = library else { return };
    for (e, obj, v, children) in &helis {
        if v.kind != crate::vehicles::kind::HELICOPTER {
            continue;
        }
        let model = &library.model(obj.model).obj;
        // A blade corner's static angle and radius (model units).
        let Some(c) = model.vertices.iter().find(|c| (1..=4).contains(&c.limb)) else {
            continue;
        };
        let (x, y) = (c.x as f32, c.y as f32);
        let r = x.hypot(y).max(1.0);
        let still = x.atan2(y) * crate::world::ANGLE_TURN / std::f32::consts::TAU;
        let n = phases.0.get(&e).copied().unwrap_or(0);
        let a = (n * 0x78 + (4 - c.limb) * 0x5a0) as f32;
        let k = if obj.burn > 0 { 0.0 } else { 256.0 / r };
        let local = Transform::from_rotation(heading_to_render(a - still)).with_scale(Vec3::new(k, 1.0, k));
        for &child in &children.0 {
            if let Ok((mut f, m)) = parts.get_mut(child)
                && m.0 == library.additive
                && f.local != local
            {
                f.local = local;
            }
        }
    }
}

/// Objects and scenery that can hide a lens flare this frame. The
/// original drops a flare whose centre pixel, set to 0 by `AddFlare`
/// (Mac @ 100a6fb0, OS4 `FUN_0103451c`), was overdrawn by the time
/// `DrawFlares` (Mac @ 100a7340, OS4 `FUN_01034804`) runs, i.e. by anything
/// painted over it later: in painter's order, what lies between the flare
/// and the camera. Here: the city (`occluded`) and these models' triangles
/// on the segment from the flare to the eye; the flare's own model is not
/// tested (INFERRED: its other faces sit around the flare vertex).
#[derive(Resource, Default)]
pub struct FlareOccluders(Vec<Occluder>);

struct Occluder {
    owner: Entity,
    /// World to model (mesh) space.
    inv: bevy::math::Affine3A,
    mesh: Handle<Mesh>,
    /// Mesh-space bounds.
    min: Vec3,
    max: Vec3,
}

impl FlareOccluders {
    /// Whether a model other than `owner` lies on the segment from the
    /// flare at `from` to the camera at `eye` (render space).
    pub fn hides(&self, meshes: &Assets<Mesh>, from: Vec3, eye: Vec3, owner: Option<Entity>) -> bool {
        self.0.iter().any(|o| {
            if Some(o.owner) == owner {
                return false;
            }
            let a = o.inv.transform_point3(from);
            let b = o.inv.transform_point3(eye);
            if !segment_hits_box(a, b, o.min, o.max) {
                return false;
            }
            let Some(mesh) = meshes.get(&o.mesh) else { return false };
            let Some(p) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else {
                return false;
            };
            let tri = |i: usize, j: usize, k: usize| {
                let (Some(&p0), Some(&p1), Some(&p2)) = (p.get(i), p.get(j), p.get(k)) else {
                    return false;
                };
                segment_hits_triangle(a, b, Vec3::from(p0), Vec3::from(p1), Vec3::from(p2))
            };
            match mesh.indices() {
                Some(ix) => {
                    let ix: Vec<usize> = ix.iter().collect();
                    ix.chunks_exact(3).any(|t| tri(t[0], t[1], t[2]))
                }
                None => (0..p.len() / 3).any(|t| tri(3 * t, 3 * t + 1, 3 * t + 2)),
            }
        })
    }
}

/// Slab test of the segment `a`-`b` against a box.
fn segment_hits_box(a: Vec3, b: Vec3, min: Vec3, max: Vec3) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            if a[k] < min[k] || a[k] > max[k] {
                return false;
            }
            continue;
        }
        let (u, v) = ((min[k] - a[k]) / d[k], (max[k] - a[k]) / d[k]);
        t0 = t0.max(u.min(v));
        t1 = t1.min(u.max(v));
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Whether the segment `a`-`b` crosses the triangle (either side), away
/// from its end at `a` (Moller-Trumbore).
fn segment_hits_triangle(a: Vec3, b: Vec3, p0: Vec3, p1: Vec3, p2: Vec3) -> bool {
    let d = b - a;
    let (e1, e2) = (p1 - p0, p2 - p0);
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-12 {
        return false;
    }
    let f = 1.0 / det;
    let s = a - p0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = f * d.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = f * e2.dot(q);
    t > 1e-3 && t < 1.0
}

/// Lens-flare sources this frame: mode-6 faces of scenery and objects
/// (traffic and crossing lights, alarm boxes, police lights, muzzle
/// flashes, missiles), at the face's first vertex, size 300 (alarm boxes
/// pulse 200..700). Burning objects have none. A flare is dropped when the
/// city or another model is between it and the camera (the original tests
/// whether its centre pixel was overdrawn; [`FlareOccluders`]). The
/// occlusion and screen tests are made per view when the flares are drawn
/// (`crate::effects`): every split-screen view draws its own
/// (`FUN_0105bd98` calls `FUN_01034804` after each view).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn collect_flares(
    mode: Res<FxMode>,
    library: Option<Res<ObjectLibrary>>,
    clock: Res<SceneryClock>,
    meshes: Res<Assets<Mesh>>,
    cameras: Query<&GlobalTransform, Or<(With<GameCamera>, With<ExtraView>)>>,
    scenery: Query<(Entity, &Scenery, &GlobalTransform, &Mesh3d, Option<&bevy::camera::primitives::Aabb>)>,
    objects: Query<(Entity, &SimObj, &GlobalTransform, &Mesh3d, Option<&Person>, Option<&bevy::camera::primitives::Aabb>)>,
    mut flares: ResMut<Flares>,
    mut occluders: ResMut<FlareOccluders>,
) {
    occluders.0.clear();
    let Some(library) = library else {
        return;
    };
    if mode.0 == 0 {
        return;
    }
    let eyes: Vec<Vec3> = cameras.iter().map(|c| c.translation()).collect();
    // Roughly the area some view can see (the rotating view sees further).
    let near = |p: Vec3| {
        eyes.iter().any(|eye| {
            let r = 2.0 * eye.y + 2.0;
            (p.x - eye.x).abs() < r && (p.z - eye.z).abs() < r
        })
    };
    let mut occluder = |owner: Entity, tf: &GlobalTransform, mesh: &Mesh3d, aabb: Option<&bevy::camera::primitives::Aabb>| {
        if let Some(b) = aabb {
            let (c, h) = (Vec3::from(b.center), Vec3::from(b.half_extents));
            occluders.0.push(Occluder {
                owner,
                inv: tf.affine().inverse(),
                mesh: mesh.0.clone(),
                min: c - h,
                max: c + h,
            });
        }
    };
    for (e, _, tf, mesh, aabb) in &scenery {
        if near(tf.translation()) {
            occluder(e, tf, mesh, aabb);
        }
    }
    for (e, _, tf, mesh, _, aabb) in &objects {
        if near(tf.translation()) {
            occluder(e, tf, mesh, aabb);
        }
    }
    let mut add = |owner: Entity, model: usize, tf: &GlobalTransform, mesh: &Mesh3d, size: i32| {
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
            flares.add(FlareSource {
                pos,
                size,
                table: f.table,
                occlude: true,
                owner: Some(owner),
            });
        }
    };
    for (e, sc, tf, mesh, _) in &scenery {
        if !near(tf.translation()) {
            continue;
        }
        let size = if sc.id == sid::ALARM_FLASH {
            (clock.pulse - 10).abs() * 50 + 200
        } else {
            FLARE_SIZE
        };
        add(e, sc.model, tf, mesh, size);
    }
    for (e, obj, tf, mesh, person, _) in &objects {
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
        add(e, model, tf, mesh, FLARE_SIZE);
    }
}

/// Whether a solid cube lies on the segment from `from` to the camera
/// `eye` (render space), sampled every 1/8 square.
pub fn occluded(city: &City, from: Vec3, eye: Vec3) -> bool {
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
    let ground = city.square_cubes(x, y)[level].top_at(0.5, 0.5) / H_PER_SQUARE;
    let rot = heading_to_render(cube.rotation as f32 * 22.5);
    let lift = sunk_lift(library.model(model), rot);
    let e = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(library.material.clone()),
            Transform::from_xyz(x as f32 + 0.5, ground + lift, y as f32 + 0.5).with_rotation(rot),
            Scenery {
                id: cube.scenery,
                special: cube.special,
                rotation: cube.rotation,
                model,
                ground,
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
    if *done || (park.is_none() && !crate::args().any(|a| a == "--damaged")) {
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

    /// The flare occlusion tests: a triangle across the segment hides it,
    /// one beside it or behind the flare does not.
    #[test]
    fn flare_segment_tests() {
        let (a, b) = (Vec3::ZERO, Vec3::new(0.0, 10.0, 0.0));
        let tri = |y: f32, dx: f32| {
            segment_hits_triangle(
                a,
                b,
                Vec3::new(-1.0 + dx, y, -1.0),
                Vec3::new(1.0 + dx, y, -1.0),
                Vec3::new(0.0 + dx, y, 1.0),
            )
        };
        assert!(tri(5.0, 0.0));
        assert!(!tri(5.0, 3.0));
        assert!(!tri(-1.0, 0.0));
        assert!(!tri(11.0, 0.0));
        assert!(segment_hits_box(a, b, Vec3::new(-1.0, 4.0, -1.0), Vec3::new(1.0, 6.0, 1.0)));
        assert!(!segment_hits_box(a, b, Vec3::new(2.0, 4.0, -1.0), Vec3::new(3.0, 6.0, 1.0)));
    }
}
