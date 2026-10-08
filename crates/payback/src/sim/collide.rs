//! Object vs. city collision (docs/re/people.md §5.1-5.2; Mac
//! `CheckForCubeCollision` @ 100af980 = 68k `FUN_00044f82`, `GetCollideInfo`
//! @ 100af0e0 = 68k `FUN_000446b8`, `GetCubeNumFast` @ 100ae920,
//! `GetCubeInfo` @ 100ae3c0 / `GetCubeInfo2` @ 100ae580 = 68k
//! `FUN_00044378` / `FUN_000444d0`, `AddCollideObj` @ 10055c70 = 68k
//! `FUN_000cdfdc`).
//!
//! Depths here are "h-depth": h-units below the reference plane, so
//! `z = depth * 16` and the plane is 1500 h-units above the ground.

use super::Trig;
use crate::objects::{ObjectLibrary, SCENERY};
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

/// The face type number (`cube + 9..0xd & 0x7f`).
fn face_bits(k: FaceType) -> u8 {
    match k {
        FaceType::Off => 0,
        FaceType::Normal => 1,
        FaceType::Genlocked => 2,
        FaceType::Alpha => 3,
        FaceType::Reflect => 4,
        FaceType::Unknown(n) => n,
    }
}

/// Surface of the cube under (x, y), as h-depth; `top` is the depth of the
/// cube's base (the top of the cube below). Slope values other than 1-5
/// take the RVert formula, as the original's `else` branch does
/// (`GetCubeInfo2` @ 100ae580, 68k `FUN_00044378`).
fn surface(c: &Cube, top: i32, x: i32, y: i32) -> Option<i32> {
    if c.slope == Slope::Off || top_type(c) == FaceType::Off {
        return None;
    }
    let (h, h2) = (c.height1 as i32, c.height2 as i32);
    let base = top - h;
    let (fx, fy) = (x & 0x7ff, y & 0x7ff);
    Some(match c.slope {
        Slope::Flat | Slope::Off => {
            if top_type(c) == FaceType::Reflect {
                top - h2
            } else {
                base
            }
        }
        Slope::Horiz => base - ((fx * (h2 - h)) >> 11),
        Slope::Vert => base - ((fy * (h2 - h)) >> 11),
        Slope::RHoriz => (top - h2) - ((fx * (h - h2)) >> 11),
        Slope::RVert | Slope::Unknown(_) => (top - h2) - ((fy * (h - h2)) >> 11),
    })
}

/// Floor under an object at (x, y, z): the lowest surface, or a higher one
/// within the 64 h-unit step of the object's height (`GetCubeInfo` @
/// 100ae3c0, 68k `FUN_00044378`, which compares `z >> 4`). Returns (floor
/// z, cube level, in water). Off-map: no floor.
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
            if ((z >> 4) - s).abs() <= best {
                best = STEP;
                found = Some((s * 16, i, top_type(c) == FaceType::Reflect));
            }
        }
        top -= c.height1 as i32;
    }
    found
}

/// Ground height under a vehicle's box point (`GetCubeInfo2` @ 100ae580,
/// 68k `FUN_000444d0`): the surface nearest the object's height `z`, a
/// higher cube winning within 32 h-units of the best so far. Returns (z,
/// level); off the map the object's own z at level 0.
pub fn ground_at(city: &City, x: i32, y: i32, z: i32) -> (i32, usize) {
    if !(0..=WORLD_MAX).contains(&x) || !(0..=WORLD_MAX).contains(&y) {
        return (z, 0);
    }
    let zh = z >> 4;
    let sq = city.map.square((x >> 11) as usize, (y >> 11) as usize);
    let mut top = PLANE;
    let mut best = 1_000_000_000;
    let mut out = (z, 0);
    for (i, c) in sq.cubes.iter().enumerate() {
        if let Some(s) = surface(c, top, x, y) {
            let d = (zh - s).abs();
            if d <= best {
                best = d + 0x20;
                out = (s << 4, i);
            }
        }
        top -= c.height1 as i32;
    }
    out
}

