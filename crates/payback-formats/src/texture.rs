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
//! `#Water.chunky` is 4160 bytes: 64x64 plus a copy of row 0 (64 bytes, for the
//! scrolling reflection; docs/re/rendering.md "Reflect / water").
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

    /// A cube texture that may be short: zero-padded to 64x64. The Mac
    /// `TentRoof.chunky` is 4032 bytes (63 rows); the Mac loader reads a
    /// fixed 0x1000 bytes with no size check (`ReloadCubeTexture` @
    /// `1015ac70`, `GetCubeTexture` @ `1015a9f0`), so its last row is
    /// whatever was in the buffer. Zero is the port's choice.
    pub fn cube_texture_padded(data: &[u8]) -> IndexedImage {
        let mut pixels = data[..data.len().min(64 * 64)].to_vec();
        pixels.resize(64 * 64, 0);
        IndexedImage {
            width: 64,
            height: 64,
            pixels,
        }
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
    /// Per-texture flags; values 4, 34, 35, 36, 50, 51, 52, 65: the footstep
    /// sound id and the vehicle grip class (stored at texture `+0x1040`,
    /// OS4 `FUN_0106dfcc`; docs/formats.md, docs/re/rendering.md §8).
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

/// First texture number of a user map's custom textures
/// ([`crate::map::CUSTOM_TEXTURE_BASE`]).
pub const CUSTOM_TEXTURE_BASE: u16 = crate::map::CUSTOM_TEXTURE_BASE;

/// `<map>.textures`: a user map's own cube textures, slot i = texture
/// number 2000 + i (loader OS4 `FUN_0106e8ec`, reload `FUN_0106eb88`;
/// `docs/formats.md`). Big-endian: `u32 count`, then per slot `u32
/// present`; if 1: `char name[40]`, `u32 flags` (as in `.CubeTextureList`),
/// `u32 gouraud` (texture number used with Gouraud on, like
/// `.CubeTextureRemapList`), 64x64 pixels. A 4-byte file is an empty set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomTextures {
    pub slots: Vec<Option<CustomTexture>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTexture {
    pub name: String,
    pub flags: u32,
    pub gouraud: u16,
    pub image: IndexedImage,
}

impl CustomTextures {
    pub fn parse(data: &[u8]) -> Result<CustomTextures> {
        let mut at = 0;
        let u32_at = |at: &mut usize| -> Result<u32> {
            let b = data
                .get(*at..*at + 4)
                .ok_or(Error::Truncated("custom textures"))?;
            *at += 4;
            Ok(u32::from_be_bytes(b.try_into().unwrap()))
        };
        let count = u32_at(&mut at)?;
        if count > 4096 {
            return Err(Error::Parse {
                what: "custom textures",
                detail: format!("bad count {count}"),
            });
        }
        let mut slots = Vec::with_capacity(count as usize);
        for _ in 0..count {
            if u32_at(&mut at)? != 1 {
                slots.push(None);
                continue;
            }
            let raw = data
                .get(at..at + 40)
                .ok_or(Error::Truncated("custom texture name"))?;
            at += 40;
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            let name = raw[..end].iter().map(|&b| b as char).collect();
            let flags = u32_at(&mut at)?;
            let gouraud = u32_at(&mut at)?.min(u16::MAX as u32) as u16;
            let image = IndexedImage::cube_texture(data.get(at..).unwrap_or_default())?;
            at += 64 * 64;
            slots.push(Some(CustomTexture {
                name,
                flags,
                gouraud,
                image,
            }));
        }
        Ok(CustomTextures { slots })
    }

    /// The custom texture of texture number `texture` (>= 2000).
    pub fn get(&self, texture: u16) -> Option<&CustomTexture> {
        let i = texture.checked_sub(CUSTOM_TEXTURE_BASE)?;
        self.slots.get(i as usize)?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_textures() {
        assert!(CustomTextures::parse(&[0, 0, 0, 0]).unwrap().slots.is_empty());
        let mut d = vec![0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut name = b"$Lake.iff".to_vec();
        name.resize(40, 0);
        d.extend(name);
        d.extend([0, 0, 0, 0x22, 0, 0, 0x07, 0xd1]);
        d.extend(std::iter::repeat_n(7u8, 4096));
        let t = CustomTextures::parse(&d).unwrap();
        assert_eq!(t.slots.len(), 2);
        assert!(t.get(2000).is_none());
        let s = t.get(2001).unwrap();
        assert_eq!((s.name.as_str(), s.flags, s.gouraud), ("$Lake.iff", 0x22, 2001));
        assert_eq!(s.image.pixels[4095], 7);
        assert!(CustomTextures::parse(&d[..d.len() - 1]).is_err());
    }

    #[test]
    fn padded_cube_texture() {
        let img = IndexedImage::cube_texture_padded(&[5; 4032]);
        assert_eq!((img.pixels.len(), img.pixels[4031], img.pixels[4032]), (4096, 5, 0));
    }

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
