//! The six level scripts (docs/re/missions_*.md), ported from the Update 7
//! code. Each level is an event-driven state machine.

mod level_00;
mod level_01;
mod level_10;
mod level_11;
mod level_20;
mod level_21;

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
