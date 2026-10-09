//! Music: the game CD's Red Book audio tracks, from files the player rips
//! from the CD (neither release ships them as data: the Amiga plays the CD
//! with SCSI commands, OS4 "Payback CDDA Thread" `0x010087a0`; the Mac OS X
//! build plays the cddafs AIFF tracks as QuickTime movies, `UCDAudio::Play`
//! @ 10103fc0). Game-side logic follows the Mac (docs/re/mac/audio_platform.md
//! §2, docs/re/frontend_audio.md §4.6).
//!
//! **Files.** Put the tracks in a `Music` folder in the working directory,
//! the saves folder (`frontend::progress::saves_dir`, e.g.
//! `Saves\Music` next to the executable), the data root or the Mac data folder, one
//! file per CD track, numbered by CD track: a name starting with the number
//! (`13.ogg`, `13 - Payback.mp3`) or with the number last
//! (`Track13.flac`, `Payback (Track 13).wav`). Any format the decoder
//! reads: WAV, Ogg Vorbis, MP3, FLAC. With no files the music is Disabled
//! and everything stays silent; the CD Audio Setup screen then says where
//! the files go ([`NO_MUSIC_HINT`]) and the log lists the folders searched.
//! The disc images (`Payback.iso`, ISO 9660, and `PaybackMac.toast`, HFS)
//! hold only the data track, so the tracks cannot be extracted from them:
//! rip them from the CD (docs/status.md, the Audio row).
//!
//! **Modes** (`Settings.cd_mode`, `DAT_0114c30c` / Mac `DAT_10188fb0`):
//! 0 Off, 1 Radio (the Mac default), 2 Continuous, 3 Disabled (forced
//! while no tracks are found, as `DoCDAudioSetup` @ 100a1500 forces it when
//! `CD_Init` fails). Every request goes through [`Cd::check`], the Mac's
//! `CD_CheckForTrackEnd` @ 100fb000: track `n > 0` is (re)started if
//! nothing plays, so the menu tracks loop; `0` advances to the next track
//! when nothing plays; `-1` stops.
//!
//! - Menus request 13, Game Over 16, Level and Game Complete 8, the pause
//!   menu 0 (`ShowOptions` @ 10093240), all only in modes 1 and 2; see
//!   `frontend::ui::Screen::cd_track`. Level loading (`InitProgress`
//!   @ 10053510), the end of a level (`amiga_finishlevel` @ 10101ab0) and
//!   the intro stop the music.
//! - Continuous: in game, `amiga_levelloop` @ 10100f50 requests 0 every
//!   frame: the next track after the one that played last, from the first
//!   track; no random start and no wrap: it stops after the last track
//!   (`UCDAudio::Next`; decision 1, docs/re/mac_version.md). The Mac's
//!   every-other-track skip and `n + 1` catalogue index are cddafs
//!   plumbing and not reproduced.
//! - Radio: getting into a vehicle stops the music and plays its radio
//!   track (`GetInOutVehicles__Fi` @ 10066140, `CD_Stop` then `CD_Play(type
//!   +0x8194)` if > 0); getting out stops it. Each frame player 1's object
//!   requests its radio track (`do_Other__Fv` @ 10146280, `do_Person__Fv`
//!   @ 1013fbd0), so it loops; on foot that is -1 (stop; INFERRED: the
//!   person types' field is -1, the default of `+0x4938`).
//! - Radio in split screen (one CD for everyone): `GetInOutVehicles(p)`
//!   has no player test, so any player getting in switches the CD to their
//!   vehicle's station and any player getting out stops it; the per-frame
//!   request is made only for player 1's object (`objects[*DAT_1017c60c]`,
//!   the first entry of the per-player object table), so another player's
//!   station keeps playing only while player 1 drives, until the track
//!   ends (then player 1's loops), and stops at once while player 1 is on
//!   foot. CONFIRMED (Mac l.24524, 24644-24801, 115952, 117982).
//! - Volume is tied to the FX slider ([`FxVolume::music`]); the "under
//!   cover" duck is not heard on the Mac (`CD_SetVolume` @ 100fb100 is an
//!   empty stub), so there is none.

