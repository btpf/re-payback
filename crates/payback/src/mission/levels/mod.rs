//! The six level scripts (docs/re/missions_*.md).

use super::LevelScript;

/// The script for city `city`, level `level` (0 or 1).
pub fn for_level(city: u8, level: u8) -> Option<Box<dyn LevelScript>> {
    let _ = (city, level);
    None
}
