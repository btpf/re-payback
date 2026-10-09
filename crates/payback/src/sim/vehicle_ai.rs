//! AI drivers (docs/re/vehicles.md §6; 68k `FUN_000e3a58` vehicle branch
//! ~124165-124810): the steer/throttle controller shared by script orders
//! and police pursuit, route-graph waypoint following (combat_police.md
//! §1), script vehicle orders, and the tank cannon (`FUN_000e2a50`).
//!
//! The original only drives vehicles in states 3 (traffic, `traffic.rs`),
//! 8-11 (chase/pursuit, `pursuit.rs`), 0xf/0x15 (goto) and 0x14 (flee by
//! route). Attack, follow, plain flee and goto-and-vanish are person-only
//! there; the port drives vehicles given those orders too (marked
//! "port" below).

use super::ai::{Ai, Order};
use super::driving::{Drive, DrivingSet};
use super::traffic::{LightPhase, TrafficSet};
use super::{Class, Obj, Rng, State, Stick, Trig, angle_diff, heading_of, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::effects::{Blend, Effects, Tex};
use crate::level::{AppState, LevelEntity, LevelSpec};
use crate::mission::{Triggers, kind};
use crate::objects::ObjectLibrary;
use crate::vehicles::{TYPES, Vehicle, VehicleType, kind as vk};
use crate::world::{City, object_to_render};
use bevy::prelude::*;
use payback_formats::map::MAP_SIZE;
use payback_formats::route::Route;

/// The current map's police routing graph (`Maps/<n>.route`, 68k
/// `FUN_000a4200`). Empty when the file is missing.
#[derive(Resource)]
pub struct RouteGraph(pub Route);

impl Default for RouteGraph {
    fn default() -> Self {
        RouteGraph(Route::empty())
    }
}

impl RouteGraph {
    /// Node position in object units (map units << 3).
    pub fn pos(&self, n: usize) -> IVec2 {
        let node = &self.0.nodes[n];
        IVec2::new((node.x as i32) << 3, (node.y as i32) << 3)
    }

    /// Nearest node to an object position (`FUN_000a42ac(x>>3, y>>3)`).
    pub fn nearest(&self, p: IVec3) -> Option<usize> {
        self.0.nearest(p.x >> 3, p.y >> 3)
    }

    pub fn is_empty(&self) -> bool {
        !self.0.nodes.iter().any(|n| n.used)
    }
}

/// Per-driver navigation state.
#[derive(Component, Clone, Debug)]
pub struct Nav {
    /// Own route node (`+0x3bc`, Mac `+0x89e`; `None` = 9999, take the
    /// nearest).
    pub node: Option<usize>,
    /// Goal node of a fleeing car: the node farthest from the player
    /// (`+0x3ba`, Mac `+0x89c`). Pursuers read the target's node from
    /// [`TargetNode`] instead.
    pub goal: Option<usize>,
    /// Steering target (`+0x2e6/+0x2ea`, Mac `+0x7c8/+0x7cc`). Every state
    /// shares it: traffic's lane choice moves it square by square, a car
    /// handed back to traffic keeps it (Mac `MoveUnit` @ 10081080 only
    /// re-targets once the car reaches it). New objects start with it on
    /// their own position (Mac `CreateObject` @ 10075630).
    pub aim: IVec2,
    /// Impatience timer (`+0x450`, Mac `+0x93c`).
    pub horn: i32,
    /// Honking (`_DAT_0034c3b4`, Mac `_DAT_1017cc70`) last tick.
    pub honking: bool,
}

impl Nav {
    /// A fresh driver aiming at its own position.
    pub fn at(pos: IVec3) -> Nav {
        Nav {
            node: None,
            goal: None,
            aim: pos.truncate(),
            horn: 0,
            honking: false,
        }
    }
}

/// The chase target's route node (`target+0x3be`, Mac `+0x8a0`) and its
/// refresh countdown (`+0x3c0`, Mac `+0x8a2`). The countdown belongs to the
/// target and drops by one per frame (Mac `ResetObject` @ 1006e490, 68k top
/// of `FUN_000e3a58`); the first pursuer to find it at 0 re-runs
/// `FindNearestNode` @ 100f11d0 and sets it to 20, so all pursuers share one
/// refresh every 20 frames (`do_Other` @ 10146280). A fleeing car (0x14)
/// re-picks its farthest node whenever the countdown is 0 but resets only
/// its own (unread) countdown, so with no pursuer it re-picks every frame.
/// Every object carries one (a new object starts at 0), so whichever object
/// is chased keeps its own cache (AI-2); the fleeing car's reset lands on
/// its own. The port's chases (route police, flight) all target the
/// player, as in the original (`+0x908` is the player for states 10/11;
/// nothing sets state 8).
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct TargetNode {
    pub node: Option<usize>,
    pub refresh: u8,
}

/// Driver parameters for [`controller`].
#[derive(Clone, Copy, Debug)]
pub struct Driver {
    /// `+0x346` skill 0..100.
    pub skill: i32,
    pub police: bool,
    /// `+0x49e`.
    pub direct: bool,
    /// Clamp the aim offset to ±1500 per axis (states 8/10/0xf/0x14/0x15
    /// without `+0x49e`): far targets are approached diagonally.
    pub clamp: bool,
}

/// What the look-ahead needs to know about the world.
pub struct Env<'a> {
    pub city: &'a City,
    pub lights: i32,
    /// Occupied squares: (square, cube level, object).
    pub occ: &'a [(IVec2, usize, Entity)],
}

