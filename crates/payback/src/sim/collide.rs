//! Object vs. city collision (docs/re/people.md §5.1; 68k `FUN_00044378`
//! floor, `FUN_000446b8` wall test, `FUN_00044f82` per-step resolution).
//!
//! Depths here are "h-depth": h-units below the reference plane, so
//! `z = depth * 16` and the plane is 1500 h-units above the ground.

use crate::world::City;
use bevy::prelude::*;
use payback_formats::map::{self, Cube, FaceType, MAP_SIZE, Slope};

pub const PLANE: i32 = 1500;
/// World coordinate limit (`FUN_00044f82` clamps x, y to [0, 0x3ffff]).
pub const WORLD_MAX: i32 = 0x3ffff;
/// Step-up height in h-units.
pub const STEP: i32 = 64;

fn top_type(c: &Cube) -> FaceType {
    c.faces[map::TOP].kind
}

/// Surface of the cube under (x, y), as h-depth; `top` is the depth of the
/// cube's base (the top of the cube below).
fn surface(c: &Cube, top: i32, x: i32, y: i32) -> Option<i32> {
    if c.slope == Slope::Off || top_type(c) == FaceType::Off {
        return None;
    }
    let (h, h2) = (c.height1 as i32, c.height2 as i32);
    let base = top - h;
    let (fx, fy) = (x & 0x7ff, y & 0x7ff);
    Some(match c.slope {
        Slope::Flat | Slope::Unknown(_) | Slope::Off => {
            if top_type(c) == FaceType::Reflect {
                top - h2
            } else {
                base
            }
        }
        Slope::Horiz => base - ((fx * (h2 - h)) >> 11),
        Slope::Vert => base - ((fy * (h2 - h)) >> 11),
        Slope::RHoriz => (top - h2) - ((fx * (h - h2)) >> 11),
        Slope::RVert => (top - h2) - ((fy * (h - h2)) >> 11),
    })
}

/// Floor under an object at (x, y, z): the lowest surface, or a higher one
/// within the 64 h-unit step of the object's height. Returns
/// (floor z, cube level, in water). Off-map: no floor.
pub fn floor(city: &City, x: i32, y: i32, z: i32) -> Option<(i32, usize, bool)> {
    if !(0..=WORLD_MAX).contains(&x) || !(0..=WORLD_MAX).contains(&y) {
        return None;
    }
    let sq = city.map.square((x >> 11) as usize, (y >> 11) as usize);
    let mut top = PLANE;
    let mut best = i32::MAX;
    let mut found = None;
    for (i, c) in sq.cubes.iter().enumerate() {
        if let Some(s) = surface(c, top, x, y) {
            if (z / 16 - s).abs() <= best {
                best = STEP;
                found = Some((s * 16, i, top_type(c) == FaceType::Reflect));
            }
        }
        top -= c.height1 as i32;
    }
    found
}

/// Cube level of square (sx, sy) containing h-depth `zh`, or None if the
/// depth is above every cube.
fn level_at(city: &City, sx: usize, sy: usize, zh: i32) -> Option<usize> {
    let sq = city.map.square(sx, sy);
    let mut base = PLANE;
    for (i, c) in sq.cubes.iter().enumerate() {
        let top = base - c.height1 as i32;
        let flat_solid = c.slope == Slope::Flat && top_type(c) != FaceType::Reflect;
        if zh >= top && zh < base && (flat_solid || zh >= base - c.height2 as i32) {
            return Some(i);
        }
        base = top;
    }
    None
}

/// Whether crossing into square (sx, sy) through face `face` at h-depth zh
/// is blocked: the cube at that height exists and the entered face is a
/// wall (type not 0 or 5).
fn entry_blocked(city: &City, sx: i32, sy: i32, face: usize, zh: i32) -> bool {
    if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) {
        return true;
    }
    let Some(level) = level_at(city, sx as usize, sy as usize, zh) else {
        return false;
    };
    let c = &city.map.square(sx as usize, sy as usize).cubes[level];
    c.slope != Slope::Off && c.faces[face].kind != FaceType::Off
}

