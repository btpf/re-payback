//! Cheats (port addition, not in the original; docs/re/frontend_menus.md
//! "Cheat menu"): F12 in a single-player level opens a menu
//! (`crate::frontend::cheats`) with God mode, Vehicle god mode, All
//! weapons, Spawn vehicle and the wanted level (clear, up, down).
//!
//! Every cheat changes the simulation, so the menu only queues it
//! ([`Cheats::queue`]); [`apply`] takes the queue at the start of the next
//! fixed tick, right after the replay input (`crate::replay`), records each
//! action in the replay as a [`CheatEvent`] at that tick, and applies it.
//! Playing a replay back applies the recorded events at their ticks
//! instead (the menu is not available then), so a game with cheats replays
//! exactly. The god-mode flags are sim state: reset at level start,
//! changed only by the events, and part of the replay hash through
//! [`Obj::god`].
//!
//! - **God mode**: the player's object (on foot, or the vehicle they drive)
//!   takes no damage: punches, bullets, flames, explosions, run-overs and
//!   crashes leave its health alone, so it can't be killed or wrecked by
//!   damage. Deaths that are not damage still happen: drowning (sinking
//!   below the water), being busted by the police, mission scripts that
//!   kill the player, the RC car dying in water.
//! - **Vehicle god mode**: the same protection for the vehicle the player
//!   drives only, whichever it is (it follows the player from vehicle to
//!   vehicle; a vehicle left behind is vulnerable again), while the player
//!   on foot can be hurt as usual. It sets the same [`Obj::god`] flag, so
//!   the vehicle takes no crash, collision, run-over, bullet, flame or
//!   blast damage and so is never set alight or wrecked (`sim::contact`
//!   wrecks a vehicle only at health < 1). God mode already covers the
//!   driven vehicle (it is the player's object while driving): this is
//!   the vehicle-only half. Sinking is not damage and is kept as in the
//!   original: a car driven into deep water still sinks and the player
//!   drowns, and the RC car still dies in water; mission scripts can still
//!   destroy it.
//! - **All weapons**: every weapon slot 1..7 gets at least [`FULL_AMMO`].
//! - **Spawn vehicle**: a parked vehicle of the chosen type
//!   ([`VEHICLES`]) in front of the player, facing the player's heading:
//!   the nearest free spot straight ahead ([`find_spot`]), else the
//!   nearest free one around. Boats and the helicopter spawn like the rest
//!   (a boat prefers water ahead, a car dry ground; either falls back to
//!   any free spot). In one of the [`PAINT_COUNT`] paint colours when the
//!   model has paintable faces (`objects::paintable`; the original paints
//!   every traffic car `random(8)`, `AddThing` @ 10058350, and scripts
//!   choose it in `CreateActor`); fixed liveries (police, taxi, bus, ...)
//!   have only colour 0.
//! - **Wanted level**: clear (0), up or down one level, 0..[`MAX_WANTED`],
//!   on the player's object (`Obj::wanted`, the original's `+0x32e`),
//!   restarting the decay timer (`WantedTimer`, `DAT_0030c3b8`) as a crime
//!   that changes the level does (`sim::police::crimes`). The police
//!   react to the new level as to any other change: pursuit from 3,
//!   police in traffic from 4, roadblocks from 5, cops on foot from 3.
//!
//! Debug: `--cheat-script <tick>:<cheat>,...` queues cheats at those ticks
//! of a recorded game (`god`, `nogod`, `vgod`, `novgod`, `weapons`,
//! `vehicle=<type>[:<paint>]`,
//! `wanted+`, `wanted-`, `wanted0`), e.g. for the replay check; `--cheat-menu` / `--cheat-vehicle <type>` open the
//! menus (`crate::frontend`).

use crate::camera::CameraTarget;
use crate::level::{AppState, GameMode};
use crate::objects::{ObjectLibrary, PAINT_COUNT};
use crate::replay::{Mode, ReplayState};
use crate::sim::combat::Arsenal;
use crate::sim::police::WantedTimer;
use crate::sim::{Obj, Trig, collide};
use crate::vehicles::{Vehicle, kind as vk};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::prelude::*;
use payback_formats::replay::CheatEvent;

