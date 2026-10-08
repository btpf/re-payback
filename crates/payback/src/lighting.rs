//! The "Gouraud shading" option (`DAT_0114c2fc`; docs/re/rendering.md §4):
//! per-vertex RGB light on the city cubes.
//!
//! At load (`FUN_010766c8`, run by `FUN_010792dc` when the option is on):
//!
//! 1. `.CubeTextureRemapList` replaces every face texture by its line in the
//!    remap list, which maps the baked-shadow variants back to the plain
//!    textures (DarkPavement → Pavement, …).
//! 2. A 256×256×10 voxel grid (`DAT_0114c80c`, `FUN_010759e0`) marks the
//!    faces: half-square cells in x/y, z = H >> 7. A top face marks the
//!    sub-cell under each corner at the corner height, a side face the
//!    column base … base + h1 of its two sub-cells.
//! 3. Every cube corner (4 top, 4 bottom) is lit (`0x80`, night `0x60`) or in
//!    shadow (`0x30`): a ray steps from the corner one half-square west, one
//!    north and one z cell (128 h-units) up per step until z 9, and any
//!    marked cell shades it (a sun in the north-west, 35° up). The NE, SW
//!    and SE corners cast a second, parallel ray from the half-square point
//!    beside them and are in shadow only if both rays are blocked (Mac
//!    `DoLightSourcing` @ 1015b840 with `CheckVoxels` @ 100b3960, the same
//!    paired tests inlined in OS4 `FUN_010766c8`). Edge squares
//!    stay lit; Reflect tops are `0x80` even at night. Lit top corners of
//!    slopes facing the sun (RVert/RHoriz with h1 > h2, Vert/Horiz with
//!    h1 < h2) get `+0x40` (night `0x20`), slopes facing away the same less.
//! 4. Lamps (`FUN_0106c904`): scenery 33 (`lamppost_dark`, renumbered 36)
//!    and 36 (`lamppost_dark2`) at `800M + rand(100M)`, `600M + rand(80M)`,
//!    `400M + rand(60M)` R/G/B, 35 (`lamp`) at `500M + rand(50M)`, twice,
//!    `400M + rand(50M)`; at the square centre, 2000 object units above the
//!    top of the stack below them. Each corner within
//!    `(isqrt(max I) >> 12) + 1` squares gets `min(255, I / d²)` per channel (d in object units,
//!    16 per h-unit), no occlusion.
//!
//! Every frame (`FUN_010753e8`, `FUN_01036574` and object code; added with
//! `FUN_0106ba08` and removed after drawing with `FUN_010729ec`) dynamic
//! lights are added the same way, radius `(isqrt(max I) >> 13) + 1`, with
//! the intensities halved by day: traffic lights by phase, crossing beacons,
//! fireballs and explosions, burning objects, missiles, petrol bombs and
//! police lights. Here they are summed per vertex in `shaders/city.wgsl`.
//!
//! Drawing (`FUN_0112c6ec`): `out = Gouraud[(R & 0xF8) << 15 | (G & 0xF8) <<
//! 10 | (B & 0xF8) << 5 | texel]`, the palette colour nearest `pal[texel] +
//! (r, g, b) − 128`; the light is clamped to 255 per vertex and
//! interpolated across the face. The shader adds the quantised light in
//! sRGB (no palette quantisation). With the option off the light is 128
//! everywhere (the plain texture).
//!
//! Hardware renderer (`crate::render_mode`; docs/re/rendering.md §11.3): the
//! option is forced on, the base values differ (day 0xbe / shadow 0x80 /
//! step 0x30, night 0x20,0x20,0x30 / 0x0c,0x0c,0x10 / 0x20, lit water
//! 0xbe), every grid point gets a random `rand(16) − 8`, the dynamic lights
//! are not halved by day, and the light modulates the texel as a W3D vertex
//! colour `min(L, 255) / 256`.

