//! Replays (`Replays/<name>`; OS4 `FUN_010bc000` load, `FUN_010bc3b0`
//! save, record word `FUN_01051a10`; docs/re/frontend_menus.md §8).
//!
//! Big-endian. Header version 3 (CD release, 144 bytes) or 10 / 0x3F2
//! (Update 7 / OS4, 152 bytes), then `records` u16 input words, one per
//! active player per frame. A word with bit 15 set is followed by a
//! NUL-terminated comment, padded to keep the stream 16-bit aligned.
//!
//! Version 0x3F3 is the Mac release's (`SaveMovie` @ 100fab30, `LoadMovie`
//! @ 100fa7b0; docs/re/mac_version.md): the same twelve header words, then
//! the map path as a NUL-terminated string of any length (`PROGDIR:Maps/<n>`
//! or `PROGDIR:UserMaps/<name>`, from `GetCityFilename` @ 10084c90), the
//! level, a u32 byte count and that many bytes of records. The Mac loader
//! reads the rampage word as `== 1` and ignores anything after the records.
//!
//! The Mac records are not raw words but a bit stream ([`mac_stream`]): the
//! same 16-bit words (no comments; bit 15 is never a comment flag there),
//! run-length coded, with 4-bit codes for single-field changes.
//!
//! Extension: after the records this implementation appends [`TRAILER`] and
//! a u32 of flags: bit 0 random phone missions in Rampage (they change the
//! simulation), bit 1 the records are the Mac bit stream, bit 2 a
//! [`RandomSetup`] follows (two more i32: the money goal and the story
//! progress the random missions were started with). The Mac game
//! ignores the trailer. A 0x3F3 file without it is a Mac recording: a bit
//! stream, random missions on (the Mac always plays them). With the trailer
//! but without bit 1 the records are raw words (this implementation's
//! first 0x3F3 files). Older versions come from the Amiga builds, which
//! have no random missions.
//!
//! Bit 3 (port addition, not in any original): cheat events follow, after
//! the [`RandomSetup`] if there is one: a u32 count, then per event three
//! u32/i32 words: the game tick it was applied at, its kind and an
//! argument ([`CheatEvent`]; kinds are defined by the game, `cheats.rs`).
//! Readers that predate the flag stop before it, so files without cheats
//! are unchanged.

use crate::{Error, Result};

/// Version written by this implementation (the Mac layout).
pub const VERSION: i32 = 0x3f3;

/// Marks the flags this implementation appends to a 0x3F3 file.
pub const TRAILER: &[u8; 4] = b"PBrm";

/// Trailer flag: Rampage with random phone missions.
pub const FLAG_RANDOM_MISSIONS: u32 = 1;
/// Trailer flag: the records are the Mac bit stream.
pub const FLAG_MAC_STREAM: u32 = 2;
/// Trailer flag: a [`RandomSetup`] (goal, progress; two i32) follows the
/// flags.
pub const FLAG_RANDOM_SETUP: u32 = 4;
/// Trailer flag (port addition): [`CheatEvent`]s follow (a u32 count,
/// then 12 bytes each).
pub const FLAG_CHEATS: u32 = 8;

/// A cheat applied during the recorded game (port addition, not in the
/// original): at the start of game tick `tick` (the replay's tick counter,
/// 1 = the first tick), action `kind` with argument `arg`. The kinds are
/// the game's (`crates/payback/src/cheats.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheatEvent {
    pub tick: u32,
    pub kind: u32,
    pub arg: i32,
}