/// Cube level of square (sx, sy) at h-depth `zh` (`GetCubeNumFast` @
/// 100ae920): the first cube whose top is at or above the depth, or, for a
/// cube that is not a solid flat one (slopes, water), whose other-end
/// height is. None above every cube.
fn cube_num(city: &City, sx: usize, sy: usize, zh: i32) -> Option<usize> {
    let sq = city.map.square(sx, sy);
    let mut base = PLANE;
    for (i, c) in sq.cubes.iter().enumerate() {
        let top = base - c.height1 as i32;
        if zh >= top {
            return Some(i);
        }
        let flat_solid = c.slope == Slope::Flat && top_type(c) != FaceType::Reflect;
        if !flat_solid && zh >= base - c.height2 as i32 {
            return Some(i);
        }
        base = top;
    }
    None
}

/// A glass cube (a face of type 3) smashed by a non-person object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glass {
    pub sx: usize,
    pub sy: usize,
    pub level: usize,
}

/// Result of a collision step: which axes hit a wall, and on which side.
#[derive(Default, Clone, Copy, Debug)]
pub struct Hit {
    pub x: bool,
    pub y: bool,
    /// Push direction per axis (the original's hit bytes): +1 = the wall
    /// is on the negative side (blocked moving −), −1 = on the positive
    /// side, 0 = no hit.
    pub side: IVec2,
}

/// Crossing into square (sx, sy) through `face` at h-depth `zh`: whether
/// it is blocked. The cube at that height must exist and the face type
/// must not be 0 or 5. Glass (type 3) breaks instead when `glass` collects
/// it (any object but a person; `GetCollideInfo` @ 100af0e0).
fn crossing(
    city: &City,
    sx: i32,
    sy: i32,
    face: usize,
    zh: i32,
    glass: &mut Option<&mut Vec<Glass>>,
) -> bool {
    // A crossing point off the side of the map: the original reads the
    // neighbouring memory; treat it as open.
    if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) {
        return false;
    }
    let (sx, sy) = (sx as usize, sy as usize);
    let Some(level) = cube_num(city, sx, sy, zh) else {
        return false;
    };
    let c = &city.map.square(sx, sy).cubes[level];
    if c.slope == Slope::Off {
        return false;
    }
    match face_bits(c.faces[face].kind) {
        0 | 5 => false,
        3 => match glass {
            Some(list) => {
                let g = Glass { sx, sy, level };
                if !list.contains(&g) {
                    list.push(g);
                }
                false
            }
            None => true,
        },
        _ => true,
    }
}

