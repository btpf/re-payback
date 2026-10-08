//! Vehicle driving physics (68k `FUN_000e3a58` vehicle branch, OS4
//! `FUN_010cc288` grip, `FUN_010cd198` wall hits; Mac `do_Other` @
//! 10146280, `PositionUnit` @ 10081450, `SkidUnit` @ 10081830,
//! `BounceUnit` @ 10082bc0, `CheckForCubeCollision` @ 100af980;
//! docs/re/vehicles.md and the damage/special-vehicle notes).
//!
//! Per tick, from the virtual stick: throttle/brake ramp the drive force,
//! steering ramps the yaw rate, the heading eases toward the nearest 45°,
//! and the velocity converges on `speed·(sin, cos)` by a surface-dependent
//! grip; the remaining slip drives skid sounds and marks.

use super::collide;
use super::people::PlayerInputs;
use super::{Obj, State, Stick, Trig, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::mapedit::MapEdit;
use crate::mission::{Triggers, kind};
use crate::objects::{ObjectLibrary, SetScenery};
use crate::vehicles::{TYPES, Vehicle, VehicleType, kind as vk};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::prelude::*;
use payback_formats::map::{self, MAP_SIZE};

/// Driving state of a vehicle beyond the core object.
#[derive(Component, Default, Clone, Debug)]
pub struct Drive {
    /// Current drive force, ramping by 5 per tick (`+0x32a`).
    pub force: i32,
    /// Braking against the direction of travel stops at zero (`+0x44c`).
    pub moving: bool,
    /// Handbrake turn this tick (`+0x3c1`).
    pub j_turn: bool,
    /// Body roll for rendering (`+0x306`), -256..256.
    pub lean: i32,
    /// Body pitch for rendering (`+0x30a`), -128..128.
    pub dip: i32,
    /// Slip 0..255 (skid intensity).
    pub slip: i32,
    /// Door opening angle 0..0x370 (`+0x418`).
    pub door: i32,
    /// Dented by a hard hit (>= 11 damage); never repaired.
    pub dented: bool,
    /// Ground heights under the 3×3 box points (`+0x8b8..+0x8d8`): rows
    /// left (−half-width), centre line, right; each rear, centre, front.
    /// Set by the collision step when nothing was hit.
    pub heights: [i32; 9],
    pub skid_sound: Option<Entity>,
    pub beep_sound: Option<Entity>,
    pub clonk: bool,
}

/// The vehicle update chain.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivingSet;

pub struct DrivingPlugin;

impl Plugin for DrivingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (player_vehicle_stick, drive, vehicle_sounds)
                .chain()
                .in_set(DrivingSet)
                .after(super::people::PeopleSet)
                .before(super::combat::CombatSet),
        )
        .add_systems(Update, sync_vehicles);
    }
}

/// The player's controls drive the vehicle they're in.
fn player_vehicle_stick(
    inputs: Res<PlayerInputs>,
    mut q: Query<(&Obj, &Vehicle, &mut Stick), With<CameraTarget>>,
) {
    for (obj, veh, mut stick) in &mut q {
        if obj.state != State::Player {
            continue;
        }
        let input = &inputs.0[obj.player.unwrap_or(0).min(3) as usize];
        if !veh.controls {
            *stick = Stick::default();
            continue;
        }
        stick.steer = if input.left {
            -1
        } else if input.right {
            1
        } else {
            0
        };
        stick.throttle = if input.accel {
            -1
        } else if input.reverse {
            1
        } else {
            0
        };
        stick.brake = input.brake;
        stick.fire = input.fire;
    }
}