/// Every vehicle type the player can drive, in `vehicles::TYPES` order: all but
/// the rocket car's two thrust frames (25, 26), which are animation frames
/// of type 24.
pub const VEHICLES: [usize; 26] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 27,
];

/// Ammo per weapon after "All weapons" (shown as 00999 on the HUD).
pub const FULL_AMMO: i32 = 999;

/// Highest wanted level (six sirens on the HUD; the crime tables top out
/// at 6, `sim::police`).
pub const MAX_WANTED: i32 = 6;

/// Replay event kinds ([`CheatEvent::kind`]).
const EV_GOD: u32 = 1;
const EV_WEAPONS: u32 = 2;
/// Argument: the type, plus the paint colour << 8 (files from before the
/// colours have 0 there: black, as such a vehicle was spawned).
const EV_VEHICLE: u32 = 3;
/// Argument: +1 up, -1 down.
const EV_WANTED: u32 = 4;
const EV_CLEAR_WANTED: u32 = 5;
/// Argument: 1 on, 0 off.
const EV_VEHICLE_GOD: u32 = 6;

/// One cheat action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cheat {
    /// God mode on or off.
    God(bool),
    /// Vehicle god mode on or off.
    VehicleGod(bool),
    AllWeapons,
    /// Spawn a vehicle of this `vehicles::TYPES` index in this paint
    /// colour (`objects::PAINTS` index).
    Vehicle(usize, u8),
    /// Wanted level up (+1) or down (-1).
    Wanted(i32),
    ClearWanted,
}

impl Cheat {
    pub fn event(self, tick: u32) -> CheatEvent {
        let (kind, arg) = match self {
            Cheat::God(on) => (EV_GOD, on as i32),
            Cheat::VehicleGod(on) => (EV_VEHICLE_GOD, on as i32),
            Cheat::AllWeapons => (EV_WEAPONS, 0),
            Cheat::Vehicle(k, c) => (EV_VEHICLE, k as i32 | (c as i32) << 8),
            Cheat::Wanted(d) => (EV_WANTED, d.signum()),
            Cheat::ClearWanted => (EV_CLEAR_WANTED, 0),
        };
        CheatEvent { tick, kind, arg }
    }

    pub fn from_event(e: &CheatEvent) -> Option<Cheat> {
        Some(match e.kind {
            EV_GOD => Cheat::God(e.arg != 0),
            EV_VEHICLE_GOD => Cheat::VehicleGod(e.arg != 0),
            EV_WEAPONS => Cheat::AllWeapons,
            EV_VEHICLE => vehicle(e.arg & 0xff, e.arg >> 8)?,
            EV_WANTED if e.arg != 0 => Cheat::Wanted(e.arg.signum()),
            EV_CLEAR_WANTED => Cheat::ClearWanted,
            _ => return None,
        })
    }

    /// `god`, `nogod`, `vgod`, `novgod`, `weapons`, `vehicle=<type>[:<paint>]`, `wanted+`,
    /// `wanted-`, `wanted0` (`--cheat-script`).
    fn parse(s: &str) -> Option<Cheat> {
        Some(match s.trim() {
            "god" => Cheat::God(true),
            "nogod" => Cheat::God(false),
            "vgod" => Cheat::VehicleGod(true),
            "novgod" => Cheat::VehicleGod(false),
            "weapons" => Cheat::AllWeapons,
            "wanted+" => Cheat::Wanted(1),
            "wanted-" => Cheat::Wanted(-1),
            "wanted0" => Cheat::ClearWanted,
            v => {
                let v = v.strip_prefix("vehicle=")?;
                let (k, c) = v.split_once(':').unwrap_or((v, "0"));
                vehicle(k.parse().ok()?, c.parse().ok()?)?
            }
        })
    }
}

/// A valid [`Cheat::Vehicle`].
fn vehicle(kind: i32, colour: i32) -> Option<Cheat> {
    let k = usize::try_from(kind).ok().filter(|k| VEHICLES.contains(k))?;
    let c = u8::try_from(colour).ok().filter(|&c| (c as usize) < PAINT_COUNT)?;
    Some(Cheat::Vehicle(k, c))
}

