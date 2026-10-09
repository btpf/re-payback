//! `Icons/*` images and fonts (docs/re/frontend_audio.md §1).
//!
//! Headerless 8-bit chunky, rows padded to a 16-pixel stride; colour 0 is
//! transparent; dimensions are fixed per file name.

use crate::data::GameData;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Icon {
    pub w: usize,
    pub h: usize,
    /// Palette indices, `w*h`, 0 = transparent.
    pub px: Vec<u8>,
}

impl Icon {
    pub fn load(data: &GameData, name: &str, w: usize, h: usize) -> Icon {
        let stride = w.div_ceil(16) * 16;
        let bytes = find(data, name).and_then(|p| crate::vfs::read(p).ok());
        let mut px = vec![0u8; w * h];
        match bytes {
            Some(b) => {
                for y in 0..h {
                    for x in 0..w {
                        px[y * w + x] = b.get(y * stride + x).copied().unwrap_or(0);
                    }
                }
            }
            None => warn!("LoadIcon: Unable to read {name}"),
        }
        Icon { w, h, px }
    }
}

impl Icon {
    /// The software hi-res copy (`LoadIcon__FPciii` @ 10086f80, Amiga
    /// `FUN_0100d3c4`): each pixel P becomes a 2×2 block of P, avg(P,
    /// right), avg(P, below) and avg(P, below right), edges clamped
    /// (`GetPointCol`); `avg` = Alpha.256 table 4 unless both are equal
    /// (`AverageVals` @ 10086ef0). Colour 0 is averaged like any other, so
    /// shapes get a dark edge.
    pub fn doubled(&self, avg: &Average) -> Icon {
        let (w, h) = (self.w, self.h);
        let at = |x: usize, y: usize| self.px[y.min(h - 1) * w + x.min(w - 1)];
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let p = at(x, y);
                let o = 2 * y * 2 * w + 2 * x;
                px[o] = p;
                px[o + 1] = avg.of(p, at(x + 1, y));
                px[o + 2 * w] = avg.of(p, at(x, y + 1));
                px[o + 2 * w + 1] = avg.of(p, at(x + 1, y + 1));
            }
        }
        Icon { w: 2 * w, h: 2 * h, px }
    }
}

/// Alpha.256 table 4, the 50 % average (`T4[a·256 + b]`).
pub struct Average(Vec<u8>);

impl Average {
    /// Computed like `CalcAlpha` @ 10083ec0 (`postfx::alpha_tables`).
    pub fn load(data: &GameData) -> Average {
        let t = crate::camera::postfx::alpha_tables(data);
        Average(t[0x40000..0x50000].to_vec())
    }

    /// `AverageVals` @ 10086ef0.
    pub fn of(&self, a: u8, b: u8) -> u8 {
        if a == b { a } else { self.0[a as usize * 256 + b as usize] }
    }
}

