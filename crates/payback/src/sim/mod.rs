//! Simulation core shared by every dynamic object, mirroring the original's
//! single object table (docs/re/people.md §1, combat_police.md §0).
//!
//! State is kept in the original integer units: x, y with 2048 per square,
//! z downward (16 per h-unit), angles 5760 per turn, rates per game tick.
//! One game tick is 1/30 s (Mac and OS4; the 68k uses 1/25 s); systems run
//! in `FixedUpdate` with ticks = 1.

pub mod ai;
pub mod boarding;
pub mod collide;
pub mod combat;
pub mod contact;
mod det_check;
pub mod driving;
pub mod people;
pub mod police;
pub mod props;
pub mod pursuit;
pub mod rc_car;
pub mod score;
pub mod traffic;
pub mod vehicle_ai;

use crate::data::GameData;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

/// Angle units per turn.
pub const TURN: i32 = 0x1680;

/// 8.8 fixed-point sine/cosine tables over 5760 angle units, generated as
/// the original does (`(int)(256·sin(i·2π/5760))`).
#[derive(Resource)]
pub struct Trig {
    sin: Vec<i32>,
    cos: Vec<i32>,
}

impl Default for Trig {
    fn default() -> Self {
        let f = |g: fn(f64) -> f64| -> Vec<i32> {
            (0..TURN)
                .map(|i| (256.0 * g(i as f64 * std::f64::consts::TAU / TURN as f64)) as i32)
                .collect()
        };
        Trig {
            sin: f(f64::sin),
            cos: f(f64::cos),
        }
    }
}

impl Trig {
    pub fn sin(&self, a: i32) -> i32 {
        self.sin[a.rem_euclid(TURN) as usize]
    }

    pub fn cos(&self, a: i32) -> i32 {
        self.cos[a.rem_euclid(TURN) as usize]
    }
}

/// Wrap an angle to 0..TURN.
pub fn wrap(a: i32) -> i32 {
    a.rem_euclid(TURN)
}

/// Signed shortest difference `to - from`, in -TURN/2..TURN/2.
pub fn angle_diff(from: i32, to: i32) -> i32 {
    let d = wrap(to - from);
    if d > TURN / 2 { d - TURN } else { d }
}

/// Heading pointing along (dx, dy) (0 = +y south, 0x5a0 = +x east).
pub fn heading_of(dx: i32, dy: i32) -> i32 {
    let a = (dx as f64).atan2(dy as f64);
    wrap((a / std::f64::consts::TAU * TURN as f64).round() as i32)
}

/// The game's random number generator: `General/RandomNumbers` holds 65536
/// big-endian i16 values 0..511; `rand(n) = T[++i & 0xffff] * n >> 9`
/// (OS4 `FUN_0103ac94`). Regenerated pseudo-randomly if the file is missing,
/// as the original does. `Clone` for debug drivers that need their own
/// stream (`autoplay`); the game itself has exactly one.
#[derive(Resource, Clone)]
pub struct Rng {
    table: Vec<i32>,
    idx: usize,
}

impl Rng {
    pub fn load(data: &GameData) -> Rng {
        let table = match data.read("General/RandomNumbers") {
            Ok(b) if b.len() >= 131072 => b
                .chunks_exact(2)
                .take(65536)
                .map(|c| i16::from_be_bytes([c[0], c[1]]) as i32)
                .collect(),
            _ => {
                let mut s: u32 = 0x1234_5678;
                (0..65536)
                    .map(|_| {
                        s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
                        ((s >> 16) & 0x7fff) as i32 >> 6
                    })
                    .collect()
            }
        };
        Rng { table, idx: 0 }
    }

    /// 0..n-1 (for n > 0). The product is a 32-bit int that wraps, as in
    /// the original (`random__Fi` @ 100834e0: `n * T[i] >> 9`); callers with
    /// n above 2^31 / 511 (the light jitter of millions) get its wrapped
    /// values.
    pub fn rand(&mut self, n: i32) -> i32 {
        self.idx = (self.idx + 1) & 0xffff;
        self.table[self.idx].wrapping_mul(n) >> 9
    }

