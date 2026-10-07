//! Per-tick trigger evaluation (68k `Trigger_Update` `0xa4a68`).

use super::api::Script;
use super::triggers::{Trigger, Triggers, kind};
use super::{ActiveLevel, MissionState};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::hud::HudData;
use crate::objects::SetScenery;
use crate::sim::people::{Person, PlayerSession};
use crate::sim::{Obj, State, angle_diff, heading_of};
use crate::vehicles::Vehicle;
use crate::world::City;
use bevy::prelude::*;
use payback_formats::map::MAP_SIZE;

/// What triggers need to know about the player this tick.
struct PlayerView {
    entity: Entity,
    pos: IVec3,
    level: usize,
    on_foot: bool,
    controlled: bool,
    speed: i32,
    vehicle_type: i32,
    money: i32,
    down: bool,
}

struct ActorView {
    pos: IVec3,
    level: usize,
    heading: i32,
    speed: i32,
    alive: bool,
    in_vehicle: bool,
}

fn within(a: IVec3, b: IVec2, r: i32) -> bool {
    (a.x - b.x).abs() <= r && (a.y - b.y).abs() <= r
}

/// Run the evaluator and dispatch fired events to the level handler.
pub fn run_triggers(world: &mut World) {
    if world.resource::<ActiveLevel>().0.is_none() || world.resource::<MissionState>().finished {
        return;
    }
    let Some(player) = player_view(world) else {
        return;
    };
    let fired = evaluate(world, &player);

    let mut complete = false;
    for (ev, ty) in fired {
        if ty == kind::MONEY_GOAL {
            complete = true;
        }
        dispatch(world, ev);
    }
    if complete && !world.resource::<MissionState>().finished {
        Script::new(world).level_complete();
    }
    let stopped = world.resource_mut::<Triggers>().apply_pending();
    for e in stopped {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    // HUD clock from the first visible countdown.
    let t = world.resource::<Triggers>().countdown_seconds();
    world.resource_mut::<HudData>().timer = t;
}

fn dispatch(world: &mut World, ev: i32) {
    let Some(mut level) = world.resource_mut::<ActiveLevel>().0.take() else {
        return;
    };
    level.event(&mut Script::new(world), ev);
    world.resource_mut::<ActiveLevel>().0 = Some(level);
}

fn player_view(world: &mut World) -> Option<PlayerView> {
    let session_down = world.resource::<PlayerSession>().respawn_timer > 0;
    let mut q =
        world.query_filtered::<(Entity, &Obj, Has<Person>, Option<&Vehicle>), With<CameraTarget>>();
    let (entity, o, on_foot, veh) = q.iter(world).next()?;
    Some(PlayerView {
        entity,
        pos: o.pos,
        level: o.level,
        on_foot,
        controlled: o.state == State::Player,
        speed: o.speed,
        vehicle_type: veh.map(|v| v.kind as i32).unwrap_or(-1),
        money: o.score,
        down: session_down || matches!(o.state, State::Dead | State::Removed),
    })
}

fn actor_view(world: &World, e: Entity) -> Option<ActorView> {
    let o = world.get::<Obj>(e)?;
    Some(ActorView {
        pos: o.pos,
        level: o.level,
        heading: o.heading,
        speed: o.speed,
        alive: !matches!(o.state, State::Dead | State::Removed) && o.health > 0,
        in_vehicle: world.get::<Vehicle>(e).is_some(),
    })
}

/// Evaluate every trigger; return fired (event, type) in table order and
/// queue their removal.
fn evaluate(world: &mut World, p: &PlayerView) -> Vec<(i32, u8)> {
    let mut triggers = std::mem::take(&mut world.resource_mut::<Triggers>().list);
    let reports = std::mem::take(&mut world.resource_mut::<Triggers>().reports);
    let actor_reports = std::mem::take(&mut world.resource_mut::<Triggers>().actor_reports);
    let city_scenery = |world: &World, x: i32, y: i32, c: i32| -> Option<u8> {
        let city = world.get_resource::<City>()?;
        if !(0..MAP_SIZE as i32).contains(&x) || !(0..MAP_SIZE as i32).contains(&y) {
            return None;
        }
        Some(city.map.square(x as usize, y as usize).cubes[c.clamp(0, 2) as usize].scenery)
    };
    let mut fired = Vec::new();
    let mut keep = Vec::with_capacity(triggers.len());
    let mut stopped = Vec::new();
    for mut t in triggers.drain(..) {
        let actor = t.actor.and_then(|e| actor_view(world, e));
        // Actor gone: most actor triggers silently vanish; "gone/killed"
        // ones fire.
        if t.actor.is_some() && actor.is_none() {
            if matches!(
                t.kind,
                kind::ACTOR_GONE
                    | kind::ACTOR_KILLED
                    | kind::ACTOR_ATTACKED
                    | kind::ACTOR_ATTACKED_QUIET
            ) {
                fired.push((t.event, t.kind));
            }
            stopped.extend(t.sound);
            continue;
        }
        let fire = condition(
            world,
            &mut t,
            p,
            actor.as_ref(),
            &reports,
            &actor_reports,
            &city_scenery,
        );
        if fire {
            fired.push((t.event, t.kind));
            stopped.extend(t.sound);
        } else {
            keep.push(t);
        }
    }
    world.resource_mut::<Triggers>().list = keep;
    for e in stopped {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    fired
}

#[allow(clippy::too_many_arguments)]
fn condition(
    world: &mut World,
    t: &mut Trigger,
    p: &PlayerView,
    actor: Option<&ActorView>,
    reports: &[(u8, i32, i32, i32)],
    actor_reports: &[(u8, Entity)],
    city_scenery: &dyn Fn(&World, i32, i32, i32) -> Option<u8>,
) -> bool {
    use kind::*;
    let at = |r: i32| within(p.pos, IVec2::new(t.a, t.b), r) && p.level as i32 == t.c;
    let reported = |k: u8| reports.iter().any(|r| r.0 == k);
    let actor_reported = |k: u8| {
        t.actor
            .is_some_and(|e| actor_reports.iter().any(|r| r.0 == k && r.1 == e))
    };
    let rect = || {
        let (x0, x1) = (t.a.min(t.c), t.a.max(t.c));
        let (y0, y1) = (t.b.min(t.d), t.b.max(t.d));
        p.pos.x > x0 && p.pos.x < x1 && p.pos.y > y0 && p.pos.y < y1
    };
    match t.kind {
        MONEY_GOAL => p.money >= t.a,
        PHONE | SECRET_PHONE => {
            let Some((pos, lvl)) = Script::new(world).phone_coords(t.a) else {
                return false;
            };
            p.controlled && p.on_foot && p.level == lvl && within(p.pos, pos.truncate(), 0x400)
        }
        AT_ACTOR_ON_FOOT => actor
            .is_some_and(|a| p.controlled && p.on_foot && within(p.pos, a.pos.truncate(), 0x400)),
        IN_VEHICLE_MODEL => p.vehicle_type == t.a,
        COUNTDOWN | TIMER => {
            t.counter -= 1;
            if t.kind == COUNTDOWN && t.counter > 0 && t.counter % 30 == 0 {
                world.spawn(Sound::once(id::TICK_TOCK, 255.0, 8400.0, SoundPos::Global));
            }
            t.counter <= 0
        }
        PLAYER_DOWN => p.down,
        ENTER_ACTOR | ENTER_ACTOR_QUIET => t.actor == Some(p.entity),
        ACTOR_GONE => actor_reported(ACTOR_GONE),
        MONEY_GAIN => p.money as i64 >= t.counter,
        IN_ANY_VEHICLE => !p.on_foot,
        NEAR_ACTOR => actor.is_some_and(|a| p.controlled && within(p.pos, a.pos.truncate(), 6000)),
        VEHICLE_CHANGED => p.vehicle_type as i64 != t.counter,
        AT_POINT => p.controlled && at(0x1000),
        STOPPED_AT_POINT => p.controlled && at(0x1000) && p.speed.abs() < 0x14,
        ACTOR_FASTER => actor.is_some_and(|a| a.speed > t.b),
        ACTOR_SLOWER => actor.is_some_and(|a| a.speed < t.b),
        NEAR_POINT_12000 => p.controlled && at(12000),
        ACTOR_OUTSIDE => actor.is_some_and(|a| !within(a.pos, IVec2::new(t.b, t.c), 0x1800)),
        ACTOR_NEAR_PLAYER => actor.is_some_and(|a| within(a.pos, p.pos.truncate(), 6000)),
        ACTOR_LOST | ACTOR_FLEES => actor.is_some_and(|a| !within(a.pos, p.pos.truncate(), 33000)),
        NEAR_POINT_10000 => p.controlled && at(10000),
        ACTOR_SEES_PLAYER | ACTOR_SEES_PLAYER_QUIET => actor.is_some_and(|a| {
            let range: i64 = if a.in_vehicle && t.kind == ACTOR_SEES_PLAYER {
                16_000_000
            } else {
                6_000_000
            };
            sees(world, a, p, range)
        }),
        PLAYER_OUTSIDE => !within(p.pos, IVec2::new(t.a, t.b), 0x1800),
        ACTOR_NEAR_ACTOR => {
            let others = Script::new(world).actors(t.b);
            actor.is_some_and(|a| {
                others
                    .iter()
                    .filter_map(|&o| actor_view(world, o))
                    .any(|b| {
                        within(a.pos, b.pos.truncate(), 3000)
                            && angle_diff(
                                a.heading,
                                heading_of(b.pos.x - a.pos.x, b.pos.y - a.pos.y),
                            )
                            .abs()
                                < 0x5a0
                    })
            })
        }
        PLAYER_X_LE => p.pos.x <= t.a,
        PLAYER_X_GT => p.pos.x > t.a,
        PLAYER_Y_LE => p.pos.y <= t.a,
        PLAYER_Y_GT => p.pos.y > t.a,
        WEAPON_FIRED => reports.iter().any(|r| r.0 == WEAPON_FIRED && r.1 == t.a),
        WEAPON_SELECTED => reports.iter().any(|r| r.0 == WEAPON_SELECTED && r.1 == t.a),
        WEAPON_SWITCHED => reported(WEAPON_SWITCHED),
        SCENERY_GONE | SCENERY_GONE_QUIET => city_scenery(world, t.a, t.b, t.c) == Some(0),
        OUT_OF_AMMO => reports.iter().any(|r| r.0 == OUT_OF_AMMO && r.1 == t.a),
        HANDBRAKE => reported(HANDBRAKE),
        ACTOR_KILLED => actor.is_some_and(|a| !a.alive),
        COP_KILLED => reported(COP_KILLED),
        CUBE_SMASHED => reports
            .iter()
            .any(|r| r.0 == CUBE_SMASHED && (r.1, r.2, r.3) == (t.a, t.b, t.c)),
        ACTOR_IN_WATER => actor_reported(ACTOR_IN_WATER),
        ON_FOOT_AT_POINT | ON_FOOT_AT_POINT_QUIET => p.controlled && p.on_foot && at(0x200),
        ACTOR_ATTACKED | ACTOR_ATTACKED_QUIET => {
            actor.is_some_and(|a| !a.alive) || actor_reported(ACTOR_ATTACKED)
        }
        ACTOR_X_LE => actor.is_some_and(|a| a.pos.x <= t.b),
        ACTOR_X_GT => actor.is_some_and(|a| a.pos.x > t.b),
        ACTOR_Y_LE => actor.is_some_and(|a| a.pos.y <= t.b),
        ACTOR_Y_GT => actor.is_some_and(|a| a.pos.y > t.b),
        ON_FOOT_NEAR_POINT => p.controlled && p.on_foot && at(0x4b0),
        IN_RECT => p.controlled && rect() && p.level as i32 == t.e,
        OUT_OF_RECT => p.controlled && !rect(),
        ACTOR_INSIDE => actor.is_some_and(|a| within(a.pos, IVec2::new(t.b, t.c), 0x1800)),
        NEAR_ACTOR_1000 => actor.is_some_and(|a| {
            p.controlled && a.level == p.level && within(p.pos, a.pos.truncate(), 1000)
        }),
        RESPAWNED => reported(RESPAWNED),
        NEAR_POINT_6000 => p.controlled && at(6000),
        AT_POINT_400 => p.controlled && at(0x400),
        RC_THROTTLE => reported(RC_THROTTLE),
        // Completion reported by the AI when the ordered action finishes.
        ACTOR_GOTO_VANISH | ACTOR_GOTO | ACTOR_GOTO_15 | ACTOR_BOARDS_PLAYER
        | ACTOR_BOARDS_ACTOR => actor_reported(t.kind),
        GARAGE => {
            p.controlled
                && !p.on_foot
                && within(
                    p.pos,
                    IVec2::new(t.a << 11 | 0x400, t.b << 11 | 0x400),
                    1000,
                )
                && p.level as i32 == t.c - 1
        }
        _ => false,
    }
}

/// "Actor sees the player": same floor, alive, in range, within ±90° of
/// the actor's heading, with line of sight (script_api.md type 0x1b).
fn sees(world: &World, a: &ActorView, p: &PlayerView, range: i64) -> bool {
    if a.level != p.level || !a.alive {
        return false;
    }
    let d = (p.pos - a.pos).as_i64vec3();
    if d.x * d.x + d.y * d.y >= range * 64 {
        return false;
    }
    if angle_diff(a.heading, heading_of(d.x as i32, d.y as i32)).abs() > 0x5a0 {
        return false;
    }
    world
        .get_resource::<City>()
        .is_some_and(|c| crate::sim::collide::line_of_sight(c, a.pos, p.pos))
}

/// Ringing phones flash (scenery 14 <-> 19) while their trigger is active;
/// cells are restored to their original scenery afterwards.
pub fn flash_phones(
    time: Res<Time>,
    triggers: Res<Triggers>,
    city: Option<Res<City>>,
    mut set: MessageWriter<SetScenery>,
    mut timer: Local<f32>,
    mut flashed: Local<std::collections::HashMap<(usize, usize, usize), u8>>,
) {
    let Some(city) = city else { return };
    *timer += time.delta_secs();
    if *timer < 0.4 {
        return;
    }
    *timer = 0.0;
    let ringing: Vec<i32> = triggers
        .list
        .iter()
        .filter(|t| t.kind == kind::PHONE)
        .map(|t| t.a)
        .collect();
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for (level, c) in city.map.square(x, y).cubes.iter().enumerate() {
                let key = (x, y, level);
                let is_ringing =
                    matches!(c.scenery, 14 | 19) && ringing.contains(&(c.special as i32));
                if is_ringing {
                    flashed.entry(key).or_insert(c.scenery);
                    let next = if c.scenery == 14 { 19 } else { 14 };
                    set.write(SetScenery {
                        sx: x,
                        sy: y,
                        level,
                        scenery: next,
                        special: None,
                        rotation: None,
                    });
                } else if let Some(original) = flashed.remove(&key) {
                    set.write(SetScenery {
                        sx: x,
                        sy: y,
                        level,
                        scenery: original,
                        special: None,
                        rotation: None,
                    });
                }
            }
        }
    }
}
