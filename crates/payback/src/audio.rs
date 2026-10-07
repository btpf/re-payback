//! Sound effects, reimplementing the original software mixer's behaviour
//! (docs/re/frontend_audio.md, OS4 `FUN_01007290` / `FUN_0100815c`).
//!
//! The original mixes at 22050 Hz; every AddSound call gives the playback
//! rate in Hz and a volume. Positional sounds are heard with amplitude
//! `A = vol / d²` (d in world units >> 4), clamped to 255, audible when
//! `A > 19`; at most 6 voices play at once. Samples are headerless signed
//! 8-bit PCM, wrapped here as WAVs at 22050 Hz and sped up or down to the
//! requested rate. Mixing is done here (a [`MixerStream`] played as one
//! endless stereo source), so the original's per-voice left/right volumes
//! (pan) and Doppler pitch can be reproduced.

use crate::data::GameData;
use bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source},
    platform::collections::HashMap,
    prelude::*,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MIX_RATE: u32 = 22050;
/// Max simultaneously audible voices (original mixer limit).
const MAX_VOICES: usize = 6;
const AUDIBLE: f32 = 19.0;

/// Sample files by sound id, in the order `InitSound` loads them
/// (OS4 `FUN_01005864`). Ids 50-52 are footstep variants of Footstep.sand.
pub const SOUND_FILES: [&str; 77] = [
    "Explosion",
    "Engine",
    "Skid",
    "CarHorn",
    "Footstep.stone",
    "Thud",
    "ShipHorn",
    "Burp",
    "Ambulance",
    "CarStart",
    "Heartbeat",
    "Siren",
    "Squelch",
    "Flames",
    "TelephoneRing",
    "LongAaah",
    "SqueakyVoice",
    "FireMissile",
    "GrenadeBounce",
    "Minigun",
    "PickupWeapon",
    "ShotgunFire",
    "GarageDoor",
    "Bleep",
    "TickTock",
    "MissionFail",
    "MissionComplete",
    "TankEngine",
    "BigGunFire",
    "Ricochet",
    "HitScenery",
    "ThumpingMusic",
    "ChangeSelection",
    "ChangeSelection2",
    "Footstep.grass",
    "Footstep.metal",
    "Footstep.sand",
    "Ow1",
    "Ow2",
    "Ow3",
    "Doorslam",
    "Flamethrower",
    "Click",
    "Splash",
    "IceCream",
    "ChangeSelection3",
    "Engine_clonk",
    "ObjectAppear",
    "Alarm",
    "BrokenGlass",
    "Footstep.sand",
    "Footstep.sand",
    "Footstep.sand",
    "Pistol",
    "HelicopterBlades",
    "Beep1",
    "Beep2",
    "Beep3",
    "Hiss",
    "Punch_Hit",
    "RCEngine",
    "Rain",
    "Lightning",
    "BoatEngine",
    "Alarm2",
    "Footstep.water",
    "Boom",
    "Engine_904",
    "Engine_911Turbo",
    "Engine_935",
    "Engine_Bus",
    "Engine_904_clonk",
    "Engine_911Turbo_clonk",
    "Engine_935_clonk",
    "Engine_Bus_clonk",
    "Thud2",
    "Screams",
];

