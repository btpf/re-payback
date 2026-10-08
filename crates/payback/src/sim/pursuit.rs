//! Police cars (docs/re/combat_police.md §2.2-2.3; 68k `FUN_000e3a58`
//! ~124171-124590 pursuit, `FUN_000e2a50` siren, `FUN_000cfe0c` roadblocks,
//! `FUN_000dc5ac` despawn).
//!
//! Police cars in traffic switch to pursuit at wanted >= 3: they follow the
//! route graph to the player's nearest node (state 10), then drive straight
//! at the point `(5 - aggr)·3` ticks of the player's velocity *behind* the
//! player (state 11). Near a slow player they stop and drop a shotgun cop.
//! At wanted >= 5 roadblocks of parked police cars, barriers and cops appear
//! at the window edge (spawned with traffic, `traffic.rs`).

use super::ai::{Ai, Order};
use super::combat::{Arsenal, Weapon};
use super::driving::Drive;
use super::people::{self, Ambient};
use super::police::Aggressiveness;
use super::traffic::{self, LightPhase};
use super::vehicle_ai::{self, Driver, Env, Nav, RouteGraph, TargetNode, VehicleAiSet};
use super::{Obj, Rng, State, Stick, Trig, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::{CameraTarget, GameCam};
use crate::data::GameData;
use crate::objects::ObjectLibrary;
use crate::vehicles::{TYPES, Vehicle, kind as vk};
use crate::world::City;
use bevy::prelude::*;

/// A police car in pursuit (states 10/11).
#[derive(Component, Clone, Copy, Debug)]
pub struct Pursuit {
    /// `+0x346 = wanted + (aggr < 4 ? 0x5c : 0x5e)`.
    pub skill: i32,
}

/// Siren loop and light-bar flash (`+0x454`, 0..9).
#[derive(Component, Default)]
pub struct Siren {
    sound: Option<Entity>,
    flash: i32,
}

/// Part of a roadblock (despawned with the window like other drones).
#[derive(Component)]
pub struct Roadblock;

/// The pursuit chain (inside [`VehicleAiSet`]).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PursuitSet;

/// `--wanted N` (debug): hold the player's wanted level at N.
#[derive(Resource, Clone, Copy)]
pub struct HoldWanted(pub Option<i32>);

pub struct PursuitPlugin;

impl Plugin for PursuitPlugin {
    fn build(&self, app: &mut App) {
        let hold = std::env::args()
            .skip_while(|a| a != "--wanted")
            .nth(1)
            .and_then(|v| v.parse().ok());
        app.insert_resource(HoldWanted(hold)).add_systems(
            FixedUpdate,
            (hold_wanted, select_pursuers, pursue, sirens, despawn_far)
                .chain()
                // Route police are unused in multiplayer.
                .distributive_run_if(crate::level::single_player)
                .in_set(VehicleAiSet)
                .in_set(PursuitSet)
                .before(vehicle_ai::vehicle_orders),
        );
    }
}

fn hold_wanted(hold: Res<HoldWanted>, mut q: Query<&mut Obj, With<CameraTarget>>) {
    if let Some(w) = hold.0 {
        for mut p in &mut q {
            p.wanted = w;
        }
    }
}

fn player_wanted(q: &Query<&Obj, With<CameraTarget>>) -> i32 {
    q.iter().next().map_or(0, |p| p.wanted)
}

/// Pursuit selection (68k ~124185): police cars except vans, in traffic
/// or pursuit, follow the player at wanted >= 3 and return to traffic
/// below it.
#[allow(clippy::type_complexity)]
fn select_pursuers(
    mut commands: Commands,
    aggr: Option<Res<Aggressiveness>>,
    player: Query<&Obj, With<CameraTarget>>,
    mut q: Query<
        (
            Entity,
            &mut Obj,
            &Vehicle,
            Option<&mut Ai>,
            Option<&mut Pursuit>,
            Option<&traffic::Traffic>,
            Has<Nav>,
        ),
        Without<CameraTarget>,
    >,
) {
    let aggr = aggr.map_or(2, |a| a.0);
    let wanted = player_wanted(&player);
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, veh, mut ai, pursuit, tr, has_nav)) = q.get_mut(e__) else { continue };
        if !TYPES[veh.kind].police || veh.kind == vk::POLICE_VAN {
            continue;
        }
        // Script drive-to 0x15 and flee 0x14 are in the original's list
        // too (68k ~124171: states 3/8/9/10/11/0x14/0x15, not 0xf), so a
        // scripted police car leaves its order: traffic below wanted 3,
        // pursuit from 3. The port runs script orders in State::Idle.
        let order = ai.as_ref().map(|a| a.order);
        let scripted =
            matches!(order, Some(Order::Goto15(_) | Order::FleeFar)) && obj.state == State::Idle;
        if !(scripted
            || matches!(
                obj.state,
                State::Wander | State::PursuitRoute | State::Pursuit
            ) && order.is_none_or(|o| o == Order::Ambient))
            || obj.burn > 0
        {
            continue;
        }
        if let (true, Some(a)) = (scripted, ai.as_mut()) {
            info!("police {e:?}: script order {order:?} dropped (wanted {wanted})");
            // The state byte is overwritten, so the order is gone and its
            // trigger never fires.
            a.order = Order::Ambient;
            // Fall into the branches below as if from traffic / pursuit.
            obj.state = if wanted < 3 { State::PursuitRoute } else { State::Wander };
        }
        if wanted < 3 {
            if obj.state != State::Wander {
                info!("police {e:?}: pursuit off");
                obj.state = State::Wander;
                // Only the state byte changes: the skill stays the
                // pursuit's (`+0x346`), the steering target stays where it
                // was until the lane code reaches it (`do_Other`
                // @ 10146280).
                let skill = pursuit.map_or(95, |p| p.skill);
                let dir = tr.map_or(0, |t| t.dir);
                let mut ent = commands.entity(e);
                ent.remove::<Pursuit>().insert(traffic::resume(skill, dir));
                if !has_nav {
                    ent.insert(Nav::at(obj.pos));
                }
            }
            continue;
        }
        let skill = wanted + if aggr < 4 { 0x5c } else { 0x5e };
        match pursuit {
            Some(mut p) => p.skill = skill,
            None => {
                // The route node `+0x3bc` and steering target carry over
                // from whatever the car did before.
                let mut ent = commands.entity(e);
                ent.insert(Pursuit { skill });
                if !has_nav {
                    ent.insert(Nav::at(obj.pos));
                }
            }
        }
        if obj.state == State::Wander {
            info!("police {e:?}: pursuit on (skill {skill})");
            obj.state = State::PursuitRoute;
        }
    }
}