/// The occupancy grid (`DAT_003178a0[x][y][level]`, Mac `_DAT_1017c490`):
/// every object marks each square its box covers, at cube level
/// `min(level, 2)` (Mac `AddCollideObj` @ 10055c70, 68k `FUN_000cdfdc`,
/// re-run for each object after its update in `object_MainLoop`
/// @ 1014a670). The box is the axis-aligned bound of the four corners
/// (±radius across, `-rear`/`front` along the heading); a person's corners
/// all sit at `+radius` along (sic: both "along" values are the radius).
/// A player's person model (`+0x8160`) marks one more square each side.
/// Projectiles are objects too and count.
pub fn occupancy<'a>(
    objs: impl Iterator<Item = (Entity, &'a Obj)>,
    library: &ObjectLibrary,
    trig: &Trig,
) -> Vec<(IVec2, usize, Entity)> {
    let mut out = Vec::new();
    for (e, o) in objs {
        if o.state == State::Removed {
            continue;
        }
        let info = &library.model(o.model).info;
        let person = o.class == Class::Person;
        let r = info.radius;
        let (back, front) = if person { (r, r) } else { (-info.min_y, info.max_y) };
        let (s, c) = (trig.sin(o.heading), trig.cos(o.heading));
        let (mut lo, mut hi) = (IVec2::MAX, IVec2::MIN);
        for along in [back, front] {
            for lat in [r, -r] {
                let p = IVec2::new(along * s + lat * c, along * c - lat * s);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        let sq = |v: IVec2| IVec2::new((v.x >> 8) + o.pos.x >> 11, (v.y >> 8) + o.pos.y >> 11);
        let grow = if person && o.player.is_some() { 1 } else { 0 };
        let top = MAP_SIZE as i32 - 1;
        let lo = (sq(lo) - grow).clamp(IVec2::ZERO, IVec2::splat(top));
        let hi = (sq(hi) + grow).clamp(IVec2::ZERO, IVec2::splat(top));
        let level = o.level.min(2);
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                out.push((IVec2::new(x, y), level, e));
            }
        }
    }
    out
}

/// Cruise band (lo, hi) by skill: below lo accelerate, above hi brake
/// (68k ~124595-124730). `steering` = the controller asked to turn.
fn speed_band(t: &VehicleType, d: &Driver, steering: bool) -> (i32, i32) {
    let corner = steering && !d.direct;
    let band = |v: i32, lo: i32| ((v - t.accel).max(lo), v + t.brake);
    if d.skill >= 0x62 && (d.police || d.direct) {
        // Uncapped branch (pursuit at wanted 6, race drivers).
        return if corner {
            band(0xf0, 0x78)
        } else {
            band(0x2da, 500)
        };
    }
    match d.skill {
        0x5f.. => {
            if corner {
                band(200, 100)
            } else {
                band(0x14a, 0xdc)
            }
        }
        0x5b..=0x5e => {
            if corner {
                band(0xb4, 0x5a)
            } else {
                band(0xe6, 0x96)
            }
        }
        0x33..=0x5a => {
            if corner {
                band(0x8c, 0x46)
            } else {
                band(200, 100)
            }
        }
        // The slowest drivers use the brake value on both sides (sic).
        _ => {
            let (v, lo) = if corner { (100, 0x32) } else { (0x96, 0x50) };
            ((v - t.brake).max(lo), v + t.brake)
        }
    }
}

