//! Weapons, projectiles, damage, explosions and pickups
//! (docs/re/combat_police.md §3–§6; 68k `FUN_000e3a58` firing,
//! `FUN_000e0e46` projectile physics, `FUN_000de0b6` hits,
//! `FUN_000cd22e` Explosion, `FUN_000d0988` pickups).

use super::people::{Person, PlayerInputs};
use super::police::Hurt;
use super::{Class, Obj, Rng, State, Stick, Trig, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::debris::Parts;
use crate::effects::{Blend, Effects, Tex};
use crate::hud::{Texts, labels::Labels};
use crate::level::LevelEntity;
use crate::mission::{MissionState, Triggers, kind};
use crate::objects::{ObjectLibrary, SetScenery};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::{math::I64Vec3, prelude::*};
use payback_formats::map::MAP_SIZE;

/// Weapons by HUD slot (key 0-7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Weapon {
    Fist = 0,
    Pistol,
    Shotgun,
    Flamethrower,
    Grenade,
    Minigun,
    Bazooka,
    Bomb,
}

impl Weapon {
    /// Weapon id in the original numbering (`+0x35e`): 0 fist, 1 bazooka,
    /// 2 flamethrower, 3 grenade, 4 minigun, 5 shotgun, 0xd pistol, 0xe bomb.
    pub fn original_id(self) -> i32 {
        match self {
            Weapon::Fist => 0,
            Weapon::Bazooka => 1,
            Weapon::Flamethrower => 2,
            Weapon::Grenade => 3,
            Weapon::Minigun => 4,
            Weapon::Shotgun => 5,
            Weapon::Pistol => 0xd,
            Weapon::Bomb => 0xe,
        }
    }

    /// Weapon code reported to "weapon fired" triggers (type 0x22).
    fn fired_code(self) -> i32 {
        match self {
            Weapon::Bazooka => 1,
            Weapon::Flamethrower => 2,
            Weapon::Grenade => 3,
            Weapon::Minigun => 4,
            Weapon::Shotgun => 5,
            Weapon::Pistol => 7,
            Weapon::Bomb => 8,
            Weapon::Fist => 0,
        }
    }

    pub const ALL: [Weapon; 8] = [
        Weapon::Fist,
        Weapon::Pistol,
        Weapon::Shotgun,
        Weapon::Flamethrower,
        Weapon::Grenade,
        Weapon::Minigun,
        Weapon::Bazooka,
        Weapon::Bomb,
    ];

    /// Index into PEOPLE_MODELS of the person holding this weapon
    /// (`DAT_012a6600[w]`, w = original weapon id).
    pub fn person_model(self) -> usize {
        match self {
            Weapon::Fist => 0,
            Weapon::Bazooka => 1,
            Weapon::Flamethrower => 2,
            Weapon::Grenade => 3,
            Weapon::Minigun => 4,
            Weapon::Shotgun => 5,
            Weapon::Pistol => 13,
            Weapon::Bomb => 14,
        }
    }

    /// Damage-filter class matching this weapon (§6).
    fn filter_class(self) -> i32 {
        match self {
            Weapon::Minigun => 4,
            Weapon::Pistol => 7,
            Weapon::Shotgun => 5,
            Weapon::Flamethrower => 2,
            _ => 1,
        }
    }
}

/// Weapons carried (people; copied into the vehicle while driving).
#[derive(Component, Clone, Debug)]
pub struct Arsenal {
    pub current: Weapon,
    /// Ammo by HUD slot (slot 0 unused).
    pub ammo: [i32; 8],
    /// Fire timer: charges to 25 while held, negative while reloading.
    pub fire_timer: i32,
    pub fire_held: bool,
    /// Placed remote bomb, if any.
    pub bomb: Option<Entity>,
    /// Frames left showing the firing/punch pose.
    pub pose: i32,
    flame_sound: Option<Entity>,
}

impl Default for Arsenal {
    fn default() -> Self {
        Arsenal {
            current: Weapon::Fist,
            ammo: [0; 8],
            fire_timer: 0,
            fire_held: false,
            bomb: None,
            pose: 0,
            flame_sound: None,
        }
    }
}

/// Ammo per pickup by weapon slot (§4).
const PICKUP_AMMO: [i32; 8] = [0, 30, 20, 75, 6, 100, 4, 1];

/// Projectile kinds and their object models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    Bullet,
    MinigunBullet,
    Pellet,
    Petrol,
    Grenade,
    Missile,
    Bomb,
}

impl ProjectileKind {
    fn model(self) -> &'static str {
        match self {
            ProjectileKind::Bullet | ProjectileKind::MinigunBullet => "weapons/bullet.obj",
            ProjectileKind::Pellet => "weapons/shotgunbullet.obj",
            ProjectileKind::Petrol => "weapons/petrol.obj",
            ProjectileKind::Grenade => "weapons/grenade.obj",
            ProjectileKind::Missile => "weapons/missile.obj",
            ProjectileKind::Bomb => "weapons/bomb.obj",
        }
    }

    /// Lifetime loss per tick (from 100), or fuse for grenades.
    fn decay(self) -> i32 {
        match self {
            ProjectileKind::Bullet | ProjectileKind::MinigunBullet | ProjectileKind::Grenade => 1,
            ProjectileKind::Pellet => 2,
            ProjectileKind::Petrol => 5,
            ProjectileKind::Missile | ProjectileKind::Bomb => 0,
        }
    }

    pub fn gravity(self) -> bool {
        matches!(self, ProjectileKind::Grenade | ProjectileKind::Bomb)
    }
}

#[derive(Component, Clone, Debug)]
pub struct Projectile {
    pub kind: ProjectileKind,
    pub shooter: Option<Entity>,
    /// The shooter is the player (for crimes and score).
    pub by_player: bool,
    /// The shooting player's number (kill credit in multiplayer).
    pub player: Option<u8>,
}

/// Request an explosion (68k `FUN_000cd22e`).
#[derive(Message, Clone, Copy, Debug)]
pub struct Explode {
    pub pos: IVec3,
    /// Power P (e.g. missile 2.5e7, grenade 6e7, remote bomb 1e9).
    pub power: i64,
    pub layer: usize,
    /// Object excluded from the blast (the exploding thing itself).
    pub source: Option<Entity>,
    /// The exploding object is on the player's side (owner side `+0x824
    /// >= 0`, [`Obj::side`]): its blast pays score and counts as the
    /// player's crime (`Shockwave` @ 10054ac0). AI grenades, missiles and
    /// bombs count too, as in the original; script explosions don't.
    pub by_player: bool,
    /// The player whose weapon this is (kill credit in multiplayer).
    pub player: Option<u8>,
}

/// Pickup respawns: (sx, sy, level, ticks left) -> becomes a crate.
#[derive(Resource, Default)]
pub struct PickupRespawns(Vec<(usize, usize, usize, i32)>);

/// The combat update chain.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CombatSet;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Explode>()
            .init_resource::<PickupRespawns>()
            .add_systems(
                FixedUpdate,
                (
                    select_weapon,
                    fire_weapons,
                    move_projectiles,
                    projectile_hits,
                    explosions,
                    deaths,
                    pickups,
                    respawn_pickups,
                )
                    .chain()
                    .in_set(CombatSet)
                    .after(super::people::PeopleSet),
            )
            .add_systems(Update, (sync_projectiles, swap_pose_models));
    }
}

