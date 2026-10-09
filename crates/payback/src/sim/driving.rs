//! Vehicle driving physics, after the Mac per-object update
//! (`object_MainLoop` @ 1014a670): `MoveObject_Vehicle` @ 1006e890 (gravity,
//! the move, `CheckForCubeCollision` @ 100af980, `BounceUnit` @ 10082bc0),
//! then the vehicle part of `do_Other` @ 10146280 (controls, throttle,
//! `PositionAirborneUnit` @ 10082620 or the ground step with steering and
//! `PositionUnit` @ 10081450, then `SkidUnit` @ 10081830), then the
//! scenery test (`AddCollideObj` @ 10055c70). The Amiga builds have the
//! same code: 68k `FUN_000e3a58` (vehicle branch), `FUN_000a197a`,
//! `FUN_00044f82`, `FUN_000a1758`, `FUN_000a241c`; OS4 `FUN_0105f594`
//! (`analysis/vehicles_os4/main.c`), `FUN_0105b9ec`, `FUN_01070868`,
//! `FUN_010cd198`, `FUN_010cbc7c`, `FUN_010cc288`, `FUN_010ccc5c`.
//! docs/re/vehicles.md §2 and analysis/mac/driving_review.md.
//!
//! Per tick: the car moves by last tick's velocity (gravity first), hits
//! walls and bounces; then, from the virtual stick, throttle and brake
//! ramp the speed (in the air they only rev the wheels); on the ground the
//! yaw rate ramps, the heading eases toward the nearest 45°, the body
//! takes the ground's roll and pitch, and the velocity converges on
//! `speed·(sin, cos)` by a surface-dependent grip, the remaining slip
//! driving skid marks, smoke and the skid sound.

use super::collide;
use super::combat::{Arsenal, Pickup, PickupRespawns, break_crate_at};
use super::people::PlayerInputs;
use super::{Obj, Rng, State, Stick, Trig, wrap};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::debris::Parts;
use crate::effects::{Blend, Effects, Tex};
use crate::hud::{BigTitle, Texts, labels::Labels};
use crate::mapedit::MapEdit;
use crate::mission::{Triggers, kind};
use crate::objects::{ObjectLibrary, SetScenery};
use crate::vehicles::{TYPES, Vehicle, VehicleType, kind as vk};
use crate::world::{City, heading_to_render, object_to_render};
use bevy::prelude::*;
use payback_formats::map::{self, MAP_SIZE};

/// Driving state of a vehicle beyond the core object (Mac `Object`
/// offsets in brackets).
#[derive(Component, Default, Clone, Debug)]
pub struct Drive {
    /// Current drive force, ramping by 5 per tick (`+0x80c`).
    pub force: i32,
    /// Braking against the direction of travel stops at zero (`+0x938`).
    pub moving: bool,
    /// Handbrake turn this tick (`+0x8a3`): grip 4 and no steering.
    pub j_turn: bool,
    /// Steering lean (`+0x7e8`), ±256·speed/max; added to the roll target.
    pub lean: i32,
    /// Throttle dip (`+0x7ec`), −128..128; added to the pitch target.
    pub dip: i32,
    /// Wheel speed (`+0x894`): the speed on the ground, revved freely by
    /// the throttle in the air; drives the engine pitch.
    pub wheel_speed: i32,
    /// The steering of the last tick on the ground (`+0x804`); a car in
    /// the air keeps turning with it.
    pub last_steer: i32,
    /// Boat bob: roll and pitch offsets (`+0x7fc`, `+0x800`) and their
    /// rates (`+0x7f4`, `+0x7f8`; −25 and −11 at creation).
    pub bob: [i32; 4],
    /// Splashed down in water (`+0x936`): the splash plays once.
    pub sunk: bool,
    /// In the air this tick (z above the ground, `+0x8c8`).
    pub airborne: bool,
    /// Last skid-mark point of each wheel (`+0x86c..+0x878` x,
    /// `+0x87c..+0x888` y): rear/front of the −half-width side, then of
    /// the +half-width side. None: no trail.
    pub trail: [Option<IVec2>; 4],
    /// Slip 0..255 (510 in water) of the last ground tick: the skid sound.
    pub slip: i32,
    /// The skid sound plays (not tracked, not in water).
    pub skid_audible: bool,
    /// Door opening angle 0..0x370 (`+0x418`).
    pub door: i32,
    /// Dented by a hard hit (>= 11 damage); never repaired.
    pub dented: bool,
    /// Ground heights under the 3×3 box points (`+0x8b8..+0x8d8`): rows
    /// left (−half-width), centre line, right; each rear, centre, front.
    /// Set by the collision step when nothing was hit; `heights[4]` is the
    /// ground z `+0x8c8`.
    pub heights: [i32; 9],
    /// The centre's cube level (`+0x8b4`; water, crates, pickups).
    pub centre_level: usize,
    /// The last position without a wall hit (`+0x8f0`, the start of the
    /// centre-path test) and the position the driving step left the car
    /// at; when something other than a contact push has moved the car
    /// since (a teleport), its position is used instead.
    pub clear: Option<(IVec3, IVec3)>,
    /// Rocket car: library models of rocketcar, _thrust1, _thrust2 (types
    /// 24-26), swapped while accelerating.
    pub frames: Option<[usize; 3]>,
    /// First tick done (heights read, boat bob set).
    pub init: bool,
    pub skid_sound: Option<Entity>,
    pub beep_sound: Option<Entity>,
    pub clonk: bool,
    /// The looped horn (`+0x918`, Mac `DoVehicleFire` @ 10070c20); see
    /// [`super::vehicle_ai::horn`].
    pub horn: Option<Entity>,
    /// Honked this tick (the original's `_DAT_1017cc70`, cleared at the
    /// start of each object's update).
    pub honked: bool,
}

/// The vehicle update chain.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivingSet;

pub struct DrivingPlugin;

impl Plugin for DrivingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                player_vehicle_stick,
                (drive_test_setup, drive_test_stick)
                    .chain()
                    .run_if(|| crate::arg("--drive-test").is_some()),
                drive,
                horns,
                vehicle_sounds,
            )
                .chain()
                .in_set(DrivingSet)
                .after(super::people::PeopleSet)
                .before(super::combat::CombatSet),
        )
        .add_systems(Update, sync_vehicles);
    }
}

/// The player's controls drive the vehicle they're in (Mac `do_Other` @
/// 10146280, state 2): accelerate → throttle −1, reverse → +1; the brake
/// key sets the throttle against the motion (+1 moving forward, −1
/// reversing, unchanged when stopped), so it brakes to a stop and never
/// reverses; left → steer −1, right → +1.
fn player_vehicle_stick(
    inputs: Res<PlayerInputs>,
    mut q: Query<(&Obj, &Vehicle, &mut Stick), With<CameraTarget>>,
) {
    for (obj, veh, mut stick) in &mut q {
        if obj.state != State::Player {
            continue;
        }
        let input = &inputs.0[obj.player.unwrap_or(0).min(3) as usize];
        if !veh.controls {
            *stick = Stick::default();
            continue;
        }
        stick.steer = if input.left {
            -1
        } else if input.right {
            1
        } else {
            0
        };
        stick.throttle = if input.accel {
            -1
        } else if input.reverse {
            1
        } else {
            0
        };
        if input.brake {
            if obj.speed > 0 {
                stick.throttle = 1;
            } else if obj.speed < 0 {
                stick.throttle = -1;
            }
        }
        stick.brake = input.brake;
        stick.fire = input.fire;
    }
}

