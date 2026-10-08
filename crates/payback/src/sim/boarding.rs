//! Getting into and out of vehicles (docs/re/people.md §6; Mac
//! `GetInOutVehicles` @ 10066140 / OS4 `FUN_01052a4c` enter,
//! `GetOutVehicle` @ 10065660 / `FUN_01052398` exit, `GetOutOfVehicle`
//! @ 10065c90 / `FUN_01052714` hijack).

use super::ai::{Ai, Order};
use super::combat::{Arsenal, Weapon};
use super::driving::Drive;
use super::people::{self, PERSON_RADIUS, Person, PlayerSession};
use super::police::{Busted, WantedTimer};
use super::{Obj, Rng, State, Stick, Trig, angle_diff, heading_of, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::hud::{Banner, Texts};
use crate::objects::ObjectLibrary;
use crate::vehicles::{TYPES, Vehicle, kind as vk};
use crate::world::City;
use bevy::prelude::*;
use payback_formats::map::{self, FaceType, MAP_SIZE};

/// Fully open door angle (55°).
const DOOR_OPEN: i32 = 0x370;

/// A cop has opened the driving player's door: he is pulled out and
/// busted (68k cop AI ~121990, `FUN_000dab40`).
#[derive(Component, Clone, Copy, Debug)]
pub struct PullOut;

/// The player walking to (stage 1) or climbing into (stage 2) a vehicle.
#[derive(Component, Clone, Copy, Debug)]
pub struct Boarding {
    pub vehicle: Entity,
    pub entering: bool,
    /// Walk waypoint (Mac +0x7f0, OS4 +0x310): 0 the door, 1-4 the
    /// corners (front-left, front-right, rear-left, rear-right), 5 the door
    /// with a re-plan on the first contact with a vehicle.
    pub waypoint: u8,
}

/// A person touched a vehicle (Mac +0x98c, OS4 +0x4a4; set by the contact
/// pass, `contact.rs`): a player walking to waypoint 5 re-plans.
#[derive(Component, Clone, Copy, Debug)]
pub struct Touched;

/// The vehicle has an AI driver (Mac +0x824, OS4 +0x344): traffic, and the
/// police cars that leave it. Taking it is a hijack even when it stands
/// in state 5 (a police car whose cop got out).
#[derive(Component, Clone, Copy, Debug)]
pub struct AiDriver;

/// Helicopter exit armed (Mac +0x986, OS4 +0x49e): the next in/out press
/// gets out.
#[derive(Component, Clone, Copy, Debug)]
pub struct HeliExit;

pub struct BoardingPlugin;

impl Plugin for BoardingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (use_pressed, walk_to_door, close_doors)
                .chain()
                .after(super::people::PeopleThink)
                .before(super::people::PeopleSet),
        );
    }
}

/// Driver's door point of a vehicle: its right side (model -X) at the
/// door's longitudinal position, `extra` units further out.
pub(crate) fn door_point(trig: &Trig, v: &Obj, radius: i32, door_y: i32, extra: i32) -> IVec3 {
    let (s, c) = (trig.sin(v.heading), trig.cos(v.heading));
    let lat = -(radius + extra);
    // Model +X = (cos h, -sin h); model +Y = (sin h, cos h).
    v.pos + IVec3::new((c * lat + s * door_y) >> 8, (-s * lat + c * door_y) >> 8, 0)
}

/// A point on a vehicle in model coordinates: `lat` along model +X (the
/// car's left), `fwd` along +Y.
pub(crate) fn car_point(trig: &Trig, v: &Obj, lat: i32, fwd: i32) -> IVec3 {
    let (s, c) = (trig.sin(v.heading), trig.cos(v.heading));
    v.pos + IVec3::new((c * lat + s * fwd) >> 8, (-s * lat + c * fwd) >> 8, 0)
}