/// Number keys select a carried weapon (§3.1): only alive and not stunned
/// (health > 0 and stun timer 0; Mac `UpdateInputStatus` @ 100f8490, the
/// Amiga agrees).
fn select_weapon(
    mut commands: Commands,
    mut inputs: ResMut<PlayerInputs>,
    mut triggers: ResMut<Triggers>,
    mut q: Query<(Entity, &Obj, &mut Arsenal, &mut Person), With<CameraTarget>>,
) {
    let slots = inputs.0.each_mut().map(|i| i.weapon_pressed.take());
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((_, obj, mut ars, mut person)) = q.get_mut(e__) else { continue };
        let Some(slot) = slots[obj.player.unwrap_or(0).min(3) as usize] else {
            continue;
        };
        let w = Weapon::ALL[slot as usize];
        let has = w == Weapon::Fist
            || ars.ammo[slot as usize] > 0
            || (w == Weapon::Bomb && ars.bomb.is_some());
        if obj.health > 0 && person.stun == 0 && has && ars.current != w {
            ars.current = w;
            if person.follows_weapon {
                person.kind = w.person_model();
            }
            commands.spawn(Sound::once(
                id::PICKUP_WEAPON,
                255.0,
                14000.0,
                SoundPos::Global,
            ));
            triggers.report(kind::WEAPON_SWITCHED, 0, 0, 0);
        }
    }
}

