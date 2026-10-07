//! Sound effects, reimplementing the original software mixer's behaviour
//! (docs/re/frontend_audio.md, OS4 `FUN_01007290` / `FUN_0100815c`).
//!
//! The original mixes at 22050 Hz; every AddSound call gives the playback
//! rate in Hz and a volume. Positional sounds are heard with amplitude
//! `A = vol / d²` (d in world units >> 4), clamped to 255, audible when
//! `A > 19`; at most 6 voices play at once. Samples are headerless signed
//! 8-bit PCM, wrapped here as WAVs at 22050 Hz and sped up or down to the
//! requested rate. Not yet reproduced: stereo panning and Doppler.

use crate::data::GameData;
use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};

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

#[derive(Resource, Default)]
pub struct Sounds {
    by_id: Vec<Option<Handle<AudioSource>>>,
}

impl Sounds {
    pub fn get(&self, id: u16) -> Option<Handle<AudioSource>> {
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
        app.init_resource::<Listener>()
            .init_resource::<OtherListeners>()
            .init_resource::<FxVolume>()
            .add_systems(Startup, load_sounds)
            .add_systems(PostUpdate, (start_sounds, mix).chain());
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
    for name in SOUND_FILES {
        let handle = data.read(format!("Sounds/{name}")).ok().map(|bytes| {
            sources.add(AudioSource {
                bytes: wav_from_pcm8(&bytes, MIX_RATE).into(),
            })
        });
        if handle.is_none() {
            warn!("missing sound {name}");
        }
        sounds.by_id.push(handle);
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

#[derive(Component)]
struct Started;

fn start_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    new: Query<(Entity, &Sound), Without<Started>>,
) {
    let Some(sounds) = sounds else { return };
    for (e, s) in &new {
        let Some(source) = sounds.get(s.id) else {
            commands.entity(e).despawn();
            continue;
        };
        let settings = if s.looping {
            PlaybackSettings::LOOP
        } else {
            PlaybackSettings::DESPAWN
        };
        commands.entity(e).insert((
            AudioPlayer(source),
            settings
                .with_speed(s.hz / MIX_RATE as f32)
                .with_volume(Volume::Linear(0.0)),
            Started,
        ));
    }
}

/// Apply the original mixer's voice selection, attenuation and pitch.
fn mix(
    listener: Res<Listener>,
    others: Res<OtherListeners>,
    fx: Res<FxVolume>,
    mut playing: Query<(&Sound, &mut AudioSink)>,
) {
    let loudest = |s: &Sound| {
        others.0.iter().fold(amplitude(s, listener.0), |a, &l| a.max(amplitude(s, l)))
    };
    let mut voices: Vec<(f32, Mut<AudioSink>, &Sound)> = playing
        .iter_mut()
        .map(|(s, sink)| (loudest(s), sink, s))
        .collect();
    voices.sort_by(|a, b| b.0.total_cmp(&a.0));
    let master = fx.0 as f32 / 100.0;
    for (rank, (a, sink, s)) in voices.iter_mut().enumerate() {
        let audible = rank < MAX_VOICES && *a > AUDIBLE;
        // Original output: three voices at full amplitude sum to full scale.
        let gain = if audible {
            *a / 255.0 / 3.0 * master
        } else {
            0.0
        };
        sink.set_volume(Volume::Linear(gain));
        sink.set_speed((s.hz / MIX_RATE as f32).clamp(0.01, 10.0));
    }
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
}
