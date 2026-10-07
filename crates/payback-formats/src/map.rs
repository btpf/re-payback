//! `Maps/<n>.map`, `UserMaps/*.map` — the city grid.
//!
//! Recovered from `LoadMap` (OS4 build `FUN_01079300`). Big-endian.
//!
//! ```text
//! u8   0xFF
//! u32  1               (version?)
//! u32  night           1 = night textures/lighting (map 2)
//! u32  variant         0/1/2 for maps 0/1/2; weather or map id, TBD
//! 128 x 128 squares, x-major (outer loop x, inner loop y):
//!   u8 kinds           2 bits per cube, cube 0 in bits 0-1
//!   3 cubes, bottom to top, each by kind:
//!     0: i16 height                      cube off, still occupies height
//!     1: CubeHeader (6 bytes) + top FaceWord
//!     2,3: CubeHeader + FaceWord x5      top, north, east, south, west
//! CubeHeader: u8 slope<<5 | special, u8 rotation, u8 scenery, u8 flags,
//!             i8 height1, i8 height2     (heights are scaled by 16)
//! FaceWord:   u16 flip<<15 | type<<12 | texture
//! ```
//!
//! A texture number below 2000 indexes `CubeTextures/.CubeTextureList`;
//! 2000+ are per-map custom textures (`<map>.textures`).

use crate::{Error, Result};

pub const MAP_SIZE: usize = 128;
pub const CUBES_PER_SQUARE: usize = 3;
pub const CUSTOM_TEXTURE_BASE: u16 = 2000;

/// Cube shape, as named by the map editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Slope {
    /// Invisible; still contributes its height to the stack.
    #[default]
    Off,
    Flat,
    /// Top slopes along the north-south axis.
    Vert,
    /// Top slopes along the east-west axis.
    Horiz,
    RVert,
    RHoriz,
    Unknown(u8),
}