/// Auto-switch after running out (§3.1).
fn auto_switch(ars: &mut Arsenal) {
    use Weapon::*;
    let order: &[Weapon] = match ars.current {
        Grenade | Bomb | Bazooka | Flamethrower => &[Minigun, Shotgun, Pistol],
        Minigun => &[Shotgun, Pistol, Flamethrower],
        Pistol => &[Shotgun, Minigun, Flamethrower],
        Shotgun => &[Pistol, Minigun, Flamethrower],
        Fist => &[],
    };
    if let Some(&w) = order.iter().find(|w| ars.ammo[**w as usize] > 0) {
        ars.current = w;
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_projectile(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    kind: ProjectileKind,
    pos: IVec3,
    vel: IVec3,
    heading: i32,
    level: usize,
    shooter: Entity,
    player: Option<u8>,
) -> Option<Entity> {
    let model = library.load(data, meshes, kind.model())?;
    let mesh = library.model(model).mesh.clone();
    let mut obj = Obj::new(Class::Projectile, model, pos, heading);
    obj.vel = vel;
    obj.state = State::Parked;
    obj.level = level;
    obj.owner = Some(shooter);
    // Owner side (`+0x824`, Mac `do_Person` @ 1013fbd0): 1 for every
    // person's shot, whoever fires it, except shotgun pellets, which are 1
    // only from the player and −1 from anyone else. Only explosions read
    // it here (`Explode::by_player`, CB-5).
    obj.side = if kind == ProjectileKind::Pellet && player.is_none() { -1 } else { 1 };
    if kind == ProjectileKind::Grenade {
        obj.health = 80;
    }
    Some(
        commands
            .spawn((
                obj,
                Projectile {
                    kind,
                    shooter: Some(shooter),
                    by_player: player.is_some(),
                    player,
                },
                Mesh3d(mesh),
                MeshMaterial3d(library.material.clone()),
                Transform::from_translation(object_to_render(pos.as_vec3())),
                LevelEntity,
            ))
            .id(),
    )
}

/// Weapon firing for people (§3.2).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fire_weapons(
    mut commands: Commands,
    data: Res<GameData>,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shooters: Query<(Entity, &mut Obj, &mut Arsenal, &Stick, Has<CameraTarget>), With<Person>>,
    mut effects: ResMut<Effects>,
    mut set_scenery: MessageWriter<SetScenery>,
    city: Option<Res<City>>,
    mut panic: MessageWriter<Panic>,
    mut hurt: MessageWriter<Hurt>,
    mut triggers: ResMut<Triggers>,
    mission: Res<MissionState>,
    (mut crimes, mut debris, mut bystanders, mut cars, session, mode): (
        Option<ResMut<super::police::Crimes>>,
        ResMut<Parts>,
        Query<(Entity, &mut Obj), (With<Person>, Without<Arsenal>)>,
        Query<(Entity, &mut Obj), (With<CameraTarget>, Without<Person>)>,
        Res<super::people::PlayerSession>,
        Res<crate::level::GameMode>,
    ),
) {
    let (Some(mut library), Some(city)) = (library, city) else {
        return;
    };
    // Melee targets: every person, armed or not, in object-table order
    // (CB-1, DT-3; Mac `do_Person` @ 1013fbd0 scans the object array).
    let mut targets: Vec<(Entity, IVec3, u32)> = shooters
        .iter()
        .map(|(e, o, ..)| (e, o.pos, o.slot))
        .chain(bystanders.iter().map(|(e, o)| (e, o.pos, o.slot)))
        .collect();
    targets.sort_by_key(|t| t.2);
    // (puncher, its player number if a player, target).
    let mut punched: Vec<(Entity, Option<u8>, Entity)> = Vec::new();
    for e__ in crate::sim::table_order(shooters.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, mut ars, stick, is_player)) = shooters.get_mut(e__) else { continue };
        if ars.pose > 0 {
            ars.pose -= 1;
        }
        if obj.state == State::Dead || obj.health <= 0 {
            continue;
        }
        let pressed = stick.fire && !ars.fire_held;
        let released = !stick.fire && ars.fire_held;
        ars.fire_held = stick.fire;
        let player = is_player.then(|| obj.player.unwrap_or(0));
        // Firearms crime (+0x95c): only a grenade throw, a bomb drop, a
        // bazooka and a shotgun shot count (Mac `do_Person` @ 1013fbd0, 68k
        // `FUN_000e3a58` ~123180/~123300/~124022); the tank cannon counts in
        // `vehicle_ai::tank_cannon`.
        let firearm = |crimes: &mut Option<ResMut<super::police::Crimes>>| {
            if let (true, Some(c)) = (is_player, crimes.as_deref_mut()) {
                c.firearms += 1;
            }
        };
        let w = ars.current;
        let slot = w as usize;

        // Flamethrower loop sound only while firing.
        if !(stick.fire && w == Weapon::Flamethrower && ars.ammo[slot] > 0) {
            if let Some(s) = ars.flame_sound.take() {
                commands.entity(s).despawn();
            }
        }

        if !stick.fire {
            if released && w == Weapon::Grenade && ars.fire_timer > 0 && ars.ammo[slot] > 0 {
                // Throw: power P = charge (0..25).
                let p = ars.fire_timer;
                let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
                let side = IVec3::new(-c, s, 0) * 256 / 256;
                let vel = obj.vel + IVec3::new(s * p * 10 / 256, c * p * 10 / 256, -p * 40);
                let pos = obj.pos + side + IVec3::new(0, 0, -50);
                spawn_projectile(
                    &mut commands,
                    &data,
                    &mut library,
                    &mut meshes,
                    ProjectileKind::Grenade,
                    pos,
                    vel,
                    obj.heading,
                    obj.level,
                    e,
                    player,
                );
                ars.ammo[slot] -= 1;
                firearm(&mut crimes);
            }
            ars.fire_timer = if ars.fire_timer < 0 {
                ars.fire_timer + 1
            } else {
                0
            };
            continue;
        }
        if ars.fire_timer < 25 {
            ars.fire_timer = (ars.fire_timer + 1).min(25);
        }
        if w == Weapon::Grenade {
            continue; // charging
        }
        if w != Weapon::Fist && ars.ammo[slot] <= 0 && !(w == Weapon::Bomb && ars.bomb.is_some()) {
            if ars.fire_timer >= 0 {
                ars.fire_timer = -10;
                commands.spawn(Sound::once(
                    id::CLICK,
                    3e6,
                    16000.0,
                    SoundPos::At(obj.posf()),
                ));
                auto_switch(&mut ars);
            }
            continue;
        }
        let edge_only = matches!(w, Weapon::Fist | Weapon::Bazooka | Weapon::Bomb);
        if edge_only && !pressed {
            continue;
        }
        if !edge_only && ars.fire_timer < 0 {
            continue;
        }
        let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
        let dir = IVec3::new(s, c, 0);
        let muzzle = obj.pos + IVec3::new(0, 0, -50);
        let at = SoundPos::At(obj.posf());
        let mut shoot = |kind: ProjectileKind, spread: i32, offset: IVec3, rng: &mut Rng| {
            let h = wrap(obj.heading + rng.rand(2 * spread + 1) - spread);
            let v = IVec3::new(trig.sin(h), trig.cos(h), 0) + obj.vel;
            spawn_projectile(
                &mut commands,
                &data,
                &mut library,
                &mut meshes,
                kind,
                muzzle + offset,
                v,
                h,
                obj.level,
                e,
                player,
            );
        };
        match w {
            Weapon::Fist => {
                // Melee: people near pos + 256·dir with line of sight at
                // head height (§3.2 Fist; Mac `do_Person` @ 1013fbd0,
                // 68k `FUN_000e3a58` ~123605: `LineOfSight(pos, z −
                // 0x400 .. target, z − 0x400)`). Punch_Hit plays once at
                // the puncher if anyone was in reach.
                let hit = obj.pos + dir;
                let mut any = false;
                for &(t, tp, _) in &targets {
                    if t != e
                        && (tp.z - hit.z).abs() < 250
                        && (tp.x - hit.x).abs() < 3 * 192
                        && (tp.y - hit.y).abs() < 3 * 192
                        && super::collide::line_of_sight(&city, obj.pos, tp)
                    {
                        punched.push((e, player, t));
                        any = true;
                    }
                }
                if any {
                    commands.spawn(Sound::once(id::PUNCH_HIT, 4e6, 22050.0, at));
                }
                // A punch also breaks a crate 512 ahead.
                let ahead = obj.pos + dir * 2;
                break_crate_at(
                    &city,
                    ahead,
                    obj.level,
                    &mut set_scenery,
                    &mut debris,
                );
                ars.pose = 2;
            }
            Weapon::Pistol => {
                shoot(ProjectileKind::Bullet, 20, IVec3::ZERO, &mut rng);
                ars.ammo[slot] -= 1;
                ars.fire_timer = -15;
                obj.speed -= 4;
                commands.spawn(Sound::once(id::PISTOL, 4e6, 14000.0, at));
                ars.pose = 1;
            }
            Weapon::Minigun => {
                shoot(ProjectileKind::MinigunBullet, 40, IVec3::ZERO, &mut rng);
                ars.ammo[slot] -= 1;
                ars.fire_timer = -3;
                obj.speed -= 0x10;
                commands.spawn(Sound::once(id::MINIGUN, 4e6, 16000.0, at));
                ars.pose = 1;
            }
            Weapon::Shotgun => {
                if ars.ammo[slot] < 2 {
                    auto_switch(&mut ars);
                    continue;
                }
                shoot(ProjectileKind::Pellet, 80, IVec3::ZERO, &mut rng);
                shoot(
                    ProjectileKind::Pellet,
                    80,
                    IVec3::new(c / 2, -s / 2, 0),
                    &mut rng,
                );
                ars.ammo[slot] -= 2;
                firearm(&mut crimes);
                ars.fire_timer = -30;
                obj.speed -= 0x20;
                commands.spawn(Sound::once(id::SHOTGUN_FIRE, 4e6, 14000.0, at));
                ars.pose = 1;
            }
            Weapon::Flamethrower => {
                if ars.flame_sound.is_none() {
                    let s = commands
                        .spawn(Sound::looping(id::FLAMETHROWER, 5e6, 12500.0, at))
                        .id();
                    ars.flame_sound = Some(s);
                }
                if ars.fire_timer % 3 == 0 {
                    let jitter = IVec3::new(rng.rand(51) - 25, rng.rand(51) - 25, 0);
                    spawn_projectile(
                        &mut commands,
                        &data,
                        &mut library,
                        &mut meshes,
                        ProjectileKind::Petrol,
                        muzzle,
                        dir + obj.vel + jitter,
                        obj.heading,
                        obj.level,
                        e,
                        player,
                    );
                    ars.ammo[slot] -= 1;
                }
            }
            Weapon::Bazooka => {
                shoot(ProjectileKind::Missile, 0, IVec3::ZERO, &mut rng);
                ars.ammo[slot] -= 1;
                firearm(&mut crimes);
                obj.speed -= 0x20;
                commands.spawn(Sound::once(id::FIRE_MISSILE, 5e6, 17000.0, at));
                for _ in 0..8 {
                    let back = obj.pos - dir * 2;
                    let v = Vec3::new(
                        (rng.rand(60) - 30) as f32,
                        (rng.rand(60) - 30) as f32,
                        -(rng.rand(30) as f32),
                    );
                    effects.add(
                        3,
                        Tex::Dust,
                        back.as_vec3(),
                        v,
                        600.0 + rng.rand(400) as f32,
                        obj.level as u8 + 1,
                        Blend::Add,
                    );
                }
            }
            Weapon::Bomb => {
                if ars.bomb.is_some() {
                    // Detonate (Destroy(bomb, 50000): P = 1e9).
                    ars.bomb = None;
                    commands.queue(move |world: &mut World| detonate_bomb(world, e));
                } else {
                    let pos = obj.pos + dir * 2;
                    ars.bomb = spawn_projectile(
                        &mut commands,
                        &data,
                        &mut library,
                        &mut meshes,
                        ProjectileKind::Bomb,
                        pos,
                        obj.vel,
                        obj.heading,
                        obj.level,
                        e,
                        player,
                    );
                    ars.ammo[slot] -= 1;
                    firearm(&mut crimes);
                    // Scripts tag the player's bombs (`SetProjectileId`).
                    if let (Some(b), true) = (ars.bomb, is_player && mission.projectile_id != -1) {
                        let id = mission.projectile_id;
                        commands.queue(move |world: &mut World| {
                            if let Some(mut o) = world.get_mut::<Obj>(b) {
                                o.script_id = id;
                            }
                        });
                    }
                }
            }
            Weapon::Grenade => {}
        }
        if w != Weapon::Fist {
            panic.write(Panic {
                at: obj.pos,
                from: e,
            });
            if is_player {
                triggers.report(kind::WEAPON_FIRED, w.fired_code(), 0, 0);
            }
        }
    }
    // Apply punches, puncher by puncher in table order (Mac `do_Person`
    // @ 1013fbd0): a target with health >= 0 says Ow and loses 30, flat.
    // On an NPC (owner side −1) the player's object gets 500 × the
    // multiplier and the hit counts as his (`CheckHitObject(t, 0)`),
    // whoever punched (CB-2); between players only the multiplayer kill
    // credit.
    let mut pays: Vec<(Option<Entity>, i32)> = Vec::new();
    for (attacker, attacker_player, target) in punched {
        let t = match shooters.get_mut(target) {
            Ok((_, o, ..)) => Some(o),
            Err(_) => bystanders.get_mut(target).ok().map(|(_, o)| o),
        };
        let Some(mut t) = t else { continue };
        if t.health < 0 {
            continue;
        }
        commands.spawn(Sound::once(
            id::OW1 + rng.rand(3) as u16,
            5e6,
            8000.0,
            SoundPos::At(t.posf()),
        ));
        // God mode (`crate::cheats`): no damage.
        if !t.god {
            t.health -= 30;
        }
        let check = t.side == -1;
        if check {
            pays.push((attacker_player.map(|_| attacker), 500 * session.multiplier));
        }
        if check || (attacker_player.is_some() && t.player.is_some()) {
            hurt.write(Hurt {
                victim: target,
                by_player: true,
                by_weapon: false,
                attacker: attacker_player,
            });
        }
    }
    for (payee, pay) in pays {
        // An NPC's punch pays the single player (the original's
        // `objects[DAT_1017c60c]`).
        let payee = payee.or_else(|| {
            if mode.is_multiplayer() {
                return None;
            }
            let on_foot = shooters.iter().filter(|(.., p)| *p).map(|(e, o, ..)| (o.slot, e));
            let driving = cars.iter().map(|(e, o)| (o.slot, e));
            on_foot.chain(driving).min().map(|(_, e)| e)
        });
        let Some(payee) = payee else { continue };
        if let Ok((_, mut o, ..)) = shooters.get_mut(payee) {
            o.score += pay;
        } else if let Ok((_, mut o)) = cars.get_mut(payee) {
            o.score += pay;
        }
    }
}