use crate::city::Pass;
use crate::data::GameData;
use crate::effects::Effects;
use crate::level::AppState;
use crate::mapedit::{self, CityRender};
use crate::render_mode::RenderMode;
use crate::objects::ObjectLibrary;
use crate::sim::combat::{Arsenal, Weapon};
use crate::sim::{Class, Obj as SimObj, VisualRng};
use crate::sim::traffic::LightPhase;
use crate::world::{City, top_corners, to_render};
use bevy::{
    asset::embedded_asset,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use payback_formats::map::{self, FaceType, MAP_SIZE, Map, Slope};
use std::sync::Arc;

/// Corner order of the light arrays: NW, NE, SW, SE (top), then the same
/// for the bottom of the cube.
pub const NW: usize = 0;
pub const NE: usize = 1;
pub const SW: usize = 2;
pub const SE: usize = 3;

/// Light of the 8 corners of a cube, RGB, 128 = unchanged.
pub type CubeLight = [[f32; 3]; 8];

/// Light without the option: the plain texture.
pub const NEUTRAL: [f32; 3] = [128.0; 3];

/// Per-cube static light (base light, shadows, slopes, lamps).
pub struct CityLighting {
    /// Indexed `(x * MAP_SIZE + y) * 3 + layer`.
    cubes: Vec<CubeLight>,
    /// Computed for the hardware renderer (other base values, jitter).
    pub hardware: bool,
}

impl CityLighting {
    pub fn cube(&self, x: usize, y: usize, layer: usize) -> &CubeLight {
        &self.cubes[(x * MAP_SIZE + y) * 3 + layer]
    }
}

/// Voxel grid of `FUN_010759e0`: half-square cells, z = H >> 7 (0..9).
struct Voxels(Vec<bool>);

const VOX: usize = 2 * MAP_SIZE;
const VOX_Z: i32 = 10;

impl Voxels {
    fn idx(hx: usize, hy: usize, z: i32) -> usize {
        (hx * VOX + hy) * VOX_Z as usize + z as usize
    }

    fn mark(&mut self, hx: usize, hy: usize, z: i32) {
        self.0[Self::idx(hx, hy, z.clamp(0, VOX_Z - 1))] = true;
    }

    fn column(&mut self, hx: usize, hy: usize, a: i32, b: i32) {
        let (lo, hi) = (a.min(b).clamp(0, VOX_Z - 1), a.max(b).clamp(0, VOX_Z - 1));
        for z in lo..=hi {
            self.mark(hx, hy, z);
        }
    }

    /// Ray from grid point (gx, gy) (half squares) at height cell z0 towards
    /// the sun: (−1, −1, +1) cells per step.
    fn shadowed(&self, gx: usize, gy: usize, z0: i32) -> bool {
        let z0 = z0.clamp(0, VOX_Z - 1);
        for k in 1.. {
            let z = z0 + k;
            if z >= VOX_Z || k as usize > gx || k as usize > gy {
                return false;
            }
            let (hx, hy) = (gx - k as usize, gy - k as usize);
            if hx < VOX && hy < VOX && self.0[Self::idx(hx, hy, z)] {
                return true;
            }
        }
        false
    }
}

/// Top corner heights above the base in light order (NW, NE, SW, SE).
fn corner_heights(cube: &map::Cube) -> [i32; 4] {
    let [nw, ne, se, sw] = top_corners(cube).map(|h| h as i32);
    [nw, ne, sw, se]
}

fn visible(face: &map::Face) -> bool {
    face.kind != FaceType::Off
}

/// Height the original places a cube's scenery light on (`FUN_010766c8`,
/// `FUN_01074cd8`): h2 on Reflect tops, the higher end of slopes, else h1.
fn scenery_top(cube: &map::Cube) -> i32 {
    let (h1, h2) = (cube.height1 as i32, cube.height2 as i32);
    let reflect = cube.faces[map::TOP].kind == FaceType::Reflect;
    let slope = !matches!(cube.slope, Slope::Off | Slope::Flat);
    if reflect || (slope && h2 > h1) { h2 } else { h1 }
}

/// A point light in original units: object position (x, y, z down) and the
/// R/G/B intensity (light = I / d², d in object units).
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub pos: Vec3,
    pub rgb: Vec3,
}

impl PointLight {
    /// Square radius: `(isqrt(max I) >> shift) + 1` (12 for the static lamps
    /// of `FUN_0106c904`, 13 for the dynamic lights of `FUN_0106ba08`).
    fn radius(&self, shift: u32) -> i32 {
        let m = self.rgb.max_element().max(0.0);
        ((m.sqrt() as i32) >> shift) + 1
    }

    /// Range of corner grid lines lit along one axis (squares `c − r … c + r`
    /// clamped to 1..126; their outermost corners are skipped).
    fn corner_range(c: f32, r: i32) -> (i32, i32) {
        let s = (c as i32) >> 11;
        ((s - r).max(1) + 1, (s + r).min(126))
    }

    /// Contribution at a corner (grid x, y in squares; height H in h-units).
    fn at(&self, gx: i32, gy: i32, h: i32) -> Vec3 {
        let d = Vec3::new(
            (gx * 2048) as f32 - self.pos.x,
            (gy * 2048) as f32 - self.pos.y,
            ((1500 - h) * 16) as f32 - self.pos.z,
        );
        let d2 = d.length_squared().max(1.0);
        (self.rgb / d2).min(Vec3::splat(255.0)).floor()
    }
}

/// Small LCG for the lamp intensities (the original draws them from its
/// main generator at load; the simulation's generator is left alone).
struct Lcg(u32);

impl Lcg {
    fn rand(&mut self, n: i32) -> i32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        if n <= 0 { 0 } else { ((self.0 >> 8) % n as u32) as i32 }
    }
}

/// Base light of `FUN_010766c8`: lit and shadow RGB, slope step, and the
/// light of lit Reflect tops. Software adds the light to the texel (128 =
/// unchanged); Warp3D modulates the texel by `min(L, 255) / 256`, so its
/// values are higher by day and much lower at night (the cube textures are
/// not darkened on the card).
fn base_light(night: bool, hardware: bool) -> ([f32; 3], [f32; 3], f32, [f32; 3]) {
    let v = |r: u8, g: u8, b: u8| [r as f32, g as f32, b as f32];
    match (hardware, night) {
        (false, false) => (v(0x80, 0x80, 0x80), v(0x30, 0x30, 0x30), 64.0, v(0x80, 0x80, 0x80)),
        (false, true) => (v(0x60, 0x60, 0x60), v(0x30, 0x30, 0x30), 32.0, v(0x80, 0x80, 0x80)),
        (true, false) => (v(0xbe, 0xbe, 0xbe), v(0x80, 0x80, 0x80), 48.0, v(0xbe, 0xbe, 0xbe)),
        (true, true) => (v(0x20, 0x20, 0x30), v(0x0c, 0x0c, 0x10), 32.0, v(0xbe, 0xbe, 0xbe)),
    }
}

