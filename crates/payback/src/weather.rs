//! Weather: the map header `variant` (`DAT_0114c854`, read through
//! `FUN_0106e478`; docs/re/rendering.md §4).
//!
//! - 1 (map 1): snow. 250 flakes (effect type 12, `Dust`, additive) are
//!   spawned around the camera when the level starts; they drift and fall,
//!   wrap around the view and, on reaching the ground, leave a mark and
//!   restart at the camera (the effect updater in `crate::effects`).
//! - 2 (map 2, level 0 only): rain and lightning. 250 drops (type 13,
//!   1-pixel streaks), splashes on landing (type 14), a looping `Rain`
//!   sound, and random lightning that flashes the palette with `Lightning`
//!   thunder.
//!
//! All of it is single-player only (`DAT_0114c7c8 == 0`), which is the only
//! mode here. Sources: OS4 main loop `FUN_010692bc` (level start ~0x1069390
//! and the per-frame lightning after `FUN_010542b8`), effect updater
//! `FUN_010386e8`. `--weather <n>` overrides the header (and the level
//! restriction of rain); `--lightning` strikes 40 times as often, for checks.

use crate::audio::{Sound, SoundPos, id};
use crate::camera::GameCam;
use crate::effects::{Blend, Effects, ScreenFlash, Tex};
use crate::level::{AppState, LevelEntity, LevelLoaded, LevelSpec};
use crate::sim::Rng;
use crate::world::City;
use bevy::prelude::*;

/// Particles spawned at level start.
const PARTICLES: usize = 250;
/// Sound id of `Lightning` (load order, `FUN_01005864`).
const LIGHTNING: u16 = 62;

/// What the level's weather is doing.
#[derive(Resource, Default, Debug)]
pub struct Weather {
    /// 0 none, 1 snow, 2 rain with lightning.
    pub kind: u32,
    /// Particles spawned (waits for the camera to be placed).
    started: bool,
    /// Lightning countdown (`DAT_0114c58c`): 2, 4 or 6 ticks; the palette
    /// flashes on even values.
    strike: i32,
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .add_systems(OnEnter(AppState::Playing), start.in_set(LevelLoaded))
            .add_systems(OnExit(AppState::Playing), |mut w: ResMut<Weather>| {
                *w = Weather::default()
            })
            .add_systems(FixedUpdate, tick.run_if(in_state(AppState::Playing)));
    }
}

/// Pick the weather from the map header: snow on any level, rain only on
/// level 0 (`DAT_0114c558 == 0`).
fn start(mut weather: ResMut<Weather>, city: Option<Res<City>>, spec: Res<LevelSpec>) {
    let forced = crate::arg("--weather").and_then(|v| v.parse::<u32>().ok());
    let header = city.map_or(0, |c| c.map.variant);
    let kind = match (forced, header) {
        (Some(k), _) => k,
        (None, 2) if spec.level != 0 => 0,
        (None, k) => k,
    };
    *weather = Weather { kind, ..default() };
    if kind != 0 {
        info!("weather {kind}");
    }
}

/// Once per game tick: the initial spawn, then lightning.
fn tick(
    mut commands: Commands,
    mut weather: ResMut<Weather>,
    mut effects: ResMut<Effects>,
    mut flash: ResMut<ScreenFlash>,
    rng: Option<ResMut<Rng>>,
    cam: Res<GameCam>,
) {
    let Some(mut rng) = rng else { return };
    if weather.kind == 0 {
        return;
    }
    if !weather.started && cam.initialised {
        weather.started = true;
        spawn_particles(&mut commands, weather.kind, &cam, &mut effects, &mut rng);
    }
    if weather.kind != 2 {
        return;
    }
    // Lightning: with chance ticks/1000 a strike of 2, 4 or 6 ticks, each
    // even tick adding 100..249 to the palette flash.
    let chance = if std::env::args().any(|a| a == "--lightning") {
        40
    } else {
        1
    };
    if weather.strike == 0 && rng.rand(1000) < chance {
        let n = (rng.rand(3) + 1) * 2;
        weather.strike = n;
        let (volume, hz) = match n {
            4 => (255.0, 6500),
            2 => (240.0, 7500),
            _ => (160.0, 8000),
        };
        let hz = (hz + rng.rand(500)) as f32;
        commands.spawn((
            Sound::once(LIGHTNING, volume, hz, SoundPos::Global),
            LevelEntity,
        ));
    }
    if weather.strike > 0 {
        if weather.strike % 2 == 0 {
            flash.0 += rng.rand(150) + 100;
        }
        weather.strike -= 1;
    }
}

/// The level-start spawn: 250 flakes or drops spread over a 16 x 10
/// square box around the camera and from the camera down to the ground
/// (z = 22500 − rand(25000)), all in effect list 3.
fn spawn_particles(
    commands: &mut Commands,
    kind: u32,
    cam: &GameCam,
    effects: &mut Effects,
    rng: &mut Rng,
) {
    let (cx, cy) = ((cam.x >> 5) as i32, (cam.y >> 5) as i32);
    for _ in 0..PARTICLES {
        let x = rng.rand(0x8000) + cx - 0x4000;
        let y = rng.rand(0x5000) + cy - 0x2800;
        let z = 0x57e4 - rng.rand(25000);
        let pos = Vec3::new(x as f32, y as f32, z as f32);
        let (kind, vel, size) = if kind == 1 {
            let vx = rng.rand(20) + 10;
            let vy = rng.rand(20) - 10;
            let vz = rng.rand(100) + 200;
            (12, IVec3::new(vx, vy, vz), rng.rand(50) + 100)
        } else {
            let vx = rng.rand(30) + 50;
            let vy = rng.rand(20) - 10;
            let vz = rng.rand(200) + 1000;
            (13, IVec3::new(vx, vy, vz), 1)
        };
        effects.add(
            kind,
            Tex::Dust,
            pos,
            vel.as_vec3(),
            size as f32,
            3,
            Blend::Add,
        );
    }
    if kind == 2 {
        commands.spawn((
            Sound::looping(id::RAIN, 255.0, 14000.0, SoundPos::Global),
            LevelEntity,
        ));
    }
}