/// Cheat state: god mode and vehicle god mode (sim state, changed only in
/// the fixed tick), the
/// player's wanted level as of the last tick (for the menu) and the
/// actions the menu has queued for the next tick.
#[derive(Resource, Default, Debug)]
pub struct Cheats {
    pub god: bool,
    /// Vehicle god mode: the vehicle the player drives takes no damage.
    pub vehicle_god: bool,
    /// The player's wanted level after the last tick (a copy, for the menu).
    pub wanted: i32,
    pub queue: Vec<Cheat>,
}

impl Cheats {
    /// The wanted level as it will be once the queue is applied (the menu
    /// shows this).
    pub fn wanted_pending(&self) -> i32 {
        self.queue.iter().fold(self.wanted, |w, c| match c {
            Cheat::Wanted(d) => (w + d).clamp(0, MAX_WANTED),
            Cheat::ClearWanted => 0,
            _ => w,
        })
    }

    /// God mode as it will be once the queue is applied (the menu shows
    /// this).
    pub fn god_pending(&self) -> bool {
        self.queue
            .iter()
            .rev()
            .find_map(|c| if let Cheat::God(on) = c { Some(*on) } else { None })
            .unwrap_or(self.god)
    }

    /// Vehicle god mode as it will be once the queue is applied (the menu
    /// shows this).
    pub fn vehicle_god_pending(&self) -> bool {
        self.queue
            .iter()
            .rev()
            .find_map(|c| if let Cheat::VehicleGod(on) = c { Some(*on) } else { None })
            .unwrap_or(self.vehicle_god)
    }
}

/// The cheat menu is available: a single-player level (story or rampage),
/// not a replay.
pub fn allowed(mode: &GameMode, replay: &ReplayState) -> bool {
    !mode.is_multiplayer() && !replay.playing()
}

pub struct CheatsPlugin;

impl Plugin for CheatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cheats>()
            .add_systems(OnEnter(AppState::Playing), |mut c: ResMut<Cheats>| *c = Cheats::default());
    }
}

/// `--cheat-script` entries: (tick, cheat).
fn script() -> Vec<(u32, Cheat)> {
    let Some(s) = crate::arg("--cheat-script") else { return Vec::new() };
    s.split(',')
        .filter_map(|e| {
            let (t, c) = e.split_once(':')?;
            Some((t.trim().parse().ok()?, Cheat::parse(c)?))
        })
        .collect()
}

/// Start of every fixed tick, after the replay input (chained after
/// `replay::debug_hash`): this tick's cheats, recorded or played back,
/// then the god-mode flag onto the player objects (every one with god
/// mode; with vehicle god mode, a player's object that is a vehicle).
pub fn apply(world: &mut World, mut script_cache: Local<Option<Vec<(u32, Cheat)>>>) {
    let queued = std::mem::take(&mut world.resource_mut::<Cheats>().queue);
    let multiplayer = world.resource::<GameMode>().is_multiplayer();
    let mut st = world.resource_mut::<ReplayState>();
    let tick = st.tick;
    let mode = st.mode;
    let mut actions: Vec<Cheat> = Vec::new();
    match mode {
        Mode::Play => {
            if let Some(r) = &st.replay {
                actions.extend(r.header.cheats.iter().filter(|e| e.tick == tick).filter_map(Cheat::from_event));
            }
        }
        _ if multiplayer => {}
        _ => {
            let script = script_cache.get_or_insert_with(script);
            actions.extend(script.iter().filter(|(t, _)| *t == tick).map(|(_, c)| *c));
            actions.extend(queued);
            if mode == Mode::Record {
                if let Some(r) = &mut st.replay {
                    r.header.cheats.extend(actions.iter().map(|c| c.event(tick)));
                }
            }
        }
    }
    for a in actions {
        info!("cheat at tick {tick}: {a:?}");
        match a {
            Cheat::God(on) => world.resource_mut::<Cheats>().god = on,
            Cheat::VehicleGod(on) => world.resource_mut::<Cheats>().vehicle_god = on,
            Cheat::AllWeapons => all_weapons(world),
            Cheat::Vehicle(k, c) => {
                spawn_vehicle(world, k, c);
            }
            Cheat::Wanted(d) => set_wanted(world, |w| w + d),
            Cheat::ClearWanted => set_wanted(world, |_| 0),
        }
    }
    let wanted = player(world).and_then(|(e, ..)| world.get::<Obj>(e)).map_or(0, |o| o.wanted);
    world.resource_mut::<Cheats>().wanted = wanted;
    let (god, vehicle_god) = {
        let c = world.resource::<Cheats>();
        (c.god, c.vehicle_god)
    };
    let mut q = world.query::<(&mut Obj, Has<Vehicle>)>();
    for (mut o, is_vehicle) in q.iter_mut(world) {
        let want = (god || (vehicle_god && is_vehicle)) && o.player.is_some();
        if o.god != want {
            o.god = want;
        }
    }
}

