//! People (docs/re/people.md; combat_police.md §6 water).

use super::*;

/// Mass `+0x48ec` per model (people.md §2.1): 90 for MaleNormal*,
/// PedMale1/2/6 and PoliceMale1; 120 PedMale3; 100 PedMale4 and PedMale5;
/// 95 PedMale7.
#[test]
fn masses_by_model_name() {
    for (i, name) in PEOPLE_MODELS.iter().enumerate() {
        let file = name.trim_start_matches("people/");
        let expected = if file.starts_with("pedmale3") {
            120
        } else if file.starts_with("pedmale4") || file.starts_with("pedmale5") {
            100
        } else if file.starts_with("pedmale7") {
            95
        } else {
            90
        };
        assert_eq!(person_mass(i), expected, "{name}");
    }
}

/// The table index of the player's model by weapon (`DAT_012a6600[w]`)
/// is the matching `malenormal<weapon>` file.
#[test]
fn player_models_follow_the_weapon() {
    use crate::sim::combat::Weapon;
    for w in Weapon::ALL {
        let file = PEOPLE_MODELS[w.person_model()];
        let suffix = match w {
            Weapon::Fist => "malenormal.obj".to_string(),
            w => format!("malenormal{}.obj", format!("{w:?}").to_lowercase()),
        };
        assert_eq!(file, format!("people/{suffix}"), "{w:?}");
    }
    assert_eq!(PEOPLE_MODELS[crate::sim::police::COP], "people/policemale1.obj");
}

/// Collision radius: MaleNormal's max |X| / 3 = 192 (people.md §2.1);
/// drowning at z ≥ 27000 (combat_police.md §6: removed, state 6).
#[test]
fn constants() {
    assert_eq!(PERSON_RADIUS, 192);
    assert_eq!(DROWN_Z, 27000);
    assert_eq!(MAX_OBJECTS, 32);
}

/// Mac `do_Person` steering toward T: the shorter way round, unless within
/// `near_left` (turning left) / `near_right` (right), which is "facing".
#[test]
fn steering_takes_the_shorter_way() {
    // Target slightly to the left (heading increases): steer -1.
    assert_eq!(steer_toward(0, 0x300, 0xc9, 0xa0), (-1, false));
    // Slightly to the right (across 0): steer +1.
    assert_eq!(steer_toward(0x100, 0x1680 - 0x300, 0xc9, 0xa0), (1, false));
    // Within the dead zones: facing.
    assert_eq!(steer_toward(0, 0xc8, 0xc9, 0xa0), (0, true));
    assert_eq!(steer_toward(0, 0xc9, 0xc9, 0xa0), (-1, false));
    assert_eq!(steer_toward(0xa0 - 1, 0, 0xc9, 0xa0), (0, true));
    assert_eq!(steer_toward(0xa0, 0, 0xc9, 0xa0), (1, false));
    // Exactly behind: d1 == d2, which turns right.
    assert_eq!(steer_toward(0, 0xb40, 0xc9, 0xa0), (1, false));
    // On target.
    assert_eq!(steer_toward(0x1234, 0x1234, 0xc9, 0xa0), (0, true));
}
