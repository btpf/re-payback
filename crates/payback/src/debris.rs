//! Flying debris: the faces of a destroyed object thrown off as single
//! spinning triangles (docs/re/rendering.md §7 "Debris").
//!
//! `Split` (Mac `Split__FP9Structureiiiiiiiiii` @ 100a78b0, OS4
//! `FUN_01035438`) turns every face of a model into a part (a person's
//! first 20 only); `Detonate` (Mac @ 100550c0) calls it twice for an object
//! blown apart, `Bang` (Mac @ 100a6dd0) once for a broken crate. The parts
//! live in their own lists (4 × 500, one per layer like the effects; record
//! `0x38` bytes), are moved by the effect updater (Mac `UpdateEffects`
//! @ 100aa1f0, OS4 `FUN_010386e8`) and drawn by `DrawPart` (Mac
//! @ 100794c0) as flat black triangles (palette colour 0 in software,
//! RGB 0 / alpha 1 in hardware); mode-5 faces as black lines. Every random
//! number comes from the cosmetic generator (`random3`), so debris never
//! touches the game's sequence: here [`VisualRng`], and a render-only
//! resource.
//!
//! Not reproduced: the removal of parts outside the view rectangle (as for
//! the effects, the lists are not per view here) and `DrawPart`'s near
//! cull (depth ≤ 6000).

use crate::effects::{Blend, Effects, Tex};
use crate::objects::ObjectLibrary;
use crate::sim::{Class, Obj, State, Trig, VisualRng};
use crate::world::object_to_render;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    platform::collections::HashSet,
    prelude::*,
};

/// Part lists, one per layer (cube storey + 1), as the effects.
const LISTS: usize = 4;
/// `Split` stops when a list holds this many parts.
const MAX_PARTS: usize = 500;
/// Street level: a part below it is removed (or bounces, 1 in 3).
const GROUND_Z: i32 = 0x57e4;
/// Added to vz every update.
const GRAVITY: i32 = 0x32;
/// A person splits into at most this many faces.
const PERSON_FACES: usize = 20;
/// Objects below this health are blown apart (`Detonate`).
const BLOWN_APART: i32 = -70;
const TURN: i32 = 0x1680;

/// One part (`Part`, 0x38 bytes): the model and its face, position and
/// velocity in object units, three angles and their spin per tick.
#[derive(Clone, Debug)]
struct Part {
    model: usize,
    /// 0-based face (the original's faces are numbered from 1).
    face: usize,
    /// Person model (`+0x8165 == 1`): the face is drawn a third the size,
    /// and the part bleeds instead of smoking.
    person: bool,
    pos: IVec3,
    vel: IVec3,
    /// Roll (X–Z), pitch (Y–Z), heading (X–Y), as the object's `+0xc`,
    /// `+0x10`, `+0x14`.
    ang: [i32; 3],
    spin: [i32; 3],
}

/// The debris lists.
#[derive(Resource, Default)]
pub struct Parts {
    lists: [Vec<Part>; LISTS],
    /// Crates broken this tick: (centre, list, cube rotation).
    bangs: Vec<(IVec3, usize, u8)>,
}

impl Parts {
    /// A crate breaks: `Bang(crate, list, x, y, z, rotation·0x1680 >> 8,
    /// 5)` (Mac @ 100a6dd0, OS4 `FUN_01035730`), done after the tick's
    /// combat (the crate model is the renderer's).
    pub fn bang(&mut self, centre: IVec3, list: usize, rotation: u8) {
        self.bangs.push((centre, list, rotation));
    }

