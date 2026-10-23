//! Runtime map edits (script `SetCube`, `SetFace*`, regions; doors opened
//! by garages; glass smashed by projectiles) and the chunked city meshes
//! they rebuild. Also the visible-layer limit (`SetVisibleLayers`, roofs cut
//! away indoors; rendering.md §1 "Layer limit").

use crate::city::{self, CHUNK, CHUNKS, Pass};
use crate::level::LevelEntity;
use crate::lighting::{CityLighting, CityMaterial};
use crate::water::WaterMaterial;
use crate::world::City;
use bevy::prelude::*;
use payback_formats::map::{Face, FaceType, MAP_SIZE, Slope};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub enum EditOp {
    /// `SetCube(sx, sy, cube, h1, h2, scenery, rotation, slope)`;
    /// slope 0 removes the cube.
    Cube {
        h1: i16,
        h2: i16,
        scenery: u8,
        rotation: u8,
        slope: u8,
    },
    /// `SetFaceTop/N/E/S/W(sx, sy, cube, type, texture, flip)`; face index
    /// 0 top, 1 N, 2 E, 3 S, 4 W.
    Face {
        face: usize,
        kind: u8,
        texture: u16,
        flip: bool,
    },
}

#[derive(Message, Clone, Copy, Debug)]
pub struct MapEdit {
    pub sx: usize,
    pub sy: usize,
    pub cube: usize,
    pub op: EditOp,
}

/// Marks a city chunk mesh.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChunkMesh {
    pub cx: usize,
    pub cy: usize,
    pub layer: usize,
    /// Still drawn when `layer` equals the visible-layer limit (sides of
    /// sloped cubes, tops of slope 2/5; `city::drawn_at_layer_limit`).
    pub at_limit: bool,
}

/// Everything needed to (re)build city chunk meshes.
#[derive(Resource)]
pub struct CityRender {
    pub names: Vec<String>,
    pub materials: HashMap<Pass, Handle<CityMaterial>>,
    pub water: Handle<WaterMaterial>,
    /// Gouraud light and texture remap, while the option is on
    /// (`lighting.rs`).
    pub lighting: Option<Arc<CityLighting>>,
    pub remap: Option<Vec<u16>>,
    pub chunks: HashMap<(usize, usize), Vec<Entity>>,
    /// Cube layers drawn (`0x34c078`): 3 = all.
    pub visible_layers: usize,
}

/// Saved cube regions for `StoreRegion` / `PasteRegion` (2 slots).
#[derive(Resource, Default)]
pub struct Regions(pub [Option<RegionData>; 2]);

#[derive(Clone, Debug)]
pub struct RegionData {
    pub w: usize,
    pub h: usize,
    pub cubes: Vec<payback_formats::map::Cube>,
}

pub struct MapEditPlugin;

impl Plugin for MapEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MapEdit>()
            .init_resource::<Regions>()
            .add_systems(FixedPostUpdate, apply_edits)
            .init_resource::<LayerHidden>()
            .add_systems(Update, (layer_visibility, cull_above_limit));
    }
}

/// Spawn the meshes of chunk (cx, cy), replacing any existing ones.
pub fn spawn_chunk(
    commands: &mut Commands,
    render: &mut CityRender,
    meshes: &mut Assets<Mesh>,
    city: &City,
    cx: usize,
    cy: usize,
) {
    if let Some(old) = render.chunks.remove(&(cx, cy)) {
        for e in old {
            commands.entity(e).despawn();
        }
    }
    let mut spawned = Vec::new();
    let meshes_of = city::build_chunk_meshes(
        &city.map,
        &render.names,
        cx,
        cy,
        render.lighting.as_deref(),
        render.remap.as_deref(),
    );
    for (layer, at_limit, pass, mesh) in meshes_of {
        let mesh = Mesh3d(meshes.add(mesh));
        let tag = ChunkMesh {
            cx,
            cy,
            layer,
            at_limit,
        };
        let e = if pass == Pass::Reflect {
            commands.spawn((mesh, MeshMaterial3d(render.water.clone()), tag, LevelEntity))
        } else {
            commands.spawn((
                mesh,
                MeshMaterial3d(render.materials[&pass].clone()),
                tag,
                LevelEntity,
            ))
        }
        .id();
        spawned.push(e);
    }
    render.chunks.insert((cx, cy), spawned);
}

pub fn spawn_all_chunks(
    commands: &mut Commands,
    render: &mut CityRender,
    meshes: &mut Assets<Mesh>,
    city: &City,
) {
    for cx in 0..CHUNKS {
        for cy in 0..CHUNKS {
            spawn_chunk(commands, render, meshes, city, cx, cy);
        }
    }
}

