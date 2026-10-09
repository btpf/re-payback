//! Boarding geometry (docs/re/vehicles.md §1.2-§1.3; Mac
//! `GetInOutVehicles` @ 10066140, `GetOutVehicle` @ 10065660).

use super::*;
use crate::sim::Class;

/// `+0x48f8` = maxY/2 unless the handling table overrides it (vehicles.md
/// §1.3): bus and truck 15/16, Vapour and Vapour GT 1/3, van and police van
/// 3/4, limo 2/3, ice cream van and pickup 5/6, RC truck 7/8, rocket car
/// 1/6.
#[test]
fn door_positions() {
    let m = 960;
    let expected: [(usize, i32); 28] = [
        (0, m / 2), (1, m / 2), (2, m / 2), (3, m * 15 / 16), (4, m / 3), (5, m / 2), (6, m / 2),
        (7, m * 3 / 4), (8, m * 2 / 3), (9, m / 2), (10, m / 2), (11, m / 2), (12, m / 2),
        (13, m / 2), (14, m * 5 / 6), (15, m * 5 / 6), (16, m / 2), (17, m / 2), (18, m / 2),
        (19, m * 15 / 16), (20, m * 3 / 4), (21, m * 7 / 8), (22, m / 2), (23, m / 2),
        (24, m / 6), (25, m / 6), (26, m / 6), (27, m / 3),
    ];
    for (kind, y) in expected {
        assert_eq!(door_y(kind, m), y, "type {kind}");
    }
}

fn car(heading: i32) -> Obj {
    Obj::new(Class::Vehicle, 0, IVec3::new(100_000, 200_000, 0), heading)
}

/// Model +X is the car's left; the driver's door is on the right (model
/// −X, `+0x48f4 = −maxX`), `door_y` along +Y (forward).
#[test]
fn door_point_is_on_the_right() {
    let t = Trig::default();
    // Heading 0 faces +y (south): forward = +y, model +X = +x... so the
    // door (−X) is at −x.
    let v = car(0);
    assert_eq!(door_point(&t, &v, 300, 400, 0), v.pos + IVec3::new(-300, 400, 0));
    assert_eq!(door_point(&t, &v, 300, 400, 50), v.pos + IVec3::new(-350, 400, 0));
    assert_eq!(car_point(&t, &v, 300, 400), v.pos + IVec3::new(300, 400, 0));
    // Facing east (+x): forward = +x, model +X = −y.
    let v = car(0x5a0);
    assert_eq!(door_point(&t, &v, 300, 400, 0), v.pos + IVec3::new(400, 300, 0));
    assert_eq!(car_point(&t, &v, 300, 400), v.pos + IVec3::new(400, -300, 0));
}

/// Exits stand `k/5·(192 + maxX)` out of the door side (k = 6: 1.2×).
#[test]
fn exit_point() {
    let t = Trig::default();
    let v = car(0);
    let l = (PERSON_RADIUS + 308) * 6 / 5;
    assert_eq!(side_point(&t, &v, 308, 0, 6), v.pos + IVec3::new(-l, 0, 0));
    // Negative k: the other side.
    assert_eq!(side_point(&t, &v, 308, 0, -6), v.pos + IVec3::new(l, 0, 0));
    // The door's longitudinal position.
    assert_eq!(side_point(&t, &v, 308, 512, 6), v.pos + IVec3::new(-l, 512, 0));
}

/// Walk-in waypoints: 0/5 the door point, 1/2 front corners (±r, +maxY),
/// 3/4 rear corners (±r, −|minY|).
#[test]
fn walk_in_waypoints() {
    let t = Trig::default();
    let v = car(0);
    let (r, front, rear, dy) = (300, 900, 800, 450);
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 0), door_point(&t, &v, r, dy, 0));
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 5), door_point(&t, &v, r, dy, 0));
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 1), v.pos + IVec3::new(r, front, 0));
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 2), v.pos + IVec3::new(-r, front, 0));
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 3), v.pos + IVec3::new(r, -rear, 0));
    assert_eq!(waypoint(&t, &v, r, front, rear, dy, 4), v.pos + IVec3::new(-r, -rear, 0));
}

#[test]
fn distance_in_32nds() {
    let a = IVec3::new(0, 0, 0);
    assert_eq!(dist2_32(a, IVec3::new(32 * 3, 32 * 4, 999)), 25);
    // Each axis is shifted before squaring (z ignored).
    assert_eq!(dist2_32(a, IVec3::new(31, -31, 0)), 1);
}

/// Mac `ReadInput` @ 10064bf0: any direction, fire or brake counts as
/// wanting control; in/out alone does not.
#[test]
fn control_requests() {
    assert!(!wants_control(&Stick::default()));
    assert!(!wants_control(&Stick { use_vehicle: true, ..default() }));
    assert!(wants_control(&Stick { steer: -1, ..default() }));
    assert!(wants_control(&Stick { throttle: 1, ..default() }));
    assert!(wants_control(&Stick { fire: true, ..default() }));
    assert!(wants_control(&Stick { brake: true, ..default() }));
}
