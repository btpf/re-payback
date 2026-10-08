//! Scripted actor orders (docs/re/people.md §7.1, script_api.md §5,
//! progression notes D.2-D.5): the AI states mission scripts put actors in.
//! People execute them here; vehicles steer with the same orders in
//! `vehicle_ai`.

use super::combat::{Arsenal, Weapon};
use super::people::Person;
use super::{Obj, Rng, State, Stick, angle_diff, heading_of};
use crate::camera::CameraTarget;
use crate::mission::{Triggers, kind};
use crate::vehicles::Vehicle;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Order {
    /// State 4: stand still.
    Idle,
    /// State 5: parked / inert (`Actor_Release`).
    Park,
    /// State 3: ambient pedestrian or traffic (`Actor_ReleaseIdle`).
    Ambient,
    /// State 0xf: go to (x, y), stop within `radius`; reports
    /// trigger 0x17 (trigger types 0x17 / 0x32).
    Goto(IVec2, i32),
    /// State 0x15 (trigger type 0x44): as `Goto` with radius 1000, but a
    /// police car in it is also subject to pursuit selection (68k
    /// `FUN_000e3a58` ~124185 lists states 3/8/9/10/11/0x14/0x15, not
    /// 0xf): traffic below wanted 3, pursuit from 3.
    Goto15(IVec2),
    /// State 0xe: go to (x, y) and vanish on arrival; reports 0x12.
    GotoVanish(IVec2),
    /// State 0xd: walk to the player's vehicle and get in; reports 0x0c.
    BoardPlayer,
    /// State 0x13: walk to actor `0`'s vehicle and get in; reports 0x3d.
    BoardActor(Entity),
    /// State 0x10/0x11: attack the player (`None`) or an actor.
    Attack { aggro: i32, target: Option<Entity> },
    /// State 0x12: flee from the player.
    Flee,
    /// State 0x14: flee via the road graph to the farthest node.
    FleeFar,
    /// Follow an actor (or the player); `None` stops following.
    Follow(Option<Entity>),
}

