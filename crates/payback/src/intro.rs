//! Intro video player (`Videos/intro.video` + `intro.audio`; 68k
//! `FUN_00010364` load, `FUN_000106c0` play; docs/re/frontend_audio.md §5).
//!
//! The 68k build plays it once at start-up (`FUN_000ec686`) and from the
//! main menu's "View intro". 15 frames per second, locked to 1470-byte
//! chunks of 22050 Hz signed 8-bit audio; any key or button skips.

use crate::audio::wav_from_pcm8;
use crate::data::GameData;
use bevy::{
    asset::RenderAssetUsages,
    input::{ButtonState, keyboard::KeyboardInput, mouse::MouseButtonInput},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use payback_formats::flc::Flc;

/// Playback rate: 1470 audio bytes per frame at 22050 Hz.
const FPS: f32 = 15.0;
const AUDIO_RATE: u32 = 22050;
/// Audio stops once a frame's chunk would start past this (`0x97D36`).
const AUDIO_END: usize = 0x97d36;

/// Ask for the intro to be played.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct PlayIntro;

/// Sent when the intro has ended or was skipped (or could not be loaded).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct IntroFinished;

/// The intro currently playing.
#[derive(Resource)]
pub struct Intro {
    flc: Flc,
    image: Handle<Image>,
    started: f32,
}

/// Whether the intro is on screen (menus should not react to input).
pub fn intro_playing(intro: Option<Res<Intro>>) -> bool {
    intro.is_some()
}

#[derive(Component)]
struct IntroNode;

#[derive(Component)]
struct IntroAudio;

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayIntro>()
            .add_message::<IntroFinished>()
            .add_systems(Startup, play_at_startup)
            .add_systems(Update, (start, advance).chain());
    }
}

/// Start-up intro when booting into the menus (not with `--map`,
/// `--screenshot` or `--no-intro`).
fn play_at_startup(mut play: MessageWriter<PlayIntro>) {
    let skip = ["--map", "--screenshot", "--no-intro"]
        .iter()
        .any(|a| std::env::args().any(|x| x == *a));
    if !skip || std::env::args().any(|x| x == "--intro") {
        play.write(PlayIntro);
    }
}

fn start(
    mut commands: Commands,
    mut requests: MessageReader<PlayIntro>,
    mut finished: MessageWriter<IntroFinished>,
    data: Res<GameData>,
    time: Res<Time<Real>>,
    mut images: ResMut<Assets<Image>>,
    mut sources: ResMut<Assets<AudioSource>>,
    playing: Option<Res<Intro>>,
) {
    if requests.read().count() == 0 || playing.is_some() {
        return;
    }
    let flc = match data.read("Videos/intro.video").map_err(|e| e.to_string()).and_then(|b| Flc::parse(b).map_err(|e| e.to_string())) {
        Ok(f) => f,
        Err(e) => {
            warn!("intro: {e}");
            finished.write(IntroFinished);
            return;
        }
    };
    let image = images.add(Image::new_fill(
        Extent3d { width: flc.width as u32, height: flc.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    // Full-screen black backdrop with the 4:3 picture centred.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(1000),
            IntroNode,
        ))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(image.clone()),
                Node { height: Val::Percent(100.0), aspect_ratio: Some(4.0 / 3.0), ..default() },
            ));
        });
    if let Ok(pcm) = data.read("Videos/intro.audio") {
        // Frames 0..=423 carry sound; the tail is never played.
        let end = pcm.len().min(AUDIO_END.div_ceil(1470) * 1470);
        let source = sources.add(AudioSource { bytes: wav_from_pcm8(&pcm[..end], AUDIO_RATE).into() });
        commands.spawn((AudioPlayer(source), PlaybackSettings::DESPAWN, IntroAudio));
    }
    commands.insert_resource(Intro { flc, image, started: time.elapsed_secs() });
}

#[allow(clippy::too_many_arguments)]
fn advance(
    mut commands: Commands,
    intro: Option<ResMut<Intro>>,
    time: Res<Time<Real>>,
    mut images: ResMut<Assets<Image>>,
    mut keys: MessageReader<KeyboardInput>,
    mut mouse: MessageReader<MouseButtonInput>,
    pads: Query<&Gamepad>,
    mut finished: MessageWriter<IntroFinished>,
    nodes: Query<Entity, Or<(With<IntroNode>, With<IntroAudio>)>>,
) {
    let Some(mut intro) = intro else {
        keys.clear();
        mouse.clear();
        return;
    };
    let skipped = keys.read().any(|k| k.state == ButtonState::Pressed)
        | mouse.read().any(|m| m.state == ButtonState::Pressed)
        | pads.iter().any(|p| p.get_just_pressed().next().is_some());
    let due = ((time.elapsed_secs() - intro.started) * FPS) as u32 + 1;
    let mut ended = skipped;
    let mut changed = false;
    while !ended && (intro.flc.position() as u32) < due {
        match intro.flc.next_frame(false) {
            Ok(true) => changed = true,
            Ok(false) => ended = true,
            Err(e) => {
                warn!("intro: {e}");
                ended = true;
            }
        }
    }
    if changed {
        let rgba = intro.flc.rgba();
        if let Some(mut img) = images.get_mut(&intro.image) {
            img.data = Some(rgba);
        }
    }
    if ended {
        for e in &nodes {
            commands.entity(e).despawn();
        }
        commands.remove_resource::<Intro>();
        finished.write(IntroFinished);
    }
}
