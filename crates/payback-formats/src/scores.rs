//! Rampage high scores (`Maps/<n>.scores`; OS4 `FUN_01029b3c` load,
//! `FUN_0102a0fc` save; docs/re/frontend_menus.md §7).
//!
//! Big-endian, 1024 bytes: 6 tables (one per police aggressiveness) × 5
//! entries × {char name[30], s32 score}, then a u32 checksum
//! `!(0x5DC1DCD1 ^ 7 BE words of each name ^ every score)`.

pub const TABLES: usize = 6;
pub const ENTRIES: usize = 5;
const NAME: usize = 30;
const ENTRY: usize = NAME + 4;
pub const SIZE: usize = TABLES * ENTRIES * ENTRY + 4;
const SEED: u32 = 0x5dc1_dcd1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub score: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighScores {
    /// `tables[police][rank]`, best first.
    pub tables: [[Entry; ENTRIES]; TABLES],
}

impl Default for HighScores {
    /// "Travis" with 500000·(table+1) at the top, halving down the list.
    fn default() -> Self {
        Self {
            tables: std::array::from_fn(|t| {
                std::array::from_fn(|r| Entry { name: "Travis".into(), score: (500_000 * (t as i32 + 1)) >> r })
            }),
        }
    }
}

fn name_bytes(name: &str) -> [u8; NAME] {
    let mut b = [0u8; NAME];
    for (dst, c) in b.iter_mut().zip(name.chars().take(NAME - 1)) {
        *dst = c as u32 as u8;
    }
    b
}

fn checksum(body: &[u8]) -> u32 {
    let mut sum = SEED;
    for e in body.chunks_exact(ENTRY) {
        for k in 0..7 {
            sum ^= u32::from_be_bytes([e[4 * k], e[4 * k + 1], e[4 * k + 2], e[4 * k + 3]]);
        }
        sum ^= u32::from_be_bytes([e[NAME], e[NAME + 1], e[NAME + 2], e[NAME + 3]]);
    }
    !sum
}

impl HighScores {
    /// Parse a scores file; `None` if the size or checksum is wrong (the
    /// game then falls back to the defaults).
    pub fn parse(d: &[u8]) -> Option<Self> {
        if d.len() < SIZE {
            return None;
        }
        let body = &d[..SIZE - 4];
        let stored = u32::from_be_bytes(d[SIZE - 4..SIZE].try_into().ok()?);
        if checksum(body) != stored {
            return None;
        }
        let mut entries = body.chunks_exact(ENTRY).map(|e| {
            let end = e[..NAME].iter().position(|&c| c == 0).unwrap_or(NAME);
            Entry {
                name: e[..end].iter().map(|&c| c as char).collect(),
                score: i32::from_be_bytes(e[NAME..ENTRY].try_into().unwrap()),
            }
        });
        Some(Self { tables: std::array::from_fn(|_| std::array::from_fn(|_| entries.next().unwrap())) })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SIZE);
        for e in self.tables.iter().flatten() {
            out.extend_from_slice(&name_bytes(&e.name));
            out.extend_from_slice(&e.score.to_be_bytes());
        }
        let sum = checksum(&out);
        out.extend_from_slice(&sum.to_be_bytes());
        out
    }

    /// Rank a score would get in a table, if it makes the top 5.
    pub fn rank(&self, police: usize, score: i32) -> Option<usize> {
        self.tables[police.min(TABLES - 1)].iter().position(|e| score > e.score)
    }

    /// Insert a score, shifting lower entries down. Returns its rank.
    pub fn insert(&mut self, police: usize, name: &str, score: i32) -> Option<usize> {
        let t = police.min(TABLES - 1);
        let rank = self.rank(t, score)?;
        let table = &mut self.tables[t];
        table[rank..].rotate_right(1);
        table[rank] = Entry { name: name.chars().take(NAME - 1).collect(), score };
        Some(rank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let mut s = HighScores::default();
        assert_eq!(s.tables[1][0].score, 1_000_000);
        assert_eq!(s.tables[1][2].score, 250_000);
        assert_eq!(s.insert(0, "Ann", 300_000), Some(1));
        assert_eq!(s.tables[0][2].score, 250_000);
        let bytes = s.write();
        assert_eq!(bytes.len(), SIZE);
        assert_eq!(HighScores::parse(&bytes), Some(s));
        let mut bad = bytes.clone();
        bad[5] ^= 1;
        assert_eq!(HighScores::parse(&bad), None);
    }
}