/// Where a person leaving a vehicle stands (Mac `GetOutVehicle` @ 10065660
/// and `GetOutOfVehicle` @ 10065c90; OS4 `FUN_01052398` / `FUN_01052714`):
/// `k/5·(192 + maxX)` out of the right (door) side, the left for negative
/// `k`, at the door's longitudinal position. Exits use k = 6 (1.2·L), the
/// boat's water fallback ±8 (1.6·L).
pub(crate) fn side_point(trig: &Trig, v: &Obj, max_x: i32, door_y: i32, k: i32) -> IVec3 {
    let (s, c) = (trig.sin(v.heading), trig.cos(v.heading));
    let l = (PERSON_RADIUS + max_x) * k;
    v.pos + IVec3::new(-c * l / 0x500 + (door_y * s >> 8), s * l / 0x500 + (door_y * c >> 8), 0)
}

/// Walk-in waypoint `wp` on a vehicle (Mac `GetInOutVehicles` @ 10066140):
/// 0/5 the bare door point (right side, `+0x48f4 = −maxX`, at `door_y`),
/// 1/2 the front-left/right corners (±r, +maxY), 3/4 the rear-left/right
/// corners (±r, −|minY|).
fn waypoint(trig: &Trig, v: &Obj, r: i32, front: i32, rear: i32, door_y: i32, wp: u8) -> IVec3 {
    let (lat, fwd) = match wp {
        1 => (r, front),
        2 => (-r, front),
        3 => (r, -rear),
        4 => (-r, -rear),
        _ => (-r, door_y),
    };
    let (s, c) = (trig.sin(v.heading), trig.cos(v.heading));
    v.pos + IVec3::new((lat * c >> 8) + (fwd * s >> 8), (-lat * s >> 8) + (fwd * c >> 8), 0)
}

/// Squared distance in >>5 units, as the walk-in code measures it.
fn dist2_32(a: IVec3, b: IVec3) -> i32 {
    let (dx, dy) = ((a.x - b.x) >> 5, (a.y - b.y) >> 5);
    dx * dx + dy * dy
}

/// Longitudinal door position (`0x48f8`): maxY/2, overridden per type.
pub(crate) fn door_y(kind: usize, max_y: i32) -> i32 {
    match kind {
        vk::BUS | 19 => max_y * 15 / 16,
        vk::BOMB_TRUCK => max_y * 7 / 8,
        4 | 27 => max_y / 3,
        8 => max_y * 2 / 3,
        7 | vk::POLICE_VAN => max_y * 3 / 4,
        14 | 15 => max_y * 5 / 6,
        vk::ROCKET_CAR | 25 | 26 => max_y / 6,
        _ => max_y / 2,
    }
}