    /// Deterministic hash lookup `T[i] * n >> 9` (OS4 `FUN_0103ad20`).
    pub fn hash(&self, i: usize, n: i32) -> i32 {
        self.table[i & 0xffff] * n >> 9
    }

    /// Restart the sequence at table index `s`. The table is fixed, so the
    /// index is the whole generator state: replays store it as their
    /// random seed and restore it at level start.
    pub fn seed(&mut self, s: u16) {
        self.idx = s as usize;
    }

    /// Current table index (the value [`Rng::seed`] restores).
    pub fn index(&self) -> u16 {
        self.idx as u16
    }

    /// The cosmetic stream over the same table, from index 0
    /// (`InitRandom` @ 100833b0 zeroes both counters).
    pub fn visual(&self) -> VisualRng {
        VisualRng {
            table: self.table.clone(),
            idx: 0,
        }
    }
}

/// The original's second generator, `random3` (Mac `random3__Fi` @
/// 10083530, OS4 `0103acc4`, 68k `FUN_0003fce6`): the same table as [`Rng`]
/// with its own counter (`_DAT_1017c6f0` / OS4 `c448`). The original draws
/// every cosmetic random number from it (effect updates, chimney smoke,
/// weather, effect lights, text wobble), so rendering never moves the
/// game's sequence. Render code uses this and never [`Rng`]; it is not
/// part of the replay state.
#[derive(Resource)]
pub struct VisualRng {
    table: Vec<i32>,
    idx: usize,
}

impl VisualRng {
    /// 0..n-1 (for n > 0): `T[++i3 & 0xffff] * n >> 9`, the product
    /// wrapping at 32 bits as in the original (see [`Rng::rand`]).
    pub fn rand(&mut self, n: i32) -> i32 {
        self.idx = (self.idx + 1) & 0xffff;
        self.table[self.idx].wrapping_mul(n) >> 9
    }
}

/// Object state byte (people.md §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Player = 2,
    Wander = 3,
    Idle = 4,
    Parked = 5,
    Removed = 6,
    Dead = 7,
    /// Police pursuit along the route graph (10) / straight at the target
    /// (11) (combat_police.md §2.2).
    PursuitRoute = 0xa,
    Pursuit = 0xb,
    Barrier = 0xc,
    Flee = 0x12,
}

/// Object class (model byte +0x4909).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Vehicle,
    Person,
    Projectile,
}

/// Per-model data derived at load time (people.md §2.1), in ×16 units.
#[derive(Clone, Debug, Default)]
pub struct ModelInfo {
    pub path: String,
    /// +0x48c4: max |X| (collision radius); people: MaleNormal's / 3 = 192.
    pub radius: i32,
    /// +0x48c8: max Y of near-ground vertices.
    pub max_y: i32,
    /// +0x48cc: |min Y| of near-ground vertices.
    pub min_y: i32,
    /// +0x48ec: mass.
    pub mass: i32,
    /// +0x4908: weapon/pose or pickup code.
    pub code: u8,
    /// +0x490a: police model.
    pub police: bool,
    /// +0x4910/+0x4914: mean X/Y of the ground vertices (all vertices if
    /// none touch the ground): the scenery circle's centre.
    pub mean: IVec2,
    /// +0x4918: scenery circle diameter, (max Y − min X) of the same
    /// vertices / 2, at least 0x80.
    pub size: i32,
}

