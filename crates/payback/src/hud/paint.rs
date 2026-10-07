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
    let mut image = Image::new(
        Extent3d {
            width: w as u32,
            height: CANVAS_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; w * CANVAS_H * 4],
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

pub struct Painter<'a> {
    pub px: &'a mut [u8],
    pub w: usize,
    pub pal: &'a [u8; 768],
}

impl Painter<'_> {
    pub fn put(&mut self, x: i32, y: i32, c: u8, blend: Blend) {
        if c == 0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= CANVAS_H {
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
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= CANVAS_H {
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

    /// Darkened panel (FadeDarken: subtract blend over the view).
    pub fn darken(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0.max(0)..=y1.min(CANVAS_H as i32 - 1) {
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
                    self.icon(g, cx, y, blend);
                }
            }
            cx += pitch;
            i += 1;
        }
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