/// What the Rampage random missions read from outside the replay at level
/// start: the money goal (top of the Rampage high-score table for the
/// police level, `GetHighestScore` @ 100a05a0) and the story progress
/// (`GetHighestLevelCompleted` @ 1009b580, which limits the vehicles drawn
/// for the jobs). Both change the simulation, so they are recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RandomSetup {
    pub goal: i32,
    pub progress: i32,
}

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
    /// Rampage with random phone missions (Mac `ProcessTrigger_Random`);
    /// false outside Rampage.
    pub random_missions: bool,
    /// The random missions' outside inputs, when recorded (files before
    /// [`FLAG_RANDOM_SETUP`] and Mac files have none).
    pub random_setup: Option<RandomSetup>,
    /// Cheats applied during the game, in tick order ([`FLAG_CHEATS`];
    /// empty for every original file).
    pub cheats: Vec<CheatEvent>,
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

/// The fixed 100-byte map name of versions 3, 10 and 0x3F2.
fn fixed(d: &[u8], at: usize) -> Result<&[u8]> {
    d.get(at..at + 100).ok_or(Error::Truncated("replay map"))
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
        // (rampage, police, map, level, first record, end of the records)
        let (rampage, police, map, level, mut off, end) = match version {
            3 => (false, 2, cstr(fixed(d, 0x2c)?), f(9)? & 1, 0x90, d.len()),
            10 | 0x3f2 => (f(9)? != 0, f(10)?, cstr(fixed(d, 0x30)?), i32_at(d, 0x94)?, 0x98, d.len()),
            0x3f3 => {
                // `LoadMovie`: rampage = (word == 1); the path is read
                // byte by byte up to the NUL (at most 0x100).
                let tail = d.get(0x30..).ok_or(Error::Truncated("replay map"))?;
                let len = tail.iter().take(0x100).position(|&c| c == 0).ok_or(Error::Truncated("replay map"))?;
                let at = 0x30 + len + 1;
                let level = i32_at(d, at)?;
                let bytes = i32_at(d, at + 4)? as u32 as usize;
                let start = at + 8;
                let end = start.checked_add(bytes).filter(|&e| e <= d.len()).ok_or(Error::Truncated("replay records"))?;
                (f(9)? == 1, f(10)?, cstr(&tail[..len]), level, start, end)
            }
            v => {
                return Err(Error::Parse { what: "replay", detail: format!("unknown version {v:#x}") });
            }
        };
        // The trailer's flags; a Mac file has none.
        let flags = match d.get(end..end + 8) {
            _ if version != 0x3f3 => 0,
            Some(t) if &t[..4] == TRAILER => u32::from_be_bytes([t[4], t[5], t[6], t[7]]),
            _ => FLAG_RANDOM_MISSIONS | FLAG_MAC_STREAM,
        };
        let random_missions = rampage && flags & FLAG_RANDOM_MISSIONS != 0;
        let random_setup = if version == 0x3f3 && flags & FLAG_RANDOM_SETUP != 0 {
            Some(RandomSetup { goal: i32_at(d, end + 8)?, progress: i32_at(d, end + 12)? })
        } else {
            None
        };
        let mut cheats = Vec::new();
        if version == 0x3f3 && flags & FLAG_CHEATS != 0 {
            let at = end + 8 + (if random_setup.is_some() { 8 } else { 0 });
            let n = i32_at(d, at)? as u32 as usize;
            for i in 0..n {
                let o = at + 4 + 12 * i;
                cheats.push(CheatEvent {
                    tick: i32_at(d, o)? as u32,
                    kind: i32_at(d, o + 4)? as u32,
                    arg: i32_at(d, o + 8)?,
                });
            }
        }
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
            random_missions,
            random_setup,
            cheats,
        };
        let count = f(6)?.max(0) as usize;
        if flags & FLAG_MAC_STREAM != 0 {
            let players = header.players.clamp(1, 4) as usize;
            let records = mac_stream::decode(&d[off..end], players, count)
                .into_iter()
                .map(|w| Record { input: Input::from_word(w), comment: None })
                .collect();
            return Ok(Self { header, records });
        }
        let d = &d[..end];
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

    /// Serialise in the version-0x3F3 layout (`SaveMovie` @ 100fab30), the
    /// records as the Mac bit stream, plus the [`TRAILER`] flags.
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
        out.extend(h.map.chars().take(0xff).map(|c| c as u32 as u8).filter(|&c| c != 0));
        out.push(0);
        out.extend_from_slice(&(h.level & 1).to_be_bytes());
        // The Mac bit stream, without its lossy shortcuts; comments cannot
        // be stored in it.
        let words: Vec<u16> = self.records.iter().map(|r| r.input.to_word()).collect();
        let data = mac_stream::encode(&words, h.players.clamp(1, 4) as usize);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(&data);
        out.extend_from_slice(TRAILER);
        let random = if h.rampage && h.random_missions { FLAG_RANDOM_MISSIONS } else { 0 };
        let setup = if h.random_setup.is_some() { FLAG_RANDOM_SETUP } else { 0 };
        let cheats = if h.cheats.is_empty() { 0 } else { FLAG_CHEATS };
        out.extend_from_slice(&(random | setup | cheats | FLAG_MAC_STREAM).to_be_bytes());
        if let Some(r) = h.random_setup {
            out.extend_from_slice(&r.goal.to_be_bytes());
            out.extend_from_slice(&r.progress.to_be_bytes());
        }
        if !h.cheats.is_empty() {
            out.extend_from_slice(&(h.cheats.len() as u32).to_be_bytes());
            for c in &h.cheats {
                out.extend_from_slice(&c.tick.to_be_bytes());
                out.extend_from_slice(&c.kind.to_be_bytes());
                out.extend_from_slice(&c.arg.to_be_bytes());
            }
        }
        out
    }

    /// City index for the shipped maps (`PROGDIR:Maps/<n>`).
    pub fn city(&self) -> Option<u8> {
        self.header.map.strip_prefix("PROGDIR:Maps/")?.parse().ok()
    }

    /// The user map's name for `PROGDIR:UserMaps/<name>` (`AddUserMap`
    /// @ 10084b30 builds these paths).
    pub fn user_map(&self) -> Option<&str> {
        self.header.map.strip_prefix("PROGDIR:UserMaps/").filter(|n| !n.is_empty())
    }
}