/// Named sound ids used by game code.
#[allow(dead_code)]
pub mod id {
    pub const EXPLOSION: u16 = 0;
    pub const ENGINE: u16 = 1;
    pub const SKID: u16 = 2;
    pub const CAR_HORN: u16 = 3;
    pub const FOOTSTEP_STONE: u16 = 4;
    pub const THUD: u16 = 5;
    pub const SIREN: u16 = 11;
    pub const SQUELCH: u16 = 12;
    pub const FLAMES: u16 = 13;
    pub const TELEPHONE_RING: u16 = 14;
    pub const LONG_AAAH: u16 = 15;
    pub const SQUEAKY_VOICE: u16 = 16;
    pub const FIRE_MISSILE: u16 = 17;
    pub const GRENADE_BOUNCE: u16 = 18;
    pub const MINIGUN: u16 = 19;
    pub const PICKUP_WEAPON: u16 = 20;
    pub const SHOTGUN_FIRE: u16 = 21;
    pub const GARAGE_DOOR: u16 = 22;
    pub const BLEEP: u16 = 23;
    pub const TICK_TOCK: u16 = 24;
    pub const MISSION_FAIL: u16 = 25;
    pub const MISSION_COMPLETE: u16 = 26;
    pub const TANK_ENGINE: u16 = 27;
    pub const BIG_GUN_FIRE: u16 = 28;
    pub const RICOCHET: u16 = 29;
    pub const HIT_SCENERY: u16 = 30;
    pub const CHANGE_SELECTION: u16 = 32;
    pub const CHANGE_SELECTION2: u16 = 33;
    pub const OW1: u16 = 37;
    pub const DOORSLAM: u16 = 40;
    pub const FLAMETHROWER: u16 = 41;
    pub const CLICK: u16 = 42;
    pub const SPLASH: u16 = 43;
    pub const ICE_CREAM: u16 = 44;
    pub const CHANGE_SELECTION3: u16 = 45;
    pub const ENGINE_CLONK: u16 = 46;
    pub const OBJECT_APPEAR: u16 = 47;
    pub const BROKEN_GLASS: u16 = 49;
    pub const PISTOL: u16 = 53;
    pub const HELICOPTER_BLADES: u16 = 54;
    pub const BEEP1: u16 = 55;
    pub const BEEP2: u16 = 56;
    pub const BEEP3: u16 = 57;
    pub const PUNCH_HIT: u16 = 59;
    pub const RC_ENGINE: u16 = 60;
    pub const RAIN: u16 = 61;
    pub const BOAT_ENGINE: u16 = 63;
    pub const FOOTSTEP_WATER: u16 = 65;
    pub const BOOM: u16 = 66;
    pub const ENGINE_904: u16 = 67;
    pub const ENGINE_911: u16 = 68;
    pub const ENGINE_935: u16 = 69;
    pub const ENGINE_BUS: u16 = 70;
    pub const THUD2: u16 = 75;
    pub const SCREAMS: u16 = 76;
}

/// Samples by sound id: signed 8-bit PCM.
#[derive(Resource, Default)]
pub struct Sounds {
    by_id: Vec<Option<Arc<[i8]>>>,
}

impl Sounds {
    pub fn get(&self, id: u16) -> Option<Arc<[i8]>> {
        self.by_id.get(id as usize).cloned().flatten()
    }
}

/// Where a sound is heard from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundPos {
    /// Non-positional (HUD, stings): amplitude = vol.
    Global,
    /// World position in original units (2048 per square).
    At(Vec3),
}

/// A playing (or requested) sound. Spawn an entity with this component to
/// play it; looping sounds live until the entity is despawned, and their
/// fields may be changed each frame (the original's ModifySound).
#[derive(Component, Clone, Debug)]
pub struct Sound {
    pub id: u16,
    pub looping: bool,
    /// Original volume value (e.g. 10_000_000 for engines, 255 global).
    pub volume: f32,
    /// Playback rate in Hz.
    pub hz: f32,
    pub pos: SoundPos,
}

impl Sound {
    pub fn once(id: u16, volume: f32, hz: f32, pos: SoundPos) -> Sound {
        Sound {
            id,
            looping: false,
            volume,
            hz,
            pos,
        }
    }

    pub fn looping(id: u16, volume: f32, hz: f32, pos: SoundPos) -> Sound {
        Sound {
            id,
            looping: true,
            volume,
            hz,
            pos,
        }
    }
}

/// The point sounds are heard from (player, or the vehicle being driven),
/// in original world units.
#[derive(Resource, Default, Clone, Copy)]
pub struct Listener(pub Vec3);

/// Split screen: the other players' listening points. The original
/// prioritises each sound per (sound, listener) (`FUN_01006750`); here a
/// sound is as loud as at its nearest listener.
#[derive(Resource, Default, Clone)]
pub struct OtherListeners(pub Vec<Vec3>);