/// `--drive-test x,y,heading[,kind[,steer[,ticks]]]` (world units, angle
/// units): once the player is on foot, put them in a new vehicle of `kind`
/// (default 0) there, on the ground, and hold the accelerator (and the
/// steering, −1/0/1) for `ticks` (default: always), then brake to a stop.
/// For checking acceleration, top speed, braking, turning, walls, kerbs,
/// slopes and crates; every tick is logged.
fn drive_test_setup(world: &mut World, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(spec) = crate::arg("--drive-test") else { return };
    let v: Vec<i32> = spec.split(',').filter_map(|k| k.trim().parse().ok()).collect();
    let (Some(&x), Some(&y), Some(&h)) = (v.first(), v.get(1), v.get(2)) else { return };
    let player = world
        .query_filtered::<(Entity, &Obj), (With<CameraTarget>, With<super::people::Person>)>()
        .iter(world)
        .next()
        .map(|(e, o)| (e, o.player));
    let Some((pe, pl)) = player else { return };
    let Some(z) = world
        .get_resource::<City>()
        .and_then(|c| collide::floor(c, x, y, collide::PLANE * 16))
        .map(|f| f.0)
    else {
        return;
    };
    *done = true;
    let kind = v.get(3).copied().unwrap_or(0);
    let Some(car) = crate::vehicles::spawn_vehicle_world(world, kind, IVec3::new(x, y, z), wrap(h), 1)
    else {
        return;
    };
    if let Some(mut o) = world.get_mut::<Obj>(car) {
        o.state = State::Player;
        o.player = Some(pl.unwrap_or(0));
    }
    world.entity_mut(car).insert((CameraTarget, Arsenal::default()));
    world.entity_mut(pe).despawn();
}

fn drive_test_stick(
    mut q: Query<(&Obj, &Drive, &mut Stick), (With<CameraTarget>, With<Vehicle>)>,
    mut ticks: Local<i32>,
) {
    let field = |n: usize| {
        crate::arg("--drive-test").and_then(|s| s.split(',').nth(n).and_then(|k| k.trim().parse::<i32>().ok()))
    };
    let (steer, limit) = (field(4).unwrap_or(0), field(5).unwrap_or(i32::MAX));
    for (o, d, mut stick) in &mut q {
        if o.state == State::Player {
            *ticks += 1;
            let on = *ticks <= limit;
            // Then brake (reverse throttle) to a stop.
            stick.throttle = if on { -1 } else if o.speed > 0 { 1 } else { 0 };
            stick.steer = if on { steer } else { 0 };
            info!(
                "drive-test: t {} pos {} {} {} vel {} {} {} roll {} pitch {} heading {:#x} speed {} yaw {} slip {} health {} air {}",
                *ticks,
                o.pos.x,
                o.pos.y,
                o.pos.z,
                o.vel.x,
                o.vel.y,
                o.vel.z,
                angle_signed(o.roll),
                angle_signed(o.pitch),
                o.heading,
                o.speed,
                o.turn_rate,
                d.slip,
                o.health,
                d.airborne,
            );
        }
    }
}

/// `SkidUnit` @ 10081830 grip per surface flag of the top texture under the
/// car (`.CubeTextureList` first column; rendering.md §8), from the base
/// grip `g`; grass, sand and snow also bleed the speed toward a limit by
/// at most 3, 20 and 10 per tick (68k `FUN_000a1758`, OS4 `FUN_010cc288`).
fn grip_for(surface: u32, g: i32, speed: &mut i32) -> i32 {
    let mut bleed = |limit: i32, rate: i32| *speed -= (*speed - limit).min(rate).max(0);
    match surface {
        0x22 => {
            bleed(450, 3);
            g >> 1
        }
        0x24 => {
            bleed(350, 20);
            g << 1
        }
        0x32 => {
            bleed(200, 10);
            4
        }
        0x33 => g * 10 / 25,
        0x34 => 1,
        _ => g * 4 / 5,
    }
}

/// Surface flag under the car (`GetSampleNum` of the top-face texture of
/// the square under the centre on the car's level `+0x8b0`); 4 off the
/// map.
fn surface_flag(city: &City, data: &GameData, obj: &Obj) -> u32 {
    if !(0..=collide::WORLD_MAX).contains(&obj.pos.x) || !(0..=collide::WORLD_MAX).contains(&obj.pos.y) {
        return 4;
    }
    let (sx, sy) = ((obj.pos.x >> 11) as usize, (obj.pos.y >> 11) as usize);
    if sx >= MAP_SIZE || sy >= MAP_SIZE || obj.level > 2 {
        return 4;
    }
    let top = city.map.square(sx, sy).cubes[obj.level].faces[map::TOP];
    data.texture_flags(top.texture).unwrap_or(4)
}

/// Effective top speed: damaged vehicles slow down (`eff = clamp(health ·
/// max / 50, max / 4, max)`; `do_Other` @ 10146280 after the throttle).
fn effective_max(t: &VehicleType, health: i32) -> i32 {
    (health * t.max_speed / 50).clamp(t.max_speed / 4, t.max_speed)
}

fn effective_reverse(t: &VehicleType, health: i32) -> i32 {
    let r = -t.max_reverse;
    (health * r / 50).clamp(r / 4, r)
}

/// Effects and sounds a vehicle step spawns, with the gameplay `random`.
struct Fx<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    effects: &'a mut Effects,
    rng: &'a mut Rng,
}