use crate::audio::FxVolume;
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::frontend::progress::Settings;
use crate::level::AppState;
use crate::sim::Obj;
use crate::vehicles::{TYPES, Vehicle};
use bevy::audio::{AudioSink, AudioSinkPlayback, AudioSource, PlaybackMode, Volume};
use bevy::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Music modes (`Settings.cd_mode`).
pub mod mode {
    pub const OFF: u8 = 0;
    pub const RADIO: u8 = 1;
    pub const CONTINUOUS: u8 = 2;
    pub const DISABLED: u8 = 3;
}

/// Set once the track folder has been scanned and holds a track.
static AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Whether any music track was found.
pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

/// The mode in force: Disabled while no tracks are found; a stored 3 (the
/// old default, or a CD that was missing) reads as Radio once there are.
pub fn effective_mode(stored: u8) -> u8 {
    if !available() {
        mode::DISABLED
    } else if stored >= mode::DISABLED {
        mode::RADIO
    } else {
        stored
    }
}

/// The CD setup menu's mode switch (`DoCDAudioSetup` @ 100a1500): Off →
/// Radio → Continuous → Off; Disabled stays.
pub fn next_mode(stored: u8) -> u8 {
    match effective_mode(stored) {
        mode::DISABLED => stored,
        m => (m + 1) % 3,
    }
}

/// What the front end asks of the music this frame.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicCue {
    /// No menu: in game the mode's own logic runs.
    #[default]
    None,
    /// `CD_CheckForTrackEnd(n)` from the screen on top (see [`Cd::check`]).
    Check(i32),
    /// Loading or the intro: `CD_Stop`.
    Stop,
}

/// The ripped tracks by CD track number.
#[derive(Resource, Default)]
pub struct MusicLibrary {
    tracks: BTreeMap<u32, PathBuf>,
    /// Decoded-format checked sources, loaded on first play.
    loaded: BTreeMap<u32, Option<Handle<AudioSource>>>,
}

/// Shown by the CD Audio Setup screen while no track is found (no
/// phrase; the original's setup just dims the mode when `CD_Init` fails).
pub const NO_MUSIC_HINT: [&str; 4] = [
    "No CD music found. Rip the game",
    "CD's audio tracks into a Music",
    "folder next to the game or the",
    "saves, named by track: 13.ogg ...",
];

/// The track folders, in search order.
fn folders(data: Option<&GameData>) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("Music"), crate::frontend::progress::saves_dir().join("Music")];
    if let Some(data) = data {
        dirs.push(data.root.join("Music"));
        if let Some(mac) = &data.mac {
            dirs.push(mac.join("Music"));
        }
    }
    dirs
}

/// The CD track number in a file name: its leading number, or else its
/// last number (`13 - Title`, `Track13`, `Payback (Track 13)`).
pub fn track_number(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_string_lossy();
    let lead: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    let digits = if !lead.is_empty() {
        lead
    } else {
        let rev: String = stem
            .chars()
            .rev()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        rev.chars().rev().collect()
    };
    digits.parse().ok().filter(|&n| (1..=99).contains(&n))
}

impl MusicLibrary {
    fn scan(data: Option<&GameData>) -> MusicLibrary {
        let mut tracks = BTreeMap::new();
        for dir in folders(data) {
            let Ok(entries) = crate::vfs::read_dir(&dir) else { continue };
            let mut files: Vec<PathBuf> = entries.flatten().filter(|e| e.is_file()).map(|e| e.path()).collect();
            files.sort();
            for f in files {
                if let Some(n) = track_number(&f) {
                    tracks.entry(n).or_insert(f);
                }
            }
        }
        if !tracks.is_empty() {
            info!("music: {} tracks ({:?})", tracks.len(), tracks.keys().collect::<Vec<_>>());
        } else {
            info!(
                "music: no CD tracks found; rip the game CD's audio tracks (e.g. 13.ogg, Track13.flac) into one of {:?}",
                folders(data)
            );
        }
        AVAILABLE.store(!tracks.is_empty(), Ordering::Relaxed);
        MusicLibrary { tracks, loaded: BTreeMap::new() }
    }

