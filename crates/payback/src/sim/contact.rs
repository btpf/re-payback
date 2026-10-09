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
use crate::vehicles::{Vehicle, kind as vk};
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
    radius: i32,
    front: i32,
    rear: i32,
    mass: i32,
    vehicle: bool,
    person: bool,
    /// Bus (`+0x818c`): its passengers scream on a hard hit.
    bus: bool,
    player: bool,
    /// Player number (kill credit in multiplayer).
    player_no: Option<u8>,
    /// Damage filter (`+0x8ac`): contacts hurt only when -1.
    filter: i32,
    /// The vehicle this person was thrown out of (Irony bonus).
    owner: Option<Entity>,
}

/// One touching pair of the current object `i` and an earlier `j`: the
/// averaged push (x, y, and z doubled) and each side's damage.
struct Touch {
    j: usize,
    push: IVec3,
    dmg_j: i32,
    dmg_i: i32,
}

/// Object-object contact (Mac `CheckForCollisions` @ 1006a150, OS4
/// `FUN_01056c58`, 68k `FUN_000de0b6`), one tick per frame. Each object
/// `i` in table order against every earlier `j`, from their current
/// positions (pushes from earlier pairs included):
/// - circles at `{0, hw − rear, front − hw, 2·front/3 − hw}` along the
///   heading for vehicles, one for anything else; contact below
///   `R = (hwA + hwB)·17 >> 4` in 3D with the z difference halved (pairs
///   20000 or more apart in that are skipped, and so are circle pairs 25001
///   or more apart on an axis); per touching circle pair the push `d·R/dist
///   − d` (integer, `dist = (int)sqrt`, at least 1), averaged;
/// - two vehicles, one of them faster than 50: a spark per touching circle
///   pair (type 9 `Flare`, between the circles ± 200, flying with the
///   mass-weighted velocity, size 100 + r(50), j's level + 1);
/// - j gets `+push·min(m)/m_j` on its velocity (z too) and i `−push·min(m)/
///   m_i`, a vehicle never by a person; the same goes on the positions
///   (the original moves its predicted position);
/// - damage `((px² + py² + (pz/2)²) >> 7)·min(m)/m_victim >> s`, s = 5 for
///   a vehicle, 3 for the player on foot, 1 for other people, only with
///   the damage filter −1.
///
/// Then per touching pair: two vehicles play Thud/Thud2 for each by its
/// own damage, the volume from the push (a hard hit dents it; a bus
/// driven by a player screams); both speeds are damped by `(8·s +
/// s·m_self/(m_i + m_j)) / 9`; and each victim takes its damage, a person
/// only from a vehicle (never person-person) faster than 50 (100 for the
/// player): it stops, says Ow above 10 damage and bleeds (a type-8 splat ±
/// 100, size `16·min(dmg, 50) + 50 + r(50)` for j, `100 + r(150)` for i,
/// and 3 type-9 droplets).
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
    mut labels: ResMut<crate::hud::labels::Labels>,
    mode: Res<crate::level::GameMode>,
    mut q: Query<
        (Entity, &mut Obj, Option<&Vehicle>, Has<Person>, Has<CameraTarget>, Option<&mut Drive>),
        Without<Projectile>,
    >,
) {
    let Some(library) = library else { return };
    let mut irony = Vec::new();
    // Pairs in object-table order (each object against the earlier ones).
    // The RC car (`+0x8160`) and state-6 objects take no part.
    let order = crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, &*o)));
    let bodies: Vec<Body> = q
        .iter_many(&order)
        .filter(|(_, o, v, ..)| o.state != State::Removed && v.is_none_or(|v| v.kind != vk::RC_CAR))
        .map(|(e, o, vehicle, person, player, _)| {
            let info = &library.model(o.model).info;
            Body {
                e,
                radius: if person { 192 } else { info.radius },
                front: info.max_y,
                rear: info.min_y,
                mass: info.mass.max(1),
                vehicle: vehicle.is_some(),
                person,
                bus: vehicle.is_some_and(|v| v.kind == vk::BUS),
                player,
                player_no: if player { Some(o.player.unwrap_or(0)) } else { None },
                filter: o.filter,
                owner: o.owner,
            }
        })
        .collect();
    let screams = |commands: &mut Commands, rng: &mut Rng| {
        let pitch = 15000 + rng.rand(2000);
        commands.spawn(Sound::once(id::SCREAMS, 255.0, pitch as f32, SoundPos::Global));
    };
    for i in 0..bodies.len() {
        let a = &bodies[i];
        let mut touches: Vec<Touch> = Vec::new();
        for j in 0..i {
            let b = &bodies[j];
            let Ok([(_, mut oa, _, _, _, mut da), (_, mut ob, _, _, _, mut db)]) = q.get_many_mut([a.e, b.e])
            else {
                continue;
            };
            let dzh = (ob.pos.z - oa.pos.z) >> 1;
            if dzh.abs() >= 20000 {
                continue;
            }
            let r = (a.radius + b.radius) * 17 >> 4;
            let r2 = r as i64 * r as i64;
            let ca = circles(&trig, &oa, a.vehicle, a.radius, a.front, a.rear);
            let cb = circles(&trig, &ob, b.vehicle, b.radius, b.front, b.rear);
            let sparks = a.vehicle && b.vehicle && (ob.speed.abs() > 50 || oa.speed.abs() > 50);
            let (mut push, mut n) = (IVec3::ZERO, 0);
            for c in &ca {
                for d in &cb {
                    let (dx, dy) = (d.x - c.x, d.y - c.y);
                    if dx.abs() >= 25001 || dy.abs() >= 25001 {
                        continue;
                    }
                    let d2 = dx as i64 * dx as i64 + dy as i64 * dy as i64 + dzh as i64 * dzh as i64;
                    if d2 >= r2 {
                        continue;
                    }
                    let dist = ((d2 as f64).sqrt() as i32).max(1);
                    n += 1;
                    push.z += (dzh * r / dist - dzh) * 2;
                    push.x += dx * r / dist - dx;
                    push.y += dy * r / dist - dy;
                    if sparks {
                        let (ma, mb) = (a.mass, b.mass);
                        let size = 100 + rng.rand(50);
                        let vz = rng.rand(80) - 40 - ((oa.vel.z + ob.vel.z) >> 1);
                        let vy = rng.rand(80) - 40 - (ob.vel.y * mb + oa.vel.y * ma) / (ma + mb);
                        let vx = rng.rand(80) - 40 - (ob.vel.x * mb + oa.vel.x * ma) / (ma + mb);
                        let z = oa.pos.z + (dzh >> 1) + rng.rand(400) - 200;
                        let y = c.y + (dy >> 1) + rng.rand(400) - 200;
                        let x = c.x + (dx >> 1) + rng.rand(400) - 200;
                        effects.add(
                            9,
                            Tex::Flare,
                            Vec3::new(x as f32, y as f32, z as f32),
                            Vec3::new(vx as f32, vy as f32, vz as f32),
                            size as f32,
                            ob.level as u8 + 1,
                            Blend::Add,
                        );
                    }
                }
            }
            if n == 0 {
                continue;
            }
            // Touch flag (OS4 +0x4a4, Mac +0x98c) for the walk-to-door
            // re-plan (`boarding.rs`); only players walk to cars.
            for (x, y) in [(a, b), (b, a)] {
                if x.person && x.player && y.vehicle {
                    commands.entity(x.e).insert(super::boarding::Touched);
                }
            }
            let p = push / n;
            let m = a.mass.min(b.mass);
            // A push is not a teleport: the car keeps its last clear
            // position for the wall test (`+0x8f0`, never written here).
            let shift = |d: &mut Option<Mut<Drive>>, dv: IVec3| {
                if let Some((_, left)) = d.as_deref_mut().and_then(|d| d.clear.as_mut()) {
                    *left += dv;
                }
            };
            if !(b.vehicle && a.person) {
                let dv = p * m / b.mass;
                ob.vel += dv;
                ob.pos += dv;
                shift(&mut db, dv);
            }
            if !(a.vehicle && b.person) {
                let dv = -p * m / a.mass;
                oa.vel += dv;
                oa.pos += dv;
                shift(&mut da, dv);
            }
            // Damage from the push, z halved, for every victim with the
            // damage filter −1.
            let pz = p.z >> 1;
            let base = ((p.x as i64).pow(2) + (p.y as i64).pow(2) + (pz as i64).pow(2)) >> 7;
            let dmg = |v: &Body| {
                if v.filter != -1 {
                    return 0;
                }
                let shift = if v.person && v.player {
                    3
                } else if v.person {
                    1
                } else {
                    5
                };
                ((m as i64 * base / v.mass as i64) >> shift).min(i32::MAX as i64) as i32
            };
            touches.push(Touch { j, push: p, dmg_j: dmg(b), dmg_i: dmg(a) });
        }
        for t in touches {
            let b = &bodies[t.j];
            let Ok([(_, mut oa, _, _, _, da), (_, mut ob, _, _, _, db)]) = q.get_many_mut([a.e, b.e]) else { continue };
            // Two vehicles: Thud below 11 damage, else Thud2 (dented; a
            // bus driven by a player screams, j's then i's), for j, then i.
            if a.vehicle && b.vehicle {
                let vol = (t.push.x.abs() + t.push.y.abs()) as f32 * 100_000.0;
                let mut drives = [db, da];
                let hits = [(ob.posf(), t.dmg_j), (oa.posf(), t.dmg_i)];
                for (k, (pos, dmg)) in hits.into_iter().enumerate() {
                    let at = SoundPos::At(pos);
                    if dmg < 11 {
                        commands.spawn(Sound::once(id::THUD, vol, 15000.0, at));
                    } else {
                        commands.spawn(Sound::once(id::THUD2, vol, 10000.0, at));
                        if let Some(d) = drives[k].as_deref_mut() {
                            d.dented = true;
                        }
                        if b.bus && ob.state == State::Player {
                            screams(&mut commands, &mut rng);
                        }
                        if a.bus && oa.state == State::Player {
                            screams(&mut commands, &mut rng);
                        }
                    }
                }
            }
            // Both speeds damped by the share of the mass.
            let total = a.mass + b.mass;
            ob.speed = (ob.speed * 8 + ob.speed * b.mass / total) / 9;
            oa.speed = (oa.speed * 8 + oa.speed * a.mass / total) / 9;
            let (sa, sb) = (oa.speed, ob.speed);
            for (earlier, victim, other, o, other_speed, dmg) in
                [(true, b, a, &mut ob, sa, t.dmg_j), (false, a, b, &mut oa, sb, t.dmg_i)]
            {
                if victim.person {
                    // Run over only by a vehicle faster than 50 (100 for
                    // the player); never person-person.
                    let limit = if victim.player { 100 } else { 50 };
                    if other.person || other_speed.abs() <= limit {
                        continue;
                    }
                }
                let alive = o.health >= 1;
                // God mode (`crate::cheats`): no damage.
                let dmg = if o.god { 0 } else { dmg };
                o.health -= dmg;
                let at = SoundPos::At(o.posf());
                // "Irony bonus!": the player runs a person over with the car
                // that person was thrown out of (`+0x820` = the car's
                // serial `+0x980`): phrase 2027, 10000 × multiplier.
                if victim.person && alive && o.health < 1 && other.player && victim.owner == Some(other.e) {
                    irony.push((other.e, other.player_no));
                }
                if victim.person {
                    if dmg > 10 {
                        commands.spawn(Sound::once(id::OW1 + rng.rand(3) as u16, 5e6, 8000.0, at));
                    }
                    o.speed = 0;
                    if victim.player && o.health < 1 {
                        if let Some(t) = &texts {
                            title.show_for(o.player, t.global(2021));
                        }
                    }
                    blood(&mut effects, &mut rng, o, dmg, earlier);
                }
                if dmg > 0 {
                    hurt.write(Hurt { victim: victim.e, by_player: other.player, by_weapon: false, attacker: other.player_no });
                }
            }
        }
    }
    for (e, player) in irony {
        if let Ok((_, mut o, ..)) = q.get_mut(e) {
            o.score += 10_000 * session.multiplier;
            // Its label at the car (Mac `CheckForCollisions` @ 1006a150:
            // `AddLabelPercent(car pos, 2, mult·10000)`).
            labels.add_score(o.pos, 10_000 * session.multiplier, mode.is_multiplayer());
        }
        if let Some(t) = &texts {
            title.show_for(player, t.global(2027));
        }
    }
}