impl Fx<'_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, kind: u8, tex: Tex, p: IVec3, v: IVec3, size: i32, layer: usize, blend: Blend) {
        self.effects.add(kind, tex, p.as_vec3(), v.as_vec3(), size as f32, layer as u8, blend);
    }

    /// Bus passengers scream (sound 0x4c, volume 0xff, pitch
    /// `15000 + random(2000)`, non-positional).
    fn screams(&mut self) {
        let pitch = 15000 + self.rng.rand(2000);
        self.commands.spawn(Sound::once(id::SCREAMS, 255.0, pitch as f32, SoundPos::Global));
    }
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    data: Res<GameData>,
    city: Option<ResMut<City>>,
    trig: Res<Trig>,
    library: Option<Res<ObjectLibrary>>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut labels: ResMut<Labels>,
    mut respawns: ResMut<PickupRespawns>,
    mut debris: ResMut<Parts>,
    mut triggers: ResMut<Triggers>,
    mut edits: MessageWriter<MapEdit>,
    mut set_scenery: MessageWriter<SetScenery>,
    mut rng: ResMut<Rng>,
    mut effects: ResMut<Effects>,
    mut q: Query<(
        Entity,
        &mut Obj,
        &mut Vehicle,
        &mut Drive,
        &Stick,
        Option<&mut Arsenal>,
    )>,
) {
    let (Some(mut city), Some(library)) = (city, library) else { return };
    let mut glass = Vec::new();
    // Snowmen knocked down this tick (the original clears the cell at
    // once, so later circles and vehicles no longer meet it).
    let mut snowmen: Vec<(usize, usize, usize)> = Vec::new();
    for e__ in crate::sim::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, veh, mut d, stick, mut ars)) = q.get_mut(e__) else { continue };
        // State 6 objects are skipped (`object_MainLoop` @ 1014a670).
        if obj.state == State::Removed {
            continue;
        }
        let t = &TYPES[veh.kind];
        let extents = |model: usize| {
            let info = &library.model(model).info;
            (info.radius.max(1), info.max_y, info.min_y)
        };
        let (hw, front, rear) = extents(obj.model);
        // Model flags: `+0x8184` tracked (tank, helicopter, boat: turns on
        // the spot, no skid marks or sound, no J-turn), `+0x8186` flies,
        // `+0x8188` boat (also `+0x818a`, the inverted lean).
        let tracked = matches!(veh.kind, vk::TANK | vk::HELICOPTER | vk::BOAT);
        let flies = veh.kind == vk::HELICOPTER;
        let boat = veh.kind == vk::BOAT;
        let player_state = obj.state == State::Player;
        if !d.init {
            d.init = true;
            let bp = collide::box_points(&trig, obj.pos, obj.roll, obj.pitch, obj.heading, hw, front, rear);
            let g = box_heights(&city, &bp, obj.pos.z);
            d.heights = g.heights;
            d.centre_level = g.centre_level;
            obj.level = g.level;
            obj.floor_z = g.heights[4];
            obj.in_water = g.water;
            if boat {
                // `CreateObject` @ 10075630.
                d.bob = [0, 0, -25, -11];
            }
        }
        let start = obj.pos;

        // ---- `MoveObject_Vehicle` @ 1006e890 (one sub-step, the last) ----
        // Helicopter lift with the engine on, then gravity unless the car
        // sits in water.
        if veh.flying && obj.pos.z > 0 && obj.vel.z > -0x100 {
            obj.vel.z -= 0xaa;
        }
        if !obj.in_water || !obj.on_ground {
            obj.vel.z += 0x20;
            obj.pos.z += obj.vel.z;
        }
        obj.pos.x += obj.vel.x;
        obj.pos.y += obj.vel.y;
        // `CheckForCubeCollision` @ 100af980, vehicle branch (vehicles.md
        // §2.7): the box edges and the centre path from the last clear
        // position.
        let clear = match d.clear {
            Some((c, left)) if left == start => c,
            _ => start,
        };
        let bp = collide::box_points(&trig, obj.pos, obj.roll, obj.pitch, obj.heading, hw, front, rear);
        let hit = collide::vehicle_walls(&city, &bp, obj.pos, clear, &mut glass);
        let clear_now = !hit.x && !hit.y;
        let clear_pos = if clear_now { obj.pos } else { clear };
        if clear_now {
            // The ground under the 9 box points (read by `PositionUnit`,
            // the ground step and the scenery test), the levels and the
            // water flag; then per point, while the car is near the
            // ground, the player's car collects pickups (`CheckForWeapon`
            // @ 100597e0) and any car smashes crates (`CheckForCrate` @
            // 100593e0), both on the centre's level. A hit keeps all of
            // it from the last clear step.
            let g = box_heights(&city, &bp, obj.pos.z);
            d.heights = g.heights;
            d.centre_level = g.centre_level;
            obj.level = g.level;
            obj.floor_z = g.heights[4];
            obj.in_water = g.water;
            let level = g.centre_level;
            if d.heights[4] - 0x200 < obj.pos.z {
                for (i, p) in bp.pts.iter().enumerate() {
                    let (sx, sy) = (p.x >> 11, p.y >> 11);
                    if player_state && let Some(ars) = ars.as_deref_mut() {
                        let mut pick = Pickup {
                            commands: &mut commands,
                            texts: texts.as_deref(),
                            labels: &mut labels,
                            respawns: &mut respawns,
                            set: &mut set_scenery,
                            rng: &mut rng,
                            effects: &mut effects,
                        };
                        if pick
                            .check_for_weapon(&city, &obj, ars, sx, sy, level, d.heights[i])
                            .is_some()
                        {
                            city.map.squares[sx as usize * MAP_SIZE + sy as usize].cubes[level]
                                .scenery = 0;
                        }
                    }
                    let at = IVec3::new(p.x, p.y, d.heights[i]);
                    if let Some(contents) =
                        break_crate_at(&city, at, level, &mut set_scenery, &mut debris)
                    {
                        // At once, as the original writes the cell: the
                        // scenery test below and later vehicles see it.
                        city.map.squares[sx as usize * MAP_SIZE + sy as usize].cubes[level]
                            .scenery = contents;
                    }
                }
            }
        }
        // A hit that opposes the velocity turns the heading by 0x5a, away
        // from the wall (tail of `CheckForCubeCollision`; all builds).
        obj.heading = deflect(obj.heading, hit.side, obj.vel.xy());
        // `BounceUnit` @ 10082bc0 (OS4 `FUN_010cd198`) per blocked axis:
        // damage v²>>14, Thud or Thud2 (dented; a bus screams for its
        // player), half the normal velocity kept pointing away from the
        // wall, and the car pushed off by it plus 0x20.
        for (axis_hit, v) in [(hit.x, obj.vel.x), (hit.y, obj.vel.y)] {
            if !axis_hit {
                continue;
            }
            let dmg = (v * v) >> 14;
            // God mode (`crate::cheats`): no damage.
            if !obj.god {
                obj.health -= dmg;
            }
            let at = SoundPos::At(obj.posf());
            let vol = obj.speed.abs() as f32 * 10000.0;
            if dmg < 11 {
                commands.spawn(Sound::once(id::THUD, vol, 12000.0, at));
            } else {
                commands.spawn(Sound::once(id::THUD2, vol, 8000.0, at));
                d.dented = true;
                if veh.kind == vk::BUS && player_state {
                    let pitch = 15000 + rng.rand(2000);
                    commands.spawn(Sound::once(id::SCREAMS, 255.0, pitch as f32, SoundPos::Global));
                }
            }
        }
        if hit.x {
            obj.vel.x = bounce(hit.side.x, obj.vel.x);
            obj.pos.x += obj.vel.x + if hit.side.x < 1 { -0x20 } else { 0x20 };
        }
        if hit.y {
            obj.vel.y = bounce(hit.side.y, obj.vel.y);
            obj.pos.y += obj.vel.y + if hit.side.y < 1 { -0x20 } else { 0x20 };
        }

        // ---- `do_Other` @ 10146280, vehicle part ----
        let (mut steer, mut throttle) = (stick.steer, stick.throttle);
        if matches!(obj.state, State::Parked | State::Dead) || obj.burn > 0 {
            steer = 0;
            throttle = 0;
        }
        // The helicopter takes no input with the engine off.
        if flies && !veh.flying {
            steer = 0;
            throttle = 0;
        }
        // In water only boats and the helicopter are driven; a boat out of
        // the water is not.
        if (obj.in_water && !boat && !flies) || (boat && !obj.in_water) {
            steer = 0;
            throttle = 0;
        }
        if throttle == 0 {
            d.force = 0;
        }
        d.j_turn = false;
        let airborne = obj.pos.z < obj.floor_z;
        let j_turn_ok = !tracked && steer != 0;
        // `UseHandbrake` (mission trigger 0x27) for the first player only.
        let handbrake = |triggers: &mut Triggers, obj: &Obj| {
            if player_state && obj.player.unwrap_or(0) == 0 {
                triggers.report(kind::HANDBRAKE, 0, 0, 0);
            }
        };
        let mut fx = Fx {
            commands: &mut commands,
            effects: &mut effects,
            rng: &mut rng,
        };

        // Throttle forward.
        if throttle < 0 {
            if airborne && !flies {
                // The wheels spin up in the air (sic: `+0x814c` brake when
                // the wheels turn backwards, else the acceleration).
                let a = if d.wheel_speed < 1 { t.brake } else { t.accel };
                d.force = a;
                d.wheel_speed += (4 * a).min(2 * t.max_speed - d.wheel_speed);
            } else if obj.speed < 0 {
                // Braking out of reverse; steering spins it (J-turn).
                if j_turn_ok {
                    handbrake(&mut triggers, &obj);
                    d.j_turn = true;
                    d.force = 0;
                    obj.turn_rate = if steer < 0 { -2 * t.turn } else { 2 * t.turn };
                }
                d.force = (d.force.max(0) + 5).min(t.brake);
                obj.speed += d.force.min(t.max_speed - obj.speed);
                if obj.speed >= 1 && d.moving {
                    obj.speed = 0;
                    d.moving = false;
                } else {
                    d.moving = obj.speed != 0;
                }
                brake_smoke(&mut fx, &trig, &obj, hw, front, rear);
            } else {
                d.force = (d.force.max(0) + 5).min(t.accel);
                obj.speed += d.force.min(t.max_speed - obj.speed);
                d.moving = true;
            }
        } else if obj.speed > 0 {
            obj.speed -= obj.speed.min(1);
        }
        if throttle < 1 {
            if obj.speed < 0 {
                obj.speed += (-obj.speed).min(1);
            }
        } else if airborne && !flies {
            let a = if d.wheel_speed < 1 { t.accel } else { t.brake };
            d.force = -a;
            d.wheel_speed += (-4 * a).max(2 * t.max_reverse - d.wheel_speed);
        } else if obj.speed < 1 {
            d.force = (d.force.min(0) - 5).max(-t.accel);
            obj.speed += d.force.max(t.max_reverse - obj.speed);
            d.moving = true;
        } else {
            // Braking forward motion; steering spins it.
            if j_turn_ok {
                handbrake(&mut triggers, &obj);
                d.j_turn = true;
                d.force = 0;
                obj.turn_rate = if steer < 0 { 2 * t.turn } else { -2 * t.turn };
            }
            d.force = (d.force.min(0) - 5).max(-t.brake);
            obj.speed += d.force.max(t.max_reverse - obj.speed);
            if obj.speed < 0 && d.moving {
                obj.speed = 0;
                d.moving = false;
            } else {
                d.moving = obj.speed != 0;
            }
            brake_smoke(&mut fx, &trig, &obj, hw, front, rear);
        }
        // Damage limits the top speed both ways.
        if obj.speed < 0 {
            obj.speed = obj.speed.max(-effective_reverse(t, obj.health));
        } else {
            obj.speed = obj.speed.min(effective_max(t, obj.health));
        }
        // Rocket car: thrust1 and thrust2 alternate while accelerating.
        if let Some(f) = d.frames {
            if throttle < 0 {
                if obj.model == f[1] {
                    obj.model = f[2];
                } else if obj.model == f[2] || obj.model == f[0] {
                    obj.model = f[1];
                }
            } else if obj.model == f[1] || obj.model == f[2] {
                obj.model = f[0];
            }
        }
        let (hw, front, rear) = extents(obj.model);

        d.airborne = airborne;
        if airborne {
            position_airborne_unit(&mut obj, &mut d, t, &trig, flies, steer);
        } else {
            d.wheel_speed = obj.speed;
            if !obj.on_ground {
                // Landing.
                if obj.vel.z < 0xa1 {
                    obj.vel.z = 0x80;
                } else {
                    let at = SoundPos::At(obj.posf() + Vec3::Z * 100.0);
                    fx.commands.spawn(Sound::once(
                        id::HIT_SCENERY,
                        obj.vel.z.abs() as f32 * 20000.0,
                        15000.0,
                        at,
                    ));
                    obj.vel.z = 0x80 - (obj.vel.z >> 2).abs();
                }
                obj.on_ground = true;
                obj.pos.z = obj.floor_z;
                d.sunk = false;
            } else if !obj.in_water {
                // Follow the ground: vz keeps the climb, so a car leaving
                // a ramp flies on.
                obj.vel.z = (obj.floor_z + 0x80 - obj.pos.z).clamp(-1000, 1000);
                obj.pos.z = obj.floor_z;
                d.sunk = false;
            } else if !d.sunk {
                // Splashed down (`ObjectSunk`); the RC car dies.
                if veh.kind == vk::RC_CAR {
                    obj.health = -71;
                }
                triggers.report_actor(kind::ACTOR_IN_WATER, e);
                d.sunk = true;
                let at = SoundPos::At(obj.posf());
                fx.commands.spawn(Sound::once(id::SPLASH, t.mass as f32 * 10000.0, 8350.0, at));
            }
            if !obj.in_water || boat {
                obj.pos.z = obj.floor_z;
                if boat && obj.in_water {
                    boat_bob(&mut fx, &trig, &obj, &mut d, start, rear, throttle);
                }
                ground_controls(&mut obj, &mut d, t, tracked, boat, steer, throttle);
                position_unit(&mut obj, &d, hw, front, rear);
            } else if sinking(&mut fx, &trig, &mut obj, hw, front, rear) {
                // Drowned (z 27000): a driving player loses the car and a
                // life ("Drowned!"); the car goes (state 6).
                if player_state {
                    if let Some(t) = &texts {
                        title.show_for(obj.player, t.global(2018));
                    }
                }
                triggers.report_actor(kind::ACTOR_KILLED, e);
                obj.state = State::Removed;
            }
            skid_unit(&mut fx, &city, &data, &trig, &mut obj, &mut d, veh.kind, start, hw, front, rear);
            // A wall hit stops the car (after the grip step, so the
            // bounce velocity stays and decays at the grip rate; OS4
            // main.c ~2503 does the same). The heights stay from before.
            if hit.x || hit.y {
                obj.speed = 0;
            }
        }

        // ---- Scenery (`AddCollideObj` @ 10055c70) ----
        // Pushed out by half the overlap and stopped; the push damages the
        // car, `(px² + py²) >> 12`, with Thud below 11, else Thud2 (dented;
        // a bus screams). A snowman is knocked down instead.
        let h = d.heights;
        let body = collide::Body {
            radius: hw,
            front,
            rear,
            vehicle: true,
            rc: veh.kind == vk::RC_CAR,
            dz: [h[4] - obj.pos.z, h[3] - obj.pos.z, h[5] - obj.pos.z, h[5] - obj.pos.z],
        };
        let (mut pos, mut vel) = (obj.pos, obj.vel);
        let bumps = collide::scenery_collide(
            &city, &library, &trig, &mut pos, &mut vel, obj.heading, obj.level, &body,
        );
        obj.pos = pos;
        obj.vel = vel;
        if !bumps.is_empty() {
            obj.speed = 0;
        }
        for b in bumps {
            if b.id == SNOWMAN {
                if snowmen.contains(&(b.sx, b.sy, b.level)) {
                    continue;
                }
                snowmen.push((b.sx, b.sy, b.level));
                // 5 type-1 `Dust` puffs (table 0, 600 + r(300)) from the
                // gameplay `random`, 7 draws each, last argument first: at
                // the square centre ± (100, 400), z = the front circle's
                // ground (+0x8c8) ± 150, v = (±150, ±50, ±50) / 3 (Mac
                // `AddCollideObj` @ 10055c70 at 100565f0, OS4
                // `FUN_01045288`).
                for _ in 0..5 {
                    let size = 600 + rng.rand(300);
                    let vz = rng.rand(100) - 50;
                    let vy = rng.rand(100) - 50;
                    let vx = rng.rand(300) - 150;
                    let z = h[4] + rng.rand(300) - 150;
                    let y = (b.sy * 2048 + 0x270) as i32 + rng.rand(800);
                    let x = (b.sx * 2048 + 0x39c) as i32 + rng.rand(200);
                    effects.add(
                        1,
                        Tex::Dust,
                        Vec3::new(x as f32, y as f32, z as f32),
                        Vec3::new(vx as f32, vy as f32, vz as f32),
                        size as f32,
                        obj.level as u8 + 1,
                        Blend::Add,
                    );
                }
                set_scenery.write(SetScenery {
                    sx: b.sx,
                    sy: b.sy,
                    level: b.level,
                    scenery: 0,
                    special: None,
                    rotation: None,
                });
                continue;
            }
            let dmg = (b.push.x * b.push.x + b.push.y * b.push.y) >> 12;
            let at = SoundPos::At(obj.posf());
            if dmg < 11 {
                commands.spawn(Sound::once(id::THUD, dmg as f32 * 100_000.0, 8000.0, at));
            } else {
                d.dented = true;
                commands.spawn(Sound::once(id::THUD2, dmg as f32 * 100_000.0, 8000.0, at));
                if veh.kind == vk::BUS {
                    commands.spawn(Sound::once(id::SCREAMS, 255.0, 15000.0, SoundPos::Global));
                }
            }
            // God mode (`crate::cheats`): no damage.
            if !obj.god {
                obj.health -= dmg;
            }
        }

        obj.prev_pos = start;
        d.clear = Some((clear_pos, obj.pos));
    }
    collide::smash_glass(&mut commands, &city, &mut edits, &mut triggers, &glass);
}

