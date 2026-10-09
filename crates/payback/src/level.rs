//! Loading a city: simulation geometry, render meshes and scenery.

use crate::city::{self, Pass};
use crate::data::GameData;
use crate::mapedit::{self, CityRender};
use crate::objects::{self, ObjectLibrary, SceneryIndex};
use crate::water::{self, Water, WaterMaterial};
use crate::world::{City, SQUARE, z_of_h};
use bevy::{ecs::world::CommandQueue, prelude::*};
use payback_formats::{Map, map::MAP_SIZE};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which map to load.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct LevelSpec {
    /// A built-in city number (`Maps/<n>.map`), or the path of a user map
    /// without its `.map` extension (`UserMaps/<name>`, relative to the
    /// game data or absolute; `FUN_01009ff8` hands out the same kind of
    /// path for cities >= 3).
    pub map: String,
    /// Level within the city (0 or 1), selecting the start square.
    pub level: u8,
}

impl LevelSpec {
    /// A user map's spec from the path of its `.map` file.
    pub fn user_map(map_file: &Path) -> LevelSpec {
        LevelSpec {
            map: map_file.with_extension("").to_string_lossy().replace('\\', "/"),
            level: 0,
        }
    }

    /// A user map (the name is a path).
    pub fn is_user_map(&self) -> bool {
        self.map.contains(['/', '\\'])
    }

    /// The built-in city number, if this is one.
    pub fn city(&self) -> Option<u8> {
        self.map.parse().ok().filter(|_| !self.is_user_map())
    }

    /// A map file next to the `.map`: `Maps/<n>.<ext>` or
    /// `<user map>.<ext>` (read through [`GameData::read`], which takes
    /// absolute paths as they are).
    pub fn path(&self, ext: &str) -> PathBuf {
        if self.is_user_map() {
            PathBuf::from(format!("{}.{ext}", self.map))
        } else {
            PathBuf::from(format!("Maps/{}.{ext}", self.map))
        }
    }
}

/// How the level is played: a story level with its mission script, or
/// single-player Rampage (`DAT_0114c7c6 = 1`, set by the game menu's
/// "Single player rampage", `FUN_0102a428`): no level script, police
/// aggressiveness from the setup screen (`DAT_0114c2ec`), score kept for
/// the high-score table.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    Story,
    Rampage {
        /// Police aggressiveness 0..5 (phrases 2491..2496).
        police: u8,
    },
    /// Split-screen deathmatch (`DAT_0114c7c8 = 1`, set by the
    /// multiplayer setup `FUN_0102b2c8`): 2..4 players (`DAT_0114c2e4`),
    /// first to `kills` (`DAT_0114c2e8`) wins; no level script, no lives,
    /// no crimes or route police (docs/re/combat_police.md §7).
    Multiplayer { players: u8, kills: u8 },
}

impl GameMode {
    pub fn is_rampage(&self) -> bool {
        matches!(self, GameMode::Rampage { .. })
    }

    pub fn is_multiplayer(&self) -> bool {
        matches!(self, GameMode::Multiplayer { .. })
    }

    /// Players in the game (1 outside multiplayer).
    pub fn players(&self) -> usize {
        match self {
            GameMode::Multiplayer { players, .. } => (*players as usize).clamp(1, 4),
            _ => 1,
        }
    }
}

/// Run condition: not a multiplayer game (crimes, police, scripts).
pub fn single_player(mode: Res<GameMode>) -> bool {
    !mode.is_multiplayer()
}

/// Start positions (original units: x, y, z-down) from squares with no
/// scenery and flags exactly 0x20, per LoadMap (OS4 `FUN_01079300`):
/// special 1 squares form a list of up to 100 (multiplayer starts,
/// INFERRED); the square whose special is `level + 2` is stored separately
/// (single-player start, INFERRED).
#[derive(Resource, Default)]
pub struct StartPoints {
    pub list: Vec<Vec3>,
    pub level_start: Option<Vec3>,
}

/// Everything spawned for the current level, for despawning on unload.
#[derive(Component)]
pub struct LevelEntity;

/// Top-level game state.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Menus (title, game menu, level select, ...).
    #[default]
    Frontend,
    /// A level is loaded and running.
    Playing,
}