impl ModelInfo {
    pub fn from_obj(path: &str, obj: &payback_formats::Obj) -> ModelInfo {
        let v = &obj.vertices;
        let radius = v.iter().map(|p| p.x.abs() * 16).max().unwrap_or(0);
        let near: Vec<_> = v.iter().filter(|p| p.z * 16 > -0x29).collect();
        let max_y = near.iter().map(|p| p.y * 16).max().unwrap_or(0);
        let min_y = near.iter().map(|p| p.y * 16).min().unwrap_or(0).abs();
        // Scenery circle (Mac `LoadStructure` @ 10074630, Mac 9 the same,
        // OS4 `FUN_010bf8bc`; CONFIRMED in all three): the ground vertices
        // (z = 0), or every vertex when there are none, give the mean and
        // the size. The size is (max Y − min X) / 2, at least 0x80: the
        // original keeps only min X and max Y of the four extents (two
        // compilers agree, OS4 drops the dead pair), so a lamp post's base
        // of 8 × 8 at Y 48..56 gives (896 + 64) / 2 = 480, radius 240.
        let ground: Vec<_> = v.iter().filter(|p| p.z == 0).collect();
        let ground = if ground.is_empty() { v.iter().collect() } else { ground };
        let n = ground.len().max(1) as i32;
        let mean = IVec2::new(
            ground.iter().map(|p| p.x * 16).sum::<i32>() / n,
            ground.iter().map(|p| p.y * 16).sum::<i32>() / n,
        );
        let min_x = ground.iter().map(|p| p.x * 16).min().unwrap_or(0);
        let top_y = ground.iter().map(|p| p.y * 16).max().unwrap_or(0);
        let extent = top_y - min_x;
        ModelInfo {
            path: path.to_string(),
            radius,
            max_y,
            min_y,
            mass: 1100,
            code: 0xff,
            police: false,
            mean,
            size: if extent < 0x100 { 0x80 } else { extent >> 1 },
        }
    }
}

/// Virtual stick: what the player's controls or the AI ask an object to do
/// this tick (people.md §4.1).
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct Stick {
    /// -1 left, +1 right.
    pub steer: i32,
    /// -1 forward, +1 back.
    pub throttle: i32,
    pub fire: bool,
    pub brake: bool,
    /// In/out of vehicle (edge-triggered).
    pub use_vehicle: bool,
}

/// The core dynamic-object record.
///
/// Render code must never add or remove components on an `Obj` entity:
/// that moves it to another ECS table at a frame-rate-dependent moment,
/// and query iteration order (which the simulation must not depend on, but
/// see [`table_order`]) would follow. The render components Bevy would add
/// later (`Aabb`, `PreviousGlobalTransform`) and the object renderer's own
/// slots are required here so they exist from the spawn.
#[derive(Component, Clone, Debug)]
#[component(on_add = obj_added, on_remove = obj_removed)]
#[require(
    bevy::camera::primitives::Aabb,
    bevy::pbr::PreviousGlobalTransform,
    crate::objects::OverlayChildren,
    crate::objects::DoorPanel,
    vehicle_ai::TargetNode
)]
pub struct Obj {
    /// Index in the object table ([`ObjTable`]), the order the simulation
    /// visits objects in. Kept up to date by the table.
    pub slot: u32,
    pub pos: IVec3,
    pub vel: IVec3,
    pub prev_pos: IVec3,
    pub heading: i32,
    /// 0x5a0 standing; 0 lying (people).
    pub pitch: i32,
    pub roll: i32,
    pub speed: i32,
    pub prev_speed: i32,
    pub turn_rate: i32,
    pub state: State,
    pub class: Class,
    pub health: i32,
    pub level: usize,
    pub floor_z: i32,
    pub on_ground: bool,
    pub in_water: bool,
    pub model: usize,
    /// Player number, if player-controlled.
    pub player: Option<u8>,
    pub wanted: i32,
    pub score: i32,
    /// Burn timer (> 0: on fire; 100 when set alight).
    pub burn: i32,
    /// Damage filter: -1 vulnerable to everything, else only the matching
    /// weapon class (combat_police.md §6).
    pub filter: i32,
    /// Script id (-1 = none), matched by mission triggers.
    pub script_id: i32,
    /// Who created/fired this object (projectiles, thrown drivers).
    pub owner: Option<Entity>,
    /// Owner side (Mac `+0x824`, 68k `+0x342`): −1 for every new object
    /// (`NewObj`), 0 for players, the vehicles they get into, script
    /// actors (`AddObject` @ 10105780) and 1 for most player-side shots
    /// (`do_Person` @ 1013fbd0). The script goto / route-flee orders set −1
    /// again. Crime, ram, punch and blast score only count from an object
    /// with `side >= 0` on one with `side == -1` (`CheckForCollisions` @
    /// 1006a150, `Shockwave` @ 10054ac0, `CheckHitObject` @ 100542d0).
    pub side: i32,
    /// God mode (port addition, `crate::cheats`): the player's object
    /// takes no damage. Set at the start of every tick for the objects
    /// with a `player` while god mode is on, and for those that are
    /// vehicles while vehicle god mode is on.
    pub god: bool,
}