/// Scenery id of the snowman (`scenery/snowman.obj`, pickup code 0xfe).
const SNOWMAN: u8 = 26;

/// Signed angle in −0xb40..0xb40.
fn angle_signed(a: i32) -> i32 {
    super::angle_diff(0, a)
}

/// Move `v` toward `target` by at most `step`.
fn slew(v: i32, target: i32, step: i32) -> i32 {
    if v < target { v + (target - v).min(step) } else { v - (v - target).min(step) }
}

/// The ground part of `do_Other` @ 10146280 before `PositionUnit` (68k
/// `FUN_000e3a58` ~125460-125750, OS4 main.c ~2200-2490): the throttle
/// dip, the steering lean, the yaw rate, the heading and the snap to the
/// nearest 45°.
fn ground_controls(
    obj: &mut Obj,
    d: &mut Drive,
    t: &VehicleType,
    tracked: bool,
    boat: bool,
    steer: i32,
    throttle: i32,
) {
    // Dip `+0x7ec`: relaxes 16 per tick; accelerating below top speed
    // lifts the nose 16 more (floor −0x80), reversing above the reverse
    // limit dips it (cap 0x80). At one tick per frame the two balance at
    // ∓16.
    d.dip = slew(d.dip, 0, 16);
    if throttle < 0 && obj.speed < t.max_speed {
        d.dip = (d.dip - 16).max(-0x80);
    }
    if throttle > 0 && t.max_reverse < obj.speed {
        d.dip = (d.dip + 16).min(0x80);
    }
    d.last_steer = steer;
    // Lean `+0x7e8`: relaxes 32 per tick, then slews 64 per tick toward
    // `−steer·|speed|·256/max` (the other way on the boat).
    d.lean = slew(d.lean, 0, 32);
    if !d.j_turn {
        let lim = if tracked { t.turn } else { obj.speed.abs().min(t.turn) };
        if steer != 0 {
            // Steering left raises the heading going forward; reversing
            // flips it.
            let dir = if (steer < 0) == (obj.speed >= 0) { 1 } else { -1 };
            if obj.turn_rate.signum() == -dir {
                obj.turn_rate = 0;
            }
            obj.turn_rate = (obj.turn_rate + 5 * dir).clamp(-lim, lim);
            let mag = obj.speed.abs() * 256 / t.max_speed.max(1);
            let target = if boat { steer * mag } else { -steer * mag };
            d.lean = slew(d.lean, target, 64);
        } else {
            obj.turn_rate = 0;
        }
    }
    obj.heading = wrap(obj.heading + obj.turn_rate);
    // Ease toward the nearest 45° while moving (driving assist).
    if obj.speed != 0 {
        let snap = wrap((obj.heading + 0x168) / 0x2d0 * 0x2d0);
        let diff = super::angle_diff(obj.heading, snap);
        obj.heading = wrap(obj.heading + diff.clamp(-4, 4));
    }
}