/// Grip divisor/limit by surface flag of the top texture under the car
/// (`.CubeTextureList` first column; rendering.md §8).
fn grip_for(surface: u32, base: i32, speed: &mut i32) -> i32 {
    match surface {
        0x22 => {
            // Grass: half grip; bleeds speed above 450.
            if *speed > 450 {
                *speed -= (*speed - 450).min(3);
            }
            base / 2
        }
        0x24 => {
            // Sand: double grip; bleeds speed above 350.
            if *speed > 350 {
                *speed -= (*speed - 350).min(20);
            }
            base * 2
        }
        0x32 => {
            // Snow: fixed grip; bleeds toward 200.
            if *speed > 200 {
                *speed -= (*speed - 200).min(10);
            }
            4
        }
        0x33 => base * 10 / 25,
        0x34 => 1,
        _ => base * 4 / 5,
    }
}

fn surface_flag(city: &City, data: &GameData, obj: &Obj) -> u32 {
    let (sx, sy) = ((obj.pos.x >> 11) as usize, (obj.pos.y >> 11) as usize);
    if sx >= MAP_SIZE || sy >= MAP_SIZE {
        return 4;
    }
    let top = city.map.square(sx, sy).cubes[obj.level].faces[map::TOP];
    if !top.visible() {
        return 4;
    }
    data.texture_flags(top.texture).unwrap_or(4)
}

/// Effective top speed: damaged vehicles slow down (`eff = clamp(health ·
/// max / 50, max / 4, max)`).
fn effective_max(t: &VehicleType, health: i32) -> i32 {
    (health * t.max_speed / 50).clamp(t.max_speed / 4, t.max_speed)
}