/// Segment wall test (`GetCollideInfo` @ 100af0e0, 68k `FUN_000446b8`):
/// walks from `b` to `a` (x, y, h-depth), interpolating the other
/// coordinate and the depth at each square boundary. Only boundaries
/// beyond the reference square `rf` count (in the walking direction), so
/// the walls of the square an object started in never hold it. Returns the
/// push side per axis (+1: blocked moving −; −1: blocked moving +).
/// Deltas of 0xae9d or more are not tested at all.
fn collide_info(
    city: &City,
    a: IVec3,
    b: IVec3,
    rf: IVec2,
    glass: &mut Option<&mut Vec<Glass>>,
) -> IVec2 {
    let (mut dx, mut dy, dz) = ((a.x - b.x) as i64, (a.y - b.y) as i64, (a.z - b.z) as i64);
    let mut out = IVec2::ZERO;
    if dx.abs() >= 0xae9d || dy.abs() >= 0xae9d || dz.abs() >= 0xae9d {
        return out;
    }
    if dx == 0 {
        dx = 1;
    }
    if dy == 0 {
        dy = 1;
    }
    let c = |v: i32| v.clamp(0, WORLD_MAX) as i64;
    let (bx, by, ax, ay) = (c(b.x), c(b.y), c(a.x), c(a.y));
    let bz = b.z as i64;
    let n = MAP_SIZE as i64;
    // x boundaries.
    let (mut sx, ex) = (bx >> 11, ax >> 11);
    if sx < ex {
        let d0 = ((bx + 0x800) & !0x7ff) - bx;
        let (mut ty, mut tz) = (d0 * dy, d0 * dz);
        while sx < ex {
            sx += 1;
            if sx < n {
                let qy = (by + ty / dx) >> 11;
                let zh = (bz + tz / dx) as i32;
                if sx > rf.x as i64
                    && crossing(city, sx as i32, qy as i32, map::WEST, zh, glass)
                {
                    out.x = -1;
                }
            } else {
                out.x = -1;
            }
            ty += dy * 0x800;
            tz += dz * 0x800;
        }
    } else if ex < sx {
        let d0 = (bx & !0x7ff) - bx;
        let (mut ty, mut tz) = (d0 * dy, d0 * dz);
        while ex < sx {
            sx -= 1;
            if sx < 0 {
                out.x = 1;
            } else {
                let qy = (by + ty / dx) >> 11;
                let zh = (bz + tz / dx) as i32;
                if sx < rf.x as i64
                    && crossing(city, sx as i32, qy as i32, map::EAST, zh, glass)
                {
                    out.x = 1;
                }
            }
            ty -= dy * 0x800;
            tz -= dz * 0x800;
        }
    }
    // y boundaries.
    let (mut sy, ey) = (by >> 11, ay >> 11);
    if sy < ey {
        let d0 = ((by + 0x800) & !0x7ff) - by;
        let (mut tx, mut tz) = (d0 * dx, d0 * dz);
        while sy < ey {
            sy += 1;
            if sy < n {
                let qx = (bx + tx / dy) >> 11;
                let zh = (bz + tz / dy) as i32;
                if sy > rf.y as i64
                    && crossing(city, qx as i32, sy as i32, map::NORTH, zh, glass)
                {
                    out.y = -1;
                }
            } else {
                out.y = -1;
            }
            tx += dx * 0x800;
            tz += dz * 0x800;
        }
    } else if ey < sy {
        let d0 = (by & !0x7ff) - by;
        let (mut tx, mut tz) = (d0 * dx, d0 * dz);
        while ey < sy {
            sy -= 1;
            if sy < 0 {
                out.y = 1;
            } else {
                let qx = (bx + tx / dy) >> 11;
                let zh = (bz + tz / dy) as i32;
                if sy < rf.y as i64
                    && crossing(city, qx as i32, sy as i32, map::SOUTH, zh, glass)
                {
                    out.y = 1;
                }
            }
            tx -= dx * 0x800;
            tz -= dz * 0x800;
        }
    }
    out
}

/// `MergeCollide` @ 100af7b0: a later non-zero hit replaces an earlier one.
fn merge(a: IVec2, b: IVec2) -> IVec2 {
    IVec2::new(
        if b.x != 0 { b.x } else { a.x },
        if b.y != 0 { b.y } else { a.y },
    )
}

fn square_of(p: IVec3) -> IVec2 {
    IVec2::new(p.x >> 11, p.y >> 11)
}

/// Resolve a moved object of radius `r` against walls, given its previous
/// valid position (people.md §5.1). Probes 64 h-units above the origin, so
/// lower obstacles are stepped over. Glass blocks. Returns the corrected
/// position.
pub fn resolve_walls(city: &City, pos: IVec3, prev: IVec3, r: i32) -> (IVec3, Hit) {
    resolve(city, pos, prev, r, None)
}

/// [`resolve_walls`] for objects that smash glass (vehicles and
/// projectiles: `GetCollideInfo` breaks a type-3 face for any class but
/// people). Smashed cubes are added to `glass`; the caller removes them
/// ([`smash_glass`]).
pub fn resolve_walls_glass(
    city: &City,
    pos: IVec3,
    prev: IVec3,
    r: i32,
    glass: &mut Vec<Glass>,
) -> (IVec3, Hit) {
    resolve(city, pos, prev, r, Some(glass))
}