/// Runs on entering `Playing` once the level is loaded; later setup
/// orders itself after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LevelLoaded;

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<GameMode>()
            .add_plugins(crate::lighting::LightingPlugin)
            .add_systems(OnEnter(AppState::Playing), load_level.before(LevelLoaded))
            .add_systems(Update, advance_loading.run_if(in_state(AppState::Frontend)))
            .add_systems(OnExit(AppState::Playing), unload_level);
    }
}

/// Despawn the level and drop its resources; per-level game state is reset
/// by the plugins that own it (`OnExit(AppState::Playing)`).
fn unload_level(mut commands: Commands, entities: Query<Entity, With<LevelEntity>>) {
    for e in &entities {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<City>();
    commands.remove_resource::<CityRender>();
    commands.remove_resource::<StartPoints>();
    commands.remove_resource::<Water>();
    commands.remove_resource::<ObjectLibrary>();
}

/// A level load in progress, one stage per frame so the front end's
/// loading panel can fill its bar (LoadMap OS4 `FUN_01079300` between
/// InitProgress("map.size") and CloseProgress, see [`Progress`]).
///
/// The stages build everything off to the side: entities are spawned
/// into a private command queue and the resources are kept here, so
/// nothing of the level exists in the world until [`load_level`] applies
/// it on entering [`AppState::Playing`]. A level started directly
/// (`--map`, `--play`) runs every stage there at once, with the same
/// result.
#[derive(Resource)]
pub struct LevelLoad {
    pub spec: LevelSpec,
    stage: usize,
    queue: CommandQueue,
    map: Option<Map>,
    city: Option<City>,
    render: Option<CityRender>,
    water: Option<Handle<WaterMaterial>>,
    library: Option<ObjectLibrary>,
    starts: Option<StartPoints>,
    /// The loading bar; `None` when nothing shows it.
    pub progress: Option<Progress>,
    /// Debug (`--load-delay <ms>`): idle time between stages.
    delay: std::time::Duration,
    wait_until: Option<bevy::platform::time::Instant>,
}

/// Loading stages, in order (see [`LevelLoad::run_stage`]).
const STAGES: usize = 7;

impl LevelLoad {
    /// Start loading `spec`; `bar` = drive a loading bar (InitProgress
    /// with `map.size`).
    pub fn new(data: &GameData, spec: LevelSpec, bar: bool) -> LevelLoad {
        let ms = if bar {
            crate::arg("--load-delay").and_then(|v| v.parse().ok()).unwrap_or(0)
        } else {
            0
        };
        let delay = std::time::Duration::from_millis(ms);
        LevelLoad {
            spec,
            stage: 0,
            queue: default(),
            map: None,
            city: None,
            render: None,
            water: None,
            library: None,
            starts: None,
            progress: bar.then(|| Progress::init(data, "map.size")),
            delay,
            wait_until: None,
        }
    }

    pub fn done(&self) -> bool {
        self.stage >= STAGES
    }

    /// Run the next stage (unless idling for `--load-delay`); the time it
    /// took, in microseconds, goes to the bar (DrawProgress).
    fn step(&mut self, world: &mut World) {
        if self.done() {
            return;
        }
        let now = bevy::platform::time::Instant::now();
        if self.wait_until.is_some_and(|t| now < t) {
            return;
        }
        self.wait_until = Some(now + self.delay);
        let t = bevy::platform::time::Instant::now();
        self.run_stage(world);
        self.stage += 1;
        if let Some(p) = &mut self.progress {
            p.draw(t.elapsed().as_micros() as u64);
        }
    }

    fn run_stage(&mut self, world: &mut World) {
        let data = world.resource::<GameData>().clone();
        match self.stage {
            // Read the map.
            0 => {
                // The cube texture set (`Variants::textures`) and a user
                // map's `<map>.textures` (`FUN_0106e8ec`), for this level.
                let mac_textures = world
                    .get_resource::<crate::frontend::progress::Settings>()
                    .is_none_or(|s| s.variants.textures);
                let custom = if self.spec.is_user_map() {
                    data.read(self.spec.path("textures"))
                        .ok()
                        .and_then(|b| {
                            payback_formats::CustomTextures::parse(&b)
                                .inspect_err(|e| warn!("{}: {e}", self.spec.path("textures").display()))
                                .ok()
                        })
                        .unwrap_or_default()
                } else {
                    Default::default()
                };
                {
                    let mut d = world.resource_mut::<GameData>();
                    d.mac_cube_textures = mac_textures;
                    d.custom_textures = std::sync::Arc::new(custom);
                }
                let path = self.spec.path("map");
                match data
                    .read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| Map::parse(&b).map_err(|e| e.to_string()))
                {
                    Ok(m) => {
                        info!(
                            "loaded {} (night: {}, weather: {})",
                            path.display(),
                            m.night,
                            m.variant
                        );
                        self.map = Some(m);
                    }
                    Err(e) => {
                        error!("{}: {e}", path.display());
                        // Nothing to build: the level stays empty.
                        self.stage = STAGES;
                    }
                }
            }
            // Cube texture atlases and the city materials.
            1 => {
                // Night maps are darkened in the atlas only in software
                // (`lighting::sync_mode`), so the atlas is built once for
                // the renderer in use.
                let night = self.map.as_ref().is_some_and(|m| m.night);
                let atlases = city::build_cube_atlases(&data, night && !crate::render_mode::is_hardware());
                let (opaque, keyed, indices, sky) = {
                    let mut images = world.resource_mut::<Assets<Image>>();
                    (
                        images.add(atlases.opaque),
                        images.add(atlases.keyed),
                        images.add(atlases.indices),
                        images.add(water::load_sky(&data, night)),
                    )
                };
                let water = world.resource_mut::<Assets<WaterMaterial>>().add(WaterMaterial {
                    params: Vec4::ZERO,
                    index_atlas: indices,
                    sky,
                });
                let names: Vec<String> = data
                    .cube_textures
                    .entries
                    .iter()
                    .map(|e| e.file.clone())
                    .collect();
                let mut pass_materials = HashMap::new();
                let mut materials = world.resource_mut::<Assets<crate::lighting::CityMaterial>>();
                for pass in [Pass::Opaque, Pass::Masked, Pass::Alpha] {
                    let material = crate::lighting::CityMaterial {
                        atlas: if pass == Pass::Masked {
                            keyed.clone()
                        } else {
                            opaque.clone()
                        },
                        lights: default(),
                        pass,
                    };
                    pass_materials.insert(pass, materials.add(material));
                }
                self.render = Some(CityRender {
                    names,
                    materials: pass_materials,
                    water: water.clone(),
                    chunks: HashMap::new(),
                    visible_layers: 3,
                    lighting: None,
                    remap: None,
                });
                self.water = Some(water);
            }
            // Cube geometry and Gouraud lighting.
            2 => {
                let (Some(map), Some(render)) = (self.map.take(), self.render.as_mut()) else {
                    return;
                };
                let city = City::new(map);
                let gouraud = world.resource::<crate::lighting::Gouraud>().0;
                crate::lighting::set_lighting(render, &data, &city, gouraud);
                self.city = Some(city);
            }
            // City meshes.
            3 => {
                let (Some(city), Some(render)) = (self.city.as_ref(), self.render.as_mut()) else {
                    return;
                };
                let queue = &mut self.queue;
                world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    let mut commands = Commands::new(queue, world);
                    mapedit::spawn_all_chunks(&mut commands, render, &mut meshes, city);
                });
            }
            // Object textures and meshes.
            4 => {
                let night = self.city.as_ref().is_some_and(|c| c.map.night);
                world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
                    let mut materials = world.resource_mut::<Assets<objects::ObjectMaterial>>();
                    self.library = Some(ObjectLibrary::new(&data, &mut images, &mut materials, night));
                });
            }
            // Scenery objects.
            5 => {
                let (Some(city), Some(library)) = (self.city.as_ref(), self.library.as_mut()) else {
                    return;
                };
                let queue = &mut self.queue;
                world.resource_scope(|world, mut scenery: Mut<SceneryIndex>| {
                    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                        let mut commands = Commands::new(queue, world);
                        objects::spawn_scenery(&mut commands, &data, library, &mut meshes, city, &mut scenery);
                    });
                });
            }
            // Start points.
            _ => {
                if let Some(city) = &self.city {
                    self.starts = Some(find_start_points(city, self.spec.level));
                }
            }
        }
    }

    /// Run what is left, put the level into the world and CloseProgress.
    fn finish(mut self, world: &mut World) {
        self.delay = default();
        while !self.done() {
            self.wait_until = None;
            self.step(world);
        }
        if let Some(p) = &self.progress {
            p.close();
        }
        self.queue.apply(world);
        if let Some(w) = self.water {
            world.insert_resource(Water(w));
        }
        if let Some(r) = self.render {
            world.insert_resource(r);
        }
        if let Some(l) = self.library {
            world.insert_resource(l);
        }
        if let Some(s) = self.starts {
            world.insert_resource(s);
        }
        if let Some(c) = self.city {
            world.insert_resource(c);
        }
    }
}