/// Static light of the whole map (`FUN_010766c8`), for the software or the
/// hardware renderer.
pub fn compute(map: &Map, night: bool, hardware: bool) -> CityLighting {
    let (lit, shadow, step, reflect) = base_light(night, hardware);

    // Bases of every cube.
    let mut bases = vec![[0i32; 3]; MAP_SIZE * MAP_SIZE];
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let mut base = 0;
            for (i, cube) in map.square(x, y).cubes.iter().enumerate() {
                bases[x * MAP_SIZE + y][i] = base;
                base += cube.height1 as i32;
            }
        }
    }

    // Voxel grid.
    let mut vox = Voxels(vec![false; VOX * VOX * VOX_Z as usize]);
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for (i, cube) in map.square(x, y).cubes.iter().enumerate() {
                if cube.slope == Slope::Off {
                    continue;
                }
                let base = bases[x * MAP_SIZE + y][i];
                let sub = [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)];
                if visible(&cube.faces[map::TOP]) {
                    for (c, h) in corner_heights(cube).into_iter().enumerate() {
                        vox.mark(sub[c].0, sub[c].1, (base + h) >> 7);
                    }
                }
                let (lo, hi) = (base >> 7, (base + cube.height1 as i32) >> 7);
                for (face, [a, b]) in [
                    (map::NORTH, [NW, NE]),
                    (map::EAST, [NE, SE]),
                    (map::SOUTH, [SW, SE]),
                    (map::WEST, [NW, SW]),
                ] {
                    if visible(&cube.faces[face]) {
                        vox.column(sub[a].0, sub[a].1, lo, hi);
                        vox.column(sub[b].0, sub[b].1, lo, hi);
                    }
                }
            }
        }
    }

    // Base light, shadows and slopes.
    let mut cubes = vec![[lit; 8]; MAP_SIZE * MAP_SIZE * 3];
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for (i, cube) in map.square(x, y).cubes.iter().enumerate() {
                if cube.slope == Slope::Off {
                    continue;
                }
                let base = bases[x * MAP_SIZE + y][i];
                let water = cube.faces[map::TOP].kind == FaceType::Reflect;
                let lit = if water { reflect } else { lit };
                let light = &mut cubes[(x * MAP_SIZE + y) * 3 + i];
                let edge = x == 0 || y == 0 || x == MAP_SIZE - 1 || y == MAP_SIZE - 1;
                let heights = corner_heights(cube);
                for c in 0..8 {
                    // The ray from the corner itself, and the one from the
                    // half-square point beside it (the same point for NW).
                    let (gx, gy) = (2 * x + 2 * (c & 1), 2 * y + (c & 2));
                    let (hx, hy) = (2 * x + (c & 1), 2 * y + ((c >> 1) & 1));
                    let h = if c < 4 { base + heights[c] } else { base };
                    let z = h >> 7;
                    let dark = !edge
                        && vox.shadowed(gx, gy, z)
                        && ((hx, hy) == (gx, gy) || vox.shadowed(hx, hy, z));
                    light[c] = if dark { shadow } else { lit };
                }
                let (h1, h2) = (cube.height1, cube.height2);
                let delta = match cube.slope {
                    Slope::Vert | Slope::Horiz if h1 > h2 => -step,
                    Slope::Vert | Slope::Horiz if h1 < h2 => step,
                    Slope::RVert | Slope::RHoriz if h1 > h2 => step,
                    Slope::RVert | Slope::RHoriz if h1 < h2 => -step,
                    _ => 0.0,
                };
                for corner in &mut light[..4] {
                    if corner[0] == lit[0] {
                        *corner = corner.map(|v| v + delta);
                    }
                }
            }
        }
    }

    // Warp3D only: every grid point of squares 1..126 gets a random
    // `rand(16) − 8` on all three channels of the four cubes meeting there
    // (same layer), once for the top corners and once for the bottom ones;
    // negative results become 0 (`FUN_010766c8`, after the slopes). The
    // original draws these from the shared game generator `FUN_0103ac94`,
    // so a hardware map load shifts the game's random sequence; the port
    // uses a local generator to keep the simulation (and replays)
    // independent of the renderer.
    if hardware {
        let mut rng = Lcg(0x4a17);
        for x in 1..MAP_SIZE - 1 {
            for y in 1..MAP_SIZE - 1 {
                for i in 0..3 {
                    for half in [0, 4] {
                        let d = (rng.rand(16) - 8) as f32;
                        // (square dx, dy, corner) sharing grid point (x, y).
                        for (dx, dy, c) in [(0, 0, NW), (1, 0, NE), (0, 1, SW), (1, 1, SE)] {
                            let corner = &mut cubes[((x - dx) * MAP_SIZE + y - dy) * 3 + i][c + half];
                            *corner = corner.map(|v| (v + d).max(0.0));
                        }
                    }
                }
            }
        }
    }

    // Lamps.
    let mut rng = Lcg(0x5eed);
    let mut lamps = Vec::new();
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for (i, cube) in map.square(x, y).cubes.iter().enumerate() {
                let rgb = match cube.scenery {
                    33 | 36 => {
                        let (r, g, b) = (rng.rand(100_000_000), rng.rand(80_000_000), rng.rand(60_000_000));
                        Vec3::new(
                            (r + 800_000_000) as f32,
                            (g + 600_000_000) as f32,
                            (b + 400_000_000) as f32,
                        )
                    }
                    35 => {
                        let (r, g, b) = (rng.rand(50_000_000), rng.rand(50_000_000), rng.rand(50_000_000));
                        Vec3::new(
                            (r + 500_000_000) as f32,
                            (g + 500_000_000) as f32,
                            (b + 400_000_000) as f32,
                        )
                    }
                    _ => continue,
                };
                let top = bases[x * MAP_SIZE + y][i] + scenery_top(cube);
                let pos = Vec3::new(
                    (x * 2048 + 0x400) as f32,
                    (y * 2048 + 0x400) as f32,
                    ((1500 - top) * 16 - 2000) as f32,
                );
                lamps.push(PointLight { pos, rgb });
            }
        }
    }
    for lamp in &lamps {
        let r = lamp.radius(12);
        let (gx0, gx1) = PointLight::corner_range(lamp.pos.x, r);
        let (gy0, gy1) = PointLight::corner_range(lamp.pos.y, r);
        for sx in (gx0 - 1)..=gx1 {
            for sy in (gy0 - 1)..=gy1 {
                let (sx, sy) = (sx as usize, sy as usize);
                for (i, cube) in map.square(sx, sy).cubes.iter().enumerate() {
                    if cube.slope == Slope::Off {
                        continue;
                    }
                    let base = bases[sx * MAP_SIZE + sy][i];
                    let heights = corner_heights(cube);
                    let light = &mut cubes[(sx * MAP_SIZE + sy) * 3 + i];
                    for (c, corner) in light.iter_mut().enumerate() {
                        let (gx, gy) = (sx as i32 + (c & 1) as i32, sy as i32 + ((c >> 1) & 1) as i32);
                        if gx < gx0 || gx > gx1 || gy < gy0 || gy > gy1 {
                            continue;
                        }
                        let h = if c < 4 { base + heights[c] } else { base };
                        let add = lamp.at(gx, gy, h);
                        for k in 0..3 {
                            corner[k] += add[k];
                        }
                    }
                }
            }
        }
    }
    CityLighting { cubes, hardware }
}

