//! Front-end pictures: title screen, arrows, cups, progress bar, logo and
//! city thumbnails (loaded in `FUN_010138dc` / `FUN_0104feb0`).

use crate::data::GameData;
use crate::hud::icons::Icon;
use bevy::prelude::*;

/// A full-screen palette picture.
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Picture {
    fn load(data: &GameData, path: &str, w: usize, h: usize) -> Picture {
        let mut px = data.read(path).unwrap_or_else(|e| {
            warn!("{path}: {e}");
            Vec::new()
        });
        px.resize(w * h, 0);
        Picture { w, h, px }
    }
}

#[derive(Resource)]
pub struct FrontendIcons {
    /// `TitleScreen.medium` (320×256), the PAL-sized title picture.
    pub title: Picture,
    pub left_arrow: Icon,
    pub right_arrow: Icon,
    pub cursor: Icon,
    /// `Cup.None/.Bronze/.Silver/.Gold`.
    pub cups: [Icon; 4],
    pub loading: Icon,
    pub piece: Icon,
    pub piece_empty: Icon,
    pub logo: Icon,
    /// `Maps/0..2.thumb`, 256×256.
    pub thumbs: Vec<Picture>,
}

impl FrontendIcons {
    pub fn load(data: &GameData) -> FrontendIcons {
        let cup = |n: &str| Icon::load(data, &format!("Cup.{n}"), 15, 20);
        FrontendIcons {
            title: Picture::load(data, "Icons/TitleScreen.medium", 320, 256),
            left_arrow: shadow_icon(data, "LeftArrow"),
            right_arrow: shadow_icon(data, "RightArrow"),
            cursor: Icon::load(data, "Cursor", 6, 10),
            cups: [cup("None"), cup("Bronze"), cup("Silver"), cup("Gold")],
            loading: Icon::load(data, "Loading", 94, 35),
            piece: Icon::load(data, "LoadingPiece", 6, 16),
            piece_empty: Icon::load(data, "LoadingPiece2", 6, 16),
            logo: Icon::load(data, "PaybackLogo", 166, 80),
            thumbs: (0..3)
                .map(|n| Picture::load(data, &format!("Maps/{n}.thumb"), 256, 256))
                .collect(),
        }
    }
}

/// `LoadIconShadow` (`FUN_01008ed4`): a 13×10 icon becomes 14×11 with a
/// colour-8 (black) drop shadow at (+1,+1) under every opaque pixel.
fn shadow_icon(data: &GameData, name: &str) -> Icon {
    let src = Icon::load(data, name, 13, 10);
    let (w, h) = (14, 11);
    let mut px = vec![0u8; w * h];
    for y in 0..10 {
        for x in 0..13 {
            if src.px[y * 13 + x] != 0 {
                px[(y + 1) * w + x + 1] = 8;
            }
        }
    }
    for y in 0..10 {
        for x in 0..13 {
            let c = src.px[y * 13 + x];
            if c != 0 {
                px[y * w + x] = c;
            }
        }
    }
    Icon { w, h, px }
}
