//! Simulation-space view of the city: heights and solidity of every cube.
//!
//! Simulation runs in original game units: x east, y south, z up, 256 units
//! per map square. Rendering converts with [`to_render`].

use bevy::prelude::*;
use payback_formats::map::{self, FaceType, MAP_SIZE, Map, Slope};

pub const SQUARE: f32 = 256.0;
pub const WORLD_SIZE: f32 = SQUARE * MAP_SIZE as f32;

/// Game units (x east, y south, z up) to Bevy render space (1 unit = 1 square,
/// Y up, north = -Z).
pub fn to_render(p: Vec3) -> Vec3 {
    Vec3::new(p.x / SQUARE, p.z / SQUARE, p.y / SQUARE)
}

/// Heading in game convention (radians, 0 = north, clockwise) to a Bevy
/// rotation about +Y.
pub fn heading_to_render(heading: f32) -> Quat {
    Quat::from_rotation_y(-heading)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CubeGeom {
    pub solid: bool,
    pub base: f32,
    /// Top heights above `base` at the corners NW, NE, SE, SW.
    pub corners: [f32; 4],
    /// Top face is water (reflective).
    pub water: bool,
    /// Texture index of the top face, if visible.
    pub top_texture: Option<u16>,
}

impl CubeGeom {
    /// Absolute height of the top surface at fractional position (fx, fy)
    /// within the square (0..1 each, fy southward).
    pub fn top_at(&self, fx: f32, fy: f32) -> f32 {
        let [nw, ne, se, sw] = self.corners;
        let n = nw + (ne - nw) * fx;
        let s = sw + (se - sw) * fx;
        self.base + n + (s - n) * fy
    }

    pub fn max_top(&self) -> f32 {
        self.base + self.corners.iter().copied().fold(0.0, f32::max)
    }
}

/// Heights of a cube's top corners above its base: [NW, NE, SE, SW].
pub fn top_corners(cube: &map::Cube) -> [f32; 4] {
    let (a, b) = (cube.height1 as f32, cube.height2 as f32);
    match cube.slope {
        Slope::Vert => [a, a, b, b],
        Slope::RVert => [b, b, a, a],
        Slope::Horiz => [a, b, b, a],
        Slope::RHoriz => [b, a, a, b],
        _ => [a; 4],
    }
}

/// Height a cube adds to its stack.
pub fn stack_height(cube: &map::Cube) -> f32 {
    match cube.slope {
        Slope::Off | Slope::Flat | Slope::Unknown(_) => cube.height1 as f32,
        _ => cube.height1.max(cube.height2) as f32,
    }
}

#[derive(Resource)]
pub struct City {
    pub map: Map,
    cubes: Vec<[CubeGeom; 3]>,
}

impl City {
    pub fn new(map: Map) -> City {
        let mut cubes = Vec::with_capacity(MAP_SIZE * MAP_SIZE);
        for sq in &map.squares {
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
            cubes.push(geoms);
        }
        City { map, cubes }
    }

    /// Cubes of the square containing game position (x, y), with the
    /// fractional position inside it. `None` outside the map.
    pub fn square_at(&self, x: f32, y: f32) -> Option<(&[CubeGeom; 3], f32, f32)> {
        if !(0.0..WORLD_SIZE).contains(&x) || !(0.0..WORLD_SIZE).contains(&y) {
            return None;
        }
        let (sx, sy) = ((x / SQUARE) as usize, (y / SQUARE) as usize);
        let cubes = &self.cubes[sx * MAP_SIZE + sy];
        Some((cubes, x / SQUARE - sx as f32, y / SQUARE - sy as f32))
    }

    pub fn square_cubes(&self, sx: usize, sy: usize) -> &[CubeGeom; 3] {
        &self.cubes[sx * MAP_SIZE + sy]
    }

    /// Highest solid surface at (x, y) whose top is at most `z + step`,
    /// i.e. the floor something at height `z` would stand on. Returns the
    /// surface height and its cube.
    pub fn floor(&self, x: f32, y: f32, z: f32, step: f32) -> Option<(f32, CubeGeom)> {
        let (cubes, fx, fy) = self.square_at(x, y)?;
        cubes
            .iter()
            .filter(|c| c.solid)
            .map(|c| (c.top_at(fx, fy), *c))
            .filter(|(top, _)| *top <= z + step)
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Whether a body occupying heights [z, z + height] at (x, y) intersects
    /// a solid cube, ignoring surfaces within `step` below its feet.
    pub fn blocked(&self, x: f32, y: f32, z: f32, height: f32, step: f32) -> bool {
        let Some((cubes, fx, fy)) = self.square_at(x, y) else {
            return true;
        };
        cubes.iter().filter(|c| c.solid).any(|c| {
            let top = c.top_at(fx, fy);
            top > z + step && c.base < z + height
        })
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
        let (h, _) = city.floor(5.0 * SQUARE, 5.0 * SQUARE, 256.0, 16.0).unwrap();
        assert_eq!(h, 256.0);
        let inside = 10.5 * SQUARE;
        assert!(city.blocked(inside, inside, 256.0, 100.0, 16.0));
        assert!(!city.blocked(9.5 * SQUARE, inside, 256.0, 100.0, 16.0));
        // Standing on the roof.
        assert_eq!(city.floor(inside, inside, 768.0, 16.0).unwrap().0, 768.0);
        assert!(city.blocked(-1.0, 0.0, 0.0, 1.0, 0.0));
    }

    #[test]
    fn slope_interpolates() {
        let city = city_with(|_, _| {
            let mut sq = Square::default();
            sq.cubes[0] = Cube {
                slope: Slope::Horiz,
                height1: 0,
                height2: 256,
                ..flat(0)
            };
            sq
        });
        let (h, _) = city.floor(0.5 * SQUARE, 0.5 * SQUARE, 1000.0, 0.0).unwrap();
        assert_eq!(h, 128.0);
    }
}