/// `.CubeTextureRemapList`: texture index → index of the texture named on
/// that line (identity where a name is unknown, as the original falls back
/// to texture 0 with a warning).
pub fn remap_table(data: &GameData) -> Vec<u16> {
    let names: Vec<&str> = data.cube_textures.entries.iter().map(|e| e.file.as_str()).collect();
    let mut table: Vec<u16> = (0..names.len() as u16).collect();
    // A user map's custom textures name their Gouraud texture themselves
    // (`.textures` field `gouraud`, read in `FUN_010766c8`).
    let custom = &data.custom_textures.slots;
    if !custom.is_empty() {
        let base = payback_formats::map::CUSTOM_TEXTURE_BASE;
        table.extend(names.len() as u16..base + custom.len() as u16);
        for (i, slot) in custom.iter().enumerate() {
            if let Some(slot) = slot {
                table[base as usize + i] = slot.gouraud;
            }
        }
    }
    // `.CubeTextureRemapList`, or the Mac's `CubeTextureRemapList.txt`.
    let Ok(raw) = data.read_texture_list("CubeTextureRemapList") else {
        warn!("missing CubeTextures/.CubeTextureRemapList");
        return table;
    };
    let text = String::from_utf8_lossy(&raw);
    for (i, line) in text.lines().enumerate().take(names.len()) {
        let name = line.trim();
        if let Some(j) = names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            table[i] = j as u16;
        }
    }
    table
}

// ---------------------------------------------------------------------------
// Material

/// Dynamic lights sent to the shader per frame: those whose square box
/// reaches a view's drawn squares, nearest a view first (the original adds
/// up to 1000; 510 lights fit a 16 KB uniform block, the portable limit).
pub const MAX_LIGHTS: usize = 510;

#[derive(ShaderType, Clone, Debug)]
pub struct LightBlock {
    /// x = number of lights, y = 1 for the hardware (Warp3D) look.
    pub params: Vec4,
    /// Per light: render position (x, y, z) and square radius; then R, G, B
    /// intensity.
    pub data: [Vec4; 2 * MAX_LIGHTS],
}

impl Default for LightBlock {
    fn default() -> Self {
        LightBlock {
            params: Vec4::ZERO,
            data: [Vec4::ZERO; 2 * MAX_LIGHTS],
        }
    }
}

/// City faces: the cube atlas with the per-vertex light
/// (`shaders/city.wgsl`). The pass selects opaque, colour-0 cut-out or the
/// 50 % Alpha blend.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(Pass)]
pub struct CityMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    #[uniform(2)]
    pub lights: LightBlock,
    pub pass: Pass,
}