/// Global FX volume, 0-100 (pause-menu option).
#[derive(Resource, Clone, Copy)]
pub struct FxVolume(pub u8);

impl Default for FxVolume {
    fn default() -> Self {
        FxVolume(100)
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<MixerStream>()
            .init_resource::<Listener>()
            .init_resource::<OtherListeners>()
            .init_resource::<FxVolume>()
            .init_resource::<Mixer>()
            .add_systems(Startup, (load_sounds, start_mixer))
            .add_systems(PostUpdate, (start_sounds, mix, end_sounds).chain());
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

fn load_sounds(mut commands: Commands, data: Res<GameData>) {
    let mut sounds = Sounds::default();
    for name in SOUND_FILES {
        let pcm = data
            .read(format!("Sounds/{name}"))
            .ok()
            .map(|bytes| bytes.iter().map(|&b| b as i8).collect::<Arc<[i8]>>());
        if pcm.is_none() {
            warn!("missing sound {name}");
        }
        sounds.by_id.push(pcm);
    }
    commands.insert_resource(sounds);
}

/// Amplitude 0-255 of a sound at the listener, per the original mixer.
pub fn amplitude(sound: &Sound, listener: Vec3) -> f32 {
    match sound.pos {
        SoundPos::Global => sound.volume.min(255.0),
        SoundPos::At(p) => {
            let d = (p - listener) / 16.0;
            (sound.volume / d.length_squared().max(1.0)).min(255.0)
        }
    }
}

/// Pan of a positional sound (`FUN_01007290`): `r = dx / |dy|` (integer,
/// world units >> 4); the far side gets `A / (|r| + 1)`, the near side `A`;
/// `r = 0` or `dy = 0` is centred. Returns (left, right) factors.
pub fn pan(sound: &Sound, listener: Vec3) -> (f32, f32) {
    let SoundPos::At(p) = sound.pos else {
        return (1.0, 1.0);
    };
    let dx = ((p.x - listener.x) / 16.0) as i32;
    let dy = ((p.y - listener.y) / 16.0) as i32;
    if dy == 0 {
        return (1.0, 1.0);
    }
    let r = dx / dy.abs();
    let far = 1.0 / (r.unsigned_abs() as f32 + 1.0);
    match r.signum() {
        1 => (far, 1.0),
        -1 => (1.0, far),
        _ => (1.0, 1.0),
    }
}

/// One mixer channel.
struct Voice {
    pcm: Arc<[i8]>,
    pos: f64,
    /// Source samples per output sample.
    step: f64,
    left: f32,
    right: f32,
    looping: bool,
    done: bool,
}

/// Voices shared between the game (which sets volumes and pitch each
/// frame) and the audio thread (which renders them).
#[derive(Resource, Clone, Default)]
pub struct Mixer(Arc<Mutex<HashMap<Entity, Voice>>>);

/// The mixer output: an endless 22050 Hz stereo stream.
#[derive(Asset, TypePath, Clone)]
pub struct MixerStream(Mixer);

impl Decodable for MixerStream {
    type Decoder = MixerDecoder;

    fn decoder(&self) -> MixerDecoder {
        MixerDecoder {
            mixer: self.0.clone(),
            buf: Vec::new(),
            at: 0,
        }
    }
}

pub struct MixerDecoder {
    mixer: Mixer,
    /// Rendered interleaved stereo block and the read position in it.
    buf: Vec<f32>,
    at: usize,
}

/// Frames rendered per lock of the voice table (about 12 ms).
const BLOCK: usize = 256;

impl Iterator for MixerDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.buf.len() {
            self.buf.clear();
            self.buf.resize(BLOCK * 2, 0.0);
            self.at = 0;
            if let Ok(mut voices) = self.mixer.0.lock() {
                for v in voices.values_mut().filter(|v| !v.done) {
                    for frame in self.buf.chunks_mut(2) {
                        if v.pos as usize >= v.pcm.len() {
                            if v.looping && !v.pcm.is_empty() {
                                v.pos %= v.pcm.len() as f64;
                            } else {
                                v.done = true;
                                break;
                            }
                        }
                        let s = v.pcm[v.pos as usize] as f32 / 128.0;
                        frame[0] += s * v.left;
                        frame[1] += s * v.right;
                        v.pos += v.step;
                    }
                }
            }
        }
        let s = self.buf[self.at];
        self.at += 1;
        Some(s.clamp(-1.0, 1.0))
    }
}

