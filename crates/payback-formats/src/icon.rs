//! Amiga Workbench icons (`*.info`, `DiskObject`): the classic planar
//! images and the OS 3.5 colour images (`FORM ICON`, "GlowIcons").
//!
//! `Payback.info` (Update 7) carries a 62x27 classic image and a 66x30
//! colour image of the title art; Workbench shows the colour one when it
//! can. Both have a normal and a selected state.

use crate::{Error, Result};

/// An RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The Workbench 2.x default four colours used by classic icons.
const WB2: [[u8; 3]; 4] = [[0x95, 0x95, 0x95], [0, 0, 0], [0xff, 0xff, 0xff], [0x3b, 0x67, 0xa2]];

fn u16_at(d: &[u8], p: usize) -> Result<u16> {
    d.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or(Error::Truncated("icon"))
}

fn u32_at(d: &[u8], p: usize) -> Result<u32> {
    d.get(p..p + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated("icon"))
}

/// The classic images (normal, selected if present), drawn in the
/// Workbench 2.x palette (colour 0 opaque grey, as Workbench shows it).
pub fn classic_images(d: &[u8]) -> Result<Vec<Rgba>> {
    if u16_at(d, 0)? != 0xe310 {
        return Err(Error::Parse { what: "icon", detail: "not a DiskObject".into() });
    }
    let renders = [u32_at(d, 22)?, u32_at(d, 26)?];
    // DiskObject is 78 bytes; a DrawerData (56 bytes) follows if present.
    let mut p = 78 + if u32_at(d, 66)? != 0 { 56 } else { 0 };
    let mut out = Vec::new();
    for render in renders {
        if render == 0 {
            continue;
        }
        let w = u16_at(d, p + 4)? as usize;
        let h = u16_at(d, p + 6)? as usize;
        let depth = u16_at(d, p + 8)? as usize;
        p += 20;
        let row = w.div_ceil(16) * 2;
        let plane = row * h;
        let data = d.get(p..p + plane * depth).ok_or(Error::Truncated("icon planes"))?;
        p += plane * depth;
        let mut pixels = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let c = (0..depth).fold(0, |c, i| c | ((data[i * plane + y * row + x / 8] >> (7 - x % 8) & 1) as usize) << i);
                pixels.extend_from_slice(&WB2[c % 4]);
                pixels.push(255);
            }
        }
        out.push(Rgba { width: w as u32, height: h as u32, pixels });
    }
    Ok(out)
}

/// Bit reader for the OS 3.5 run-length streams.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for _ in 0..n {
            let byte = *self.d.get(self.pos / 8)?;
            v = v << 1 | (byte >> (7 - self.pos % 8) & 1) as u32;
            self.pos += 1;
        }
        Some(v)
    }
}

/// OS 3.5 RLE: a control byte `c < 128` copies `c + 1` values, `c > 128`
/// repeats one value `257 - c` times (128 is a no-op); values are `bits`
/// wide.
fn unpack(d: &[u8], bits: u32, count: usize) -> Vec<u32> {
    let mut r = Bits { d, pos: 0 };
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let Some(c) = r.read(8) else { break };
        if c < 128 {
            for _ in 0..=c {
                match r.read(bits) {
                    Some(v) => out.push(v),
                    None => break,
                }
            }
        } else if c > 128 {
            let Some(v) = r.read(bits) else { break };
            out.extend(std::iter::repeat_n(v, 257 - c as usize));
        }
    }
    out.truncate(count);
    out
}