/// `PositionAirborneUnit` @ 10082620 (OS4 `FUN_010ccc5c`, 68k
/// `FUN_000a241c`). A car in the air loses its skid trails and keeps
/// turning with the steering it had on the ground, at `min(|speed/2|,
/// yaw)`; its nose tips 16 per tick, down going forward until 0x5a0, up
/// otherwise until −0x5a0. The flying helicopter instead grips the air
/// with its own grip, ramps the yaw rate like a tracked vehicle, takes
/// pitch = speed and roll 0, and counts as level 2.
fn position_airborne_unit(obj: &mut Obj, d: &mut Drive, t: &VehicleType, trig: &Trig, flies: bool, steer: i32) {
    d.trail = [None; 4];
    obj.on_ground = false;
    d.sunk = false;
    if !flies {
        let r = (obj.speed >> 1).abs().min(t.turn);
        let turn = match d.last_steer {
            -1 => r,
            1 => -r,
            _ => 0,
        };
        obj.heading = wrap(obj.heading + if obj.speed < 1 { -turn } else { turn });
        if obj.speed < 1 {
            if obj.pitch > 0x10df || obj.pitch < 0xb40 {
                obj.pitch = wrap(obj.pitch - 16);
            }
        } else if obj.pitch < 0x5a0 || obj.pitch > 0xb40 {
            obj.pitch = wrap(obj.pitch + 16);
        }
        return;
    }
    d.wheel_speed = obj.speed;
    obj.level = 2;
    let tx = obj.speed * trig.sin(obj.heading) >> 8;
    let ty = obj.speed * trig.cos(obj.heading) >> 8;
    obj.vel.x = slew(obj.vel.x, tx, t.grip);
    obj.vel.y = slew(obj.vel.y, ty, t.grip);
    if steer != 0 {
        let dir = if (steer < 0) == (obj.speed >= 0) { 1 } else { -1 };
        if obj.turn_rate.signum() == -dir {
            obj.turn_rate = 0;
        }
        obj.turn_rate = (obj.turn_rate + 5 * dir).clamp(-t.turn, t.turn);
    } else {
        obj.turn_rate = 0;
    }
    obj.heading = wrap(obj.heading + obj.turn_rate);
    obj.pitch = wrap(obj.speed);
    obj.roll = 0;
}

/// Tyre smoke while braking (`do_Other` @ 10146280, both brake branches):
/// at each wheel, at 0.7 of the box extents, with chance 1/2 a type-1
/// `Dust` puff (table 0) at `+ random(300)` on each axis, velocity `v/2 +
/// random(50)`, size `min(|speed|·front >> 12, 1000)`, on the car's level.
fn brake_smoke(fx: &mut Fx, trig: &Trig, obj: &Obj, hw: i32, front: i32, rear: i32) {
    let s7 = trig.sin(obj.heading) * 70 / 100;
    let c7 = trig.cos(obj.heading) * 70 / 100;
    let size = (obj.speed.abs() * front >> 12).min(1000);
    for i in [-1, 1] {
        for j in [-1, 1] {
            let lon = if j == 1 { front } else { -rear };
            let x = obj.pos.x + (c7 * i * hw + s7 * lon >> 8);
            let y = obj.pos.y + (c7 * lon - s7 * i * hw >> 8);
            if fx.rng.rand(2) == 0 {
                let vz = fx.rng.rand(50);
                let vy = fx.rng.rand(50);
                let vx = fx.rng.rand(50);
                let rz = fx.rng.rand(300);
                let ry = fx.rng.rand(300);
                let rx = fx.rng.rand(300);
                let p = IVec3::new(x + rx, y + ry, obj.pos.z + rz);
                let v = IVec3::new((obj.vel.x >> 1) + vx, (obj.vel.y >> 1) + vy, (obj.vel.z >> 1) + vz);
                fx.add(1, Tex::Dust, p, v, size, obj.level, Blend::Add);
            }
        }
    }
}

/// Boat on water (`do_Other` @ 10146280): the bob, a spring on the roll and
/// pitch offsets (`+0x7fc += +0x7f4`, the rate ±2 toward 0; likewise
/// `+0x800`/`+0x7f8`), and with the throttle a wake of 2 type-8 `Dust`
/// puffs per tick behind the stern, interpolated from last tick's position
/// (`k·(pos − start)`, k = 1, 2), pushed back (forward when reversing) by
/// half the heading vector.
fn boat_bob(fx: &mut Fx, trig: &Trig, obj: &Obj, d: &mut Drive, start: IVec3, rear: i32, throttle: i32) {
    d.bob[0] += d.bob[2];
    d.bob[1] += d.bob[3];
    d.bob[2] += if d.bob[0] < 1 { 2 } else { -2 };
    d.bob[3] += if d.bob[1] < 1 { 2 } else { -2 };
    if throttle == 0 {
        return;
    }
    let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
    let back = if throttle < 0 { -1 } else { 1 };
    for k in 1..=2 {
        let size = 0x82 + fx.rng.rand(50);
        let vz = fx.rng.rand(10) - 5;
        let vy = fx.rng.rand(300);
        let vx = fx.rng.rand(300);
        let z = fx.rng.rand(200);
        let y = fx.rng.rand(100);
        let x = fx.rng.rand(100);
        let p = IVec3::new(
            start.x + k * (obj.pos.x - start.x) - (rear * s >> 8) + x - 50,
            start.y + k * (obj.pos.y - start.y) - (rear * c >> 8) + y - 50,
            obj.pos.z + z - 100,
        );
        let v = IVec3::new(
            obj.vel.x + vx - 150 + back * (s >> 1),
            obj.vel.y + vy - 150 + back * (c >> 1),
            vz,
        );
        fx.add(8, Tex::Dust, p, v, size, obj.level, Blend::Add);
    }
}

/// A non-boat in water (`do_Other` @ 10146280; 68k ~125750, OS4 main.c
/// ~1976): it stops, the nose rises 4 per tick to 0x5a0, and it sinks 32
/// per tick with bubbles (type 5, `Bubble`, at each of the 9 box points
/// with chance 1/5) until z 27000. Returns true when it is gone.
fn sinking(fx: &mut Fx, trig: &Trig, obj: &mut Obj, hw: i32, front: i32, rear: i32) -> bool {
    obj.speed = 0;
    if obj.pitch < 0x5a0 {
        obj.pitch += 4;
    }
    if obj.pos.z >= 27000 {
        return true;
    }
    obj.pos.z += 32;
    box_bubbles(fx.effects, fx.rng, trig, obj, obj.pos, hw, front, rear);
    false
}

