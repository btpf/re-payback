//! Vehicles (docs/re/vehicles.md; type table from the 68k object loader
//! `FUN_000d7874`, ids confirmed by the mission and `.vehicles` analyses).

use crate::audio::{Sound, SoundPos, id};
use crate::data::GameData;
use crate::level::LevelEntity;
use crate::objects::ObjectLibrary;
use crate::sim::{Class, Obj, State};
use crate::world::{heading_to_render, object_to_render};
use bevy::prelude::*;

/// Static data per vehicle type (OS4 loader `FUN_0104e13c`; field offsets
/// of the model record in brackets).
#[derive(Clone, Copy, Debug)]
pub struct VehicleType {
    pub model: &'static str,
    /// Name phrase (`0x493c`), shown when entering.
    pub name_phrase: u32,
    /// City restriction for traffic (`0x48d4`): -1 any, else city 0..2.
    pub city: i32,
    /// Max forward / reverse speed (`0x48d8` / `0x48dc`).
    pub max_speed: i32,
    pub max_reverse: i32,
    /// Throttle acceleration and brake deceleration per tick (`0x48e0`, `0x48f0`).
    pub accel: i32,
    pub brake: i32,
    /// Max yaw rate (`0x48e4`) and grip (`0x48e8`).
    pub turn: i32,
    pub grip: i32,
    /// Mass / toughness T (`0x48ec`).
    pub mass: i32,
    /// Traffic spawn weight /256 (`0x4934`).
    pub weight: i32,
    /// Engine, horn and reversing-beeper sounds (`0x4924`, `0x491c`, `0x4920`).
    pub engine: u16,
    pub horn: u16,
    pub reverse_beep: Option<u16>,
    /// In-car CD track in Radio mode (`0x4938`).
    pub radio: Option<u8>,
    pub door: Door,
    pub police: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    Default,
    Police,
    Bus,
    Taxi,
    Without,
}

#[allow(clippy::too_many_arguments)]
const fn vt(
    model: &'static str,
    name_phrase: u32,
    city: i32,
    [
        max_speed,
        max_reverse,
        accel,
        brake,
        turn,
        grip,
        mass,
        weight,
    ]: [i32; 8],
    engine: u16,
    horn: u16,
    radio: i32,
    door: Door,
) -> VehicleType {
    VehicleType {
        model,
        name_phrase,
        city,
        max_speed,
        max_reverse,
        accel,
        brake,
        turn,
        grip,
        mass,
        weight,
        engine,
        horn,
        reverse_beep: None,
        radio: if radio < 0 { None } else { Some(radio as u8) },
        door,
        police: horn == id::SIREN,
    }
}

const fn beeps(mut t: VehicleType) -> VehicleType {
    t.reverse_beep = Some(id::BLEEP);
    t
}

use Door::*;