    /// The decodable source of track `n`, read on first use. A file the
    /// decoder rejects is skipped (Bevy would panic on it).
    fn source(&mut self, n: u32, sources: &mut Assets<AudioSource>) -> Option<Handle<AudioSource>> {
        if let Some(h) = self.loaded.get(&n) {
            return h.clone();
        }
        let path = self.tracks.get(&n)?;
        let handle = match crate::vfs::read(path) {
            Ok(bytes) => {
                let bytes: Arc<[u8]> = bytes.into();
                let ok = rodio::Decoder::builder()
                    .with_byte_len(bytes.len() as u64)
                    .with_data(std::io::Cursor::new(AudioSource { bytes: bytes.clone() }))
                    .build()
                    .map_err(|e| warn!("music: cannot decode {}: {e}", path.display()))
                    .is_ok();
                ok.then(|| sources.add(AudioSource { bytes }))
            }
            Err(e) => {
                warn!("music: reading {}: {e}", path.display());
                None
            }
        };
        self.loaded.insert(n, handle.clone());
        handle
    }

    /// The first track after `n` (`UCDAudio::Next`, no wrap).
    fn after(&self, n: u32) -> Option<u32> {
        self.tracks.range(n + 1..).next().map(|(&k, _)| k)
    }
}

/// The CD player state (`UCDAudio`).
#[derive(Resource, Default)]
pub struct Cd {
    /// The playing track's entity (despawned by Bevy when it ends).
    playing: Option<Entity>,
    /// The track played last; Continuous carries on after it.
    last: u32,
    /// Radio: the vehicle each player is in, by player number.
    radio_in: Vec<Option<Entity>>,
    /// A track was started this frame (its entity is not spawned yet).
    fresh: bool,
}

/// The `CD_*` calls, with what they need.
struct Player<'a, 'w, 's> {
    cd: &'a mut Cd,
    lib: &'a mut MusicLibrary,
    sources: &'a mut Assets<AudioSource>,
    commands: &'a mut Commands<'w, 's>,
    /// The track entity at the start of the frame still exists.
    alive: bool,
    volume: f32,
}

/// Marks the playing music entity.
#[derive(Component)]
pub struct MusicTrack;

impl Player<'_, '_, '_> {
    fn is_playing(&self) -> bool {
        self.cd.fresh || (self.cd.playing.is_some() && self.alive)
    }

    /// `CD_Stop` @ 100faf80.
    fn stop(&mut self) {
        if let Some(e) = self.cd.playing.take() {
            if self.alive || self.cd.fresh {
                self.commands.entity(e).despawn();
            }
        }
        self.cd.fresh = false;
        self.alive = false;
    }

    /// `CD_Play` @ 100faf00 / `UCDAudio::PlayTrack`: track `n` from the
    /// start. A track with no file plays nothing.
    fn play(&mut self, n: u32) {
        self.stop();
        self.cd.last = n;
        if let Some(h) = self.lib.source(n, self.sources) {
            let settings = PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::Linear(self.volume),
                ..default()
            };
            self.cd.playing = Some(self.commands.spawn((MusicTrack, AudioPlayer(h), settings)).id());
            self.cd.fresh = true;
        }
    }

    /// `CD_CheckForTrackEnd(n)` @ 100fb000: `n > 0` plays n if nothing
    /// plays; 0 plays the next track if nothing plays; -1 stops.
    fn check(&mut self, n: i32) {
        if n == -1 {
            self.stop();
        } else if !self.is_playing() {
            if n > 0 {
                self.play(n as u32);
            } else if let Some(next) = self.lib.after(self.cd.last) {
                self.play(next);
            } else {
                // Past the last track: stays silent (no wrap).
                self.cd.playing = None;
            }
        }
    }
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicCue>()
            .init_resource::<Cd>()
            .init_resource::<MusicLibrary>()
            .add_systems(Startup, |mut commands: Commands, data: Option<Res<GameData>>| {
                commands.insert_resource(MusicLibrary::scan(data.as_deref()));
            })
            .add_systems(OnExit(AppState::Playing), stop_on_exit)
            .add_systems(Update, run_music.after(crate::frontend::FrontendRun));
    }
}

