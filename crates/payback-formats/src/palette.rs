//! The global 256-colour palette.
//!
//! It is not a separate file: it lives in the executable's data as a
//! `graphics.library/LoadRGB32` table:
//!
//! ```text
//! u32 0x01000000         count (256) << 16 | first colour (0)
//! 256 x (u32 r, g, b)    component in the top byte; low bytes vary by build
//! u32 0                  terminator
//! ```
//!
//! Update 7's `Payback.exe` and `PaybackWOS.exe` hold several identical
//! copies; the original CD release's differs in colour 8 only (7,7,7 for
//! 0,0,0). The Mac release's `Payback (OS X)` and `Payback (OS 9)` hold four
//! copies each, byte-identical to Update 7's, but inside their
//! pattern-initialized PEF data section (`pef.rs`), so they are searched
//! after unpacking it (`docs/re/mac/assets.md` §15). We extract the palette
//! at runtime from the user's install rather than shipping it.

use crate::{Error, Result};

pub type Palette = [u8; 768];

const TABLE_LEN: usize = 4 + 768 * 4 + 4;

/// Find the first LoadRGB32 palette table in an executable image: an Amiga
/// hunk file as it is, a Mac PEF container in its unpacked sections.
pub fn find_in_executable(exe: &[u8]) -> Result<Palette> {
    if crate::pef::is_pef(exe) {
        return crate::pef::sections(exe)?
            .iter()
            .find_map(|s| find_table(&s.data).ok())
            .ok_or_else(|| Error::Parse { what: "palette", detail: "no LoadRGB32 table in the PEF sections".into() });
    }
    find_table(exe)
}

fn find_table(exe: &[u8]) -> Result<Palette> {
    // Header followed by colour 0 = black.
    const SIG: [u8; 16] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut start = 0;
    while let Some(off) = find(&exe[start..], &SIG).map(|o| o + start) {
        if let Some(table) = exe.get(off..off + TABLE_LEN) {
            let terminator = &table[TABLE_LEN - 4..];
            let mut pal = [0u8; 768];
            for (i, c) in pal.iter_mut().enumerate() {
                *c = table[4 + i * 4];
            }
            // Distinct-colour check rejects zero-filled regions.
            let distinct = pal
                .chunks(3)
                .map(|c| (c[0], c[1], c[2]))
                .collect::<std::collections::HashSet<_>>()
                .len();
            if terminator == [0, 0, 0, 0] && distinct > 64 {
                return Ok(pal);
            }
        }
        start = off + 1;
    }
    Err(Error::Parse {
        what: "palette",
        detail: "no LoadRGB32 table found".into(),
    })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_table() {
        let mut exe = vec![0xAA; 100];
        exe.extend_from_slice(&[1, 0, 0, 0]);
        for i in 0..256u32 {
            for c in 0..3 {
                exe.extend_from_slice(&[(i as u8).wrapping_add(c), 0x12, 0x34, 0x56]);
            }
        }
        // Colour 0 must be black for the signature.
        exe[104..116].fill(0);
        exe.extend_from_slice(&[0, 0, 0, 0, 0xBB]);
        let p = find_in_executable(&exe).unwrap();
        assert_eq!(&p[..3], &[0, 0, 0]);
        assert_eq!(&p[3..6], &[1, 2, 3]);
        assert_eq!(&p[765..768], &[255, 0, 1]);
    }

    #[test]
    fn rejects_missing() {
        assert!(find_in_executable(&[0; 5000]).is_err());
    }
}