impl From<&CityMaterial> for Pass {
    fn from(m: &CityMaterial) -> Self {
        m.pass
    }
}

impl Material for CityMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://payback/shaders/city.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/city.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.pass {
            // Warp3D blends genlocked faces (7, 8) with the filtered
            // colour-key alpha of the A1R5G5B5 texture.
            Pass::Masked if self.lights.params.y > 0.5 => AlphaMode::Blend,
            Pass::Masked => AlphaMode::Mask(0.5),
            Pass::Alpha => AlphaMode::Blend,
            _ => AlphaMode::Opaque,
        }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use crate::city::{ATTRIBUTE_CORNER_LIGHT, ATTRIBUTE_CORNER_POS, ATTRIBUTE_QUAD_ST};
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_QUAD_ST.at_shader_location(2),
            ATTRIBUTE_CORNER_POS[0].at_shader_location(3),
            ATTRIBUTE_CORNER_POS[1].at_shader_location(4),
            ATTRIBUTE_CORNER_POS[2].at_shader_location(5),
            ATTRIBUTE_CORNER_POS[3].at_shader_location(6),
            ATTRIBUTE_CORNER_LIGHT[0].at_shader_location(7),
            ATTRIBUTE_CORNER_LIGHT[1].at_shader_location(8),
            ATTRIBUTE_CORNER_LIGHT[2].at_shader_location(9),
            ATTRIBUTE_CORNER_LIGHT[3].at_shader_location(10),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(11),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Both sides are in the mesh already.
        descriptor.primitive.cull_mode = None;
        if let Some(fragment) = descriptor.fragment.as_mut() {
            match key.bind_group_data {
                Pass::Masked => fragment.shader_defs.push("CITY_MASKED".into()),
                Pass::Alpha => fragment.shader_defs.push("CITY_ALPHA".into()),
                _ => {}
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Option and dynamic lights

/// Whether Gouraud lighting is on: the pause menu's "Gouraud shading"
/// (`Settings::gouraud`; OS4 initial value 1), `--gouraud 0|1` wins.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gouraud(pub bool);

impl Default for Gouraud {
    fn default() -> Self {
        Gouraud(crate::arg("--gouraud").is_none_or(|v| v != "0"))
    }
}

/// The level's lights for this frame (object units).
#[derive(Resource, Default)]
pub struct DynamicLights(pub Vec<PointLight>);

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/city.wgsl");
        app.add_plugins(MaterialPlugin::<CityMaterial>::default())
            .init_resource::<Gouraud>()
            .init_resource::<DynamicLights>()
            .add_systems(
                Update,
                (follow_settings, rebuild_on_toggle, sync_mode, follow_textures, collect_lights, upload_lights)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// The pause menu's option drives [`Gouraud`] (`--gouraud` wins); hardware
/// rendering forces it on (LoadConfig `FUN_0104199c`: `DAT_0114c7d8 != 0` →
/// `DAT_0114c2fc = 1`).
fn follow_settings(
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mode: Res<RenderMode>,
    mut g: ResMut<Gouraud>,
) {
    if mode.hardware() {
        if !g.0 {
            g.0 = true;
        }
        return;
    }
    let Some(settings) = settings else { return };
    if (settings.is_changed() || mode.is_changed()) && crate::arg("--gouraud").is_none() && g.0 != settings.gouraud {
        g.0 = settings.gouraud;
    }
}

/// Toggling the option relights the map (`FUN_010792dc` / `FUN_01074cbc` +
/// `FUN_01079c54`): rebuild every chunk with or without the light.
fn rebuild_on_toggle(
    mut commands: Commands,
    gouraud: Res<Gouraud>,
    data: Res<GameData>,
    city: Option<Res<City>>,
    render: Option<ResMut<CityRender>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(city), Some(mut render)) = (city, render) else {
        return;
    };
    let hardware = crate::render_mode::is_hardware();
    let current = render.lighting.as_ref().map(|l| l.hardware);
    if current.is_some() == gouraud.0 && current.is_none_or(|h| h == hardware) {
        return;
    }
    set_lighting(&mut render, &data, &city, gouraud.0);
    mapedit::spawn_all_chunks(&mut commands, &mut render, &mut meshes, &city);
}

fn hardware_flag() -> f32 {
    if crate::render_mode::is_hardware() { 1.0 } else { 0.0 }
}

/// Follow the renderer in the city materials: the shading flag, and at
/// night the cube atlases (darkened through `T1[c][0x67]` in software,
/// left alone on the Warp3D card, `FUN_0106ecf0`: the night is in the
/// vertex colours there).
#[allow(clippy::too_many_arguments)]
fn sync_mode(
    mode: Res<RenderMode>,
    data: Res<GameData>,
    city: Option<Res<City>>,
    render: Option<Res<CityRender>>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<Option<(bool, bool)>>,
) {
    let (Some(city), Some(render)) = (city, render) else {
        *done = None;
        return;
    };
    let hardware = mode.hardware();
    let fresh = city.is_added() || render.is_added();
    if !fresh && *done == Some((hardware, city.map.night)) {
        return;
    }
    let flag = hardware_flag();
    for handle in render.materials.values() {
        if materials.get(handle).is_some_and(|m| m.lights.params.y != flag)
            && let Some(mut m) = materials.get_mut(handle)
        {
            m.lights.params.y = flag;
        }
    }
    // Daytime atlases are the same for both renderers; a night map loaded
    // in software is darkened, so a change of renderer rebuilds them.
    let was_hardware = done.is_some_and(|d| d.0);
    if city.map.night && (hardware || (!fresh && was_hardware)) {
        let atlases = crate::city::build_cube_atlases(&data, !hardware);
        for (pass, handle) in &render.materials {
            let Some(atlas) = materials.get(handle).map(|m| m.atlas.clone()) else { continue };
            let image = if *pass == Pass::Masked { &atlases.keyed } else { &atlases.opaque };
            if let Some(mut slot) = images.get_mut(&atlas) {
                *slot = image.clone();
            }
        }
    }
    *done = Some((hardware, city.map.night));
}

/// The "Textures: Mac / Amiga look" setting (`Variants::textures`) changed
/// in the pause menu's graphics screen: switch the cube texture set and
/// rebuild the level's atlases in place (city materials and the water's
/// index atlas), as [`sync_mode`] does for the renderer. The set is
/// render-only (the surface flags come from `.CubeTextureList`), so the
/// simulation is untouched. At load `level.rs` sets it from the settings.
#[allow(clippy::too_many_arguments)]
fn follow_textures(
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mode: Res<RenderMode>,
    mut data: ResMut<GameData>,
    city: Option<Res<City>>,
    render: Option<Res<CityRender>>,
    materials: Res<Assets<CityMaterial>>,
    water: Res<Assets<crate::water::WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(settings) = settings.filter(|s| s.is_changed()) else { return };
    let want = settings.variants.textures;
    if data.mac_cube_textures == want {
        return;
    }
    data.mac_cube_textures = want;
    let (Some(city), Some(render)) = (city, render) else { return };
    info!("cube textures: {} set", if want { "Mac" } else { "Amiga" });
    // Night maps are darkened in the atlas only in software (`sync_mode`).
    let atlases = crate::city::build_cube_atlases(&data, city.map.night && !mode.hardware());
    for (pass, handle) in &render.materials {
        let Some(atlas) = materials.get(handle).map(|m| m.atlas.clone()) else { continue };
        let image = if *pass == Pass::Masked { &atlases.keyed } else { &atlases.opaque };
        if let Some(mut slot) = images.get_mut(&atlas) {
            *slot = image.clone();
        }
    }
    if let Some(index) = water.get(&render.water).map(|w| w.index_atlas.clone())
        && let Some(mut slot) = images.get_mut(&index)
    {
        *slot = atlases.indices;
    }
}

/// Compute (or drop) the static light and the texture remap.
pub fn set_lighting(render: &mut CityRender, data: &GameData, city: &City, on: bool) {
    if on {
        let hardware = crate::render_mode::is_hardware();
        render.lighting = Some(Arc::new(compute(&city.map, city.map.night, hardware)));
        render.remap = Some(remap_table(data));
    } else {
        render.lighting = None;
        render.remap = None;
    }
}

/// Traffic-light colour by state k = special + phase − 1 (mod 6)
/// (`FUN_010753e8`): red, red+amber, green, amber.
fn traffic_light_rgb(k: i32) -> Vec3 {
    let m = 1_000_000.0;
    match k {
        ..2 => Vec3::new(300.0, 50.0, 50.0) * m,
        2 => Vec3::new(600.0, 300.0, 50.0) * m,
        3 | 4 => Vec3::new(50.0, 300.0, 50.0) * m,
        _ => Vec3::new(300.0, 300.0, 50.0) * m,
    }
}

/// Crossing beacon by the phase sub-counter (`FUN_010753e8`).
fn crossing_rgb(sub: i32) -> Option<Vec3> {
    let m = 1_000_000.0;
    match sub {
        24 => Some(Vec3::new(150.0, 150.0, 25.0) * m),
        25.. => Some(Vec3::new(300.0, 300.0, 50.0) * m),
        0 => Some(Vec3::new(200.0, 200.0, 33.333_332) * m),
        1 => Some(Vec3::new(100.0, 100.0, 16.666_666) * m),
        _ => None,
    }
}

/// A traffic light or crossing beacon of the map.
struct SceneryLight {
    scenery: u8,
    special: u8,
    pos: Vec3,
}

fn scenery_lights(city: &City, library: &ObjectLibrary) -> Vec<SceneryLight> {
    let traffic = library.loaded("scenery/trafficlight1.obj");
    let crossing = library.loaded("scenery/pedestriancrossing.obj");
    let mut out = Vec::new();
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let mut base = 0;
            for cube in &city.map.square(x, y).cubes {
                let top = base + scenery_top(cube);
                base += cube.height1 as i32;
                let model = match cube.scenery {
                    1 => traffic,
                    15 => crossing,
                    _ => continue,
                };
                let Some(model) = model else { continue };
                let mean = library.model(model).info.mean.as_vec2();
                // cos/sin tables of 256 (`DAT_012a676c`, `DAT_012ac16c`).
                let a = cube.rotation as f32 / 256.0 * std::f32::consts::TAU;
                let (s, c) = ((a.sin() * 256.0).trunc() / 256.0, (a.cos() * 256.0).trunc() / 256.0);
                out.push(SceneryLight {
                    scenery: cube.scenery,
                    special: cube.special,
                    pos: Vec3::new(
                        (x * 2048 + 0x400) as f32 + c * mean.x + s * mean.y,
                        (y * 2048 + 0x400) as f32 + c * mean.y - s * mean.x,
                        ((1500 - top) * 16 - 500) as f32,
                    ),
                });
            }
        }
    }
    out
}

