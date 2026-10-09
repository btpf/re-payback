//! The bomb truck's remote-controlled car (type 21's linked model
//! `+0x815c` = type 22, the RC car, `+0x8160` set). Mac `DoVehicleFire` @
//! 10070c20 (launch), the RC branch at the top of `do_Other` @ 10146280
//! (steering it, detonating it), `DoCollideStuff` @ 10067b50 (its end),
//! `GetInOutVehicles` @ 10066140 (getting out blows it up) and
//! `UpdateMapPosition` @ 10068cb0 (the camera follows it). Level 11's M9
//! and its "Kill frenzy" bonus are built around it.
//!
//! - Fire pressed (not held last tick, `_DAT_1017c32c`) in a bomb truck
//!   with no RC car out (`_DAT_1017c484[player] == −1`) launches one at the
//!   truck's position and heading with its velocity, speed, level and
//!   ground; its engine loops (10e6 @ 20000), it belongs to the player
//!   (`+0x824 = 1`, `+0x998` = the player).
//! - While it is out the player's keys drive the RC car (accelerate,
//!   reverse, brake against the motion, steer; accelerating fires the
//!   trigger 0x42 `PlAccelerate` @ 100f71a0) and the truck gets none, not
//!   even the fire key. A fire press sets the RC car's health to −71: it
//!   blows apart (`Detonate`, the blast paid to the player).
//! - Getting out of the truck, or the player dying, blows the RC car up
//!   too. When it blows up the slot clears and the player's camera holds
//!   still for 60 ticks (`_DAT_1017c364`), then goes back to the truck.
//! - It takes no part in object contacts (`contact.rs`), dies in water
//!   (`driving.rs`) and has a larger scenery footprint (`collide.rs`).

use super::people::PlayerInputs;
use super::{Obj, State, Stick};
use crate::camera::CameraTarget;
use crate::mission::{Triggers, kind};
use crate::vehicles::{Vehicle, kind as vk};
use bevy::prelude::*;

/// Ticks the camera holds still after an RC car blew up (`_DAT_1017c364`).
pub const CAMERA_HOLD: i32 = 60;

/// Health that blows a vehicle apart (`+0x82c = −71`).
const BLOW_APART: i32 = -71;

/// The RC cars out, per player.
#[derive(Resource, Default, Debug)]
pub struct RcCars {
    /// The player's RC car (`_DAT_1017c484[player]`, −1 = none).
    pub car: [Option<Entity>; 4],
    /// Camera hold after it blew up (`_DAT_1017c364[player]`).
    pub hold: [i32; 4],
    /// Fire held last tick (`_DAT_1017c32c[player]`).
    fire_prev: [bool; 4],
}

impl RcCars {
    /// The RC car the player's camera follows, if one is out.
    pub fn follow(&self, player: usize) -> Option<Entity> {
        self.car.get(player).copied().flatten()
    }

    /// The camera holds still (after the RC car blew up).
    pub fn holding(&self, player: usize) -> bool {
        self.hold.get(player).is_some_and(|&h| h > 0)
    }

    /// The RC car `e` is gone: free its player's slot and hold the camera
    /// (`DoCollideStuff` @ 10067b50: health < −70 → slot −1, hold 60).
    pub fn ended(&mut self, e: Entity, player: u8) {
        let p = player as usize & 3;
        if self.car[p] == Some(e) {
            self.car[p] = None;
        }
        self.hold[p] = CAMERA_HOLD;
        info!("rc car: player {p}'s {e:?} blew up");
    }
}

/// An RC car and the player it belongs to (`+0x998`).
#[derive(Component, Clone, Copy, Debug)]
pub struct RcCar {
    pub player: u8,
}