fn apply_edits(
    mut commands: Commands,
    mut edits: MessageReader<MapEdit>,
    city: Option<ResMut<City>>,
    render: Option<ResMut<CityRender>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(mut city), Some(mut render)) = (city, render) else {
        edits.clear();
        return;
    };
    let mut dirty = HashSet::new();
    for edit in edits.read() {
        if edit.sx >= MAP_SIZE || edit.sy >= MAP_SIZE || edit.cube > 2 {
            continue;
        }
        let cube = &mut city.map.squares[edit.sx * MAP_SIZE + edit.sy].cubes[edit.cube];
        match edit.op {
            EditOp::Cube {
                h1,
                h2,
                scenery,
                rotation,
                slope,
            } => {
                cube.height1 = h1;
                cube.height2 = h2;
                cube.scenery = scenery;
                cube.rotation = rotation;
                cube.slope = slope_of(slope);
            }
            EditOp::Face {
                face,
                kind,
                texture,
                flip,
            } => {
                cube.faces[face] = Face {
                    kind: face_type(kind),
                    texture,
                    flipped: flip,
                };
            }
        }
        city.refresh_square(edit.sx, edit.sy);
        dirty.insert((edit.sx / CHUNK, edit.sy / CHUNK));
    }
    for (cx, cy) in dirty {
        spawn_chunk(&mut commands, &mut render, &mut meshes, &city, cx, cy);
    }
}

fn slope_of(v: u8) -> Slope {
    match v {
        0 => Slope::Off,
        1 => Slope::Flat,
        2 => Slope::Vert,
        3 => Slope::Horiz,
        4 => Slope::RVert,
        5 => Slope::RHoriz,
        n => Slope::Unknown(n),
    }
}

fn face_type(v: u8) -> FaceType {
    match v {
        0 => FaceType::Off,
        1 => FaceType::Normal,
        2 => FaceType::Genlocked,
        3 => FaceType::Alpha,
        4 => FaceType::Reflect,
        n => FaceType::Unknown(n),
    }
}

/// Objects and scenery the layer limit hid ([`cull_above_limit`]), so only
/// those are shown again: the simulation hides some objects for good (a
/// drowned player's corpse, wrecks), which must stay hidden.
#[derive(Resource, Default)]
struct LayerHidden(HashSet<Entity>);

/// The layer limit for what stands on the cubes (`FUN_01049374` / Mac
/// `DrawMap` @ 1005a260, rendering.md §1 "Layer limit"): in the pass for
/// cube layer L the moving objects of that layer (`FUN_010c9f28`) and the
/// cube scenery (`FUN_010c9360`) are drawn only when L < limit, so
/// everything on a floor at or above the limit is hidden indoors. (Effect
/// list L draws while L <= limit, `effects::build_meshes`; lists are the
/// object's floor + 1, the same cut.) Render-only: only `Visibility`
/// changes, never components.
fn cull_above_limit(
    render: Option<Res<CityRender>>,
    index: Option<Res<crate::objects::SceneryIndex>>,
    mut hidden: ResMut<LayerHidden>,
    mut objs: Query<(Entity, &crate::sim::Obj, &mut Visibility)>,
    mut scenery: Query<&mut Visibility, (With<crate::objects::Scenery>, Without<crate::sim::Obj>)>,
) {
    let limit = render.as_ref().map_or(3, |r| r.visible_layers);
    if limit >= 3 && hidden.0.is_empty() {
        return;
    }
    let mut set = |e: Entity, vis: &mut Visibility, hide: bool, hidden: &mut LayerHidden| {
        if hide {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
                hidden.0.insert(e);
            }
        } else if hidden.0.remove(&e) {
            *vis = Visibility::Inherited;
        }
    };
    for (e, o, mut vis) in &mut objs {
        set(e, &mut vis, o.level >= limit, &mut hidden);
    }
    if let Some(index) = index {
        for (&(_, _, level), &e) in &index.0 {
            if let Ok(mut vis) = scenery.get_mut(e) {
                set(e, &mut vis, level >= limit, &mut hidden);
            }
        }
    }
    // Despawned entities.
    hidden.0.retain(|&e| objs.contains(e) || scenery.contains(e));
}

/// Visible-layer limit: cube layers below it draw whole, the layer equal to
/// it only its `at_limit` part (sides of sloped cubes, tops of slope 2/5),
/// layers above it nothing. Mac `DrawMap` @ 1005a260, OS4 `FUN_01049374`
/// (rendering.md §1 "Layer limit").
fn layer_visibility(render: Option<Res<CityRender>>, mut q: Query<(&ChunkMesh, &mut Visibility)>) {
    let Some(render) = render else { return };
    if !render.is_changed() {
        return;
    }
    let limit = render.visible_layers;
    for (chunk, mut vis) in &mut q {
        *vis = if chunk.layer < limit || (chunk.layer == limit && chunk.at_limit) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
