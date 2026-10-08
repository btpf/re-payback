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
use crate::effects::{Blend, Effects, Tex};
use crate::hud::{Banner, Texts};
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

    fn gravity(self) -> bool {
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

/// Number keys select a carried weapon (§3.1).
fn select_weapon(
    mut commands: Commands,
    mut inputs: ResMut<PlayerInputs>,
    mut triggers: ResMut<Triggers>,
    mut q: Query<(&Obj, &mut Arsenal, &mut Person), With<CameraTarget>>,
) {
    let slots = inputs.0.each_mut().map(|i| i.weapon_pressed.take());
    for (obj, mut ars, mut person) in &mut q {
        let Some(slot) = slots[obj.player.unwrap_or(0).min(3) as usize] else {
            continue;
        };
        let w = Weapon::ALL[slot as usize];
        let has = w == Weapon::Fist
            || ars.ammo[slot as usize] > 0
            || (w == Weapon::Bomb && ars.bomb.is_some());
        if obj.health > 0 && has && ars.current != w {
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
) {
    let (Some(mut library), Some(city)) = (library, city) else {
        return;
    };
    // Snapshot of targets for melee.
    let targets: Vec<(Entity, IVec3)> = shooters.iter().map(|(e, o, ..)| (e, o.pos)).collect();
    let mut punched: Vec<(Entity, Entity)> = Vec::new();
    for (e, mut obj, mut ars, stick, is_player) in &mut shooters {
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
                // Melee: people near pos + 256·dir (§3.2 Fist).
                let hit = obj.pos + dir;
                for &(t, tp) in &targets {
                    if t != e
                        && (tp.z - hit.z).abs() < 250
                        && (tp.x - hit.x).abs() < 3 * 192
                        && (tp.y - hit.y).abs() < 3 * 192
                    {
                        punched.push((e, t));
                    }
                }
                // A punch also breaks a crate 512 ahead.
                let ahead = obj.pos + dir * 2;
                break_crate_at(
                    &city,
                    ahead,
                    obj.level,
                    &mut set_scenery,
                    &mut effects,
                    &mut rng,
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
    // Apply punches: 30 damage, flat.
    for (attacker, target) in punched {
        let Ok([(_, mut a, ..), (_, mut t, ..)]) = shooters.get_many_mut([attacker, target]) else {
            continue;
        };
        if t.health <= 0 {
            continue;
        }
        t.health -= 30;
        commands.spawn(Sound::once(
            id::PUNCH_HIT,
            4e6,
            22050.0,
            SoundPos::At(t.posf()),
        ));
        commands.spawn(Sound::once(
            id::OW1 + rng.rand(3) as u16,
            5e6,
            8000.0,
            SoundPos::At(t.posf()),
        ));
        if a.state == State::Player {
            // Players punching each other only matters for the kill
            // credit (multiplayer).
            if t.state != State::Player {
                a.score += 500;
            }
            hurt.write(Hurt {
                victim: target,
                by_player: true,
                by_weapon: false,
                attacker: Some(a.player.unwrap_or(0)),
            });
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

/// Smash a crate on the square at `pos` into its contents (§4 Crates).
fn break_crate_at(
    city: &City,
    pos: IVec3,
    level: usize,
    set: &mut MessageWriter<SetScenery>,
    effects: &mut Effects,
    rng: &mut Rng,
) {
    let (sx, sy) = ((pos.x >> 11) as usize, (pos.y >> 11) as usize);
    if sx >= MAP_SIZE || sy >= MAP_SIZE {
        return;
    }
    let cube = &city.map.square(sx, sy).cubes[level];
    if cube.scenery != 6 {
        return;
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
    set.write(SetScenery {
        sx,
        sy,
        level,
        scenery: contents,
        special: None,
        rotation: None,
    });
    let centre = Vec3::new(
        (sx * 2048 + 1024) as f32,
        (sy * 2048 + 1024) as f32,
        pos.z as f32,
    );
    for _ in 0..3 {
        effects.add(
            3,
            Tex::Dust,
            centre,
            Vec3::ZERO,
            1500.0 + rng.rand(1000) as f32,
            level as u8 + 1,
            Blend::Add,
        );
    }
}

/// Projectile flight and contact with the city (§3.3).
#[allow(clippy::too_many_arguments)]
fn move_projectiles(
    mut commands: Commands,
    city: Option<Res<City>>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
    mut explode: MessageWriter<Explode>,
    mut set_scenery: MessageWriter<SetScenery>,
    mut q: Query<(Entity, &mut Obj, &Projectile)>,
) {
    let Some(city) = city else { return };
    for (e, mut obj, p) in &mut q {
        let kind = p.kind;
        obj.health -= kind.decay();
        if kind.gravity() {
            obj.vel.z += 32;
        }
        let prev = obj.pos;
        let mut next = obj.pos + obj.vel;
        let floor = super::collide::floor(&city, next.x, next.y, prev.z);
        let (resolved, hit) = super::collide::resolve_walls(&city, next, prev, 0);
        let on_ground = floor.is_some_and(|(fz, ..)| next.z >= fz);
        let layer = obj.level as u8 + 1;
        let at = SoundPos::At(prev.as_vec3());
        let mut remove = false;

        // Scenery stops projectiles and breaks crates.
        let (sx, sy) = ((next.x >> 11) as usize, (next.y >> 11) as usize);
        let scenery_hit = sx < MAP_SIZE
            && sy < MAP_SIZE
            && city.map.square(sx, sy).cubes[obj.level].scenery != 0
            && !kind.gravity();
        if scenery_hit {
            break_crate_at(
                &city,
                next,
                obj.level,
                &mut set_scenery,
                &mut effects,
                &mut rng,
            );
        }

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
                        by_player: p.by_player,
                        player: p.player,
                    });
                    remove = true;
                }
            }
            ProjectileKind::Missile => {
                if hit.x || hit.y || on_ground || scenery_hit || floor.is_none() {
                    explode.write(Explode {
                        pos: prev,
                        power: 1000 * 25000,
                        layer: obj.level,
                        source: Some(e),
                        by_player: p.by_player,
                        player: p.player,
                    });
                    remove = true;
                }
            }
            ProjectileKind::Petrol => {
                if hit.x || hit.y || on_ground || scenery_hit || obj.health <= 0 {
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
                if hit.x || hit.y || on_ground || scenery_hit {
                    commands.spawn(Sound::once(
                        id::RICOCHET,
                        4e6 + rng.rand(2_000_000) as f32,
                        11500.0 + rng.rand(1000) as f32,
                        at,
                    ));
                    for _ in 0..2 {
                        let v = Vec3::new(
                            (rng.rand(80) - 40) as f32,
                            (rng.rand(80) - 40) as f32,
                            -(rng.rand(40) as f32),
                        );
                        effects.add(9, Tex::Flare, prev.as_vec3(), v, 100.0, layer, Blend::Add);
                    }
                    remove = true;
                }
                if obj.health <= 0 {
                    remove = true;
                }
            }
        }
        if remove {
            commands.entity(e).despawn();
            continue;
        }
        obj.prev_pos = obj.pos;
        obj.pos = next;
    }
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
    projectiles: Query<(Entity, &Obj, &Projectile)>,
    mut targets: Query<(Entity, &mut Obj, Has<Person>), Without<Projectile>>,
) {
    let (Some(library), Some(city)) = (library, city) else {
        return;
    };
    for (pe, po, p) in &projectiles {
        if matches!(p.kind, ProjectileKind::Grenade | ProjectileKind::Bomb) {
            continue;
        }
        for (te, mut t, is_person) in &mut targets {
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
                        by_player: p.by_player,
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
            if t.filter == -1 || t.filter == weapon.filter_class() {
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
}

/// Explosion damage, knock-back and effects (§5.2).
#[allow(clippy::type_complexity)]
fn explosions(
    mut commands: Commands,
    mut events: MessageReader<Explode>,
    mut rng: ResMut<Rng>,
    library: Option<Res<ObjectLibrary>>,
    mut effects: ResMut<Effects>,
    mut flash: ResMut<crate::effects::ScreenFlash>,
    mut panic: MessageWriter<Panic>,
    mut hurt: MessageWriter<Hurt>,
    mut q: Query<(Entity, &mut Obj, Has<Person>, Option<&Projectile>)>,
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
        for (e, mut o, is_person, proj) in &mut q {
            if Some(e) == ex.source || proj.is_some() {
                continue;
            }
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
            let push = (d * q / k).clamp(I64Vec3::splat(-1000), I64Vec3::splat(1000));
            o.vel += push.as_ivec3();
            if matches!(o.filter, -1 | 1 | 3) {
                let dmg = if is_person { q / k } else { (q >> 4) / k };
                o.health -= dmg.min(i32::MAX as i64) as i32;
                if dmg > 0 {
                    hurt.write(Hurt {
                        victim: e,
                        by_player: ex.by_player,
                        by_weapon: true,
                        attacker: ex.player,
                    });
                }
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
    for (e, mut o, _) in &mut q {
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

/// The player collects pickups by entering their square (§4).
#[allow(clippy::too_many_arguments)]
fn pickups(
    mut commands: Commands,
    city: Option<Res<City>>,
    texts: Option<Res<Texts>>,
    mut banner: ResMut<Banner>,
    mut respawns: ResMut<PickupRespawns>,
    mut set: MessageWriter<SetScenery>,
    mut q: Query<(&Obj, &mut Arsenal), With<CameraTarget>>,
) {
    let Some(city) = city else { return };
    for (o, mut ars) in &mut q {
        if o.state != State::Player {
            continue;
        }
        let (sx, sy) = ((o.pos.x >> 11) as usize, (o.pos.y >> 11) as usize);
        if sx >= MAP_SIZE || sy >= MAP_SIZE {
            continue;
        }
        let scenery = city.map.square(sx, sy).cubes[o.level].scenery;
        let (slot, kind): (Option<usize>, u32) = match scenery {
            9 => (Some(6), 1),
            10 => (Some(3), 2),
            11 => (Some(4), 3),
            12 => (Some(5), 4),
            13 => (Some(2), 5),
            27 => (Some(1), 6),
            28 => (Some(7), 7),
            24 => (None, 0x50),
            34 => (None, 0x51),
            _ => continue,
        };
        if let Some(slot) = slot {
            ars.ammo[slot] += PICKUP_AMMO[slot];
            commands.spawn(Sound::once(
                id::PICKUP_WEAPON,
                1e6,
                14000.0,
                SoundPos::At(o.posf()),
            ));
        }
        if let Some(t) = &texts {
            banner.show_for(o.player, t.global(0x834 + kind - 1));
        }
        set.write(SetScenery {
            sx,
            sy,
            level: o.level,
            scenery: 0,
            special: None,
            rotation: None,
        });
        respawns.0.push((sx, sy, o.level, 1200));
    }
}

/// Collected pickups come back as crates after 1200 ticks (§4 Respawn).
fn respawn_pickups(
    mut commands: Commands,
    mut respawns: ResMut<PickupRespawns>,
    mut set: MessageWriter<SetScenery>,
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