/// Drive pursuing cars (states 10/11) and drop off a cop near a slow
/// player (68k ~124213-124590).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pursue(
    mut commands: Commands,
    city: Option<Res<City>>,
    route: Res<RouteGraph>,
    lights: Res<LightPhase>,
    trig: Res<Trig>,
    data: Res<GameData>,
    aggr: Option<Res<Aggressiveness>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: ResMut<Rng>,
    mut target: ResMut<TargetNode>,
    mut set: ParamSet<(
        Query<(Entity, &Obj)>,
        Query<(Entity, &Obj), With<CameraTarget>>,
        Query<
            (
                Entity,
                &mut Obj,
                &Vehicle,
                &Pursuit,
                &mut Nav,
                &mut Stick,
                &mut Drive,
            ),
            Without<CameraTarget>,
        >,
    )>,
) {
    let (Some(city), Some(mut library)) = (city, library) else {
        return;
    };
    let aggr = aggr.map_or(2, |a| a.0);
    let occ = vehicle_ai::occupancy(set.p0().iter(), &library, &trig);
    let Some((ppos, pvel, pspeed)) = set.p1().iter().next().map(|(_, o)| (o.pos, o.vel, o.speed))
    else {
        return;
    };
    let env = Env {
        city: &city,
        lights: lights.phase,
        occ: &occ,
    };
    let order = crate::sim::table_order(set.p2().iter().map(|(e, o, ..)| (e, o)));
    let mut objs = set.p2();
    for e__ in order {
        let Ok((e, mut obj, veh, pursuit, mut nav, mut stick, mut drive)) = objs.get_mut(e__) else { continue };
        if !matches!(obj.state, State::PursuitRoute | State::Pursuit) {
            continue;
        }
        let t = &TYPES[veh.kind];
        let front = library.model(obj.model).info.max_y;
        if obj.state == State::PursuitRoute {
            if route.is_empty() {
                // No graph: straight at the player ("erratic").
                obj.state = State::Pursuit;
            } else {
                // The player's nearest node, refreshed by the first pursuer
                // to find the player's countdown at 0 (`do_Other`
                // @ 10146280; [`TargetNode`]).
                if target.refresh == 0 {
                    target.node = route.nearest(ppos);
                    target.refresh = 20;
                }
                if vehicle_ai::follow_route(&route, &mut nav, &obj, target.node, true) {
                    info!("police {e:?}: on the player's node, direct chase");
                    obj.state = State::Pursuit;
                }
            }
        }
        if obj.state == State::Pursuit {
            // `target + v·(5 − aggr)·−3` (Mac `do_Other` @ 10146280 l.118091,
            // 68k ~124290): objects move by `pos += v` (Mac
            // `MoveObject_Vehicle` @ 1006e890, `MoveObject_General`
            // @ 1006e4f0), so this is a point *behind* the player, which
            // the police follow rather than cut off (ahead only at aggr 6).
            nav.aim = (ppos - pvel * ((5 - aggr) * 3)).truncate();
        }
        let driver = Driver {
            skill: pursuit.skill,
            police: true,
            direct: false,
            clamp: obj.state == State::PursuitRoute,
        };
        let aim = nav.aim;
        *stick = vehicle_ai::controller(
            e, &obj, t, front, &drive, driver, aim, &env, &mut nav, &mut rng, &trig,
        );
        stick.fire = false;

        // Drop off a cop: close, and both nearly stopped or the car hurt.
        let d = (obj.pos - ppos).truncate().abs();
        if d.x + d.y < 8000 && ((obj.speed.abs() < 100 && pspeed.abs() < 100) || obj.health < 50) {
            info!("police {e:?}: cop out");
            obj.state = State::Parked;
            drive.door = 0x370;
            *stick = Stick::default();
            commands.entity(e).remove::<Pursuit>();
            let info = library.model(obj.model).info.clone();
            // `GetOutOfVehicle` @ 10065c90 with the side toggle set to 1:
            // the driver's door, 1.2·(192 + maxX) out at `door_y`, starting
            // the collision step from the car centre.
            let door_y = super::boarding::door_y(veh.kind, info.max_y);
            let at = super::boarding::side_point(&trig, &obj, info.radius, door_y, 6);
            let centre = obj.pos;
            let cop = spawn_cop(
                &mut commands,
                &data,
                &mut library,
                &mut meshes,
                at,
                wrap(obj.heading - 0x5a0),
                obj.level,
            );
            if let Some(cop) = cop {
                commands.queue(move |world: &mut World| {
                    if let Some(mut o) = world.get_mut::<Obj>(cop) {
                        o.prev_pos = centre;
                    }
                });
            }
        }
    }
}

