//! Per-tick trigger evaluation (68k `Trigger_Update` `0xa4a68`).

use super::api::Script;
use super::triggers::{Trigger, Triggers, kind};
use super::{ActiveLevel, MissionState};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::hud::HudData;
use crate::mapedit::{EditOp, MapEdit};
use crate::objects::SetScenery;
use crate::sim::combat::Arsenal;
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
    /// Selected weapon in the original numbering (`+0x35e`).
    weapon_id: i32,
    /// Ammo by HUD slot.
    ammo: [i32; 8],
}

struct ActorView {
    pos: IVec3,
    level: usize,
    heading: i32,
    speed: i32,
    alive: bool,
    in_vehicle: bool,
    script_id: i32,
}

fn within(a: IVec3, b: IVec2, r: i32) -> bool {
    (a.x - b.x).abs() <= r && (a.y - b.y).abs() <= r
}

/// Run the evaluator and dispatch fired events to the level handler.
pub fn run_triggers(world: &mut World) {
    if world.resource::<ActiveLevel>().0.is_none() {
        // No script (rampage, multiplayer): drop the game's reports so
        // they don't pile up.
        let mut t = world.resource_mut::<Triggers>();
        t.reports.clear();
        t.actor_reports.clear();
        return;
    }
    if world.resource::<MissionState>().finished {
        // `Mission_Shutdown` once the level is over.
        if let Some(mut level) = world.resource_mut::<ActiveLevel>().0.take() {
            level.shutdown(&mut Script::new(world));
        }
        return;
    }
    let Some(player) = player_view(world) else {
        return;
    };
    // `DAT_0114c628 = 1` before `Trigger_Update` every frame (OS4 ~49717,
    // 68k ~127806).
    world.resource_mut::<MissionState>().exit_toggle = true;
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
    let slot = world.resource::<MissionState>().cur_slot;
    crate::autoplay::log(format_args!("event {ev} ({ev:#x}), slot {slot}"));
    level.event(&mut Script::new(world), ev);
    world.resource_mut::<ActiveLevel>().0 = Some(level);
}

fn player_view(world: &mut World) -> Option<PlayerView> {
    let session_down = world.resource::<PlayerSession>().respawn_timer > 0;
    let mut q = world.query_filtered::<(
        Entity,
        &Obj,
        Has<Person>,
        Option<&Vehicle>,
        Option<&Arsenal>,
    ), With<CameraTarget>>();
    let (entity, o, on_foot, veh, ars) = q.iter(world).next()?;
    let ars = ars.cloned().unwrap_or_default();
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
        weapon_id: ars.current.original_id(),
        ammo: ars.ammo,
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
        script_id: o.script_id,
    })
}

