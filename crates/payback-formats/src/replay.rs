//! Replays (`Replays/<name>`; OS4 `FUN_010bc000` load, `FUN_010bc3b0`
//! save, record word `FUN_01051a10`; docs/re/frontend_menus.md §8).
//!
//! Big-endian. Header version 3 (CD release, 144 bytes) or 10 / 0x3F2
//! (Update 7 / OS4, 152 bytes), then `records` u16 input words, one per
//! active player per frame. A word with bit 15 set is followed by a
//! NUL-terminated comment, padded to keep the stream 16-bit aligned.

use crate::{Error, Result};

/// Version written by this implementation (the Update 7 layout).
pub const VERSION: i32 = 10;

/// Weapon field value meaning "no change".
pub const NO_WEAPON_CHANGE: u8 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
    pub players: i32,
    /// Controller code per player (-1 off, 0-3 joystick, 4-7 PSX, 8-11
    /// CD32, 12 keyboard).
    pub controllers: [i32; 4],
    pub required_kills: i32,
    /// 30 Hz ticks in the whole replay (sum of the frame tick counts).
    pub duration: i32,
    pub seed: i32,
    pub rampage: bool,
    pub police: i32,
    /// e.g. `PROGDIR:Maps/1`.
    pub map: String,
    /// Level within the city.
    pub level: i32,
}

/// One player's input for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub accelerate: bool,
    pub reverse: bool,
    pub shoot: bool,
    pub use_vehicle: bool,
    pub brake: bool,
    /// Weapon selected this frame, or [`NO_WEAPON_CHANGE`].
    pub weapon: u8,
    /// Game ticks in this frame, 1..=5.
    pub ticks: u8,
}

impl Input {
    pub fn from_word(w: u16) -> Self {
        let bit = |b: u32| w >> b & 1 != 0;
        Self {
            left: bit(14),
            right: bit(13),
            accelerate: bit(12),
            reverse: bit(11),
            shoot: bit(10),
            use_vehicle: bit(9),
            brake: bit(8),
            weapon: (w >> 3 & 0x1f) as u8,
            ticks: (w & 7) as u8,
        }
    }

    pub fn to_word(self) -> u16 {
        let flags = [self.left, self.right, self.accelerate, self.reverse, self.shoot, self.use_vehicle, self.brake];
        let mut w = 0u16;
        for (i, f) in flags.into_iter().enumerate() {
            w |= (f as u16) << (14 - i);
        }
        w | (self.weapon as u16 & 0x1f) << 3 | self.ticks as u16 & 7
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub input: Input,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    pub header: Header,
    /// Interleaved: frame f, player p is `records[f * players + p]`.
    pub records: Vec<Record>,
}

fn i32_at(d: &[u8], o: usize) -> Result<i32> {
    d.get(o..o + 4)
        .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated("replay"))
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

impl Replay {
    pub fn parse(d: &[u8]) -> Result<Self> {
        let version = i32_at(d, 0)?;
        let f = |k: usize| i32_at(d, 4 + 4 * k);
        let controllers = [f(1)?, f(2)?, f(3)?, f(4)?];
        let (rampage, police, map_at, level, mut off) = match version {
            3 => (false, 2, 0x2c, f(9)? & 1, 0x90),
            10 | 0x3f2 => (f(9)? != 0, f(10)?, 0x30, i32_at(d, 0x94)?, 0x98),
            v => {
                return Err(Error::Parse { what: "replay", detail: format!("unknown version {v:#x}") });
            }
        };
        let map = cstr(d.get(map_at..map_at + 100).ok_or(Error::Truncated("replay map"))?);
        let header = Header {
            version,
            players: f(0)?,
            controllers,
            required_kills: f(5)?,
            duration: f(7)?,
            seed: f(8)?,
            rampage,
            police,
            map,
            level,
        };
        let count = f(6)?.max(0) as usize;
        // The game's byte counter starts at 0x14, so padding follows its parity.
        let mut pos = 0x14usize;
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let b = d.get(off..off + 2).ok_or(Error::Truncated("replay records"))?;
            let w = u16::from_be_bytes([b[0], b[1]]);
            off += 2;
            pos += 2;
            let comment = if w & 0x8000 != 0 {
                let len = d[off..].iter().position(|&c| c == 0).ok_or(Error::Truncated("replay comment"))?;
                let s = cstr(&d[off..off + len]);
                off += len + 1;
                pos += len + 1;
                if pos & 1 != 0 {
                    off += 1;
                    pos += 1;
                }
                Some(s)
            } else {
                None
            };
            records.push(Record { input: Input::from_word(w), comment });
        }
        Ok(Self { header, records })
    }

    /// Serialise in the version-10 layout.
    pub fn write(&self) -> Vec<u8> {
        let h = &self.header;
        let mut out = Vec::new();
        let mut put = |v: i32| out.extend_from_slice(&v.to_be_bytes());
        put(VERSION);
        put(h.players);
        for c in h.controllers {
            put(c);
        }
        put(h.required_kills);
        put(self.records.len() as i32);
        put(h.duration);
        put(h.seed);
        put(h.rampage as i32);
        put(h.police);
        let mut name = [0u8; 100];
        for (dst, c) in name.iter_mut().zip(h.map.chars().take(99)) {
            *dst = c as u32 as u8;
        }
        out.extend_from_slice(&name);
        out.extend_from_slice(&h.level.to_be_bytes());
        let mut pos = 0x14usize;
        for r in &self.records {
            let w = r.input.to_word() | (r.comment.is_some() as u16) << 15;
            out.extend_from_slice(&w.to_be_bytes());
            pos += 2;
            if let Some(c) = &r.comment {
                out.extend(c.chars().map(|ch| ch as u32 as u8));
                out.push(0);
                pos += c.chars().count() + 1;
                if pos & 1 != 0 {
                    out.push(0);
                    pos += 1;
                }
            }
        }
        out
    }

    /// City index for the shipped maps (`PROGDIR:Maps/<n>`).
    pub fn city(&self) -> Option<u8> {
        self.header.map.strip_prefix("PROGDIR:Maps/")?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut records = vec![];
        for i in 0..20u16 {
            let input = Input { left: i % 3 == 0, shoot: i % 5 == 0, weapon: NO_WEAPON_CHANGE, ticks: 1 + (i % 5) as u8, ..Default::default() };
            let comment = (i == 7).then(|| "hello".to_string());
            records.push(Record { input, comment });
        }
        let r = Replay {
            header: Header {
                version: VERSION,
                players: 1,
                controllers: [12, -1, -1, -1],
                required_kills: 5,
                duration: records.iter().map(|r| r.input.ticks as i32).sum(),
                seed: 1234,
                rampage: false,
                police: 2,
                map: "PROGDIR:Maps/0".into(),
                level: 1,
            },
            records,
        };
        assert_eq!(Replay::parse(&r.write()).unwrap(), r);
        assert_eq!(r.city(), Some(0));
    }
}
