//! The six level scripts (docs/re/missions_*.md), ported from the Update 7
//! code, and the Mac's Random missions for Rampage ([`random`]). Each level
//! is an event-driven state machine.

mod level_00;
mod level_01;
pub(crate) mod level_10;
mod level_11;
mod level_20;
pub(crate) mod level_21;
pub mod random;

use super::LevelScript;

/// The script for city `city`, level `level` (0 or 1).
pub fn for_level(city: u8, level: u8) -> Option<Box<dyn LevelScript>> {
    Some(match (city, level) {
        (0, 0) => Box::new(level_00::Level00::default()),
        (0, 1) => Box::new(level_01::Level01::default()),
        (1, 0) => Box::new(level_10::Level10::default()),
        (1, 1) => Box::new(level_11::Level11::default()),
        (2, 0) => Box::new(level_20::Level20::default()),
        (2, 1) => Box::new(level_21::Level21::default()),
        _ => return None,
    })
}

/// A mission a level script starts when its phone is answered with no
/// mission running (the mission selector, `frontend::missions`).
#[derive(Clone, Copy, Debug)]
pub struct MissionInfo {
    /// The mission's slot (`set_cur_slot`).
    pub slot: i32,
    /// The phone event that starts it.
    pub event: i32,
    /// The level phrase of its first briefing line.
    pub brief: u32,
    /// Short English description (port-added, no phrase).
    pub name: &'static str,
    /// Where it starts: the map phone it rings on, or (negative) the
    /// script actor `-from` the player gets into instead (level 2-1's first
    /// mission, car 50).
    pub from: i32,
}

const fn m(slot: i32, event: i32, brief: u32, name: &'static str, from: i32) -> MissionInfo {
    MissionInfo { slot, event, brief, name, from }
}

/// Every mission of city `city`, level `level`, in the order the level
/// offers them (the first phones, then each wave the unlock routine opens;
/// hidden phones last). Level 2-0, the jailbreak, is one scripted sequence
/// without phones: none.
pub fn missions(city: u8, level: u8) -> &'static [MissionInfo] {
    match (city, level) {
        // level_00.rs `answer_phone`, `unlock_phones`.
        (0, 0) => {
            const {
                &[
                    m(1, 100, 0, "Shooting range", 2),
                    m(9, 900, 86, "Limo time trial", 3),
                    m(4, 400, 34, "Two taxis to Rob's", 4),
                    m(2, 200, 15, "The dope car", 5),
                    m(11, 1100, 98, "KKK meeting", 11),
                    m(3, 300, 27, "Bus to Jon's garage", 12),
                    m(6, 600, 52, "Tail the traitor", 13),
                    m(5, 500, 39, "Van bomb", 14),
                    m(10, 1000, 90, "Escort the boss", 17),
                    m(0, 20, 3, "Bank getaway", 15),
                    m(7, 700, 58, "White pug (hidden)", 1),
                    m(19, 1900, 20, "Tank (hidden)", 10),
                ]
            }
        }
        // level_01.rs `answer_phone`, unlock at the end of the file.
        (0, 1) => {
            const {
                &[
                    m(0, 1, 0, "Avenge Harry", 8),
                    m(1, 100, 6, "Four cops, 3 minutes", 9),
                    m(2, 200, 15, "Ram-raid the shop", 18),
                    m(6, 600, 46, "Ice-cream van trap", 20),
                    m(3, 300, 21, "Cars in the sea", 6),
                    m(4, 400, 31, "The briefcase", 7),
                    m(5, 500, 43, "The pickup driver", 19),
                    m(7, 700, 52, "Wasteland deal", 21),
                    m(8, 800, 69, "Prison bus (hidden)", 16),
                ]
            }
        }
        // level_10.rs `event` (the `start_*` calls).
        (1, 0) => {
            const {
                &[
                    m(0, 1, 2, "Street race", 1),
                    m(1, 100, 14, "Follow the henchman", 2),
                    m(2, 200, 27, "Steal the truck", 3),
                    m(3, 300, 36, "Gang meeting ambush", 4),
                    m(4, 400, 42, "Help Kyle", 5),
                    m(5, 500, 49, "Hijack the police van", 6),
                    m(6, 600, 54, "Ike's tail", 7),
                    m(7, 700, 60, "Kyle's brother", 8),
                ]
            }
        }
        // level_11.rs `phone`.
        (1, 1) => {
            const {
                &[
                    m(1, 100, 2, "Apartment bomb", 10),
                    m(2, 200, 10, "The drug bus", 11),
                    m(3, 300, 14, "Kidnap the brother", 12),
                    m(4, 400, 48, "Taxi for the wife", 13),
                    m(5, 500, 30, "Ransom handover", 12),
                    m(6, 600, 63, "The dumped Vapour", 14),
                    m(7, 700, 72, "The love rival", 15),
                    m(8, 800, 77, "The rival gang", 16),
                    m(9, 900, 82, "RC car chase", 17),
                ]
            }
        }
        // level_21.rs `start` (M1 starts by getting into car 50).
        (2, 1) => {
            const {
                &[
                    m(1, 100, 2, "The kidnapping", -50),
                    m(2, 200, 19, "Rocket car", 1),
                    m(3, 300, 28, "The race", 2),
                    m(6, 600, 57, "Bank van hijack", 3),
                    m(4, 400, 40, "Surveillance van", 5),
                    m(5, 500, 46, "Nasty Nick", 6),
                    m(7, 700, 66, "The bomb truck", 7),
                    m(8, 800, 80, "Grenade frenzy", 8),
                ]
            }
        }
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every listed mission is distinct and its level has a script; the
    /// jailbreak has none to list.
    #[test]
    fn mission_tables() {
        for (city, level) in [(0, 0), (0, 1), (1, 0), (1, 1), (2, 1)] {
            let list = missions(city, level);
            assert!(!list.is_empty() && for_level(city, level).is_some());
            for (i, a) in list.iter().enumerate() {
                assert!(list[i + 1..].iter().all(|b| b.event != a.event && b.slot != a.slot), "{city}-{level} {a:?}");
            }
        }
        assert!(missions(2, 0).is_empty());
    }
}
