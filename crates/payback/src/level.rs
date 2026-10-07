//! Loading a city: simulation geometry, render meshes and scenery.

use crate::city::{self, Pass};
use crate::data::GameData;
use crate::mapedit::{self, CityRender};
use crate::objects::{self, ObjectLibrary, SceneryIndex};
use crate::water::{self, Water, WaterMaterial};
use crate::world::{City, SQUARE, z_of_h};
use bevy::prelude::*;
use payback_formats::{Map, map::MAP_SIZE};
use std::collections::HashMap;

/// Which map to load (`Maps/<n>.map`).
#[derive(Resource, Clone)]
pub struct LevelSpec {
    pub map: String,
    /// Level within the city (0 or 1), selecting the start square.
    pub level: u8,
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
            .add_systems(OnEnter(AppState::Playing), load_level.before(LevelLoaded))
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

#[allow(clippy::too_many_arguments)]
pub fn load_level(
    mut commands: Commands,
    data: Res<GameData>,
    spec: Res<LevelSpec>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut object_materials: ResMut<Assets<objects::ObjectMaterial>>,
    mut scenery: ResMut<SceneryIndex>,
) {
    let path = format!("Maps/{}.map", spec.map);
    let map = match data
        .read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| Map::parse(&b).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            error!("{path}: {e}");
            return;
        }
    };
    info!(
        "loaded {path} (night: {}, weather: {})",
        map.night, map.variant
    );

    let atlases = city::build_cube_atlases(&data, map.night);
    let opaque = images.add(atlases.opaque);
    let keyed = images.add(atlases.keyed);
    let water = water_materials.add(WaterMaterial {
        params: Vec4::ZERO,
        index_atlas: images.add(atlases.indices),
        sky: images.add(water::load_sky(&data, map.night)),
    });
    let names: Vec<String> = data
        .cube_textures
        .entries
        .iter()
        .map(|e| e.file.clone())
        .collect();
    let mut pass_materials = HashMap::new();
    for pass in [Pass::Opaque, Pass::Masked, Pass::Alpha] {
        let material = StandardMaterial {
            base_color_texture: Some(if pass == Pass::Masked {
                keyed.clone()
            } else {
                opaque.clone()
            }),
            unlit: true,
            alpha_mode: match pass {
                Pass::Masked => AlphaMode::Mask(0.5),
                Pass::Alpha => AlphaMode::Blend,
                _ => AlphaMode::Opaque,
            },
            base_color: if pass == Pass::Alpha {
                Color::srgba(1.0, 1.0, 1.0, 0.5)
            } else {
                Color::WHITE
            },
            ..default()
        };
        pass_materials.insert(pass, materials.add(material));
    }
    let mut render = CityRender {
        names,
        materials: pass_materials,
        water: water.clone(),
        chunks: HashMap::new(),
        visible_layers: 3,
    };
    commands.insert_resource(Water(water));

    let city = City::new(map);
    mapedit::spawn_all_chunks(&mut commands, &mut render, &mut meshes, &city);
    commands.insert_resource(render);
    let mut library = ObjectLibrary::new(&data, &mut images, &mut object_materials, city.map.night);
    objects::spawn_scenery(
        &mut commands,
        &data,
        &mut library,
        &mut meshes,
        &city,
        &mut scenery,
    );
    commands.insert_resource(library);
    commands.insert_resource(find_start_points(&city, spec.level));
    commands.insert_resource(city);
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