impl Obj {
    pub fn new(class: Class, model: usize, pos: IVec3, heading: i32) -> Obj {
        Obj {
            slot: u32::MAX,
            pos,
            vel: IVec3::ZERO,
            prev_pos: pos,
            heading,
            pitch: if class == Class::Person { 0x5a0 } else { 0 },
            roll: 0,
            speed: 0,
            prev_speed: 0,
            turn_rate: 0,
            state: State::Idle,
            class,
            health: 100,
            level: 0,
            floor_z: pos.z,
            on_ground: false,
            in_water: false,
            model,
            player: None,
            wanted: 0,
            score: 0,
            burn: 0,
            filter: -1,
            script_id: -1,
            owner: None,
            side: -1,
            god: false,
        }
    }

    /// Position for the camera/audio, as floats.
    pub fn posf(&self) -> Vec3 {
        self.pos.as_vec3()
    }
}

/// The object table: every `Obj` entity in table order, as the original's
/// object array (Mac `NewObj` @ 100575b0 appends at index `++count`,
/// `RemoveObj` @ 10057830 moves the last object into the freed index;
/// OS4/68k keep the same array). Its order is decided by the simulation
/// alone (spawns and despawns are applied in a fixed system order), never
/// by the ECS storage or entity ids, which render code also touches. The
/// original moves player-model objects into the first slots on creation;
/// here players simply take the next slot.
#[derive(Resource, Default, Debug)]
pub struct ObjTable(pub Vec<Entity>);

fn obj_added(mut world: DeferredWorld, ctx: HookContext) {
    let Some(mut table) = world.get_resource_mut::<ObjTable>() else {
        return;
    };
    table.0.push(ctx.entity);
    let slot = (table.0.len() - 1) as u32;
    if let Some(mut o) = world.get_mut::<Obj>(ctx.entity) {
        o.slot = slot;
    }
}

fn obj_removed(mut world: DeferredWorld, ctx: HookContext) {
    let Some(slot) = world.get::<Obj>(ctx.entity).map(|o| o.slot as usize) else {
        return;
    };
    let Some(mut table) = world.get_resource_mut::<ObjTable>() else {
        return;
    };
    if table.0.get(slot) != Some(&ctx.entity) {
        return;
    }
    table.0.swap_remove(slot);
    let moved = table.0.get(slot).copied();
    if let Some(mut o) = moved.and_then(|m| world.get_mut::<Obj>(m)) {
        o.slot = slot as u32;
    }
}

/// The entities of `objs` in object-table order. Every simulation loop whose
/// result depends on the visiting order (random numbers drawn per object,
/// spawns, first-come picks, messages and reports, pairs) goes through this
/// instead of iterating the query directly: the query order follows the
/// ECS tables, which are not part of the replayed state.
pub fn table_order<'a>(objs: impl IntoIterator<Item = (Entity, &'a Obj)>) -> Vec<Entity> {
    let mut v: Vec<(u32, Entity)> = objs.into_iter().map(|(e, o)| (o.slot, e)).collect();
    v.sort_unstable_by_key(|p| p.0);
    v.into_iter().map(|p| p.1).collect()
}