/// In/out pressed: pick a vehicle and walk to it (a boat is boarded on the
/// spot), or get out (Mac `GetInOutVehicles` @ 10066140, OS4
/// `FUN_01052a4c`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn use_pressed(
    mut commands: Commands,
    data: Res<GameData>,
    trig: Res<Trig>,
    city: Option<Res<City>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut session: ResMut<PlayerSession>,
    on_foot: Query<
        (Entity, &Obj, &Stick, Option<&Boarding>),
        (With<CameraTarget>, With<Person>, Without<Busted>),
    >,
    mut set: ParamSet<(
        // Every vehicle, including other players' cars (multiplayer
        // hijack; the candidates are all objects but self, `FUN_01052a4c`).
        Query<(Entity, &Obj, &Vehicle, Option<&Ai>), Without<Person>>,
        Query<
            (
                Entity,
                &mut Obj,
                &mut Vehicle,
                &mut Drive,
                &Stick,
                Option<&Arsenal>,
                Option<&PullOut>,
                Has<HeliExit>,
            ),
            (With<CameraTarget>, Without<Person>),
        >,
    )>,
) {
    let Some(mut library) = library else { return };
    // On foot: choose a vehicle (nearest door point, no range limit).
    for e__ in crate::sim::table_order(on_foot.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((pe, p, stick, boarding)) = on_foot.get(e__) else { continue };
        if !stick.use_vehicle || p.state != State::Player {
            continue;
        }
        if boarding.is_some() {
            commands.entity(pe).remove::<Boarding>();
            continue;
        }
        let vehicles = set.p0();
        // Candidates: not burning, not in water unless a boat, and not in
        // state 0x14 (route flee) or, unless police, 0xf (goto). The door
        // point is 192 (the person's radius) inside the right side: lateral
        // `+0x48f4 + person.0x48c4` = −maxX + 192.
        let best = vehicles
            .iter()
            .filter(|(_, v, veh, ai)| {
                let order = ai.filter(|_| v.state == State::Idle).map(|a| a.order);
                !matches!(v.state, State::Dead | State::Removed)
                    && v.burn == 0
                    && (!v.in_water || veh.kind == vk::BOAT)
                    && !matches!(order, Some(Order::FleeFar))
                    && !(matches!(order, Some(Order::Goto(..))) && !TYPES[veh.kind].police)
            })
            .min_by_key(|(_, v, veh, _)| {
                let info = &library.model(v.model).info;
                let d = door_point(
                    &trig,
                    v,
                    info.radius,
                    door_y(veh.kind, info.max_y),
                    -PERSON_RADIUS,
                ) - p.pos;
                // Ties: the earlier object in the table.
                (((d.x >> 5) as i64).pow(2) + ((d.y >> 5) as i64).pow(2), v.slot)
            })
            .map(|(e, v, veh, _)| (e, v.pos, veh.kind, library.model(v.model).info.radius));
        commands.entity(pe).remove::<Touched>();
        let Some((ve, vpos, kind, r)) = best else {
            continue;
        };
        if kind == vk::BOAT {
            // A boat is boarded at once within r/8 (>>5 units, i.e. 4·r)
            // of its centre and 0xc00 in height; otherwise nothing happens.
            let rr = r >> 3;
            if dist2_32(vpos, p.pos) < rr * rr && (vpos.z - p.pos.z).abs() < 0xc00 {
                commands.entity(pe).insert(Boarding {
                    vehicle: ve,
                    entering: true,
                    waypoint: 0,
                });
            }
            continue;
        }
        commands.entity(pe).insert(Boarding {
            vehicle: ve,
            entering: false,
            waypoint: 5,
        });
    }
    // Driving: get out, at any speed. A cop who has opened the door pulls
    // the player out (`PullOut`, police.rs).
    let order = crate::sim::table_order(set.p1().iter().map(|(e, o, ..)| (e, o)));
    let mut cars = set.p1();
    for e__ in order {
        let Ok((ve, mut v, mut veh, mut drive, stick, ars, pull, armed)) = cars.get_mut(e__) else { continue };
        let forced = pull.is_some();
        if forced {
            commands.entity(ve).remove::<PullOut>();
        }
        if v.state != State::Player || !(forced || stick.use_vehicle) {
            continue;
        }
        if !forced && veh.kind == vk::HELICOPTER {
            // Three presses (`GetInOutVehicles`): engine on (+0x984, takes
            // off); engine off and exit armed (+0x986); get out. No speed
            // or height check, so a mid-air bail-out is possible.
            if veh.flying {
                veh.flying = false;
                commands.entity(ve).insert(HeliExit);
                continue;
            }
            if !armed {
                veh.flying = true;
                continue;
            }
        }
        commands.entity(ve).remove::<HeliExit>();
        let Some(pe) = eject(
            &mut commands,
            &data,
            &trig,
            city.as_deref(),
            &mut library,
            &mut meshes,
            &mut session,
            ve,
            &mut v,
            &veh,
            &mut drive,
            ars,
        ) else {
            continue;
        };
        if forced {
            // Busted while driving (68k ~121990): the exit `FUN_000dab40`
            // makes the player a person again, then the bust freezes him.
            info!("busted: pulled out of {ve:?} at {:?} (car parked, door open)", v.pos);
            commands.entity(pe).insert(Busted);
            commands.queue(move |world: &mut World| {
                if let Some(mut o) = world.get_mut::<Obj>(pe) {
                    o.state = State::Dead;
                }
            });
        }
    }
}

