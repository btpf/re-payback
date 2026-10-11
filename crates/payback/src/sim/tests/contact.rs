//! Contact circles (Mac `CheckForCollisions` @ 1006a150, OS4
//! `FUN_01056c58`; docs/re/combat_police.md §6, people.md §5).

use super::*;
use crate::sim::{Class, ModelInfo, TURN};

/// People are one circle at their position.
#[test]
fn a_person_is_one_circle() {
    let t = Trig::default();
    let o = Obj::new(Class::Person, 0, IVec3::new(5000, 6000, 70), 1234);
    assert_eq!(circles_at(&t, o.pos, o.heading, false, 192, 0, 0), vec![o.pos]);
}

/// Vehicles: four circles along the heading at `{0, hw − rear, front − hw,
/// 2·front/3 − hw}`.
#[test]
fn vehicle_circles_along_the_heading() {
    let t = Trig::default();
    let (hw, front, rear) = (300, 900, 840);
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0);
    let along = [0, hw - rear, front - hw, 2 * front / 3 - hw];
    let c = circles_at(&t, o.pos, o.heading, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(0, d, 0)).to_vec());
    // Facing east: along +x.
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0x5a0);
    let c = circles_at(&t, o.pos, o.heading, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(d, 0, 0)).to_vec());
    // Facing north: along −y.
    let o = Obj::new(Class::Vehicle, 0, IVec3::new(50_000, 60_000, 0), 0xb40);
    let c = circles_at(&t, o.pos, o.heading, true, hw, front, rear);
    assert_eq!(c, along.map(|d| o.pos + IVec3::new(0, -d, 0)).to_vec());
}

/// The contact test places objects where this tick's move takes them: `pos
/// + vel` in x and y, z plus the pushes taken so far (Mac
/// `CheckForCollisions` @ 1006a150).
#[test]
fn contacts_test_the_predicted_position() {
    let mut o = Obj::new(Class::Person, 0, IVec3::new(5000, 6000, 70), 0);
    o.vel = IVec3::new(90, -40, 30);
    assert_eq!(predicted(&o, 7), IVec3::new(5090, 5960, 77));
}

/// Circles stay inside the body: the front circles at most `front − hw`
/// ahead, the rear one `rear − hw` behind, so the radius-hw circles touch
/// the bumpers.
#[test]
fn circles_reach_the_bumpers() {
    let t = Trig::default();
    let (hw, front, rear) = (320, 1000, 900);
    let o = Obj::new(Class::Vehicle, 0, IVec3::ZERO, 0);
    let c = circles_at(&t, o.pos, o.heading, true, hw, front, rear);
    let max = c.iter().map(|p| p.y).max().unwrap();
    let min = c.iter().map(|p| p.y).min().unwrap();
    assert_eq!(max + hw, front);
    assert_eq!(min - hw, -rear);
}