    /// `Split(model, list, x, y, z, vx, vy, vz, roll, pitch, heading)`:
    /// one part per face at the face's centroid (a person's at ±500 at
    /// random), velocity `v ± 125` (z: `vz − 250 … vz`; people ±50 and
    /// `vz − 100 … vz`), spin ±125 on each axis.
    #[allow(clippy::too_many_arguments)]
    pub fn split(
        &mut self,
        library: &ObjectLibrary,
        model: usize,
        person: bool,
        list: usize,
        pos: IVec3,
        vel: IVec3,
        ang: [i32; 3],
        rand: &mut impl FnMut(i32) -> i32,
    ) {
        let obj = &library.model(model).obj;
        let list = &mut self.lists[list.min(LISTS - 1)];
        let (n, spread) = if person {
            (obj.faces.len().min(PERSON_FACES), 100)
        } else {
            (obj.faces.len(), 250)
        };
        for face in 0..n {
            if list.len() >= MAX_PARTS {
                break;
            }
            let offset = if person {
                let x = rand(1000) - 500;
                let y = rand(1000) - 500;
                let z = rand(1000) - 500;
                IVec3::new(x, y, z)
            } else {
                centroid(obj, face)
            };
            let vx = vel.x + rand(spread) - (spread >> 1);
            let vy = vel.y + rand(spread) - (spread >> 1);
            let vz = vel.z - rand(spread) - spread;
            let s0 = rand(250) - 125;
            let s1 = rand(250) - 125;
            let s2 = rand(250) - 125;
            list.push(Part {
                model,
                face,
                person,
                pos: pos + offset,
                vel: IVec3::new(vx, vy, vz),
                ang,
                spin: [s0, s1, s2],
            });
        }
    }

    pub fn clear(&mut self) {
        self.lists.iter_mut().for_each(Vec::clear);
        self.bangs.clear();
    }

    fn is_empty(&self) -> bool {
        self.lists.iter().all(Vec::is_empty)
    }

    /// One tick of the part lists (`UpdateEffects`, ticks = 1).
    fn tick(&mut self, effects: &mut Effects, rand: &mut impl FnMut(i32) -> i32) {
        for (l, list) in self.lists.iter_mut().enumerate() {
            let layer = l as u8;
            let mut i = 0;
            while i < list.len() {
                // Below the street: removed (replaced by the last part,
                // which is tested again), leaving a blood splat (people)
                // or three smoke puffs.
                let p = &list[i];
                if p.pos.z > GROUND_Z {
                    landing(p, layer, effects, rand);
                    list.swap_remove(i);
                    continue;
                }
                let p = &mut list[i];
                p.pos += p.vel;
                if p.pos.z > GROUND_Z && rand(3) == 1 {
                    p.pos.z = GROUND_Z;
                    p.vel.z = -(p.vel.z >> 1);
                }
                p.vel.z += GRAVITY;
                for k in 0..3 {
                    p.ang[k] = (p.ang[k] + p.spin[k]).rem_euclid(TURN);
                }
                trail(p, layer, effects, rand);
                i += 1;
            }
        }
    }
}

/// The centroid of a face in object units (vertices ×16, `/3` truncated).
fn centroid(obj: &payback_formats::Obj, face: usize) -> IVec3 {
    let f = &obj.faces[face];
    let v = |k: usize| obj.vertices.get(f.indices[k]).map_or(IVec3::ZERO, |v| IVec3::new(v.x, v.y, v.z) * 16);
    (v(0) + v(1) + v(2)) / 3
}

/// A part reaching the street (`UpdateEffects`): people leave a type-8
/// `Blood` splat of 150 + r(200) through table 3 in the list below; other
/// parts three type-6 smoke puffs (`Dust` with the shadow flag, table 1)
/// of 400 + r(200) rising at r(100)/3.
fn landing(p: &Part, layer: u8, effects: &mut Effects, rand: &mut impl FnMut(i32) -> i32) {
    if p.person {
        let size = rand(200) + 150;
        let at = Vec3::new(p.pos.x as f32, p.pos.y as f32, GROUND_Z as f32);
        effects.add(8, Tex::Blood, at, Vec3::ZERO, size as f32, layer.saturating_sub(1), Blend::Tint);
        return;
    }
    for _ in 0..3 {
        let size = rand(200) + 400;
        let vz = -rand(100);
        let vy = rand(100) - 50;
        let vx = rand(100) - 50;
        let y = p.pos.y + rand(100) - 50;
        let x = p.pos.x + rand(100) - 50;
        let at = Vec3::new(x as f32, y as f32, GROUND_Z as f32);
        let v = Vec3::new(vx as f32, vy as f32, vz as f32);
        effects.add(6, Tex::Dust2, at, v, size as f32, layer, Blend::Subtract);
    }
}