/// `amiga_finishlevel` @ 10101ab0: `StopAllSounds`, then `CD_Stop`.
fn stop_on_exit(mut commands: Commands, mut cd: ResMut<Cd>, alive: Query<(), With<MusicTrack>>) {
    if let Some(e) = cd.playing.take().filter(|&e| alive.contains(e)) {
        commands.entity(e).despawn();
    }
    cd.radio_in.clear();
}

#[allow(clippy::too_many_arguments)]
fn run_music(
    mut commands: Commands,
    mut cd: ResMut<Cd>,
    mut lib: ResMut<MusicLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    cue: Res<MusicCue>,
    settings: Res<Settings>,
    fx: Res<FxVolume>,
    state: Res<State<AppState>>,
    virtual_time: Res<Time<Virtual>>,
    alive: Query<(), With<MusicTrack>>,
    mut sinks: Query<&mut AudioSink, With<MusicTrack>>,
    players: Query<(Entity, &Obj, Option<&Vehicle>), With<CameraTarget>>,
) {
    let volume = fx.music();
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(volume));
    }
    let m = effective_mode(settings.cd_mode);
    cd.fresh = false;
    let cd_alive = cd.playing.is_some_and(|e| alive.contains(e));
    let mut p = Player {
        cd: &mut cd,
        lib: &mut lib,
        sources: &mut sources,
        commands: &mut commands,
        alive: cd_alive,
        volume,
    };
    if m == mode::DISABLED || m == mode::OFF {
        // Switching to Off stops the CD (`DoCDAudioSetup`); every other
        // request is skipped in mode 0.
        p.stop();
        p.cd.radio_in.clear();
        return;
    }
    match *cue {
        MusicCue::Stop => p.stop(),
        MusicCue::Check(n) => p.check(n),
        MusicCue::None => {
            if *state.get() != AppState::Playing || virtual_time.is_paused() {
                return;
            }
            if m == mode::CONTINUOUS {
                p.check(0);
                return;
            }
            // Radio: every player's getting in or out (in player order),
            // then player 1's object's request.
            let n = players.iter().filter_map(|(_, o, _)| o.player).max().map_or(1, |m| m as usize + 1);
            p.cd.radio_in.resize(n.max(1), None);
            let mut me = None;
            for i in 0..n.max(1) {
                let obj = players
                    .iter()
                    .find(|(_, o, _)| o.player == Some(i as u8))
                    .or_else(|| (i == 0).then(|| players.iter().next()).flatten());
                let vehicle = obj.and_then(|(e, _, v)| v.map(|v| (e, v.kind)));
                if i == 0 {
                    me = obj.map(|_| vehicle);
                }
                // Another player's object gone (dead): nothing changes;
                // player 1's: as getting out (the music stops).
                if obj.is_none() && i > 0 {
                    continue;
                }
                let now = vehicle.map(|v| v.0);
                if now != p.cd.radio_in[i] {
                    p.cd.radio_in[i] = now;
                    p.stop();
                    if let Some(n) = vehicle.and_then(|v| TYPES[v.1].radio).filter(|&n| n > 0) {
                        p.play(n as u32);
                    }
                }
            }
            if let Some(vehicle) = me {
                let track = vehicle.and_then(|v| TYPES[v.1].radio).map_or(-1, i32::from);
                p.check(track);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_numbers() {
        let n = |s: &str| track_number(Path::new(s));
        assert_eq!(n("Music/13.ogg"), Some(13));
        assert_eq!(n("13 - Payback.mp3"), Some(13));
        assert_eq!(n("02 Track 2.flac"), Some(2));
        assert_eq!(n("Track13.wav"), Some(13));
        assert_eq!(n("Payback (Track 08).mp3"), Some(8));
        assert_eq!(n("cover.jpg"), None);
        assert_eq!(n("0.wav"), None);
    }

    #[test]
    fn music_volume_follows_the_slider() {
        assert_eq!(FxVolume(0).music(), 200.0 / 256.0);
        assert_eq!(FxVolume(50).music(), 200.0 / 256.0);
        assert_eq!(FxVolume(70).music(), 120.0 / 256.0);
        assert_eq!(FxVolume(100).music(), 0.0);
    }
}