fn effective_reverse(t: &VehicleType, health: i32) -> i32 {
    let r = -t.max_reverse;
    (health * r / 50).clamp(r / 4, r)
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    data: Res<GameData>,
    city: Option<Res<City>>,
    trig: Res<Trig>,
    library: Option<Res<ObjectLibrary>>,
    mut triggers: ResMut<Triggers>,
    mut edits: MessageWriter<MapEdit>,
    mut set_scenery: MessageWriter<SetScenery>,
    mut q: Query<(
        Entity,
        &mut Obj,
        &mut Vehicle,
        &mut Drive,
        &Stick,
        Has<CameraTarget>,
    )>,
) {
    let (Some(city), Some(library)) = (city, library) else { return };
    let mut glass = Vec::new();
    for (e, mut obj, veh, mut d, stick, is_player) in &mut q {
        let t = &TYPES[veh.kind];
        let info = &library.model(obj.model).info;
        let (hw, front, rear) = (info.radius.max(1), info.max_y, info.min_y);
        let tracked = matches!(veh.kind, vk::TANK | vk::HELICOPTER | vk::BOAT);
        let (mut steer, mut throttle) = (stick.steer, stick.throttle);
        if matches!(obj.state, State::Parked | State::Dead | State::Removed) || obj.burn > 0 {
            steer = 0;
            throttle = 0;
        }
        if veh.kind == vk::HELICOPTER && !veh.flying {
            steer = 0;
            throttle = 0;
        }
        // Boats only drive on water; other vehicles stall in it.
        if (veh.kind == vk::BOAT) != obj.in_water {
            steer = 0;
            throttle = 0;
            if obj.in_water {
                obj.speed = 0;
            }
        }
        if throttle == 0 {
            d.force = 0;
        }
        d.j_turn = false;
        let j_turn_ok = !tracked && steer != 0;

        // Throttle forward.
        if throttle < 0 {
            if obj.speed < 0 {
                // Braking out of reverse; steering spins it (J-turn).
                if j_turn_ok {
                    if is_player {
                        triggers.report(kind::HANDBRAKE, 0, 0, 0);
                    }
                    d.j_turn = true;
                    obj.turn_rate = if steer < 0 { -2 * t.turn } else { 2 * t.turn };
                }
                d.force = (d.force.max(0) + 5).min(t.brake);
                obj.speed += d.force.min(t.max_speed - obj.speed);
                if obj.speed >= 1 && d.moving {
                    obj.speed = 0;
                    d.moving = false;
                } else {
                    d.moving = obj.speed != 0;
                }
            } else {
                d.force = (d.force.max(0) + 5).min(t.accel);
                obj.speed += d.force.min(t.max_speed - obj.speed);
                d.moving = true;
            }
        } else if obj.speed > 0 {
            obj.speed -= obj.speed.min(1);
        }
        if throttle < 1 {
            if obj.speed < 0 {
                obj.speed += (-obj.speed).min(1);
            }
            if obj.speed >= 0 {
                obj.speed = obj.speed.min(effective_max(t, obj.health));
            } else {
                obj.speed = obj.speed.max(-effective_reverse(t, obj.health));
            }
        } else {
            if obj.speed < 1 {
                d.force = (d.force.min(0) - 5).max(-t.accel);
                obj.speed += d.force.max(t.max_reverse - obj.speed);
                d.moving = true;
            } else {
                // Braking forward motion; steering spins it.
                if j_turn_ok {
                    if is_player {
                        triggers.report(kind::HANDBRAKE, 0, 0, 0);
                    }
                    d.j_turn = true;
                    obj.turn_rate = if steer < 0 { 2 * t.turn } else { -2 * t.turn };
                }
                d.force = (d.force.min(0) - 5).max(-t.brake);
                obj.speed += d.force.max(t.max_reverse - obj.speed);
                if obj.speed < 0 && d.moving {
                    obj.speed = 0;
                    d.moving = false;
                } else {
                    d.moving = obj.speed != 0;
                }
            }
            if obj.speed < 0 {
                obj.speed = obj.speed.max(-effective_reverse(t, obj.health));
            } else {
                obj.speed = obj.speed.min(effective_max(t, obj.health));
            }
        }

        // Steering: the yaw rate ramps by 5 toward min(|speed|, turn).
        if !d.j_turn {
            let cap = if tracked {
                t.turn
            } else {
                obj.speed.abs().min(t.turn)
            };
            let left = steer < 0;
            if steer != 0 {
                // Turning left raises the heading going forward; reversing
                // flips it.
                let up = left != (obj.speed < 0);
                if up {
                    if obj.turn_rate < 0 {
                        obj.turn_rate = 0;
                    }
                    obj.turn_rate = (obj.turn_rate + 5).min(cap);
                } else {
                    if obj.turn_rate > 0 {
                        obj.turn_rate = 0;
                    }
                    obj.turn_rate = (obj.turn_rate - 5).max(-cap);
                }
            } else {
                obj.turn_rate = 0;
            }
            // Body lean toward speed / max (visual).
            let want = (obj.speed.abs() << 8) / t.max_speed.max(1) * steer.signum();
            d.lean += (want - d.lean).clamp(-0x40, 0x40);
        }
        obj.heading = wrap(obj.heading + obj.turn_rate);
        // Ease toward the nearest 45° while moving (driving assist).
        if obj.speed != 0 {
            let snap = wrap((obj.heading + 0x168) / 0x2d0 * 0x2d0);
            let diff = super::angle_diff(obj.heading, snap);
            obj.heading = wrap(obj.heading + diff.clamp(-4, 4));
        }

        // Body roll and pitch from the ground, downhill speed (on the
        // ground); nose tipping in the air; sinking.
        let sinking = obj.in_water && veh.kind != vk::BOAT;
        if sinking {
            if angle_signed(obj.pitch) < 0x5a0 {
                obj.pitch = wrap(obj.pitch + 4);
            }
        } else if obj.on_ground {
            position_unit(&mut obj, &d, hw, front, rear);
        } else if veh.kind == vk::HELICOPTER {
            // `PositionAirborneUnit` @ 10082620, flying branch: the body
            // leans by the speed, level sideways.
            obj.pitch = wrap(obj.speed);
            obj.roll = 0;
        } else {
            // `PositionAirborneUnit`: the nose tips 16 per tick, down
            // going forward, up otherwise, stopping at ±0x5a0.
            if obj.speed < 1 {
                if obj.pitch > 0x10df || obj.pitch < 0xb40 {
                    obj.pitch = wrap(obj.pitch - 16);
                }
            } else if obj.pitch < 0x5a0 || obj.pitch > 0xb40 {
                obj.pitch = wrap(obj.pitch + 16);
            }
        }

        // Grip: velocity converges on the heading.
        let flag = surface_flag(&city, &data, &obj);
        let base = if stick.brake && is_player { 4 } else { t.grip };
        let mut speed = obj.speed;
        let g = grip_for(flag, base, &mut speed).max(1);
        obj.speed = speed;
        let tx = obj.speed * trig.sin(obj.heading) >> 8;
        let ty = obj.speed * trig.cos(obj.heading) >> 8;
        obj.vel.x += (tx - obj.vel.x).clamp(-g, g);
        obj.vel.y += (ty - obj.vel.y).clamp(-g, g);
        let slip = (((obj.vel.x - tx).abs() + (obj.vel.y - ty).abs()) * 2).clamp(0, 255);
        d.slip = if tracked || veh.kind == vk::HELICOPTER {
            0
        } else if obj.in_water {
            slip * 2
        } else {
            slip
        };

        // Move with wall collisions (bounce and damage).
        let prev = obj.pos;
        let mut next = obj.pos + IVec3::new(obj.vel.x, obj.vel.y, 0);
        let r = 256;
        let (resolved, hit) = collide::resolve_walls_glass(&city, next, prev, r, &mut glass);
        // A hit that opposes the velocity turns the heading by 0x5a, away
        // from the wall (tail of `CheckForCubeCollision` @ 100af980, 68k
        // `FUN_00044f82`; all builds).
        obj.heading = deflect(obj.heading, hit.side, obj.vel.xy());
        // `BounceUnit` @ 10082bc0 per blocked axis: damage v²>>14, Thud or
        // Thud2 (dented, the bus screams for its player), half the normal
        // velocity kept, pointing away from the wall.
        for (axis_hit, v) in [(hit.x, obj.vel.x), (hit.y, obj.vel.y)] {
            if !axis_hit {
                continue;
            }
            let dmg = (v * v) >> 14;
            obj.health -= dmg;
            let at = SoundPos::At(obj.posf());
            let vol = obj.speed.abs() as f32 * 10000.0;
            if dmg < 11 {
                commands.spawn(Sound::once(id::THUD, vol, 12000.0, at));
            } else {
                commands.spawn(Sound::once(id::THUD2, vol, 8000.0, at));
                d.dented = true;
                if veh.kind == vk::BUS && is_player {
                    commands.spawn(Sound::once(id::SCREAMS, 255.0, 15000.0, SoundPos::Global));
                }
            }
        }
        // (The port also halves the speed on a hit; `BounceUnit` leaves it.)
        if hit.x {
            obj.vel.x = bounce(hit.side.x, obj.vel.x);
            obj.speed /= 2;
        }
        if hit.y {
            obj.vel.y = bounce(hit.side.y, obj.vel.y);
            obj.speed /= 2;
        }
        next = resolved;
        // The ground under the 3×3 box points, kept from the last step
        // without a hit (`CheckForCubeCollision`; `PositionUnit` reads it).
        if !hit.x && !hit.y {
            d.heights = box_heights(&city, &trig, &obj, next, hw, front, rear);
        }

        // Follow the ground; fall when it drops away (vehicles.md §2.6).
        // On the ground vz follows the terrain (`ground + 0x80 - z`), so a
        // car leaving a ramp keeps its climb and jumps; in the air only
        // gravity acts.
        if let Some((fz, level, water)) = collide::floor(&city, next.x, next.y, prev.z) {
            obj.level = level;
            obj.floor_z = fz;
            if next.z >= fz {
                if !obj.on_ground {
                    // Landing.
                    if obj.vel.z > 160 {
                        let at = SoundPos::At(obj.posf());
                        commands.spawn(Sound::once(
                            id::HIT_SCENERY,
                            obj.vel.z as f32 * 20000.0,
                            15000.0,
                            at,
                        ));
                        obj.vel.z = 0x80 - obj.vel.z.abs() / 4;
                    } else {
                        obj.vel.z = 0x80;
                    }
                    obj.on_ground = true;
                } else {
                    obj.vel.z = (fz + 0x80 - next.z).clamp(-1000, 1000);
                }
                next.z = fz;
            } else {
                obj.on_ground = false;
                obj.vel.z += 32;
                next.z = (next.z + obj.vel.z).min(fz);
            }
            // Water counts once the car is down on it (§2.8), not while
            // it flies over.
            let was_water = obj.in_water;
            obj.in_water = water && next.z >= fz;
            if obj.in_water && !was_water && veh.kind != vk::BOAT {
                let at = SoundPos::At(obj.posf());
                commands.spawn(Sound::once(id::SPLASH, t.mass as f32 * 10000.0, 8350.0, at));
                triggers.report_actor(kind::ACTOR_IN_WATER, e);
                if veh.kind == vk::RC_CAR {
                    obj.health = -71;
                }
            }
            // Non-boats sink in water.
            if obj.in_water && veh.kind != vk::BOAT {
                next.z += 0x20;
            }
        }

        // Scenery (`AddCollideObj` @ 10055c70): pushed out by half the
        // overlap and stopped; the push damages the car,
        // `(px² + py²) >> 12`, with Thud below 11, else Thud2 (dented; a
        // bus screams). A snowman is knocked down instead.
        let h = d.heights;
        let body = collide::Body {
            radius: hw,
            front,
            rear,
            vehicle: true,
            rc: veh.kind == vk::RC_CAR,
            dz: [h[4] - next.z, h[3] - next.z, h[5] - next.z, h[5] - next.z],
        };
        let mut vel = obj.vel;
        let bumps = collide::scenery_collide(
            &city, &library, &trig, &mut next, &mut vel, obj.heading, obj.level, &body,
        );
        obj.vel = vel;
        if !bumps.is_empty() {
            obj.speed = 0;
        }
        for b in bumps {
            if b.id == SNOWMAN {
                set_scenery.write(SetScenery {
                    sx: b.sx,
                    sy: b.sy,
                    level: b.level,
                    scenery: 0,
                    special: None,
                    rotation: None,
                });
                continue;
            }
            let dmg = (b.push.x * b.push.x + b.push.y * b.push.y) >> 12;
            let at = SoundPos::At(obj.posf());
            if dmg < 11 {
                commands.spawn(Sound::once(id::THUD, dmg as f32 * 100_000.0, 8000.0, at));
            } else {
                d.dented = true;
                commands.spawn(Sound::once(id::THUD2, dmg as f32 * 100_000.0, 8000.0, at));
                if veh.kind == vk::BUS {
                    commands.spawn(Sound::once(id::SCREAMS, 255.0, 15000.0, SoundPos::Global));
                }
            }
            obj.health -= dmg;
        }

        obj.prev_pos = obj.pos;
        obj.pos = next;
    }
    collide::smash_glass(&mut commands, &city, &mut edits, &mut triggers, &glass);
}