impl Order {
    /// State 0x15 drives exactly like 0xf (radius 1000): fold it into
    /// `Goto` for the movement code.
    pub fn as_goto(self) -> Order {
        match self {
            Order::Goto15(goal) => Order::Goto(goal, 1000),
            o => o,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct Ai {
    pub order: Order,
    /// `+0x346`: fire rate / driving aggression.
    pub aggro: i32,
    /// `+0x386`: maximum speed.
    pub max_speed: i32,
    pub follow: Option<Entity>,
    /// `+0x49e`: set by trigger 0x32 and never cleared ("race driver"):
    /// steers straight at the goal, ignores the corner speeds and honks
    /// at nothing (vehicles.md §6.3).
    pub direct: bool,
    /// `+0x30e`: door waypoint of a boarding passenger (people.md §6.1):
    /// 0 door, 1..4 corners, 5 "door, re-plan on contact".
    pub waypoint: u8,
}

impl Default for Ai {
    fn default() -> Self {
        Ai {
            order: Order::Idle,
            aggro: 0,
            max_speed: i32::MAX,
            follow: None,
            direct: false,
            waypoint: 5,
        }
    }
}

fn ai_mut(world: &mut World, e: Entity) -> Option<Mut<'_, Ai>> {
    if world.get::<Ai>(e).is_none() {
        world.entity_mut(e).insert(Ai::default());
    }
    world.get_mut::<Ai>(e)
}

/// Give an actor an order (from the script API).
pub fn give_order(world: &mut World, e: Entity, order: Order) {
    let is_player = world
        .get::<Obj>(e)
        .is_some_and(|o| o.state == State::Player);
    if is_player || world.get_entity(e).is_err() {
        return;
    }
    if let Order::Follow(lead) = order {
        if let Some(mut ai) = ai_mut(world, e) {
            ai.follow = lead;
        }
        return;
    }
    let vehicle = world.get::<Vehicle>(e).is_some();
    if let Some(mut ai) = ai_mut(world, e) {
        ai.order = order;
        ai.waypoint = 5;
    }
    let state = match order {
        Order::Park => State::Parked,
        Order::Ambient => State::Wander,
        Order::Flee if !vehicle => State::Flee,
        _ => State::Idle,
    };
    if vehicle {
        // A fresh order restarts route following; ambient cars go back to
        // the traffic driver (state 3).
        let aggro = world.get::<Ai>(e).map_or(0, |a| a.aggro);
        let mut ent = world.entity_mut(e);
        ent.insert(super::vehicle_ai::Nav::default());
        if order == Order::Ambient {
            if let Some(o) = ent.get::<Obj>() {
                let tr = super::traffic::resume(o, aggro);
                ent.insert((tr, super::people::Ambient));
            }
        }
    } else if let Order::Attack { .. } = order {
        // Attackers get a shotgun with 100 shells.
        let mut ars = Arsenal::default();
        ars.current = Weapon::Shotgun;
        ars.ammo[Weapon::Shotgun as usize] = 100;
        world.entity_mut(e).insert(ars);
    }
    if let Order::Attack { aggro, .. } = order {
        if let Some(mut ai) = world.get_mut::<Ai>(e) {
            ai.aggro = aggro;
        }
    }
    if let Some(mut o) = world.get_mut::<Obj>(e) {
        o.state = state;
    }
    if let Some(mut p) = world.get_mut::<Person>(e) {
        p.wander = -1;
        if state == State::Flee {
            p.flee = i32::MAX;
        }
    }
}

pub fn set_aggro(world: &mut World, e: Entity, v: i32) {
    if let Some(mut ai) = ai_mut(world, e) {
        ai.aggro = v;
    }
}

pub fn set_max_speed(world: &mut World, e: Entity, v: i32) {
    if let Some(mut ai) = ai_mut(world, e) {
        ai.max_speed = v;
    }
}

/// Trigger 0x32 (`AddTrigger` case at 68k `0xb3866`): `+0x49e = 1`.
pub fn set_direct(world: &mut World, e: Entity) {
    if let Some(mut ai) = ai_mut(world, e) {
        ai.direct = true;
    }
}

/// Steer toward `goal`: turn, and walk once within 200 angle units.
fn steer_to(obj: &Obj, goal: IVec2, cap: i32) -> Stick {
    let want = heading_of(goal.x - obj.pos.x, goal.y - obj.pos.y);
    let err = angle_diff(obj.heading, want);
    Stick {
        steer: if err > 0 {
            -1
        } else if err < 0 {
            1
        } else {
            0
        },
        throttle: if err.abs() < 200 && obj.speed < cap {
            -1
        } else {
            0
        },
        ..default()
    }
}

/// People following script orders (runs after the ambient AI, before
/// movement, overriding the stick).
#[allow(clippy::type_complexity)]
pub fn people_orders(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    mut triggers: ResMut<Triggers>,
    player: Query<(Entity, &Obj), (With<CameraTarget>, Without<Person>)>,
    player_foot: Query<(Entity, &Obj), (With<CameraTarget>, With<Person>)>,
    others: Query<&Obj, Without<Ai>>,
    vehicles: Query<(&Obj, &Vehicle), Without<Person>>,
    trig: Res<super::Trig>,
    library: Option<Res<crate::objects::ObjectLibrary>>,
    mut q: Query<
        (Entity, &mut Obj, &mut Ai, &mut Stick, Option<&mut Arsenal>),
        (With<Person>, Without<CameraTarget>),
    >,
) {
    let player_pos = player
        .iter()
        .chain(player_foot.iter())
        .map(|(e, o)| (e, o.pos))
        .next();
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, mut ai, mut stick, ars)) = q.get_mut(e__) else { continue };
        if matches!(
            obj.state,
            State::Dead | State::Removed | State::Wander | State::Flee
        ) {
            continue;
        }
        // Following overrides idle standing.
        if let (Some(lead), Order::Idle) = (ai.follow, ai.order) {
            let goal = others
                .get(lead)
                .ok()
                .map(|l| l.pos)
                .or(player_pos.filter(|(pe, _)| *pe == lead).map(|(_, p)| p));
            if let Some(g) = goal {
                *stick = steer_to(&obj, g.truncate(), 0x30);
            }
            continue;
        }
        let cap = ai.max_speed.min(0x30);
        match ai.order.as_goto() {
            // (`as_goto` folds 0x15 into `Goto`.)
            Order::Idle | Order::Park | Order::Ambient | Order::Follow(_) | Order::Goto15(_) => {}
            Order::Goto(goal, radius) => {
                let d = (goal - obj.pos.truncate()).abs();
                if d.x + d.y <= radius {
                    triggers.report_actor(kind::ACTOR_GOTO, e);
                    ai.order = Order::Idle;
                    *stick = Stick::default();
                } else {
                    *stick = steer_to(&obj, goal, cap);
                }
            }
            Order::GotoVanish(goal) => {
                let d = goal - obj.pos.truncate();
                if d.x.abs() + d.y.abs() < 0x579 {
                    triggers.report_actor(kind::ACTOR_GOTO_VANISH, e);
                    obj.state = State::Removed;
                    commands.entity(e).despawn();
                } else {
                    *stick = steer_to(&obj, goal, cap);
                }
            }
            Order::BoardPlayer | Order::BoardActor(_) => {
                // State 0x0d / 0x13: walk the door waypoints of the
                // player's (or actor b's) vehicle and get in at the door
                // (people.md §6.1, progression D.5). Nothing while the
                // player is on foot.
                let target = match ai.order {
                    Order::BoardActor(t) => Some(t),
                    _ => player.single().ok().map(|(pe, _)| pe),
                };
                let (Some((v, veh)), Some(lib)) = (target.and_then(|t| vehicles.get(t).ok()), &library) else {
                    *stick = Stick::default();
                    continue;
                };
                let info = &lib.model(v.model).info;
                let door_y = super::boarding::door_y(veh.kind, info.max_y);
                let point = |wp: u8| {
                    let (lat, fwd) = match wp {
                        1 => (info.radius, info.max_y),
                        2 => (-info.radius, info.max_y),
                        3 => (info.radius, -info.min_y),
                        4 => (-info.radius, -info.min_y),
                        _ => (-(info.radius + 192), door_y),
                    };
                    super::boarding::car_point(&trig, v, lat, fwd)
                };
                if ai.waypoint == 5 {
                    // First contact with the car: head for the nearest
                    // waypoint instead (walk round to the door).
                    let rel = obj.pos - v.pos;
                    let (s, c) = (trig.sin(v.heading), trig.cos(v.heading));
                    let fwd = (rel.x * s + rel.y * c) >> 8;
                    let lat = (rel.x * c - rel.y * s) >> 8;
                    let touching = lat.abs() < info.radius + 256 && fwd > -info.min_y - 256 && fwd < info.max_y + 256;
                    if touching {
                        ai.waypoint = (0..5u8)
                            .min_by_key(|&w| {
                                let d = (point(w) - obj.pos).truncate().as_i64vec2();
                                d.x * d.x + d.y * d.y
                            })
                            .unwrap_or(0);
                    }
                }
                let goal = point(ai.waypoint);
                let d = goal - obj.pos;
                if d.x.abs() < 512 && d.y.abs() < 512 && (v.pos.z - obj.pos.z).abs() <= 0x7ff {
                    ai.waypoint = match ai.waypoint {
                        1 => 2,
                        3 => 4,
                        2 | 4 => 0,
                        _ => {
                            let k = if matches!(ai.order, Order::BoardActor(_)) {
                                kind::ACTOR_BOARDS_ACTOR
                            } else {
                                kind::ACTOR_BOARDS_PLAYER
                            };
                            triggers.report_actor(k, e);
                            obj.state = State::Removed;
                            commands.entity(e).despawn();
                            continue;
                        }
                    };
                } else {
                    *stick = steer_to(&obj, goal.truncate(), cap);
                }
            }
            Order::Attack { aggro, target } => {
                let goal = target
                    .and_then(|t| others.get(t).ok().map(|o| o.pos))
                    .or(player_pos.map(|p| p.1));
                let Some(goal) = goal else { continue };
                let d = (goal - obj.pos).truncate();
                *stick = steer_to(&obj, goal.truncate(), 0x30);
                if (d.x.abs() + d.y.abs()) / 2 < 6000 {
                    stick.throttle = 0;
                    let fire = ars.is_some_and(|a| a.fire_timer >= 0)
                        && rng.rand((0x65 - aggro).max(1)) < 1;
                    stick.fire = fire;
                }
            }
            Order::Flee | Order::FleeFar => {
                if let Some((_, pp)) = player_pos {
                    let away = heading_of(obj.pos.x - pp.x, obj.pos.y - pp.y);
                    let err = angle_diff(obj.heading, away);
                    stick.steer = if err > 0 {
                        -1
                    } else if err < 0 {
                        1
                    } else {
                        0
                    };
                    stick.throttle = if obj.speed < 0x18 { -1 } else { 0 };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goto_15_drives_like_goto() {
        let g = IVec2::new(0x3efd0, 0x21c00);
        assert_eq!(Order::Goto15(g).as_goto(), Order::Goto(g, 1000));
        assert_eq!(Order::Goto(g, 600).as_goto(), Order::Goto(g, 600));
        assert_eq!(Order::Ambient.as_goto(), Order::Ambient);
    }
}