/// Top face of the cube at `p` on `level` is water (cube byte +9 & 0x7f
/// == 4). Off the map counts as dry.
fn water_at(city: &City, p: IVec3, level: usize) -> bool {
    let (x, y) = (p.x >> 11, p.y >> 11);
    (0..MAP_SIZE as i32).contains(&x)
        && (0..MAP_SIZE as i32).contains(&y)
        && city.map.square(x as usize, y as usize).cubes[level.min(2)].faces[map::TOP].kind
            == FaceType::Reflect
}

/// A driving player gets out (Mac `GetOutVehicle` @ 10065660, OS4
/// `FUN_01052398`, 68k `FUN_000dab40`): a person appears 1.2·(192 + maxX)
/// out of the right side at the door, facing away from the car, with the
/// car's weapons, score, wanted level and the stored on-foot health; the
/// car is parked with its door fully open. A boat whose exit point is on
/// water tries the left side at 1.6·L (facing the other way), then the
/// right at 1.6·L; with water on both there is no exit (`None`). Returns
/// the new person.
#[allow(clippy::too_many_arguments)]
pub(crate) fn eject(
    commands: &mut Commands,
    data: &GameData,
    trig: &Trig,
    city: Option<&City>,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    session: &mut PlayerSession,
    ve: Entity,
    v: &mut Obj,
    veh: &Vehicle,
    drive: &mut Drive,
    ars: Option<&Arsenal>,
) -> Option<Entity> {
    let info = library.model(v.model).info.clone();
    let dy = door_y(veh.kind, info.max_y);
    let mut exit = side_point(trig, v, info.radius, dy, 6);
    let mut heading = wrap(v.heading - 0x5a0);
    let boat = veh.kind == vk::BOAT;
    if let (true, Some(city)) = (boat, city) {
        if water_at(city, exit, v.level) {
            exit = side_point(trig, v, info.radius, dy, -8);
            if water_at(city, exit, v.level) {
                exit = side_point(trig, v, info.radius, dy, 8);
                if water_at(city, exit, v.level) {
                    return None;
                }
            } else {
                heading = wrap(heading + 0xb40);
            }
        }
    }
    exit.z = v.pos.z;
    let ars = ars.cloned().unwrap_or_default();
    let model = ars.current.person_model();
    let pe = people::spawn_person(commands, data, library, meshes, model, exit, heading, State::Player)?;
    let player = v.player.unwrap_or(0);
    let (score, wanted, health) = (v.score, v.wanted, *session.foot_health_mut(v.player));
    // The first collision step tests car centre -> exit point, so walls
    // between pull the player back (Update 4 fix). Boats start from the
    // exit point.
    let prev = if boat { exit } else { v.pos };
    commands.entity(pe).insert((CameraTarget, ars));
    commands.queue(move |world: &mut World| {
        if let Some(mut o) = world.get_mut::<Obj>(pe) {
            o.score = score;
            o.wanted = wanted;
            o.health = health;
            o.player = Some(player);
            o.prev_pos = prev;
        }
        if let Some(mut p) = world.get_mut::<Person>(pe) {
            p.follows_weapon = true;
        }
    });
    commands.entity(ve).remove::<(CameraTarget, Arsenal)>();
    v.state = State::Parked;
    v.player = None;
    v.speed = 0;
    drive.door = DOOR_OPEN;
    Some(pe)
}