/// Scenery id of the snowman (`scenery/snowman.obj`, pickup code 0xfe).
const SNOWMAN: u8 = 26;

/// Signed angle in −0xb40..0xb40.
fn angle_signed(a: i32) -> i32 {
    super::angle_diff(0, a)
}

/// N7: a wall hit that opposes the velocity turns the heading by 0x5a
/// (tail of `CheckForCubeCollision` @ 100af980, 68k `FUN_00044f82`). For
/// x the turn is −0x5a with the heading in [1, 0x5a0] or [0xb41, 0x10e0]
/// and +0x5a otherwise; for y the signs swap.
fn deflect(heading: i32, side: IVec2, vel: IVec2) -> i32 {
    let opposes = |s: i32, v: i32| (s > 0 && v < 1) || (s < 0 && v >= 0);
    let in_quarters = |h: i32| (1..=0x5a0).contains(&h) || (0xb41..=0x10e0).contains(&h);
    let mut h = heading;
    if opposes(side.x, vel.x) {
        h = wrap(if in_quarters(h) { h - 0x5a } else { h + 0x5a });
    }
    if opposes(side.y, vel.y) {
        h = wrap(if in_quarters(h) { h + 0x5a } else { h - 0x5a });
    }
    h
}

/// `BounceUnit` @ 10082bc0: half the normal velocity, pointing away from
/// the wall (`side` +1: the wall is on the − side).
fn bounce(side: i32, v: i32) -> i32 {
    let half = (v >> 1).abs();
    if side < 1 { -half } else { half }
}