/// Explode a placed remote bomb (`Destroy(bomb, 50000)`).
fn detonate_bomb(world: &mut World, owner: Entity) {
    let bomb = world
        .query::<(Entity, &Obj, &Projectile)>()
        .iter(world)
        .find(|(_, _, p)| p.kind == ProjectileKind::Bomb && p.shooter == Some(owner))
        .map(|(e, o, _)| (e, o.pos, o.level));
    if let Some((e, pos, level)) = bomb {
        let player = world.get::<Obj>(owner).and_then(|o| o.player).unwrap_or(0);
        // `Destroy` (0xa653a): "actor destroyed" triggers on the bomb
        // (its script id from `SetProjectileId`) fire.
        let mut triggers = world.resource_mut::<Triggers>();
        triggers.report_actor(kind::ACTOR_KILLED, e);
        triggers.report_actor(kind::ACTOR_GONE, e);
        world.entity_mut(e).despawn();
        world.write_message(Explode {
            pos,
            power: 50000 * 20000,
            layer: level,
            source: None,
            by_player: true,
            player: Some(player),
        });
    }
}

/// Pedestrians near a shot or explosion panic (68k `FUN_000ccabe`).
#[derive(Message, Clone, Copy, Debug)]
pub struct Panic {
    pub at: IVec3,
    pub from: Entity,
}

/// Smash a crate on the square at `pos` into its contents (§4 Crates;
/// `CheckForCrate` @ 100593e0). Returns the contents' scenery id (0 for
/// none) when there was a crate.
pub(crate) fn break_crate_at(
    city: &City,
    pos: IVec3,
    level: usize,
    set: &mut MessageWriter<SetScenery>,
    debris: &mut Parts,
) -> Option<u8> {
    let (sx, sy) = ((pos.x >> 11) as usize, (pos.y >> 11) as usize);
    if pos.x < 0 || pos.y < 0 || sx >= MAP_SIZE || sy >= MAP_SIZE || level > 2 {
        return None;
    }
    let cube = &city.map.square(sx, sy).cubes[level];
    if cube.scenery != 6 {
        return None;
    }
    let contents = match cube.special {
        1 => 9,
        2 => 10,
        3 => 11,
        4 => 12,
        5 => 13,
        6 => 27,
        7 => 28,
        _ => 0,
    };
    let rotation = cube.rotation;
    set.write(SetScenery {
        sx,
        sy,
        level,
        scenery: contents,
        special: None,
        rotation: None,
    });
    // `Bang(crate, layer+1, centre, z, rotation·0x1680 >> 8, 5)` (Mac
    // `CheckForCrate` @ 100593e0 -> `Bang` @ 100a6dd0, 68k `FUN_000d0670`
    // -> `FUN_000384e8`): the crate's fragments and 2 dust puffs, all from
    // the cosmetic `random3` stream (`crate::debris`).
    let centre = IVec3::new((sx * 2048 + 1024) as i32, (sy * 2048 + 1024) as i32, pos.z);
    debris.bang(centre, level + 1, rotation);
    Some(contents)
}

/// One type-3 Dust puff of `Bang` / `CheckForWeapon` / `RespawnWeapons`:
/// size 1500 + r(1000), v = r(500) − 250 per axis (divided by 3 in
/// `AddEffect`), pos = `base` + (r(800), r(800), r(300)). Seven draws in
/// the Mac/68k order (last argument first): size, vz, vy, vx, z, y, x.
pub(crate) fn dust_puff(effects: &mut Effects, r: &mut impl FnMut(i32) -> i32, base: IVec3, layer: u8) {
    let size = 1500 + r(1000);
    let vz = r(500) - 250;
    let vy = r(500) - 250;
    let vx = r(500) - 250;
    let z = base.z + r(300);
    let y = base.y + r(800);
    let x = base.x + r(800);
    effects.add(
        3,
        Tex::Dust,
        Vec3::new(x as f32, y as f32, z as f32),
        Vec3::new(vx as f32, vy as f32, vz as f32),
        size as f32,
        layer,
        Blend::Add,
    );
}