impl Slope {
    fn from_bits(b: u8) -> Slope {
        match b {
            0 => Slope::Off,
            1 => Slope::Flat,
            2 => Slope::Vert,
            3 => Slope::Horiz,
            4 => Slope::RVert,
            5 => Slope::RHoriz,
            n => Slope::Unknown(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaceType {
    #[default]
    Off,
    Normal,
    /// Colour 0 is transparent.
    Genlocked,
    /// 50% transparent; top faces only.
    Alpha,
    /// Texture used as a diffusion map; `#Water.chunky` scrolls.
    Reflect,
    Unknown(u8),
}

impl FaceType {
    fn from_bits(b: u8) -> FaceType {
        match b {
            0 => FaceType::Off,
            1 => FaceType::Normal,
            2 => FaceType::Genlocked,
            3 => FaceType::Alpha,
            4 => FaceType::Reflect,
            n => FaceType::Unknown(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Face {
    pub kind: FaceType,
    pub texture: u16,
    pub flipped: bool,
}

impl Face {
    fn from_word(w: u16) -> Face {
        Face {
            kind: FaceType::from_bits(((w >> 12) & 7) as u8),
            texture: w & 0xFFF,
            flipped: w & 0x8000 != 0,
        }
    }

    pub fn visible(&self) -> bool {
        self.kind != FaceType::Off
    }
}

/// Face order within [`Cube::faces`].
pub const TOP: usize = 0;
pub const NORTH: usize = 1;
pub const EAST: usize = 2;
pub const SOUTH: usize = 3;
pub const WEST: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cube {
    pub slope: Slope,
    /// Height in world units (file value x 16). For sloped cubes, one end.
    pub height1: i16,
    /// Other end of a sloped cube; equals `height1` when flat.
    pub height2: i16,
    /// "Special parameter" (5 bits): crate contents, traffic light
    /// sequence, start point number, phone reference.
    pub special: u8,
    /// Scenery object rotation, 0-255 = full turn.
    pub rotation: u8,
    /// Scenery object id, 0 = none. 14 = phone.
    pub scenery: u8,
    /// Square flags. 0x20 = pedestrians may walk here; low bits are probably
    /// traffic flow (N/E/S/W/straight) and the roadblock flag. TBD.
    pub flags: u8,
    pub faces: [Face; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Square {
    pub cubes: [Cube; CUBES_PER_SQUARE],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub version: u32,
    pub night: bool,
    pub variant: u32,
    /// Indexed `[x * MAP_SIZE + y]`, matching file order.
    pub squares: Vec<Square>,
}

impl Map {
    pub fn square(&self, x: usize, y: usize) -> &Square {
        &self.squares[x * MAP_SIZE + y]
    }

    pub fn parse(data: &[u8]) -> Result<Map> {
        let mut r = Reader { data, pos: 0 };
        let magic = r.u8()?;
        if magic != 0xFF {
            return Err(Error::Parse {
                what: "map",
                detail: format!("bad magic {magic:#x}"),
            });
        }
        let version = r.u32()?;
        let night = r.u32()? == 1;
        let variant = r.u32()?;

        let mut squares = Vec::with_capacity(MAP_SIZE * MAP_SIZE);
        for _ in 0..MAP_SIZE * MAP_SIZE {
            let kinds = r.u8()?;
            let mut sq = Square::default();
            for (i, cube) in sq.cubes.iter_mut().enumerate() {
                match (kinds >> (2 * i)) & 3 {
                    0 => {
                        let h = r.i16()?;
                        cube.height1 = h;
                        cube.height2 = h;
                    }
                    kind => {
                        let b0 = r.u8()?;
                        cube.slope = Slope::from_bits(b0 >> 5);
                        cube.special = b0 & 0x1F;
                        cube.rotation = r.u8()?;
                        cube.scenery = r.u8()?;
                        cube.flags = r.u8()?;
                        cube.height1 = (r.u8()? as i8 as i16) << 4;
                        cube.height2 = (r.u8()? as i8 as i16) << 4;
                        let nfaces = if kind == 1 { 1 } else { 5 };
                        for face in cube.faces.iter_mut().take(nfaces) {
                            *face = Face::from_word(r.u16()?);
                        }
                    }
                }
            }
            squares.push(sq);
        }
        if r.pos != data.len() {
            return Err(Error::Parse {
                what: "map",
                detail: format!("{} trailing bytes", data.len() - r.pos),
            });
        }
        Ok(Map {
            version,
            night,
            variant,
            squares,
        })
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or(Error::Truncated("map"))?;
        self.pos += N;
        Ok(b.try_into().unwrap())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take()?))
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map where every square has a flat ground cube with a flipped,
    /// normal top face using texture 7, and two "off" cubes above it.
    fn flat_map() -> Vec<u8> {
        let mut d = vec![0xFF, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 2];
        for _ in 0..MAP_SIZE * MAP_SIZE {
            d.push(0b00_00_01);
            d.extend_from_slice(&[0x20 | 3, 64, 0, 0x20, 16, 16, 0x90, 7]);
            d.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        }
        d
    }

    #[test]
    fn parses_flat_map() {
        let m = Map::parse(&flat_map()).unwrap();
        assert!(m.night);
        assert_eq!(m.variant, 2);
        let c = &m.square(5, 9).cubes[0];
        assert_eq!(c.slope, Slope::Flat);
        assert_eq!((c.special, c.rotation, c.flags), (3, 64, 0x20));
        assert_eq!((c.height1, c.height2), (256, 256));
        assert_eq!(
            c.faces[TOP],
            Face {
                kind: FaceType::Normal,
                texture: 7,
                flipped: true
            }
        );
        assert!(!c.faces[NORTH].visible());
        let above = &m.square(5, 9).cubes[1];
        assert_eq!((above.slope, above.height1), (Slope::Off, 256));
        assert_eq!(m.square(5, 9).cubes[2].height1, 0);
    }

    #[test]
    fn rejects_trailing_data() {
        let mut d = flat_map();
        d.push(0);
        assert!(Map::parse(&d).is_err());
    }
}
