//! Autodesk Animator FLI/FLC animation (`Videos/intro.video`).
//!
//! Little-endian. 128-byte header, then frames (`u32 size, u16 0xF1FA,
//! u16 chunks, 8 reserved`) of chunks (`u32 size, u16 type`, next chunk at
//! `(size+1) & !1`). The file holds `frames` shown frames plus one ring
//! frame that loops back to frame 0. Player: 68k `FUN_000106c0`, delta
//! decoder `FUN_0001021c` (docs/re/frontend_audio.md §5.1).

use crate::{Error, Result};

const FRAME: u16 = 0xF1FA;
const PREFIX: u16 = 0xF100;

fn u16_at(d: &[u8], p: usize) -> Result<u16> {
    d.get(p..p + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Truncated("flc"))
}

fn u32_at(d: &[u8], p: usize) -> Result<u32> {
    d.get(p..p + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated("flc"))
}

fn byte(d: &[u8], p: usize) -> Result<u8> {
    d.get(p).copied().ok_or(Error::Truncated("flc"))
}

fn bad(detail: String) -> Error {
    Error::Parse { what: "flc", detail }
}

/// A streaming FLC decoder holding the current frame and palette.
pub struct Flc {
    data: Vec<u8>,
    pub frames: u16,
    pub width: usize,
    pub height: usize,
    /// Milliseconds per frame (FLC; FLI files use 1/70 s units).
    pub speed: u32,
    /// Current 8-bit frame, `width * height`.
    pub pixels: Vec<u8>,
    /// Current palette, 256 × RGB.
    pub palette: [u8; 768],
    offset: usize,
    shown: u16,
}

impl Flc {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let magic = u16_at(&data, 4)?;
        if magic != 0xAF12 && magic != 0xAF11 {
            return Err(bad(format!("bad magic {magic:#06x}")));
        }
        let frames = u16_at(&data, 6)?;
        let width = u16_at(&data, 8)? as usize;
        let height = u16_at(&data, 10)? as usize;
        let speed = u32_at(&data, 0x10)?;
        Ok(Self {
            data,
            frames,
            width,
            height,
            speed,
            pixels: vec![0; width * height],
            palette: [0; 768],
            offset: 0x80,
            shown: 0,
        })
    }

    /// Index of the next frame to decode.
    pub fn position(&self) -> u16 {
        self.shown
    }

    /// Decode the next frame into `pixels`/`palette`. Returns `Ok(false)`
    /// after the last shown frame. With `ring`, the loop frame after the
    /// last one is decoded too (it reproduces frame 0).
    pub fn next_frame(&mut self, ring: bool) -> Result<bool> {
        let limit = self.frames + ring as u16;
        while self.shown < limit && self.offset + 16 <= self.data.len() {
            let start = self.offset;
            let size = u32_at(&self.data, start)? as usize;
            let kind = u16_at(&self.data, start + 4)?;
            if size < 16 {
                return Err(bad(format!("frame size {size} at {start:#x}")));
            }
            self.offset = start + size;
            if kind == PREFIX {
                continue;
            }
            if kind != FRAME {
                return Err(bad(format!("frame type {kind:#06x} at {start:#x}")));
            }
            let chunks = u16_at(&self.data, start + 6)?;
            let mut p = start + 16;
            for _ in 0..chunks {
                let csize = u32_at(&self.data, p)? as usize;
                let ctype = u16_at(&self.data, p + 4)?;
                let end = (p + csize).min(self.data.len());
                let body = self.data.get(p + 6..end).ok_or(Error::Truncated("flc chunk"))?;
                match ctype {
                    4 => colour(body, &mut self.palette, 0)?,
                    11 => colour(body, &mut self.palette, 2)?,
                    7 => delta_flc(body, &mut self.pixels, self.width)?,
                    13 => self.pixels.fill(0),
                    15 => byte_run(body, &mut self.pixels, self.width, self.height)?,
                    16 => {
                        let n = self.pixels.len();
                        let src = body.get(..n).ok_or(Error::Truncated("flc copy"))?;
                        self.pixels.copy_from_slice(src);
                    }
                    18 => {}
                    t => return Err(bad(format!("unrecognised chunk type {t:04x}"))),
                }
                p += (csize + 1) & !1;
            }
            self.shown += 1;
            return Ok(true);
        }
        Ok(false)
    }

    /// RGBA8 copy of the current frame.
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &i in &self.pixels {
            let c = &self.palette[i as usize * 3..i as usize * 3 + 3];
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        out
    }
}