/// Projectile flight and contact with the city (§3.3): the move and the
/// wall/ground handling of `do_Weapon` @ 1013e390, then the scenery test of
/// `AddCollideObj` @ 10055c70 (`object_MainLoop` @ 1014a670 runs them in
/// that order for every object).
#[allow(clippy::too_many_arguments)]
fn move_projectiles(
    mut commands: Commands,
    city: Option<Res<City>>,
    library: Option<Res<ObjectLibrary>>,
    trig: Res<Trig>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
    mut explode: MessageWriter<Explode>,
    mut set_scenery: MessageWriter<SetScenery>,
    mut triggers: ResMut<Triggers>,
    mut edits: MessageWriter<crate::mapedit::MapEdit>,
    mut debris: ResMut<Parts>,
    mut arsenals: Query<&mut Arsenal>,
    mut q: Query<(Entity, &mut Obj, &Projectile)>,
) {
    let Some(city) = city else { return };
    // Snowmen knocked down and crates broken this tick (the map edits land
    // later; the original clears the square at once).
    let mut snowmen: Vec<(usize, usize, usize)> = Vec::new();
    let mut crates: Vec<(usize, usize, usize)> = Vec::new();
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, p)) = q.get_mut(e__) else { continue };
        let kind = p.kind;
        obj.health -= kind.decay();
        if kind.gravity() {
            obj.vel.z += 32;
        }
        let prev = obj.pos;
        let mut next = obj.pos + obj.vel;
        let floor = super::collide::floor(&city, next.x, next.y, prev.z);
        // +0x8c8, the ground under the projectile (`MoveObject_General` @
        // 1006e4f0; the last one is kept when there is none).
        if let Some((fz, ..)) = floor {
            obj.floor_z = fz;
        }
        // Projectiles smash glass (`GetCollideInfo` @ 100af0e0: any
        // class but people).
        let mut glass = Vec::new();
        let (resolved, hit) = super::collide::resolve_walls_glass(&city, next, prev, 0, &mut glass);
        super::collide::smash_glass(&mut commands, &city, &mut edits, &mut triggers, &glass);
        let on_ground = floor.is_some_and(|(fz, ..)| next.z >= fz);
        let layer = obj.level as u8 + 1;
        let at = SoundPos::At(prev.as_vec3());
        let mut remove = false;
        // A missile's explosion, written unless scenery removes it first.
        let mut blast = None;

        match kind {
            ProjectileKind::Grenade | ProjectileKind::Bomb => {
                if hit.x {
                    obj.vel.x = -obj.vel.x;
                }
                if hit.y {
                    obj.vel.y = -obj.vel.y;
                }
                if hit.x || hit.y {
                    let hz = if kind == ProjectileKind::Bomb {
                        5000.0
                    } else {
                        8000.0
                    };
                    commands.spawn(Sound::once(id::GRENADE_BOUNCE, 4e6, hz, at));
                }
                next = resolved;
                if let Some((fz, ..)) = floor {
                    if next.z >= fz {
                        next.z = fz;
                        if obj.vel.z > 500 {
                            commands.spawn(Sound::once(id::GRENADE_BOUNCE, 4e6, 8000.0, at));
                        }
                        obj.vel.z = -obj.vel.z / 4;
                        let f = |v: i32| v - v.signum() * v.abs().min(8);
                        obj.vel.x = f(obj.vel.x);
                        obj.vel.y = f(obj.vel.y);
                    }
                }
                if kind == ProjectileKind::Grenade && obj.health < 2 {
                    explode.write(Explode {
                        pos: next,
                        power: 3000 * 20000,
                        layer: obj.level,
                        source: Some(e),
                        by_player: obj.side >= 0,
                        player: p.player,
                    });
                    remove = true;
                }
                // A placed remote bomb destroyed by damage (a script
                // `ExplodeObject`) goes off in full and frees its owner's
                // bomb slot (Mac `DoCollideStuff` @ 10067b50: health < -70
                // or <= 0 -> owner +0x860 = 0, `Detonate(i, 50000)`). The
                // Amiga (68k `FUN_000dc5ac`, OS4 `FUN_010542b8`) left the
                // slot pointing at the dead bomb.
                if kind == ProjectileKind::Bomb && obj.health < 1 {
                    for mut a in &mut arsenals {
                        if a.bomb == Some(e) {
                            a.bomb = None;
                        }
                    }
                    triggers.report_actor(kind::ACTOR_KILLED, e);
                    triggers.report_actor(kind::ACTOR_GONE, e);
                    explode.write(Explode {
                        pos: next,
                        power: 50000 * 20000,
                        layer: obj.level,
                        source: Some(e),
                        by_player: obj.side >= 0,
                        player: p.player,
                    });
                    remove = true;
                }
            }
            ProjectileKind::Missile => {
                // It explodes on walls, the ground and objects (68k
                // `FUN_000cdfdc`); scenery only removes it (below).
                if hit.x || hit.y || on_ground || floor.is_none() {
                    blast = Some(Explode {
                        pos: prev,
                        power: 1000 * 25000,
                        layer: obj.level,
                        source: Some(e),
                        by_player: obj.side >= 0,
                        player: p.player,
                    });
                    remove = true;
                }
            }
            ProjectileKind::Petrol => {
                if hit.x || hit.y || on_ground || obj.health <= 0 {
                    effects.add(
                        0xf,
                        Tex::Flame,
                        prev.as_vec3(),
                        Vec3::ZERO,
                        800.0,
                        layer,
                        Blend::Add,
                    );
                    remove = true;
                } else if rng.rand(2) == 0 {
                    effects.add(
                        1,
                        Tex::Flame,
                        prev.as_vec3(),
                        Vec3::ZERO,
                        250.0,
                        layer,
                        Blend::Add,
                    );
                }
            }
            _ => {
                if hit.x || hit.y || on_ground {
                    // Ricochet and sparks (Mac `do_Weapon` @ 1013e390, 68k
                    // ~121690-121800): the pitch is drawn before the volume;
                    // bullets make 2 sparks of size rand(150)+150, pellets 3
                    // of rand(150)+250. Gameplay code, so the sim `Rng`
                    // (`random`), drawn in the original's order.
                    let hz = 11500.0 + rng.rand(1000) as f32;
                    let vol = 4e6 + rng.rand(2_000_000) as f32;
                    commands.spawn(Sound::once(id::RICOCHET, vol, hz, at));
                    let (n, base) = if kind == ProjectileKind::Pellet {
                        (3, 250)
                    } else {
                        (2, 150)
                    };
                    for _ in 0..n {
                        spark(&mut effects, &mut rng, prev, obj.vel, base, layer);
                    }
                    remove = true;
                }
                if obj.health <= 0 {
                    remove = true;
                }
            }
        }

        // Scenery (Mac `AddCollideObj` @ 10055c70, 68k `FUN_000cdfdc`;
        // combat_police.md §3.3): every scenery object on the projectile's
        // cube level is a circle (`collide::scenery_collide`, the test
        // people and vehicles use) of radius scenery size / 2 + the
        // projectile model's half-width (+0x8120), tested only while the
        // projectile is within 500 of its ground (+0x8c8 − z). Not the
        // whole square: a lamp post stops a bullet (half-width 192) within
        // 240 + 192 of its base.
        // A hit pushes the projectile out by half the overlap, then by
        // kind: a snowman is knocked down (the shot flies on), a missile is
        // removed without exploding, petrol leaves a flame, bullets and
        // pellets ricochet with 2 sparks, grenades and bombs only bounce
        // off with a sound; all but those two break a crate. The wall
        // branch above runs first, as in `object_MainLoop` @ 1014a670.
        if let Some(library) = &library {
            let info = &library.model(obj.model).info;
            let mut pos = if remove { prev } else { next };
            let body = super::collide::Body {
                radius: info.radius,
                front: info.max_y,
                rear: info.min_y,
                vehicle: false,
                rc: false,
                projectile: true,
                dz: [obj.floor_z - pos.z, 0, 0, 0],
            };
            let mut vel = obj.vel;
            let bumps = super::collide::scenery_collide(
                &city, library, &trig, &mut pos, &mut vel, obj.heading, obj.level, &body,
            );
            if !bumps.is_empty() {
                obj.vel = vel;
                if !remove {
                    next = pos;
                }
            }
            for b in bumps {
                let key = (b.sx, b.sy, b.level);
                let hit_at = IVec3::new(b.at.x, b.at.y, pos.z);
                let snd = SoundPos::At(hit_at.as_vec3());
                let square = IVec3::new((b.sx * 2048 + 0x400) as i32, (b.sy * 2048 + 0x400) as i32, pos.z);
                let mut break_crate = |set: &mut MessageWriter<SetScenery>, debris: &mut Parts| {
                    if !crates.contains(&key) && break_crate_at(&city, square, b.level, set, debris).is_some() {
                        crates.push(key);
                    }
                };
                if b.id == super::driving::SNOWMAN {
                    // 5 type-1 Dust puffs as for a car (`driving.rs`), from
                    // the ground under the projectile (+0x8c8) ± 150 (CB-6).
                    if snowmen.contains(&key) {
                        continue;
                    }
                    snowmen.push(key);
                    for _ in 0..5 {
                        let size = 600 + rng.rand(300);
                        let vz = rng.rand(100) - 50;
                        let vy = rng.rand(100) - 50;
                        let vx = rng.rand(300) - 150;
                        let z = obj.floor_z + rng.rand(300) - 150;
                        let y = (b.sy * 2048 + 0x270) as i32 + rng.rand(800);
                        let x = (b.sx * 2048 + 0x39c) as i32 + rng.rand(200);
                        effects.add(
                            1,
                            Tex::Dust,
                            Vec3::new(x as f32, y as f32, z as f32),
                            Vec3::new(vx as f32, vy as f32, vz as f32),
                            size as f32,
                            layer,
                            Blend::Add,
                        );
                    }
                    set_scenery.write(SetScenery {
                        sx: b.sx,
                        sy: b.sy,
                        level: b.level,
                        scenery: 0,
                        special: None,
                        rotation: None,
                    });
                    continue;
                }
                match kind {
                    ProjectileKind::Missile => {
                        break_crate(&mut set_scenery, &mut debris);
                        blast = None;
                        remove = true;
                    }
                    ProjectileKind::Petrol => {
                        // One type-1 flame of size 800 in a random Flame
                        // frame; drawn last argument first: vz = r(20) −
                        // 100, vy = r(20) − 10, vx = r(20), z ± 50, y and
                        // x ± 200, then the frame.
                        remove = true;
                        let vz = rng.rand(20) - 100;
                        let vy = rng.rand(20) - 10;
                        let vx = rng.rand(20);
                        let z = hit_at.z + rng.rand(100) - 50;
                        let y = hit_at.y + rng.rand(400) - 200;
                        let x = hit_at.x + rng.rand(400) - 200;
                        let frame = rng.rand(4) as u8;
                        effects.add_frame(
                            1,
                            Tex::Flame,
                            frame,
                            Vec3::new(x as f32, y as f32, z as f32),
                            Vec3::new(vx as f32, vy as f32, vz as f32),
                            800.0,
                            layer,
                            Blend::Add,
                        );
                        break_crate(&mut set_scenery, &mut debris);
                    }
                    ProjectileKind::Grenade => {
                        commands.spawn(Sound::once(id::GRENADE_BOUNCE, 4e6, 8000.0, snd));
                    }
                    ProjectileKind::Bomb => {
                        commands.spawn(Sound::once(id::GRENADE_BOUNCE, 4e6, 5000.0, snd));
                    }
                    ProjectileKind::Bullet | ProjectileKind::MinigunBullet | ProjectileKind::Pellet => {
                        // Crate, Ricochet (pitch drawn first), 2 sparks of
                        // rand(150) + 150 for every bullet kind (pellets
                        // too), thrown back along the pushed velocity.
                        break_crate(&mut set_scenery, &mut debris);
                        let hz = 11500.0 + rng.rand(1000) as f32;
                        let vol = 4e6 + rng.rand(2_000_000) as f32;
                        commands.spawn(Sound::once(id::RICOCHET, vol, hz, snd));
                        for _ in 0..2 {
                            spark(&mut effects, &mut rng, hit_at, obj.vel, 150, layer);
                        }
                        remove = true;
                    }
                }
            }
        }
        if let Some(b) = blast {
            explode.write(b);
        }
        if remove {
            commands.entity(e).despawn();
            continue;
        }
        obj.prev_pos = obj.pos;
        obj.pos = next;
    }
}