/// Evaluate every trigger; return fired (event, type) in table order and
/// queue their removal.
fn evaluate(world: &mut World, p: &PlayerView) -> Vec<(i32, u8)> {
    let mut triggers = std::mem::take(&mut world.resource_mut::<Triggers>().list);
    let mut reports = std::mem::take(&mut world.resource_mut::<Triggers>().reports);
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
        // Orders: the first completion by any actor that got the order
        // and still carries the id (the Mac's arrival/boarding hooks
        // `ObjectAtLocation`, `ObjectIntoBuilding`, `ObjectIntoPlVehicle`,
        // `ObjectIntoVehicle` compare ids). Until then the trigger waits,
        // also when every actor is gone.
        if !t.group.is_empty() {
            let want = match t.kind {
                kind::ACTOR_GOTO | kind::ACTOR_GOTO_15 => kind::ACTOR_GOTO,
                k => k,
            };
            let done = actor_reports.iter().any(|r| {
                r.0 == want
                    && t.group.contains(&r.1)
                    && world.get::<Obj>(r.1).is_none_or(|o| o.script_id == t.a)
            });
            if done {
                fired.push((t.event, t.kind));
                stopped.extend(t.sound);
            } else {
                keep.push(t);
            }
            continue;
        }
        // Id triggers: keep the bound actor while it is there and carries
        // the id, else bind to the first actor with the id, else wait.
        if t.by_id {
            let bound = t
                .actor
                .and_then(|e| world.get::<Obj>(e))
                .is_some_and(|o| o.script_id == t.a && o.state != State::Removed);
            if !bound {
                t.actor = Script::new(world).actors(t.a).first().copied();
                if t.actor.is_none() {
                    keep.push(t);
                    continue;
                }
            }
        }
        let actor = t.actor.and_then(|e| actor_view(world, e));
        // Actor gone: only a reported death fires the "destroyed/killed"
        // types (0xa653a is called from damage code); removal on arrival or
        // boarding drops the trigger silently.
        if t.actor.is_some() && actor.is_none() {
            let killed = t.actor.is_some_and(|e| {
                actor_reports
                    .iter()
                    .any(|r| r.0 == kind::ACTOR_KILLED && r.1 == e)
            });
            // Orders that end with the actor removed (0x12 arrived and
            // vanished, 0x0c / 0x3d got into the vehicle) report their
            // completion as they remove it.
            let completed = matches!(
                t.kind,
                kind::ACTOR_GOTO_VANISH | kind::ACTOR_BOARDS_PLAYER | kind::ACTOR_BOARDS_ACTOR
            ) && t.actor.is_some_and(|e| actor_reports.iter().any(|r| r.0 == t.kind && r.1 == e));
            if completed
                || killed
                    && matches!(
                        t.kind,
                        kind::ACTOR_GONE
                            | kind::ACTOR_KILLED
                            | kind::ACTOR_ATTACKED
                            | kind::ACTOR_ATTACKED_QUIET
                    )
            {
                fired.push((t.event, t.kind));
            }
            stopped.extend(t.sound);
            continue;
        }
        // The Mac stores the script id and looks the actor up by it every
        // frame (`SetTrigger_Long` @ 10106e50, `CheckForTriggers` @
        // 100f4a50 via `GetObject` @ 100530e0; `ObjectDead` @ 100f6c40
        // compares ids). An actor whose id changed (released by
        // `Actor_Release`/`ReleaseIdle`, id -1) no longer matches: the
        // trigger stays armed but dormant. (The Amiga kept the pointer and
        // went on watching it, 68k `0xa4a68`.)
        if let Some(a) = &actor
            && t.a != -1
            && a.script_id != t.a
        {
            keep.push(t);
            continue;
        }
        let fire = condition(
            world,
            &mut t,
            p,
            actor.as_ref(),
            &mut reports,
            &actor_reports,
            &city_scenery,
        );
        if fire {
            if let Some(a) = &actor {
                crate::autoplay::log(format_args!(
                    "trigger {:#x} type {:#x} fired on actor at {:?} alive {}",
                    t.event, t.kind, a.pos, a.alive
                ));
            }
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
    reports: &mut Vec<(u8, i32, i32, i32)>,
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
            // Phone -1 (a random phone, Mac `GetPhoneCoords(-1)` @
            // 100b4540): the square was picked when it was added.
            let at = if t.a == -1 {
                let s = Script::new(world);
                Some((s.phone_square_pos(t.b as usize, t.c as usize, t.d as usize), t.d as usize))
            } else {
                Script::new(world).phone_coords(t.a)
            };
            let Some((pos, lvl)) = at else {
                return false;
            };
            p.controlled && p.on_foot && p.level == lvl && within(p.pos, pos.truncate(), 0x400)
        }
        // The actor's point and floor taken when the trigger was added.
        AT_ACTOR_ON_FOOT => {
            p.controlled && p.on_foot && p.level as i32 == t.c && within(p.pos, IVec2::new(t.a, t.b), 0x400)
        }
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
        // 0x09 and 0x28 are fired by the same routine (`0xa653a`): a
        // wrecked vehicle (reported as killed) is "gone" too.
        ACTOR_GONE => actor_reported(ACTOR_GONE) || actor_reported(ACTOR_KILLED),
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
        WEAPON_SELECTED => p.weapon_id == t.a,
        WEAPON_SWITCHED => reported(WEAPON_SWITCHED),
        SCENERY_GONE | SCENERY_GONE_QUIET => city_scenery(world, t.a, t.b, t.c) == Some(0),
        // Weapon slots 1 bazooka, 2 flamethrower, 3 grenade, 4 minigun,
        // 5 shotgun -> HUD slots.
        OUT_OF_AMMO => {
            let slot = match t.a {
                1 => 6,
                2 => 3,
                3 => 4,
                4 => 5,
                _ => 2,
            };
            p.ammo[slot] <= 0
        }
        HANDBRAKE => reported(HANDBRAKE),
        ACTOR_KILLED => actor.is_some_and(|a| !a.alive),
        // Each cop kill fires only the first matching trigger.
        COP_KILLED => match reports.iter().position(|r| r.0 == COP_KILLED) {
            Some(i) => {
                reports.remove(i);
                true
            }
            None => false,
        },
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
        // An arrival fires the actor's 0x17 and 0x44 triggers alike (68k
        // `FUN_000a6640`); the AI reports it as 0x17.
        ACTOR_GOTO | ACTOR_GOTO_15 => actor_reported(ACTOR_GOTO),
        ACTOR_GOTO_VANISH | ACTOR_BOARDS_PLAYER | ACTOR_BOARDS_ACTOR => actor_reported(t.kind),
        GARAGE => {
            let centre = IVec2::new(t.a << 11 | 0x400, t.b << 11 | 0x400);
            garage_door(world, t, within(p.pos, centre, 6 * 0x800));
            let arrived = p.controlled
                && !p.on_foot
                && within(p.pos, centre, 1000)
                && p.level as i32 == t.c - 1;
            if arrived {
                garage_door(world, t, false);
                Script::new(world).garage_dropoff(t.a, t.b, t.c - 1, t.d);
            }
            arrived
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

/// The phone cells [`flash_phones`] changed, with their original scenery
/// (cleared at level end).
#[derive(Resource, Default)]
pub struct FlashedPhones(pub std::collections::HashMap<(usize, usize, usize), u8>);

/// Ringing phones flash: every game tick a ringing phone's cell swaps
/// scenery 14 (phone) and 19 (phone with arrow), as Mac `CheckForTriggers`
/// @ 100f4a50 does for each trigger square holding scenery 0xe/0x13 on
/// every call (once per frame of the level loop `amiga_levelloop` @
/// 10100f50, i.e. per game tick here). It runs in the fixed tick: the cell's
/// scenery is part of the simulation (scenery collisions read it). INFERRED:
/// the original leaves the cell as it was when the trigger goes; here it
/// gets its original scenery back.
pub fn flash_phones(
    triggers: Res<Triggers>,
    city: Option<Res<City>>,
    mut set: MessageWriter<SetScenery>,
    mut flashed: ResMut<FlashedPhones>,
) {
    let Some(city) = city else { return };
    let flashed = &mut flashed.0;
    let phones = triggers.list.iter().filter(|t| t.kind == kind::PHONE);
    let ringing: Vec<i32> = phones.clone().filter(|t| t.a != -1).map(|t| t.a).collect();
    // Random phones (-1) ring by square.
    let ringing_at: Vec<(usize, usize, usize)> = phones
        .filter(|t| t.a == -1)
        .map(|t| (t.b as usize, t.c as usize, t.d as usize))
        .collect();
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            for (level, c) in city.map.square(x, y).cubes.iter().enumerate() {
                let key = (x, y, level);
                let is_ringing = matches!(c.scenery, 14 | 19)
                    && (ringing.contains(&(c.special as i32)) || ringing_at.contains(&key));
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

/// Open (face type 5, sound 0x16) or close the door face of a garage
/// trigger's square; the original face type is kept in the trigger's
/// counter while open.
fn garage_door(world: &mut World, t: &mut Trigger, open: bool) {
    let is_open = t.counter != 0;
    if open == is_open {
        return;
    }
    let (sx, sy, cube) = (t.a, t.b, t.c.clamp(0, 2));
    if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) {
        return;
    }
    let face = (t.d.clamp(0, 3) + 1) as usize;
    let current = world
        .get_resource::<City>()
        .map(|c| c.map.square(sx as usize, sy as usize).cubes[cube as usize].faces[face]);
    let Some(current) = current else { return };
    let kind = if open {
        t.counter = face_code(current.kind) as i64 + 1;
        5
    } else {
        let k = (t.counter - 1).max(0) as u8;
        t.counter = 0;
        k
    };
    world.write_message(MapEdit {
        sx: sx as usize,
        sy: sy as usize,
        cube: cube as usize,
        op: EditOp::Face {
            face,
            kind,
            texture: current.texture,
            flip: current.flipped,
        },
    });
    let at = Vec3::new(
        ((sx << 11) | 0x400) as f32,
        ((sy << 11) | 0x400) as f32,
        0.0,
    );
    world.spawn(Sound::once(id::GARAGE_DOOR, 8e6, 10500.0, SoundPos::At(at)));
}

fn face_code(f: payback_formats::map::FaceType) -> u8 {
    use payback_formats::map::FaceType::*;
    match f {
        Off => 0,
        Normal => 1,
        Genlocked => 2,
        Alpha => 3,
        Reflect => 4,
        Unknown(n) => n,
    }
}
