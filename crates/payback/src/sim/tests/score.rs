//! Kill scoring (docs/re/combat_police.md §7, CONFIRMED in OS4
//! `FUN_01043924`, `FUN_0104405c`, `FUN_01056c58`; copkiller bonus Mac
//! `CheckHitObject` @ 100542d0).

use super::*;
use crate::sim::Class;
use bevy::ecs::system::RunSystemOnce;

fn world(multiplier: i32) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Messages<Killed>>();
    world.init_resource::<Messages<Hurt>>();
    world.init_resource::<LastAttacker>();
    world.init_resource::<CopKills>();
    world.insert_resource(PlayerSession { multiplier, ..default() });
    world.init_resource::<BigTitle>();
    world.init_resource::<crate::hud::labels::Labels>();
    world.init_resource::<crate::level::GameMode>();
    let mut o = Obj::new(Class::Person, 0, IVec3::new(4096, 4096, 0), 0);
    o.player = Some(0);
    let p = world.spawn((o, CameraTarget)).id();
    (world, p)
}

fn victim(world: &mut World) -> Entity {
    world.spawn(Obj::new(Class::Person, 6, IVec3::new(5000, 4096, 0), 0)).id()
}

/// `victim` hurt by the player (or not), then killed.
fn kill(world: &mut World, victim: Entity, by_player: bool, police: bool) {
    world.write_message(Hurt { victim, by_player, by_weapon: true, attacker: by_player.then_some(0) });
    world.run_system_once(track_attackers).unwrap();
    world.resource_mut::<Messages<Hurt>>().clear();
    world.write_message(Killed { victim, police });
    world.run_system_once(score_kills).unwrap();
    world.resource_mut::<Messages<Killed>>().clear();
}

fn score(world: &World, p: Entity) -> i32 {
    world.get::<Obj>(p).unwrap().score
}

#[test]
fn civilian_and_police_kills_times_multiplier() {
    let (mut w, p) = world(3);
    let v = victim(&mut w);
    kill(&mut w, v, true, false);
    assert_eq!(score(&w, p), 1500 * 3);
    let v = victim(&mut w);
    kill(&mut w, v, true, true);
    assert_eq!(score(&w, p), 1500 * 3 + 2500 * 3);
    assert_eq!(w.resource::<CopKills>().0, 1);
}

#[test]
fn only_kills_the_player_dealt() {
    let (mut w, p) = world(1);
    let v = victim(&mut w);
    kill(&mut w, v, false, true);
    assert_eq!(score(&w, p), 0);
    assert_eq!(w.resource::<CopKills>().0, 0);
    // Never hurt by anyone: nothing either.
    let v = victim(&mut w);
    w.write_message(Killed { victim: v, police: false });
    w.run_system_once(score_kills).unwrap();
    assert_eq!(score(&w, p), 0);
}

#[test]
fn last_attacker_decides() {
    let (mut w, p) = world(1);
    let v = victim(&mut w);
    // The player hurts it, then someone else finishes it off.
    w.write_message(Hurt { victim: v, by_player: true, by_weapon: true, attacker: Some(0) });
    w.write_message(Hurt { victim: v, by_player: false, by_weapon: false, attacker: None });
    w.run_system_once(track_attackers).unwrap();
    w.write_message(Killed { victim: v, police: false });
    w.run_system_once(score_kills).unwrap();
    assert_eq!(score(&w, p), 0);
}

#[test]
fn a_kill_pays_once() {
    let (mut w, p) = world(1);
    let v = victim(&mut w);
    kill(&mut w, v, true, false);
    // A second death report of the same victim: its attacker was consumed.
    w.write_message(Killed { victim: v, police: false });
    w.run_system_once(score_kills).unwrap();
    assert_eq!(score(&w, p), 1500);
}

/// Every 11th cop kill: counter `DAT_0114c6a4` > 10, then reset to 0;
/// 100000 × the multiplier on top of the 2500.
#[test]
fn copkiller_bonus_every_eleventh_cop() {
    let (mut w, p) = world(2);
    for _ in 0..10 {
        let v = victim(&mut w);
        kill(&mut w, v, true, true);
    }
    assert_eq!(score(&w, p), 10 * 2500 * 2);
    assert_eq!(w.resource::<CopKills>().0, 10);
    let v = victim(&mut w);
    kill(&mut w, v, true, true);
    assert_eq!(score(&w, p), 11 * 2500 * 2 + 100_000 * 2);
    assert_eq!(w.resource::<CopKills>().0, 0);
    // Civilians don't count toward it.
    let v = victim(&mut w);
    kill(&mut w, v, true, false);
    assert_eq!(w.resource::<CopKills>().0, 0);
}

#[test]
fn the_player_dying_scores_nothing() {
    let (mut w, p) = world(1);
    w.write_message(Hurt { victim: p, by_player: true, by_weapon: true, attacker: Some(0) });
    w.run_system_once(track_attackers).unwrap();
    w.write_message(Killed { victim: p, police: false });
    w.run_system_once(score_kills).unwrap();
    assert_eq!(score(&w, p), 0);
}

/// Score labels at the player (`AddLabelPercent`): one for the kill, none
/// for the bonus.
#[test]
fn kill_label() {
    let (mut w, _) = world(1);
    let v = victim(&mut w);
    kill(&mut w, v, true, false);
    assert_eq!(w.resource::<crate::hud::labels::Labels>().0.len(), 1);
}