/// One bullet/pellet spark (effect type 9, Flare): pos ± 100 per axis,
/// v = rand(80) − 40 − v/8 per axis (thrown back), size rand(150) + `base`.
/// The draws follow the original's order: size, vz, vy, vx, z, y, x (68k
/// ~121690, arguments pushed right to left).
fn spark(effects: &mut Effects, rng: &mut Rng, at: IVec3, vel: IVec3, base: i32, layer: u8) {
    let size = rng.rand(150) + base;
    let vz = rng.rand(80) - ((vel.z >> 3) + 40);
    let vy = rng.rand(80) - ((vel.y >> 3) + 40);
    let vx = rng.rand(80) - ((vel.x >> 3) + 40);
    let z = rng.rand(200) + at.z - 100;
    let y = rng.rand(200) + at.y - 100;
    let x = rng.rand(200) + at.x - 100;
    effects.add(
        9,
        Tex::Flare,
        Vec3::new(x as f32, y as f32, z as f32),
        Vec3::new(vx as f32, vy as f32, vz as f32),
        size as f32,
        layer,
        Blend::Add,
    );
}

/// Projectiles hitting people and vehicles (68k `FUN_000de0b6`).
#[allow(clippy::type_complexity)]
fn projectile_hits(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    city: Option<Res<City>>,
    library: Option<Res<ObjectLibrary>>,
    mut effects: ResMut<Effects>,
    mut explode: MessageWriter<Explode>,
    mut hurt: MessageWriter<Hurt>,
    session: Res<super::people::PlayerSession>,
    projectiles: Query<(Entity, &Obj, &Projectile)>,
    mut targets: Query<(Entity, &mut Obj, Has<Person>), Without<Projectile>>,
) {
    let (Some(library), Some(city)) = (library, city) else {
        return;
    };
    // Hit money for the shooting player, paid after the loop.
    let mut pay: Vec<(u8, i32)> = Vec::new();
    for e__ in crate::sim::table_order(projectiles.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((pe, po, p)) = projectiles.get(e__) else { continue };
        if matches!(p.kind, ProjectileKind::Grenade | ProjectileKind::Bomb) {
            continue;
        }
        for e__ in crate::sim::table_order(targets.iter().map(|(e, o, ..)| (e, o))) {
            let Ok((te, mut t, is_person)) = targets.get_mut(e__) else { continue };
            if Some(te) == p.shooter || t.state == State::Removed {
                continue;
            }
            let info = &library.model(t.model).info;
            let r = (info.radius + library.model(po.model).info.radius) as i64 * 17 / 16;
            let d = (t.pos - po.pos).as_i64vec3();
            if d.x * d.x + d.y * d.y + (d.z / 2) * (d.z / 2) >= r * r {
                continue;
            }
            let mass = info.mass.max(1);
            let at = SoundPos::At(t.posf());
            let (weapon, base) = match p.kind {
                ProjectileKind::MinigunBullet => (Weapon::Minigun, 10000),
                ProjectileKind::Bullet => (Weapon::Pistol, 5000),
                ProjectileKind::Pellet => {
                    // Only with line of sight from the shooter.
                    if !super::collide::line_of_sight(&city, po.prev_pos, t.pos) {
                        continue;
                    }
                    (Weapon::Shotgun, 20000)
                }
                ProjectileKind::Petrol => (Weapon::Flamethrower, 3000),
                ProjectileKind::Missile => {
                    explode.write(Explode {
                        pos: po.pos,
                        power: 1000 * 25000,
                        layer: po.level,
                        source: Some(pe),
                        by_player: po.side >= 0,
                        player: p.player,
                    });
                    commands.entity(pe).despawn();
                    break;
                }
                _ => continue,
            };
            hurt.write(Hurt {
                victim: te,
                by_player: p.by_player,
                by_weapon: true,
                attacker: p.player,
            });
            // God mode (`crate::cheats`): no damage.
            if (t.filter == -1 || t.filter == weapon.filter_class()) && !t.god {
                if p.kind == ProjectileKind::Petrol {
                    let dmg = (base / mass).clamp(1, (t.health + 71).max(1));
                    t.health -= dmg;
                    effects.add(
                        1,
                        Tex::Flame,
                        t.posf(),
                        Vec3::ZERO,
                        800.0,
                        t.level as u8 + 1,
                        Blend::Add,
                    );
                    if is_person && t.health <= 0 && t.burn == 0 {
                        t.health = 0;
                        t.burn = 100;
                        commands.spawn(Sound::once(id::LONG_AAAH, 5e6, 6000.0, at));
                    }
                } else {
                    t.health -= base / mass;
                    // Money: min(8000, k / T), k = 40 x the damage numerator,
                    // T = the target's toughness, for player shots on
                    // non-players (68k `FUN_000de0b6` ~119024).
                    if let (true, Some(pl), None) = (p.by_player, p.player, t.player) {
                        pay.push((pl, (40 * base / mass).min(8000)));
                    }
                }
            }
            if p.kind != ProjectileKind::Petrol {
                if is_person {
                    commands.spawn(Sound::once(
                        id::OW1 + rng.rand(3) as u16,
                        3e6 + rng.rand(1_000_000) as f32,
                        7500.0 + rng.rand(1000) as f32,
                        at,
                    ));
                    effects.add(
                        9,
                        Tex::Blood,
                        t.posf(),
                        Vec3::ZERO,
                        120.0,
                        t.level as u8 + 1,
                        Blend::Tint,
                    );
                } else {
                    commands.spawn(Sound::once(
                        id::RICOCHET,
                        4e6 + rng.rand(2_000_000) as f32,
                        11500.0 + rng.rand(1000) as f32,
                        at,
                    ));
                    for _ in 0..2 {
                        effects.add(
                            9,
                            Tex::Flare,
                            po.posf(),
                            Vec3::ZERO,
                            100.0,
                            t.level as u8 + 1,
                            Blend::Add,
                        );
                    }
                }
                commands.entity(pe).despawn();
            }
            break;
        }
    }
    for (pl, money) in pay {
        if let Some((_, mut o, _)) = targets.iter_mut().find(|(_, o, _)| o.player == Some(pl) && o.state == State::Player) {
            o.score += money * session.multiplier.max(1);
        }
    }
}

/// Report a player-side blast's hit on every NPC, even at zero damage, as
/// the original does (`Shockwave` @ 10054ac0 calls `CheckHitObject(i, 1)`
/// for every owner-side −1 object, no radius). Off: with it every grenade
/// or missile makes each NPC around a GBH victim and the wanted level jumps
/// to 4 at once, which got the autoplay bot busted repeatedly (even in the
/// grenade tutorial). Only hits that do damage count, as before (CB-4,
/// explosions part, left for a decision).
const BLAST_CRIME_AT_ZERO_DAMAGE: bool = false;

