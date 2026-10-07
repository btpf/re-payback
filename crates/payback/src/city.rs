//! Builds renderable geometry for a city map.
//!
//! World layout: one Bevy unit is one map square (256 game units). Square
//! (x, y) covers X in [x, x+1] and Z in [y, y+1]; north is -Z, Y is up.
//! Cubes stack from Y = 0, each occupying its height above the one below.
//!
//! Unconfirmed against the original renderer (see docs/research.md):
//! which end of a sloped cube `height1` belongs to, how much a sloped cube
//! raises the cubes above it, and texture orientation/flip on each face.

use crate::data::GameData;
use crate::world::{stack_height, top_corners};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
use payback_formats::map::{self, Face, FaceType, MAP_SIZE, Map, Slope};

pub const UNITS_PER_SQUARE: f32 = 256.0;

const TILE: u32 = 64;
const ATLAS_TILES: u32 = 32;
const ATLAS_SIZE: u32 = TILE * ATLAS_TILES;

/// All cube textures packed into one 2048x2048 atlas, 64x64 per tile, in
/// `.CubeTextureList` order. Colour 0 is transparent (for genlocked faces).
pub fn build_cube_atlas(data: &GameData) -> Image {
    let mut rgba = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
    for (i, entry) in data.cube_textures.entries.iter().enumerate() {
        let Ok(raw) = data.read(format!("CubeTextures/{}", entry.file)) else {
            warn!("missing cube texture {}", entry.file);
            continue;
        };
        let Ok(img) = payback_formats::IndexedImage::cube_texture(&raw) else {
            warn!("short cube texture {}", entry.file);
            continue;
        };
        let pixels = img.to_rgba(&data.palette, Some(0));
        let (tx, ty) = (i as u32 % ATLAS_TILES, i as u32 / ATLAS_TILES);
        for row in 0..TILE {
            let src = (row * TILE * 4) as usize;
            let dst = (((ty * TILE + row) * ATLAS_SIZE + tx * TILE) * 4) as usize;
            rgba[dst..dst + (TILE * 4) as usize]
                .copy_from_slice(&pixels[src..src + (TILE * 4) as usize]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// UV rectangle of atlas tile `texture`, inset by half a texel so nearest
/// sampling never bleeds into the neighbouring tile.
fn tile_uv(texture: u16) -> (Vec2, Vec2) {
    let i = texture as u32;
    let half = 0.5 / ATLAS_SIZE as f32;
    let min = Vec2::new(
        (i % ATLAS_TILES * TILE) as f32 / ATLAS_SIZE as f32,
        (i / ATLAS_TILES * TILE) as f32 / ATLAS_SIZE as f32,
    );
    let max = min + Vec2::splat(TILE as f32 / ATLAS_SIZE as f32);
    (min + half, max - half)
}

/// Which render pass a face belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pass {
    Opaque,
    /// Genlocked: colour 0 transparent.
    Masked,
    /// 50% transparent.
    Alpha,
    /// Water / reflective surfaces.
    Reflect,
}

impl Pass {
    pub const ALL: [Pass; 4] = [Pass::Opaque, Pass::Masked, Pass::Alpha, Pass::Reflect];

    fn of(kind: FaceType) -> Option<Pass> {
        match kind {
            FaceType::Off => None,
            FaceType::Normal | FaceType::Unknown(_) => Some(Pass::Opaque),
            FaceType::Genlocked => Some(Pass::Masked),
            FaceType::Alpha => Some(Pass::Alpha),
            FaceType::Reflect => Some(Pass::Reflect),
        }
    }
}

#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    /// Quad with corners in order top-left, top-right, bottom-right,
    /// bottom-left as seen from the front; texture mapped to match.
    fn quad(&mut self, corners: [Vec3; 4], face: &Face) {
        let (min, max) = tile_uv(face.texture);
        let (mut l, mut r) = (min.x, max.x);
        if face.flipped {
            std::mem::swap(&mut l, &mut r);
        }
        let uv = [[l, min.y], [r, min.y], [r, max.y], [l, max.y]];
        let normal = (corners[1] - corners[0])
            .cross(corners[3] - corners[0])
            .normalize_or_zero();
        let base = self.positions.len() as u32;
        for (c, t) in corners.iter().zip(uv) {
            self.positions.push(c.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push(t);
        }
        // Both windings: faces are single-sided in the original but our
        // corner order is not yet verified, so avoid culling surprises.
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    fn build(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

pub fn build_city_meshes(map: &Map) -> Vec<(Pass, Mesh)> {
    let mut builders: Vec<(Pass, MeshBuilder)> = Pass::ALL
        .iter()
        .map(|&p| (p, MeshBuilder::default()))
        .collect();
    let s = 1.0 / UNITS_PER_SQUARE;

    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let mut base = 0.0f32;
            for cube in &map.square(x, y).cubes {
                if cube.slope != Slope::Off {
                    let [nw, ne, se, sw] = top_corners(cube).map(|h| (base + h) * s);
                    let b = base * s;
                    let (x0, x1, z0, z1) = (x as f32, x as f32 + 1.0, y as f32, y as f32 + 1.0);
                    let quads = [
                        // Top, seen from above: north edge at the top of the texture.
                        [
                            Vec3::new(x0, nw, z0),
                            Vec3::new(x1, ne, z0),
                            Vec3::new(x1, se, z1),
                            Vec3::new(x0, sw, z1),
                        ],
                        // North face, seen from the north.
                        [
                            Vec3::new(x1, ne, z0),
                            Vec3::new(x0, nw, z0),
                            Vec3::new(x0, b, z0),
                            Vec3::new(x1, b, z0),
                        ],
                        // East face, seen from the east.
                        [
                            Vec3::new(x1, se, z1),
                            Vec3::new(x1, ne, z0),
                            Vec3::new(x1, b, z0),
                            Vec3::new(x1, b, z1),
                        ],
                        // South face, seen from the south.
                        [
                            Vec3::new(x0, sw, z1),
                            Vec3::new(x1, se, z1),
                            Vec3::new(x1, b, z1),
                            Vec3::new(x0, b, z1),
                        ],
                        // West face, seen from the west.
                        [
                            Vec3::new(x0, nw, z0),
                            Vec3::new(x0, sw, z1),
                            Vec3::new(x0, b, z1),
                            Vec3::new(x0, b, z0),
                        ],
                    ];
                    for (face, corners) in [map::TOP, map::NORTH, map::EAST, map::SOUTH, map::WEST]
                        .into_iter()
                        .map(|i| (&cube.faces[i], quads[i]))
                    {
                        if let Some(pass) = Pass::of(face.kind) {
                            let b = &mut builders.iter_mut().find(|(p, _)| *p == pass).unwrap().1;
                            b.quad(corners, face);
                        }
                    }
                }
                base += stack_height(cube);
            }
        }
    }
    builders
        .into_iter()
        .filter(|(_, b)| !b.positions.is_empty())
        .map(|(p, b)| (p, b.build()))
        .collect()
}
