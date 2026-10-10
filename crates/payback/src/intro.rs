//! Intro video player: the Amiga FLC or the Mac MPEG film
//! (docs/re/frontend_audio.md §5, docs/re/mac/audio_platform.md §3).
//!
//! Amiga: `Videos/intro.video` + `intro.audio` (68k `FUN_00010364` load,
//! `FUN_000106c0` play), 15 frames per second locked to 1470-byte chunks of
//! 22050 Hz signed 8-bit audio. The 68k build plays it once at the end of
//! start-up loading (`FUN_000ec686`, call at 0xeda52; requested by
//! `startup.rs`) and from the main menu's "View intro".
//!
//! Mac: `OpenAnimation` @ 100fb130 ignores its argument and
//! `GetPaybackMovie` @ 100fcbc0 opens the hard-coded
//! `:Videos:payback_high_mpeg1.mpg` (MPEG-1 640×480 25 fps, MP2 48 kHz, 72 s);
//! `PlayAnimation` @ 100fb1d0 scales it to fit the window keeping 4:3
//! (`ScaleRectToFit`) on black and plays it with its own sound at full
//! volume. Same call sites (`InitAll` @ 100632a0, `DoGameMenu` @ 100a05e0).
//! The film is decoded with the vendored pl_mpeg (`payback-mpeg`); it is
//! used when `Settings.variants.intro` is on (or the Amiga data is missing:
//! `GameData::mac_variant`) and the Mac data has it, otherwise the FLC
//! plays.
//!
//! Any key or mouse button skips (also a pad button, our extension). Only new
//! presses count: the Mac polls the *held* keys (`ALAnyKeyPressed`), so a key
//! still down from the menu could skip at once, a platform artefact.

use crate::audio::wav_from_pcm8;
use crate::data::GameData;
use crate::frontend::progress::Settings;
use bevy::{
    asset::RenderAssetUsages,
    audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source},
    input::{ButtonState, keyboard::KeyboardInput, mouse::MouseButtonInput},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::widget::NodeImageMode,
    window::PrimaryWindow,
};
use payback_formats::flc::Flc;
use payback_mpeg::Mpeg;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Amiga playback rate: 1470 audio bytes per frame at 22050 Hz.
const FPS: f32 = 15.0;
const AUDIO_RATE: u32 = 22050;
/// Audio stops once a frame's chunk would start past this (`0x97D36`).
const AUDIO_END: usize = 0x97d36;
/// The Mac film (`GetPaybackMovie` @ 100fcbc0; data `0x1018fac0`).
const MAC_FILM: &str = "Videos/payback_high_mpeg1.mpg";

/// `--intro-at <seconds>`: start the intro part-way in (a testing aid, e.g.
/// for `--intro --screenshot` mid-film).
fn start_offset() -> f32 {
    crate::arg("--intro-at").and_then(|s| s.parse().ok()).unwrap_or(0.0f32).max(0.0)
}

/// Ask for the intro to be played.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct PlayIntro;

/// Sent when the intro has ended or was skipped (or could not be loaded).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct IntroFinished;

/// The intro currently playing.
#[derive(Resource)]
pub struct Intro {
    video: Video,
    image: Handle<Image>,
    started: f32,
}

enum Video {
    /// The Amiga FLC.
    Flc(Flc),
    /// The Mac film: its decoder, frames shown so far, the RGBA frame buffer
    /// and the audio's play counter (samples pulled by the output, total,
    /// rate; interleaved stereo).
    Film { mpeg: Mpeg, frames: u32, rgba: Vec<u8>, played: Arc<AtomicUsize>, samples: usize, rate: u32 },
}

/// Whether the intro is on screen (menus should not react to input).
pub fn intro_playing(intro: Option<Res<Intro>>) -> bool {
    intro.is_some()
}

#[derive(Component)]
struct IntroNode;

/// The picture, sized to fit the window at 4:3.
#[derive(Component)]
struct IntroPicture;

#[derive(Component)]
struct IntroAudio;

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayIntro>()
            .add_message::<IntroFinished>()
            .add_audio_source::<FilmAudio>()
            .add_systems(Update, (start, advance, fit).chain());
    }
}

