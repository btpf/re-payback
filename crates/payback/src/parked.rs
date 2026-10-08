//! Parked vehicles from `Maps/<n>.vehicles` (OS4 `FUN_0106dbbc`), spawned
//! only in Rampage and multiplayer (`DAT_0114c7c6 || DAT_0114c7c8`,
//! `FUN_010692bc` ~49456), standing on the top of their cube layer, with
//! script id 2000 + index and the black paint pair (`DAT_012a6328/48`).

use crate::data::GameData;
use crate::level::{AppState, GameMode, LevelLoaded, LevelSpec};
use crate::world::{City, SQUARE, z_of_h};
use bevy::prelude::*;
use payback_formats::vehicles::MapVehicles;
use payback_formats::map::MAP_SIZE;

pub struct ParkedPlugin;

impl Plugin for ParkedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_parked.after(LevelLoaded));
    }
}

fn spawn_parked(world: &mut World) {
    let mode = *world.resource::<GameMode>();
    if !(mode.is_rampage() || mode.is_multiplayer()) {
        return;
    }
    let map = world.resource::<LevelSpec>().map.clone();
    let Ok(bytes) = world.resource::<GameData>().read(format!("Maps/{map}.vehicles")) else {
        return;
    };
    let file = match MapVehicles::parse(&bytes) {
        Ok(f) => f,
        Err(e) => {
            warn!("Maps/{map}.vehicles: {e}");
            return;
        }
    };
    for (i, p) in file.parked.into_iter().enumerate() {
        let (sx, sy) = ((p.x >> 11) as usize, (p.y >> 11) as usize);
        let Some(top) = world.get_resource::<City>().and_then(|city| {
            (sx < MAP_SIZE && sy < MAP_SIZE).then(|| {
                let (fx, fy) = ((p.x as f32 / SQUARE).fract(), (p.y as f32 / SQUARE).fract());
                city.square_cubes(sx, sy)[p.layer.clamp(0, 2) as usize].top_at(fx, fy)
            })
        }) else {
            continue;
        };
        let pos = IVec3::new(p.x, p.y, z_of_h(top) as i32);
        if let Some(e) = crate::vehicles::spawn_vehicle_world(world, p.model, pos, p.heading, 0) {
            if let Some(mut o) = world.get_mut::<crate::sim::Obj>(e) {
                o.level = p.layer.clamp(0, 2) as usize;
                // Id 2000 + index (`+0x480`), so the far-object cull leaves
                // them alone.
                o.script_id = 2000 + i as i32;
            }
        }
    }
}