/// The OS 3.5 colour images (normal, selected if present), with the
/// transparent colour as alpha 0. Empty if the icon has none.
pub fn colour_images(d: &[u8]) -> Result<Vec<Rgba>> {
    let Some(start) = d.windows(4).position(|w| w == b"FORM") else {
        return Ok(Vec::new());
    };
    if d.get(start + 8..start + 12) != Some(b"ICON") {
        return Ok(Vec::new());
    }
    let end = (start + 8 + u32_at(d, start + 4)? as usize).min(d.len());
    let mut p = start + 12;
    let mut size = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut out = Vec::new();
    while p + 8 <= end {
        let id = &d[p..p + 4];
        let n = u32_at(d, p + 4)? as usize;
        let body = d.get(p + 8..p + 8 + n).ok_or(Error::Truncated("icon chunk"))?;
        match id {
            b"FACE" => size = Some((body[0] as usize + 1, body[1] as usize + 1)),
            b"IMAG" => {
                let (w, h) = size.ok_or(Error::Parse { what: "icon", detail: "IMAG before FACE".into() })?;
                let (trans, ncol, flags, icomp, pcomp, depth) = (body[0], body[1] as usize + 1, body[2], body[3], body[4], body[5]);
                let isize = u16_at(body, 6)? as usize + 1;
                let psize = u16_at(body, 8)? as usize + 1;
                let idata = body.get(10..10 + isize).ok_or(Error::Truncated("icon image"))?;
                let pixels: Vec<u32> = if icomp != 0 {
                    unpack(idata, depth as u32, w * h)
                } else {
                    idata.iter().map(|&b| b as u32).collect()
                };
                if flags & 2 != 0 {
                    let pdata = body.get(10 + isize..10 + isize + psize).ok_or(Error::Truncated("icon palette"))?;
                    let raw: Vec<u32> =
                        if pcomp != 0 { unpack(pdata, 8, ncol * 3) } else { pdata.iter().map(|&b| b as u32).collect() };
                    palette = raw.chunks(3).map(|c| [c[0] as u8, *c.get(1).unwrap_or(&0) as u8, *c.get(2).unwrap_or(&0) as u8]).collect();
                }
                let mut rgba = Vec::with_capacity(w * h * 4);
                for k in 0..w * h {
                    let c = pixels.get(k).copied().unwrap_or(0) as usize;
                    rgba.extend_from_slice(palette.get(c).unwrap_or(&[0, 0, 0]));
                    rgba.push(if flags & 1 != 0 && c == trans as usize { 0 } else { 255 });
                }
                out.push(Rgba { width: w as u32, height: h as u32, pixels: rgba });
            }
            _ => {}
        }
        p += 8 + n + (n & 1);
    }
    Ok(out)
}

/// The image Workbench shows normally: the colour one if present,
/// otherwise the classic one.
pub fn workbench_image(d: &[u8]) -> Result<Rgba> {
    colour_images(d)?
        .into_iter()
        .next()
        .or(classic_images(d)?.into_iter().next())
        .ok_or(Error::Parse { what: "icon", detail: "no image".into() })
}

impl Rgba {
    /// Centred on a transparent square canvas of `size`: scaled up by a
    /// whole factor (nearest neighbour) so the pixel art stays sharp, or,
    /// when it is larger than the canvas, scaled down by averaging the
    /// source pixels each output pixel covers (alpha-weighted).
    pub fn square(&self, size: u32) -> Rgba {
        let side = self.width.max(self.height);
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        // Output pixels per source pixel.
        let scale = if side <= size { (size / side) as f32 } else { size as f32 / side as f32 };
        let (w, h) = ((self.width as f32 * scale).round() as u32, (self.height as f32 * scale).round() as u32);
        let (ox, oy) = ((size - w.min(size)) / 2, (size - h.min(size)) / 2);
        for y in 0..h.min(size) {
            for x in 0..w.min(size) {
                // Source rectangle covered by this output pixel.
                let (sx0, sy0) = ((x as f32 / scale) as u32, (y as f32 / scale) as u32);
                let sx1 = (((x + 1) as f32 / scale).ceil() as u32).clamp(sx0 + 1, self.width);
                let sy1 = (((y + 1) as f32 / scale).ceil() as u32).clamp(sy0 + 1, self.height);
                let (mut acc, mut alpha, mut n) = ([0u32; 3], 0u32, 0u32);
                for sy in sy0..sy1 {
                    for sx in sx0..sx1 {
                        let s = ((sy * self.width + sx) * 4) as usize;
                        let a = self.pixels[s + 3] as u32;
                        for c in 0..3 {
                            acc[c] += self.pixels[s + c] as u32 * a;
                        }
                        alpha += a;
                        n += 1;
                    }
                }
                let t = (((oy + y) * size + ox + x) * 4) as usize;
                if alpha > 0 {
                    for c in 0..3 {
                        pixels[t + c] = (acc[c] / alpha) as u8;
                    }
                }
                pixels[t + 3] = (alpha / n) as u8;
            }
        }
        Rgba { width: size, height: size, pixels }
    }
}

/// A resource from a classic Mac resource fork (the `.rsrc` files written
/// by `tools/extract/extract_mac.py`): the data of `kind`/`id`, if present.
pub fn resource<'a>(fork: &'a [u8], kind: &[u8; 4], id: i16) -> Option<&'a [u8]> {
    let be32 = |p: usize| fork.get(p..p + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let be16 = |p: usize| fork.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let (data_off, map_off) = (be32(0)?, be32(4)?);
    let types = map_off + be16(map_off + 24)?;
    let ntypes = be16(types)?.wrapping_add(1) & 0xffff;
    for t in 0..ntypes {
        let e = types + 2 + t * 8;
        if fork.get(e..e + 4)? != kind {
            continue;
        }
        let count = be16(e + 4)? + 1;
        let refs = types + be16(e + 6)?;
        for r in 0..count {
            let re = refs + r * 12;
            if be16(re)? as u16 as i16 != id {
                continue;
            }
            let off = data_off + (be32(re + 4)? & 0x00ff_ffff);
            let len = be32(off)?;
            return fork.get(off + 4..off + 4 + len);
        }
    }
    None
}