/// A run-over person bleeds (Mac `CheckForCollisions` @ 1006a150 at
/// 1006c184 / 1006c854): a type-8 `Blood` splat at ± 100 (size `16·min(dmg,
/// 50) + 50 + r(50)` for the earlier object of the pair, `100 + r(150)` for
/// the current one) on its level, then 3 type-9 droplets ± 100 flying at
/// `r(80) − 40 − v/4`, size `8·min + 80 + r(2·min)`, a level up; all tinted.
/// Drawn last argument first, as the original evaluates them.
fn blood(effects: &mut Effects, rng: &mut Rng, o: &Obj, dmg: i32, earlier: bool) {
    let m = dmg.clamp(0, 50);
    let size = if earlier { 16 * m + 50 + rng.rand(50) } else { 100 + rng.rand(150) };
    let y = o.pos.y + rng.rand(200) - 100;
    let x = o.pos.x + rng.rand(200) - 100;
    effects.add(
        8,
        Tex::Blood,
        Vec3::new(x as f32, y as f32, o.pos.z as f32),
        Vec3::ZERO,
        size as f32,
        o.level as u8,
        Blend::Tint,
    );
    for _ in 0..3 {
        let size = 8 * m + 80 + rng.rand(2 * m);
        let vz = rng.rand(80) - 40 - (o.vel.z >> 2);
        let vy = rng.rand(80) - 40 - (o.vel.y >> 2);
        let vx = rng.rand(80) - 40 - (o.vel.x >> 2);
        let z = o.pos.z + rng.rand(200) - 100;
        let y = o.pos.y + rng.rand(200) - 100;
        let x = o.pos.x + rng.rand(200) - 100;
        effects.add(
            9,
            Tex::Blood,
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new(vx as f32, vy as f32, vz as f32),
            size as f32,
            o.level as u8 + 1,
            Blend::Tint,
        );
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
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut o, _veh, is_player)) = q.get_mut(e__) else { continue };
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
