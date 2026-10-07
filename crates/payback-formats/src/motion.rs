//! `Motion/Human.motion` — the people walk cycle (docs/re/people.md §3).
//!
//! 336 ASCII integers (one per CRLF line): 12 frames × 14 limbs × (A, B).
//! `A` translates a limb's vertices along model Z (forward/back swing,
//! negative = forward), `B` along model Y (negative = lift), in file units.
//! Frames 6-11 mirror 0-5 with left and right limbs swapped.

use crate::{Error, Result};

pub const FRAMES: usize = 12;
pub const LIMBS: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Motion {
    /// `swing[frame][limb - 1]`
    pub swing: [[i32; LIMBS]; FRAMES],
    /// `lift[frame][limb - 1]`
    pub lift: [[i32; LIMBS]; FRAMES],
}

impl Motion {
    pub fn parse(data: &[u8]) -> Result<Motion> {
        let text: String = data.iter().map(|&b| b as char).collect();
        let mut nums = text.split_whitespace().map(|t| {
            t.parse::<i32>().map_err(|_| Error::Parse {
                what: "motion",
                detail: format!("bad number {t:?}"),
            })
        });
        let mut m = Motion {
            swing: [[0; LIMBS]; FRAMES],
            lift: [[0; LIMBS]; FRAMES],
        };
        for f in 0..FRAMES {
            for l in 0..LIMBS {
                m.swing[f][l] = nums.next().ok_or(Error::Truncated("motion"))??;
                m.lift[f][l] = nums.next().ok_or(Error::Truncated("motion"))??;
            }
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordered_pairs() {
        let text: String = (0..336).map(|i| format!("{i}\r\n")).collect();
        let m = Motion::parse(text.as_bytes()).unwrap();
        assert_eq!((m.swing[0][0], m.lift[0][0]), (0, 1));
        assert_eq!((m.swing[0][1], m.lift[0][1]), (2, 3));
        assert_eq!((m.swing[11][13], m.lift[11][13]), (334, 335));
        assert!(Motion::parse(b"1\n2\n").is_err());
    }
}