/// One tick of the AI driver (68k ~124391-124810): steer toward `aim`,
/// look ahead for obstacles and red lights, pick throttle/brake from the
/// skill's cruise band, honk when stuck. `fire` is the horn/fire input
/// (`_DAT_0034c3b4`; on the tank it fires the cannon).
#[allow(clippy::too_many_arguments)]
pub fn controller(
    e: Entity,
    obj: &Obj,
    t: &VehicleType,
    front: i32,
    drive: &Drive,
    d: Driver,
    aim: IVec2,
    env: &Env,
    nav: &mut Nav,
    rng: &mut Rng,
    trig: &Trig,
) -> Stick {
    let pos = obj.pos.truncate();
    let mut v = aim - pos;
    if d.clamp && !d.direct {
        v = v.clamp(IVec2::splat(-0x5dc), IVec2::splat(0x5dc));
    }
    let want = heading_of(v.x, v.y);
    // Dead zone: half a tick's maximum yaw. The two ways round are
    // compared as in `do_Other` @ 10146280 (68k ~124420): turn up when the
    // short way is up and longer than the dead zone, down when the down way
    // is at least the dead zone (ties go down).
    let dead = t.turn >> 1;
    let up = wrap(want - obj.heading);
    let down = wrap(obj.heading - want);
    let mut steer = if up < down {
        if dead < up { -1 } else { 0 }
    } else if dead <= down {
        1
    } else {
        0
    };

    // Look-ahead along heading + 2 ticks of yaw; skipped by reckless
    // drivers (skill >= 99, health >= 50), who ram.
    let mut blocked = false;
    if d.skill < 99 || obj.health < 50 {
        let look = obj.heading + obj.turn_rate * 2;
        let (s, c) = (trig.sin(look), trig.cos(look));
        let reach = (front + obj.speed * obj.speed * 4 / t.brake.max(1) + 0x400).max(0x800);
        let mut sq = IVec2::new(pos.x >> 11, pos.y >> 11);
        let mut dist = 0x800;
        while dist <= reach {
            sq = IVec2::new(
                (pos.x + (dist * s >> 8)) >> 11,
                (pos.y + (dist * c >> 8)) >> 11,
            );
            let inside =
                (0..MAP_SIZE as i32).contains(&sq.x) && (0..MAP_SIZE as i32).contains(&sq.y);
            if !inside
                || env
                    .occ
                    .iter()
                    .any(|(q, l, oe)| *q == sq && *l == obj.level && *oe != e)
            {
                blocked = true;
                break;
            }
            dist += 0x800;
        }
        // Red lights stop only drivers below skill 70.
        if (0..MAP_SIZE as i32).contains(&sq.x) && (0..MAP_SIZE as i32).contains(&sq.y) {
            let special =
                env.city.map.square(sq.x as usize, sq.y as usize).cubes[obj.level].special as i32;
            if (1..=6).contains(&special) {
                let mut l = special + env.lights;
                if l > 6 {
                    l -= 6;
                }
                if d.skill < 0x46 && matches!(l, 1 | 2 | 3 | 6) {
                    blocked = true;
                }
            }
        }
    }

    nav.horn = (nav.horn - 1).max(0);
    let mut honk = false;
    let mut throttle = 0;
    if !blocked {
        let (lo, hi) = speed_band(t, &d, steer != 0);
        if obj.speed < lo {
            throttle = -1;
        } else if obj.speed > hi {
            throttle = 1;
        }
    } else {
        if !d.police && !d.direct {
            // Impatience: honk, and every so often push on regardless.
            let (base, range, honk_above) = match d.skill {
                91.. => (20, 20, 10),
                61..=90 => (50, 50, 40),
                31..=60 => (100, 50, 80),
                _ => (100, 50, i32::MAX),
            };
            if nav.horn == 0 {
                honk = d.skill >= 31;
                nav.horn = rng.rand(range) + base;
                blocked = false;
                throttle = -1;
            } else if nav.horn > honk_above {
                honk = true;
            }
        }
        if blocked {
            if obj.speed > t.brake >> 1 {
                throttle = 1;
            } else if obj.speed < -t.accel {
                throttle = -1;
            }
        }
    }

    // Don't steer into a handbrake spin. The Mac (`do_Other` @ 10146280)
    // only cuts low-skill steering while the J-turn latch is set; the Amiga
    // (68k ~124855, OS4 ~1138) also cut it while braking or reversing
    // (`|| throttle == 1`), so slow drivers backed out straight.
    if d.skill < 0x51 {
        if drive.j_turn {
            steer = 0;
        }
    } else if drive.j_turn && ((obj.turn_rate < 0 && steer > 0) || (obj.turn_rate > 0 && steer < 0))
    {
        steer = 0;
    }
    Stick {
        steer,
        throttle,
        fire: honk,
        ..default()
    }
}