/// Advance a level load started from the menus by one stage per frame.
fn advance_loading(world: &mut World) {
    let Some(mut load) = world.remove_resource::<LevelLoad>() else {
        return;
    };
    load.step(world);
    world.insert_resource(load);
}

/// Load the level on entering [`AppState::Playing`]: finish the load the
/// front end started for this spec, or do all of it now.
pub fn load_level(world: &mut World) {
    let spec = world.resource::<LevelSpec>().clone();
    let load = match world.remove_resource::<LevelLoad>() {
        Some(l) if l.spec == spec => l,
        _ => LevelLoad::new(world.resource::<GameData>(), spec, false),
    };
    load.finish(world);
}

/// Pieces in the loading bar.
pub const PROGRESS_PIECES: u64 = 12;

/// The loading bar's bookkeeping (docs/re/frontend_audio.md §6.3):
/// InitProgress `FUN_01042ff8` reads the units per bar piece from
/// `General/<name>` (one big-endian u32), DrawProgress `FUN_010434a0` →
/// `FUN_01042688` adds work done and fills `min(12, acc / unit)` pieces,
/// and CloseProgress `FUN_010434d4` writes `acc / 12` back when it
/// changed ("Wrote updated file sizes"), so the bar calibrates itself.
///
/// Here the work units are microseconds spent loading (the original's are
/// counts such as 50000 per texture), and the calibrated value goes to
/// `Saves/<name>`, which is read first, so the game data stays untouched.
#[derive(Clone, Debug)]
pub struct Progress {
    name: &'static str,
    /// Units per piece.
    unit: u64,
    acc: u64,
}

