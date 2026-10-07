//! Raw 8-bit chunky (one byte per pixel, palette-indexed) images.
//!
//! | Location                  | Size      | Dimensions |
//! |---------------------------|-----------|------------|
//! | `CubeTextures/*.chunky`   | 4096      | 64x64      |
//! | `EffectTextures/*`        | 4096      | 64x64      |
//! | `ObjectTextures/*`        | 32768     | 256x128    |
//! | `Backgrounds/*.chunky`    | 81920     | 320x256    |
//! | `Maps/*.thumb`            | 65536     | 256x256    |
//!
//! `#Water.chunky` is 4160 bytes: 64x64 plus 64 trailing bytes (purpose TBD).
//! Update 4+ adds `Backgrounds/*.chunky2` (320x256 24-bit RGB).
//!
//! Textures use a single global 256-colour palette laid out in ramps: shades
//! of one material occupy consecutive indices (e.g. grass 0x7a-0x7d).

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl IndexedImage {
    /// Interpret the first `width * height` bytes of `data` as a chunky image.
    pub fn from_chunky(data: &[u8], width: usize, height: usize) -> Result<IndexedImage> {
        let n = width * height;
        let pixels = data
            .get(..n)
            .ok_or(Error::Truncated("chunky image"))?
            .to_vec();
        Ok(IndexedImage {
            width,
            height,
            pixels,
        })
    }

    pub fn cube_texture(data: &[u8]) -> Result<IndexedImage> {
        Self::from_chunky(data, 64, 64)
    }

    pub fn object_texture(data: &[u8]) -> Result<IndexedImage> {
        Self::from_chunky(data, 256, 128)
    }

    /// Expand to RGBA8 with `palette` (768 bytes, RGB). `transparent`, if set,
    /// gets alpha 0 (the game's "genlocked" faces treat colour 0 this way).
    pub fn to_rgba(&self, palette: &[u8; 768], transparent: Option<u8>) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &p in &self.pixels {
            let i = p as usize * 3;
            out.extend_from_slice(&palette[i..i + 3]);
            out.push(if Some(p) == transparent { 0 } else { 255 });
        }
        out
    }
}

/// `CubeTextures/.CubeTextureList`: one `<flags> <filename>` per line. A map
/// face's texture number indexes this list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubeTextureList {
    pub entries: Vec<CubeTextureEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubeTextureEntry {
    /// Per-texture flags; observed values 4, 34, 35, 36, 50, 51, 52, 65.
    /// Probably surface material (footstep sound, drivability). TBD.
    pub flags: u32,
    pub file: String,
}

impl CubeTextureList {
    pub fn parse(data: &[u8]) -> Result<CubeTextureList> {
        let text: String = data.iter().map(|&b| b as char).collect();
        let mut entries = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            let (flags, file) = line.split_once(' ').ok_or_else(|| Error::Parse {
                what: "CubeTextureList",
                detail: format!("line {}: missing space", n + 1),
            })?;
            let flags = flags.trim().parse().map_err(|_| Error::Parse {
                what: "CubeTextureList",
                detail: format!("line {}: bad flags {flags:?}", n + 1),
            })?;
            entries.push(CubeTextureEntry {
                flags,
                file: file.to_string(),
            });
        }
        Ok(CubeTextureList { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_list() {
        let l = CubeTextureList::parse(b"4 A.chunky\n50 #Water.chunky\n").unwrap();
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[1].flags, 50);
        assert_eq!(l.entries[1].file, "#Water.chunky");
    }

    #[test]
    fn rgba_expansion() {
        let img = IndexedImage::from_chunky(&[0, 1], 2, 1).unwrap();
        let mut pal = [0u8; 768];
        pal[3..6].copy_from_slice(&[10, 20, 30]);
        assert_eq!(
            img.to_rgba(&pal, Some(0)),
            vec![0, 0, 0, 0, 10, 20, 30, 255]
        );
    }
}
