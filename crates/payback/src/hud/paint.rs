//! Drawing into the 256-line palette canvases shared by the HUD and the
//! front end (blits of `Icon`s and fixed-pitch bitmap-font text).

use super::icons::{Font, Icon};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// Canvas height: the original's 256-line PAL screen.
pub const CANVAS_H: usize = 256;

/// Canvas width for a window aspect ratio (the original lays everything out
/// against the view width).
pub fn canvas_width(aspect: f32) -> usize {
    ((CANVAS_H as f32 * aspect).round() as usize).clamp(256, 1024)
}

/// A transparent `w`×256 RGBA canvas image, sampled nearest-neighbour.
pub fn blank(w: usize) -> Image {
    blank_sized(w, CANVAS_H)
}

/// A transparent `w`×`h` RGBA canvas image, sampled nearest-neighbour.
pub fn blank_sized(w: usize, h: usize) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; w * h * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// A vertex of a textured triangle: screen position, q = 1/w and texel
/// coordinates (the original's `triangle` vertex: x, y, 1/w, u, v).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexVertex {
    pub x: f32,
    pub y: f32,
    pub q: f32,
    pub u: f32,
    pub v: f32,
}

/// Per-pixel x gradients of the attributes 1..4 of `a` (u·q, v·q, q) over
/// the triangle's plane; None for a degenerate triangle.
fn x_gradients(v: &[TexVertex; 3], a: &[[f32; 4]; 3]) -> Option<[f32; 3]> {
    let det = (v[1].x - v[0].x) * (v[2].y - v[0].y) - (v[2].x - v[0].x) * (v[1].y - v[0].y);
    if det == 0.0 {
        return None;
    }
    let mut g = [0.0; 3];
    for k in 0..3 {
        let (d1, d2) = (a[1][k + 1] - a[0][k + 1], a[2][k + 1] - a[0][k + 1]);
        g[k] = (d1 * (v[2].y - v[0].y) - d2 * (v[1].y - v[0].y)) / det;
    }
    Some(g)
}

/// A triangle edge walked one row at a time: (x, u·q, v·q, q) and its
/// per-row step, rows = ⌊y_end − y_start⌋.
struct Edge {
    cur: [f32; 4],
    d: [f32; 4],
    rows: i32,
}

impl Edge {
    fn new(v: &[TexVertex; 3], a: &[[f32; 4]; 3], from: usize, to: usize) -> Edge {
        let rows = (v[to].y - v[from].y) as i32;
        let mut d = [0.0; 4];
        if rows > 0 {
            for k in 0..4 {
                d[k] = (a[to][k] - a[from][k]) / rows as f32;
            }
        }
        Edge { cur: a[from], d, rows: rows.max(0) }
    }

    fn step(&mut self) {
        for k in 0..4 {
            self.cur[k] += self.d[k];
        }
        self.rows -= 1;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Plain,
    /// Alpha.256 table 0x30000.
    Lighten,
    /// Table 0x70000 (green-additive).
    Green,
}

/// An RGBA canvas `w` pixels wide; the height follows from `px` (256 lines
/// for the lo-res canvases, the window's for the hi-res HUD).
pub struct Painter<'a> {
    pub px: &'a mut [u8],
    pub w: usize,
    pub pal: &'a [u8; 768],
}