/// Explosion damage, knock-back, score and effects (§5.2; Mac `Shockwave`
/// @ 10054ac0 = 68k `FUN_000cd22e` / OS4 `FUN_0104405c`).
///
/// With q = (P>>4)/T: people and vehicles lose q/k, class-2 props
/// (barrier, cone, dummy; `model+0x8165 == 2`) only (q>>4)/k and get no
/// knock-back. In single player a blast from a player-side object (owner
/// side `+0x824 >= 0`: the player's car, his shots, and every person's
/// grenade, missile or bomb) on an NPC (side −1) pays `min(6000,
/// q/(k>>5))` (6000 when k>>5 < 1) times the multiplier and reports the
/// hit (`CheckHitObject(i, 1)`) even at zero damage: there is no radius,
/// so every NPC in the object table counts as hit once.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn explosions(
    mut commands: Commands,
    mut events: MessageReader<Explode>,
    mut rng: ResMut<Rng>,
    library: Option<Res<ObjectLibrary>>,
    mut effects: ResMut<Effects>,
    mut flash: ResMut<crate::effects::ScreenFlash>,
    mut panic: MessageWriter<Panic>,
    mut hurt: MessageWriter<Hurt>,
    mode: Res<crate::level::GameMode>,
    session: Res<super::people::PlayerSession>,
    mut q: Query<(Entity, &mut Obj, Has<Person>, Option<&Projectile>, Has<CameraTarget>)>,
) {
    let Some(library) = library else {
        events.clear();
        return;
    };
    for ex in events.read() {
        let p = ex.power;
        // Screen flash proportional to the blast (rendering.md §4).
        flash.0 += (p / 250_000) as i32;
        let hz = if p < 140_000_000 {
            12000.0 - (p / 20000) as f32
        } else {
            5000.0
        };
        let mut at = ex.pos.as_vec3();
        at.z -= 100.0;
        commands.spawn(Sound::once(
            id::EXPLOSION,
            50e6,
            hz.max(1000.0),
            SoundPos::At(at),
        ));
        if let Some(src) = ex.source {
            panic.write(Panic {
                at: ex.pos,
                from: src,
            });
        }
        // Score for the player (single player only), paid after the loop.
        let pays = ex.by_player && !mode.is_multiplayer();
        let mut score: i64 = 0;
        for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
            let Ok((e, mut o, is_person, proj, is_target)) = q.get_mut(e__) else { continue };
            if Some(e) == ex.source || proj.is_some() {
                continue;
            }
            // Class 2: props without a `Projectile` (barrier, cone, dummy).
            let prop = o.class == Class::Projectile;
            let d = (o.pos - ex.pos).as_i64vec3() / 256;
            let r2 = d.length_squared();
            let u = (r2 >> 4) * (r2 as f64).sqrt() as i64;
            let k = if is_person {
                if u == 0 { 1 } else { 5 * u }
            } else {
                (u >> 1).max(1)
            };
            let mass = library.model(o.model).info.mass.max(1) as i64;
            let q = (p >> 4) / mass;
            if !prop {
                let push = (d * q / k).clamp(I64Vec3::splat(-1000), I64Vec3::splat(1000));
                o.vel += push.as_ivec3();
            }
            // God mode (`crate::cheats`): no damage.
            if matches!(o.filter, -1 | 1 | 3) && !o.god {
                let dmg = if prop { (q >> 4) / k } else { q / k };
                o.health -= dmg.min(i32::MAX as i64) as i32;
                // Single player, a player-side blast on an NPC (owner side
                // −1): score and `CheckHitObject(i, 1)`. The original
                // calls it whatever the damage, with no radius, so every
                // NPC in the object table counts as hit; that is
                // `BLAST_CRIME_AT_ZERO_DAMAGE` (off: see its note).
                let check = pays && o.side == -1 && !is_target;
                if check {
                    score += if k >> 5 < 1 { 6000 } else { (q / (k >> 5)).min(6000) };
                }
                if (check && BLAST_CRIME_AT_ZERO_DAMAGE) || dmg > 0 {
                    hurt.write(Hurt {
                        victim: e,
                        // Crime only from the check; in multiplayer the
                        // blast's player gets the kill credit.
                        by_player: check || (ex.by_player && mode.is_multiplayer()),
                        by_weapon: true,
                        attacker: ex.player,
                    });
                }
            }
        }
        if score > 0 {
            if let Some((_, mut pl, ..)) = q.iter_mut().filter(|(.., t)| *t).min_by_key(|(_, o, ..)| o.slot) {
                let add = score * session.multiplier.max(1) as i64;
                pl.score = (pl.score as i64 + add).min(i32::MAX as i64) as i32;
            }
        }
        let layer = ex.layer as u8 + 1;
        let pos = ex.pos.as_vec3();
        let scorch = if p >= 120_000_000 {
            6000.0
        } else {
            ((p / 20000) as f32).max(1000.0)
        };
        effects.add(
            8,
            Tex::Dust2,
            pos,
            Vec3::ZERO,
            scorch,
            layer,
            Blend::Subtract,
        );
        effects.add(
            10,
            Tex::Shockwave,
            pos,
            Vec3::ZERO,
            300.0,
            layer,
            Blend::Add,
        );
        let flash = Vec3::new(
            pos.x + (rng.rand(400) - 200) as f32,
            pos.y + (rng.rand(400) - 200) as f32,
            21000.0,
        );
        effects.add(
            11,
            Tex::LensFlare,
            flash,
            Vec3::ZERO,
            1000.0,
            layer,
            Blend::Add,
        );
        for _ in 0..3 {
            let v = Vec3::new(
                (rng.rand(200) - 100) as f32,
                (rng.rand(200) - 100) as f32,
                -(rng.rand(300) as f32),
            );
            effects.add(
                7,
                Tex::Flame,
                pos,
                v,
                2500.0 + rng.rand(1500) as f32,
                3,
                Blend::Add,
            );
        }
    }
}

/// People at <= 0 health die; burning people burn out (§6).
fn deaths(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
    mut triggers: ResMut<Triggers>,
    library: Option<Res<ObjectLibrary>>,
    mut killed: MessageWriter<super::score::Killed>,
    mut q: Query<(Entity, &mut Obj, &Person)>,
) {
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut o, _)) = q.get_mut(e__) else { continue };
        let police = library.as_ref().is_some_and(|l| l.model(o.model).info.police);
        let was_alive = !matches!(o.state, State::Dead | State::Removed);
        if o.burn > 0 {
            o.burn -= 1;
            if rng.rand(3) == 0 {
                let p = o.posf()
                    + Vec3::new(
                        (rng.rand(300) - 150) as f32,
                        (rng.rand(300) - 150) as f32,
                        0.0,
                    );
                effects.add(
                    4,
                    Tex::Flame,
                    p,
                    Vec3::new(0.0, 0.0, -600.0),
                    192.0 + rng.rand(150) as f32,
                    o.level as u8 + 1,
                    Blend::Add,
                );
            }
            if o.burn < 2 && o.state != State::Dead {
                o.state = State::Dead;
            }
        }
        if o.health < -70 && o.state != State::Removed {
            // Blown apart: squelch and a blood pool.
            commands.spawn(Sound::once(
                id::SQUELCH,
                15e6,
                18000.0,
                SoundPos::At(o.posf()),
            ));
            effects.add(
                8,
                Tex::Blood,
                o.posf(),
                Vec3::ZERO,
                500.0 + rng.rand(300) as f32,
                o.level as u8 + 1,
                Blend::Tint,
            );
            o.state = State::Removed;
            if was_alive {
                killed.write(super::score::Killed { victim: e, police });
            }
            triggers.report_actor(kind::ACTOR_KILLED, e);
            triggers.report_actor(kind::ACTOR_GONE, e);
            // Removed (state 6) by `despawn_far` at the start of the next
            // tick, so the crime and the kill are still counted this tick.
            if o.player.is_none() {
                commands.entity(e).insert(Visibility::Hidden);
            }
        } else if o.health <= 0 && !matches!(o.state, State::Dead | State::Removed) {
            o.state = State::Dead;
            killed.write(super::score::Killed { victim: e, police });
            triggers.report_actor(kind::ACTOR_KILLED, e);
            triggers.report_actor(kind::ACTOR_GONE, e);
            commands.spawn(Sound::once(
                id::LONG_AAAH,
                10e6,
                6000.0,
                SoundPos::At(o.posf()),
            ));
        } else if o.state == State::Dead && rng.rand(6) == 0 {
            effects.add(
                8,
                Tex::Blood,
                o.posf(),
                Vec3::ZERO,
                120.0 + rng.rand(120) as f32,
                o.level as u8 + 1,
                Blend::Tint,
            );
        }
    }
}