impl Source for MixerDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("non-zero")
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(MIX_RATE).expect("non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn start_mixer(mut commands: Commands, mixer: Res<Mixer>, mut streams: ResMut<Assets<MixerStream>>) {
    let stream = streams.add(MixerStream(mixer.clone()));
    commands.spawn((AudioPlayer(stream), PlaybackSettings::LOOP));
}

#[derive(Component)]
struct Started;

/// Distance to the listener last frame (Doppler).
#[derive(Component)]
struct LastDistance(f32);

fn start_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    mixer: Res<Mixer>,
    new: Query<(Entity, &Sound), Without<Started>>,
) {
    let Some(sounds) = sounds else { return };
    let Ok(mut voices) = mixer.0.lock() else { return };
    for (e, s) in &new {
        let Some(pcm) = sounds.get(s.id) else {
            commands.entity(e).despawn();
            continue;
        };
        voices.insert(
            e,
            Voice {
                pcm,
                pos: 0.0,
                step: s.hz as f64 / MIX_RATE as f64,
                left: 0.0,
                right: 0.0,
                looping: s.looping,
                done: false,
            },
        );
        commands.entity(e).insert(Started);
    }
}

/// Doppler (`FUN_01007290`): the rate byte (Hz / 200) gains
/// `(prevDist - dist) * 184 / len` per 30 Hz frame of `len` = 1134
/// samples, clamped to 1..255.
fn doppler(hz: f32, approach_per_frame: f32) -> f32 {
    let rate = (hz / 200.0).floor() + (approach_per_frame * 184.0 / 1134.0).trunc();
    rate.clamp(1.0, 255.0) * 200.0
}

