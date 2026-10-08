//! Sound effects, reimplementing the original software mixer's behaviour
//! (docs/re/frontend_audio.md, OS4 `FUN_01007290` / `FUN_0100815c`; Mac
//! `DoAudio` @ 100f32f0 / `AddSound` @ 100f4000, analysis/mac/audio_platform.md).
//!
//! The original mixes at 22050 Hz; every AddSound call gives the playback
//! rate in Hz (kept as a rate byte, Hz / 200) and a volume. Positional
//! sounds are heard with amplitude `A = vol / d²` (d in world units >> 4),
//! audible when `A > 19`; at most 6 voices play at once. Samples are
//! headerless signed 8-bit PCM, played here at the requested rate with
//! nearest-sample stepping (the Mac opens its channels with `initNoInterp`).
//! Mixing is done here (a [`MixerStream`] played as one endless stereo
//! source), so the per-voice left/right volumes (pan) and Doppler pitch can
//! be reproduced. Levels follow the Mac (`DoAudio` → `CMSPlayPaybackSFX`
//! @ 10156a50): per side `FX · min(A, 255) · 2.56e-5`, summed and clipped.
//! Menu sounds ([`Sound::menu`], Mac `PlaySound` @ 100f1960) mix alongside
//! the game voices; the pause menu pauses the game voices
//! (`PauseAllSounds` @ 100f1810 / `ResumeAllSounds` @ 100f1880).

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
    /// A menu sound (`PlaySound`): exact Hz, volume `FX·256/100`, outside
    /// the 6-voice selection, never paused or cut.
    pub menu: bool,
}

impl Sound {
    pub fn once(id: u16, volume: f32, hz: f32, pos: SoundPos) -> Sound {
        Sound {
            id,
            looping: false,
            volume,
            hz,
            pos,
            menu: false,
        }
    }

    pub fn looping(id: u16, volume: f32, hz: f32, pos: SoundPos) -> Sound {
        Sound {
            id,
            looping: true,
            volume,
            hz,
            pos,
            menu: false,
        }
    }

