//! Props: the roadblock barrier, the cone, the training dummy and the
//! script bullet (special objects `model − 1000`, [`crate::vehicles::Prop`]).
//! Class 2 objects without a weapon routine of their own: each tick they
//! move with `MoveObject_General` @ 1006e4f0 (68k `FUN_000e0e46`) and then
//! take the prop branches of `do_Weapon` @ 1013e390. Contacts (pushes,
//! knocking a barrier or cone over) are in `contact.rs`.

use super::{Obj, State, collide};
use crate::audio::{Sound, SoundPos, id};
use crate::objects::ObjectLibrary;
use crate::vehicles::{Prop, prop};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::prelude::*;

/// `v` moved towards 0 by at most `step`.
fn towards_zero(v: i32, step: i32) -> i32 {
    v + (-v).clamp(-step, step)
}

/// One tick of every prop, in object-table order (Mac `object_MainLoop`
/// @ 1014a670: class 2 → `MoveObject_General`, then `do_Weapon`):
///
/// - gravity: on (or below) its floor `+0x8c8` the object is put on it
///   with vz = 0x20, then vz += 0x20 and z += vz;
/// - x, y += v; walls as for people (`CheckForCubeCollision` @ 100af980,
///   class 1/2 branch, radius `+0x8120`), the floor read at the new place;
/// - barrier / cone (`_DAT_1017c5f8[0]` / `[1]`): a wall hit plays Thud
///   (5, 2e6 @ 12000; the cone HitScenery 30, 2e6 @ 8500) and turns that
///   velocity away from the wall. With no wall hit and on the floor: the
///   same sound if vz > 500, z = floor, vz = −|vz >> 3|, and vx, vy go
///   32 per tick towards 0. Otherwise (in the air or against a wall) they
///   go 1 per tick towards 0;
/// - dummy (`[2]`): no horizontal motion at all (vx = vy = 0); landing
///   (no wall hit): HitScenery if vz > 500, z = floor, vz = 0;
/// - bullet (`[3]`): vx = vy = vz = 0, z = floor.
///
/// No random numbers, spawns or despawns. The original's class-2 wall test
/// smashes glass; here the people's test is used, which glass blocks.
pub(super) fn move_props(
    mut commands: Commands,
    city: Option<Res<City>>,
    library: Option<Res<ObjectLibrary>>,
    mut q: Query<(Entity, &mut Obj, &Prop)>,
) {
    let (Some(city), Some(library)) = (city, library) else { return };
    for e in super::table_order(q.iter().map(|(e, o, _)| (e, o))) {
        let Ok((_, mut obj, &Prop(kind))) = q.get_mut(e) else { continue };
        if obj.state == State::Removed {
            continue;
        }
        // `MoveObject_General` @ 1006e4f0 (one tick, `+0x81a` clear).
        let start = obj.pos;
        if obj.pos.z < obj.floor_z {
            obj.on_ground = false;
        } else {
            obj.vel.z = 0x20;
            obj.pos.z = obj.floor_z;
            obj.on_ground = true;
        }
        obj.vel.z += 0x20;
        obj.pos.z += obj.vel.z;
        let mut next = obj.pos;
        next.x += obj.vel.x;
        next.y += obj.vel.y;
        let r = library.model(obj.model).info.radius;
        let (resolved, hit) = collide::resolve_person(&city, next, start, r);
        next = resolved;
        if let Some((fz, level, water)) = collide::floor(&city, next.x, next.y, next.z) {
            obj.floor_z = fz;
            obj.level = level;
            obj.in_water = water;
        }
        obj.prev_pos = start;
        obj.pos = next;
        let wall = hit.x || hit.y;
        let landed = !wall && obj.floor_z <= obj.pos.z;
        let at = SoundPos::At(obj.posf());
        // `do_Weapon` @ 1013e390, the prop branches.
        match kind {
            prop::BARRIER | prop::CONE => {
                let (snd, hz) = if kind == prop::BARRIER { (id::THUD, 12000.0) } else { (id::HIT_SCENERY, 8500.0) };
                if hit.x {
                    commands.spawn(Sound::once(snd, 2e6, hz, at));
                    obj.vel.x = if hit.side.x < 1 { -obj.vel.x.abs() } else { obj.vel.x.abs() };
                }
                if hit.y {
                    commands.spawn(Sound::once(snd, 2e6, hz, at));
                    obj.vel.y = if hit.side.y < 1 { -obj.vel.y.abs() } else { obj.vel.y.abs() };
                }
                if landed {
                    if obj.vel.z > 500 {
                        commands.spawn(Sound::once(snd, 2e6, hz, at));
                    }
                    obj.pos.z = obj.floor_z;
                    obj.vel.z = -(obj.vel.z >> 3).abs();
                    obj.vel.x = towards_zero(obj.vel.x, 32);
                    obj.vel.y = towards_zero(obj.vel.y, 32);
                } else {
                    obj.vel.x = towards_zero(obj.vel.x, 1);
                    obj.vel.y = towards_zero(obj.vel.y, 1);
                }
            }
            prop::DUMMY => {
                obj.vel.x = 0;
                obj.vel.y = 0;
                if landed {
                    if obj.vel.z > 500 {
                        commands.spawn(Sound::once(id::HIT_SCENERY, 2e6, 8500.0, at));
                    }
                    obj.pos.z = obj.floor_z;
                    obj.vel.z = 0;
                }
            }
            _ => {
                obj.vel = IVec3::ZERO;
                obj.pos.z = obj.floor_z;
            }
        }
    }
}

/// Prop transforms: position, heading, then pitch and roll (a knocked-over
/// barrier or cone lies on its side, roll 0x5a0).
pub(super) fn sync_props(mut q: Query<(&Obj, &mut Transform), With<Prop>>) {
    let rad = |a: i32| {
        let a = a.rem_euclid(super::TURN);
        let a = if a > super::TURN / 2 { a - super::TURN } else { a };
        a as f32 / super::TURN as f32 * std::f32::consts::TAU
    };
    for (obj, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        tf.rotation = heading_to_render(obj.heading as f32)
            * Quat::from_rotation_x(rad(obj.pitch))
            * Quat::from_rotation_z(rad(obj.roll));
    }
}
