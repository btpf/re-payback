//! The `Loading` box at its native size (docs/re/frontend_audio.md §6.3).
//!
//! InitProgress (`FUN_01042ff8`, Mac `InitProgress__FPc` @ 10053510)
//! clears a 108×46 box round the screen centre, `(cx−54, cy−22)–(cx+53,
//! cy+23)`, and draws `Loading` (94×35) at `(cx−46, cy−17)`; DrawProgress
//! (`FUN_01042688`, Mac `DrawProgress__Fi` @ 100538c0) adds a
//! `LoadingPiece` (6×16) per segment at `(cx−46+8i, cy+2)`. The pieces
//! and `Loading` are never doubled, so in hi-res (every Mac size) the box
//! is half the size it has on the lo-res screen (`analysis/mac/
//! rendering.md` §7, frontend.md §5.6).
//!
//! The front end's canvases are always lo-res (256 lines), so the box is
//! its own image node, scaled like a pixel of the HUD canvas
//! ([`super::hud_canvas`]): native size on the hi-res canvas, the
//! front end's pixel size otherwise. The front end publishes the panel
//! each frame it shows it ([`LoadingPanel`]).

use crate::data::GameData;
use crate::frontend::assets::FrontendIcons;
use bevy::{prelude::*, window::PrimaryWindow};

use super::paint::{Painter, blank_sized};

/// Box size and the `Loading` / piece offsets inside it.
const BOX: (usize, usize) = (108, 46);
const LOADING_AT: (i32, i32) = (8, 5);
const PIECES_Y: i32 = 24;

/// Set by the front end on every frame the loading panel shows: the filled
/// pieces, 0..=12.
#[derive(Resource, Clone, Copy, Debug)]
pub struct LoadingPanel(pub usize);

#[derive(Component)]
struct LoadingBox {
    image: Handle<Image>,
    pieces: Option<usize>,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn).add_systems(PostUpdate, show);
}

fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(blank_sized(BOX.0, BOX.1));
    commands.spawn((
        ImageNode::new(image.clone()),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        // Above the front end's layers (`GlobalZIndex(10)`).
        GlobalZIndex(20),
        Visibility::Hidden,
        LoadingBox { image, pieces: None },
    ));
}

fn show(
    panel: Option<Res<LoadingPanel>>,
    window: Query<&Window, With<PrimaryWindow>>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    icons: Option<Res<FrontendIcons>>,
    data: Res<GameData>,
    mut images: ResMut<Assets<Image>>,
    mut boxes: Query<(&mut LoadingBox, &mut Node, &mut Visibility)>,
) {
    let Ok((mut b, mut node, mut vis)) = boxes.single_mut() else { return };
    // Shown only while the front end keeps publishing the panel.
    let pieces = panel.filter(|p| p.is_changed()).map(|p| p.0.min(12));
    let (Some(pieces), Ok(window), Some(icons)) = (pieces, window.single(), icons) else {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        b.pieces = None;
        return;
    };
    if b.pieces != Some(pieces) {
        b.pieces = Some(pieces);
        if let Some(mut img) = images.get_mut(&b.image)
            && let Some(px) = img.data.as_mut()
        {
            let mut p = Painter { px, w: BOX.0, pal: &data.palette };
            for y in 0..BOX.1 as i32 {
                for x in 0..BOX.0 as i32 {
                    p.put_rgb(x, y, [0, 0, 0]);
                }
            }
            p.icon(&icons.loading, LOADING_AT.0, LOADING_AT.1, super::Blend::Plain);
            for i in 0..pieces as i32 {
                p.icon(&icons.piece, LOADING_AT.0 + 8 * i, PIECES_Y, super::Blend::Plain);
            }
        }
    }
    // One canvas pixel in logical pixels, and the box at (cx−54, cy−22)
    // with cx = (w−1)/2, cy = (h−1)/2 as the front end centres it.
    let (cw, ch, _) = super::hud_canvas(window.physical_size(), settings.as_ref().is_none_or(|s| s.hires_hud));
    let (sx, sy) = (window.width() / cw as f32, window.height() / ch as f32);
    let (cx, cy) = ((cw as i32 - 1) / 2, (ch as i32 - 1) / 2);
    let want = (
        Val::Px((cx - 54) as f32 * sx),
        Val::Px((cy - 22) as f32 * sy),
        Val::Px(BOX.0 as f32 * sx),
        Val::Px(BOX.1 as f32 * sy),
    );
    if (node.left, node.top, node.width, node.height) != want {
        (node.left, node.top, node.width, node.height) = want;
    }
    if *vis != Visibility::Inherited {
        *vis = Visibility::Inherited;
    }
}