/// The sinking bubbles of `do_Other` @ 10146280 and `do_Person` @ 1013fbd0
/// (identical loops; 68k `FUN_000e3a58` ~122510 and ~125766): for each of
/// the 9 box points (lateral `i·hw`, longitudinal front / 0 / −rear, turned
/// by roll, pitch and heading) with chance `random(5) == 1`,
/// `AddEffect(5, Bubble, p + random(500)−250 per axis, (random(10)−5,
/// random(10)−5, random(100)−300), 100 + random(150), level + 1, 0)`, all
/// from the gameplay `random`. `hw`, `front`, `rear` are the model's
/// `+0x8120/+0x8124/+0x8128` (the per-object dispatcher after `do_Other`
/// loads them for every object); `pos` is the position after this tick's
/// sink step.
#[allow(clippy::too_many_arguments)]
pub(super) fn box_bubbles(
    effects: &mut Effects,
    rng: &mut Rng,
    trig: &Trig,
    obj: &Obj,
    pos: IVec3,
    hw: i32,
    front: i32,
    rear: i32,
) {
    let (sr, cr) = (trig.sin(obj.roll), trig.cos(obj.roll));
    let (sp, cp) = (trig.sin(obj.pitch), trig.cos(obj.pitch));
    let (sh, ch) = (trig.sin(obj.heading), trig.cos(obj.heading));
    for i in -1..=1 {
        for j in -1..=1 {
            if rng.rand(5) != 1 {
                continue;
            }
            let lat = cr * i * hw >> 8;
            let up = sr * -i * hw >> 8;
            let l = match j {
                1 => front,
                -1 => -rear,
                _ => 0,
            };
            let fwd = cp * l - up * sp >> 8;
            let z = pos.z + (up * cp + sp * l >> 8);
            let y = pos.y + (fwd * ch - lat * sh >> 8);
            let x = pos.x + (lat * ch + fwd * sh >> 8);
            let size = 100 + rng.rand(150);
            let vz = rng.rand(100) - 300;
            let vy = rng.rand(10) - 5;
            let vx = rng.rand(10) - 5;
            let rz = rng.rand(500);
            let ry = rng.rand(500);
            let rx = rng.rand(500);
            let p = IVec3::new(x + rx - 250, y + ry - 250, z + rz - 250);
            let v = IVec3::new(vx, vy, vz);
            let layer = (obj.level + 1) as u8;
            effects.add(5, Tex::Bubble, p.as_vec3(), v.as_vec3(), size as f32, layer, Blend::Add);
        }
    }
}

/// `SkidUnit` @ 10081830 (OS4 `FUN_010cc288` + `FUN_010cc010`, 68k
/// `FUN_000a1758`), on the ground only. The velocity is pulled toward
/// `speed·(sin, cos)` (taken before the surface bleeds the speed) by the
/// surface grip per tick, from the model grip, or 4 in a J-turn. Then,
/// except for the helicopter, the slip `2·(|Δvx| + |Δvy|)` after the pull
/// (0..255, doubled in water) lays skid marks from each wheel's last point
/// (type 8, `DirtSkidmark` tinted on grass and sand, `Dust` added on snow,
/// slush and ice, else `Dust` subtracted, size 0xe6 or 100 for the RC car,
/// every 7/8 of it) unless tracked or in water, puffs smoke or dirt at the
/// wheels with chance 1/2 (spray in water), and sets the skid sound.
#[allow(clippy::too_many_arguments)]
fn skid_unit(
    fx: &mut Fx,
    city: &City,
    data: &GameData,
    trig: &Trig,
    obj: &mut Obj,
    d: &mut Drive,
    kind: usize,
    start: IVec3,
    hw: i32,
    front: i32,
    rear: i32,
) {
    let (s, c) = (trig.sin(obj.heading), trig.cos(obj.heading));
    let tx = s * obj.speed >> 8;
    let ty = c * obj.speed >> 8;
    let surface = surface_flag(city, data, obj);
    let base = if d.j_turn { 4 } else { TYPES[kind].grip };
    let r = grip_for(surface, base, &mut obj.speed);
    obj.vel.x = slew(obj.vel.x, tx, r);
    obj.vel.y = slew(obj.vel.y, ty, r);
    if kind == vk::HELICOPTER {
        return;
    }
    let mut slip = (((obj.vel.y - ty).abs() + (obj.vel.x - tx).abs()) * 2).clamp(0, 255);
    if obj.in_water {
        slip <<= 1;
    }
    let tracked = matches!(kind, vk::TANK | vk::HELICOPTER | vk::BOAT);
    d.slip = slip;
    d.skid_audible = !tracked && !obj.in_water;
    if slip < 1 {
        d.trail = [None; 4];
        return;
    }
    if kind == vk::BUS && obj.state == State::Player && d.trail[0].is_none() {
        fx.screams();
    }
    if kind == vk::BOAT {
        return;
    }
    let size = if kind == vk::RC_CAR { 100 } else { 0xe6 };
    let spacing = size * 7 >> 3;
    let (mark, mark_blend) = match surface {
        0x22 | 0x24 => (Tex::DirtSkidmark, Blend::Tint),
        0x32..=0x34 => (Tex::Dust, Blend::Add),
        _ => (Tex::Dust2, Blend::Subtract),
    };
    let level = obj.level;
    for (a, lat) in [-hw, hw].into_iter().enumerate() {
        for (b, lon) in [-rear, front].into_iter().enumerate() {
            let x = obj.pos.x + (c * lat + s * lon >> 8);
            let y = obj.pos.y + (c * lon - s * lat >> 8);
            if obj.in_water {
                if fx.rng.rand(2) == 0 {
                    let size = 500 + fx.rng.rand(500);
                    let (v, p) = puff_jitter(fx, obj, x, y, 2);
                    fx.add(1, Tex::Dust, p, v, size, level + 1, Blend::Add);
                }
                continue;
            }
            if !tracked {
                let idx = a * 2 + b;
                let from = d.trail[idx]
                    .unwrap_or(IVec2::new(start.x + (x - obj.pos.x), start.y + (y - obj.pos.y)));
                let (dx, dy) = (x - from.x, y - from.y);
                let n = dx.abs().max(dy.abs()) / spacing + 1;
                for k in 1..=n {
                    let p = IVec3::new(from.x + dx * k / n, from.y + dy * k / n, obj.pos.z);
                    fx.add(8, mark, p, IVec3::ZERO, size, level, mark_blend);
                }
                d.trail[idx] = Some(IVec2::new(x, y));
            }
            if fx.rng.rand(2) < 1 {
                let (tex, blend, shift) = match surface {
                    0x22 | 0x24 => (Tex::Dirt, Blend::Tint, 7),
                    0x32 => (Tex::Dust, Blend::Add, 7),
                    _ => (Tex::Dust, Blend::Add, 8),
                };
                let (v, p) = puff_jitter(fx, obj, x, y, 1);
                fx.add(1, tex, p, v, slip * hw >> shift, level, blend);
            }
        }
    }
}

/// The velocity `v >> shift + random(50)` and position `+ random(300)` of a
/// wheel puff, drawn last argument first as the original evaluates them.
fn puff_jitter(fx: &mut Fx, obj: &Obj, x: i32, y: i32, shift: i32) -> (IVec3, IVec3) {
    let vz = fx.rng.rand(50);
    let vy = fx.rng.rand(50);
    let vx = fx.rng.rand(50);
    let rz = fx.rng.rand(300);
    let ry = fx.rng.rand(300);
    let rx = fx.rng.rand(300);
    let v = IVec3::new((obj.vel.x >> shift) + vx, (obj.vel.y >> shift) + vy, (obj.vel.z >> shift) + vz);
    (v, IVec3::new(x + rx, y + ry, obj.pos.z + rz))
}

/// N7: a wall hit that opposes the velocity turns the heading by 0x5a
/// (tail of `CheckForCubeCollision` @ 100af980, 68k `FUN_00044f82`). For
/// x the turn is −0x5a with the heading in [1, 0x5a0] or [0xb41, 0x10e0]
/// and +0x5a otherwise; for y the signs swap.
fn deflect(heading: i32, side: IVec2, vel: IVec2) -> i32 {
    let opposes = |s: i32, v: i32| (s > 0 && v < 1) || (s < 0 && v >= 0);
    let in_quarters = |h: i32| (1..=0x5a0).contains(&h) || (0xb41..=0x10e0).contains(&h);
    let mut h = heading;
    if opposes(side.x, vel.x) {
        h = wrap(if in_quarters(h) { h - 0x5a } else { h + 0x5a });
    }
    if opposes(side.y, vel.y) {
        h = wrap(if in_quarters(h) { h + 0x5a } else { h - 0x5a });
    }
    h
}