/// Vehicle types by script/traffic id (table `0x348120`, 28 entries).
pub const TYPES: [VehicleType; 28] = [
    vt(
        "vehicles/mundaneo.obj",
        2330,
        -1,
        [280, -150, 5, 18, 40, 25, 1400, 256],
        id::ENGINE,
        id::CAR_HORN,
        7,
        Default,
    ),
    vt(
        "vehicles/pug.obj",
        2331,
        -1,
        [320, -120, 6, 18, 45, 26, 900, 256],
        id::ENGINE_935,
        id::CAR_HORN,
        11,
        Default,
    ),
    vt(
        "vehicles/diblo.obj",
        2332,
        -1,
        [550, -250, 15, 35, 55, 34, 1400, 45],
        id::ENGINE_904,
        id::CAR_HORN,
        5,
        Default,
    ),
    beeps(vt(
        "vehicles/bus.obj",
        2333,
        -1,
        [1000, -150, 2, 5, 25, 15, 10000, 160],
        id::ENGINE_BUS,
        id::CAR_HORN,
        -1,
        Bus,
    )),
    vt(
        "vehicles/vapour.obj",
        2334,
        -1,
        [500, -240, 16, 35, 50, 28, 1800, 65],
        id::ENGINE_904,
        id::CAR_HORN,
        4,
        Default,
    ),
    vt(
        "vehicles/pug GTI.obj",
        2335,
        -1,
        [400, -200, 10, 24, 50, 30, 850, 240],
        id::ENGINE_935,
        id::CAR_HORN,
        6,
        Default,
    ),
    vt(
        "vehicles/fjord.obj",
        2336,
        -1,
        [300, -190, 6, 22, 45, 25, 1300, 256],
        id::ENGINE,
        id::CAR_HORN,
        7,
        Default,
    ),
    vt(
        "vehicles/van.obj",
        2337,
        -1,
        [250, -160, 4, 20, 35, 23, 2200, 256],
        id::ENGINE_BUS,
        id::CAR_HORN,
        15,
        Default,
    ),
    vt(
        "vehicles/limo.obj",
        2338,
        -1,
        [380, -180, 7, 25, 45, 26, 3000, 100],
        id::ENGINE_BUS,
        id::CAR_HORN,
        2,
        Default,
    ),
    vt(
        "vehicles/police.obj",
        2339,
        -1,
        [600, -250, 17, 40, 60, 45, 1600, 200],
        id::ENGINE_911,
        id::SIREN,
        -1,
        Police,
    ),
    vt(
        "vehicles/scooby.obj",
        2340,
        -1,
        [480, -240, 14, 35, 53, 42, 1235, 150],
        id::ENGINE_911,
        id::CAR_HORN,
        10,
        Default,
    ),
    vt(
        "vehicles/tank.obj",
        2341,
        -1,
        // Reverse -220 as on the Mac (`InitStructures` @ 1005ffb0); the
        // Amiga had -100 (68k `FUN_000d7874`, OS4 `FUN_0104e13c`).
        [220, -220, 6, 20, 55, 50, 40000, 0],
        id::TANK_ENGINE,
        id::CAR_HORN,
        -1,
        Without,
    ),
    vt(
        "vehicles/evo.obj",
        2342,
        -1,
        [470, -230, 15, 35, 55, 45, 1400, 130],
        id::ENGINE_911,
        id::CAR_HORN,
        9,
        Default,
    ),
    vt(
        "vehicles/taxi.obj",
        2343,
        -1,
        [310, -200, 6, 21, 43, 24, 1380, 256],
        id::ENGINE,
        id::CAR_HORN,
        3,
        Taxi,
    ),
    vt(
        "vehicles/icecreamvan.obj",
        2344,
        0,
        [240, -170, 5, 20, 32, 23, 2000, 100],
        id::ENGINE_BUS,
        id::ICE_CREAM,
        -1,
        Bus,
    ),
    vt(
        "vehicles/pickup.obj",
        2345,
        -1,
        [230, -165, 5, 23, 37, 25, 1800, 160],
        id::ENGINE_BUS,
        id::CAR_HORN,
        14,
        Default,
    ),
    vt(
        "vehicles/hotrod.obj",
        2346,
        -1,
        [450, -200, 13, 30, 40, 40, 1800, 100],
        id::ENGINE_904,
        id::CAR_HORN,
        2,
        Default,
    ),
    vt(
        "vehicles/helicopter.obj",
        2347,
        -1,
        [650, -650, 6, 6, 40, 5, 1500, 0],
        id::HELICOPTER_BLADES,
        id::CAR_HORN,
        -1,
        Default,
    ),
    vt(
        "vehicles/pug racer.obj",
        2348,
        -1,
        [340, -130, 8, 22, 50, 30, 700, 0],
        id::ENGINE_935,
        id::CAR_HORN,
        5,
        Default,
    ),
    beeps(vt(
        "vehicles/truck.obj",
        2349,
        2,
        [250, -150, 2, 4, 15, 14, 12000, 100],
        id::ENGINE_BUS,
        id::CAR_HORN,
        12,
        Bus,
    )),
    vt(
        "vehicles/policevan.obj",
        2350,
        1,
        [300, -150, 6, 15, 40, 25, 2500, 50],
        id::ENGINE,
        id::SIREN,
        -1,
        Police,
    ),
    beeps(vt(
        "vehicles/bombtruck.obj",
        2351,
        -1,
        [200, -100, 4, 15, 20, 25, 2500, 0],
        id::ENGINE_BUS,
        id::CAR_HORN,
        16,
        Bus,
    )),
    vt(
        "vehicles/remotecontrolledcar.obj",
        2352,
        -1,
        [400, -400, 20, 45, 65, 70, 5000, 0],
        id::RC_ENGINE,
        id::CAR_HORN,
        16,
        Default,
    ),
    vt(
        "vehicles/boat.obj",
        2353,
        -1,
        [300, -200, 20, 45, 40, 5, 10000, 0],
        id::BOAT_ENGINE,
        id::CAR_HORN,
        8,
        Without,
    ),
    vt(
        "vehicles/rocketcar.obj",
        2354,
        -1,
        [2000, -100, 40, 30, 43, 40, 2500, 0],
        id::FLAMETHROWER,
        id::CAR_HORN,
        12,
        Default,
    ),
    vt(
        "vehicles/rocketcar_thrust1.obj",
        2354,
        -1,
        [2000, -100, 40, 30, 43, 40, 2500, 0],
        id::FLAMETHROWER,
        id::CAR_HORN,
        12,
        Default,
    ),
    vt(
        "vehicles/rocketcar_thrust2.obj",
        2354,
        -1,
        [2000, -100, 40, 30, 43, 40, 2500, 0],
        id::FLAMETHROWER,
        id::CAR_HORN,
        12,
        Default,
    ),
    vt(
        "vehicles/vapour racer.obj",
        2355,
        -1,
        [500, -240, 16, 35, 50, 28, 1800, 0],
        id::ENGINE_904,
        id::CAR_HORN,
        5,
        Default,
    ),
];

