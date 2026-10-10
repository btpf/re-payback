//! Classic Mac OS PEF containers (`Joy!peff`): the Mac release's
//! `Payback (OS X)` and `Payback (OS 9)` (Carbon CFM applications).
//!
//! Only what the port needs: the section table and the unpacked contents of
//! each section. A data section is usually stored *pattern-initialized*
//! (section kind 2): a small byte-code that expands to the initialized data.
//! The game's static tables (the palette, `palette::find_in_executable`)
//! exist only in that expanded form, so a search of the raw file finds none
//! of them (`docs/re/mac/assets.md` §15).
//!
//! Layout (Apple, "Mac OS Runtime Architectures", ch. 8): a 40-byte container
//! header (`Joy!` `peff` `pwpc` ..., section count at +32), then one 28-byte
//! header per section (`unpackedLength` +12, `containerLength` +16,
//! `containerOffset` +20, `sectionKind` +24). All big-endian.

use crate::{Error, Result};

const MAGIC: &[u8; 8] = b"Joy!peff";
const HEADER_LEN: usize = 40;
const SECTION_HEADER_LEN: usize = 28;
/// `sectionKind` of a pattern-initialized data section.
const KIND_PIDATA: u8 = 2;

/// `data` starts with the PEF container signature.
pub fn is_pef(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

/// One section, unpacked (`unpackedLength` bytes; the zero-filled rest up to
/// `totalLength` is left out).
pub struct Section {
    pub kind: u8,
    pub data: Vec<u8>,
}

fn be32(d: &[u8], at: usize) -> Result<usize> {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize)
        .ok_or(Error::Truncated("PEF"))
}

/// The sections of a PEF container, pattern-initialized ones expanded.
pub fn sections(file: &[u8]) -> Result<Vec<Section>> {
    if !is_pef(file) {
        return Err(Error::Parse { what: "PEF", detail: "no Joy!peff header".into() });
    }
    let count = file.get(32..34).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or(Error::Truncated("PEF"))?;
    (0..count as usize)
        .map(|i| {
            let h = HEADER_LEN + i * SECTION_HEADER_LEN;
            let unpacked = be32(file, h + 12)?;
            let (len, off) = (be32(file, h + 16)?, be32(file, h + 20)?);
            let kind = *file.get(h + 24).ok_or(Error::Truncated("PEF"))?;
            let raw = file.get(off..off + len).ok_or(Error::Truncated("PEF section"))?;
            let data = if kind == KIND_PIDATA { unpack_pidata(raw, unpacked)? } else { raw.to_vec() };
            Ok(Section { kind, data })
        })
        .collect()
}

/// A variable-length argument: 7 bits per byte, most significant first,
/// the top bit set on every byte but the last.
fn arg(b: &[u8], i: &mut usize) -> Result<usize> {
    let mut v = 0usize;
    loop {
        let c = *b.get(*i).ok_or(Error::Truncated("PEF pidata"))?;
        *i += 1;
        v = (v << 7) | (c & 0x7f) as usize;
        if c & 0x80 == 0 {
            return Ok(v);
        }
    }
}

/// Expands pattern-initialized data. Each instruction byte is an opcode (top
/// 3 bits) and a count (low 5 bits; 0 = a following argument):
/// 0 zero-fill, 1 block copy, 2 repeated block, 3 interleave a repeated
/// block with copied ones, 4 interleave zeros with copied blocks.
pub fn unpack_pidata(b: &[u8], unpacked_len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(unpacked_len);
    let mut i = 0;
    let take = |i: &mut usize, n: usize| -> Result<&[u8]> {
        let s = b.get(*i..*i + n).ok_or(Error::Truncated("PEF pidata"))?;
        *i += n;
        Ok(s)
    };
    while i < b.len() {
        let op = b[i];
        i += 1;
        let mut count = (op & 0x1f) as usize;
        if count == 0 {
            count = arg(b, &mut i)?;
        }
        match op >> 5 {
            0 => out.resize(out.len() + count, 0),
            1 => out.extend_from_slice(take(&mut i, count)?),
            2 => {
                let repeat = arg(b, &mut i)? + 1;
                let block = take(&mut i, count)?;
                for _ in 0..repeat {
                    out.extend_from_slice(block);
                }
            }
            3 => {
                let custom = arg(b, &mut i)?;
                let repeat = arg(b, &mut i)?;
                let common = take(&mut i, count)?;
                out.extend_from_slice(common);
                for _ in 0..repeat {
                    out.extend_from_slice(take(&mut i, custom)?);
                    out.extend_from_slice(common);
                }
            }
            4 => {
                let custom = arg(b, &mut i)?;
                let repeat = arg(b, &mut i)?;
                out.resize(out.len() + count, 0);
                for _ in 0..repeat {
                    out.extend_from_slice(take(&mut i, custom)?);
                    out.resize(out.len() + count, 0);
                }
            }
            k => {
                return Err(Error::Parse { what: "PEF pidata", detail: format!("opcode {k} at {}", i - 1) });
            }
        }
        if out.len() > unpacked_len {
            return Err(Error::Parse { what: "PEF pidata", detail: "expands past the section".into() });
        }
    }
    out.resize(unpacked_len, 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pidata_opcodes() {
        // zero 2; copy "ab"; repeat "xy" 3x; interleave common "-" with
        // 2 customs of 1; zeros(1) interleaved with 1 custom of 2.
        let code = [
            0x02, // zero, 2
            0x22, b'a', b'b', // copy 2
            0x42, 0x02, b'x', b'y', // repeated block of 2, 2+1 times
            0x61, 0x01, 0x02, b'-', b'1', b'2', // interleave
            0x81, 0x02, 0x01, b'p', b'q', // interleave with zero
        ];
        let out = unpack_pidata(&code, 23).unwrap();
        assert_eq!(&out, b"\0\0abxyxyxy-1-2-\0pq\0\0\0\0\0");
    }

    #[test]
    fn long_argument() {
        // Zero-fill with a count of 0x81 = [0x81, 0x01] (1 << 7 | 1).
        let out = unpack_pidata(&[0x00, 0x81, 0x01, 0x21, 7], 0x82).unwrap();
        assert_eq!(out.len(), 0x82);
        assert_eq!(out[0x81], 7);
        assert!(out[..0x81].iter().all(|&b| b == 0));
    }
}