/// Ground heights under the 3×3 box points at `pos` (the vehicle branch of
/// `CheckForCubeCollision` @ 100af980): across the half-width (−, 0, +)
/// and along the body (−rear, 0, front), rotated by the roll, pitch and
/// heading. The centre uses the floor rule (`GetCubeInfo`), the others the
/// nearest surface (`GetCubeInfo2`).
fn box_heights(
    city: &City,
    trig: &Trig,
    obj: &Obj,
    pos: IVec3,
    hw: i32,
    front: i32,
    rear: i32,
) -> [i32; 9] {
    let (sr, cr) = (trig.sin(obj.roll), trig.cos(obj.roll));
    let (sp, cp) = (trig.sin(obj.pitch), trig.cos(obj.pitch));
    let (sh, ch) = (trig.sin(obj.heading), trig.cos(obj.heading));
    let mut out = [pos.z; 9];
    for (i, w) in [-hw, 0, hw].into_iter().enumerate() {
        let lat = cr * w >> 8;
        let up = -(sr * w) >> 8;
        for (j, l) in [-rear, 0, front].into_iter().enumerate() {
            out[i * 3 + j] = if (i, j) == (1, 1) {
                collide::floor(city, pos.x, pos.y, pos.z).map_or(pos.z, |f| f.0)
            } else {
                let fwd = (cp * l - up * sp) >> 8;
                let x = pos.x + ((lat * ch + fwd * sh) >> 8);
                let y = pos.y + ((fwd * ch - lat * sh) >> 8);
                collide::ground_at(city, x, y, pos.z).0
            };
        }
    }
    out
}