    /// A menu sound (Mac `PlaySound__Fii` @ 100f1960): played at `hz`
    /// exactly, centred, at `FX·256/100` (unity at FX 100) on any free
    /// channel, mixed with the game voices (the Amiga's `FUN_010053c4`
    /// stopped the mixer channels instead).
    pub fn menu(id: u16, hz: f32) -> Sound {
        Sound {
            menu: true,
            ..Sound::once(id, 255.0, hz, SoundPos::Global)
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

/// Global FX volume, 0-100 in steps of 10 (pause-menu option). Default 50,
/// the Mac's (`ClearPreference` @ 100508c0: `prefs[0] = 0x32`); at 50 the
/// Mac's level law gives the Amiga's loudness at 100.
#[derive(Resource, Clone, Copy)]
pub struct FxVolume(pub u8);

impl Default for FxVolume {
    fn default() -> Self {
        FxVolume(50)
    }
}

impl FxVolume {
    /// The CD music volume tied to the slider (Mac `SetSoundVolume`
    /// @ 100f24f0 → `UCDAudio::SetVolume`): 200/256 up to FX 50, then
    /// `200 − 4·(FX − 50)`, silent at 100. 0..1.
    pub fn music(self) -> f32 {
        let fx = self.0.min(100) as f32;
        (200.0 - 4.0 * (fx - 50.0).max(0.0)) / 256.0
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
            .add_systems(Startup, start_mixer)
            .add_systems(FixedPostUpdate, slot_clock)
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

/// Run by the start-up loader (`startup.rs`).
pub(crate) fn load_sounds(mut commands: Commands, data: Res<GameData>) {
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

/// Unclamped amplitude of a sound at the listener: `vol / d²` (d² = 0 → 1),
/// or `vol` for a non-positional sound.
pub fn amplitude_raw(sound: &Sound, listener: Vec3) -> f32 {
    match sound.pos {
        SoundPos::Global => sound.volume,
        SoundPos::At(p) => {
            let d = (p - listener) / 16.0;
            sound.volume / d.length_squared().max(1.0)
        }
    }
}

/// Amplitude 0-255 of a sound at the listener, per the original mixer.
pub fn amplitude(sound: &Sound, listener: Vec3) -> f32 {
    amplitude_raw(sound, listener).min(255.0)
}

/// Left and right levels 0..255 of a sound (Mac `DoAudio` @ 100f32f0,
/// OS4 `FUN_01007290` from 0x01007590): `dx`, `dy` are differences of the
/// positions >> 4; `r = dx / |dy|` in floating point; the near side gets
/// `min(A, 255)`, the far side `min(A / (|r| + 1), 255)` with the
/// unclamped `A`, so a loud close sound stays centred. `|dy| < 1` or
/// `r = 0` is centred.
pub fn pan(sound: &Sound, listener: Vec3) -> (f32, f32) {
    let a = amplitude_raw(sound, listener);
    let near = a.min(255.0);
    let SoundPos::At(p) = sound.pos else {
        return (near, near);
    };
    let dx = ((p.x as i32) >> 4) - ((listener.x as i32) >> 4);
    let dy = (((p.y as i32) >> 4) - ((listener.y as i32) >> 4)).abs();
    if dy < 1 || dx == 0 {
        return (near, near);
    }
    let r = dx as f64 / dy as f64;
    let far = ((a as f64 / (r.abs() + 1.0)) as i32).min(255) as f32;
    if r > 0.0 { (far, near) } else { (near, far) }
}

/// The rate actually played: Hz kept as the rate byte `Hz / 200`
/// (`AddSound` @ 100f4000, OS4 `FUN_0100815c`), so every sound plays in
/// 200 Hz steps.
pub fn quantise(hz: f32) -> f32 {
    (hz / 200.0).floor().max(0.0) * 200.0
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
    /// Held where it is (pause menu); neither advances nor sounds.
    paused: bool,
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

/// Frames rendered per lock of the voice table (about 3 ms): the grain at
/// which new voices, gains and pitch changes reach the output.
const BLOCK: usize = 64;

impl Iterator for MixerDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.buf.len() {
            self.buf.clear();
            self.buf.resize(BLOCK * 2, 0.0);
            self.at = 0;
            if let Ok(mut voices) = self.mixer.0.lock() {
                for v in voices.values_mut().filter(|v| !v.done && !v.paused) {
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
    // ONCE, not LOOP: the stream never ends, and bevy_audio loops a source
    // with rodio's `repeat_infinite`, which reads it through `buffered()` in
    // 32,768-sample spans (743 ms here). The mixer then ran only once per
    // span, so a new voice was heard 0–743 ms late and every sound
    // triggered within one span started in the same block, as one sound
    // (and the buffer kept every span ever rendered).
    commands.spawn((AudioPlayer(stream), PlaybackSettings::ONCE));
}

#[derive(Component)]
struct Started;

/// Distance to the listener last frame (Doppler).
#[derive(Component)]
struct LastDistance(f32);

/// A one-shot's slot position in source bytes (the Mac one-shot cut).
#[derive(Component)]
struct SlotClock(f64);

/// Whether the Mac one-shot cut is on (`Settings.variants.oneshot_cut`).
fn oneshot_cut(settings: Option<&crate::frontend::progress::Settings>) -> bool {
    settings.is_some_and(|s| s.variants.oneshot_cut)
}

/// The Mac slot clock, run once per game tick so the cut follows the
/// (recorded) ticks and replays sound the same (`DoAudio` @ 100f32f0, lines
/// 486–497; OS4 `FUN_01007290` the same): each frame of `t` ticks advances
/// a slot by `(735·t + 399)·200·rate/22050` source bytes, `rate` = Hz/200
/// without Doppler, and deletes a one-shot once that passes the sample
/// length (`DeleteSound` → `CMSQuietSound`). Real playback consumes only
/// `735·t` output samples a frame, so at the Mac's 30 fps (t = 1) one-shots
/// are silenced after 735/1134 ≈ 65 % of their length. Here a tick is one
/// such frame. Off by default (decision 2, docs/re/mac_version.md); loops
/// are unaffected, menu sounds have no slot.
fn slot_clock(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    time: Res<Time<Fixed>>,
    mut q: Query<(Entity, &Sound, Option<&mut SlotClock>)>,
) {
    let Some(sounds) = sounds else { return };
    if !oneshot_cut(settings.as_deref()) {
        return;
    }
    let per_hz = time.timestep().as_secs_f64() * (735.0 + 399.0) / 735.0;
    for (e, s, clock) in &mut q {
        if s.looping || s.menu {
            continue;
        }
        let step = quantise(s.hz) as f64 * per_hz;
        let pos = match clock {
            Some(mut c) => {
                c.0 += step;
                c.0
            }
            None => {
                commands.entity(e).insert(SlotClock(step));
                step
            }
        };
        if sounds.get(s.id).is_some_and(|pcm| pos > pcm.len() as f64) {
            commands.entity(e).despawn();
        }
    }
}

fn start_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    mixer: Res<Mixer>,
    fx: Res<FxVolume>,
    new: Query<(Entity, &Sound), Without<Started>>,
) {
    let Some(sounds) = sounds else { return };
    let Ok(mut voices) = mixer.0.lock() else { return };
    for (e, s) in &new {
        let pcm = sounds.get(s.id);
        // AddSound refuses one-shots at FX 0 (both builds).
        let refused = fx.0 == 0 && !s.looping && !s.menu;
        let Some(pcm) = pcm.filter(|_| !refused) else {
            commands.entity(e).despawn();
            continue;
        };
        // A menu sound starts at its fixed level; a game voice is held at
        // its first sample until `mix` gives it its level and pitch, so the
        // audio thread cannot play its start silently in between.
        let gain = if s.menu { menu_gain(fx.0) } else { 0.0 };
        voices.insert(
            e,
            Voice {
                pcm,
                pos: 0.0,
                step: s.hz as f64 / MIX_RATE as f64,
                left: gain,
                right: gain,
                looping: s.looping,
                done: false,
                paused: !s.menu,
            },
        );
        commands.entity(e).insert(Started);
    }
}

/// Doppler (`FUN_01007290`, Mac `DoAudio` lines 300–345): the rate byte
/// (Hz / 200) gains `(prevDist - dist) * 184 / (len + 400)` per 30 Hz frame
/// of `len + 400` = 1134 samples, clamped to 1..255.
fn doppler(hz: f32, approach_per_frame: f32) -> f32 {
    let rate = (hz / 200.0).floor() + (approach_per_frame * 184.0 / 1134.0).trunc();
    rate.clamp(1.0, 255.0) * 200.0
}

/// Per-side voice gain from a 0..255 level (Mac `DoAudio` lines 435–445:
/// `volumeCmd = FX · (L << 8) · 256 / 1e7`, 0x100 = unity), i.e.
/// `FX · L · 2.56e-5`: 0.653 at FX 100, L 255. The quiet-voice doubling the
/// Mac leaves uncompensated (+6 dB) is not reproduced (decision 3).
fn voice_gain(fx: u8, level: f32) -> f32 {
    fx as f32 * level.min(255.0) * 65536.0 / 1e7 / 256.0
}

/// A menu sound's level on both sides (`PlaySound` @ 100f1960:
/// `(FX << 8) / 100`, 0x100 = unity).
fn menu_gain(fx: u8) -> f32 {
    fx as f32 / 100.0
}

/// Apply the original mixer's voice selection, attenuation, pan and pitch.
#[allow(clippy::type_complexity)]
fn mix(
    mut commands: Commands,
    listener: Res<Listener>,
    others: Res<OtherListeners>,
    fx: Res<FxVolume>,
    time: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
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
    // The pause menu holds every game voice where it is (Mac `ShowOptions`
    // @ 10093240: `PauseAllSounds`, then `ResumeAllSounds` on leaving);
    // its own menu sounds still play.
    let paused = virtual_time.is_paused();
    let mut ranked: Vec<(f32, Vec3, Entity, &Sound, Option<Mut<LastDistance>>)> = Vec::new();
    for (e, s, last) in playing.iter_mut() {
        if s.menu {
            // Centred at `FX·256/100`, outside the voice selection.
            if let Some(v) = voices.get_mut(&e) {
                let gain = menu_gain(fx.0);
                v.left = gain;
                v.right = gain;
                v.step = s.hz as f64 / MIX_RATE as f64;
            }
            continue;
        }
        if let Some(v) = voices.get_mut(&e) {
            v.paused = paused;
        }
        if paused {
            continue;
        }
        let (a, l) = nearest(s);
        ranked.push((a, l, e, s, last));
    }
    if paused {
        return;
    }
    // Stable: earlier sounds win ties, as the original's strict `<` insertion.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Original mixer frames (30 Hz) since the last update.
    let frames = (time.delta_secs() * 30.0).max(1e-3);
    for (rank, (a, l, e, s, last)) in ranked.into_iter().enumerate() {
        let Some(v) = voices.get_mut(&e) else { continue };
        let audible = rank < MAX_VOICES && a > AUDIBLE;
        let (pl, pr) = if audible { pan(s, l) } else { (0.0, 0.0) };
        v.left = voice_gain(fx.0, pl);
        v.right = voice_gain(fx.0, pr);
        // The rate byte: 200 Hz steps, unclamped on the first frame, then
        // with Doppler clamped to 1..255.
        let mut hz = quantise(s.hz);
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
        let voice = |looping, left, right| Voice { pcm: pcm.clone(), pos: 0.0, step: 1.0, left, right, looping, done: false, paused: false };
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
        // `vol = d² · A` puts amplitude A at the listener.
        let at = |x: f32, y: f32, a: f32| {
            Sound::once(id::THUD, (x * x + y * y) * a, 15000.0, SoundPos::At(Vec3::new(x * 16.0, y * 16.0, 0.0)))
        };
        // r = -0.5 in floating point: right at 90 / 1.5.
        assert_eq!(pan(&at(-100.0, 200.0, 90.0), Vec3::ZERO), (90.0, 60.0));
        // Twice as far east as south: r = 2, left at A/3.
        assert_eq!(pan(&at(400.0, 200.0, 90.0), Vec3::ZERO), (30.0, 90.0));
        assert_eq!(pan(&at(-400.0, -200.0, 90.0), Vec3::ZERO), (90.0, 30.0));
        // A loud sound stays centred: the far side uses the unclamped A.
        assert_eq!(pan(&at(400.0, 200.0, 3000.0), Vec3::ZERO), (255.0, 255.0));
        // dy = 0 is centred (quirk of the original).
        assert_eq!(pan(&at(500.0, 0.0, 90.0), Vec3::ZERO), (90.0, 90.0));
        // Approaching 2 x 1134/184 units per frame raises the rate two steps.
        assert_eq!(doppler(15000.0, 0.0), 15000.0);
        assert_eq!(doppler(15000.0, 1134.0 / 184.0 * 2.0), 15400.0);
        assert_eq!(doppler(100.0, -1e6), 200.0);
    }

    #[test]
    fn rate_steps_and_levels() {
        // Beep1 at 4182 Hz plays at 4000 Hz.
        assert_eq!(quantise(4182.0), 4000.0);
        assert_eq!(quantise(22050.0), 22000.0);
        // Mac level law: 167/256 at FX 100, A 255; the Amiga's 1/3 at FX 50.
        assert!((voice_gain(100, 255.0) - 0.65280).abs() < 1e-4);
        assert!((voice_gain(50, 255.0) - 0.3264).abs() < 1e-4);
        assert_eq!(voice_gain(0, 255.0), 0.0);
    }

    fn voice(pcm: &Arc<[i8]>) -> Voice {
        Voice { pcm: pcm.clone(), pos: 0.0, step: 1.0, left: 1.0, right: 1.0, looping: false, done: false, paused: false }
    }

    /// A voice added while the stream runs is heard within one block, and
    /// each of several rapid triggers starts on its own (none merge or drop).
    #[test]
    fn new_voices_start_within_a_block() {
        let mixer = Mixer::default();
        let pcm: Arc<[i8]> = Arc::from(vec![16i8; 4000]);
        let mut d = MixerStream(mixer.clone()).decoder();
        // The stream is running (half-way through a block).
        let _: Vec<f32> = (0..BLOCK + 7).map(|_| d.next().unwrap()).collect();
        let mut onsets = Vec::new();
        let mut at = 0usize;
        let mut last = 0.0;
        for i in 1..=4 {
            mixer.0.lock().unwrap().insert(Entity::from_raw_u32(i).unwrap(), voice(&pcm));
            let inserted = at;
            // Every trigger raises the level within one block.
            for _ in 0..BLOCK * 2 * 2 {
                let s = d.next().unwrap();
                at += 1;
                if s > last + 0.1 {
                    onsets.push(at - inserted);
                    last = s;
                    break;
                }
            }
            let _: Vec<f32> = (0..BLOCK * 2 * 3).map(|_| d.next().unwrap()).collect();
            at += BLOCK * 2 * 3;
        }
        assert_eq!(onsets.len(), 4, "{onsets:?}");
        assert!(onsets.iter().all(|&n| n <= BLOCK * 2), "{onsets:?}");
        let v = mixer.0.lock().unwrap();
        let mut pos: Vec<f64> = v.values().map(|v| v.pos).collect();
        pos.sort_by(f64::total_cmp);
        pos.dedup();
        assert_eq!(pos.len(), 4);
    }

    /// Menu sounds start at their level; game voices wait for `mix`.
    #[test]
    fn start_levels() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let pcm: Arc<[i8]> = Arc::from(vec![64i8; 100]);
        world.insert_resource(Sounds { by_id: vec![Some(pcm)] });
        world.insert_resource(Mixer::default());
        world.insert_resource(FxVolume(50));
        let menu = world.spawn(Sound::menu(0, 15232.0)).id();
        let game = world.spawn(Sound::once(0, 255.0, 15000.0, SoundPos::Global)).id();
        world.run_system_once(start_sounds).unwrap();
        let mixer = world.resource::<Mixer>().0.lock().unwrap();
        assert_eq!((mixer[&menu].left, mixer[&menu].right, mixer[&menu].paused), (0.5, 0.5, false));
        assert!(mixer[&game].paused);
    }

    #[test]
    fn paused_voices_hold() {
        let mixer = Mixer::default();
        let pcm: Arc<[i8]> = Arc::from(vec![64i8; 100]);
        let e = Entity::from_raw_u32(1).unwrap();
        mixer.0.lock().unwrap().insert(
            e,
            Voice { pcm, pos: 0.0, step: 1.0, left: 1.0, right: 1.0, looping: false, done: false, paused: true },
        );
        let mut d = MixerStream(mixer.clone()).decoder();
        assert_eq!(d.next(), Some(0.0));
        let v = mixer.0.lock().unwrap();
        assert_eq!(v[&e].pos, 0.0);
    }
}
