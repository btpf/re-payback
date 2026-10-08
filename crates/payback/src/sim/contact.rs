//! Object-object contact (OS4 `FUN_01056c58`, 68k `FUN_000de0b6`) and
//! vehicle destruction (`FUN_010542b8`, `FUN_01044b70` Ignite,
//! `FUN_0104465c` Destroy).
//!
//! People are one circle; vehicles four circles along their heading. Pairs
//! closer than `(rA + rB)·17/16` are pushed apart, exchange momentum by
//! the smaller mass, and take damage from the relative velocity.

use super::combat::{Explode, Projectile};
use super::people::{Person, PlayerSession};
use super::police::Hurt;
use super::{Obj, Rng, State, Trig};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::effects::{Blend, Effects, Tex};
use crate::hud::{BigTitle, Texts};
use crate::mission::{Triggers, kind};
use crate::objects::ObjectLibrary;
use crate::sim::driving::Drive;
use crate::vehicles::Vehicle;
use bevy::prelude::*;

pub struct ContactPlugin;

impl Plugin for ContactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (contacts, vehicle_damage)
                .chain()
                .after(super::driving::DrivingSet)
                .before(super::combat::CombatSet),
        );
    }
}

/// Collision circles (centre offsets along the heading) of an object.
fn circles(trig: &Trig, o: &Obj, is_vehicle: bool, radius: i32, front: i32, rear: i32) -> Vec<IVec3> {
    if !is_vehicle {
        return vec![o.pos];
    }
    let (s, c) = (trig.sin(o.heading), trig.cos(o.heading));
    [0, radius - rear, front - radius, 2 * front / 3 - radius]
        .into_iter()
        .map(|d| o.pos + IVec3::new(s * d >> 8, c * d >> 8, 0))
        .collect()
}