/// The port's simplified vehicle box (`CheckForCubeCollision` vehicle
/// branch tests 8 box edges plus the centre path): a cross of half-size
/// `r` against the walls of the neighbouring squares (reference square =
/// the centre's), then the path from the previous position (reference =
/// the previous square). A blocked axis goes back to the previous
/// position. Probes off the map count as a wall hit, as on the Mac
/// (`CheckForCubeCollision` @ 100af980 clamps the 3×3 points and flags
/// them; 68k `FUN_00044f82` / OS4 `FUN_01070868` flag the edge only on the
/// earlier sub-steps, so there a car could leave the map).
fn resolve(
    city: &City,
    pos: IVec3,
    prev: IVec3,
    r: i32,
    mut glass: Option<&mut Vec<Glass>>,
) -> (IVec3, Hit) {
    let zh = (pos.z - 0x400) >> 4;
    let pzh = (prev.z - 0x400) >> 4;
    let mut p = pos;
    let mut hit = Hit::default();
    if r > 0 {
        let here = square_of(p);
        let at = |x: i32, y: i32| IVec3::new(x, y, zh);
        let s = merge(
            collide_info(city, at(p.x - r, p.y), at(p.x + r, p.y), here, &mut glass),
            collide_info(city, at(p.x + r, p.y), at(p.x - r, p.y), here, &mut glass),
        );
        if s.x != 0 && p.x != prev.x {
            hit.x = true;
            hit.side.x = s.x;
            p.x = prev.x;
        }
        let s = merge(
            collide_info(city, at(p.x, p.y - r), at(p.x, p.y + r), here, &mut glass),
            collide_info(city, at(p.x, p.y + r), at(p.x, p.y - r), here, &mut glass),
        );
        if s.y != 0 && p.y != prev.y {
            hit.y = true;
            hit.side.y = s.y;
            p.y = prev.y;
        }
    }
    // The path from the previous position must also be clear.
    let from = square_of(prev);
    for _ in 0..3 {
        let a = IVec3::new(p.x, p.y, zh);
        let b = IVec3::new(prev.x, prev.y, pzh);
        let s = collide_info(city, a, b, from, &mut glass);
        if s.x != 0 {
            hit.x = true;
            hit.side.x = s.x;
            p.x = prev.x;
        }
        if s.y != 0 {
            hit.y = true;
            hit.side.y = s.y;
            p.y = prev.y;
        }
        if s == IVec2::ZERO {
            break;
        }
    }
    // The map edge, approached.
    let edge = |v: i32, was: i32| -> i32 {
        if v - r < 0 && v < was {
            1
        } else if v + r > WORLD_MAX && v > was {
            -1
        } else {
            0
        }
    };
    let e = IVec2::new(edge(p.x, prev.x), edge(p.y, prev.y));
    if e.x != 0 {
        hit.x = true;
        hit.side.x = e.x;
        p.x = prev.x;
    }
    if e.y != 0 {
        hit.y = true;
        hit.side.y = e.y;
        p.y = prev.y;
    }
    p.x = p.x.clamp(0, WORLD_MAX);
    p.y = p.y.clamp(0, WORLD_MAX);
    (p, hit)
}

/// People's version (`CheckForCubeCollision` person branch, 68k
/// `FUN_00044f82`, people.md §5.1). Four radius probes against the walls
/// beyond the previous square; a blocked axis goes back to the previous
/// position **pushed 0x80 away from the wall** (when both sides are
/// blocked the later probe wins: −0x80), so a person overlapping a wall
/// steps out instead of being stuck there. Then one 2D segment from the
/// previous position, reverting each blocked axis until it is clear.
/// Glass blocks people. The map clamp counts as a hit.
pub fn resolve_person(city: &City, pos: IVec3, prev: IVec3, r: i32) -> (IVec3, Hit) {
    let zh = (pos.z - 0x400) >> 4;
    let pzh = (prev.z - 0x400) >> 4;
    let from = square_of(prev);
    let mut p = pos;
    let mut hit = Hit::default();
    let mut none = None;
    let at = |x: i32, y: i32| IVec3::new(x, y, zh);
    let s = merge(
        merge(
            collide_info(city, at(p.x - r, p.y), at(p.x + r, p.y), from, &mut none),
            collide_info(city, at(p.x + r, p.y), at(p.x - r, p.y), from, &mut none),
        ),
        merge(
            collide_info(city, at(p.x, p.y - r), at(p.x, p.y + r), from, &mut none),
            collide_info(city, at(p.x, p.y + r), at(p.x, p.y - r), from, &mut none),
        ),
    );
    let step = |side: i32| if side < 1 { -0x80 } else { 0x80 };
    if s.x != 0 {
        hit.x = true;
        hit.side.x = s.x;
        p.x = prev.x + step(s.x);
    }
    if s.y != 0 {
        hit.y = true;
        hit.side.y = s.y;
        p.y = prev.y + step(s.y);
    }
    for _ in 0..3 {
        let a = IVec3::new(p.x, p.y, zh);
        let b = IVec3::new(prev.x, prev.y, pzh);
        let s = collide_info(city, a, b, from, &mut none);
        if s.x != 0 {
            hit.x = true;
            hit.side.x = s.x;
            p.x = prev.x;
        }
        if s.y != 0 {
            hit.y = true;
            hit.side.y = s.y;
            p.y = prev.y;
        }
        if s == IVec2::ZERO {
            break;
        }
    }
    for (v, axis_hit, side) in [
        (&mut p.x, &mut hit.x, &mut hit.side.x),
        (&mut p.y, &mut hit.y, &mut hit.side.y),
    ] {
        if *v > WORLD_MAX {
            *v = WORLD_MAX;
            *axis_hit = true;
            *side = -1;
        } else if *v < 0 {
            *v = 0;
            *axis_hit = true;
            *side = 1;
        }
    }
    (p, hit)
}