/// The Mac's replay record stream (CONFIRMED: `AddFrame` @ 100fa5d0,
/// `GetAbbreviatedVals` @ 100fa2b0, `GetNextFrame` @ 100f9ea0, `WriteBits`
/// / `ReadBits` @ 100f9700 / 100f97b0, `TidyMovie` @ 100faaa0;
/// docs/re/mac/input_movie.md §1).
///
/// The words are the Amiga's (`FUN_01051a10`): flags in bits 14..8, the
/// weapon in 7..3 (10 = none), ticks in 2..0. Bits are written LSB first
/// from bit 0 of each byte. Per player `p` (cycling through the players in
/// record order) the coder keeps the last word (initially 0xFFFFFFFF) and
/// the last selected weapon (initially 0); one run counter is shared. An
/// entry is `1` + a 16-bit literal or `0` + a 4-bit code applied to the
/// player's last word, followed by its run length: 4-bit counts, each 0
/// adding 15, ended by a nonzero count. The literal 0xFFFF ends the stream.
///
/// | Code | Change |
/// |---|---|
/// | 0 / 1 | steering (−1 left wins, 0, +1 right) +1 / −1 cyclic |
/// | 2 / 3 | pedal (−1 accelerate wins, 0, +1 reverse) +1 / −1 cyclic |
/// | 4 / 5 / 6 | fire / in-out / brake toggled |
/// | 7..13 | weapon = last selected + code − 6, mod 8 |
/// | 14 / 15 | ticks +1 / −1 cyclic in 1..5 |
///
/// The Mac encoder is lossy in four cases: a no-change weapon word after a
/// select is merged into a repeat (the select is played again), both
/// directions held come back as one, and a weapon change together with a
/// tick change is coded as the tick change only. [`decode`] reproduces
/// them; [`encode`] avoids them (no merge, and a literal whenever the code
/// would not decode back to the word), so it still writes a valid Mac
/// stream.
pub mod mac_stream {
    const LEFT: u32 = 0x4000;
    const RIGHT: u32 = 0x2000;
    const ACCEL: u32 = 0x1000;
    const REVERSE: u32 = 0x800;
    const NO_WEAPON: u32 = 0x50;
    /// End of the records (`TidyMovie`).
    pub const END: u16 = 0xffff;