/// Type ids with special handling.
pub mod kind {
    pub const POLICE: usize = 9;
    pub const TANK: usize = 11;
    pub const HELICOPTER: usize = 17;
    pub const POLICE_VAN: usize = 20;
    pub const BOMB_TRUCK: usize = 21;
    pub const RC_CAR: usize = 22;
    pub const BOAT: usize = 23;
    pub const ROCKET_CAR: usize = 24;
    pub const BUS: usize = 3;
}

/// A vehicle object.
#[derive(Component, Clone, Debug)]
pub struct Vehicle {
    /// Index into [`TYPES`].
    pub kind: usize,
    /// Paint colour index (Cardef.* variants).
    pub colour: i32,
    /// Looping engine sound.
    pub engine: Option<Entity>,
    /// Helicopter in the air.
    pub flying: bool,
    /// Player controls enabled (`+0x49c`, default on).
    pub controls: bool,
}

/// Spawn a parked vehicle of type `kind` at `pos` (original units).
pub fn spawn_vehicle_world(
    world: &mut World,
    kind: i32,
    pos: IVec3,
    heading: i32,
    colour: i32,
) -> Option<Entity> {
    let t = TYPES.get(usize::try_from(kind).ok()?)?;
    let data = world.resource::<GameData>().clone();
    let kind = kind as usize;
    let (model, mesh, material, frames) =
        world.resource_scope(|world, mut library: Mut<ObjectLibrary>| {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            let mut setup = |library: &mut ObjectLibrary, kind: usize| {
                let t = &TYPES[kind];
                let model = library.load(&data, &mut meshes, t.model)?;
                let m = library.model_mut(model);
                // The model record is shared and set once (the original's
                // loader, Mac `InitStructures` @ 1005ffb0); the default
                // mass marks a fresh one. The helicopter's half-width is a
                // third of its max |X| (rotor), its door at −that.
                if m.info.mass != t.mass && kind == kind::HELICOPTER {
                    m.info.radius /= 3;
                }
                m.info.mass = t.mass;
                m.info.police = t.police;
                m.info.code = if kind == kind::TANK { 0 } else { 0xff };
                Some(model)
            };
            let model = setup(&mut library, kind)?;
            // Rocket car: the thrust frames swapped in while accelerating
            // (`do_Other` @ 10146280).
            let frames = if (kind::ROCKET_CAR..=kind::ROCKET_CAR + 2).contains(&kind) {
                let f: Option<Vec<usize>> =
                    (0..3).map(|i| setup(&mut library, kind::ROCKET_CAR + i)).collect();
                f.map(|f| [f[0], f[1], f[2]])
            } else {
                None
            };
            Some((
                model,
                library.model(model).mesh.clone(),
                library.material.clone(),
                frames,
            ))
        })?;
    let mut obj = Obj::new(Class::Vehicle, model, pos, heading);
    obj.state = State::Parked;
    let engine = world
        .spawn(Sound::looping(
            t.engine,
            10_000_000.0,
            6000.0,
            SoundPos::At(pos.as_vec3()),
        ))
        .id();
    Some(
        world
            .spawn((
                obj,
                Vehicle {
                    kind,
                    colour,
                    engine: Some(engine),
                    flying: false,
                    controls: true,
                },
                crate::sim::driving::Drive {
                    frames,
                    ..std::default::Default::default()
                },
                crate::sim::Stick::default(),
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(object_to_render(pos.as_vec3()))
                    .with_rotation(heading_to_render(heading as f32)),
                LevelEntity,
            ))
            .id(),
    )
}