/// The trail, 1 in 3 per tick: people drip type-6 `Blood` (table 3, 150 +
/// r(50), ±25), other parts trail type-1 smoke (`Dust` with the shadow
/// flag, table 1, 250 + r(100), ±50). Both drift `(r(20), r(20) − 10,
/// 10 − r(20)) / 3`. Arguments are drawn last first.
fn trail(p: &Part, layer: u8, effects: &mut Effects, rand: &mut impl FnMut(i32) -> i32) {
    if rand(3) != 1 {
        return;
    }
    let (r, base) = if p.person { (50, 150) } else { (100, 250) };
    let size = rand(r) + base;
    let vz = 10 - rand(20);
    let vy = rand(20) - 10;
    let vx = rand(20);
    let z = p.pos.z + rand(r) - r / 2;
    let y = p.pos.y + rand(r) - r / 2;
    let x = p.pos.x + rand(r) - r / 2;
    let at = Vec3::new(x as f32, y as f32, z as f32);
    let v = Vec3::new(vx as f32, vy as f32, vz as f32);
    if p.person {
        effects.add(6, Tex::Blood, at, v, size as f32, layer, Blend::Tint);
    } else {
        effects.add(1, Tex::Dust2, at, v, size as f32, layer, Blend::Subtract);
    }
}

/// Objects already split, so `Detonate` splits each once.
#[derive(Resource, Default)]
struct Detonated(HashSet<Entity>);

/// The debris mesh (triangles) and its mode-5 lines.
#[derive(Component)]
struct DebrisMesh(bool);

pub struct DebrisPlugin;

impl Plugin for DebrisPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Parts>()
            .init_resource::<Detonated>()
            .add_systems(Startup, spawn_meshes)
            .add_systems(FixedPreUpdate, tick_parts.after(crate::effects::update_effects))
            .add_systems(FixedUpdate, detonate.after(crate::sim::combat::CombatSet))
            .add_systems(
                OnExit(crate::level::AppState::Playing),
                |mut parts: ResMut<Parts>, mut done: ResMut<Detonated>| {
                    parts.clear();
                    done.0.clear();
                },
            )
            .add_systems(PostUpdate, build_meshes);
    }
}

fn tick_parts(mut parts: ResMut<Parts>, mut effects: ResMut<Effects>, rng: Option<ResMut<VisualRng>>) {
    let Some(mut rng) = rng else { return };
    if !parts.is_empty() {
        parts.tick(&mut effects, &mut |n| rng.rand(n));
    }
}

/// `Detonate` (Mac @ 100550c0): an object blown apart (health below −70:
/// vehicles in `DoCollideStuff`, people, the RC car in water) is split
/// twice, at its position, with half its velocity and its angles, into
/// list `layer + 1`. The simulation's blow-apart paths (`sim::contact`,
/// `sim::combat`) leave such an object at that health, dead or removed;
/// this splits it once, after the tick's combat.
fn detonate(
    library: Option<Res<ObjectLibrary>>,
    rng: Option<ResMut<VisualRng>>,
    mut parts: ResMut<Parts>,
    mut effects: ResMut<Effects>,
    mut done: ResMut<Detonated>,
    q: Query<(Entity, &Obj)>,
) {
    let (Some(library), Some(mut rng)) = (library, rng) else {
        parts.bangs.clear();
        return;
    };
    // `Bang`: `Split(crate, list, x, y, z, 0, 0, 0, 0, 0, angle)`, then
    // max(2, min(6, 5 >> 1)) = 2 type-3 Dust puffs at the centre ± 400
    // (z ± 150).
    let crate_model = library.loaded(crate::objects::SCENERY[5]);
    for (centre, list, rotation) in std::mem::take(&mut parts.bangs) {
        if let Some(model) = crate_model {
            let angle = rotation as i32 * TURN >> 8;
            parts.split(&library, model, false, list, centre, IVec3::ZERO, [0, 0, angle], &mut |n| rng.rand(n));
        }
        let base = centre - IVec3::new(400, 400, 150);
        for _ in 0..2 {
            crate::sim::combat::dust_puff(&mut effects, &mut |n| rng.rand(n), base, list as u8);
        }
    }
    done.0.retain(|&e| q.get(e).is_ok_and(|(_, o)| o.health < BLOWN_APART));
    for e in crate::sim::table_order(q.iter()) {
        let Ok((_, o)) = q.get(e) else { continue };
        let blown = match o.class {
            Class::Vehicle => matches!(o.state, State::Dead | State::Removed),
            Class::Person => o.state == State::Removed,
            Class::Projectile => false,
        };
        if o.health >= BLOWN_APART || !blown || !done.0.insert(e) {
            continue;
        }
        let person = o.class == Class::Person;
        for _ in 0..2 {
            parts.split(
                &library,
                o.model,
                person,
                o.level + 1,
                o.pos,
                o.vel / 2,
                [o.roll, o.pitch, o.heading],
                &mut |n| rng.rand(n),
            );
        }
    }
}

