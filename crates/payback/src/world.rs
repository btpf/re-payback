//! Simulation-space view of the city, in the original game's units
//! (docs/re/rendering.md §0):
//!
//! - x, y: 2048 (`0x800`) per map square, x east, y south.
//! - z: object depth, **downward**, 16 per h-unit, measured from a plane
//!   1500 h-units above the bottom of cube 0: `z = (1500 - H) << 4`.
//! - H ("h-units"): cube heights; 256 per square, so a ground cube is a cube.
//! - Angles: 5760 (`0x1680`) per turn; heading 0 = +y (south), 0x5a0 = +x.
//!
//! Rendering converts with [`to_render`] (1 Bevy unit = 1 square, Y up).

use bevy::prelude::*;
use payback_formats::map::{self, FaceType, MAP_SIZE, Map, Slope};

pub const SQUARE: f32 = 2048.0;
pub const H_PER_SQUARE: f32 = 256.0;
pub const WORLD_SIZE: f32 = SQUARE * MAP_SIZE as f32;
pub const ANGLE_TURN: f32 = 5760.0;
/// Reference plane for z, in h-units above the bottom of cube 0.
pub const Z_PLANE: f32 = 1500.0;

/// h-units above the ground -> original downward z.
pub fn z_of_h(h: f32) -> f32 {
    (Z_PLANE - h) * 16.0
}

/// Original downward z -> h-units.
pub fn h_of_z(z: f32) -> f32 {
    Z_PLANE - z / 16.0
}

/// Original position (x, y, z-down) to Bevy render space.
pub fn to_render(p: Vec3) -> Vec3 {
    Vec3::new(p.x / SQUARE, h_of_z(p.z) / H_PER_SQUARE, p.y / SQUARE)
}

/// Original heading (5760 units/turn, 0 = south, 0x5a0 = east) to a Bevy
/// rotation about +Y for models whose forward axis is object +y.
pub fn heading_to_render(heading: f32) -> Quat {
    Quat::from_rotation_y(heading / ANGLE_TURN * std::f32::consts::TAU)
}