/// Route following for states 8/10/0x14 (Mac `do_Other` @ 10146280, 68k
/// ~124213-124290): returns true on reaching `goal` (state 8 -> 9,
/// 10 -> 11). `pursuit` selects the node arrival radius of states 8/10.
pub fn follow_route(
    route: &RouteGraph,
    nav: &mut Nav,
    obj: &Obj,
    goal: Option<usize>,
    pursuit: bool,
) -> bool {
    let Some(node) = nav.node else {
        // 9999: take the nearest node to self.
        nav.node = route.nearest(obj.pos);
        if let Some(n) = nav.node {
            nav.aim = route.pos(n);
        }
        return false;
    };
    let Some(goal) = goal else {
        return false;
    };
    // Reached when |d| < r on both axes. Pursuit (states 8/10) uses the
    // Mac radius `300 + speed²>>7`; the Amiga's `300 + |5·speed|` (68k
    // ~124245) reached nodes far too early at speed. Flee (0x14) keeps
    // `300 + |5·speed|` on the Mac too.
    let r = if pursuit {
        300 + (obj.speed * obj.speed >> 7)
    } else {
        300 + (obj.speed * 5).abs()
    };
    let d = nav.aim - obj.pos.truncate();
    if d.x.abs() < r && d.y.abs() < r {
        if node == goal {
            return true;
        }
        nav.node = route.0.next(node, goal);
        match nav.node {
            Some(n) => nav.aim = route.pos(n),
            None => return false,
        }
    }
    false
}

/// The horn (Mac `DoVehicleFire` @ 10070c20, 68k `FUN_000e2a50`, OS4
/// `FUN_0105e02c`), for AI drivers and the player alike: while the
/// fire/horn input is on, a looped horn sound (`+0x8178`, 68k `+0x491a`) at
/// volume 20e6 and `clamp(16e6 / mass, 3000, 20000)` Hz (a police model:
/// its horn is the siren, 25e6 at 10000 Hz) is started into the object's
/// slot `+0x918` and then follows the car (`ModifySound`). It is deleted
/// the first update the input is off (`do_Other` @ 10146280 tail:
/// `DeleteSound(+0x918)`); here [`super::driving`]'s `horns` does that for
/// every vehicle that did not honk this tick. The tank fires its gun instead
/// (`+0x8164`, `tank_cannon`) and the bomb truck launches its RC car
/// (`+0x815c`, not in the port): neither honks. The sound is a child of the
/// car, so it goes with it.
pub fn horn(
    commands: &mut Commands,
    sounds: &mut Query<&mut Sound>,
    drive: &mut Drive,
    e: Entity,
    veh: &Vehicle,
    obj: &Obj,
    honk: bool,
) {
    if !honk || matches!(veh.kind, vk::TANK | vk::BOMB_TRUCK) {
        return;
    }
    drive.honked = true;
    let at = SoundPos::At(obj.posf());
    if let Some(mut snd) = drive.horn.and_then(|s| sounds.get_mut(s).ok()) {
        snd.pos = at;
        return;
    }
    let t = &TYPES[veh.kind];
    let (vol, hz) = if t.police {
        (25_000_000.0, 10_000.0)
    } else {
        (20_000_000.0, (16_000_000 / t.mass.max(1)).clamp(3000, 20000) as f32)
    };
    drive.horn = Some(commands.spawn((Sound::looping(t.horn, vol, hz, at), ChildOf(e))).id());
}

