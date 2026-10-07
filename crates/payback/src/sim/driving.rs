//! Vehicle driving physics (68k `FUN_000e3a58` vehicle branch, OS4
//! `FUN_010cc288` grip, `FUN_010cd198` wall hits; docs/re/vehicles.md and
//! the damage/special-vehicle notes).
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
use crate::mission::{Triggers, kind};
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
    data.cube_textures
        .entries
        .get(top.texture as usize)
        .map_or(4, |e| e.flags)
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
    mut triggers: ResMut<Triggers>,
    mut q: Query<(
        Entity,
        &mut Obj,
        &mut Vehicle,
        &mut Drive,
        &Stick,
        Has<CameraTarget>,
    )>,
) {
    let Some(city) = city else { return };
    for (e, mut obj, veh, mut d, stick, is_player) in &mut q {
        let t = &TYPES[veh.kind];
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
        let (resolved, hit) = collide::resolve_walls(&city, next, prev, r);
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
        if hit.x {
            obj.vel.x = -obj.vel.x / 2;
            obj.speed /= 2;
        }
        if hit.y {
            obj.vel.y = -obj.vel.y / 2;
            obj.speed /= 2;
        }
        next = resolved;

        // Follow the ground; fall when it drops away.
        if let Some((fz, level, water)) = collide::floor(&city, next.x, next.y, prev.z) {
            obj.level = level;
            obj.floor_z = fz;
            let was_water = obj.in_water;
            obj.in_water = water;
            if water && !was_water && veh.kind != vk::BOAT {
                let at = SoundPos::At(obj.posf());
                commands.spawn(Sound::once(id::SPLASH, t.mass as f32 * 10000.0, 8350.0, at));
                triggers.report_actor(kind::ACTOR_IN_WATER, e);
                if veh.kind == vk::RC_CAR {
                    obj.health = -71;
                }
            }
            if next.z < fz {
                obj.vel.z += 32;
                next.z = (next.z + obj.vel.z).min(fz);
            } else {
                if obj.vel.z > 160 {
                    let at = SoundPos::At(obj.posf());
                    commands.spawn(Sound::once(
                        id::HIT_SCENERY,
                        obj.vel.z as f32 * 20000.0,
                        15000.0,
                        at,
                    ));
                }
                obj.vel.z = 0;
                next.z = fz;
            }
            // Non-boats sink in water.
            if obj.in_water && veh.kind != vk::BOAT {
                next.z += 0x20;
            }
        }
        obj.prev_pos = obj.pos;
        obj.pos = next;
    }
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

/// Vehicle transforms, with a slight body roll into turns.
fn sync_vehicles(mut q: Query<(&Obj, &Drive, &mut Transform), With<Vehicle>>) {
    for (obj, d, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        let roll = d.lean as f32 / 256.0 * 0.08;
        tf.rotation = heading_to_render(obj.heading as f32) * Quat::from_rotation_z(roll);
    }
}