/// The player's RC car controls (Mac `do_Other` @ 10146280, the `+0x8160`
/// branch), before the driving step: the RC car takes the player's
/// keys, the truck none. A fire press detonates it. Getting out of the
/// truck or losing it blows the RC car up (`GetInOutVehicles` @ 10066140,
/// `DoCollideStuff` @ 10067b50).
pub(super) fn rc_controls(
    inputs: Res<PlayerInputs>,
    mut rc: ResMut<RcCars>,
    mut triggers: ResMut<Triggers>,
    mut trucks: Query<(&Obj, &Vehicle, &mut Stick), With<CameraTarget>>,
    mut cars: Query<(&mut Obj, &mut Stick), (With<RcCar>, Without<CameraTarget>)>,
) {
    for p in 0..4 {
        if rc.hold[p] > 0 {
            rc.hold[p] -= 1;
        }
        let Some(e) = rc.car[p] else { continue };
        let Ok((mut car, mut stick)) = cars.get_mut(e) else {
            // Despawned without blowing up (level end, script removal).
            rc.car[p] = None;
            continue;
        };
        // The player still driving the bomb truck?
        let truck = trucks.iter_mut().find(|(o, v, _)| {
            o.player.map(|n| n as usize) == Some(p) && o.state == State::Player && v.kind == vk::BOMB_TRUCK
        });
        let Some((_, _, mut truck_stick)) = truck else {
            car.health = car.health.min(BLOW_APART);
            *stick = Stick::default();
            rc.car[p] = None;
            continue;
        };
        *truck_stick = Stick::default();
        let input = &inputs.0[p];
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
        if input.accel {
            // `PlAccelerate` @ 100f71a0: trigger 0x42.
            triggers.report(kind::RC_THROTTLE, 0, 0, 0);
        }
        if input.brake {
            if car.speed > 0 {
                stick.throttle = 1;
            } else if car.speed < 0 {
                stick.throttle = -1;
            }
        }
        stick.brake = input.brake;
        stick.fire = false;
        if input.fire && !rc.fire_prev[p] {
            car.health = BLOW_APART;
            rc.fire_prev[p] = true;
        }
    }
}

/// The bomb truck's fire key (Mac `DoVehicleFire` @ 10070c20, the
/// `+0x815c` branch, called at the end of the truck's `do_Other`): a press
/// with no RC car out launches one. Then the fire latch is kept for the
/// next tick (`MainLoop` @ 1014a920 copies the fire keys).
pub(super) fn rc_launch(
    mut commands: Commands,
    inputs: Res<PlayerInputs>,
    mut rc: ResMut<RcCars>,
    trucks: Query<(&Obj, &Vehicle), With<CameraTarget>>,
) {
    for (truck, veh) in &trucks {
        if veh.kind != vk::BOMB_TRUCK || truck.state != State::Player {
            continue;
        }
        let p = truck.player.unwrap_or(0) as usize & 3;
        if !inputs.0[p].fire || rc.fire_prev[p] || rc.car[p].is_some() {
            continue;
        }
        // Taken until the queued spawn fills in the entity.
        rc.car[p] = Some(Entity::PLACEHOLDER);
        let (pos, heading, vel, speed, level, floor_z, on_ground) =
            (truck.pos, truck.heading, truck.vel, truck.speed, truck.level, truck.floor_z, truck.on_ground);
        commands.queue(move |world: &mut World| {
            let e = crate::vehicles::spawn_vehicle_world(world, vk::RC_CAR as i32, pos, heading, 0);
            if let Some(e) = e {
                if let Some(mut o) = world.get_mut::<Obj>(e) {
                    o.state = State::Idle;
                    o.player = None;
                    // Player-owned (`+0x824 = 1`): its blast pays.
                    o.side = 1;
                    o.vel = vel;
                    o.speed = speed;
                    o.level = level;
                    o.floor_z = floor_z;
                    o.on_ground = on_ground;
                }
                if let Some(mut d) = world.get_mut::<super::driving::Drive>(e) {
                    // The last clear position is the truck's (`+0x8f0`).
                    d.clear = Some((pos, pos));
                }
                world.entity_mut(e).insert(RcCar { player: p as u8 });
            }
            info!("rc car: player {p} launched {e:?} at {pos}");
            world.resource_mut::<RcCars>().car[p] = e;
        });
    }
    for p in 0..4 {
        rc.fire_prev[p] = inputs.0[p].fire;
    }
}
