//! `Maps/<n>.vehicles`: parked vehicles and traffic city flags (OS4
//! `FUN_0106dbbc`).
//!
//! Big-endian i32s: `version (1)`, three booleans (traffic models whose home
//! city is 0/1/2 may appear on this map — only used for user maps),
//! `count` (at most 6), then `count` × {model id 0..27, x, y (0x800 per
//! square), layer 0..2, heading (5760 per turn)}. The parked vehicles are
//! spawned only in Rampage and multiplayer (`FUN_010692bc` ~49456).

use crate::{Error, Result};

/// Most records the game reads.
pub const MAX: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parked {
    pub model: i32,
    pub x: i32,
    pub y: i32,
    pub layer: i32,
    pub heading: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapVehicles {
    pub version: i32,
    /// Traffic models of home city 0, 1, 2 allowed.
    pub cities: [bool; 3],
    pub parked: Vec<Parked>,
}

impl MapVehicles {
    pub fn parse(d: &[u8]) -> Result<Self> {
        let at = |i: usize| {
            d.get(i * 4..i * 4 + 4)
                .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or(Error::Truncated("vehicles"))
        };
        let count = at(4)?;
        if !(0..=MAX as i32).contains(&count) {
            return Err(Error::Parse { what: "vehicles", detail: format!("count {count}") });
        }
        let parked = (0..count as usize)
            .map(|r| {
                let b = 5 + r * 5;
                Ok(Parked { model: at(b)?, x: at(b + 1)?, y: at(b + 2)?, layer: at(b + 3)?, heading: at(b + 4)? })
            })
            .collect::<Result<_>>()?;
        Ok(Self { version: at(0)?, cities: [at(1)? != 0, at(2)? != 0, at(3)? != 0], parked })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let words = [1, 0, 1, 0, 1, 17, 145600, 118208, 0, 3690];
        let bytes: Vec<u8> = words.iter().flat_map(|w: &i32| w.to_be_bytes()).collect();
        let v = MapVehicles::parse(&bytes).unwrap();
        assert_eq!(v.cities, [false, true, false]);
        assert_eq!(v.parked, vec![Parked { model: 17, x: 145600, y: 118208, layer: 0, heading: 3690 }]);
        assert!(MapVehicles::parse(&bytes[..30]).is_err());
    }
}