/// The film's decoded sound, interleaved, played as one source at full
/// volume (the Mac never sets the movie volume).
#[derive(Asset, TypePath, Clone)]
struct FilmAudio {
    samples: Arc<[f32]>,
    channels: u16,
    rate: u32,
    played: Arc<AtomicUsize>,
}

impl Decodable for FilmAudio {
    type Decoder = FilmDecoder;

    fn decoder(&self) -> FilmDecoder {
        FilmDecoder { audio: self.clone(), at: 0 }
    }
}

struct FilmDecoder {
    audio: FilmAudio,
    at: usize,
}

impl Iterator for FilmDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let s = *self.audio.samples.get(self.at)?;
        self.at += 1;
        self.audio.played.store(self.at, Ordering::Relaxed);
        Some(s)
    }
}

impl Source for FilmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.audio.channels).expect("non-zero")
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.audio.rate).expect("non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.audio.samples.len() / self.audio.channels as usize;
        Some(Duration::from_secs_f64(frames as f64 / self.audio.rate as f64))
    }
}

/// Opens the Mac film: a video-only decoder for playback and the whole MP2
/// track decoded up front (72 s, well under 0.1 s of work).
fn open_film(data: &GameData) -> Option<(Mpeg, Option<FilmAudio>)> {
    let path = data.mac_path(MAC_FILM)?;
    let bytes: Arc<[u8]> = match crate::vfs::read(&path) {
        Ok(b) => b.into(),
        Err(e) => {
            warn!("intro: {}: {e}", path.display());
            return None;
        }
    };
    let Some(mut video) = Mpeg::new(bytes.clone()).filter(|m| m.has_video() && m.framerate() > 0.0) else {
        warn!("intro: {} is not an MPEG-1 film", path.display());
        return None;
    };
    video.set_audio_enabled(false);
    let audio = Mpeg::new(bytes).filter(|m| m.has_audio() && m.samplerate() > 0).map(|mut m| {
        m.set_video_enabled(false);
        let rate = m.samplerate();
        // pl_mpeg always yields stereo (mono duplicated).
        let skip = (start_offset() * rate as f32) as usize * 2;
        let samples: Arc<[f32]> = m.decode_all_audio().get(skip..).unwrap_or_default().into();
        FilmAudio { samples, channels: 2, rate, played: Arc::new(AtomicUsize::new(0)) }
    });
    Some((video, audio))
}

/// The Amiga FLC and its sound.
fn open_flc(data: &GameData) -> Result<(Flc, Option<Vec<u8>>), String> {
    let flc = data
        .read("Videos/intro.video")
        .map_err(|e| e.to_string())
        .and_then(|b| Flc::parse(b).map_err(|e| e.to_string()))?;
    // Frames 0..=423 carry sound; the tail is never played.
    let wav = data.read("Videos/intro.audio").ok().map(|pcm| {
        let end = pcm.len().min(AUDIO_END.div_ceil(1470) * 1470);
        wav_from_pcm8(&pcm[..end], AUDIO_RATE)
    });
    Ok((flc, wav))
}

