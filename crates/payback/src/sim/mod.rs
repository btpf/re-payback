//! Simulation core shared by every dynamic object, mirroring the original's
//! single object table (docs/re/people.md §1, combat_police.md §0).
//!
//! State is kept in the original integer units: x, y with 2048 per square,
//! z downward (16 per h-unit), angles 5760 per turn, rates per game tick.
//! One game tick is 1/25 s; systems run in `FixedUpdate` with ticks = 1.

pub mod ai;
pub mod boarding;
pub mod collide;
pub mod combat;
pub mod contact;
pub mod driving;
pub mod people;
pub mod police;
pub mod score;
pub mod traffic;

use crate::data::GameData;
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
/// as the original does.
#[derive(Resource)]
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

    /// 0..n-1 (for n > 0).
    pub fn rand(&mut self, n: i32) -> i32 {
        self.idx = (self.idx + 1) & 0xffff;
        self.table[self.idx] * n >> 9
    }

    /// Deterministic hash lookup `T[i] * n >> 9` (OS4 `FUN_0103ad20`).
    pub fn hash(&self, i: usize, n: i32) -> i32 {
        self.table[i & 0xffff] * n >> 9
    }

    pub fn seed(&mut self, s: u16) {
        self.idx = s as usize;
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
    /// +0x4910/+0x4914: mean X/Y of ground vertices (scenery collision).
    pub mean: IVec2,
    /// +0x4918: scenery collision size, at least 0x80.
    pub size: i32,
}

impl ModelInfo {
    pub fn from_obj(path: &str, obj: &payback_formats::Obj) -> ModelInfo {
        let v = &obj.vertices;
        let radius = v.iter().map(|p| p.x.abs() * 16).max().unwrap_or(0);
        let near: Vec<_> = v.iter().filter(|p| p.z * 16 > -0x29).collect();
        let max_y = near.iter().map(|p| p.y * 16).max().unwrap_or(0);
        let min_y = near.iter().map(|p| p.y * 16).min().unwrap_or(0).abs();
        let ground: Vec<_> = v.iter().filter(|p| p.z == 0).collect();
        let n = ground.len().max(1) as i32;
        let mean = IVec2::new(
            ground.iter().map(|p| p.x * 16).sum::<i32>() / n,
            ground.iter().map(|p| p.y * 16).sum::<i32>() / n,
        );
        let xs = v.iter().map(|p| p.x * 16);
        let ys = v.iter().map(|p| p.y * 16);
        let x_range = xs.clone().max().unwrap_or(0) - xs.min().unwrap_or(0);
        let y_range = ys.clone().max().unwrap_or(0) - ys.min().unwrap_or(0);
        ModelInfo {
            path: path.to_string(),
            radius,
            max_y,
            min_y,
            mass: 1100,
            code: 0xff,
            police: false,
            mean,
            size: (x_range.min(y_range) / 2).max(0x80),
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
#[derive(Component, Clone, Debug)]
pub struct Obj {
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
}

impl Obj {
    pub fn new(class: Class, model: usize, pos: IVec3, heading: i32) -> Obj {
        Obj {
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
        }
    }

    /// Position for the camera/audio, as floats.
    pub fn posf(&self) -> Vec3 {
        self.pos.as_vec3()
    }
}

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Trig>()
            .add_systems(Startup, |mut commands: Commands, data: Res<GameData>| {
                commands.insert_resource(Rng::load(&data));
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
}
