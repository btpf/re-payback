//! Contact circles (Mac `CheckForCollisions` @ 1006a150, OS4
//! `FUN_01056c58`; docs/re/combat_police.md §6, people.md §5).

use super::*;
use crate::sim::Class;

/// People are one circle at their position.
#[test]
fn a_person_is_one_circle() {
    let t = Trig::default();
    let o = Obj::new(Class::Person, 0, IVec3::new(5000, 6000, 70), 1234);
    assert_eq!(circles(&t, &o, false, 192, 0, 0), vec![o.pos]);
}

/// Vehicles: four circles along the heading at `{0, hw − rear, front − hw,
/// 2·front/3 − hw}`.
#[test]
fn vehicle_circles_along_the_heading() {
    let t = Trig::default();
    let (hw, front, rear) = (300, 900, 840);
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0);
    let along = [0, hw - rear, front - hw, 2 * front / 3 - hw];
    let c = circles(&t, &o, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(0, d, 0)).to_vec());
    // Facing east: along +x.
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0x5a0);
    let c = circles(&t, &o, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(d, 0, 0)).to_vec());
    // Facing north: along −y.
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0xb40);
    let c = circles(&t, &o, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(0, -d, 0)).to_vec());
}

/// Circles stay inside the body: the front circles at most `front − hw`
/// ahead, the rear one `rear − hw` behind, so the radius-hw circles touch
/// the bumpers.
#[test]
fn circles_reach_the_bumpers() {
    let t = Trig::default();
    let (hw, front, rear) = (320, 1000, 900);
    let o = Obj::new(Class::Vehicle, 0, IVec3::ZERO, 0);
    let c = circles(&t, &o, true, hw, front, rear);
    let max = c.iter().map(|p| p.y).max().unwrap();
    let min = c.iter().map(|p| p.y).min().unwrap();
    assert_eq!(max + hw, front);
    assert_eq!(min - hw, -rear);
}