fn find(data: &GameData, name: &str) -> Option<std::path::PathBuf> {
    let dir = data.root.join("Icons");
    let exact = dir.join(name);
    if crate::vfs::is_file(&exact) {
        return Some(exact);
    }
    crate::vfs::read_dir(&dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

/// A bitmap font: glyphs by Latin-1 code.
#[derive(Default)]
pub struct Font {
    pub glyphs: HashMap<u8, Icon>,
    pub w: usize,
    pub h: usize,
}

/// Named glyph files. `Dollar`, `Percent` and `Pound` exist in `Icons/`
/// but no build loads them (OS4 `LoadFont` in `FUN_010138dc`, 68k, Mac
/// `LoadFont` @ 10089380 / `LoadFontPartial` @ 10087d80), so '$', '%' and
/// '£' fall back to '?' as in the originals (`analysis/mac/frontend.md`
/// §5.3).
const NAMED: [(&str, u8); 23] = [
    ("FullStop", b'.'),
    ("Comma", b','),
    ("Colon", b':'),
    ("SemiColon", b';'),
    ("Hyphen", b'-'),
    ("Quotes", b'"'),
    ("Apostrophe", b'\''),
    ("OpenBracket", b'('),
    ("CloseBracket", b')'),
    ("ExclamationMark", b'!'),
    ("QuestionMark", b'?'),
    ("GreaterThan", b'>'),
    ("LessThan", b'<'),
    ("Slash", b'/'),
    ("Underscore", b'_'),
    ("Plus", b'+'),
    ("ae", 0xC6),
    ("o_line", 0xD8),
    ("a_circle", 0xC5),
    ("a_dots", 0xC4),
    ("o_dots", 0xD6),
    ("n_tilde", 0xD1),
    ("91", b'['),
];

impl Font {
    /// `prefix` is "Big", "You" or "Them". Big has capitals only (mapped to
    /// both cases); You/Them load lower case from `little_<c>` files.
    pub fn load(data: &GameData, prefix: &str, w: usize, h: usize) -> Font {
        let mut f = Font {
            glyphs: HashMap::new(),
            w,
            h,
        };
        let big = prefix == "Big";
        let mut add = |file: String, codes: &[u8]| {
            if find(data, &file).is_some() {
                let icon = Icon::load(data, &file, w, h);
                for &c in codes {
                    f.glyphs.insert(c, icon.clone());
                }
            }
        };
        for c in b'0'..=b'9' {
            add(format!("{prefix}.{}", c as char), &[c]);
        }
        for c in b'A'..=b'Z' {
            let lower = c + 0x20;
            if big {
                add(format!("{prefix}.{}", c as char), &[c, lower]);
            } else {
                add(format!("{prefix}.{}", c as char), &[c]);
                add(format!("{prefix}.little_{}", lower as char), &[lower]);
            }
        }
        for (name, code) in NAMED {
            add(format!("{prefix}.{name}"), &[code]);
            if !big && code >= 0xC0 {
                add(format!("{prefix}.little_{name}"), &[code + 0x20]);
            }
        }
        add(format!("{prefix}.93"), &[b']']);
        // Latin-1 numbered files (accented letters, ¡ ¿ ß ...).
        for code in 0xA1u8..=0xFF {
            let codes: Vec<u8> = if big && (0xC0..0xDF).contains(&code) {
                vec![code, code + 0x20]
            } else {
                vec![code]
            };
            add(format!("{prefix}.{code}"), &codes);
        }
        if !big {
            // Controller-button symbols c0..c11 -> codes 1..12.
            for i in 0..12u8 {
                add(format!("{prefix}.c{i}"), &[i + 1]);
            }
        }
        f
    }

    /// The same font with every glyph passed through `f`.
    pub fn map(&self, f: &impl Fn(&Icon) -> Icon) -> Font {
        Font {
            glyphs: self.glyphs.iter().map(|(&c, g)| (c, f(g))).collect(),
            w: self.w * 2,
            h: self.h * 2,
        }
    }

    pub fn glyph(&self, c: u8) -> Option<&Icon> {
        self.glyphs.get(&c).or_else(|| self.glyphs.get(&b'?'))
    }
}

/// Every icon the in-game HUD uses.
#[derive(Resource)]
pub struct HudIcons {
    pub big: Font,
    pub you: Font,
    pub them: Font,
    pub small_digits: Vec<Icon>,
    pub small_x: Icon,
    pub small_person: Icon,
    pub heart_big: Icon,
    pub heart_small: Icon,
    pub siren: Vec<Icon>,
    /// (normal, bright) by HUD slot: fist, pistol, shotgun, flamethrower,
    /// grenade, minigun, bazooka, bomb.
    pub weapons: Vec<(Icon, Icon)>,
    /// Phone, Lips, Mobile, Pager, Info.
    pub speakers: Vec<Icon>,
    pub pager: Icon,
    pub pager_digits: Vec<Icon>,
    pub lights: Vec<Icon>,
    pub blobs: Vec<Icon>,
    pub checkpoint: [Icon; 2],
    /// `EffectTextures/FadeDarken` (message box, kills box) and
    /// `FadeDarken2` (vehicle-name banner), 64×64, stretched over panels
    /// (`Draw2DEffect`); never doubled.
    pub fade: [Icon; 2],
}

impl HudIcons {
    /// The software hi-res set: every icon doubled with [`Icon::doubled`]
    /// (the original loads them so when `AreIconsScaled` @ 10085eb0).
    pub fn doubled(&self, avg: &Average) -> HudIcons {
        let f = |i: &Icon| i.doubled(avg);
        let all = |v: &[Icon]| v.iter().map(f).collect::<Vec<_>>();
        HudIcons {
            big: self.big.map(&f),
            you: self.you.map(&f),
            them: self.them.map(&f),
            small_digits: all(&self.small_digits),
            small_x: f(&self.small_x),
            small_person: f(&self.small_person),
            heart_big: f(&self.heart_big),
            heart_small: f(&self.heart_small),
            siren: all(&self.siren),
            weapons: self.weapons.iter().map(|(a, b)| (f(a), f(b))).collect(),
            speakers: all(&self.speakers),
            pager: f(&self.pager),
            pager_digits: all(&self.pager_digits),
            lights: all(&self.lights),
            blobs: all(&self.blobs),
            checkpoint: [f(&self.checkpoint[0]), f(&self.checkpoint[1])],
            fade: self.fade.clone(),
        }
    }

    pub fn load(data: &GameData) -> HudIcons {
        let small = |n: &str| Icon::load(data, &format!("Small.{n}"), 9, 13);
        let weapon_names = [
            "Fist",
            "Pistol",
            "Shotgun",
            "Flamethrower",
            "Grenade",
            "Minigun",
            "Bazooka",
            "Bomb",
        ];
        HudIcons {
            big: Font::load(data, "Big", 23, 22),
            you: Font::load(data, "You", 6, 10),
            them: Font::load(data, "Them", 6, 10),
            small_digits: (0..10).map(|d| small(&d.to_string())).collect(),
            small_x: small("x"),
            small_person: small("Person"),
            heart_big: Icon::load(data, "Heart.Big", 9, 10),
            heart_small: Icon::load(data, "Heart.Small", 9, 10),
            siren: (1..=10)
                .map(|i| Icon::load(data, &format!("Siren.{i}"), 9, 13))
                .collect(),
            weapons: weapon_names
                .iter()
                .map(|n| {
                    (
                        Icon::load(data, &format!("{n}.Normal"), 9, 20),
                        Icon::load(data, &format!("{n}.Bright"), 9, 20),
                    )
                })
                .collect(),
            speakers: ["Phone", "Lips", "Mobile", "Pager", "Info"]
                .iter()
                .map(|n| Icon::load(data, n, 25, 38))
                .collect(),
            pager: Icon::load(data, "BigPager.image", 38, 38),
            pager_digits: (0..10)
                .map(|d| Icon::load(data, &format!("BigPager.{d}"), 7, 7))
                .collect(),
            lights: (0..4)
                .map(|i| Icon::load(data, &format!("Lights.{i}"), 37, 20))
                .collect(),
            blobs: (0..4)
                .map(|i| Icon::load(data, &format!("Blob.{i}"), 7, 7))
                .collect(),
            checkpoint: [
                Icon::load(data, "Checkpoint.red", 54, 54),
                Icon::load(data, "Checkpoint.blue", 54, 54),
            ],
            fade: ["FadeDarken", "FadeDarken2"].map(|n| {
                let px = data.read(&format!("EffectTextures/{n}")).unwrap_or_default();
                let mut px = px.get(..64 * 64).map_or_else(Vec::new, <[u8]>::to_vec);
                px.resize(64 * 64, 0);
                Icon { w: 64, h: 64, px }
            }),
        }
    }
}