/// `BounceUnit` @ 10082bc0: half the normal velocity, pointing away from
/// the wall (`side` +1: the wall is on the − side).
fn bounce(side: i32, v: i32) -> i32 {
    let half = (v >> 1).abs();
    if side < 1 { -half } else { half }
}

/// What a clear collision step reads under the box.
struct Ground {
    heights: [i32; 9],
    /// The highest cube level under the 9 points (`+0x8b0`): the car's
    /// level for the surface, the scenery and its effects.
    level: usize,
    /// The centre's level (`+0x8b4`).
    centre_level: usize,
    /// The centre's cube on that level has a water top (`+0x81a`).
    water: bool,
}

/// Ground heights under the 3×3 box points (the vehicle branch of
/// `CheckForCubeCollision` @ 100af980 after a step without a hit). The
/// centre uses the floor rule (`GetCubeInfo` @ 100ae3c0), the others the
/// surface nearest the car's height `z` (`GetCubeInfo2` @ 100ae580), so a
/// point over a low kerb reads the kerb, one over the open road the road.
fn box_heights(city: &City, b: &collide::BoxPoints, z: i32) -> Ground {
    let mut g = Ground {
        heights: [z; 9],
        level: 0,
        centre_level: 0,
        water: false,
    };
    for (i, p) in b.pts.iter().enumerate() {
        let (h, l) = if i == 4 {
            match collide::floor(city, p.x, p.y, z) {
                Some((f, l, water)) => {
                    g.centre_level = l;
                    g.water = water;
                    (f, l)
                }
                None => (z, 0),
            }
        } else {
            collide::ground_at(city, p.x, p.y, z)
        };
        g.heights[i] = h;
        g.level = g.level.max(l);
    }
    g
}

/// Average two angles the short way round (`PositionUnit`'s filter).
fn ease_angle(old: i32, target: i32) -> i32 {
    let (mut a, mut b) = (target, old);
    if (a - b).abs() > 0xb40 {
        if a < b {
            a += 0x1680;
        } else {
            b += 0x1680;
        }
    }
    wrap((a + b) >> 1)
}

/// `PositionUnit` @ 10081450 (68k `FUN_000a1758`, OS4 `FUN_010cbc7c`):
/// body roll and pitch from the box heights, each eased halfway toward
/// its target, and the downhill speed `speed += pitch·ticks >> 8` (the
/// unfiltered slope; ticks = 1). Across, the lowest point of each row is
/// used; along, the centre line. When the middle is a crest (both halves
/// slope away) the gentler half wins. The roll target adds the boat bob
/// `+0x7fc` and the lean `+0x7e8`, the pitch target the bob `+0x800` and
/// the dip `+0x7ec`. The roll ratio is scaled by 1/256 before `arctan`
/// (constant `0.00390625` at TOC `1017ceec`+0x38).
fn position_unit(obj: &mut Obj, d: &Drive, hw: i32, front: i32, rear: i32) {
    let h = &d.heights;
    let min3 = |k: usize| h[k].min(h[k + 1]).min(h[k + 2]);
    let (l, c, r) = (min3(0), min3(3), min3(6));
    let (lc, cr) = (l - c, c - r);
    let ratio = if lc < 0 || cr > 0 {
        ((l - r) * 0x100) / (hw * 2)
    } else if cr.abs() < lc.abs() {
        (cr * 0x100) / hw
    } else {
        (lc * 0x100) / hw
    };
    let roll = collide::arctan(ratio as f64 / 256.0) + d.bob[0] + d.lean;
    obj.roll = ease_angle(obj.roll, wrap(roll));

    let (back, mid, nose) = (h[3], h[4], h[5]);
    let (bm, mn) = (back - mid, mid - nose);
    let pitch = -if bm < 0 || mn > 0 {
        collide::arctan((back - nose) as f64 / (front + rear).max(1) as f64)
    } else if mn.abs() < bm.abs() {
        collide::arctan(mn as f64 / front.max(1) as f64)
    } else {
        collide::arctan(bm as f64 / rear.max(1) as f64)
    };
    obj.speed += pitch >> 8;
    obj.pitch = ease_angle(obj.pitch, wrap(pitch + d.bob[1] + d.dip));
}

/// Engine pitch, clonk swap, skid and reversing beeper (`do_Other` @
/// 10146280 engine block and `PositionAirborneUnit` @ 10082620, `SkidUnit`
/// @ 10081830): on the ground the engine pitch is `|wheel speed|·20000 /
/// eff + 6000` (the tank `·10000 / eff + 4000`; the helicopter with its
/// engine off 4000), in the air the same over the plain top speed; below
/// 50 health the engine sample swaps to its clonking version.
fn vehicle_sounds(
    mut commands: Commands,
    mut q: Query<(&Obj, &Vehicle, &mut Drive)>,
    mut sounds: Query<&mut Sound>,
) {
    for (obj, veh, mut d) in &mut q {
        let t = &TYPES[veh.kind];
        let top = if d.airborne { t.max_speed } else { effective_max(t, obj.health) }.max(1);
        let at = SoundPos::At(obj.posf() - Vec3::Z * 100.0);
        let ws = d.wheel_speed.abs();
        if let Some(mut s) = veh.engine.and_then(|e| sounds.get_mut(e).ok()) {
            s.pos = at;
            s.hz = match veh.kind {
                vk::TANK => (ws * 10000 / top + 4000) as f32,
                vk::HELICOPTER if !veh.flying && !d.airborne => 4000.0,
                _ => (ws * 20000 / top + 6000) as f32,
            };
            if obj.health < 50 && !d.clonk && !d.airborne {
                d.clonk = true;
                s.id = match s.id {
                    id::ENGINE => id::ENGINE_CLONK,
                    id::ENGINE_904..=id::ENGINE_BUS => s.id + 4,
                    other => other,
                };
            }
        }
        // Skid loop.
        let skid_at = SoundPos::At(obj.posf() - Vec3::Z * 250.0);
        let skid = *d.skid_sound.get_or_insert_with(|| {
            commands
                .spawn(Sound::looping(id::SKID, 0.0, 10000.0, skid_at))
                .id()
        });
        if let Ok(mut s) = sounds.get_mut(skid) {
            let slip = if d.skid_audible { d.slip } else { 0 };
            s.volume = slip as f32 * 40000.0;
            s.hz = (slip * 10 + 10000) as f32;
            s.pos = skid_at;
        }
        // Reversing beeper.
        match (t.reverse_beep, obj.speed < 0, d.beep_sound) {
            (Some(beep), true, None) => {
                d.beep_sound = Some(
                    commands
                        .spawn(Sound::looping(beep, 10_000_000.0, 22050.0, at))
                        .id(),
                );
            }
            (_, false, Some(e)) => {
                commands.entity(e).despawn();
                d.beep_sound = None;
            }
            _ => {}
        }
    }
}

/// The player's horn and the end of every horn (Mac `do_Other` @ 10146280,
/// state 2 and tail). A player-driven vehicle honks while its player holds
/// the shoot key (phrase 2461 "Shoot/horn", default Space; the input word's
/// shoot bit, so replays carry it): `_DAT_1017cc70 = fire[player]`, read
/// before and regardless of the controls-off flag `+0x988`, which only
/// zeroes the throttle and steering. A police model's horn is the siren
/// with the flashing light bar (`pursuit::sirens`). Then every vehicle
/// that did not honk this tick (player or AI, [`super::vehicle_ai::horn`])
/// loses its horn sound: `if (!cc70 && +0x918 >= 0) DeleteSound`. Nothing
/// in the simulation listens to the horn; only sound entities are spawned.
fn horns(
    mut commands: Commands,
    inputs: Res<PlayerInputs>,
    mut q: Query<(Entity, &Obj, &Vehicle, &mut Drive)>,
    mut sounds: Query<&mut Sound>,
) {
    for e__ in super::table_order(q.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, obj, veh, mut d)) = q.get_mut(e__) else { continue };
        if obj.state == State::Player && !TYPES[veh.kind].police {
            let fire = inputs.0[obj.player.unwrap_or(0).min(3) as usize].fire;
            super::vehicle_ai::horn(&mut commands, &mut sounds, &mut d, e, veh, obj, fire);
        }
        if !d.honked
            && let Some(s) = d.horn.take()
        {
            commands.entity(s).try_despawn();
        }
        d.honked = false;
    }
}