/// Walk the segment x0 -> x1 at row y, testing each square boundary crossed.
fn seg_blocked_x(city: &City, x0: i32, x1: i32, y: i32, zh: i32) -> bool {
    let sy = y >> 11;
    let (a, b) = (x0 >> 11, x1 >> 11);
    if x1 > x0 {
        (a + 1..=b).any(|sx| entry_blocked(city, sx, sy, map::WEST, zh))
    } else {
        (b..a)
            .rev()
            .any(|sx| entry_blocked(city, sx, sy, map::EAST, zh))
    }
}

fn seg_blocked_y(city: &City, x: i32, y0: i32, y1: i32, zh: i32) -> bool {
    let sx = x >> 11;
    let (a, b) = (y0 >> 11, y1 >> 11);
    if y1 > y0 {
        (a + 1..=b).any(|sy| entry_blocked(city, sx, sy, map::NORTH, zh))
    } else {
        (b..a)
            .rev()
            .any(|sy| entry_blocked(city, sx, sy, map::SOUTH, zh))
    }
}

/// Result of a collision step: which axes hit a wall.
#[derive(Default, Clone, Copy, Debug)]
pub struct Hit {
    pub x: bool,
    pub y: bool,
}

/// Resolve a moved object of radius `r` against walls, given its previous
/// valid position (people.md §5.1). Probes 64 h-units above the origin, so
/// lower obstacles are stepped over. Returns the corrected position.
pub fn resolve_walls(city: &City, pos: IVec3, prev: IVec3, r: i32) -> (IVec3, Hit) {
    let zh = (pos.z - 0x400) >> 4;
    let mut p = pos;
    let mut hit = Hit::default();
    let bx = seg_blocked_x(city, p.x - r, p.x + r, p.y, zh)
        || seg_blocked_x(city, p.x + r, p.x - r, p.y, zh);
    if bx && (p.x - prev.x).signum() != 0 {
        hit.x = true;
        p.x = prev.x;
    }
    let by = seg_blocked_y(city, p.x, p.y - r, p.y + r, zh)
        || seg_blocked_y(city, p.x, p.y + r, p.y - r, zh);
    if by && (p.y - prev.y).signum() != 0 {
        hit.y = true;
        p.y = prev.y;
    }
    // The path from the previous position must also be clear.
    let pzh = (prev.z - 0x400) >> 4;
    if seg_blocked_x(city, prev.x, p.x, prev.y, pzh.min(zh)) {
        hit.x = true;
        p.x = prev.x;
    }
    if seg_blocked_y(city, p.x, prev.y, p.y, pzh.min(zh)) {
        hit.y = true;
        p.y = prev.y;
    }
    p.x = p.x.clamp(0, WORLD_MAX);
    p.y = p.y.clamp(0, WORLD_MAX);
    (p, hit)
}

/// People's version (68k `FUN_00044f82`, people.md §5.1): a blocked axis
/// goes back to the previous position **pushed 0x80 away from the wall**,
/// so a person overlapping a wall (created next to it by a script, or
/// shoved into it) steps out instead of being stuck there.
pub fn resolve_person(city: &City, pos: IVec3, prev: IVec3, r: i32) -> (IVec3, Hit) {
    let zh = (pos.z - 0x400) >> 4;
    let mut p = pos;
    let mut hit = Hit::default();
    // Which side the wall is on: the +x probe enters a wall to the east.
    let east = seg_blocked_x(city, p.x - r, p.x + r, p.y, zh);
    let west = seg_blocked_x(city, p.x + r, p.x - r, p.y, zh);
    if east || west {
        hit.x = true;
        p.x = prev.x
            + match (east, west) {
                (true, false) => -0x80,
                (false, true) => 0x80,
                _ => 0,
            };
    }
    let south = seg_blocked_y(city, p.x, p.y - r, p.y + r, zh);
    let north = seg_blocked_y(city, p.x, p.y + r, p.y - r, zh);
    if south || north {
        hit.y = true;
        p.y = prev.y
            + match (south, north) {
                (true, false) => -0x80,
                (false, true) => 0x80,
                _ => 0,
            };
    }
    // The path from the previous position must also be clear.
    let pzh = (prev.z - 0x400) >> 4;
    if seg_blocked_x(city, prev.x, p.x, prev.y, pzh.min(zh)) {
        hit.x = true;
        p.x = prev.x;
    }
    if seg_blocked_y(city, p.x, prev.y, p.y, pzh.min(zh)) {
        hit.y = true;
        p.y = prev.y;
    }
    p.x = p.x.clamp(0, WORLD_MAX);
    p.y = p.y.clamp(0, WORLD_MAX);
    (p, hit)
}