/// The simulation tick. The original (`amiga_levelloop` @ 10100f50) reads
/// the input, spawns (`AddVehicles`, `DoCollideStuff`), runs the weather,
/// the contacts (`CheckForCollisions` @ 1006a150), then every object in
/// table order (`MainLoop` @ 1014a920 → `object_MainLoop` @ 1014a670: move,
/// then its AI/controls/firing in `do_Person` / `do_Other` / `do_Weapon`,
/// then its occupancy and scenery push), then the triggers. The port keeps
/// that order per phase (AI-1):
///
/// 1. `ReplayInput` (input, cheats, the replay hash), the autoplay bot;
/// 2. `traffic::SpawnSet` (light phase, ambient spawner), the weather;
/// 3. `contact::ContactSet` (contacts, vehicle damage);
/// 4. people: move (`people::PeopleThink` starts with `move_people`), AI
///    and controls, boarding, steer (`people::PeopleSet`);
/// 5. vehicles: move (`driving::DrivingMoveSet`), traffic and script AI
///    and pursuit (`TrafficSet`, `VehicleAiSet`), controls and scenery
///    (`driving::DrivingSet`);
/// 6. combat (firing, projectiles, explosions, deaths, pickups), score,
///    crimes, then the triggers (`mission::eval::run_triggers`).
///
/// What remains different: the original interleaves step 4/5/6 per object
/// in table order (an object sees the ones before it updated and the ones
/// after it not yet; the occupancy grid is rebuilt per object), where the
/// port runs each step over every object; people and vehicles are
/// separate passes; firing and projectile moves are a pass after all
/// movement rather than inside each object's update.
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        // The fixed-tick schedules run on one thread. Systems without an
        // ordering between them (Bevy reports a few hundred conflicting
        // pairs in `FixedUpdate`) then always run in the schedule's own
        // topological order, the same in every run of a build; the
        // multi-threaded executor would run them in whatever order the
        // threads happen to reach them, and a replay could diverge on the
        // order of two systems drawing from `Rng`. The simulation is small;
        // it doesn't need the threads.
        use bevy::ecs::schedule::ScheduleLabel;
        for label in [FixedPreUpdate.intern(), FixedUpdate.intern(), FixedPostUpdate.intern()] {
            app.edit_schedule(label, |s| {
                s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
            });
        }
        det_check::plugin(app);
        app.init_resource::<Trig>()
            .init_resource::<ObjTable>()
            .add_systems(Startup, |mut commands: Commands, data: Res<GameData>| {
                let rng = Rng::load(&data);
                commands.insert_resource(rng.visual());
                commands.insert_resource(rng);
            })
            .add_plugins((
                people::PeoplePlugin,
                combat::CombatPlugin,
                police::PolicePlugin,
                driving::DrivingPlugin,
                boarding::BoardingPlugin,
                traffic::TrafficPlugin,
                contact::ContactPlugin,
                score::ScorePlugin,
                vehicle_ai::VehicleAiPlugin,
                pursuit::PursuitPlugin,
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_tables() {
        let t = Trig::default();
        assert_eq!(t.sin(0), 0);
        assert_eq!(t.cos(0), 256);
        assert_eq!(t.sin(0x5a0), 256);
        assert_eq!(t.sin(-0x5a0), -256);
    }

    #[test]
    fn angles() {
        assert_eq!(heading_of(0, 100), 0);
        assert_eq!(heading_of(100, 0), 0x5a0);
        assert_eq!(angle_diff(100, TURN - 100), -200);
        assert_eq!(angle_diff(TURN - 100, 100), 200);
    }

    /// Scenery circle (`LoadStructure` @ 10074630): mean and (max Y − min
    /// X)/2 of the ground vertices only, so a lamp post's overhanging arm
    /// and light do not count.
    #[test]
    fn scenery_size_from_the_ground_vertices() {
        let vertex = |x, y, z| payback_formats::obj::Vertex { x, y, z, limb: 0 };
        let mut vertices: Vec<_> = [(-4, 48), (4, 48), (-4, 56), (4, 56)]
            .into_iter()
            .map(|(x, y)| vertex(x, y, 0))
            .collect();
        // The arm and a wide light halo up top.
        vertices.extend([vertex(-2, -248, -248), vertex(120, 82, -252), vertex(-120, -150, -200)]);
        let obj = payback_formats::Obj { flag: 0, vertices, faces: Vec::new() };
        let info = ModelInfo::from_obj("lamppost", &obj);
        assert_eq!(info.mean, IVec2::new(0, 52 * 16));
        assert_eq!(info.size, (56 * 16 + 4 * 16) >> 1);
        // No ground vertices: all of them; below 0x100 the floor of 0x80.
        let obj = payback_formats::Obj { flag: 0, vertices: vec![vertex(-2, 2, -5), vertex(2, 4, -9)], faces: Vec::new() };
        let info = ModelInfo::from_obj("small", &obj);
        assert_eq!((info.mean, info.size), (IVec2::new(0, 48), 0x80));
    }
}