/// Script orders for vehicles.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn vehicle_orders(
    mut commands: Commands,
    city: Option<Res<City>>,
    route: Res<RouteGraph>,
    lights: Res<LightPhase>,
    trig: Res<Trig>,
    library: Option<Res<ObjectLibrary>>,
    mut rng: ResMut<Rng>,
    mut triggers: ResMut<Triggers>,
    mut tnodes: Query<&mut TargetNode>,
    mut sounds: Query<&mut Sound>,
    player: Query<Entity, With<CameraTarget>>,
    mut set: ParamSet<(
        Query<(Entity, &Obj)>,
        Query<
            (
                Entity,
                &mut Obj,
                &Vehicle,
                &mut Ai,
                &mut Stick,
                &mut Drive,
                Option<&mut Nav>,
            ),
            Without<CameraTarget>,
        >,
    )>,
) {
    let (Some(city), Some(library)) = (city, library) else {
        return;
    };
    let snap: Vec<(Entity, IVec3, IVec3)> =
        set.p0().iter().map(|(e, o)| (e, o.pos, o.vel)).collect();
    let occ = occupancy(set.p0().iter(), &library, &trig);
    let env = Env {
        city: &city,
        lights: lights.phase,
        occ: &occ,
    };
    let player = player.single().ok();
    let find = |e: Option<Entity>| -> Option<(IVec3, IVec3)> {
        let e = e.or(player)?;
        snap.iter().find(|s| s.0 == e).map(|s| (s.1, s.2))
    };
    let player_pos = find(None).map(|p| p.0);
    let player_refresh = player.and_then(|p| tnodes.get(p).ok()).map(|t| t.refresh);
    let order = crate::sim::table_order(set.p1().iter().map(|(e, o, ..)| (e, o)));
    let mut objs = set.p1();
    for e__ in order {
        let Ok((e, mut obj, veh, mut ai, mut stick, mut drive, nav)) = objs.get_mut(e__) else { continue };
        if matches!(
            obj.state,
            State::Player
                | State::Dead
                | State::Removed
                | State::Parked
                | State::Wander
                | State::PursuitRoute
                | State::Pursuit
        ) || obj.burn > 0
        {
            continue;
        }
        let Some(mut nav) = nav else {
            commands.entity(e).insert(Nav::at(obj.pos));
            continue;
        };
        let t = &TYPES[veh.kind];
        let front = library.model(obj.model).info.max_y;
        let mut driver = Driver {
            skill: ai.aggro,
            police: t.police,
            direct: ai.direct,
            clamp: true,
        };
        let pos = obj.pos.truncate();
        // Bonnet point: 3/4 of the front length ahead (68k ~124370).
        let bonnet = pos
            + IVec2::new(
                trig.sin(obj.heading) * front * 3 >> 10,
                trig.cos(obj.heading) * front * 3 >> 10,
            );
        let mut aim = None;
        let mut tank_fire = false;
        match ai.order.as_goto() {
            Order::Goto(goal, radius) => {
                let d = (bonnet - goal).abs();
                if d.x + d.y <= radius * 2 {
                    // Arrived: state 4, trigger 0x17 (`FUN_000a6640`).
                    triggers.report_actor(kind::ACTOR_GOTO, e);
                    ai.order = Order::Idle;
                    obj.state = State::Idle;
                } else {
                    aim = Some(goal);
                }
            }
            Order::GotoVanish(goal) => {
                // Port: state 0xe is person-only in the original.
                let d = (bonnet - goal).abs();
                if d.x + d.y < 0x579 * 2 {
                    triggers.report_actor(kind::ACTOR_GOTO_VANISH, e);
                    obj.state = State::Removed;
                    if let Some(s) = veh.engine {
                        commands.entity(s).despawn();
                    }
                    commands.entity(e).despawn();
                    continue;
                }
                aim = Some(goal);
            }
            Order::FleeFar | Order::Flee => {
                // State 0x14: drive the route toward the node farthest from
                // the player, re-chosen while the player's node countdown
                // is 0 (`do_Other` @ 10146280, 68k ~124330; see
                // [`TargetNode`]). (Plain flee 0x12 is person-only; the port
                // flees by route.)
                if let Some(pp) = player_pos {
                    if route.is_empty() {
                        aim = Some(
                            pos + (pos - pp.truncate())
                                .clamp(IVec2::splat(-0x5dc), IVec2::splat(0x5dc)),
                        );
                    } else {
                        if player_refresh == Some(0) || nav.goal.is_none() {
                            nav.goal = route.0.farthest(pp.x >> 3, pp.y >> 3);
                            // Resets its own countdown (Mac `+0x8a2` of the
                            // fleeing car, not the player's).
                            if let Ok(mut own) = tnodes.get_mut(e) {
                                own.refresh = 20;
                            }
                        }
                        // On the farthest node the original drops into
                        // state 11 (straight chase); the port waits there.
                        let goal = nav.goal;
                        if !follow_route(&route, &mut nav, &obj, goal, false) {
                            aim = Some(nav.aim);
                        }
                    }
                }
            }
            Order::Attack { target, .. } => {
                // Port: ram the target like a state 9 chaser; the tank also
                // shoots (guards' rule: within 6000, rand(101-aggro) < 1).
                if let Some((tp, _)) = find(target) {
                    driver.clamp = false;
                    aim = Some(tp.truncate());
                    let d = (tp - obj.pos).truncate().abs();
                    let facing =
                        angle_diff(obj.heading, heading_of(tp.x - obj.pos.x, tp.y - obj.pos.y))
                            .abs()
                            < 0x100;
                    if veh.kind == vk::TANK && (d.x + d.y) / 2 < 6000 && facing {
                        tank_fire = rng.rand((101 - ai.aggro).max(1)) < 1;
                    }
                }
            }
            Order::Idle | Order::Park | Order::BoardPlayer | Order::BoardActor(_) => {
                // Port: follow the leader at a distance.
                if let Some((lp, _)) = ai.follow.and_then(|l| find(Some(l))) {
                    let d = (lp - obj.pos).truncate().abs();
                    if d.x + d.y > 0xc00 {
                        driver.clamp = false;
                        aim = Some(lp.truncate());
                    }
                }
            }
            // (`as_goto` folds 0x15 into `Goto`.)
            Order::Ambient | Order::Follow(_) | Order::Goto15(_) => {}
        }
        let attacking = matches!(ai.order, Order::Attack { .. });
        *stick = match aim {
            Some(aim) => {
                nav.aim = aim;
                let s = controller(
                    e, &obj, t, front, &drive, driver, aim, &env, &mut nav, &mut rng, &trig,
                );
                nav.honking = s.fire;
                horn(&mut commands, &mut sounds, &mut drive, e, veh, &obj, s.fire);
                s
            }
            // No AI input: the car coasts (state 4).
            None => Stick::default(),
        };
        if veh.kind == vk::TANK && attacking {
            // An attacking tank fires on its own rule, not on honks.
            stick.fire = tank_fire;
        }
        // Script speed cap (`+0x386`).
        if obj.speed > ai.max_speed {
            obj.speed = ai.max_speed;
        }
    }
}

