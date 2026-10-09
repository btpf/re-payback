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
use crate::vehicles::{Prop, Vehicle, kind as vk, prop};
use bevy::prelude::*;

pub struct ContactPlugin;

impl Plugin for ContactPlugin {
    fn build(&self, app: &mut App) {
        // Contacts run first in the tick, before any object moves or
        // thinks (`amiga_levelloop` @ 10100f50 calls `CheckForCollisions`
        // @ 1006a150 before `MainLoop` @ 1014a920; AI-1).
        app.add_systems(
            FixedUpdate,
            (contacts, vehicle_damage)
                .chain()
                .in_set(ContactSet)
                .after(crate::replay::ReplayInput)
                .before(super::people::PeopleThink),
        );
    }
}

/// The contact pass, at the start of the tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContactSet;

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
    /// Owner side (`+0x824`, [`Obj::side`]): ram score and crime go from
    /// a side >= 0 object to a side −1 one.
    side: i32,
    /// Which prop (barrier, cone, dummy, bullet), if one.
    prop: Option<u8>,
    /// Already has the touch flag.
    touched: bool,
}

impl Body {
    fn is_prop(&self, k: u8) -> bool {
        self.prop == Some(k)
    }
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
/// and 3 type-9 droplets). A victim (not a prop) on the NPC side (owner
/// side −1) hit by a player-side object (side >= 0) pays the player the
/// ram score `mult · min(6000, v² >> 9)` and is reported (`Hurt`, the
/// original's `CheckHitObject(victim, 0)`) even at zero damage.
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
    mut crimes: Option<ResMut<super::police::Crimes>>,
    mut q: Query<
        (
            Entity,
            &mut Obj,
            Option<&Vehicle>,
            Has<Person>,
            Has<CameraTarget>,
            Option<&mut Drive>,
            Option<&Prop>,
            Has<super::boarding::Touched>,
        ),
        Without<Projectile>,
    >,
) {
    let Some(library) = library else { return };
    let mut irony = Vec::new();
    // Ram score: (paying player's object if the rammer is one, else the
    // single player), amount.
    let mut rams: Vec<(Option<Entity>, i32)> = Vec::new();
    // Pairs in object-table order (each object against the earlier ones).
    // The RC car (`+0x8160`) and state-6 objects take no part.
    let order = crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, &*o)));
    let bodies: Vec<Body> = q
        .iter_many(&order)
        .filter(|(_, o, v, ..)| o.state != State::Removed && v.is_none_or(|v| v.kind != vk::RC_CAR))
        .map(|(e, o, vehicle, person, player, drive, prop, touched)| {
            let info = &library.model(o.model).info;
            // A police van on its red light-bar frame is `policevan_red`
            // (mass 2000; `driving::light_bar`).
            let red = drive.is_some_and(|d| d.red_frame);
            Body {
                e,
                radius: if person { 192 } else { info.radius },
                front: info.max_y,
                rear: info.min_y,
                mass: if red { crate::vehicles::POLICEVAN_RED.mass } else { info.mass.max(1) },
                vehicle: vehicle.is_some(),
                person,
                bus: vehicle.is_some_and(|v| v.kind == vk::BUS),
                player,
                player_no: if player { Some(o.player.unwrap_or(0)) } else { None },
                filter: o.filter,
                owner: o.owner,
                side: o.side,
                prop: prop.map(|p| p.0),
                touched,
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
            let Ok([(_, mut oa, _, _, _, mut da, ..), (_, mut ob, _, _, _, mut db, ..)]) = q.get_many_mut([a.e, b.e])
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
            // Touch flag (OS4 +0x4a4, Mac +0x98c): set on both objects
            // whenever one is a vehicle (Mac `CheckForCollisions` @
            // 1006a150, `+0x98c = 1`). Only walkers read it (the walk-to-door
            // re-plan, `boarding.rs`), so only they get it, players or not.
            for (x, y) in [(a, b), (b, a)] {
                if x.person && y.vehicle && !x.touched {
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
            // The dummy takes no push and its partner takes all of it,
            // unscaled; the bullet prop exchanges none (only the barrier,
            // cone and dummy of the class-2 objects are pushed; Mac
            // `CheckForCollisions` @ 1006a150).
            let pushed = !a.is_prop(prop::BULLET) && !b.is_prop(prop::BULLET);
            if pushed && !b.is_prop(prop::DUMMY) && !(b.vehicle && a.person) {
                let dv = if a.is_prop(prop::DUMMY) { p } else { p * m / b.mass };
                ob.vel += dv;
                ob.pos += dv;
                shift(&mut db, dv);
            }
            if pushed && !a.is_prop(prop::DUMMY) && !(a.vehicle && b.person) {
                let dv = if b.is_prop(prop::DUMMY) { -p } else { -p * m / a.mass };
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
            let Ok([(_, mut oa, _, _, _, da, ..), (_, mut ob, _, _, _, db, ..)]) = q.get_many_mut([a.e, b.e]) else { continue };
            // Two vehicles: Thud below 11 damage, else Thud2 (dented; a
            // bus driven by a player screams, j's then i's), for j, then i.
            if a.vehicle && b.vehicle {
                // Shunt counter `+0x944` of both cars, every touching tick
                // (Mac `CheckForCollisions` @ 1006a150); the player's car
                // carries the player's record (copied in and out with the
                // player), here [`super::police::Crimes`].
                if let Some(c) = crimes.as_deref_mut() {
                    c.shunt += a.player as i32 + b.player as i32;
                }
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
                // Ram score and the hit report (Mac `CheckForCollisions` @
                // 1006a150, after each victim's damage): from a player-side
                // object (owner side >= 0) on an NPC (−1), `mult ·
                // min(6000, v² >> 9)` with v the larger of |px|, |py| ·
                // min(m)/m_victim, and `CheckHitObject(victim, 0)` whatever
                // the damage (CB-3, CB-4). Props (class 2) take no part.
                // A surviving vehicle counts as a crime only when the
                // player's own object is a vehicle (`police::crimes`).
                let check = other.side != -1 && victim.side == -1 && victim.prop.is_none();
                if check {
                    let m = a.mass.min(b.mass) as i64;
                    let v = (t.push.x as i64 * m / victim.mass as i64)
                        .abs()
                        .max((t.push.y as i64 * m / victim.mass as i64).abs());
                    let pay = ((v * v) >> 9).min(6000) as i32 * session.multiplier;
                    rams.push((other.player.then_some(other.e), pay));
                }
                if check || dmg > 0 {
                    // Crime only from the check; between players (both
                    // side 0) the kill credit in multiplayer.
                    let by_player = check || (other.player && victim.player);
                    hurt.write(Hurt { victim: victim.e, by_player, by_weapon: false, attacker: other.player_no });
                }
            }
            // A barrier or cone hit by anything is knocked over (roll
            // 0x5a0, on its side) with HitScenery (30, 2e6 @ 8500, at the
            // current object i) and an extra quarter of the push on its
            // velocity, away from the other; the dummy only makes the sound
            // (the second test reads i again, sic). Mac `CheckForCollisions`
            // @ 1006a150, the class-2 tails for i then j.
            let topple = |o: &mut Obj, dv: IVec3| {
                o.vel += dv;
                o.roll = 0x5a0;
            };
            let thump = |commands: &mut Commands, pos: Vec3| {
                commands.spawn(Sound::once(id::HIT_SCENERY, 2e6, 8500.0, SoundPos::At(pos)));
            };
            let quarter = t.push / 4;
            if a.prop.is_some() {
                if a.is_prop(prop::BARRIER) || a.is_prop(prop::CONE) {
                    topple(&mut oa, -quarter);
                    thump(&mut commands, oa.posf());
                }
                if a.is_prop(prop::DUMMY) {
                    thump(&mut commands, oa.posf());
                }
            }
            if b.prop.is_some() {
                if b.is_prop(prop::BARRIER) || b.is_prop(prop::CONE) {
                    topple(&mut ob, quarter);
                    thump(&mut commands, oa.posf());
                }
                if a.is_prop(prop::DUMMY) {
                    thump(&mut commands, oa.posf());
                }
            }
        }
    }
    // The score goes to the player's object (`+0x864` of
    // `objects[DAT_1017c60c]`); an empty player-side car pays the single
    // player. No label.
    for (payee, pay) in rams {
        // Bodies are in table order: the first player is the lowest slot.
        let single = (!mode.is_multiplayer()).then(|| bodies.iter().find(|b| b.player).map(|b| b.e));
        let payee = payee.or(single.flatten());
        if let Some(Ok((_, mut o, ..))) = payee.map(|e| q.get_mut(e)) {
            o.score += pay;
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
    mut rc_cars: ResMut<super::rc_car::RcCars>,
    mut q: Query<(Entity, &mut Obj, &Vehicle, Has<CameraTarget>, Option<&super::rc_car::RcCar>)>,
) {
    let Some(library) = library else { return };
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut o, _veh, is_player, rc)) = q.get_mut(e__) else { continue };
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
                // `Detonate(i, 0)` -> `Shockwave(.., i)`: a player-side
                // wreck's blast (owner side >= 0) pays and counts (CB-5).
                by_player: o.side >= 0,
                // An RC car's blast is its player's (`+0x998`).
                player: rc.map(|r| r.player).or(o.player),
            });
            // An RC car frees its player's slot and holds the camera
            // (`DoCollideStuff` @ 10067b50).
            if let Some(r) = rc {
                rc_cars.ended(e, r.player);
            }
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
                // `Explode` @ 10055570 sets state 5: a driving player
                // loses the controls and can't get out of the burning car
                // (`GetInOutVehicles` @ 10066140 wants state 2); he dies
                // when it burns out ("Nuked!", below). Other vehicles go
                // straight to the wreck state (the original's 5 then 7).
                o.state = if is_player { State::Parked } else { State::Dead };
                let n = ((mass >> 10) + 2) as i32;
                for _ in 0..(n / 2).clamp(2, 6) {
                    let p = o.posf() + Vec3::new((rng.rand(1600) - 800) as f32, (rng.rand(1600) - 800) as f32, 0.0);
                    effects.add(7, Tex::Flame, p, Vec3::ZERO, 2500.0 + rng.rand(1500) as f32, 3, Blend::Add);
                    effects.add(2, Tex::Flame, p, Vec3::new(0.0, 0.0, -300.0), 800.0, 3, Blend::Add);
                }
                let mut at = o.pos;
                at.z -= 500;
                explode.write(Explode {
                    pos: at,
                    power: mass * 25000,
                    layer: o.level,
                    source: Some(e),
                    // `Explode` -> `Shockwave(.., i)`: pays and counts when
                    // the car is on the player's side (CB-5).
                    by_player: o.side >= 0,
                    player: o.player,
                });
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

#[cfg(test)]
#[path = "tests/contact.rs"]
mod tests;
