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
                if s[3] < 0.5 {
                    continue;
                }
                let i = (py as usize * self.w + px as usize) * 4;
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