impl Progress {
    pub fn init(data: &GameData, name: &'static str) -> Progress {
        let saved = crate::frontend::progress::read_file(&crate::frontend::progress::saves_dir().join(name)).ok();
        let unit = saved
            .and_then(|b| be_u32(&b))
            .or_else(|| data.read(format!("General/{name}")).ok().and_then(|b| be_u32(&b)))
            .unwrap_or(0x12179);
        Progress {
            name,
            unit: (unit as u64).max(1),
            acc: 0,
        }
    }

    pub fn draw(&mut self, n: u64) {
        self.acc += n;
    }

    /// Filled pieces, 0..=12.
    pub fn pieces(&self) -> usize {
        (self.acc / self.unit).min(PROGRESS_PIECES) as usize
    }

    pub fn close(&self) {
        let unit = (self.acc / PROGRESS_PIECES).clamp(1, u32::MAX as u64);
        if unit == self.unit {
            return;
        }
        let dir = crate::frontend::progress::saves_dir();
        let res = crate::frontend::progress::write_file(&dir.join(self.name), &(unit as u32).to_be_bytes());
        match res {
            Ok(()) => info!("Wrote updated file sizes ({} = {unit})", self.name),
            Err(e) => warn!("{}: {e}", self.name),
        }
    }
}

fn be_u32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

fn find_start_points(city: &City, level: u8) -> StartPoints {
    let mut points = StartPoints::default();
    for x in 0..MAP_SIZE {
        for y in 0..MAP_SIZE {
            let sq = city.map.square(x, y);
            for (i, cube) in sq.cubes.iter().enumerate() {
                if cube.scenery != 0 || cube.flags != 0x20 {
                    continue;
                }
                let geom = city.square_cubes(x, y)[i];
                let pos = Vec3::new(
                    (x as f32 + 0.5) * SQUARE,
                    (y as f32 + 0.5) * SQUARE,
                    z_of_h(geom.top_at(0.5, 0.5)),
                );
                if cube.special == 1 && points.list.len() < 100 {
                    points.list.push(pos);
                } else if cube.special as u32 == level as u32 + 2 {
                    points.level_start = Some(pos);
                }
            }
        }
    }
    points
}