/// Muzzle flash of a person who fired this tick (Mac `do_Person` @
/// 1013fbd0, next to the fire sound): minigun, pistol and shotgun, 500
/// z-units above the shooter.
fn muzzle_flash(weapon: Weapon) -> Option<Vec3> {
    let m = 1_000_000.0;
    match weapon {
        Weapon::Minigun => Some(Vec3::new(60.0, 60.0, 8.0) * m),
        Weapon::Pistol => Some(Vec3::new(40.0, 40.0, 10.0) * m),
        Weapon::Shotgun => Some(Vec3::new(60.0, 60.0, 20.0) * m),
        _ => None,
    }
}

/// Gather this frame's dynamic lights. The jitter comes from the cosmetic
/// generator ([`VisualRng`], `random3` in Mac `DoEffectsLighting` @
/// 100a82b0), drawn last argument first (B, G, R) as the Mac and 68k
/// builds evaluate them (OS4: first to last). The burning-object and petrol-bomb jitter is drawn
/// from the game's generator inside the original's object update; here it
/// is cosmetic too, so the simulation does not depend on the renderer.
#[allow(clippy::too_many_arguments)]
fn collect_lights(
    gouraud: Res<Gouraud>,
    city: Option<Res<City>>,
    library: Option<Res<ObjectLibrary>>,
    phase: Option<Res<LightPhase>>,
    effects: Res<Effects>,
    objects: Query<(&SimObj, Option<&Arsenal>)>,
    mut lights: ResMut<DynamicLights>,
    rng: Option<ResMut<VisualRng>>,
    mut sources: Local<Option<Vec<SceneryLight>>>,
) {
    lights.0.clear();
    let (true, Some(city), Some(library), Some(mut rng)) = (gouraud.0, city, library, rng) else {
        return;
    };
    let m = 1_000_000;
    let (phase, sub) = phase.map_or((0, 0), |p| (p.phase, p.sub));

    // Traffic lights (scenery 1) and crossing beacons (15), at the model's
    // ground-vertex mean (+0x4910/+0x4914) rotated by the cube's rotation,
    // 500 z-units above the top of their cube (`FUN_01074cd8`).
    if city.is_changed() || sources.is_none() {
        *sources = Some(scenery_lights(&city, &library));
    }
    for s in sources.iter().flatten() {
        let rgb = if s.scenery == 1 {
            let mut k = s.special as i32 + phase - 1;
            if k > 5 {
                k -= 6;
            }
            Some(traffic_light_rgb(k))
        } else {
            crossing_rgb(sub)
        };
        if let Some(rgb) = rgb {
            lights.0.push(PointLight { pos: s.pos, rgb });
        }
    }

    // Fireballs, explosions, lens-flare debris and fires (`FUN_01036574`).
    for (kind, pos, r) in effects.light_sources() {
        // (base, jitter range) per channel R, G, B; drawn B, G, R.
        let [(rb, rj), (gb, gj), (bb, bj)] = match kind {
            0xb => [(900_000, 0), (900_000, 0), (700_000, 0)],
            2 => [(140_000, 5 * m), (120_000, 4 * m), (100_000, 3 * m)],
            7 => [(50_000, 2 * m), (40_000, 3 * m / 2), (30_000, m)],
            0xf => [(100_000, 5 * m), (70_000, 3 * m), (50_000, 2 * m)],
            _ => continue,
        };
        let mut jitter = |n: i32| if n > 0 { rng.rand(n) } else { 0 };
        let b = r * bb + jitter(bj);
        let g = r * gb + jitter(gj);
        let red = r * rb + jitter(rj);
        let rgb = Vec3::new(red as f32, g as f32, b as f32);
        lights.0.push(PointLight { pos, rgb });
    }

    // Objects: burning (main object update, burn timer obj+0x33c), missiles
    // (`AddCollideObj`), petrol bombs, police lights.
    for (obj, arsenal) in &objects {
        let info = &library.model(obj.model).info;
        let p = obj.pos.as_vec3();
        let path = info.path.as_str();
        if obj.burn > 1 {
            // The Mac's order: spread, B, G, R, then the z, y, x offsets.
            let (spread, half) = if obj.burn < 16 {
                ((info.radius + rng.rand(150)) * 200_000, 5 * m)
            } else {
                ((info.radius * 3 / 2 + rng.rand(300)) * 200_000, 10 * m)
            };
            let b = spread - rng.rand(half);
            let g = rng.rand(half) + spread - half / 2;
            let red = rng.rand(half) + spread;
            let jz = rng.rand(300) - 150;
            let jy = rng.rand(300) - 150;
            let jx = rng.rand(300) - 150;
            let j = Vec3::new(jx as f32, jy as f32, jz as f32);
            let rgb = Vec3::new(red as f32, g as f32, b as f32);
            lights.0.push(PointLight { pos: p + j, rgb });
        }
        let lift = Vec3::new(0.0, 0.0, -500.0);
        // A firearm fired this tick (the firing pose is set to 1 then).
        if obj.class == Class::Person
            && let Some(ars) = arsenal
            && ars.pose == 1
            && let Some(rgb) = muzzle_flash(ars.current)
        {
            lights.0.push(PointLight { pos: p + lift, rgb });
        }
        let rgb = match path {
            "weapons/missile.obj" => Some(Vec3::new(100.0, 90.0, 80.0) * m as f32),
            "weapons/petrol.obj" => {
                let b = rng.rand(2 * m) + 5 * m;
                let g = rng.rand(5 * m) + 15 * m;
                let red = rng.rand(10 * m) + 30 * m;
                Some(Vec3::new(red as f32, g as f32, b as f32))
            }
            "vehicles/police_red.obj" | "vehicles/policevan_red.obj" => {
                Some(Vec3::new(200.0, 20.0, 20.0) * m as f32)
            }
            "vehicles/police_blue.obj" | "vehicles/policevan_blue.obj" => {
                Some(Vec3::new(20.0, 20.0, 200.0) * m as f32)
            }
            _ => None,
        };
        if let Some(rgb) = rgb {
            lights.0.push(PointLight { pos: p + lift, rgb });
        }
    }

    // By day the dynamic lights are at half intensity (`FUN_0106ba08`;
    // software only: `DAT_0114c84c == 0 && DAT_0114c7d8 == 0`).
    if !city.map.night && !crate::render_mode::is_hardware() {
        for l in &mut lights.0 {
            l.rgb = (l.rgb / 2.0).floor();
        }
    }
}