/// Press-edge latch for the tank cannon.
#[derive(Component, Default)]
pub struct GunLatch(bool);

/// Tank cannon (68k `FUN_000e2a50`, OS4 `FUN_0105e02c`): on a fire press
/// edge, a no-gravity missile 1280 units ahead at tank v + 512/tick,
/// BigGunFire, and a dust puff. Unlimited ammo.
#[allow(clippy::type_complexity)]
fn tank_cannon(
    mut commands: Commands,
    data: Res<GameData>,
    trig: Res<Trig>,
    mut library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut effects: ResMut<Effects>,
    mut crimes: Option<ResMut<super::police::Crimes>>,
    mut q: Query<(
        Entity,
        &Obj,
        &Vehicle,
        &Stick,
        Option<&mut GunLatch>,
        Has<CameraTarget>,
    )>,
) {
    let Some(library) = library.as_deref_mut() else {
        return;
    };
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, obj, veh, stick, latch, is_player)) = q.get_mut(e__) else { continue };
        if veh.kind != vk::TANK {
            continue;
        }
        let Some(mut latch) = latch else {
            commands.entity(e).insert(GunLatch(stick.fire));
            continue;
        };
        let pressed = stick.fire && !latch.0;
        latch.0 = stick.fire;
        if !pressed || obj.health <= 0 || matches!(obj.state, State::Dead | State::Removed) {
            continue;
        }
        let dir = IVec3::new(trig.sin(obj.heading), trig.cos(obj.heading), 0);
        let pos = obj.pos + dir * 5 + IVec3::new(0, 0, -50);
        let Some(model) = library.load(&data, &mut meshes, "weapons/missile.obj") else {
            continue;
        };
        let mut m = Obj::new(Class::Projectile, model, pos, obj.heading);
        // Horizontal only: a car's vz holds the terrain slope (+0x80).
        m.vel = IVec3::new(obj.vel.x, obj.vel.y, 0) + dir * 2;
        m.state = State::Parked;
        m.level = obj.level;
        m.owner = Some(e);
        m.side = 1; // owner side 1 (`DoVehicleFire` @ 10070c20)
        commands.spawn((
            m,
            super::combat::Projectile {
                kind: super::combat::ProjectileKind::Missile,
                shooter: Some(e),
                by_player: is_player,
                player: is_player.then(|| obj.player.unwrap_or(0)),
            },
            Mesh3d(library.model(model).mesh.clone()),
            MeshMaterial3d(library.material.clone()),
            Transform::from_translation(object_to_render(pos.as_vec3())),
            LevelEntity,
        ));
        // Firearms crime +1 (Mac `DoVehicleFire` @ 10070c20).
        if let (true, Some(c)) = (is_player, crimes.as_deref_mut()) {
            c.firearms += 1;
        }
        commands.spawn(Sound::once(
            id::BIG_GUN_FIRE,
            15e6,
            9000.0,
            SoundPos::At(obj.posf()),
        ));
        // 8 dust puffs at the muzzle, fanned out (no RNG: keeps this
        // system order-independent).
        for k in 0..8 {
            let a = obj.heading + (k - 4) * 0x80;
            let v = Vec3::new(trig.sin(a) as f32 / 8.0, trig.cos(a) as f32 / 8.0, -10.0);
            effects.add(
                3,
                Tex::Dust,
                pos.as_vec3(),
                v,
                600.0 + 50.0 * k as f32,
                obj.level as u8 + 1,
                Blend::Add,
            );
        }
    }
}