/// Walk to the driver's door by waypoints, open it, get in (and hijack a
/// driven car) (Mac `GetInOutVehicles` @ 10066140, OS4 `FUN_01052a4c`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn walk_to_door(
    mut commands: Commands,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    data: Res<GameData>,
    city: Option<Res<City>>,
    texts: Option<Res<Texts>>,
    mut banner: ResMut<Banner>,
    timer: Option<ResMut<WantedTimer>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut session: ResMut<PlayerSession>,
    mut walkers: Query<
        (Entity, &mut Obj, &mut Stick, &mut Boarding, Option<&Arsenal>, Has<Touched>),
        (With<Person>, Without<Vehicle>),
    >,
    mut vehicles: Query<
        (&mut Obj, &Vehicle, &mut Drive, Option<&Arsenal>, Has<AiDriver>),
        Without<Person>,
    >,
) {
    let Some(mut library) = library else { return };
    let mut timer = timer;
    for e__ in crate::sim::table_order(walkers.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((pe, mut p, mut stick, mut b, ars, touched)) = walkers.get_mut(e__) else { continue };
        let Ok((mut v, veh, mut drive, v_ars, ai_driver)) = vehicles.get_mut(b.vehicle) else {
            commands.entity(pe).remove::<Boarding>();
            continue;
        };
        let info = library.model(v.model).info.clone();
        let dy = door_y(veh.kind, info.max_y);
        if !b.entering {
            // Arrival: within person.r/12 in >>5 units (512) of the
            // waypoint and 0x800 in height. Door (0, 5) -> get in; corners
            // 1 -> 2 -> door, 3 -> 4 -> door.
            let rr = PERSON_RADIUS / 12;
            let at = |wp| waypoint(&trig, &v, info.radius, info.max_y, info.min_y, dy, wp);
            if dist2_32(p.pos, at(b.waypoint)) < rr * rr && (v.pos.z - p.pos.z).abs() < 0x800 {
                match b.waypoint {
                    1 => b.waypoint = 2,
                    2 | 4 => b.waypoint = 0,
                    3 => b.waypoint = 4,
                    _ => b.entering = true,
                }
            }
            // Waypoint 5: on the first contact with a vehicle, head for the
            // nearest of the door and the four corners.
            if b.waypoint == 5 && touched {
                commands.entity(pe).remove::<Touched>();
                b.waypoint = (0..5).min_by_key(|&wp| dist2_32(p.pos, at(wp))).unwrap_or(0);
            }
            if !b.entering {
                // Auto-steer to the waypoint, walking once facing it.
                let d = at(b.waypoint) - p.pos;
                let want = heading_of(d.x, d.y);
                let err = angle_diff(p.heading, want);
                *stick = Stick {
                    steer: if err > 0 {
                        -1
                    } else if err < 0 {
                        1
                    } else {
                        0
                    },
                    throttle: if err.abs() < 200 { -1 } else { 0 },
                    ..default()
                };
            } else {
                *stick = Stick::default();
                p.speed = 0;
            }
            continue;
        }
        *stick = Stick::default();
        p.speed = 0;
        let boat = veh.kind == vk::BOAT;
        if !boat {
            // Open the door 100 per tick; get in when fully open and not
            // burning (a boat is boarded on the spot).
            drive.door = (drive.door + 100).min(DOOR_OPEN);
            if drive.door < DOOR_OPEN || p.burn > 0 {
                continue;
            }
        }
        let t = &TYPES[veh.kind];
        let mut hijack = false;
        if v.state == State::Player {
            // Multiplayer: another player is driving (OS4 `FUN_01052a4c`,
            // `DAT_0114c7c8 != 0` and state 2). He is put out by the normal
            // exit `FUN_01052398` and keeps playing on foot, knocked down
            // for 50 ticks (+0x494 = 0x32). No wanted level, score or
            // carjack count. A helicopter's exit arming is cleared.
            if p.player == v.player {
                continue;
            }
            commands.entity(b.vehicle).remove::<HeliExit>();
            let v_ars = v_ars.cloned();
            if let Some(victim) = eject(
                &mut commands,
                &data,
                &trig,
                city.as_deref(),
                &mut library,
                &mut meshes,
                &mut session,
                b.vehicle,
                &mut v,
                veh,
                &mut drive,
                v_ars.as_ref(),
            ) {
                commands.queue(move |world: &mut World| {
                    if let Some(mut pp) = world.get_mut::<Person>(victim) {
                        pp.stun = 50;
                    }
                });
            }
        } else if ai_driver || v.state != State::Parked {
            // Hijack (+0x824 AI driver set): the wanted level rises to at
            // least 3 (police) / 2, resetting the wanted timer, and the
            // score gains 1500 / 1000 × multiplier.
            hijack = true;
            let police = t.police;
            let floor = if police { 3 } else { 2 };
            if v.wanted <= floor {
                if let Some(timer) = timer.as_deref_mut() {
                    timer.0 = 0;
                }
                v.wanted = p.wanted.max(floor);
            }
            v.score = p.score + if police { 1500 } else { 1000 } * session.multiplier;
            if v.state != State::Parked {
                // A driven car throws its driver out; a stopped one
                // (state 5, e.g. a police car whose cop got out) has no one
                // to eject. `GetOutOfVehicle` alternates sides with
                // `DAT_0114c628`, but the hijack sets it to 1 first, so the
                // driver always lands on the right (door) side, 1.2·L out,
                // starting from the car centre for the collision step.
                let kind = if police { super::police::COP } else { 6 };
                let out = side_point(&trig, &v, info.radius, dy, 6);
                let centre = v.pos;
                if let Some(driver) = people::spawn_person(
                    &mut commands,
                    &data,
                    &mut library,
                    &mut meshes,
                    kind,
                    out,
                    wrap(v.heading - 0x5a0),
                    State::Wander,
                ) {
                    if police {
                        // PoliceMale1 with a shotgun and 100 shells.
                        let mut a = Arsenal::default();
                        a.current = Weapon::Shotgun;
                        a.ammo[Weapon::Shotgun as usize] = 100;
                        commands.entity(driver).insert(a);
                    }
                    let car = b.vehicle;
                    commands.queue(move |world: &mut World| {
                        if let Some(mut o) = world.get_mut::<Obj>(driver) {
                            o.prev_pos = centre;
                            // The car it came out of (`GetOutOfVehicle` @
                            // 10065c90 writes `+0x820`): the Irony bonus.
                            o.owner = Some(car);
                        }
                        if let Some(mut pp) = world.get_mut::<Person>(driver) {
                            pp.stun = 50;
                        }
                    });
                }
                let at = SoundPos::At(v.posf());
                commands.spawn(Sound::once(id::OW1 + rng.rand(3) as u16, 5e6, 8000.0, at));
                if veh.kind == vk::BUS {
                    commands.spawn(Sound::once(
                        id::SCREAMS,
                        255.0,
                        15000.0 + rng.rand(2000) as f32,
                        SoundPos::Global,
                    ));
                }
            }
        }
        // The vehicle becomes the player; not a hijack: it takes the
        // person's wanted level and score.
        if !hijack {
            v.score = p.score;
            v.wanted = p.wanted;
        }
        *session.foot_health_mut(p.player) = p.health;
        v.state = State::Player;
        v.player = Some(p.player.unwrap_or(0));
        if let Some(t) = &texts {
            banner.show_for(v.player, t.global(TYPES[veh.kind].name_phrase));
        }
        let arsenal = ars.cloned().unwrap_or_default();
        commands.entity(b.vehicle).insert((CameraTarget, arsenal)).remove::<AiDriver>();
        commands.entity(pe).despawn();
    }
}

/// Doors shut 50 per tick and slam while the vehicle is driven: by the
/// player (state 2), traffic (3), pursuit (8-11), script drives (0xf,
/// 0x15, route flee 0x14) and a police car at a roadblock (0xc) (Mac
/// `do_Other` @ 10146280).
fn close_doors(
    mut commands: Commands,
    mut q: Query<(&Obj, &Vehicle, &mut Drive, Option<&Ai>)>,
) {
    for (v, veh, mut d, ai) in &mut q {
        let scripted = v.state == State::Idle
            && ai.is_some_and(|a| {
                matches!(a.order, Order::Goto(..) | Order::Goto15(_) | Order::FleeFar)
            });
        let driven = matches!(
            v.state,
            State::Player | State::Wander | State::PursuitRoute | State::Pursuit
        ) || scripted
            || (v.state == State::Barrier && TYPES[veh.kind].police);
        if d.door > 0 && driven {
            d.door = (d.door - 50).max(0);
            if d.door == 0 {
                commands.spawn(Sound::once(
                    id::DOORSLAM,
                    5e6,
                    20500.0,
                    SoundPos::At(v.posf()),
                ));
            }
        }
    }
}