/// Send the lights that can reach a view (every split-screen view; their
/// square boxes against the squares each view draws), at most
/// [`MAX_LIGHTS`], nearest a view first, to the city materials.
fn upload_lights(
    lights: Res<DynamicLights>,
    render: Option<Res<CityRender>>,
    eyes: Res<crate::camera::ViewEyes>,
    rotating: Res<crate::camera::RotatingView>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut last: Local<usize>,
) {
    let Some(render) = render else { return };
    if lights.0.is_empty() && *last == 0 {
        return;
    }
    // The views' eyes (the rotating view and the replay TV camera draw from
    // their own camera positions, camera.rs).
    let boxes: Vec<(Vec2, IVec2, (i32, i32))> = eyes
        .cams
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let centre = Vec2::new(c.x as f32 / 32.0, c.y as f32 / 32.0);
            (centre, c.square(), eyes.visible_squares(i, rotating.0))
        })
        .collect();
    let mut sorted: Vec<(f32, &PointLight)> = lights
        .0
        .iter()
        .filter_map(|l| {
            let r = l.radius(13);
            let s = IVec2::new(l.pos.x as i32 >> 11, l.pos.y as i32 >> 11);
            boxes
                .iter()
                .filter(|(_, sq, (hw, hh))| (s.x - sq.x).abs() <= hw + r && (s.y - sq.y).abs() <= hh + r)
                .map(|(centre, ..)| l.pos.truncate().distance_squared(*centre))
                .min_by(f32::total_cmp)
                .map(|d| (d, l))
        })
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    sorted.truncate(MAX_LIGHTS);
    let sorted: Vec<&PointLight> = sorted.into_iter().map(|(_, l)| l).collect();
    let mut block = LightBlock::default();
    block.params.x = sorted.len() as f32;
    block.params.y = hardware_flag();
    for (i, l) in sorted.iter().enumerate() {
        let p = to_render(l.pos);
        block.data[2 * i] = p.extend(l.radius(13) as f32);
        block.data[2 * i + 1] = l.rgb.extend(0.0);
    }
    *last = sorted.len();
    for handle in render.materials.values() {
        if let Some(mut m) = materials.get_mut(handle) {
            m.lights = block.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamp_radius_and_falloff() {
        let lamp = PointLight {
            pos: Vec3::new(1024.0, 1024.0, ((1500 - 256) * 16 - 2000) as f32),
            rgb: Vec3::new(900e6, 680e6, 460e6),
        };
        // isqrt(900M) = 30000 >> 12 = 7, + 1.
        assert_eq!(lamp.radius(12), 8);
        assert_eq!(lamp.radius(13), 4);
        // Straight below at 2000 units: 900M / 4M = 225.
        let below = PointLight { pos: Vec3::new(0.0, 0.0, lamp.pos.z), ..lamp };
        assert_eq!(below.at(0, 0, 256).x, 225.0);
    }

    #[test]
    fn traffic_light_states() {
        assert_eq!(traffic_light_rgb(0).x, 300e6);
        assert_eq!(traffic_light_rgb(2).y, 300e6);
        assert_eq!(traffic_light_rgb(4).y, 300e6);
        assert_eq!(traffic_light_rgb(5).x, 300e6);
        assert!(crossing_rgb(10).is_none());
    }
}