/// A shotgun cop (`policemale1`, weapon 5, 100 shells) in state 3.
fn spawn_cop(
    commands: &mut Commands,
    data: &GameData,
    library: &mut ObjectLibrary,
    meshes: &mut Assets<Mesh>,
    pos: IVec3,
    heading: i32,
    level: usize,
) -> Option<Entity> {
    let cop = people::spawn_person(
        commands,
        data,
        library,
        meshes,
        super::police::COP,
        pos,
        heading,
        State::Wander,
    )?;
    let mut ars = Arsenal::default();
    ars.current = Weapon::Shotgun;
    ars.ammo[Weapon::Shotgun as usize] = 100;
    commands.entity(cop).insert((ars, Ambient));
    commands.queue(move |world: &mut World| {
        if let Some(mut o) = world.get_mut::<Obj>(cop) {
            o.level = level;
        }
    });
    Some(cop)
}

/// A shotgun cop for a roadblock, straight into the world (the roadblock
/// spawner runs on the `World`, `traffic::add_police`).
pub fn spawn_cop_world(world: &mut World, pos: IVec3, heading: i32, level: usize) -> Option<Entity> {
    // A system state, not `resource_scope` (that marks the resource added).
    let mut st: bevy::ecs::system::SystemState<(
        Commands,
        Res<GameData>,
        ResMut<ObjectLibrary>,
        ResMut<Assets<Mesh>>,
    )> = bevy::ecs::system::SystemState::new(world);
    let cop = {
        let Ok((mut commands, data, mut library, mut meshes)) = st.get_mut(world) else {
            return None;
        };
        spawn_cop(&mut commands, &data, &mut library, &mut meshes, pos, heading, level)
    };
    st.apply(world);
    world.flush();
    cop
}