/// The single player's object (on foot or driving): (entity, position,
/// heading, z of the object).
fn player(world: &mut World) -> Option<(Entity, IVec3, i32)> {
    let mut q = world.query_filtered::<(Entity, &Obj), With<CameraTarget>>();
    let mut v: Vec<(u32, Entity, IVec3, i32)> = q
        .iter(world)
        .filter(|(_, o)| o.player.is_some())
        .map(|(e, o)| (o.slot, e, o.pos, o.heading))
        .collect();
    v.sort_unstable_by_key(|p| p.0);
    v.first().map(|&(_, e, p, h)| (e, p, h))
}

/// Set the player's wanted level to `f(level)`, within 0..[`MAX_WANTED`];
/// a change restarts the decay timer, as a crime's does
/// (`sim::police::crimes`, 68k `FUN_000ccc96`).
fn set_wanted(world: &mut World, f: impl Fn(i32) -> i32) {
    let Some((e, ..)) = player(world) else { return };
    let Some(mut o) = world.get_mut::<Obj>(e) else { return };
    let w = f(o.wanted).clamp(0, MAX_WANTED);
    if o.wanted != w {
        o.wanted = w;
        if let Some(mut t) = world.get_resource_mut::<WantedTimer>() {
            t.0 = 0;
        }
    }
}

/// Every weapon with at least [`FULL_AMMO`] (the arsenal travels with the
/// player: the person's, or the car's while driving).
fn all_weapons(world: &mut World) {
    let Some((e, ..)) = player(world) else { return };
    if let Some(mut a) = world.get_mut::<Arsenal>(e) {
        for slot in 1..8 {
            a.ammo[slot] = a.ammo[slot].max(FULL_AMMO);
        }
    }
}

/// Spawn a parked vehicle of type `kind` in paint `colour` in front of the
/// player, facing the player's heading ([`find_spot`]). Nothing when there
/// is no free spot.
fn spawn_vehicle(world: &mut World, kind: usize, colour: u8) -> Option<Entity> {
    let (_, pos, heading) = player(world)?;
    let car = crate::vehicles::spawn_vehicle_world(world, kind as i32, pos, heading, colour as i32)?;
    let (model, slot) = world.get::<Obj>(car).map(|o| (o.model, o.slot))?;
    let info = world.resource::<ObjectLibrary>().model(model).info.clone();
    let body = Body {
        hw: if kind == vk::HELICOPTER { info.radius } else { info.radius.max(0x100) },
        front: info.max_y.max(0x100),
        rear: info.min_y.max(0x100),
    };
    // Other vehicles to keep clear of: (position, radius), in table order.
    let mut others: Vec<(u32, IVec3, i32)> = {
        let mut q = world.query_filtered::<&Obj, With<Vehicle>>();
        let lib = world.resource::<ObjectLibrary>();
        q.iter(world)
            .filter(|o| o.slot != slot)
            .map(|o| {
                let i = &lib.model(o.model).info;
                (o.slot, o.pos, i.radius.max(i.max_y).max(i.min_y))
            })
            .collect()
    };
    others.sort_unstable_by_key(|o| o.0);
    let others: Vec<(IVec3, i32)> = others.into_iter().map(|o| (o.1, o.2)).collect();
    let spot = {
        let city = world.get_resource::<City>();
        let trig = world.resource::<Trig>();
        city.and_then(|c| find_spot(c, trig, pos, heading, &body, kind == vk::BOAT, &others))
    };
    let Some((at, level, water)) = spot else {
        warn!("cheat: no free spot for vehicle {kind}");
        let engine = world.get::<Vehicle>(car).and_then(|v| v.engine);
        world.entity_mut(car).despawn();
        if let Some(e) = engine {
            world.entity_mut(e).despawn();
        }
        return None;
    };
    if let Some(mut o) = world.get_mut::<Obj>(car) {
        o.pos = at;
        o.prev_pos = at;
        o.floor_z = at.z;
        o.level = level;
        o.in_water = water;
    }
    if let Some(mut t) = world.get_mut::<Transform>(car) {
        *t = Transform::from_translation(object_to_render(at.as_vec3())).with_rotation(heading_to_render(heading as f32));
    }
    Some(car)
}