/// Bullets and walkers against parked vehicles at several headings (user
/// reports 2026-10: shots passing through a diagonal car, the player inside
/// a diagonal bus), with the vehicle's circles ([`circles_at`]) against the
/// old single centre circle for bullets, and the contact rule at `pos + vel`
/// with the push on the velocity (the original) against the old test at
/// `pos` pushing both. Needs `extracted/` (the Amiga models the simulation
/// reads): `cargo test -p payback --bin payback vehicle_shape_table --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn vehicle_shape_table() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/game/Objects/Vehicles");
    let t = Trig::default();
    for name in ["Mundaneo.Obj", "Police.Obj", "Limo.Obj", "Van.Obj", "Bus.Obj", "Truck.Obj"] {
        let Ok(b) = std::fs::read(root.join(name)) else { continue };
        let m = ModelInfo::from_obj(name, &payback_formats::Obj::parse(&b).unwrap());
        let (hw, front, rear) = (m.radius, m.max_y, m.min_y);
        // The deepest point outside every circle along the side: the cusp
        // of the widest gap between neighbouring circles.
        let r = (hw + 192) * 17 >> 4;
        let mut along = [0, hw - rear, front - hw, 2 * front / 3 - hw];
        along.sort();
        let (gap, at) = along.windows(2).map(|w| (w[1] - w[0], (w[0] + w[1]) / 2)).max().unwrap();
        let reach = (((r * r - (gap / 2) * (gap / 2)).max(0)) as f64).sqrt() as i32;
        println!(
            "{name}: hw {hw} front {front} rear {rear}, circles {along:?}, R {r}; widest gap {gap} \
             (cusp at {at} along, {reach} from the axis, {} inside the side)",
            hw - reach
        );
        for deg in [0, 30, 45, 60, 90] {
            let car = Obj::new(Class::Vehicle, 0, IVec3::new(100_000, 100_000, 0), deg * TURN / 360);
            let (s, c) = (t.sin(car.heading), t.cos(car.heading));
            // Car-local coordinates (x across, y along).
            let local = |p: IVec3| {
                let d = (p - car.pos).as_vec3();
                let (s, c) = (s as f32 / 256.0, c as f32 / 256.0);
                Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
            };
            let inside = |l: Vec2| l.x.abs() <= hw as f32 && l.y <= front as f32 && l.y >= -(rear as f32);
            let cs = circles_at(&t, car.pos, car.heading, true, hw, front, rear);
            let touch = |q: IVec3, p: IVec3| {
                let d = p - q;
                (d.x as i64).pow(2) + (d.y as i64).pow(2) < (r as i64).pow(2)
            };
            // Bullets (radius 192, 256 a tick) flying north at x offsets
            // every 64 through the footprint, each from 4 start phases
            // (distances 6000, 6064, 6128, 6192 south of the car).
            let (mut body, mut old, mut new) = (0, 0, 0);
            for off in (-3000..=3000).step_by(64) {
                for phase in 0..4 {
                    let mut p = car.pos + IVec3::new(off, -6000 - 64 * phase, 0);
                    let (mut crossed, mut h_old, mut h_new) = (false, false, false);
                    for _ in 0..60 {
                        p.y += 256;
                        crossed |= inside(local(p));
                        h_old |= touch(car.pos, p);
                        h_new |= cs.iter().any(|q| touch(*q, p));
                    }
                    body += crossed as i32;
                    old += (crossed && h_old) as i32;
                    new += (crossed && h_new) as i32;
                }
            }
            // A walker (192) running at 96 a tick (`do_Person`: `vel =
            // (vel + 2·speed·dir) / 3`, after the move) straight at the
            // car's left side, aimed every 64 along its length, 200 ticks.
            let push_at = |p: IVec3| {
                let (mut push, mut n) = (IVec3::ZERO, 0);
                for q in &cs {
                    let d = p - *q;
                    let d2 = (d.x as i64).pow(2) + (d.y as i64).pow(2);
                    if d2 < (r as i64).pow(2) {
                        let dist = ((d2 as f64).sqrt() as i32).max(1);
                        push += IVec3::new(d.x * r / dist - d.x, d.y * r / dist - d.y, 0);
                        n += 1;
                    }
                }
                if n > 0 { push / n } else { IVec3::ZERO }
            };
            let walk = |original: bool| {
                let (mut through, mut deep) = (0, i32::MIN);
                let side = IVec3::new(-c, s, 0);
                for along in (-rear..=front).step_by(64) {
                    let mut p = car.pos + IVec3::new(s * along >> 8, c * along >> 8, 0) + side * 3000 / 256;
                    let mut v = IVec3::ZERO;
                    let mut worst = i32::MIN;
                    for _ in 0..200 {
                        if original {
                            v += push_at(p + v);
                        } else {
                            let dv = push_at(p);
                            p += dv;
                            v += dv;
                        }
                        p += v;
                        v = (v - side * 192 / 256) / 3;
                        let l = local(p);
                        if l.y <= (front - 192) as f32 && l.y >= -(rear - 192) as f32 {
                            worst = worst.max(hw - l.x.abs() as i32);
                        }
                    }
                    // Crossed the axis within the body's length: walked
                    // through.
                    if worst >= hw - 32 {
                        through += 1;
                    } else {
                        deep = deep.max(worst);
                    }
                }
                (through, deep)
            };
            let ((t_old, d_old), (t_new, d_new)) = (walk(false), walk(true));
            println!(
                "  {deg:>2} deg: bullets through the body {body}: hit old {old}, new {new} | walker: \
                 old through {t_old}, else deepest {d_old}; original through {t_new}, else deepest {d_new}"
            );
        }
    }
}