/// Every object's node countdown drops by one per frame (Mac
/// `ResetObject` @ 1006e490, called for each object not in state 6 by
/// `object_MainLoop` @ 1014a670).
fn tick_target_node(mut q: Query<(&Obj, &mut TargetNode)>) {
    for (o, mut t) in &mut q {
        if o.state != State::Removed {
            t.refresh = t.refresh.saturating_sub(1);
        }
    }
}

fn load_route(mut commands: Commands, data: Res<GameData>, spec: Res<LevelSpec>) {
    let path = spec.path("route");
    let route = data
        .read(&path)
        .ok()
        .and_then(|b| Route::parse(&b).ok())
        .unwrap_or_else(Route::empty);
    info!(
        "{}: {} route nodes",
        path.display(),
        route.nodes.iter().filter(|n| n.used).count()
    );
    commands.insert_resource(RouteGraph(route));
}

/// `--ai-test` (debug): a race car (skill 99, direct) next to the player
/// drives a chain of gotos along route-graph nodes, logging arrivals.
#[derive(Resource)]
struct AiTest {
    car: Option<Entity>,
    path: Vec<usize>,
    leg: usize,
    ticks: u32,
}

fn ai_test(world: &mut World) {
    let Some(mut test) = world.remove_resource::<AiTest>() else {
        return;
    };
    test.ticks += 1;
    let player = world
        .query_filtered::<&Obj, With<CameraTarget>>()
        .iter(world)
        .next()
        .map(|o| o.pos);
    match test.car {
        None => {
            let (Some(pp), true) = (player, world.contains_resource::<City>()) else {
                world.insert_resource(test);
                return;
            };
            let route = world.resource::<RouteGraph>();
            let Some(mut n) = route.nearest(pp) else {
                world.insert_resource(test);
                return;
            };
            let start = route.pos(n);
            // Walk the graph without turning back, 12 hops.
            let mut path: Vec<usize> = Vec::new();
            let mut prev = None;
            for _ in 0..12 {
                let node = &route.0.nodes[n];
                let Some(next) = node
                    .targets()
                    .find(|&t| Some(t) != prev)
                    .or_else(|| node.targets().next())
                else {
                    break;
                };
                path.push(next);
                prev = Some(n);
                n = next;
            }
            let pos = start.extend(pp.z - 0x200);
            if let Some(e) = crate::vehicles::spawn_vehicle_world(world, 27, pos, 0, 1) {
                super::ai::set_aggro(world, e, 99);
                super::ai::set_direct(world, e);
                info!("ai-test: car {e:?} at {start}, path {path:?}");
                test.car = Some(e);
                test.path = path;
                test.leg = usize::MAX;
            }
        }
        Some(e) => {
            let Some(ai) = world.get::<Ai>(e) else {
                world.insert_resource(test);
                return;
            };
            let idle = ai.order == Order::Idle;
            let (pos, speed) = world
                .get::<Obj>(e)
                .map_or((IVec3::ZERO, 0), |o| (o.pos, o.speed));
            if test.ticks % 25 == 0 {
                info!(
                    "ai-test: t={} pos={pos} speed={speed} leg={}",
                    test.ticks, test.leg
                );
            }
            if idle {
                test.leg = test.leg.wrapping_add(1);
                if let Some(&n) = test.path.get(test.leg) {
                    let goal = world.resource::<RouteGraph>().pos(n);
                    info!(
                        "ai-test: t={} leg {} -> node {n} at {goal}",
                        test.ticks, test.leg
                    );
                    super::ai::give_order(world, e, Order::Goto(goal, 1000));
                } else if test.leg == test.path.len() {
                    info!("ai-test: t={} route done", test.ticks);
                }
            }
        }
    }
    world.insert_resource(test);
}