fn spawn_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    for (lines, topology) in [(false, PrimitiveTopology::TriangleList), (true, PrimitiveTopology::LineList)] {
        // A degenerate primitive: an empty mesh has no vertex data to
        // upload.
        let n = if lines { 2 } else { 3 };
        let mesh = Mesh::new(topology, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n])
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
            .with_inserted_indices(Indices::U32((0..n as u32).collect()));
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
            Visibility::Hidden,
            bevy::camera::visibility::NoFrustumCulling,
            DebrisMesh(lines),
        ));
    }
}

/// `DrawPart`: each part's face around its centroid (people a third the
/// size), turned by roll (X–Z), pitch (Y–Z) and heading (X–Y) as in the
/// original's integer steps, at the part's position.
fn build_meshes(
    parts: Res<Parts>,
    library: Option<Res<ObjectLibrary>>,
    trig: Res<Trig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&DebrisMesh, &Mesh3d, &mut Visibility)>,
) {
    let mut tris: Vec<[f32; 3]> = Vec::new();
    let mut lines: Vec<[f32; 3]> = Vec::new();
    if let Some(library) = &library {
        for p in parts.lists.iter().flatten() {
            let obj = &library.model(p.model).obj;
            let Some(face) = obj.faces.get(p.face) else { continue };
            let c = centroid(obj, p.face);
            let (ca, sa) = (trig.cos(p.ang[0]), trig.sin(p.ang[0]));
            let (cb, sb) = (trig.cos(p.ang[1]), trig.sin(p.ang[1]));
            let (cg, sg) = (trig.cos(p.ang[2]), trig.sin(p.ang[2]));
            let div = if p.person { 0x300 } else { 0x100 };
            let corner = |k: usize| -> [f32; 3] {
                let v = obj.vertices.get(face.indices[k]).map_or(IVec3::ZERO, |v| IVec3::new(v.x, v.y, v.z) * 16) - c;
                let x1 = (v.x * ca + v.z * sa) >> 8;
                let z1 = (v.z * ca - v.x * sa) >> 8;
                let y1 = (v.y * cb - sb * z1) >> 8;
                let z = (cb * z1 + v.y * sb) / div;
                let y = (y1 * cg - sg * x1) / div;
                let x = (cg * x1 + y1 * sg) / div;
                object_to_render((p.pos + IVec3::new(x, y, z)).as_vec3()).to_array()
            };
            // Mode 5 (slot 0's mode, active or not): a line between the
            // first two corners.
            if face.slots[0].mode == 5 {
                lines.extend([corner(0), corner(1)]);
            } else {
                tris.extend([corner(0), corner(1), corner(2)]);
            }
        }
    }
    for (kind, mesh, mut vis) in &mut q {
        let pos = if kind.0 { &lines } else { &tris };
        let want = if pos.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        if pos.is_empty() {
            continue;
        }
        let Some(mut m) = meshes.get_mut(&mesh.0) else { continue };
        let n = pos.len();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
        m.insert_indices(Indices::U32((0..n as u32).collect()));
    }
}
