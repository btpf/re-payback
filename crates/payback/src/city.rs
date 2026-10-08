//! City geometry for rendering, following docs/re/rendering.md.
//!
//! Render space: one Bevy unit is one map square; Y up in squares
//! (256 h-units); square (x, y) covers X in [x, x+1], Z in [y, y+1].

use crate::data::GameData;
use crate::lighting::{CityLighting, NE, NEUTRAL, NW, SE, SW};
use crate::world::{H_PER_SQUARE, stack_height, top_corners};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, MeshVertexAttribute, VertexFormat},
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};

/// Position of a vertex in its quad: (0,0) at the first corner, s along
/// the first edge, t along the last (`shaders/city.wgsl` interpolates the
/// Gouraud light bilinearly with it).
pub const ATTRIBUTE_QUAD_ST: MeshVertexAttribute =
    MeshVertexAttribute::new("QuadSt", 0x5042_0001, VertexFormat::Float32x2);

/// The four corners of the vertex's quad (mesh space), the same on all four
/// vertices: the shader adds the dynamic lights at each corner.
pub const ATTRIBUTE_CORNER_POS: [MeshVertexAttribute; 4] = [
    MeshVertexAttribute::new("CornerPos0", 0x5042_0010, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerPos1", 0x5042_0011, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerPos2", 0x5042_0012, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerPos3", 0x5042_0013, VertexFormat::Float32x3),
];

/// Static Gouraud light of the four corners (0..255+, 128 = unchanged).
pub const ATTRIBUTE_CORNER_LIGHT: [MeshVertexAttribute; 4] = [
    MeshVertexAttribute::new("CornerLight0", 0x5042_0020, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerLight1", 0x5042_0021, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerLight2", 0x5042_0022, VertexFormat::Float32x3),
    MeshVertexAttribute::new("CornerLight3", 0x5042_0023, VertexFormat::Float32x3),
];
use payback_formats::map::{self, Face, FaceType, MAP_SIZE, Map, Slope};

const TILE: u32 = 64;
const ATLAS_TILES: u32 = 32;
pub const ATLAS_SIZE: u32 = TILE * ATLAS_TILES;
/// Atlas tile of a user map's first custom texture (texture number 2000):
/// the last 112 tiles (the Map Editor's slot limit), after the 844 listed
/// textures.
const CUSTOM_TILE0: u32 = ATLAS_TILES * ATLAS_TILES - 112;

/// Atlas tile of texture number `t` (custom textures from 2000 on).
fn atlas_tile(t: u16) -> u32 {
    match t.checked_sub(map::CUSTOM_TEXTURE_BASE) {
        Some(i) => CUSTOM_TILE0 + i as u32,
        None => t as u32,
    }
}

/// Cube textures packed into 2048x2048 atlases (64x64 tiles in
/// `.CubeTextureList` order): RGBA with colour 0 opaque, RGBA with colour 0
/// transparent (genlocked faces), and raw palette indices (water shader).
pub struct CubeAtlases {
    pub opaque: Image,
    pub keyed: Image,
    pub indices: Image,
}

/// Night darkening: every cube texture whose name doesn't start with `#` or
/// `$` maps colour c to `T1[c][0x67]`, the palette colour nearest
/// pal[c] - (87, 87, 87) (rendering.md §4, `FUN_0106ecf0(1)`; the table is
/// read from the shipped `Alpha.256`).
pub fn build_cube_atlases(data: &GameData, night: bool) -> CubeAtlases {
    let n = (ATLAS_SIZE * ATLAS_SIZE) as usize;
    let remap = night.then(|| crate::camera::postfx::night_remap(data));
    let mut opaque = vec![0u8; n * 4];
    let mut keyed = vec![0u8; n * 4];
    let mut indices = vec![0u8; n];
    // The list's textures (Mac or Amiga set, `GameData::cube_texture`),
    // then the user map's custom ones at `CUSTOM_TILE0`; at night custom
    // textures are exempt only with a `$` name (`FUN_0106e8ec`).
    let listed = data.cube_textures.entries.iter().enumerate().map(|(i, entry)| {
        let img = data.cube_texture(&entry.file);
        if img.is_none() {
            warn!("missing cube texture {}", entry.file);
        }
        (i as u32, img, !entry.file.starts_with(['#', '$']))
    });
    let custom = data.custom_textures.slots.iter().enumerate().filter_map(|(i, slot)| {
        let slot = slot.as_ref()?;
        let tile = CUSTOM_TILE0 + i as u32;
        (tile < ATLAS_TILES * ATLAS_TILES)
            .then(|| (tile, Some(slot.image.clone()), !slot.name.starts_with('$')))
    });
    for (i, img, darkens) in listed.chain(custom) {
        let Some(img) = img else { continue };
        let darken = night && darkens;
        let (tx, ty) = (i % ATLAS_TILES * TILE, i / ATLAS_TILES * TILE);
        for row in 0..TILE {
            for col in 0..TILE {
                let mut c = img.pixels[(row * TILE + col) as usize];
                let p = ((ty + row) * ATLAS_SIZE + tx + col) as usize;
                if let (true, Some(remap)) = (darken, &remap) {
                    c = remap[c as usize];
                }
                let mut rgb = [0u8; 3];
                rgb.copy_from_slice(&data.palette[c as usize * 3..c as usize * 3 + 3]);
                opaque[p * 4..p * 4 + 3].copy_from_slice(&rgb);
                opaque[p * 4 + 3] = 255;
                keyed[p * 4..p * 4 + 3].copy_from_slice(&rgb);
                keyed[p * 4 + 3] = if c == 0 { 0 } else { 255 };
                indices[p] = c;
            }
        }
    }
    let make = |bytes, format| {
        let mut image = Image::new(
            Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
        image
    };
    CubeAtlases {
        opaque: make(opaque, TextureFormat::Rgba8UnormSrgb),
        keyed: make(keyed, TextureFormat::Rgba8UnormSrgb),
        indices: make(indices, TextureFormat::R8Unorm),
    }
}

/// Which render pass a face belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pass {
    Opaque,
    /// Genlocked: colour 0 transparent.
    Masked,
    /// 50% blend (Alpha.256 table 4); colour 0 not transparent.
    Alpha,
    /// Water / reflective tops (screen-space sky displacement).
    Reflect,
}

impl Pass {
    pub const ALL: [Pass; 4] = [Pass::Opaque, Pass::Masked, Pass::Alpha, Pass::Reflect];
}

fn pass_of(kind: FaceType, is_top: bool) -> Option<Pass> {
    match kind {
        // Type 5 is an open door (set at runtime): not drawn.
        FaceType::Off | FaceType::Unknown(5) => None,
        FaceType::Normal | FaceType::Unknown(_) => Some(Pass::Opaque),
        FaceType::Genlocked => Some(Pass::Masked),
        // Alpha and Reflect only apply to tops; sides draw as Normal.
        FaceType::Alpha if is_top => Some(Pass::Alpha),
        FaceType::Reflect if is_top => Some(Pass::Reflect),
        FaceType::Alpha | FaceType::Reflect => Some(Pass::Opaque),
    }
}

/// Added to UV_1.x of scrolling (`#`-named) water faces.
pub const SCROLL_FLAG: f32 = 100_000.0;

#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    /// Tile origin in atlas texels (+ SCROLL_FLAG for scrolling water).
    uvs_b: Vec<[f32; 2]>,
    /// Position in the quad (0..1, 0..1).
    st: Vec<[f32; 2]>,
    /// The quad's corners and their Gouraud light (128 = unchanged;
    /// `lighting.rs`), repeated on each vertex.
    corner_pos: [Vec<[f32; 3]>; 4],
    corner_light: [Vec<[f32; 3]>; 4],
    indices: Vec<u32>,
}

