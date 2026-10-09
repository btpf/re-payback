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
//! The Mac build (`amiga_startlevel` @ 100fff40, `amiga_levelloop` @
//! 10100f50) differs from OS4 in three ways, all followed here: the
//! level-start spawn draws from the cosmetic generator `random3`
//! ([`VisualRng`]) instead of the game's (rendering D4), last argument
//! first; lightning sets the flash to `100 + rand(150)` instead of adding
//! (D3); and the rain loop plays at 0x7f while the visible-layer limit is
//! not 3 (`SetHighestCube` @ 10106d90, script indoor scenes).
//!
//! All of it is single-player only: the level start (and the rain sound)
//! and the lightning are skipped when `DAT_0114c7c8 != 0`, so split-screen
//! games have no weather in any view. Sources: OS4 main loop `FUN_010692bc` (level start ~0x1069390
//! and the per-frame lightning after `FUN_010542b8`), effect updater
//! `FUN_010386e8`. `--weather <n>` overrides the header (and the level
//! restriction of rain); `--lightning` strikes 40 times as often, for checks.

use crate::audio::{Sound, SoundPos, id};
use crate::camera::GameCam;
use crate::effects::{Blend, Effects, ScreenFlash, Tex};
use crate::level::{AppState, GameMode, LevelEntity, LevelLoaded, LevelSpec};
use crate::mapedit::CityRender;
use crate::sim::{Rng, VisualRng};
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
            // Draws from the game Rng (a raindrop's y): pinned before the
            // people systems so the sequence never depends on schedule order.
            .add_systems(
                FixedUpdate,
                tick.before(crate::sim::people::PeopleThink)
                    .after(crate::sim::traffic::SpawnSet)
                    .before(crate::sim::contact::ContactSet)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, rain_volume.run_if(in_state(AppState::Playing)));
    }
}

/// The looping `Rain` sound (`DAT_1017c368`, `GetRainSound` @ 100679e0).
#[derive(Component)]
struct RainSound;

/// Rain volume by the visible-layer limit (`SetHighestCube` @ 10106d90:
/// `ModifySound(rain, vol 0xff)` when the limit is 3, else 0x7f), so the
/// rain is quieter indoors.
fn rain_volume(render: Option<Res<CityRender>>, mut q: Query<&mut Sound, With<RainSound>>) {
    let Some(render) = render else { return };
    let volume = if render.visible_layers == 3 { 255.0 } else { 127.0 };
    for mut sound in &mut q {
        if sound.volume != volume {
            sound.volume = volume;
        }
    }
}

/// Pick the weather from the map header: snow on any level, rain only on
/// level 0 (`DAT_0114c558 == 0`).
fn start(
    mut weather: ResMut<Weather>,
    city: Option<Res<City>>,
    spec: Res<LevelSpec>,
    mode: Res<GameMode>,
) {
    let forced = crate::arg("--weather").and_then(|v| v.parse::<u32>().ok());
    let header = city.map_or(0, |c| c.map.variant);
    let kind = match (forced, header) {
        _ if mode.is_multiplayer() => 0,
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
    visual: Option<ResMut<VisualRng>>,
    cam: Res<GameCam>,
) {
    let (Some(mut rng), Some(mut visual)) = (rng, visual) else { return };
    if weather.kind == 0 {
        return;
    }
    if !weather.started && cam.initialised {
        weather.started = true;
        spawn_particles(&mut commands, weather.kind, &cam, &mut effects, &mut rng, &mut visual);
    }
    if weather.kind != 2 {
        return;
    }
    // Lightning: with chance ticks/1000 a strike of 2, 4 or 6 ticks, each
    // even tick setting the palette flash to 100..249 (the Mac sets it,
    // `amiga_levelloop` @ 10100f50; OS4 added, so strikes built up). Drawn
    // from the game's generator, as in both builds.
    let chance = if crate::args().any(|a| a == "--lightning") {
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
            flash.0 = rng.rand(150) + 100;
        }
        weather.strike -= 1;
    }
}

/// The level-start spawn: 250 flakes or drops spread over a 16 x 10
/// square box around the camera and from the camera down to the ground
/// (z = 22500 − rand(25000)), all in effect list 3. Mac `amiga_startlevel`
/// @ 100fff40 draws from `random3`, last argument first (as the 68k; OS4
/// evaluates first to last); one draw, the y
/// of a raindrop, is the game's `random(0x5000)` there, so the rain spawn
/// still takes 250 numbers from the game's sequence (the snow takes none).
fn spawn_particles(
    commands: &mut Commands,
    kind: u32,
    cam: &GameCam,
    effects: &mut Effects,
    rng: &mut Rng,
    visual: &mut VisualRng,
) {
    let (cx, cy) = ((cam.x >> 5) as i32, (cam.y >> 5) as i32);
    for _ in 0..PARTICLES {
        let (kind, vel, size, y) = if kind == 1 {
            let size = visual.rand(50) + 100;
            let vz = visual.rand(100) + 200;
            let vy = visual.rand(20) - 10;
            let vx = visual.rand(20) + 10;
            let z = visual.rand(25000);
            let y = visual.rand(0x5000);
            (12, IVec3::new(vx, vy, vz), (size, z), y)
        } else {
            let vz = visual.rand(200) + 1000;
            let vy = visual.rand(20) - 10;
            let vx = visual.rand(30) + 50;
            let z = visual.rand(25000);
            let y = rng.rand(0x5000);
            (13, IVec3::new(vx, vy, vz), (1, z), y)
        };
        let (size, z) = size;
        let x = visual.rand(0x8000) + cx - 0x4000;
        let y = y + cy - 0x2800;
        let z = 0x57e4 - z;
        let pos = Vec3::new(x as f32, y as f32, z as f32);
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
            RainSound,
            LevelEntity,
        ));
    }
}