/// COLOR_256 / COLOR_64: packets of (skip, count (0 = 256), count × RGB).
fn colour(d: &[u8], pal: &mut [u8; 768], shift: u32) -> Result<()> {
    let packets = u16_at(d, 0)?;
    let (mut p, mut idx) = (2, 0usize);
    for _ in 0..packets {
        idx += byte(d, p)? as usize;
        let count = match byte(d, p + 1)? {
            0 => 256,
            n => n as usize,
        };
        p += 2;
        for _ in 0..count {
            for k in 0..3 {
                pal[(idx & 0xff) * 3 + k] = byte(d, p + k)? << shift;
            }
            p += 3;
            idx += 1;
        }
    }
    Ok(())
}

/// BYTE_RUN: per row an ignored packet count, then runs until the row is
/// full: `c < 0x80` repeats the next byte c times, else `256 - c` literals.
fn byte_run(d: &[u8], fb: &mut [u8], w: usize, h: usize) -> Result<()> {
    let mut p = 0;
    for y in 0..h {
        p += 1;
        let row = &mut fb[y * w..(y + 1) * w];
        let mut x = 0;
        while x < w {
            let c = byte(d, p)? as usize;
            p += 1;
            if c < 0x80 {
                let v = byte(d, p)?;
                p += 1;
                let n = c.min(w - x);
                row[x..x + n].fill(v);
                x += c;
            } else {
                let n = 256 - c;
                let src = d.get(p..p + n).ok_or(Error::Truncated("flc brun"))?;
                let m = n.min(w - x);
                row[x..x + m].copy_from_slice(&src[..m]);
                p += n;
                x += n;
            }
        }
    }
    Ok(())
}

/// DELTA_FLC (word delta). Per line, opcodes `0xC000..` skip `-op` lines,
/// `0x8000..0xBFFF` set the line's last pixel; anything else is the packet
/// count. Packet: column skip, signed count (`>0` literal words, `<0` one
/// word repeated).
fn delta_flc(d: &[u8], fb: &mut [u8], w: usize) -> Result<()> {
    let lines = u16_at(d, 0)?;
    let (mut p, mut y) = (2, 0usize);
    let len = fb.len();
    for _ in 0..lines {
        let packets = loop {
            let op = u16_at(d, p)?;
            p += 2;
            if op & 0x8000 == 0 {
                break op;
            }
            if op & 0x4000 != 0 {
                y += 0x10000 - op as usize;
            } else if let Some(px) = fb.get_mut(y * w + w - 1) {
                *px = op as u8;
            }
        };
        let mut x = 0;
        for _ in 0..packets {
            x += byte(d, p)? as usize;
            let c = byte(d, p + 1)? as i8;
            p += 2;
            let o = y * w + x;
            if c >= 0 {
                let n = 2 * c as usize;
                let src = d.get(p..p + n).ok_or(Error::Truncated("flc delta"))?;
                let end = (o + n).min(len);
                fb[o.min(len)..end].copy_from_slice(&src[..end - o.min(len)]);
                p += n;
                x += n;
            } else {
                let n = -(c as i32) as usize;
                let word = [byte(d, p)?, byte(d, p + 1)?];
                p += 2;
                for k in 0..2 * n {
                    if let Some(px) = fb.get_mut(o + k) {
                        *px = word[k & 1];
                    }
                }
                x += 2 * n;
            }
        }
        y += 1;
    }
    Ok(())
}