/// Helicopter take-off / landing (script `0xb4666` / `0xb46dc`).
pub fn set_flying(world: &mut World, e: Entity, up: bool) {
    if let Some(mut v) = world.get_mut::<Vehicle>(e) {
        v.flying = up;
    }
    if !up {
        if let Some(mut o) = world.get_mut::<Obj>(e) {
            o.speed = 0;
        }
    }
}

/// Enable or disable the player controls of a vehicle (`+0x49c`).
pub fn set_controls(world: &mut World, e: Entity, on: bool) {
    if let Some(mut v) = world.get_mut::<Vehicle>(e) {
        v.controls = on;
    }
}

/// Special objects created by scripts (model >= 1000; table `0x30c3c0`):
/// barrier, cone, dummy, bullet. (path, mass)
pub const PROPS: [(&str, i32); 4] = [
    ("misc/barrier.obj", 50),
    ("misc/cone.obj", 20),
    ("misc/dummy.obj", 115),
    // Mass 10 (Mac `InitStructures` @ 1005ffb0 table `toc[-0x1b92]`).
    ("weapons/bullet.obj", 10),
];

/// Spawn a static prop (special object `index`).
pub fn spawn_prop_world(world: &mut World, index: i32, pos: IVec3, heading: i32) -> Option<Entity> {
    let (path, mass) = *PROPS.get(usize::try_from(index).ok()?)?;
    let data = world.resource::<GameData>().clone();
    let (model, mesh, material) =
        world.resource_scope(|world, mut library: Mut<ObjectLibrary>| {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            let model = library.load(&data, &mut meshes, path)?;
            let info = &mut library.model_mut(model).info;
            // The dummy's half-width is halved, once per (shared) model
            // record: a fresh one still has the default mass (Mac
            // `InitStructures` @ 1005ffb0).
            if index == 2 && info.mass != mass {
                info.radius >>= 1;
            }
            info.mass = mass;
            Some((
                model,
                library.model(model).mesh.clone(),
                library.material.clone(),
            ))
        })?;
    let mut obj = Obj::new(crate::sim::Class::Projectile, model, pos, heading);
    obj.state = State::Barrier;
    Some(
        world
            .spawn((
                obj,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(object_to_render(pos.as_vec3()))
                    .with_rotation(heading_to_render(heading as f32)),
                LevelEntity,
            ))
            .id(),
    )
}