/// The images of a Mac OS X `icns` icon family with 32-bit colour and an
/// 8-bit mask (`it32`/`t8mk` 128, `ih32`/`h8mk` 48, `il32`/`l8mk` 32,
/// `is32`/`s8mk` 16), largest first.
pub fn icns_images(d: &[u8]) -> Result<Vec<Rgba>> {
    if d.get(0..4) != Some(b"icns") {
        return Err(Error::Parse { what: "icns", detail: "not an icns".into() });
    }
    let mut chunks = std::collections::HashMap::new();
    let mut p = 8;
    while p + 8 <= d.len() {
        let len = u32_at(d, p + 4)? as usize;
        if len < 8 {
            break;
        }
        chunks.insert(&d[p..p + 4], d.get(p + 8..p + len).ok_or(Error::Truncated("icns"))?);
        p += len;
    }
    let mut out = Vec::new();
    for (img, mask, size, skip) in
        [(b"it32", b"t8mk", 128usize, 4usize), (b"ih32", b"h8mk", 48, 0), (b"il32", b"l8mk", 32, 0), (b"is32", b"s8mk", 16, 0)]
    {
        let (Some(img), Some(mask)) = (chunks.get(&img[..]), chunks.get(&mask[..])) else { continue };
        let n = size * size;
        // Three channel planes, each PackBits-like: c < 0x80 copies c + 1
        // bytes, otherwise repeats the next byte c - 0x80 + 3 times.
        let mut planes = Vec::with_capacity(3 * n);
        let mut q = skip;
        while planes.len() < 3 * n && q < img.len() {
            let c = img[q] as usize;
            q += 1;
            if c < 0x80 {
                planes.extend_from_slice(img.get(q..q + c + 1).ok_or(Error::Truncated("icns rle"))?);
                q += c + 1;
            } else {
                let v = *img.get(q).ok_or(Error::Truncated("icns rle"))?;
                planes.extend(std::iter::repeat_n(v, c - 0x80 + 3));
                q += 1;
            }
        }
        if planes.len() < 3 * n || mask.len() < n {
            return Err(Error::Truncated("icns image"));
        }
        let mut pixels = Vec::with_capacity(n * 4);
        for k in 0..n {
            pixels.extend_from_slice(&[planes[k], planes[n + k], planes[2 * n + k], mask[k]]);
        }
        out.push(Rgba { width: size as u32, height: size as u32, pixels });
    }
    Ok(out)
}

/// The Mac release's application icon (`icns` 128 in the resource fork of
/// `Payback (OS X)`), largest image first.
pub fn mac_app_icon(fork: &[u8]) -> Result<Vec<Rgba>> {
    let icns = resource(fork, b"icns", 128).ok_or(Error::Parse { what: "icns", detail: "no icns 128".into() })?;
    icns_images(icns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_runs() {
        // Copy 2 values (0b101, 0b011), then repeat 0b111 three times, 3-bit values.
        // 00000001 101 011 | 11111110 111 (pad)
        let bits = "00000001101011111111101110000000";
        let bytes: Vec<u8> = bits.as_bytes().chunks(8).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()).collect();
        assert_eq!(unpack(&bytes, 3, 5), vec![5, 3, 7, 7, 7]);
    }

    #[test]
    fn square_keeps_aspect() {
        let img = Rgba { width: 2, height: 1, pixels: vec![255; 8] };
        let s = img.square(8);
        assert_eq!((s.width, s.height), (8, 8));
        // Scaled 4x into the middle rows 2..6; corners transparent.
        assert_eq!(s.pixels[3], 0);
        assert_eq!(s.pixels[((2 * 8) * 4 + 3) as usize], 255);
        // Larger than the canvas: shrunk to fit (not cropped).
        let wide = Rgba { width: 8, height: 2, pixels: vec![255; 64] };
        let small = wide.square(4);
        let opaque = small.pixels.chunks(4).filter(|p| p[3] == 255).count();
        assert_eq!(opaque, 4, "8x2 -> 4x1 row");
    }
}