#[allow(clippy::too_many_arguments)]
fn start(
    mut commands: Commands,
    mut requests: MessageReader<PlayIntro>,
    mut finished: MessageWriter<IntroFinished>,
    data: Res<GameData>,
    settings: Option<Res<Settings>>,
    time: Res<Time<Real>>,
    mut images: ResMut<Assets<Image>>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut films: ResMut<Assets<FilmAudio>>,
    playing: Option<Res<Intro>>,
) {
    if requests.read().count() == 0 || playing.is_some() {
        return;
    }
    let mac = data.mac_variant(settings.is_none_or(|s| s.variants.intro));
    let (video, size) = match mac.then(|| open_film(&data)).flatten() {
        Some((mpeg, audio)) => {
            let size = (mpeg.width(), mpeg.height());
            let (played, samples, rate) = match audio {
                Some(audio) => {
                    let counter = (audio.played.clone(), audio.samples.len(), audio.rate);
                    commands.spawn((AudioPlayer(films.add(audio)), PlaybackSettings::DESPAWN, IntroAudio));
                    counter
                }
                None => (Arc::new(AtomicUsize::new(0)), 0, 0),
            };
            info!("intro: Mac film {}x{} at {} fps, {samples} audio samples", size.0, size.1, mpeg.framerate());
            let rgba = vec![0; size.0 as usize * size.1 as usize * 4];
            (Video::Film { mpeg, frames: 0, rgba, played, samples, rate }, size)
        }
        None => match open_flc(&data) {
            Ok((flc, wav)) => {
                if let Some(wav) = wav {
                    let source = sources.add(AudioSource { bytes: wav.into() });
                    commands.spawn((AudioPlayer(source), PlaybackSettings::DESPAWN, IntroAudio));
                }
                let size = (flc.width as u32, flc.height as u32);
                (Video::Flc(flc), size)
            }
            Err(e) => {
                warn!("intro: {e}");
                finished.write(IntroFinished);
                return;
            }
        },
    };
    let image = images.add(Image::new_fill(
        Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    // Full-screen black backdrop with the 4:3 picture centred (sized by `fit`).
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
                // Stretched: the FLC's 320×200 fills the 4:3 screen like the Amiga's.
                ImageNode::new(image.clone()).with_mode(NodeImageMode::Stretch),
                Node { height: Val::Percent(100.0), aspect_ratio: Some(4.0 / 3.0), ..default() },
                IntroPicture,
            ));
        });
    commands.insert_resource(Intro { video, image, started: time.elapsed_secs() - start_offset() });
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
    // New presses only; auto-repeat of a key held from the menu does not count.
    let skipped = keys.read().any(|k| k.state == ButtonState::Pressed && !k.repeat)
        | mouse.read().any(|m| m.state == ButtonState::Pressed)
        | pads.iter().any(|p| p.get_just_pressed().next().is_some());
    let elapsed = time.elapsed_secs() - intro.started;
    let mut ended = skipped;
    let mut frame = None;
    match &mut intro.video {
        Video::Flc(flc) => {
            let due = (elapsed * FPS) as u32 + 1;
            let mut changed = false;
            while !ended && (flc.position() as u32) < due {
                match flc.next_frame(false) {
                    Ok(true) => changed = true,
                    Ok(false) => ended = true,
                    Err(e) => {
                        warn!("intro: {e}");
                        ended = true;
                    }
                }
            }
            if changed {
                frame = Some(flc.rgba());
            }
        }
        Video::Film { mpeg, frames, rgba, played, samples, rate } => {
            // The sound is the clock while it plays, as with QuickTime; real
            // time before it starts or without sound (and never behind the
            // sound once it has ended).
            let pulled = played.load(Ordering::Relaxed);
            let sound = (pulled as f64 / (2 * *rate).max(1) as f64) as f32 + start_offset();
            let clock = match pulled {
                0 => elapsed,
                p if p < *samples => sound,
                _ => sound.max(elapsed),
            };
            let due = (clock as f64 * mpeg.framerate()) as u32 + 1;
            let mut changed = false;
            // Late frames are decoded and dropped; only the newest is shown.
            while !ended && *frames < due {
                match mpeg.decode_video() {
                    Some(f) => {
                        *frames += 1;
                        if *frames == due {
                            f.to_rgba(rgba);
                            changed = true;
                        }
                    }
                    None => ended = true,
                }
            }
            if changed {
                frame = Some(rgba.clone());
            }
            if ended {
                info!(
                    "intro: film {} after {frames} frames ({elapsed:.1} s real, {clock:.1} s clock), {} of {samples} audio samples played",
                    if skipped { "skipped" } else { "ended" },
                    played.load(Ordering::Relaxed),
                );
            }
        }
    }
    if let Some(rgba) = frame
        && let Some(mut img) = images.get_mut(&intro.image)
    {
        img.data = Some(rgba);
    }
    if ended {
        for e in &nodes {
            commands.entity(e).despawn();
        }
        commands.remove_resource::<Intro>();
        finished.write(IntroFinished);
    }
}

/// `ScaleRectToFit`: the largest 4:3 box that fits the window, centred.
fn fit(windows: Query<&Window, With<PrimaryWindow>>, mut pictures: Query<&mut Node, With<IntroPicture>>) {
    let Ok(window) = windows.single() else {
        return;
    };
    let (w, h) = (window.width(), window.height());
    let (w, h) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 3.0 / 4.0) };
    for mut node in &mut pictures {
        if node.width != Val::Px(w) || node.height != Val::Px(h) {
            node.width = Val::Px(w);
            node.height = Val::Px(h);
            node.aspect_ratio = None;
        }
    }
}