/// Line of sight between two points at head height (`LineOfSight` @
/// 100af820, 68k `FUN_0009217e`): no wall face (or glass) is crossed on
/// the way from `a`, probing 0x400 above the lower point. As in the
/// original, points 0xae9d or more apart on an axis always see each other.
pub fn line_of_sight(city: &City, a: IVec3, b: IVec3) -> bool {
    let zh = (a.z.max(b.z) - 0x400) >> 4;
    let s = collide_info(
        city,
        IVec3::new(b.x, b.y, zh),
        IVec3::new(a.x, a.y, zh),
        square_of(a),
        &mut None,
    );
    s == IVec2::ZERO
}

/// Remove smashed glass cubes: the cube goes (`cube + 8 = 0`), sound 0x31
/// BrokenGlass plays at the square centre (volume 20000000, pitch 7000,
/// z 20000; `GetCollideInfo` @ 100af0e0), and the "cube smashed" trigger
/// (0x2a) sees it.
pub fn smash_glass(
    commands: &mut Commands,
    city: &City,
    edits: &mut MessageWriter<crate::mapedit::MapEdit>,
    triggers: &mut crate::mission::Triggers,
    glass: &[Glass],
) {
    use crate::audio::{Sound, SoundPos, id};
    use crate::mapedit::{EditOp, MapEdit};
    for g in glass {
        let c = &city.map.square(g.sx, g.sy).cubes[g.level];
        edits.write(MapEdit {
            sx: g.sx,
            sy: g.sy,
            cube: g.level,
            op: EditOp::Cube {
                h1: c.height1,
                h2: c.height2,
                scenery: c.scenery,
                rotation: c.rotation,
                slope: 0,
            },
        });
        let at = Vec3::new(
            (g.sx * 0x800 + 0x400) as f32,
            (g.sy * 0x800 + 0x400) as f32,
            20000.0,
        );
        commands.spawn(Sound::once(id::BROKEN_GLASS, 20_000_000.0, 7000.0, SoundPos::At(at)));
        triggers.report(
            crate::mission::kind::CUBE_SMASHED,
            g.sx as i32,
            g.sy as i32,
            g.level as i32,
        );
    }
}

/// Shape of an object for the scenery test (`AddCollideObj`): a person is
/// one circle of its radius; a vehicle four along its heading at
/// {0, r − rear, front − r, front/2 − r} with the half-width as radius.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub radius: i32,
    pub front: i32,
    pub rear: i32,
    pub vehicle: bool,
    /// The RC car's footprint is one square larger on every side.
    pub rc: bool,
    /// Per circle: its ground height minus the object's z. Circles 500 or
    /// more above (or below) their ground are not tested.
    pub dz: [i32; 4],
}

/// A scenery object pushed against.
#[derive(Clone, Copy, Debug)]
pub struct SceneryBump {
    pub sx: usize,
    pub sy: usize,
    pub level: usize,
    pub id: u8,
    /// Half the overlap, taken off the position and the velocity.
    pub push: IVec2,
}