/// Siren loop and red/blue light bar while pursuing (68k `FUN_000e2a50`):
/// sound 11 at 25e6 / 10000 Hz; the model alternates `_blue` (flash < 5)
/// and `_red` every 5 ticks; the plain model returns when it stops.
#[allow(clippy::type_complexity)]
fn sirens(
    mut commands: Commands,
    data: Res<GameData>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut models: Local<Option<[Handle<Mesh>; 6]>>,
    mut q: Query<(Entity, &Obj, &Vehicle, Option<&mut Siren>, &mut Mesh3d)>,
    mut sounds: Query<&mut Sound>,
) {
    let Some(mut library) = library else { return };
    if models.is_none() {
        let mut load = |p: &str| {
            library
                .load(&data, &mut meshes, p)
                .map(|m| library.model(m).mesh.clone())
        };
        let all = [
            "vehicles/police.obj",
            "vehicles/police_blue.obj",
            "vehicles/police_red.obj",
            "vehicles/policevan.obj",
            "vehicles/policevan_blue.obj",
            "vehicles/policevan_red.obj",
        ]
        .map(|p| load(p));
        if all.iter().any(Option::is_none) {
            return;
        }
        *models = Some(all.map(Option::unwrap));
    }
    let Some(models) = models.as_ref() else {
        return;
    };
    for (e, obj, veh, siren, mut mesh) in &mut q {
        if !TYPES[veh.kind].police {
            continue;
        }
        let base = if veh.kind == vk::POLICE_VAN { 3 } else { 0 };
        let on = matches!(obj.state, State::PursuitRoute | State::Pursuit);
        let at = SoundPos::At(obj.posf());
        match (on, siren) {
            (true, Some(mut s)) => {
                s.flash = (s.flash + 1) % 10;
                let want = &models[base + if s.flash < 5 { 1 } else { 2 }];
                if mesh.0 != *want {
                    mesh.0 = want.clone();
                }
                match s.sound.and_then(|h| sounds.get_mut(h).ok()) {
                    Some(mut snd) => snd.pos = at,
                    None => {
                        s.sound = Some(
                            commands
                                .spawn(Sound::looping(id::SIREN, 25e6, 10000.0, at))
                                .id(),
                        );
                    }
                }
            }
            (true, None) => {
                commands.entity(e).insert(Siren::default());
            }
            (false, Some(s)) => {
                if let Some(h) = s.sound {
                    commands.entity(h).despawn();
                }
                mesh.0 = models[base].clone();
                commands.entity(e).remove::<Siren>();
            }
            (false, None) => {}
        }
    }
}

/// Drones leave with the view window (68k `FUN_000dc5ac`): pursuing cars
/// and roadblock barriers without a script id, plus their sounds.
#[allow(clippy::type_complexity)]
fn despawn_far(
    mut commands: Commands,
    cam: Option<Res<GameCam>>,
    q: Query<
        (
            Entity,
            &Obj,
            Option<&Vehicle>,
            Option<&Siren>,
            Has<Roadblock>,
        ),
        Without<CameraTarget>,
    >,
) {
    let Some(cam) = cam else { return };
    let c = cam.square();
    let h = cam.window();
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, obj, veh, siren, roadblock)) = q.get(e__) else { continue };
        let drone = matches!(obj.state, State::PursuitRoute | State::Pursuit)
            || (roadblock && obj.state == State::Barrier);
        let sq = IVec2::new(obj.pos.x >> 11, obj.pos.y >> 11);
        if !drone || obj.script_id != -1 || ((sq - c).abs().max_element() <= h) {
            continue;
        }
        if let Some(s) = veh.and_then(|v| v.engine) {
            commands.entity(s).despawn();
        }
        if let Some(s) = siren.and_then(|s| s.sound) {
            commands.entity(s).despawn();
        }
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Class;
    use bevy::ecs::system::RunSystemOnce;

    /// A police car under script goto 0x15 drops the order: pursuit from
    /// wanted 3, traffic below; goto 0xf is left alone (68k ~124171).
    fn run(order: Order, wanted: i32) -> (State, Order, bool) {
        let mut world = World::new();
        world.insert_resource(Aggressiveness(2));
        let mut p = Obj::new(Class::Person, 0, IVec3::ZERO, 0);
        p.wanted = wanted;
        world.spawn((p, CameraTarget));
        let mut car = Obj::new(Class::Vehicle, 0, IVec3::new(5000, 0, 0), 0);
        car.state = State::Idle;
        let ai = Ai { order, ..default() };
        let veh = Vehicle { kind: vk::POLICE, colour: 0, engine: None, flying: false, controls: true };
        let e = world.spawn((car, veh, ai)).id();
        world.run_system_once(select_pursuers).unwrap();
        (
            world.get::<Obj>(e).unwrap().state,
            world.get::<Ai>(e).unwrap().order,
            world.get::<Pursuit>(e).is_some(),
        )
    }

    #[test]
    fn script_police_goto_15() {
        let g = IVec2::new(1000, 2000);
        assert_eq!(run(Order::Goto15(g), 4), (State::PursuitRoute, Order::Ambient, true));
        assert_eq!(run(Order::Goto15(g), 1).1, Order::Ambient);
        assert_eq!(run(Order::Goto15(g), 1).0, State::Wander);
        assert_eq!(run(Order::Goto(g, 1000), 4), (State::Idle, Order::Goto(g, 1000), false));
    }
}
