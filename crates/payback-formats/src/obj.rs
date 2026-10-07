//! `Objects/**/*.Obj` — 3D models (vehicles, people, scenery, weapons).
//!
//! Plain text, one integer or name per line (CRLF):
//!
//! ```text
//! 1001                 magic / version
//! <flag>               0 or 1 (meaning not yet known)
//! <nverts>
//! x y z w              per vertex; z is vertical (negative = up) for scenery.
//!                      w is the limb index for people (driven by Human.motion),
//!                      0 for rigid objects.
//! <nfaces>
//! a b c                per face: 1-based vertex indices
//! 2 x TextureSlot      name, u0 v0 u1 v1 u2 v2, mode, param, active
//! ```
//!
//! Texture names refer to files in `ObjectTextures/` (256x128, 8bpp). UVs are in
//! texel units. Exactly one of the two slots normally has `active == 1`; the
//! other is a placeholder (often `Engine`/`LoadingSmallLeft`).

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Limb index for articulated models, 0 otherwise.
    pub limb: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSlot {
    /// Texture file name in `ObjectTextures/`; may be empty.
    pub texture: String,
    pub uv: [(i32, i32); 3],
    /// Render mode; observed values 0-4, 17, 18, 20, 49, 50. Meaning TBD.
    pub mode: i32,
    /// Second render parameter; observed 0-4, 13, 18, 24, 25. Meaning TBD.
    pub param: i32,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// 0-based vertex indices.
    pub indices: [usize; 3],
    pub slots: [TextureSlot; 2],
}

impl Face {
    /// The slot flagged active, falling back to the first.
    pub fn active_slot(&self) -> &TextureSlot {
        self.slots
            .iter()
            .find(|s| s.active)
            .unwrap_or(&self.slots[0])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obj {
    pub flag: i32,
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
}

struct Lines<'a> {
    iter: std::str::Split<'a, char>,
    line: usize,
}

impl<'a> Lines<'a> {
    fn next_str(&mut self) -> Result<&'a str> {
        self.line += 1;
        self.iter
            .next()
            .map(|s| s.trim_end_matches('\r'))
            .ok_or(Error::Truncated("obj"))
    }

    fn next_int(&mut self) -> Result<i32> {
        let s = self.next_str()?;
        s.trim().parse().map_err(|_| Error::Parse {
            what: "obj",
            detail: format!("line {}: expected integer, got {s:?}", self.line),
        })
    }

    fn next_count(&mut self) -> Result<usize> {
        let n = self.next_int()?;
        usize::try_from(n).map_err(|_| Error::Parse {
            what: "obj",
            detail: format!("line {}: negative count {n}", self.line),
        })
    }
}

impl Obj {
    pub fn parse(data: &[u8]) -> Result<Obj> {
        // Latin-1: every byte maps to the same code point.
        let text: String = data.iter().map(|&b| b as char).collect();
        let mut l = Lines {
            iter: text.split('\n'),
            line: 0,
        };

        let magic = l.next_int()?;
        if magic != 1001 {
            return Err(Error::Parse {
                what: "obj",
                detail: format!("bad magic {magic}"),
            });
        }
        let flag = l.next_int()?;

        let nverts = l.next_count()?;
        let mut vertices = Vec::with_capacity(nverts);
        for _ in 0..nverts {
            vertices.push(Vertex {
                x: l.next_int()?,
                y: l.next_int()?,
                z: l.next_int()?,
                limb: l.next_int()?,
            });
        }

        let nfaces = l.next_count()?;
        let mut faces = Vec::with_capacity(nfaces);
        for _ in 0..nfaces {
            let mut indices = [0usize; 3];
            for i in &mut indices {
                let v = l.next_count()?;
                if v == 0 || v > nverts {
                    return Err(Error::Parse {
                        what: "obj",
                        detail: format!("line {}: vertex index {v} out of range", l.line),
                    });
                }
                *i = v - 1;
            }
            let slots = [read_slot(&mut l)?, read_slot(&mut l)?];
            faces.push(Face { indices, slots });
        }
        // Some files (e.g. Weapons/Petrol.Obj) carry trailing data the game ignores.
        Ok(Obj {
            flag,
            vertices,
            faces,
        })
    }
}

fn read_slot(l: &mut Lines<'_>) -> Result<TextureSlot> {
    let texture = l.next_str()?.to_string();
    let mut uv = [(0, 0); 3];
    for p in &mut uv {
        *p = (l.next_int()?, l.next_int()?);
    }
    Ok(TextureSlot {
        texture,
        uv,
        mode: l.next_int()?,
        param: l.next_int()?,
        active: l.next_int()? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic one-triangle model in the original layout.
    const TRIANGLE: &[&str] = &[
        "1001", "0", "3", "-10", "50", "0", "0", "10", "50", "0", "0", "0", "70", "-5", "0", "1",
        "1", "2", "3", "Unused", "2", "2", "253", "2", "2", "253", "2", "3", "0", "Tex", "1", "2",
        "3", "4", "5", "6", "4", "3", "1",
    ];

    fn crlf(tokens: &[&str]) -> Vec<u8> {
        tokens
            .iter()
            .flat_map(|t| format!("{t}\r\n").into_bytes())
            .collect()
    }

    #[test]
    fn parses_triangle() {
        let o = Obj::parse(&crlf(TRIANGLE)).unwrap();
        assert_eq!(o.vertices.len(), 3);
        assert_eq!(
            o.vertices[2],
            Vertex {
                x: 0,
                y: 70,
                z: -5,
                limb: 0
            }
        );
        assert_eq!(o.faces.len(), 1);
        assert_eq!(o.faces[0].indices, [0, 1, 2]);
        let s = o.faces[0].active_slot();
        assert_eq!(s.texture, "Tex");
        assert_eq!(s.uv, [(1, 2), (3, 4), (5, 6)]);
        assert_eq!((s.mode, s.param), (4, 3));
    }
}