/// Unit direction (x, y) of an original heading.
pub fn heading_dir(heading: f32) -> Vec2 {
    let a = heading / ANGLE_TURN * std::f32::consts::TAU;
    Vec2::new(a.sin(), a.cos())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CubeGeom {
    pub solid: bool,
    /// Bottom of the cube, h-units.
    pub base: f32,
    /// Top heights above `base` at the corners NW, NE, SE, SW (h-units).
    pub corners: [f32; 4],
    /// Top face is water/reflective (face type 4).
    pub water: bool,
    /// Texture index of the top face, if visible.
    pub top_texture: Option<u16>,
}

impl CubeGeom {
    /// Absolute top height (h-units) at fractional position (fx, fy) within
    /// the square (0..1 each, fy southward).
    pub fn top_at(&self, fx: f32, fy: f32) -> f32 {
        let [nw, ne, se, sw] = self.corners;
        let n = nw + (ne - nw) * fx;
        let s = sw + (se - sw) * fx;
        self.base + n + (s - n) * fy
    }
}

/// Heights of a cube's top corners above its base: [NW, NE, SE, SW]
/// (rendering.md §1). Reflect flat tops (water) sit at h2.
pub fn top_corners(cube: &map::Cube) -> [f32; 4] {
    let (a, b) = (cube.height1 as f32, cube.height2 as f32);
    match cube.slope {
        Slope::Vert => [a, a, b, b],
        Slope::RVert => [b, b, a, a],
        Slope::Horiz => [a, b, b, a],
        Slope::RHoriz => [b, a, a, b],
        _ if cube.faces[map::TOP].kind == FaceType::Reflect => [b; 4],
        _ => [a; 4],
    }
}

/// Height a cube adds to its stack: always h1 (rendering.md §1).
pub fn stack_height(cube: &map::Cube) -> f32 {
    cube.height1 as f32
}

#[derive(Resource)]
pub struct City {
    pub map: Map,
    cubes: Vec<[CubeGeom; 3]>,
}

fn square_geoms(sq: &map::Square) -> [CubeGeom; 3] {
    let mut base = 0.0;
    let mut geoms = [CubeGeom::default(); 3];
    for (g, cube) in geoms.iter_mut().zip(&sq.cubes) {
        let top = cube.faces[map::TOP];
        *g = CubeGeom {
            solid: cube.slope != Slope::Off,
            base,
            corners: top_corners(cube),
            water: top.kind == FaceType::Reflect,
            top_texture: top.visible().then_some(top.texture),
        };
        base += stack_height(cube);
    }
    geoms
}

impl City {
    pub fn new(map: Map) -> City {
        let cubes = map.squares.iter().map(square_geoms).collect();
        City { map, cubes }
    }

    /// Recompute derived geometry after the map square was edited.
    pub fn refresh_square(&mut self, sx: usize, sy: usize) {
        let i = sx * MAP_SIZE + sy;
        self.cubes[i] = square_geoms(&self.map.squares[i]);
    }

    /// Square containing world (x, y) and the fractional position in it.
    pub fn locate(x: f32, y: f32) -> Option<(usize, usize, f32, f32)> {
        if !(0.0..WORLD_SIZE).contains(&x) || !(0.0..WORLD_SIZE).contains(&y) {
            return None;
        }
        let (sx, sy) = ((x / SQUARE) as usize, (y / SQUARE) as usize);
        Some((sx, sy, x / SQUARE - sx as f32, y / SQUARE - sy as f32))
    }

    pub fn square_cubes(&self, sx: usize, sy: usize) -> &[CubeGeom; 3] {
        &self.cubes[sx * MAP_SIZE + sy]
    }

    /// Highest solid surface (h-units) at world (x, y) whose top is at most
    /// `h + step`: what something at height `h` stands on, and its layer.
    pub fn floor(&self, x: f32, y: f32, h: f32, step: f32) -> Option<(f32, usize, CubeGeom)> {
        let (sx, sy, fx, fy) = Self::locate(x, y)?;
        self.square_cubes(sx, sy)
            .iter()
            .enumerate()
            .filter(|(_, c)| c.solid)
            .map(|(i, c)| (c.top_at(fx, fy), i, *c))
            .filter(|(top, _, _)| *top <= h + step)
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Whether a body occupying heights [h, h + height] at world (x, y)
    /// intersects a solid cube, ignoring surfaces within `step` of its feet.
    /// Off-map counts as blocked.
    pub fn blocked(&self, x: f32, y: f32, h: f32, height: f32, step: f32) -> bool {
        let Some((sx, sy, fx, fy)) = Self::locate(x, y) else {
            return true;
        };
        self.square_cubes(sx, sy)
            .iter()
            .filter(|c| c.solid)
            .any(|c| {
                let top = c.top_at(fx, fy);
                top > h + step && c.base < h + height
            })
    }

    /// Whether the surface under world (x, y) at height h is water.
    pub fn water_at(&self, x: f32, y: f32, h: f32) -> bool {
        self.floor(x, y, h, 64.0).is_some_and(|(_, _, c)| c.water)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payback_formats::map::{Cube, Face, Square};

    fn city_with(f: impl Fn(usize, usize) -> Square) -> City {
        let mut squares = Vec::new();
        for x in 0..MAP_SIZE {
            for y in 0..MAP_SIZE {
                squares.push(f(x, y));
            }
        }
        City::new(Map {
            version: 1,
            night: false,
            variant: 0,
            squares,
        })
    }

    fn flat(h: i16) -> Cube {
        Cube {
            slope: Slope::Flat,
            height1: h,
            height2: h,
            faces: [Face {
                kind: FaceType::Normal,
                texture: 0,
                flipped: false,
            }; 5],
            ..Default::default()
        }
    }

    #[test]
    fn floor_and_walls() {
        // Ground everywhere; a 512-high building on top at square (10, 10).
        let city = city_with(|x, y| {
            let mut sq = Square::default();
            sq.cubes[0] = flat(256);
            if (x, y) == (10, 10) {
                sq.cubes[1] = flat(512);
            }
            sq
        });
        let (h, layer, _) = city.floor(5.0 * SQUARE, 5.0 * SQUARE, 256.0, 16.0).unwrap();
        assert_eq!((h, layer), (256.0, 0));
        let inside = 10.5 * SQUARE;
        assert!(city.blocked(inside, inside, 256.0, 100.0, 16.0));
        assert!(!city.blocked(9.5 * SQUARE, inside, 256.0, 100.0, 16.0));
        assert_eq!(city.floor(inside, inside, 768.0, 16.0).unwrap().0, 768.0);
        assert!(city.blocked(-1.0, 0.0, 0.0, 1.0, 0.0));
    }

    #[test]
    fn slopes_and_stacking() {
        let city = city_with(|_, _| {
            let mut sq = Square::default();
            sq.cubes[0] = Cube {
                slope: Slope::Horiz,
                height1: 0,
                height2: 256,
                ..flat(0)
            };
            sq.cubes[1] = flat(100);
            sq
        });
        // Horiz: west edge h1 = 0, east edge h2 = 256.
        let (h, _, _) = city.floor(0.5 * SQUARE, 0.5 * SQUARE, 1000.0, 0.0).unwrap();
        assert_eq!(h, 128.0);
        // The cube above stacks on h1 (0), not max(h1, h2).
        assert_eq!(city.square_cubes(0, 0)[1].base, 0.0);
    }

    #[test]
    fn water_sits_at_h2() {
        let city = city_with(|_, _| {
            let mut sq = Square::default();
            sq.cubes[0] = flat(256);
            sq.cubes[0].height2 = 0;
            sq.cubes[0].faces[map::TOP].kind = FaceType::Reflect;
            sq
        });
        let c = city.square_cubes(0, 0)[0];
        assert!(c.water);
        assert_eq!(c.top_at(0.5, 0.5), 0.0);
        assert_eq!(city.square_cubes(0, 0)[1].base, 256.0);
    }

    #[test]
    fn units() {
        assert_eq!(z_of_h(256.0), (1500.0 - 256.0) * 16.0);
        assert_eq!(
            to_render(Vec3::new(SQUARE, 2.0 * SQUARE, z_of_h(256.0))),
            Vec3::new(1.0, 1.0, 2.0)
        );
        let d = heading_dir(1440.0);
        assert!((d - Vec2::X).length() < 1e-5);
    }
}