/// Scenery collision (`AddCollideObj` @ 10055c70, 68k `FUN_000cdfdc`, OS4
/// `FUN_01045288`; people.md §5.2). Every scenery object in a square under
/// the body's footprint, on the object's cube level, is a circle at the
/// square centre plus the model's mean ground point (rotated by the
/// scenery rotation) of radius `size/2`. A body circle inside it moves out
/// by half the overlap, position and velocity both. The caller sets the
/// speed to 0 on any bump (the original does for every class) and applies
/// the vehicle damage. One test per tick (the original sub-steps only when
/// a frame spans several ticks).
#[allow(clippy::too_many_arguments)]
pub fn scenery_collide(
    city: &City,
    library: &ObjectLibrary,
    trig: &Trig,
    pos: &mut IVec3,
    vel: &mut IVec3,
    heading: i32,
    level: usize,
    body: &Body,
) -> Vec<SceneryBump> {
    let (s, c) = (trig.sin(heading), trig.cos(heading));
    let p0 = *pos;
    let along = |d: i32| IVec2::new(p0.x + (d * s >> 8), p0.y + (d * c >> 8));
    let mut circles: Vec<IVec2> = if body.vehicle {
        let r = body.radius;
        [0, r - body.rear, body.front - r, (body.front >> 1) - r]
            .into_iter()
            .map(along)
            .collect()
    } else {
        vec![p0.xy()]
    };
    // Footprint: the rotated body box, in squares.
    let (w, back, front) = if body.vehicle {
        (body.radius, -body.rear, body.front)
    } else {
        (body.radius, -body.radius, body.radius)
    };
    let corners = [(-w, back), (w, back), (-w, front), (w, front)]
        .map(|(w, l)| IVec2::new(p0.x + ((w * c + l * s) >> 8), p0.y + ((l * c - w * s) >> 8)));
    let extra = if body.rc { 1 } else { 0 };
    let sq = |v: i32, d: i32| ((v >> 11) + d).clamp(0, MAP_SIZE as i32 - 1);
    let lo = corners.iter().fold(IVec2::MAX, |m, p| m.min(*p));
    let hi = corners.iter().fold(IVec2::MIN, |m, p| m.max(*p));
    let layer = level.min(2);
    let mut bumps = Vec::new();
    for sx in sq(lo.x, -extra)..=sq(hi.x, extra) {
        for sy in sq(lo.y, -extra)..=sq(hi.y, extra) {
            let cube = &city.map.square(sx as usize, sy as usize).cubes[layer];
            if cube.scenery == 0 {
                continue;
            }
            let Some(model) = SCENERY
                .get(cube.scenery as usize - 1)
                .and_then(|p| library.loaded(p))
            else {
                continue;
            };
            let info = &library.model(model).info;
            let a = (cube.rotation as i32 * 0x1680) >> 8;
            let (rs, rc) = (trig.sin(a), trig.cos(a));
            let centre = IVec2::new(
                sx * 0x800 + 0x400 + ((info.mean.x * rc + info.mean.y * rs) >> 8),
                sy * 0x800 + 0x400 + ((info.mean.y * rc - info.mean.x * rs) >> 8),
            );
            let r = body.radius + (info.size >> 1);
            for i in 0..circles.len() {
                if body.dz[i].abs() >= 0x1f5 {
                    continue;
                }
                let d = centre - circles[i];
                let d2 = d.x as i64 * d.x as i64 + d.y as i64 * d.y as i64;
                if d2 > r as i64 * r as i64 {
                    continue;
                }
                let dist = ((d2 as f64).sqrt() as i32).max(1);
                let push = IVec2::new((d.x * r / dist - d.x) >> 1, (d.y * r / dist - d.y) >> 1);
                vel.x -= push.x;
                vel.y -= push.y;
                pos.x -= push.x;
                pos.y -= push.y;
                for p in &mut circles {
                    *p -= push;
                }
                bumps.push(SceneryBump {
                    sx: sx as usize,
                    sy: sy as usize,
                    level: layer,
                    id: cube.scenery,
                    push,
                });
            }
        }
    }
    bumps
}