    struct Writer {
        bytes: Vec<u8>,
        cur: u8,
        bit: u32,
    }

    impl Writer {
        fn put(&mut self, v: u32, n: u32) {
            for i in 0..n {
                self.cur |= ((v >> i & 1) as u8) << self.bit;
                self.bit += 1;
                if self.bit == 8 {
                    self.bytes.push(self.cur);
                    self.cur = 0;
                    self.bit = 0;
                }
            }
        }
    }

    struct Reader<'a> {
        d: &'a [u8],
        pos: usize,
        bit: u32,
    }

    impl Reader<'_> {
        fn get(&mut self, n: u32) -> u32 {
            let mut v = 0;
            for i in 0..n {
                let byte = self.d.get(self.pos).copied().unwrap_or(0) as u32;
                v |= (byte >> self.bit & 1) << i;
                self.bit += 1;
                if self.bit == 8 {
                    self.pos += 1;
                    self.bit = 0;
                }
            }
            v
        }
    }

    /// −1 / 0 / +1 from a (negative, positive) bit pair, the negative one
    /// winning.
    fn axis(w: u32, neg: u32, pos: u32) -> i32 {
        if w & neg != 0 {
            -1
        } else if w & pos != 0 {
            1
        } else {
            0
        }
    }

    /// The code for `last` -> `v` (`GetAbbreviatedVals`), None = literal.
    fn abbreviate(last: u32, v: u32, last_weapon: u32) -> Option<u32> {
        let x = last ^ v;
        let mut code: Option<u32> = None;
        let step = |a: i32, b: i32, up: u32, down: u32| match b - a {
            1 | -2 => Some(up),
            -1 | 2 => Some(down),
            _ => None,
        };
        if x & (LEFT | RIGHT) != 0 {
            code = step(axis(last, LEFT, RIGHT), axis(v, LEFT, RIGHT), 0, 1);
        }
        if x & (ACCEL | REVERSE) != 0 {
            if code.is_some() {
                return None;
            }
            code = step(axis(last, ACCEL, REVERSE), axis(v, ACCEL, REVERSE), 2, 3);
        }
        for (bit, c) in [(0x400, 4), (0x200, 5), (0x100, 6)] {
            if x & bit != 0 {
                if code.is_some() {
                    return None;
                }
                code = Some(c);
            }
        }
        if x & 0xf8 != 0 {
            if code.is_some() {
                return None;
            }
            if v & 0xf8 != NO_WEAPON {
                let d = (v & 0xf8) as i32 / 8 - last_weapon as i32;
                code = match d {
                    1..=7 => Some(6 + d as u32),
                    -7..=-1 => Some((14 + d) as u32),
                    // The last selected weapon again: no code.
                    0 => None,
                    // Out of range: the "GetAbbreviatedVals" message.
                    _ => return None,
                };
            }
        }
        if x & 7 != 0 {
            if code.is_some() {
                return None;
            }
            code = match (v & 7) as i32 - (last & 7) as i32 {
                1 | -4 => Some(14),
                -1 | 4 => Some(15),
                _ => None,
            };
        }
        code
    }

    /// Apply a code to the player's last word (`GetNextFrame`).
    fn apply(code: u32, last: u32, last_weapon: u32) -> u32 {
        apply_wide(code, last, last_weapon) & 0xffff
    }

    fn apply_wide(code: u32, last: u32, last_weapon: u32) -> u32 {
        let cycle = |w: u32, neg: u32, pos: u32, up: bool| {
            let rest = w & !(neg | pos);
            let a = axis(w, neg, pos);
            let n = if up { if a == 1 { -1 } else { a + 1 } } else if a == -1 { 1 } else { a - 1 };
            rest | match n {
                -1 => neg,
                1 => pos,
                _ => 0,
            }
        };
        match code {
            0 => cycle(last, LEFT, RIGHT, true),
            1 => cycle(last, LEFT, RIGHT, false),
            2 => cycle(last, ACCEL, REVERSE, true),
            3 => cycle(last, ACCEL, REVERSE, false),
            4 => last ^ 0x400,
            5 => last ^ 0x200,
            6 => last ^ 0x100,
            7..=13 => {
                let mut w = last_weapon as i32 + code as i32 - 6;
                if w > 7 {
                    w -= 8;
                }
                ((w as u32) << 3 | last & 0xffff_ff07) & 0xffff
            }
            14 => {
                let t = if last & 7 >= 5 { 1 } else { (last & 7) + 1 };
                t | last & 0xfff8
            }
            _ => {
                let t = (last & 7) as i32 - 1;
                let t = if t < 1 { 5 } else { t as u32 };
                (t | last & 0xffff_fff8) & 0xffff
            }
        }
    }

    /// Up to `count` words from a Mac stream of `players` (1..4), stopping
    /// at the end word or the end of the data.
    pub fn decode(d: &[u8], players: usize, count: usize) -> Vec<u16> {
        let players = players.clamp(1, 4);
        let mut r = Reader { d, pos: 0, bit: 0 };
        let mut last = [0xffff_ffffu32; 4];
        let mut weapon = [0u32; 4];
        let mut run: i32 = 0;
        let mut out = Vec::new();
        for i in 0..count {
            if r.pos >= d.len() {
                break;
            }
            let p = i % players;
            run -= 1;
            let w = if run >= 1 {
                last[p] & 0xffff
            } else {
                let w = if r.get(1) == 0 {
                    apply(r.get(4), last[p], weapon[p])
                } else {
                    r.get(16)
                };
                if w & 0xf8 != NO_WEAPON {
                    weapon[p] = (w & 0xf8) >> 3;
                }
                last[p] = w;
                let mut n = r.get(4) as i32;
                run = n;
                while n == 0 {
                    run += 15;
                    n = r.get(4) as i32;
                    run += n;
                }
                w & 0xffff
            };
            if w == END as u32 {
                break;
            }
            out.push(w as u16);
        }
        out
    }

    /// Encode words (records interleaved by player) as a lossless Mac
    /// stream, end marker included; the byte count is the stream's length
    /// (the Mac's counts the partial last byte, even when empty).
    pub fn encode(words: &[u16], players: usize) -> Vec<u8> {
        let players = players.clamp(1, 4);
        let mut w = Writer { bytes: Vec::new(), cur: 0, bit: 0 };
        let mut last = [0xffff_ffffu32; 4];
        let mut weapon = [0u32; 4];
        let mut run = 0u32;
        for (i, &v) in words.iter().enumerate() {
            let p = i % players;
            let v = v as u32 & 0x7fff;
            if v == last[p] {
                run += 1;
                if run > 15 {
                    w.put(0, 4);
                    run = 1;
                }
                continue;
            }
            if run > 0 {
                w.put(run, 4);
            }
            let code = abbreviate(last[p] & 0xffff, v, weapon[p])
                .filter(|&c| apply(c, last[p], weapon[p]) & 0xffff == v);
            if v & 0xf8 != NO_WEAPON {
                weapon[p] = (v & 0xf8) >> 3;
            }
            match code {
                Some(c) => {
                    w.put(0, 1);
                    w.put(c, 4);
                }
                None => {
                    w.put(1, 1);
                    w.put(v, 16);
                }
            }
            run = 1;
            last[p] = v;
        }
        if run > 0 {
            w.put(run, 4);
        }
        w.put(1, 1);
        w.put(END as u32, 16);
        w.put(1, 4);
        let mut bytes = w.bytes;
        bytes.push(w.cur);
        bytes
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The Mac's own encoder (`AddFrame`), lossy shortcuts included.
        fn mac_encode(words: &[u16], players: usize) -> Vec<u8> {
            let mut w = Writer { bytes: Vec::new(), cur: 0, bit: 0 };
            let mut last = [0xffff_ffffu32; 4];
            let mut weapon = [0u32; 4];
            let mut run = 0u32;
            for (i, &v) in words.iter().enumerate() {
                let p = i % players;
                let v = v as u32;
                if v & 0xf8 == NO_WEAPON && v & !0xf8 == last[p] & !0xf8 {
                    last[p] = v;
                }
                if v == last[p] {
                    run += 1;
                    if run > 15 {
                        w.put(0, 4);
                        run = 1;
                    }
                    continue;
                }
                if run > 0 {
                    w.put(run, 4);
                }
                let code = abbreviate(last[p] & 0xffff, v, weapon[p]);
                if v & 0xf8 != NO_WEAPON {
                    weapon[p] = (v & 0xf8) >> 3;
                }
                match code {
                    Some(c) => {
                        w.put(0, 1);
                        w.put(c, 4);
                    }
                    None => {
                        w.put(1, 1);
                        w.put(v, 16);
                    }
                }
                run = 1;
                last[p] = v;
            }
            if run > 0 {
                w.put(run, 4);
            }
            w.put(1, 1);
            w.put(0xffff, 16);
            w.put(1, 4);
            let mut bytes = w.bytes;
            bytes.push(w.cur);
            bytes
        }

        /// A word from a little generator: flags, weapon 0..7 or none,
        /// ticks 1..5, sometimes both directions held.
        fn words(n: usize, seed: u32) -> Vec<u16> {
            let mut x = seed;
            let mut next = || {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                x >> 16
            };
            let mut w = 0x51u16;
            (0..n)
                .map(|_| {
                    match next() % 8 {
                        0 => w ^= 1 << (8 + next() % 7),
                        1 => w = (w & !0xf8) | ((next() % 8) as u16) << 3,
                        2 => w = (w & !0xf8) | 0x50,
                        3 => w = (w & !7) | (1 + next() % 5) as u16,
                        4 => w = (next() & 0x7fff) as u16 & !7 | 1,
                        _ => {}
                    }
                    w
                })
                .collect()
        }

        #[test]
        fn lossless_round_trip() {
            for players in 1..=4 {
                for seed in 0..20 {
                    let w = words(3000, seed);
                    let b = encode(&w, players);
                    assert_eq!(decode(&b, players, w.len() + 5), w, "players {players} seed {seed}");
                }
            }
        }

        #[test]
        fn mac_stream_layout() {
            // One literal 0x0051 with a run of 3, then the end marker.
            let b = encode(&[0x51, 0x51, 0x51], 1);
            let mut r = Reader { d: &b, pos: 0, bit: 0 };
            assert_eq!((r.get(1), r.get(16), r.get(4)), (1, 0x51, 3));
            assert_eq!((r.get(1), r.get(16), r.get(4)), (1, 0xffff, 1));
            // A fire toggle is code 4; a run of 17 is 0 (+15) then 2.
            let mut w = vec![0x51u16; 17];
            w[0] = 0x451;
            let b = encode(&w, 1);
            let mut r = Reader { d: &b, pos: 0, bit: 0 };
            assert_eq!((r.get(1), r.get(16), r.get(4)), (1, 0x451, 1));
            assert_eq!((r.get(1), r.get(4), r.get(4), r.get(4)), (0, 4, 0, 1));
        }

        /// The Mac's shortcuts: a weapon select followed by a no-change word
        /// is played again (the merge); both directions come back as left.
        #[test]
        fn mac_losses() {
            let w = [0x51, 0x19, 0x51, 0x51];
            assert_eq!(decode(&mac_encode(&w, 1), 1, 4), [0x51, 0x19, 0x19, 0x19]);
            assert_eq!(decode(&encode(&w, 1), 1, 4), w);
            let w = [0x2051, 0x6051];
            assert_eq!(decode(&mac_encode(&w, 1), 1, 2), [0x2051, 0x4051]);
            assert_eq!(decode(&encode(&w, 1), 1, 2), w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(rampage: bool, random_missions: bool, map: &str) -> Replay {
        let mut records = vec![];
        for i in 0..20u16 {
            let input = Input { left: i % 3 == 0, shoot: i % 5 == 0, weapon: NO_WEAPON_CHANGE, ticks: 1 + (i % 5) as u8, ..Default::default() };
            // The Mac stream has no comments.
            let comment = None;
            records.push(Record { input, comment });
        }
        Replay {
            header: Header {
                version: VERSION,
                players: 1,
                controllers: [12, -1, -1, -1],
                required_kills: 5,
                duration: records.iter().map(|r| r.input.ticks as i32).sum(),
                seed: 1234,
                rampage,
                police: 2,
                map: map.into(),
                level: 1,
                random_missions,
                random_setup: None,
                cheats: vec![],
            },
            records,
        }
    }

    #[test]
    fn round_trip() {
        let r = sample(false, false, "PROGDIR:Maps/0");
        assert_eq!(Replay::parse(&r.write()).unwrap(), r);
        assert_eq!(r.city(), Some(0));
        for rm in [false, true] {
            let r = sample(true, rm, "PROGDIR:UserMaps/The Killing Zone");
            let b = r.write();
            assert_eq!(Replay::parse(&b).unwrap(), r);
            assert_eq!(r.user_map(), Some("The Killing Zone"));
            // A Mac file (no trailer) is Rampage with random missions.
            let mac = Replay::parse(&b[..b.len() - 8]).unwrap();
            assert!(mac.header.random_missions);
        }
    }

    /// The 0x3F3 header: twelve words, the path and its NUL, the level,
    /// the byte count, the records.
    #[test]
    fn mac_layout() {
        let r = sample(true, true, "PROGDIR:Maps/2");
        let b = r.write();
        assert_eq!(&b[0..4], &[0, 0, 3, 0xf3]);
        let at = 0x30 + "PROGDIR:Maps/2".len();
        assert_eq!(&b[0x30..at], b"PROGDIR:Maps/2");
        assert_eq!(b[at], 0);
        assert_eq!(i32_at(&b, at + 1).unwrap(), 1);
        let bytes = i32_at(&b, at + 5).unwrap() as usize;
        assert_eq!(b.len(), at + 9 + bytes + 8);
    }

    /// The random missions' goal and progress round-trip in the trailer;
    /// files without them (older ones, Mac ones) still load.
    #[test]
    fn random_setup() {
        let mut r = sample(true, true, "PROGDIR:Maps/1");
        r.header.random_setup = Some(RandomSetup { goal: 1_250_000, progress: 3 });
        let b = r.write();
        assert_eq!(Replay::parse(&b).unwrap(), r);
        assert_eq!(&b[b.len() - 8..b.len() - 4], &1_250_000i32.to_be_bytes());
        // An older trailer (flags only): no setup.
        let mut old = b[..b.len() - 8].to_vec();
        let n = old.len();
        old[n - 1] &= !(FLAG_RANDOM_SETUP as u8);
        let p = Replay::parse(&old).unwrap();
        assert_eq!(p.header.random_setup, None);
        assert_eq!(p.records, r.records);
        // A Mac file: no trailer at all.
        let mac = Replay::parse(&b[..b.len() - 16]).unwrap();
        assert!(mac.header.random_missions && mac.header.random_setup.is_none());
    }

    /// Cheat events (port addition) round-trip after the random setup;
    /// cutting them off leaves a file an older build wrote.
    #[test]
    fn cheat_events() {
        let mut r = sample(true, true, "PROGDIR:Maps/1");
        r.header.cheats = vec![
            CheatEvent { tick: 12, kind: 1, arg: 1 },
            CheatEvent { tick: 40, kind: 3, arg: 11 },
        ];
        for setup in [None, Some(RandomSetup { goal: 7, progress: 2 })] {
            r.header.random_setup = setup;
            let b = r.write();
            assert_eq!(Replay::parse(&b).unwrap(), r);
            assert_eq!(&b[b.len() - 12..b.len() - 8], &40u32.to_be_bytes());
            // Without the block and its flag: the same game, no cheats.
            let mut old = b[..b.len() - 4 - 24].to_vec();
            let f = old.len() - (if setup.is_some() { 8 } else { 0 }) - 1;
            old[f] &= !(FLAG_CHEATS as u8);
            let p = Replay::parse(&old).unwrap();
            assert!(p.header.cheats.is_empty());
            assert_eq!((p.header.random_setup, &p.records), (setup, &r.records));
        }
        // No cheats: no flag, no block.
        r.header.cheats.clear();
        let b = r.write();
        assert_eq!(Replay::parse(&b).unwrap(), r);
    }

    /// Versions 10 / 0x3F2 (fixed 100-byte path at 0x30, level at 0x94).
    #[test]
    fn reads_update7() {
        let r = sample(true, true, "PROGDIR:Maps/1");
        let mut b = Vec::new();
        b.extend_from_slice(&10i32.to_be_bytes());
        b.extend_from_slice(&r.write()[4..0x30]);
        let mut name = [0u8; 100];
        name[..14].copy_from_slice(b"PROGDIR:Maps/1");
        b.extend_from_slice(&name);
        b.extend_from_slice(&1i32.to_be_bytes());
        for rec in &r.records {
            let w = rec.input.to_word() | (rec.comment.is_some() as u16) << 15;
            b.extend_from_slice(&w.to_be_bytes());
            if let Some(c) = &rec.comment {
                b.extend_from_slice(c.as_bytes());
                b.push(0);
            }
        }
        let p = Replay::parse(&b).unwrap();
        assert_eq!(p.header.version, 10);
        assert_eq!(p.records, r.records);
        assert_eq!(p.city(), Some(1));
        assert!(p.header.rampage && !p.header.random_missions, "the Amiga has no random missions");
    }

    /// The first 0x3F3 files of this implementation: raw words (with
    /// comments) and a trailer without the stream flag.
    #[test]
    fn reads_raw_0x3f3() {
        let mut r = sample(true, true, "PROGDIR:Maps/0");
        r.records[7].comment = Some("hello".into());
        let new = r.write();
        let at = 0x30 + "PROGDIR:Maps/0".len() + 1 + 4;
        let mut b = new[..at].to_vec();
        let mut body = Vec::new();
        let mut pos = 0x14usize;
        for rec in &r.records {
            let w = rec.input.to_word() | (rec.comment.is_some() as u16) << 15;
            body.extend_from_slice(&w.to_be_bytes());
            pos += 2;
            if let Some(c) = &rec.comment {
                body.extend_from_slice(c.as_bytes());
                body.push(0);
                pos += c.len() + 1;
                if pos & 1 != 0 {
                    body.push(0);
                    pos += 1;
                }
            }
        }
        b.extend_from_slice(&(body.len() as u32).to_be_bytes());
        b.extend_from_slice(&body);
        b.extend_from_slice(TRAILER);
        b.extend_from_slice(&FLAG_RANDOM_MISSIONS.to_be_bytes());
        assert_eq!(Replay::parse(&b).unwrap(), r);
    }
}