/// Apply the original mixer's voice selection, attenuation, pan and pitch.
#[allow(clippy::type_complexity)]
fn mix(
    mut commands: Commands,
    listener: Res<Listener>,
    others: Res<OtherListeners>,
    fx: Res<FxVolume>,
    time: Res<Time<Real>>,
    mixer: Res<Mixer>,
    mut playing: Query<(Entity, &Sound, Option<&mut LastDistance>), With<Started>>,
) {
    // The listener a sound is heard best from.
    let nearest = |s: &Sound| {
        others.0.iter().fold((amplitude(s, listener.0), listener.0), |best, &l| {
            let a = amplitude(s, l);
            if a > best.0 { (a, l) } else { best }
        })
    };
    let Ok(mut voices) = mixer.0.lock() else { return };
    let mut ranked: Vec<(f32, Vec3, Entity, &Sound, Option<Mut<LastDistance>>)> = playing
        .iter_mut()
        .map(|(e, s, last)| {
            let (a, l) = nearest(s);
            (a, l, e, s, last)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let master = fx.0 as f32 / 100.0;
    // Original mixer frames (30 Hz) since the last update.
    let frames = (time.delta_secs() * 30.0).max(1e-3);
    for (rank, (a, l, e, s, last)) in ranked.into_iter().enumerate() {
        let Some(v) = voices.get_mut(&e) else { continue };
        let audible = rank < MAX_VOICES && a > AUDIBLE;
        // Three voices at full amplitude sum to full scale.
        let gain = if audible { a / 255.0 / 3.0 * master } else { 0.0 };
        let (pl, pr) = pan(s, l);
        v.left = gain * pl;
        v.right = gain * pr;
        let mut hz = s.hz;
        if let SoundPos::At(p) = s.pos {
            let dist = (p - l).length() / 16.0;
            match last {
                Some(mut last) => {
                    hz = doppler(s.hz, (last.0 - dist) / frames);
                    last.0 = dist;
                }
                None => {
                    commands.entity(e).insert(LastDistance(dist));
                }
            }
        }
        v.step = hz as f64 / MIX_RATE as f64;
    }
}

/// One-shot sounds end when their sample has played; voices of despawned
/// sounds are dropped.
fn end_sounds(mut commands: Commands, mixer: Res<Mixer>, alive: Query<(), With<Sound>>) {
    let Ok(mut voices) = mixer.0.lock() else { return };
    voices.retain(|&e, v| {
        if !alive.contains(e) {
            return false;
        }
        if v.done {
            commands.entity(e).despawn();
            return false;
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header() {
        let w = wav_from_pcm8(&[0x00, 0x80, 0x7f], 8000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 3);
        assert_eq!(&w[44..47], &[0x80, 0x00, 0xff]);
        assert_eq!(w.len() % 2, 0);
    }

    #[test]
    fn engine_range() {
        // Engines (vol 10M) are full volume within ~1.5 squares, silent past ~5.7.
        let s = Sound::looping(id::ENGINE, 10_000_000.0, 20000.0, SoundPos::At(Vec3::ZERO));
        assert_eq!(amplitude(&s, Vec3::new(198.0 * 16.0, 0.0, 0.0)), 255.0);
        assert!(amplitude(&s, Vec3::new(730.0 * 16.0, 0.0, 0.0)) < AUDIBLE);
        let g = Sound::once(id::BEEP1, 160.0, 4182.0, SoundPos::Global);
        assert_eq!(amplitude(&g, Vec3::splat(1e6)), 160.0);
    }

    #[test]
    fn mixer_renders_voices() {
        let mixer = Mixer::default();
        let pcm: Arc<[i8]> = Arc::from(vec![64i8; 100]);
        let voice = |looping, left, right| Voice { pcm: pcm.clone(), pos: 0.0, step: 1.0, left, right, looping, done: false };
        {
            let mut v = mixer.0.lock().unwrap();
            v.insert(Entity::from_raw_u32(1).unwrap(), voice(true, 1.0, 0.0));
            v.insert(Entity::from_raw_u32(2).unwrap(), voice(false, 0.0, 0.5));
        }
        let mut d = MixerStream(mixer.clone()).decoder();
        let (l, r) = (d.next().unwrap(), d.next().unwrap());
        assert_eq!((l, r), (0.5, 0.25));
        // The one-shot (100 samples) is done after the first block; the loop isn't.
        let _: Vec<f32> = (0..BLOCK * 4).map(|_| d.next().unwrap()).collect();
        let v = mixer.0.lock().unwrap();
        assert!(v.values().any(|v| v.looping && !v.done));
        assert!(v.values().any(|v| !v.looping && v.done));
    }

    #[test]
    fn pan_and_doppler() {
        let at = |x: f32, y: f32| Sound::once(id::THUD, 1e6, 15000.0, SoundPos::At(Vec3::new(x, y, 0.0)));
        // Within the |dx| < |dy| cone the integer ratio is 0: centred.
        assert_eq!(pan(&at(-100.0 * 16.0, 200.0 * 16.0), Vec3::ZERO), (1.0, 1.0));
        // Twice as far east as south: r = 2, left at A/3.
        assert_eq!(pan(&at(400.0 * 16.0, 200.0 * 16.0), Vec3::ZERO), (1.0 / 3.0, 1.0));
        assert_eq!(pan(&at(-400.0 * 16.0, -200.0 * 16.0), Vec3::ZERO), (1.0, 1.0 / 3.0));
        // dy = 0 is centred (quirk of the original).
        assert_eq!(pan(&at(500.0 * 16.0, 0.0), Vec3::ZERO), (1.0, 1.0));
        // Approaching 2 x 1134/184 units per frame raises the rate two steps.
        assert_eq!(doppler(15000.0, 0.0), 15000.0);
        assert_eq!(doppler(15000.0, 1134.0 / 184.0 * 2.0), 15400.0);
        assert_eq!(doppler(100.0, -1e6), 200.0);
    }
}