/// A vehicle's footprint: half-width and front / rear extents (model
/// radius, max Y, min Y).
pub struct Body {
    pub hw: i32,
    pub front: i32,
    pub rear: i32,
}

/// Where a new vehicle goes: straight ahead of the player at `pos`
/// (heading `heading`), at the first distance from just clear of the
/// player out to 8 squares where it fits ([`fits`]); else the nearest such
/// spot in any direction (rings of 16 directions, nearest ring first).
/// Prefers water for a boat and dry ground for the rest, then takes any
/// fit. Returns (position, cube level, in water).
pub fn find_spot(
    city: &City,
    trig: &Trig,
    pos: IVec3,
    heading: i32,
    body: &Body,
    boat: bool,
    others: &[(IVec3, i32)],
) -> Option<(IVec3, usize, bool)> {
    let start = 0x100 + 192 + body.rear;
    let ahead = |d: i32, a: i32| IVec2::new(pos.x + (trig.sin(a) * d >> 8), pos.y + (trig.cos(a) * d >> 8));
    let mut candidates: Vec<IVec2> = (0..)
        .map(|i| start + 0x100 * i)
        .take_while(|&d| d <= 8 * 2048)
        .map(|d| ahead(d, heading))
        .collect();
    for d in (0..).map(|i| start + 0x200 * i).take_while(|&d| d <= 8 * 2048) {
        for k in 1..16 {
            candidates.push(ahead(d, heading + k * 0x168));
        }
    }
    let mut any = None;
    for c in candidates {
        let Some((z, level, water)) = fits(city, trig, pos, c, heading, body, others) else { continue };
        if water == boat {
            return Some((IVec3::new(c.x, c.y, z), level, water));
        }
        any.get_or_insert((IVec3::new(c.x, c.y, z), level, water));
    }
    any
}

