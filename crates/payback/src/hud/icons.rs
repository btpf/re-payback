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
        let bytes = find(data, name).and_then(|p| std::fs::read(p).ok());
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

fn find(data: &GameData, name: &str) -> Option<std::path::PathBuf> {
    let dir = data.root.join("Icons");
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(&dir)
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

const NAMED: [(&str, u8); 26] = [
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
    ("Dollar", b'$'),
    ("Percent", b'%'),
    ("Pound", 0xA3),
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
}

impl HudIcons {
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
        }
    }
}
