//! Scripted actor orders (docs/re/people.md §7.1, script_api.md §5,
//! progression notes D.2-D.5): the AI states mission scripts put actors in.
//! People execute them here; vehicles steer with the same orders in
//! `vehicles`.

use super::combat::{Arsenal, Weapon};
use super::people::Person;
use super::{Obj, Rng, State, Stick, angle_diff, heading_of};
use crate::camera::CameraTarget;
use crate::mission::{Triggers, kind};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Order {
    /// State 4: stand still.
    Idle,
    /// State 5: parked / inert (`Actor_Release`).
    Park,
    /// State 3: ambient pedestrian or traffic (`Actor_ReleaseIdle`).
    Ambient,
    /// State 0xf/0x15: go to (x, y), stop within `radius`; reports
    /// trigger 0x17.
    Goto(IVec2, i32),
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

#[derive(Component, Clone, Debug)]
pub struct Ai {
    pub order: Order,
    /// `+0x346`: fire rate / driving aggression.
    pub aggro: i32,
    /// `+0x386`: maximum speed.
    pub max_speed: i32,
    pub follow: Option<Entity>,
}

impl Default for Ai {
    fn default() -> Self {
        Ai {
            order: Order::Idle,
            aggro: 0,
            max_speed: i32::MAX,
            follow: None,
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
    if let Some(mut ai) = ai_mut(world, e) {
        ai.order = order;
    }
    let state = match order {
        Order::Park => State::Parked,
        Order::Ambient => State::Wander,
        Order::Flee => State::Flee,
        _ => State::Idle,
    };
    if let Order::Attack { .. } = order {
        // Attackers get a shotgun with 100 shells.
        let mut ars = Arsenal::default();
        ars.current = Weapon::Shotgun;
        ars.ammo[Weapon::Shotgun as usize] = 100;
        world.entity_mut(e).insert(ars);
        if let Some(mut ai) = world.get_mut::<Ai>(e) {
            if let Order::Attack { aggro, .. } = order {
                ai.aggro = aggro;
            }
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
    for (e, mut obj, mut ai, mut stick, ars) in &mut q {
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
        match ai.order {
            Order::Idle | Order::Park | Order::Ambient | Order::Follow(_) => {}
            Order::Goto(goal, radius) => {
                if (goal - obj.pos.truncate()).abs().max_element() < radius {
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
            Order::BoardPlayer => {
                // Walk to the player's vehicle; boarding needs the player
                // to be driving.
                if let Ok((_, v)) = player.single() {
                    if (v.pos - obj.pos).truncate().abs().max_element() < 512 {
                        triggers.report_actor(kind::ACTOR_BOARDS_PLAYER, e);
                        obj.state = State::Removed;
                        commands.entity(e).despawn();
                    } else {
                        *stick = steer_to(&obj, v.pos.truncate(), cap);
                    }
                }
            }
            Order::BoardActor(target) => {
                if let Ok(v) = others.get(target) {
                    if (v.pos - obj.pos).truncate().abs().max_element() < 512 {
                        triggers.report_actor(kind::ACTOR_BOARDS_ACTOR, e);
                        obj.state = State::Removed;
                        commands.entity(e).despawn();
                    } else {
                        *stick = steer_to(&obj, v.pos.truncate(), cap);
                    }
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