/// Arc tangent in angle units, ±0x5a0 (`arctan` @ 10083170, tables from
/// `InitArctan` @ 10083050): below 1 a 1000-entry table of `atan(i/1000)`,
/// above it a table of `atan(1 + i)` by whole units, 0x5a0 past the table.
/// (The table constants are INFERRED: 2880/π scale, 1000 steps.)
pub fn arctan(r: f64) -> i32 {
    let scale = 2880.0 / std::f64::consts::PI;
    let a = r.abs();
    let v = if a < 1.0 {
        (scale * (((a * 1000.0) as i32) as f64 / 1000.0).atan()) as i32
    } else if a <= 999.0 {
        (scale * (1.0 + ((a - 1.0) as i32) as f64).atan()) as i32
    } else {
        0x5a0
    };
    if r < 0.0 { -v } else { v }
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

    #[test]
    fn face_type_5_and_glass() {
        // (5,5): a tall cube with type-5 faces (passable); (7,5): glass.
        let c = city(|x, y| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            if (x, y) == (5, 5) {
                sq.cubes[1] = cube(512, FaceType::Unknown(5));
            }
            if (x, y) == (7, 5) {
                sq.cubes[1] = cube(512, FaceType::Alpha);
            }
            sq
        });
        let y = 5 * 2048 + 1024;
        let prev = IVec3::new(5 * 2048 - 300, y, GROUND_Z);
        let into = IVec3::new(5 * 2048 - 100, y, GROUND_Z);
        assert!(!resolve_walls(&c, into, prev, 192).1.x);
        // Glass stops people and line of sight, and breaks for cars.
        let prev = IVec3::new(7 * 2048 - 300, y, GROUND_Z);
        let into = IVec3::new(7 * 2048 - 100, y, GROUND_Z);
        assert!(resolve_person(&c, into, prev, 192).1.x);
        assert!(resolve_walls(&c, into, prev, 192).1.x);
        assert!(!line_of_sight(&c, prev, IVec3::new(8 * 2048, y, GROUND_Z)));
        let mut glass = Vec::new();
        let (_, hit) = resolve_walls_glass(&c, into, prev, 192, &mut glass);
        assert!(!hit.x);
        assert_eq!(glass, vec![Glass { sx: 7, sy: 5, level: 1 }]);
    }

    #[test]
    fn map_edge_is_a_wall() {
        let c = city(|_, _| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            sq
        });
        let prev = IVec3::new(300, 0x8000, GROUND_Z);
        let (p, hit) = resolve_walls(&c, IVec3::new(200, 0x8000, GROUND_Z), prev, 256);
        assert!(hit.x);
        assert_eq!((hit.side.x, p.x), (1, 300));
        let prev = IVec3::new(WORLD_MAX - 300, 0x8000, GROUND_Z);
        let (_, hit) = resolve_walls(&c, IVec3::new(WORLD_MAX - 200, 0x8000, GROUND_Z), prev, 256);
        assert_eq!(hit.side.x, -1);
        // Moving away from the edge is free.
        let (_, hit) = resolve_walls(&c, IVec3::new(250, 0x8000, GROUND_Z), IVec3::new(200, 0x8000, GROUND_Z), 256);
        assert!(!hit.x);
    }

    #[test]
    fn slope_level_is_an_or() {
        // A Horiz ramp from 0 to 256 h-units on cube 1: a depth below its
        // high end is inside it although above its base height h1.
        let c = city(|_, _| {
            let mut sq = Square::default();
            sq.cubes[0] = cube(256, FaceType::Normal);
            sq.cubes[1] = Cube {
                slope: Slope::Horiz,
                height1: 0,
                height2: 256,
                ..cube(0, FaceType::Normal)
            };
            sq
        });
        assert_eq!(cube_num(&c, 3, 3, 1500 - 256 - 100), Some(1));
        assert_eq!(cube_num(&c, 3, 3, 1500 - 256 - 300), None);
    }

    #[test]
    fn arctan_table() {
        assert_eq!(arctan(0.0), 0);
        assert_eq!(arctan(1.0), 0x2d0);
        assert_eq!(arctan(-1.0), -0x2d0);
        assert_eq!(arctan(5000.0), 0x5a0);
        assert!((arctan(0.5) - 425).abs() <= 1);
    }
}
