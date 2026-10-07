//! Sound effects: the original's headerless signed 8-bit samples, wrapped as
//! in-memory WAV files for Bevy's audio backend.

use crate::data::GameData;
use bevy::{audio::Volume, prelude::*};
use std::collections::HashMap;

/// Playback rate used for samples until per-sound rates are recovered.
pub const DEFAULT_RATE: u32 = 16574;

#[derive(Resource, Default)]
pub struct Sounds {
    by_name: HashMap<String, Handle<AudioSource>>,
}

impl Sounds {
    pub fn get(&self, name: &str) -> Option<Handle<AudioSource>> {
        self.by_name.get(&name.to_lowercase()).cloned()
    }
}

/// Request to play a sound once.
#[derive(Message, Clone)]
pub struct PlaySound {
    pub name: &'static str,
    pub volume: f32,
    pub speed: f32,
}

impl PlaySound {
    pub fn new(name: &'static str) -> PlaySound {
        PlaySound {
            name,
            volume: 1.0,
            speed: 1.0,
        }
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySound>()
            .add_systems(Startup, load_sounds)
            .add_systems(Update, play_sounds);
    }
}

/// Build a mono 8-bit PCM WAV. WAV 8-bit samples are unsigned, so flip the
/// sign bit of the Amiga's signed bytes.
pub fn wav_from_pcm8(samples: &[u8], rate: u32) -> Vec<u8> {
    let mut w = Vec::with_capacity(44 + samples.len());
    let len = samples.len() as u32;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); // byte rate
    w.extend_from_slice(&1u16.to_le_bytes()); // block align
    w.extend_from_slice(&8u16.to_le_bytes()); // bits
    w.extend_from_slice(b"data");
    w.extend_from_slice(&len.to_le_bytes());
    w.extend(samples.iter().map(|&b| b ^ 0x80));
    if len % 2 == 1 {
        w.push(0x80);
    }
    w
}

fn load_sounds(
    mut commands: Commands,
    data: Res<GameData>,
    mut sources: ResMut<Assets<AudioSource>>,
) {
    let mut sounds = Sounds::default();
    if let Ok(dir) = std::fs::read_dir(data.root.join("Sounds")) {
        for entry in dir.flatten() {
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            let wav = wav_from_pcm8(&bytes, DEFAULT_RATE);
            let handle = sources.add(AudioSource { bytes: wav.into() });
            sounds
                .by_name
                .insert(entry.file_name().to_string_lossy().to_lowercase(), handle);
        }
    }
    info!("loaded {} sounds", sounds.by_name.len());
    commands.insert_resource(sounds);
}

fn play_sounds(
    mut commands: Commands,
    mut requests: MessageReader<PlaySound>,
    sounds: Res<Sounds>,
) {
    for req in requests.read() {
        let Some(source) = sounds.get(req.name) else {
            warn!("unknown sound {}", req.name);
            continue;
        };
        commands.spawn((
            AudioPlayer(source),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(req.volume))
                .with_speed(req.speed),
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wav_header() {
        let w = super::wav_from_pcm8(&[0x00, 0x80, 0x7f], 8000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 3);
        assert_eq!(&w[44..47], &[0x80, 0x00, 0xff]);
        assert_eq!(w.len() % 2, 0);
    }
}