/// Average two angles the short way round (`PositionUnit`'s filter).
fn ease_angle(old: i32, target: i32) -> i32 {
    let (mut a, mut b) = (target, old);
    if (a - b).abs() > 0xb40 {
        if a < b {
            a += 0x1680;
        } else {
            b += 0x1680;
        }
    }
    wrap((a + b) >> 1)
}

/// `PositionUnit` @ 10081450 (68k `FUN_000a1758`, OS4 `FUN_010cbc7c`):
/// body roll and pitch from the box heights, each eased halfway toward
/// its target, and the downhill speed `speed += pitch·ticks >> 8` (the
/// unfiltered slope; ticks = 1). Across, the lowest point of each row is
/// used; along, the centre line. When the middle is a crest (both halves
/// slope away) the gentler half wins. The port draws the steering lean
/// separately, so it is not added to the roll here (the original adds
/// `+0x7e8`).
fn position_unit(obj: &mut Obj, d: &Drive, hw: i32, front: i32, rear: i32) {
    let h = &d.heights;
    let min3 = |k: usize| h[k].min(h[k + 1]).min(h[k + 2]);
    let (l, c, r) = (min3(0), min3(3), min3(6));
    let (lc, cr) = (l - c, c - r);
    let ratio = if lc < 0 || cr > 0 {
        ((l - r) * 0x100) / (hw * 2)
    } else if cr.abs() < lc.abs() {
        (cr * 0x100) / hw
    } else {
        (lc * 0x100) / hw
    };
    // The 1/256 scale back is INFERRED (a constant the decompile hides).
    let roll = collide::arctan(ratio as f64 / 256.0);
    obj.roll = ease_angle(obj.roll, wrap(roll));

    let (back, mid, nose) = (h[3], h[4], h[5]);
    let (bm, mn) = (back - mid, mid - nose);
    let pitch = -if bm < 0 || mn > 0 {
        collide::arctan((back - nose) as f64 / (front + rear).max(1) as f64)
    } else if mn.abs() < bm.abs() {
        collide::arctan(mn as f64 / front.max(1) as f64)
    } else {
        collide::arctan(bm as f64 / rear.max(1) as f64)
    };
    obj.speed += pitch >> 8;
    obj.pitch = ease_angle(obj.pitch, wrap(pitch + d.dip));
}