/// Vehicle transforms: heading, then the body pitch (+ = nose down) and
/// roll (+ = the −x side down) from `PositionUnit`, which include the
/// steering lean, the throttle dip and the boat bob.
fn sync_vehicles(mut q: Query<(&Obj, &mut Transform), With<Vehicle>>) {
    let rad = |a: i32| angle_signed(a) as f32 / super::TURN as f32 * std::f32::consts::TAU;
    for (obj, mut tf) in &mut q {
        tf.translation = object_to_render(obj.posf());
        tf.rotation = heading_to_render(obj.heading as f32)
            * Quat::from_rotation_x(rad(obj.pitch))
            * Quat::from_rotation_z(rad(obj.roll));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mundaneo() -> &'static VehicleType {
        &TYPES[0]
    }

    /// Throttle alone on flat ground: the force ramps by 5 to `accel`, so
    /// a Mundaneo (accel 5, max 280) reaches top speed in 56 ticks.
    #[test]
    fn acceleration_ramp() {
        let t = mundaneo();
        let (mut speed, mut force, mut ticks) = (0, 0, 0);
        while speed < t.max_speed {
            force = (force.max(0) + 5).min(t.accel);
            speed += force.min(t.max_speed - speed);
            ticks += 1;
        }
        assert_eq!(ticks, 56);
    }

    #[test]
    fn grip_by_surface() {
        let mut s = 500;
        assert_eq!(grip_for(0x22, 25, &mut s), 12);
        assert_eq!(s, 497);
        let mut s = 400;
        assert_eq!(grip_for(0x24, 25, &mut s), 50);
        assert_eq!(s, 380);
        let mut s = 300;
        assert_eq!(grip_for(0x32, 25, &mut s), 4);
        assert_eq!(s, 290);
        let mut s = 100;
        assert_eq!(grip_for(0x32, 25, &mut s), 4);
        assert_eq!(s, 100);
        assert_eq!(grip_for(0x33, 25, &mut s), 10);
        assert_eq!(grip_for(0x34, 25, &mut s), 1);
        assert_eq!(grip_for(4, 25, &mut s), 20);
    }

    #[test]
    fn lean_and_yaw() {
        let t = mundaneo();
        let mut obj = Obj::new(super::super::Class::Vehicle, 0, IVec3::ZERO, 0);
        obj.speed = 280;
        let mut d = Drive::default();
        // Left at full speed: the yaw rate ramps 5 per tick to 40, the
        // lean slews (−32 + 64) per tick toward +256.
        for _ in 0..10 {
            ground_controls(&mut obj, &mut d, t, false, false, -1, 0);
        }
        assert_eq!(obj.turn_rate, 40);
        assert_eq!(d.lean, 256);
        // Released: yaw 0 at once, the lean relaxes 32 per tick.
        ground_controls(&mut obj, &mut d, t, false, false, 0, 0);
        assert_eq!(obj.turn_rate, 0);
        assert_eq!(d.lean, 224);
        // Slow: the yaw rate is capped by the speed.
        obj.speed = 12;
        for _ in 0..5 {
            ground_controls(&mut obj, &mut d, t, false, false, 1, 0);
        }
        assert_eq!(obj.turn_rate, -12);
        // Boat: the lean goes the other way.
        let mut d = Drive::default();
        obj.speed = 280;
        ground_controls(&mut obj, &mut d, t, true, true, -1, 0);
        assert_eq!(d.lean, -64);
    }

    #[test]
    fn dip_on_throttle() {
        let t = mundaneo();
        let mut obj = Obj::new(super::super::Class::Vehicle, 0, IVec3::ZERO, 0);
        let mut d = Drive::default();
        // The relax (16) and the push (16) balance: −16 per tick while
        // accelerating (the −0x80 floor only matters for larger steps).
        for _ in 0..10 {
            ground_controls(&mut obj, &mut d, t, false, false, 0, -1);
        }
        assert_eq!(d.dip, -16);
        obj.speed = t.max_speed;
        ground_controls(&mut obj, &mut d, t, false, false, 0, -1);
        assert_eq!(d.dip, 0);
    }

    #[test]
    fn boat_bob_oscillates() {
        let mut d = Drive {
            bob: [0, 0, -25, -11],
            ..Default::default()
        };
        let (mut lo, mut hi) = (0, 0);
        for _ in 0..200 {
            d.bob[0] += d.bob[2];
            d.bob[2] += if d.bob[0] < 1 { 2 } else { -2 };
            lo = lo.min(d.bob[0]);
            hi = hi.max(d.bob[0]);
        }
        assert!(lo < -100 && hi > 50, "{lo} {hi}");
    }

    #[test]
    fn airborne_turns_with_the_last_steer() {
        let t = mundaneo();
        let trig = Trig::default();
        let mut obj = Obj::new(super::super::Class::Vehicle, 0, IVec3::ZERO, 0x100);
        obj.speed = 60;
        let mut d = Drive {
            last_steer: -1,
            trail: [Some(IVec2::ZERO); 4],
            ..Default::default()
        };
        position_airborne_unit(&mut obj, &mut d, t, &trig, false, 1);
        // min(|60 >> 1|, 40) = 30, left (up) going forward.
        assert_eq!(obj.heading, 0x100 + 30);
        assert_eq!(obj.pitch, 16);
        assert!(d.trail.iter().all(Option::is_none));
    }

    /// The player's horn (`do_Other` @ 10146280 state 2, `DoVehicleFire`
    /// @ 10070c20): a CarHorn loop at 20e6 and `16e6 / mass` Hz while the
    /// shoot key is held, even with the controls off; gone on release or
    /// when the car is no longer the player's. Police models use the siren
    /// (`pursuit::sirens`); the tank never honks.
    #[test]
    fn player_horn() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<PlayerInputs>();
        let car = |world: &mut World, kind: usize, state: State| {
            let mut o = Obj::new(super::super::Class::Vehicle, 0, IVec3::new(1000, 2000, 0), 0);
            o.state = state;
            o.player = Some(0);
            let veh = Vehicle { kind, colour: 0, engine: None, flying: false, controls: false };
            world.spawn((o, veh, Drive::default())).id()
        };
        let e = car(&mut world, 0, State::Player);
        let tank = car(&mut world, vk::TANK, State::Player);
        let cop = car(&mut world, vk::POLICE, State::Player);
        let horn = |world: &mut World, e: Entity| world.get::<Drive>(e).unwrap().horn;
        let fire = |world: &mut World, on: bool| world.resource_mut::<PlayerInputs>().0[0].fire = on;

        world.run_system_once(horns).unwrap();
        assert_eq!(horn(&mut world, e), None);
        fire(&mut world, true);
        world.run_system_once(horns).unwrap();
        let s = horn(&mut world, e).expect("horn starts");
        let snd = world.get::<Sound>(s).unwrap();
        assert_eq!((snd.id, snd.looping, snd.volume), (id::CAR_HORN, true, 20e6));
        assert_eq!(snd.hz, (16_000_000 / TYPES[0].mass).clamp(3000, 20000) as f32);
        assert!(crate::audio::amplitude(snd, Vec3::new(1000.0, 2000.0, 0.0)) > 19.0);
        assert_eq!(horn(&mut world, tank), None);
        assert_eq!(horn(&mut world, cop), None);
        // Held: the same voice.
        world.run_system_once(horns).unwrap();
        assert_eq!(horn(&mut world, e), Some(s));
        // Released: deleted.
        fire(&mut world, false);
        world.run_system_once(horns).unwrap();
        assert_eq!(horn(&mut world, e), None);
        assert!(world.get_entity(s).is_err());
        // Held, then the player gets out: deleted.
        fire(&mut world, true);
        world.run_system_once(horns).unwrap();
        let s = horn(&mut world, e).unwrap();
        world.get_mut::<Obj>(e).unwrap().state = State::Parked;
        world.run_system_once(horns).unwrap();
        assert_eq!(horn(&mut world, e), None);
        assert!(world.get_entity(s).is_err());
    }
}