/// The vehicle AI chain: after traffic, before the driving physics.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VehicleAiSet;

pub struct VehicleAiPlugin;

impl Plugin for VehicleAiPlugin {
    fn build(&self, app: &mut App) {
        if crate::args().any(|a| a == "--ai-test") {
            app.insert_resource(AiTest {
                car: None,
                path: Vec::new(),
                leg: 0,
                ticks: 0,
            });
        }
        app.init_resource::<RouteGraph>()
            .add_systems(OnEnter(AppState::Playing), load_route)
            .configure_sets(
                FixedUpdate,
                VehicleAiSet.after(TrafficSet).before(DrivingSet),
            )
            .add_systems(
                FixedUpdate,
                tick_target_node
                    .in_set(VehicleAiSet)
                    .before(super::pursuit::PursuitSet)
                    .before(vehicle_orders),
            )
            .add_systems(FixedUpdate, vehicle_orders.in_set(VehicleAiSet))
            .add_systems(
                FixedUpdate,
                ai_test
                    .run_if(resource_exists::<AiTest>)
                    .in_set(VehicleAiSet)
                    .before(vehicle_orders),
            )
            .add_systems(
                FixedUpdate,
                tank_cannon
                    .after(DrivingSet)
                    .before(super::combat::CombatSet),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(kind: usize) -> &'static VehicleType {
        &TYPES[kind]
    }

    #[test]
    fn cruise_bands() {
        let d = |skill, police, direct| Driver {
            skill,
            police,
            direct,
            clamp: true,
        };
        // Police at wanted 6 (skill 98+): uncapped 730 on straights.
        let t = ty(vk::POLICE);
        assert_eq!(
            speed_band(t, &d(98, true, false), false),
            (730 - 17, 730 + 40)
        );
        assert_eq!(
            speed_band(t, &d(98, true, false), true),
            (240 - 17, 240 + 40)
        );
        // Civilians never take the uncapped branch; race drivers do and
        // ignore corners.
        assert_eq!(
            speed_band(t, &d(99, false, false), false),
            (330 - 17, 330 + 40)
        );
        assert_eq!(
            speed_band(t, &d(99, false, true), true),
            (730 - 17, 730 + 40)
        );
        // Timid drivers use the brake value on both sides.
        let m = ty(0);
        assert_eq!(
            speed_band(m, &d(10, false, false), false),
            (150 - 18, 150 + 18)
        );
        assert_eq!(
            speed_band(m, &d(60, false, false), true),
            (140 - 5, 140 + 18)
        );
    }
}
