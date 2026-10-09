//! Weapons (docs/re/combat_police.md §3, §6; script_api.md trigger 0x22).

use super::*;
use Weapon::*;

/// §3.1 table: HUD slot order, original ids `+0x35e`.
#[test]
fn slots_and_original_ids() {
    let slots = [Fist, Pistol, Shotgun, Flamethrower, Grenade, Minigun, Bazooka, Bomb];
    let ids = [0, 0xd, 5, 2, 3, 4, 1, 0xe];
    for (slot, (w, id)) in slots.iter().zip(ids).enumerate() {
        assert_eq!(Weapon::ALL[slot], *w);
        assert_eq!(*w as usize, slot);
        assert_eq!(w.original_id(), id, "{w:?}");
    }
}

/// Trigger 0x22 codes: 1 bazooka, 2 flamethrower, 3 grenade, 4 minigun,
/// 5 shotgun, 7 pistol, 8 remote bomb (script_api.md, CONFIRMED).
#[test]
fn weapon_fired_codes() {
    let codes = [(Bazooka, 1), (Flamethrower, 2), (Grenade, 3), (Minigun, 4), (Shotgun, 5), (Pistol, 7), (Bomb, 8)];
    for (w, c) in codes {
        assert_eq!(w.fired_code(), c, "{w:?}");
    }
}

/// Damage filter classes (§6): 4 minigun, 7 pistol, 5 shotgun, 2 flame,
/// 1 explosion (grenade, bazooka, bomb).
#[test]
fn filter_classes() {
    let classes = [(Minigun, 4), (Pistol, 7), (Shotgun, 5), (Flamethrower, 2), (Grenade, 1), (Bazooka, 1), (Bomb, 1)];
    for (w, c) in classes {
        assert_eq!(w.filter_class(), c, "{w:?}");
    }
}

fn armed(current: Weapon, with: &[Weapon]) -> Arsenal {
    let mut a = Arsenal { current, ..default() };
    for w in with {
        a.ammo[*w as usize] = 5;
    }
    a
}

/// §3.1 "Out of ammo": the first weapon with ammo in this priority.
#[test]
fn auto_switch_priority() {
    for from in [Grenade, Bomb, Bazooka, Flamethrower] {
        let mut a = armed(from, &[Pistol, Shotgun, Minigun]);
        auto_switch(&mut a);
        assert_eq!(a.current, Minigun, "{from:?}");
        let mut a = armed(from, &[Pistol, Shotgun]);
        auto_switch(&mut a);
        assert_eq!(a.current, Shotgun, "{from:?}");
        let mut a = armed(from, &[Pistol, Flamethrower]);
        if from == Flamethrower {
            a.ammo[Flamethrower as usize] = 0;
        }
        auto_switch(&mut a);
        assert_eq!(a.current, Pistol, "{from:?}");
    }
    let mut a = armed(Minigun, &[Pistol, Shotgun, Flamethrower]);
    auto_switch(&mut a);
    assert_eq!(a.current, Shotgun);
    let mut a = armed(Pistol, &[Minigun, Shotgun, Flamethrower]);
    auto_switch(&mut a);
    assert_eq!(a.current, Shotgun);
    let mut a = armed(Pistol, &[Minigun, Flamethrower]);
    auto_switch(&mut a);
    assert_eq!(a.current, Minigun);
    let mut a = armed(Shotgun, &[Minigun, Pistol, Flamethrower]);
    auto_switch(&mut a);
    assert_eq!(a.current, Pistol);
    let mut a = armed(Shotgun, &[Flamethrower]);
    auto_switch(&mut a);
    assert_eq!(a.current, Flamethrower);
    // Grenades, bazooka and bomb are never switched to.
    let mut a = armed(Pistol, &[Grenade, Bazooka, Bomb]);
    auto_switch(&mut a);
    assert_eq!(a.current, Pistol);
    // Nothing with ammo: it stays (and clicks).
    let mut a = armed(Minigun, &[]);
    auto_switch(&mut a);
    assert_eq!(a.current, Minigun);
    let mut a = armed(Fist, &[Pistol]);
    auto_switch(&mut a);
    assert_eq!(a.current, Fist);
}

/// §3.3 lifetimes from 100: bullet −1/tick (100 ticks), pellet −2 (50),
/// petrol −5 (20); grenade fuse −1; missile and bomb none. Only the
/// grenade and the bomb fall.
#[test]
fn projectile_lifetimes() {
    use ProjectileKind as P;
    let life = |k: P| if k.decay() == 0 { None } else { Some(100 / k.decay()) };
    assert_eq!(life(P::Bullet), Some(100));
    assert_eq!(life(P::MinigunBullet), Some(100));
    assert_eq!(life(P::Pellet), Some(50));
    assert_eq!(life(P::Petrol), Some(20));
    assert_eq!(P::Grenade.decay(), 1);
    assert_eq!(life(P::Missile), None);
    assert_eq!(life(P::Bomb), None);
    for k in [P::Bullet, P::MinigunBullet, P::Pellet, P::Petrol, P::Missile] {
        assert!(!k.gravity(), "{k:?}");
    }
    assert!(P::Grenade.gravity() && P::Bomb.gravity());
}

#[test]
fn fresh_arsenal_is_fists_only() {
    // combat_police.md §3: fists only, no ammo, at level start and respawn.
    let a = Arsenal::default();
    assert_eq!(a.current, Fist);
    assert!(a.ammo.iter().all(|&n| n == 0));
    assert!(a.bomb.is_none());
}