/// The player collects pickups by entering their square (§4). On foot the
/// square under the centre; a player-driven vehicle collects through its
/// box points instead (`driving.rs`, Mac `CheckForCubeCollision` @
/// 100af980).
#[allow(clippy::too_many_arguments)]
fn pickups(
    mut commands: Commands,
    city: Option<Res<City>>,
    texts: Option<Res<Texts>>,
    mut labels: ResMut<Labels>,
    mut respawns: ResMut<PickupRespawns>,
    mut set: MessageWriter<SetScenery>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
    mut q: Query<(Entity, &Obj, &mut Arsenal), (With<CameraTarget>, Without<crate::vehicles::Vehicle>)>,
) {
    let Some(city) = city else { return };
    // Players in object-table order: the first one there takes a pickup.
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((_, o, mut ars)) = q.get_mut(e__) else { continue };
        if o.state != State::Player {
            continue;
        }
        let (sx, sy) = (o.pos.x >> 11, o.pos.y >> 11);
        let mut pick = Pickup {
            commands: &mut commands,
            texts: texts.as_deref(),
            labels: &mut labels,
            respawns: &mut respawns,
            set: &mut set,
            rng: &mut rng,
            effects: &mut effects,
        };
        pick.check_for_weapon(&city, o, &mut ars, sx, sy, o.level, o.pos.z);
    }
}

/// What `CheckForWeapon` touches.
pub(crate) struct Pickup<'a, 'w, 's, 'm> {
    pub commands: &'a mut Commands<'w, 's>,
    pub texts: Option<&'a Texts>,
    pub labels: &'a mut Labels,
    pub respawns: &'a mut PickupRespawns,
    pub set: &'a mut MessageWriter<'m, SetScenery>,
    pub rng: &'a mut Rng,
    pub effects: &'a mut Effects,
}

impl Pickup<'_, '_, '_, '_> {
    /// `CheckForWeapon(obj, sx, sy, level, z)` @ 100597e0 (68k
    /// `FUN_000d0988`): a pickup on that cell goes to `o`'s arsenal, with
    /// 2 dust puffs at `z`, the pickup sound and its name floating up; it
    /// comes back as a crate after 1200 ticks. Returns the scenery id
    /// taken (the cell is now empty), if any.
    #[allow(clippy::too_many_arguments)]
    pub fn check_for_weapon(
        &mut self,
        city: &City,
        o: &Obj,
        ars: &mut Arsenal,
        sx: i32,
        sy: i32,
        level: usize,
        z: i32,
    ) -> Option<u8> {
        if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) || level > 2 {
            return None;
        }
        let (sx, sy) = (sx as usize, sy as usize);
        let scenery = city.map.square(sx, sy).cubes[level].scenery;
        // (ammo slot, name phrase): kinds 1-7 are 2100-2104, 2106, 2107,
        // the info pickup (0x50) 2105, the key (0x51) 2108 (Mac
        // `CheckForWeapon` @ 100597e0).
        let (slot, phrase): (Option<usize>, u32) = match scenery {
            9 => (Some(6), 2100),
            10 => (Some(3), 2101),
            11 => (Some(4), 2102),
            12 => (Some(5), 2103),
            13 => (Some(2), 2104),
            27 => (Some(1), 2106),
            28 => (Some(7), 2107),
            24 => (None, 2105),
            34 => (None, 2108),
            _ => return None,
        };
        // 2 dust puffs before the sound, from the gameplay `random`: 14
        // draws (Mac `CheckForWeapon` @ 100597e0, 68k `FUN_000d0988`).
        let base = IVec3::new((sx * 2048 + 0x270) as i32, (sy * 2048 + 0x270) as i32, z - 150);
        for _ in 0..2 {
            dust_puff(self.effects, &mut |n| self.rng.rand(n), base, level as u8 + 1);
        }
        // Every kind plays the pickup sound and floats its name up from
        // the square centre at the object's z (`AddLabel`, hud::labels).
        if let Some(slot) = slot {
            ars.ammo[slot] += PICKUP_AMMO[slot];
        }
        self.commands.spawn(Sound::once(
            id::PICKUP_WEAPON,
            1e6,
            14000.0,
            SoundPos::At(o.posf()),
        ));
        if let Some(t) = self.texts {
            let centre = IVec3::new((sx * 2048 + 0x400) as i32, (sy * 2048 + 0x400) as i32, o.pos.z);
            self.labels.add(centre, t.global(phrase));
        }
        self.set.write(SetScenery {
            sx,
            sy,
            level,
            scenery: 0,
            special: None,
            rotation: None,
        });
        self.respawns.0.push((sx, sy, level, 1200));
        Some(scenery)
    }
}

/// Collected pickups come back as crates after 1200 ticks (§4 Respawn),
/// with 2 dust puffs at the fixed height 0x4d8a from the gameplay `random`
/// (14 draws; Mac `RespawnWeapons` @ 100595c0, 68k `FUN_000d079e`).
fn respawn_pickups(
    mut commands: Commands,
    mut respawns: ResMut<PickupRespawns>,
    mut set: MessageWriter<SetScenery>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
) {
    for r in &mut respawns.0 {
        r.3 -= 1;
        if r.3 == 0 {
            set.write(SetScenery {
                sx: r.0,
                sy: r.1,
                level: r.2,
                scenery: 6,
                special: None,
                rotation: None,
            });
            let base = IVec3::new((r.0 * 2048 + 0x270) as i32, (r.1 * 2048 + 0x270) as i32, 0x4d8a);
            let layer = (r.2 as u8 + 1).min(3);
            for _ in 0..2 {
                dust_puff(&mut effects, &mut |n| rng.rand(n), base, layer);
            }
            let at = Vec3::new((r.0 * 2048 + 1024) as f32, (r.1 * 2048 + 1024) as f32, 0.0);
            commands.spawn(Sound::once(
                id::OBJECT_APPEAR,
                20e6,
                8000.0,
                SoundPos::At(at),
            ));
        }
    }
    respawns.0.retain(|r| r.3 > 0);
}

fn sync_projectiles(mut q: Query<(&Obj, &mut Transform), With<Projectile>>) {
    for (o, mut tf) in &mut q {
        tf.translation = object_to_render(o.posf());
        tf.rotation = heading_to_render(o.heading as f32);
    }
}

/// People show the model for their weapon, or the firing/punch pose
/// briefly after a shot (§2.1 model swaps).
fn swap_pose_models(mut q: Query<(&Arsenal, &mut Person)>) {
    for (ars, mut person) in &mut q {
        let base = ars.current.person_model();
        let pose = match (ars.current, ars.pose > 0) {
            (Weapon::Fist, true) => Some("people/malenormalpunch.obj"),
            (Weapon::Pistol, true) => Some("people/malenormalpistol_fire.obj"),
            (Weapon::Shotgun, true) => Some("people/malenormalshotgun_fire.obj"),
            (Weapon::Minigun, true) => Some("people/malenormalminigun_fire.obj"),
            _ => None,
        };
        person.pose_model = pose;
        if person.follows_weapon {
            person.kind = base;
        }
    }
}

#[cfg(test)]
#[path = "tests/combat.rs"]
mod tests;