impl MeshBuilder {
    /// Quad with corners in winding order and per-corner texture coordinates
    /// in texels of the 64x64 tile (u = column, v = row, edges at 0 and 64).
    fn quad(
        &mut self,
        corners: [Vec3; 4],
        tex_uv: [Vec2; 4],
        texture: u16,
        scrolls: bool,
        light: [[f32; 3]; 4],
    ) {
        let tile = atlas_tile(texture);
        let tile = Vec2::new((tile % ATLAS_TILES * TILE) as f32, (tile / ATLAS_TILES * TILE) as f32);
        let normal = (corners[1] - corners[0])
            .cross(corners[3] - corners[0])
            .normalize_or_zero();
        let base = self.positions.len() as u32;
        for ((c, t), st) in corners.iter().zip(tex_uv).zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]) {
            self.st.push(st);
            for k in 0..4 {
                self.corner_pos[k].push(corners[k].to_array());
                self.corner_light[k].push(light[k]);
            }
            // Keep a quarter-texel inside the tile so nearest sampling never
            // bleeds into the neighbour.
            let t = t.clamp(Vec2::splat(0.25), Vec2::splat(63.75));
            self.positions.push(c.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push(((tile + t) / ATLAS_SIZE as f32).to_array());
            let flag = if scrolls { SCROLL_FLAG } else { 0.0 };
            self.uvs_b.push([tile.x + flag, tile.y]);
        }
        // Draw both sides so winding differences between face kinds can't
        // drop geometry (the original culls back faces).
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    fn build(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.uvs_b)
        .with_inserted_attribute(ATTRIBUTE_QUAD_ST, self.st);
        for (k, (p, l)) in self.corner_pos.into_iter().zip(self.corner_light).enumerate() {
            mesh.insert_attribute(ATTRIBUTE_CORNER_POS[k], p);
            mesh.insert_attribute(ATTRIBUTE_CORNER_LIGHT[k], l);
        }
        mesh.with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Mirror u when the face's flip bit is set (rendering.md §2: flip mirrors
/// u only, for every face and slope).
fn flip_u(uv: [Vec2; 4], face: &Face) -> [Vec2; 4] {
    if face.flipped {
        uv.map(|t| Vec2::new(64.0 - t.x, t.y))
    } else {
        uv
    }
}

/// Top-face texture coordinates at corners [NW, NE, SE, SW] for flip = 0
/// (rendering.md §2).
fn top_uv(slope: Slope) -> [Vec2; 4] {
    let (lo, hi) = (0.0, 64.0);
    match slope {
        // u = 63 at west; v = 0 at south.
        Slope::Vert | Slope::RVert => [
            Vec2::new(hi, hi),
            Vec2::new(lo, hi),
            Vec2::new(lo, lo),
            Vec2::new(hi, lo),
        ],
        // u runs north -> south; v runs west -> east.
        Slope::Horiz | Slope::RHoriz => [
            Vec2::new(lo, lo),
            Vec2::new(lo, hi),
            Vec2::new(hi, hi),
            Vec2::new(hi, lo),
        ],
        // Flat: u = 63 at west; v = 0 at north.
        _ => [
            Vec2::new(hi, lo),
            Vec2::new(lo, lo),
            Vec2::new(lo, hi),
            Vec2::new(hi, hi),
        ],
    }
}

/// Squares per chunk side; the city is meshed in 8x8 chunks so map edits
/// only rebuild one.
pub const CHUNK: usize = 16;
pub const CHUNKS: usize = MAP_SIZE / CHUNK;

/// Meshes for one chunk (cx, cy), per cube layer and pass. With Gouraud
/// lighting (`lighting.rs`), `light` gives the corner light of every cube and
/// `remap` the `.CubeTextureRemapList` substitution.
pub fn build_chunk_meshes(
    map: &Map,
    cube_texture_names: &[String],
    cx: usize,
    cy: usize,
    light: Option<&CityLighting>,
    remap: Option<&[u16]>,
) -> Vec<(usize, Pass, Mesh)> {
    let tex = |t: u16| remap.and_then(|r| r.get(t as usize).copied()).unwrap_or(t);
    // Indexed by [layer][Pass as usize].
    let mut builders: [[MeshBuilder; 4]; 3] = Default::default();
    let s = 1.0 / H_PER_SQUARE;

    for x in cx * CHUNK..(cx + 1) * CHUNK {
        for y in cy * CHUNK..(cy + 1) * CHUNK {
            let mut base = 0.0f32;
            for (layer, cube) in map.square(x, y).cubes.iter().enumerate() {
                let builders = &mut builders[layer];
                let lit = light.map_or([NEUTRAL; 8], |l| *l.cube(x, y, layer));
                // Corner light in quad order.
                let pick = |k: [usize; 4]| k.map(|i| lit[i]);
                if cube.slope != Slope::Off {
                    let corners = top_corners(cube);
                    let [nw, ne, se, sw] = corners.map(|h| (base + h) * s);
                    let (x0, x1, z0, z1) = (x as f32, x as f32 + 1.0, y as f32, y as f32 + 1.0);

                    let top = cube.faces[map::TOP];
                    if let Some(pass) = pass_of(top.kind, true) {
                        let quad = [
                            Vec3::new(x0, nw, z0),
                            Vec3::new(x1, ne, z0),
                            Vec3::new(x1, se, z1),
                            Vec3::new(x0, sw, z1),
                        ];
                        // Alpha tops ignore the flip bit and put u = 0 at west.
                        let uv = if pass == Pass::Alpha {
                            top_uv(cube.slope).map(|t| Vec2::new(64.0 - t.x, t.y))
                        } else {
                            flip_u(top_uv(cube.slope), &top)
                        };
                        let scrolls = cube_texture_names
                            .get(top.texture as usize)
                            .is_some_and(|n| n.starts_with('#'));
                        builders[pass as usize].quad(
                            quad,
                            uv,
                            tex(top.texture),
                            scrolls,
                            pick([NW, NE, SE, SW]),
                        );
                    }

                    // Walls: rectangles base..base+h1 clipped to the slope;
                    // u = 63 at the face's x0/y0 end, v = 0 at base+h1.
                    let h1 = cube.height1 as f32;
                    if h1 > 0.0 {
                        let wall = corners.map(|h| (base + h.min(h1)) * s);
                        let wall_v = corners.map(|h| (h1 - h.min(h1)) / h1 * 64.0);
                        let b = base * s;
                        let [wnw, wne, wse, wsw] = wall;
                        let [vnw, vne, vse, vsw] = wall_v;
                        let sides = [
                            (
                                map::NORTH,
                                [(x0, wnw, z0, vnw), (x1, wne, z0, vne)],
                                [(x1, z0), (x0, z0)],
                                [NW, NE, NE + 4, NW + 4],
                            ),
                            (
                                map::EAST,
                                [(x1, wne, z0, vne), (x1, wse, z1, vse)],
                                [(x1, z1), (x1, z0)],
                                [NE, SE, SE + 4, NE + 4],
                            ),
                            (
                                map::SOUTH,
                                [(x0, wsw, z1, vsw), (x1, wse, z1, vse)],
                                [(x1, z1), (x0, z1)],
                                [SW, SE, SE + 4, SW + 4],
                            ),
                            (
                                map::WEST,
                                [(x0, wnw, z0, vnw), (x0, wsw, z1, vsw)],
                                [(x0, z1), (x0, z0)],
                                [NW, SW, SW + 4, NW + 4],
                            ),
                        ];
                        for (fi, [(ax, ay, az, av), (bx, by, bz, bv)], [(cx, cz), (dx, dz)], k) in
                            sides
                        {
                            let face = cube.faces[fi];
                            let Some(pass) = pass_of(face.kind, false) else {
                                continue;
                            };
                            let quad = [
                                Vec3::new(ax, ay, az),
                                Vec3::new(bx, by, bz),
                                Vec3::new(cx, b, cz),
                                Vec3::new(dx, b, dz),
                            ];
                            let uv = flip_u(
                                [
                                    Vec2::new(64.0, av),
                                    Vec2::new(0.0, bv),
                                    Vec2::new(0.0, 64.0),
                                    Vec2::new(64.0, 64.0),
                                ],
                                &face,
                            );
                            builders[pass as usize].quad(quad, uv, tex(face.texture), false, pick(k));
                        }
                    }
                }
                base += stack_height(cube);
            }
        }
    }
    let mut out = Vec::new();
    for (layer, passes) in builders.into_iter().enumerate() {
        for (pass, b) in Pass::ALL.into_iter().zip(passes) {
            if !b.positions.is_empty() {
                out.push((layer, pass, b.build()));
            }
        }
    }
    out
}