struct Body {
    e: Entity,
    circles: Vec<IVec3>,
    radius: i32,
    mass: i32,
    vehicle: bool,
    person: bool,
    player: bool,
    /// Player number (kill credit in multiplayer).
    player_no: Option<u8>,
    speed: i32,
    /// Damage filter (`+0x8ac`): contacts hurt only when -1.
    filter: i32,
    /// The vehicle this person was thrown out of (Irony bonus).
    owner: Option<Entity>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn contacts(
    mut commands: Commands,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    library: Option<Res<ObjectLibrary>>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut effects: ResMut<Effects>,
    mut hurt: MessageWriter<Hurt>,
    session: Res<PlayerSession>,
    mut q: Query<
        (Entity, &mut Obj, Has<Vehicle>, Has<Person>, Has<CameraTarget>, Option<&mut Drive>),
        Without<Projectile>,
    >,
) {
    let Some(library) = library else { return };
    let mut irony = Vec::new();
    let bodies: Vec<Body> = q
        .iter()
        .filter(|(_, o, ..)| o.state != State::Removed)
        .map(|(e, o, vehicle, person, player, _)| {
            let info = &library.model(o.model).info;
            let radius = if person { 192 } else { info.radius.max(64) };
            Body {
                e,
                circles: circles(&trig, &o, vehicle, radius, info.max_y, info.min_y),
                radius,
                mass: info.mass.max(1),
                vehicle,
                person,
                player,
                player_no: if player { Some(o.player.unwrap_or(0)) } else { None },
                speed: o.speed,
                filter: o.filter,
                owner: o.owner,
            }
        })
        .collect();
    for i in 0..bodies.len() {
        for j in 0..i {
            let (a, b) = (&bodies[i], &bodies[j]);
            // A person never pushes a car; two people only nudge.
            let r = ((a.radius + b.radius) * 17 / 16) as i64;
            let mut push = IVec3::ZERO;
            // The z correction, kept apart from the push (damage only):
            // `2·(dz/2·R/d − dz/2)` per circle pair.
            let mut push_z = 0i64;
            let mut contacts = 0;
            for ca in &a.circles {
                for cb in &b.circles {
                    let d = *cb - *ca;
                    if (d.z / 2).abs() >= 20000 {
                        continue;
                    }
                    let dist2 = (d.x as i64).pow(2) + (d.y as i64).pow(2) + ((d.z / 2) as i64).pow(2);
                    if dist2 >= r * r || dist2 == 0 {
                        continue;
                    }
                    let dist = (dist2 as f64).sqrt();
                    let k = r as f64 / dist - 1.0;
                    push += IVec3::new((d.x as f64 * k) as i32, (d.y as f64 * k) as i32, 0);
                    push_z += 2 * ((d.z / 2) as f64 * k) as i64;
                    contacts += 1;
                }
            }
            if contacts == 0 {
                continue;
            }
            // Touch flag (OS4 +0x4a4, Mac +0x98c) for the walk-to-door
            // re-plan (`boarding.rs`); only players walk to cars.
            for (x, y) in [(a, b), (b, a)] {
                if x.person && x.player && y.vehicle {
                    commands.entity(x.e).insert(super::boarding::Touched);
                }
            }
            let p = push / contacts;
            let m = a.mass.min(b.mass);
            let Ok([(_, mut oa, .., da), (_, mut ob, .., db)]) = q.get_many_mut([a.e, b.e]) else { continue };
            if !(a.person && b.vehicle) {
                ob.vel += p * m / b.mass;
                ob.pos += p * m / b.mass;
            }
            if !(b.person && a.vehicle) {
                oa.vel -= p * m / a.mass;
                oa.pos -= p * m / a.mass;
            }
            // Damage from the averaged push correction, z included (Mac
            // `CheckForCollisions` @ 1006a150, 68k `FUN_000de0b6`:
            // `((px² + py² + (pz/2)²) >> 7)·min(m)/m_self`, the z term
            // doubled per pair and halved here), for every victim with the
            // damage filter -1. A person is only hurt by a vehicle moving
            // faster than 50 (100 for the player), so a cop stepping out
            // beside his parked car is pushed clear, not crushed.
            let pz = push_z / contacts as i64 >> 1;
            let base = ((p.x as i64).pow(2) + (p.y as i64).pow(2) + pz * pz) >> 7;
            let base = base.min(i32::MAX as i64) as i32;
            for (victim, other, o, other_speed, drive) in [(b, a, &mut ob, a.speed, db), (a, b, &mut oa, b.speed, da)] {
                if victim.filter != -1 {
                    continue;
                }
                let shift = if victim.vehicle { 5 } else if victim.player { 3 } else { 1 };
                let mut dmg = (base as i64 * m as i64 / victim.mass as i64) as i32 >> shift;
                if victim.person {
                    // Run over only above speed 50 (100 for the player).
                    let limit = if victim.player { 100 } else { 50 };
                    if other.person || other_speed.abs() <= limit {
                        dmg = 0;
                    }
                }
                if dmg <= 0 {
                    continue;
                }
                let alive = o.health >= 1;
                o.health -= dmg;
                let at = SoundPos::At(o.posf());
                // "Irony bonus!": the player runs a person over with the car
                // that person was thrown out of (`+0x820` = the car's
                // serial `+0x980`): phrase 2027, 10000 × multiplier.
                if victim.person && alive && o.health < 1 && other.player && victim.owner == Some(other.e) {
                    irony.push((other.e, other.player_no));
                }
                if victim.person {
                    o.speed = 0;
                    if dmg > 10 {
                        commands.spawn(Sound::once(id::OW1 + rng.rand(3) as u16, 5e6, 8000.0, at));
                    }
                    let size = 16.0 * dmg.min(50) as f32 + 50.0 + rng.rand(50) as f32;
                    effects.add(8, Tex::Blood, o.posf(), Vec3::ZERO, size, o.level as u8 + 1, Blend::Tint);
                    if victim.player && o.health < 1 {
                        if let Some(t) = &texts {
                            title.show_for(o.player, t.global(2021));
                        }
                    }
                } else {
                    let vol = (p.x.abs() + p.y.abs()) as f32 * 100_000.0;
                    if dmg < 11 {
                        commands.spawn(Sound::once(id::THUD, vol, 15000.0, at));
                    } else {
                        commands.spawn(Sound::once(id::THUD2, vol, 10000.0, at));
                        if let Some(mut d) = drive {
                            d.dented = true;
                        }
                    }
                    if other.vehicle && (victim.speed.abs() > 50 || other_speed.abs() > 50) {
                        effects.add(9, Tex::Flare, o.posf(), Vec3::ZERO, 100.0 + rng.rand(50) as f32, o.level as u8 + 1, Blend::Add);
                    }
                }
                hurt.write(Hurt { victim: victim.e, by_player: other.player, by_weapon: false, attacker: other.player_no });
            }
        }
    }
    for (e, player) in irony {
        if let Ok((_, mut o, ..)) = q.get_mut(e) {
            o.score += 10_000 * session.multiplier;
        }
        if let Some(t) = &texts {
            title.show_for(player, t.global(2027));
        }
    }
}

/// Per-tick damage states of vehicles: smoke, explode, burn, blow apart.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn vehicle_damage(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    trig: Res<Trig>,
    library: Option<Res<ObjectLibrary>>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut effects: ResMut<Effects>,
    mut triggers: ResMut<Triggers>,
    mut session: ResMut<PlayerSession>,
    mut explode: MessageWriter<Explode>,
    mut killed: MessageWriter<super::score::Killed>,
    mut q: Query<(Entity, &mut Obj, &Vehicle, Has<CameraTarget>)>,
) {
    let Some(library) = library else { return };
    for (e, mut o, _veh, is_player) in &mut q {
        let info = &library.model(o.model).info;
        let (mass, radius) = (info.mass as i64, info.radius as f32);
        let layer = o.level as u8 + 1;
        let player = o.player;
        let say = |title: &mut BigTitle, n: u32| {
            if let Some(t) = &texts {
                title.show_for(player, t.global(n));
            }
        };
        // Blown apart.
        if o.health < -70 && o.state != State::Removed {
            // Destroyed outright, without burning first.
            if o.burn == 0 && !matches!(o.state, State::Dead | State::Parked) {
                killed.write(super::score::Killed { victim: e, police: info.police });
            }
            if is_player {
                say(&mut title, 2025);
                *session.foot_health_mut(o.player) = -71;
            }
            explode.write(Explode {
                pos: o.pos,
                power: mass / 2 * 20000,
                layer: o.level,
                source: Some(e),
                by_player: false,
                player: None,
            });
            triggers.report_actor(kind::ACTOR_KILLED, e);
            o.state = if is_player { State::Dead } else { State::Removed };
            if !is_player {
                // Removed by `despawn_far` next tick (after the crime and
                // the kill are counted).
                commands.entity(e).insert(Visibility::Hidden);
            }
            continue;
        }
        if o.burn < 2 {
            // Damaged: smoke from the bonnet, more often the lower the health.
            if (1..51).contains(&o.health) && rng.rand(o.health + 5) <= 5 {
                let (s, c) = (trig.sin(o.heading), trig.cos(o.heading));
                let ahead = (info.max_y - 300).max(0);
                let at = o.pos.as_vec3() + Vec3::new((s * ahead >> 8) as f32, (c * ahead >> 8) as f32, 0.0);
                let v = Vec3::new(
                    (o.vel.x / 2 + 30 + rng.rand(30)) as f32,
                    (o.vel.y / 2 + rng.rand(20) - 10) as f32,
                    (o.vel.z / 2 - 10) as f32,
                );
                effects.add(6, Tex::Dust2, at, v, radius + rng.rand(300) as f32, layer, Blend::Subtract);
            }
            // Wrecked: set alight, hop, explode (Ignite).
            if o.health < 1 && o.burn == 0 && o.state != State::Dead {
                // An occupied vehicle counts as a kill (parked ones don't).
                if o.state != State::Parked {
                    killed.write(super::score::Killed { victim: e, police: info.police });
                }
                o.burn = 100;
                o.speed = 0;
                o.vel += IVec3::new(rng.rand(100) - 50, rng.rand(100) - 50, -(200 + rng.rand(300)));
                if !is_player {
                    o.state = State::Dead;
                }
                let n = ((mass >> 10) + 2) as i32;
                for _ in 0..(n / 2).clamp(2, 6) {
                    let p = o.posf() + Vec3::new((rng.rand(1600) - 800) as f32, (rng.rand(1600) - 800) as f32, 0.0);
                    effects.add(7, Tex::Flame, p, Vec3::ZERO, 2500.0 + rng.rand(1500) as f32, 3, Blend::Add);
                    effects.add(2, Tex::Flame, p, Vec3::new(0.0, 0.0, -300.0), 800.0, 3, Blend::Add);
                }
                let mut at = o.pos;
                at.z -= 500;
                explode.write(Explode { pos: at, power: mass * 25000, layer: o.level, source: Some(e), by_player: false, player: None });
                triggers.report_actor(kind::ACTOR_KILLED, e);
            }
            continue;
        }
        // Burning: flames, then the wreck smoulders.
        if rng.rand(3) == 0 {
            let p = o.posf() + Vec3::new((rng.rand(300) - 150) as f32, (rng.rand(300) - 150) as f32, 0.0);
            let size = if o.burn >= 16 { 1.5 * radius + rng.rand(300) as f32 } else { radius + rng.rand(150) as f32 };
            effects.add(4, Tex::Flame, p, Vec3::new(0.0, 0.0, -600.0), size, layer, Blend::Add);
        }
        o.burn -= 1;
        if o.burn < 2 {
            if is_player {
                say(&mut title, 2024);
                o.state = State::Dead;
            }
            o.burn = 15;
        }
    }
}