/// Engine pitch, clonk swap, skid and reversing beeper (OS4 `FUN_0105f594`
/// engine block, `FUN_010cc288` skid).
fn vehicle_sounds(
    mut commands: Commands,
    mut q: Query<(&Obj, &Vehicle, &mut Drive)>,
    mut sounds: Query<&mut Sound>,
) {
    for (obj, veh, mut d) in &mut q {
        let t = &TYPES[veh.kind];
        let eff = effective_max(t, obj.health).max(1);
        let at = SoundPos::At(obj.posf() - Vec3::Z * 100.0);
        if let Some(mut s) = veh.engine.and_then(|e| sounds.get_mut(e).ok()) {
            s.pos = at;
            s.hz = match veh.kind {
                vk::TANK => (obj.speed.abs() * 10000 / eff + 4000) as f32,
                vk::HELICOPTER if !veh.flying => 4000.0,
                _ => (obj.speed.abs() * 20000 / eff + 6000) as f32,
            };
            if obj.health <= 49 && !d.clonk {
                d.clonk = true;
                s.id = match s.id {
                    id::ENGINE => id::ENGINE_CLONK,
                    id::ENGINE_904..=id::ENGINE_BUS => s.id + 4,
                    other => other,
                };
            }
        }
        // Skid loop.
        let skid_at = SoundPos::At(obj.posf() - Vec3::Z * 250.0);
        let skid = *d.skid_sound.get_or_insert_with(|| {
            commands
                .spawn(Sound::looping(id::SKID, 0.0, 10000.0, skid_at))
                .id()
        });
        if let Ok(mut s) = sounds.get_mut(skid) {
            s.volume = d.slip as f32 * 40000.0;
            s.hz = (d.slip * 10 + 10000) as f32;
            s.pos = skid_at;
        }
        // Reversing beeper.
        match (t.reverse_beep, obj.speed < 0, d.beep_sound) {
            (Some(beep), true, None) => {
                d.beep_sound = Some(
                    commands
                        .spawn(Sound::looping(beep, 10_000_000.0, 22050.0, at))
                        .id(),
                );
            }
            (_, false, Some(e)) => {
                commands.entity(e).despawn();
                d.beep_sound = None;
            }
            _ => {}
        }
    }
}

/// Vehicle transforms: heading, then the body pitch (+ = nose down) and
/// roll (+ = the −x side down) from `PositionUnit`, plus a slight lean
/// into turns.
fn sync_vehicles(mut q: Query<(&Obj, &Drive, &mut Transform), With<Vehicle>>) {
    let rad = |a: i32| angle_signed(a) as f32 / super::TURN as f32 * std::f32::consts::TAU;
    for (obj, d, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        let roll = rad(obj.roll) + d.lean as f32 / 256.0 * 0.08;
        tf.rotation = heading_to_render(obj.heading as f32)
            * Quat::from_rotation_x(rad(obj.pitch))
            * Quat::from_rotation_z(roll);
    }
}