/// Line of sight between two points at head height (68k `FUN_0009217e`):
/// no wall face is crossed on the way, probing 0x400 above the lower point.
pub fn line_of_sight(city: &City, a: IVec3, b: IVec3) -> bool {
    let zh = (a.z.max(b.z) - 0x400) >> 4;
    let d = b - a;
    let steps = (d.x.abs().max(d.y.abs()) / 256).max(1);
    let mut prev = a;
    for k in 1..=steps {
        let p = a + d * k / steps;
        if seg_blocked_x(city, prev.x, p.x, prev.y, zh) || seg_blocked_y(city, p.x, prev.y, p.y, zh)
        {
            return false;
        }
        prev = p;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use payback_formats::Map;
    use payback_formats::map::{Face, Square};

    fn cube(h: i16, faces: FaceType) -> Cube {
        Cube {
            slope: Slope::Flat,
            height1: h,
            height2: h,
            faces: [Face {
                kind: faces,
                texture: 0,
                flipped: false,
            }; 5],
            ..Default::default()
        }
    }

    fn city(f: impl Fn(usize, usize) -> Square) -> City {
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

    const GROUND_Z: i32 = (1500 - 256) * 16;

    #[test]
    fn floor_on_ground_and_step() {
        let c = city(|x, _| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            if x == 5 {
                sq.cubes[1] = cube(48, FaceType::Normal); // a kerb, steppable
            }
            sq
        });
        assert_eq!(
            floor(&c, 0x400, 0x400, GROUND_Z),
            Some((GROUND_Z, 0, false))
        );
        let (fz, level, _) = floor(&c, 5 * 2048 + 10, 0x400, GROUND_Z).unwrap();
        assert_eq!((fz, level), ((1500 - 304) * 16, 1));
    }

    #[test]
    fn walls_block_by_face() {
        let c = city(|x, y| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            if (x, y) == (5, 5) {
                sq.cubes[1] = cube(512, FaceType::Normal);
            }
            if (x, y) == (7, 5) {
                sq.cubes[1] = cube(512, FaceType::Off); // no walls: passable
            }
            sq
        });
        let prev = IVec3::new(5 * 2048 - 300, 5 * 2048 + 1024, GROUND_Z);
        let into = IVec3::new(5 * 2048 - 100, 5 * 2048 + 1024, GROUND_Z);
        let (p, hit) = resolve_walls(&c, into, prev, 192);
        assert!(hit.x);
        assert_eq!(p.x, prev.x);
        let prev2 = IVec3::new(7 * 2048 - 300, 5 * 2048 + 1024, GROUND_Z);
        let into2 = IVec3::new(7 * 2048 - 100, 5 * 2048 + 1024, GROUND_Z);
        assert!(!resolve_walls(&c, into2, prev2, 192).1.x);
    }

    #[test]
    fn person_steps_out_of_a_wall() {
        // A wall on (4,5)'s east face; a person 20 units inside (5,5)
        // overlaps it and is pushed 0x80 east per step until clear.
        let c = city(|x, y| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            if (x, y) == (4, 5) {
                sq.cubes[1] = cube(512, FaceType::Normal);
            }
            sq
        });
        let mut p = IVec3::new(5 * 2048 + 20, 5 * 2048 + 1024, GROUND_Z);
        for _ in 0..2 {
            let (q, hit) = resolve_person(&c, p, p, 192);
            assert!(hit.x);
            assert_eq!(q.x, p.x + 0x80);
            p = q;
        }
        assert!(!resolve_person(&c, p, p, 192).1.x);
        // Walking into the wall is still blocked.
        let into = IVec3::new(p.x - 200, p.y, p.z);
        assert!(resolve_person(&c, into, p, 192).0.x >= p.x);
    }
}