/// A vehicle fits at `c` (heading `heading`) when the floor there is within
/// a step of the player's height (or, for water, up to two squares below
/// it: a boat off a quay), the floor under all nine box points within a
/// step of that, the box meets no wall on the way from the player
/// (`collide::vehicle_walls`, the centre path from `from`), and it is
/// clear of the other vehicles. Returns (floor z, cube level, in water).
fn fits(
    city: &City,
    trig: &Trig,
    from: IVec3,
    c: IVec2,
    heading: i32,
    body: &Body,
    others: &[(IVec3, i32)],
) -> Option<(i32, usize, bool)> {
    let step = collide::STEP * 16;
    let (z, level, water) = collide::floor(city, c.x, c.y, from.z)
        .filter(|&(z, _, water)| (z - from.z).abs() <= step || (water && (0..=2 * 2048).contains(&(z - from.z))))?;
    let near = |pz: i32| (pz - z).abs() <= step;
    let at = IVec3::new(c.x, c.y, z);
    let b = collide::box_points(trig, at, 0, 0, heading, body.hw, body.front, body.rear);
    if b.edge != IVec2::ZERO {
        return None;
    }
    for p in b.pts {
        let (pz, _, _) = collide::floor(city, p.x, p.y, z)?;
        if !near(pz) {
            return None;
        }
    }
    let mut glass = Vec::new();
    let hit = collide::vehicle_walls(city, &b, at, IVec3::new(from.x, from.y, z), &mut glass);
    if hit.x || hit.y || !glass.is_empty() {
        return None;
    }
    let r = body.hw.max(body.front).max(body.rear);
    let clear = others.iter().all(|(p, pr)| {
        let d = (*p - at).truncate().as_i64vec2();
        let m = (r + pr) as i64;
        d.length_squared() >= m * m
    });
    clear.then_some((z, level, water))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicles::TYPES;

    #[test]
    fn events_round_trip() {
        for c in [
            Cheat::God(true),
            Cheat::God(false),
            Cheat::VehicleGod(true),
            Cheat::VehicleGod(false),
            Cheat::AllWeapons,
            Cheat::Vehicle(11, 0),
            Cheat::Vehicle(27, 7),
            Cheat::Vehicle(0, 3),
            Cheat::Wanted(1),
            Cheat::Wanted(-1),
            Cheat::ClearWanted,
        ] {
            let e = c.event(42);
            assert_eq!(e.tick, 42);
            assert_eq!(Cheat::from_event(&e), Some(c));
        }
        // Files from before the colours: the type alone, black.
        assert_eq!(Cheat::from_event(&CheatEvent { tick: 1, kind: EV_VEHICLE, arg: 11 }), Some(Cheat::Vehicle(11, 0)));
        // Thrust frames, bad colours and unknown kinds are not cheats.
        assert_eq!(Cheat::from_event(&CheatEvent { tick: 1, kind: EV_VEHICLE, arg: 25 }), None);
        assert_eq!(Cheat::from_event(&CheatEvent { tick: 1, kind: EV_VEHICLE, arg: 11 | 8 << 8 }), None);
        assert_eq!(Cheat::from_event(&CheatEvent { tick: 1, kind: 99, arg: 0 }), None);
        assert_eq!(Cheat::parse("vehicle=23"), Some(Cheat::Vehicle(23, 0)));
        assert_eq!(Cheat::parse("vehicle=11:3"), Some(Cheat::Vehicle(11, 3)));
        assert_eq!(Cheat::parse("vehicle=11:8"), None);
        assert_eq!(Cheat::parse("nogod"), Some(Cheat::God(false)));
        assert_eq!(Cheat::parse("vgod"), Some(Cheat::VehicleGod(true)));
        assert_eq!(Cheat::parse("novgod"), Some(Cheat::VehicleGod(false)));
        assert_eq!(Cheat::parse("wanted+"), Some(Cheat::Wanted(1)));
        assert_eq!(Cheat::parse("wanted-"), Some(Cheat::Wanted(-1)));
        assert_eq!(Cheat::parse("wanted0"), Some(Cheat::ClearWanted));
    }

    #[test]
    fn pending_wanted() {
        let mut c = Cheats { wanted: 5, ..Default::default() };
        c.queue = vec![Cheat::Wanted(1), Cheat::Wanted(1), Cheat::AllWeapons];
        assert_eq!(c.wanted_pending(), MAX_WANTED);
        c.queue = vec![Cheat::ClearWanted, Cheat::Wanted(-1), Cheat::Wanted(1)];
        assert_eq!(c.wanted_pending(), 1);
    }

    #[test]
    fn vehicle_list() {
        assert_eq!(VEHICLES.len(), 26);
        for (i, &k) in VEHICLES.iter().enumerate() {
            // One name each, 2330..2355.
            assert_eq!(TYPES[k].name_phrase, 2330 + i as u32);
        }
    }

    #[test]
    fn pending_god() {
        let mut c = Cheats::default();
        assert!(!c.god_pending());
        c.queue.push(Cheat::God(true));
        c.queue.push(Cheat::AllWeapons);
        assert!(c.god_pending());
        c.god = true;
        c.queue = vec![Cheat::God(false)];
        assert!(!c.god_pending());
        // Vehicle god mode is separate.
        c.queue = vec![Cheat::VehicleGod(true)];
        assert!(c.god_pending());
        assert!(c.vehicle_god_pending());
        c.vehicle_god = true;
        c.queue = vec![Cheat::VehicleGod(false), Cheat::God(false)];
        assert!(!c.vehicle_god_pending());
        assert!(!c.god_pending());
    }
}