impl Painter<'_> {
    /// Canvas height in lines.
    pub fn h(&self) -> usize {
        self.px.len() / (self.w * 4).max(1)
    }

    pub fn put(&mut self, x: i32, y: i32, c: u8, blend: Blend) {
        if c == 0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h() {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let rgb = &self.pal[c as usize * 3..c as usize * 3 + 3];
        match blend {
            Blend::Plain => {
                self.px[i..i + 3].copy_from_slice(rgb);
                self.px[i + 3] = 255;
            }
            Blend::Lighten => {
                let m = *rgb.iter().max().unwrap();
                if m == 0 {
                    return;
                }
                for k in 0..3 {
                    self.px[i + k] = (rgb[k] as u32 * 255 / m as u32) as u8;
                }
                self.px[i + 3] = m;
            }
            Blend::Green => {
                self.px[i] = 0;
                self.px[i + 1] = 255;
                self.px[i + 2] = 0;
                self.px[i + 3] = rgb[1];
            }
        }
    }

    /// An RGB colour through the lighten table (as [`Blend::Lighten`]):
    /// `s + (1 − max s)·d` over what lies below.
    pub fn put_lighten_rgb(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        let m = rgb[0].max(rgb[1]).max(rgb[2]);
        if m == 0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h() {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        for k in 0..3 {
            self.px[i + k] = (rgb[k] as u32 * 255 / m as u32) as u8;
        }
        self.px[i + 3] = m;
    }

    /// Opaque RGB pixel (no palette, no transparency).
    pub fn put_rgb(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h() {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        self.px[i..i + 3].copy_from_slice(&rgb);
        self.px[i + 3] = 255;
    }

    pub fn icon(&mut self, icon: &Icon, x: i32, y: i32, blend: Blend) {
        for iy in 0..icon.h {
            for ix in 0..icon.w {
                self.put(
                    x + ix as i32,
                    y + iy as i32,
                    icon.px[iy * icon.w + ix],
                    blend,
                );
            }
        }
    }

    /// An icon at twice its size, bilinear-filtered: the hardware hi-res
    /// HUD, where the Mac draws each icon as a quad of 2w×2h with
    /// `GL_LINEAR` (`DrawIcon__FP4Iconii` @ 10085300, `W3D_SetFilter` @
    /// 10154650; Amiga Warp3D `FUN_0100a53c`). Colour 0 is transparent;
    /// texels are filtered as premultiplied colour of the blend and
    /// composited over the canvas.
    pub fn icon_bilinear2(&mut self, icon: &Icon, x: i32, y: i32, blend: Blend) {
        if icon.w == 0 || icon.h == 0 {
            return;
        }
        // Premultiplied RGBA of a texel as the blend shows it.
        let texel = |tx: i32, ty: i32| -> [f32; 4] {
            let tx = tx.clamp(0, icon.w as i32 - 1) as usize;
            let ty = ty.clamp(0, icon.h as i32 - 1) as usize;
            let c = icon.px[ty * icon.w + tx] as usize;
            if c == 0 {
                return [0.0; 4];
            }
            let rgb = [self.pal[c * 3] as f32, self.pal[c * 3 + 1] as f32, self.pal[c * 3 + 2] as f32];
            match blend {
                Blend::Plain => [rgb[0], rgb[1], rgb[2], 255.0],
                Blend::Lighten => [rgb[0], rgb[1], rgb[2], rgb[0].max(rgb[1]).max(rgb[2])],
                Blend::Green => [0.0, rgb[1], 0.0, rgb[1]],
            }
        };
        let (w, h) = (self.w as i32, self.h() as i32);
        for oy in 0..2 * icon.h as i32 {
            let py = y + oy;
            if py < 0 || py >= h {
                continue;
            }
            // Sample at (oy + 0.5) / 2 - 0.5 texels: between texel ty and
            // ty + 1, weight fy on the second (edges clamped).
            let (ty, fy) = ((oy - 1).div_euclid(2), if oy % 2 == 0 { 0.75 } else { 0.25 });
            for ox in 0..2 * icon.w as i32 {
                let px = x + ox;
                if px < 0 || px >= w {
                    continue;
                }
                let (tx, fx) = ((ox - 1).div_euclid(2), if ox % 2 == 0 { 0.75 } else { 0.25 });
                let (a, b, c, d) = (texel(tx, ty), texel(tx + 1, ty), texel(tx, ty + 1), texel(tx + 1, ty + 1));
                let mut s = [0.0f32; 4];
                for k in 0..4 {
                    let top = a[k] * (1.0 - fx) + b[k] * fx;
                    let bot = c[k] * (1.0 - fx) + d[k] * fx;
                    s[k] = top * (1.0 - fy) + bot * fy;
                }
                self.over(px, py, s);
            }
        }
    }

    /// A premultiplied RGBA sample (0..255) over the canvas pixel (x, y),
    /// which must be on the canvas; samples with alpha < 0.5 are dropped.
    fn over(&mut self, x: i32, y: i32, s: [f32; 4]) {
        if s[3] < 0.5 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        // Over the canvas (straight alpha).
        let da = self.px[i + 3] as f32;
        let sa = s[3] / 255.0;
        let out_a = s[3] + da * (1.0 - sa);
        for k in 0..3 {
            let dst = self.px[i + k] as f32 * da / 255.0;
            let v = s[k] + dst * (1.0 - sa);
            self.px[i + k] = (v * 255.0 / out_a.max(1.0)).round().clamp(0.0, 255.0) as u8;
        }
        self.px[i + 3] = out_a.round().clamp(0.0, 255.0) as u8;
    }

    /// A perspective-textured triangle of a 64×64 texture, colour 0
    /// transparent, nearest texel: the software renderer's
    /// `DrawPerspectiveTriangleGenlock__FP8triangle` @ 100b8c50 (OS4
    /// `FUN_0107fc64`). Edges are walked from the topmost vertex one row
    /// at a time from row ⌊y⌋ (x truncated, no sub-pixel correction), the
    /// edge that runs out first replaced by the third; a row covers
    /// [⌊x_left⌋, ⌊x_right⌋), clipped to the canvas. u·q, v·q and q are
    /// stepped linearly and divided every 16 pixels (spans of 16, the last
    /// one shorter), with u, v in 1/64 texel steps in between; the texel is
    /// `tex[(v & 0xFC0) + (u >> 6)]`.
    pub fn textured_triangle(&mut self, tex: &Icon, v: [TexVertex; 3]) {
        if tex.w != 64 || tex.h != 64 {
            return;
        }
        let (w, h) = (self.w as i32, self.h() as i32);
        if v.iter().all(|p| p.x < 0.0)
            || v.iter().all(|p| p.y < 0.0)
            || v.iter().all(|p| p.x >= w as f32)
            || v.iter().all(|p| p.y >= h as f32)
        {
            return;
        }
        // (x, 64·u·q, 64·v·q, q) per vertex.
        let a = v.map(|p| [p.x, 64.0 * p.u * p.q, 64.0 * p.v * p.q, p.q]);
        let Some(grad) = x_gradients(&v, &a) else { return };
        let mut top = if v[1].y < v[0].y { 1 } else { 0 };
        if v[2].y < v[top].y {
            top = 2;
        }
        let (next, prev) = ((top + 1) % 3, (top + 2) % 3);
        let mut ea = Edge::new(&v, &a, top, next);
        let mut eb = Edge::new(&v, &a, top, prev);
        let mut y = v[top].y as i32;
        for half in 0..2 {
            let n = ea.rows.min(eb.rows);
            for _ in 0..n {
                if y >= h {
                    return;
                }
                if y >= 0 {
                    self.span_row(tex, y, &ea.cur, &eb.cur, &grad);
                }
                ea.step();
                eb.step();
                y += 1;
            }
            if half == 1 {
                break;
            }
            if ea.rows == 0 {
                ea = Edge::new(&v, &a, next, prev);
            } else {
                eb = Edge::new(&v, &a, prev, next);
            }
        }
    }

    /// One row of [`Painter::textured_triangle`] between the edge points
    /// `ea` and `eb`.
    fn span_row(&mut self, tex: &Icon, y: i32, ea: &[f32; 4], eb: &[f32; 4], grad: &[f32; 3]) {
        let (xa, xb) = (ea[0] as i32, eb[0] as i32);
        let (l, mut x, x1) = match xa.cmp(&xb) {
            std::cmp::Ordering::Less => (ea, xa, xb),
            std::cmp::Ordering::Greater => (eb, xb, xa),
            std::cmp::Ordering::Equal => return,
        };
        let mut s = [l[1], l[2], l[3]];
        if x < 0 {
            for k in 0..3 {
                s[k] += grad[k] * (-x) as f32;
            }
            x = 0;
        }
        let mut n = x1.min(self.w as i32) - x;
        let (mut u, mut v) = ((s[0] / s[2]) as i32, (s[1] / s[2]) as i32);
        while n > 0 {
            let span = n.min(16);
            for k in 0..3 {
                s[k] += grad[k] * span as f32;
            }
            let (u1, v1) = ((s[0] / s[2]) as i32, (s[1] / s[2]) as i32);
            let (du, dv) = if span < 16 {
                ((u1 - u) / span, (v1 - v) / span)
            } else {
                ((u1 - u) >> 4, (v1 - v) >> 4)
            };
            for _ in 0..span {
                let c = tex.px[(v as u32 & 0xFC0) as usize + (u >> 6).clamp(0, 63) as usize];
                self.put(x, y, c, Blend::Plain);
                u += du;
                v += dv;
                x += 1;
            }
            n -= span;
        }
    }

    /// A perspective-textured triangle, bilinear-filtered: the hardware
    /// renderer's `W3D_DrawTriangle` (OS4 `FUN_0100499c`; the Mac's GL
    /// path filters `GL_LINEAR`, see [`Painter::icon_bilinear2`]) with
    /// blend `(7, 8)` (source alpha, one minus source alpha). Pixel centres
    /// inside the triangle (top-left rule) are drawn; u, v are in texels
    /// (texel centres at i + 1/2, edges clamped), colour 0 is transparent
    /// and texels are filtered premultiplied.
    pub fn textured_triangle_bilinear(&mut self, tex: &Icon, v: [TexVertex; 3]) {
        if tex.w == 0 || tex.h == 0 {
            return;
        }
        let (w, h) = (self.w as i32, self.h() as i32);
        let det = (v[1].x - v[0].x) * (v[2].y - v[0].y) - (v[2].x - v[0].x) * (v[1].y - v[0].y);
        if det == 0.0 {
            return;
        }
        let x0 = v.iter().map(|p| p.x).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let x1 = v.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil().min(w as f32) as i32;
        let y0 = v.iter().map(|p| p.y).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let y1 = v.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil().min(h as f32) as i32;
        // Edge k runs from vertex k+1 to k+2 (opposite vertex k), signed so
        // that the inside is positive.
        let sign = det.signum();
        let edge = |k: usize, px: f32, py: f32| {
            let (a, b) = (v[(k + 1) % 3], v[(k + 2) % 3]);
            sign * ((b.x - a.x) * (py - a.y) - (b.y - a.y) * (px - a.x))
        };
        // Pixels exactly on an edge belong to one of the two triangles
        // sharing it: the "top-left" rule.
        let owns = |k: usize| {
            let (a, b) = (v[(k + 1) % 3], v[(k + 2) % 3]);
            let (dx, dy) = (sign * (b.x - a.x), sign * (b.y - a.y));
            dy < 0.0 || (dy == 0.0 && dx > 0.0)
        };
        let pal = self.pal;
        let texel = |tx: i32, ty: i32| -> [f32; 4] {
            let tx = tx.clamp(0, tex.w as i32 - 1) as usize;
            let ty = ty.clamp(0, tex.h as i32 - 1) as usize;
            let c = tex.px[ty * tex.w + tx] as usize;
            if c == 0 {
                return [0.0; 4];
            }
            [pal[c * 3] as f32, pal[c * 3 + 1] as f32, pal[c * 3 + 2] as f32, 255.0]
        };
        for py in y0..y1 {
            for px in x0..x1 {
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                let e = [edge(0, cx, cy), edge(1, cx, cy), edge(2, cx, cy)];
                if (0..3).any(|k| e[k] < 0.0 || (e[k] == 0.0 && !owns(k))) {
                    continue;
                }
                let sum = e[0] + e[1] + e[2];
                let b = e.map(|e| e / sum);
                let q: f32 = (0..3).map(|k| b[k] * v[k].q).sum();
                let u = (0..3).map(|k| b[k] * v[k].u * v[k].q).sum::<f32>() / q - 0.5;
                let t = (0..3).map(|k| b[k] * v[k].v * v[k].q).sum::<f32>() / q - 0.5;
                let (tx, ty) = (u.floor() as i32, t.floor() as i32);
                let (fx, fy) = (u - u.floor(), t - t.floor());
                let (a, bb, c, d) = (texel(tx, ty), texel(tx + 1, ty), texel(tx, ty + 1), texel(tx + 1, ty + 1));
                let mut s = [0.0f32; 4];
                for k in 0..4 {
                    let top = a[k] * (1.0 - fx) + bb[k] * fx;
                    let bot = c[k] * (1.0 - fx) + d[k] * fx;
                    s[k] = top * (1.0 - fy) + bot * fy;
                }
                self.over(px, py, s);
            }
        }
    }

    /// Darkened panel (FadeDarken: subtract blend over the view).
    pub fn darken(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0.max(0)..=y1.min(self.h() as i32 - 1) {
            for x in x0.max(0)..=x1.min(self.w as i32 - 1) {
                let i = (y as usize * self.w + x as usize) * 4;
                self.px[i..i + 4].copy_from_slice(&[0, 0, 24, 170]);
            }
        }
    }

    /// Fixed-pitch text (7 px per character for the small fonts). A `'^'`
    /// prefix selects the Latin-2 glyph table in the original; without it
    /// the following character is drawn from the normal table.
    pub fn text(&mut self, font: &Font, s: &str, x: i32, y: i32, pitch: i32, blend: Blend) {
        for_each_glyph(font, s, x, pitch, |g, cx| self.icon(g, cx, y, blend));
    }
}

/// The glyphs of `s` and their x positions at a fixed `pitch` (spaces and
/// `'^'` prefixes take no glyph; see [`Painter::text`]).
pub fn for_each_glyph(font: &Font, s: &str, x: i32, pitch: i32, mut f: impl FnMut(&Icon, i32)) {
    let mut cx = x;
    let bytes = latin1(s);
    let mut i = 0;
    while i < bytes.len() {
        let mut b = bytes[i];
        if b == b'^' && i + 1 < bytes.len() {
            i += 1;
            b = bytes[i];
        }
        if b != b' ' {
            if let Some(g) = font.glyph(b) {
                f(g, cx);
            }
        }
        cx += pitch;
        i += 1;
    }
}

/// Characters as Latin-1 codes (others become `'?'`).
pub fn latin1(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect()
}

/// Number of character cells a string occupies (`'^'` prefixes don't
/// count), as measured by the original's centring loops.
pub fn text_len(s: &str) -> i32 {
    let mut n = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '^' && chars.next().is_none() {
            break;
        }
        n += 1;
    }
    n
}
