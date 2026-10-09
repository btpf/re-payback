//! Wanted level (docs/re/combat_police.md §2; decay CONFIRMED 68k
//! `FUN_000e3a58` ~125921, timer `DAT_0030c3b8` / OS4 `DAT_0114c724`).

use super::*;
use crate::sim::Class;
use bevy::ecs::system::RunSystemOnce;

fn world(aggr: Option<i32>, wanted: i32) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<WantedTimer>();
    if let Some(a) = aggr {
        world.insert_resource(Aggressiveness(a));
    }
    let mut o = Obj::new(Class::Person, 0, IVec3::ZERO, 0);
    o.wanted = wanted;
    let p = world.spawn((o, CameraTarget)).id();
    (world, p)
}

fn wanted(w: &World, p: Entity) -> i32 {
    w.get::<Obj>(p).unwrap().wanted
}

/// `wantedTimer += ticks; if wantedTimer >= aggr*100 + 150: timer = 0,
/// wanted -= 1 (if > 0)`: one level per 150..650 ticks.
#[test]
fn decay_period_follows_aggressiveness() {
    for aggr in 0..=5 {
        let period = aggr * 100 + 150;
        let (mut w, p) = world(Some(aggr), 3);
        for _ in 0..period - 1 {
            w.run_system_once(decay_wanted).unwrap();
        }
        assert_eq!(wanted(&w, p), 3, "aggr {aggr}: not before {period} ticks");
        w.run_system_once(decay_wanted).unwrap();
        assert_eq!(wanted(&w, p), 2, "aggr {aggr}: one level at {period} ticks");
        assert_eq!(w.resource::<WantedTimer>().0, 0);
    }
}

#[test]
fn decay_stops_at_zero() {
    let (mut w, p) = world(Some(0), 1);
    for _ in 0..1000 {
        w.run_system_once(decay_wanted).unwrap();
    }
    assert_eq!(wanted(&w, p), 0);
}

/// A crime that changes the wanted level resets the timer (`crimes`);
/// here: the timer restarts the full period.
#[test]
fn reset_timer_restarts_the_period() {
    let (mut w, p) = world(Some(1), 2);
    for _ in 0..200 {
        w.run_system_once(decay_wanted).unwrap();
    }
    w.resource_mut::<WantedTimer>().0 = 0;
    for _ in 0..249 {
        w.run_system_once(decay_wanted).unwrap();
    }
    assert_eq!(wanted(&w, p), 2);
    w.run_system_once(decay_wanted).unwrap();
    assert_eq!(wanted(&w, p), 1);
}

/// Without the resource (no level loaded) the aggressiveness is 2.
#[test]
fn default_aggressiveness() {
    let (mut w, p) = world(None, 1);
    for _ in 0..349 {
        w.run_system_once(decay_wanted).unwrap();
    }
    assert_eq!(wanted(&w, p), 1);
    w.run_system_once(decay_wanted).unwrap();
    assert_eq!(wanted(&w, p), 0);
}

/// The kill tables of §2.1, indexed by the current level and clamped to
/// 0..6: civilian killed `<3→3, 3→4, 4→5, 5→6`; police person killed
/// `0→3, 1..4 → w+2, 5→6`; driven police vehicle `<2→4, 2..4 → w+2, 5→6`.
#[test]
fn crime_tables() {
    let civilian = [3, 3, 3, 4, 5, 6, 6];
    let cop = [3, 3, 4, 5, 6, 6, 6];
    let cop_car = [4, 4, 4, 5, 6, 6, 6];
    for w in 0..=6 {
        let doc_civ = if w < 3 { 3 } else { (w + 1).min(6) };
        let doc_cop = if w == 0 { 3 } else { (w + 2).min(6) };
        let doc_car = if w < 2 { 4 } else { (w + 2).min(6) };
        assert_eq!(raise(w, civilian), doc_civ);
        assert_eq!(raise(w, cop), doc_cop);
        assert_eq!(raise(w, cop_car), doc_car);
    }
    // Out-of-range levels clamp.
    assert_eq!(raise(-3, civilian), 3);
    assert_eq!(raise(9, civilian), 6);
}
