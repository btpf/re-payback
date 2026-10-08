//! Getting into and out of vehicles (docs/re/people.md §6; OS4
//! `FUN_01052a4c` enter, `FUN_01052398` exit, `FUN_01052714` hijack).

use super::combat::Arsenal;
use super::driving::Drive;
use super::people::{self, Person, PlayerSession};
use super::police::Busted;
use super::{Obj, Rng, State, Stick, Trig, angle_diff, heading_of, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::hud::{Banner, Texts};
use crate::objects::ObjectLibrary;
use crate::vehicles::{TYPES, Vehicle, kind as vk};
use bevy::prelude::*;

/// Fully open door angle (55°).
const DOOR_OPEN: i32 = 0x370;

/// The player walking to (stage 1) or climbing into (stage 2) a vehicle.
#[derive(Component, Clone, Copy, Debug)]
pub struct Boarding {
    pub vehicle: Entity,
    pub entering: bool,
}

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

/// In/out pressed: start walking to the nearest vehicle, or get out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn use_pressed(
    mut commands: Commands,
    data: Res<GameData>,
    trig: Res<Trig>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut session: ResMut<PlayerSession>,
    on_foot: Query<
        (Entity, &Obj, &Stick, Option<&Boarding>),
        (With<CameraTarget>, With<Person>, Without<Busted>),
    >,
    mut driving: Query<
        (
            Entity,
            &mut Obj,
            &Vehicle,
            &mut Drive,
            &Stick,
            Option<&Arsenal>,
        ),
        (With<CameraTarget>, Without<Person>),
    >,
    vehicles: Query<(Entity, &Obj, &Vehicle), Without<CameraTarget>>,
) {
    let Some(mut library) = library else { return };
    // On foot: choose a vehicle (nearest door point, no range limit).
    for (pe, p, stick, boarding) in &on_foot {
        if !stick.use_vehicle || p.state != State::Player {
            continue;
        }
        if boarding.is_some() {
            commands.entity(pe).remove::<Boarding>();
            continue;
        }
        let best = vehicles
            .iter()
            .filter(|(_, v, veh)| {
                !matches!(v.state, State::Dead | State::Removed)
                    && v.burn == 0
                    && (!v.in_water || veh.kind == vk::BOAT)
            })
            .min_by_key(|(_, v, veh)| {
                let info = &library.model(v.model).info;
                let d =
                    door_point(&trig, v, info.radius, door_y(veh.kind, info.max_y), 192) - p.pos;
                (d.x as i64).pow(2) + (d.y as i64).pow(2)
            });
        if let Some((ve, _, _)) = best {
            commands.entity(pe).insert(Boarding {
                vehicle: ve,
                entering: false,
            });
        }
    }
    // Driving: get out (helicopters must land first).
    for (ve, mut v, veh, mut drive, stick, ars) in &mut driving {
        if !stick.use_vehicle || v.state != State::Player || (veh.kind == vk::HELICOPTER && veh.flying)
        {
            continue;
        }
        let info = library.model(v.model).info.clone();
        let mut exit = door_point(
            &trig,
            &v,
            info.radius,
            door_y(veh.kind, info.max_y),
            192 * 6 / 5,
        );
        exit.z = v.pos.z;
        let heading = wrap(v.heading - 0x5a0);
        let ars = ars.cloned().unwrap_or_default();
        let model = ars.current.person_model();
        let Some(pe) = people::spawn_person(
            &mut commands,
            &data,
            &mut library,
            &mut meshes,
            model,
            exit,
            heading,
            State::Player,
        ) else {
            continue;
        };
        let player = v.player.unwrap_or(0);
        let (score, wanted, health) = (v.score, v.wanted, *session.foot_health_mut(v.player));
        let prev = v.pos;
        commands.entity(pe).insert((CameraTarget, ars));
        commands.queue(move |world: &mut World| {
            if let Some(mut o) = world.get_mut::<Obj>(pe) {
                o.score = score;
                o.wanted = wanted;
                o.health = health;
                o.player = Some(player);
                // The first collision step tests car centre -> exit point, so
                // walls between pull the player back (Update 4 fix).
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
    }
}

/// Walk to the driver's door, open it, get in (and hijack a driven car).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn walk_to_door(
    mut commands: Commands,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    data: Res<GameData>,
    texts: Option<Res<Texts>>,
    mut banner: ResMut<Banner>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut session: ResMut<PlayerSession>,
    mut walkers: Query<
        (Entity, &mut Obj, &mut Stick, &Boarding, Option<&Arsenal>),
        (With<Person>, Without<Vehicle>),
    >,
    mut vehicles: Query<(&mut Obj, &Vehicle, &mut Drive), Without<Person>>,
) {
    let Some(mut library) = library else { return };
    for (pe, mut p, mut stick, b, ars) in &mut walkers {
        let Ok((mut v, veh, mut drive)) = vehicles.get_mut(b.vehicle) else {
            commands.entity(pe).remove::<Boarding>();
            continue;
        };
        let info = library.model(v.model).info.clone();
        let door = door_point(&trig, &v, info.radius, door_y(veh.kind, info.max_y), 192);
        let d = door - p.pos;
        let arrived = d.x.abs() < 512 && d.y.abs() < 512 && (v.pos.z - p.pos.z).abs() <= 0x7ff;
        let boat_close = veh.kind == vk::BOAT && d.x.abs().max(d.y.abs()) < info.radius;
        if !(arrived || boat_close || b.entering) {
            // Auto-steer to the door, walking once facing it.
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
            continue;
        }
        *stick = Stick::default();
        p.speed = 0;
        if !b.entering {
            commands.entity(pe).insert(Boarding {
                vehicle: b.vehicle,
                entering: true,
            });
        }
        // Open the door 100 per tick; get in when fully open.
        drive.door = (drive.door + 100).min(DOOR_OPEN);
        if drive.door < DOOR_OPEN && veh.kind != vk::BOAT {
            continue;
        }
        let t = &TYPES[veh.kind];
        // Hijack: a driven car throws its driver out.
        if !matches!(v.state, State::Parked) {
            let police = t.police;
            p.wanted = p.wanted.max(if police { 3 } else { 2 });
            p.score += if police { 1500 } else { 1000 } * session.multiplier;
            let kind = if police { 7 } else { 6 };
            let side = if rng.rand(2) == 0 { 1 } else { -1 };
            let mut out = door_point(
                &trig,
                &v,
                info.radius,
                door_y(veh.kind, info.max_y),
                192 * 6 / 5,
            );
            out.x += side;
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
                commands.queue(move |world: &mut World| {
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
        // The vehicle becomes the player.
        *session.foot_health_mut(p.player) = p.health;
        v.state = State::Player;
        v.player = Some(p.player.unwrap_or(0));
        v.score = p.score;
        v.wanted = p.wanted;
        if let Some(t) = &texts {
            banner.show_for(v.player, t.global(TYPES[veh.kind].name_phrase));
        }
        let arsenal = ars.cloned().unwrap_or_default();
        commands.entity(b.vehicle).insert((CameraTarget, arsenal));
        commands.entity(pe).despawn();
    }
}

/// While the player drives, the door shuts 50 per tick and slams.
fn close_doors(mut commands: Commands, mut q: Query<(&Obj, &mut Drive)>) {
    for (v, mut d) in &mut q {
        if d.door > 0 && v.state == State::Player {
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
