//! Multiplayer: 2–4 player split-screen deathmatch
//! (docs/re/frontend_menus.md §7, docs/re/combat_police.md §7; setup
//! `FUN_0102b2c8`, split screen `FUN_0105bd98`, results `FUN_01027330`).
//!
//! Every player starts at a different start point (map squares with
//! special 1). There are no lives: a dead player respawns after the
//! 80-tick death timer at a start point away from the others. A kill
//! scores +1 for the killer and a death with nobody else to blame
//! (suicide, own explosives, drowning, traffic) −1 for the victim
//! (killer = the victim's last attacking player, `+0x4b0`). The first
//! player to reach the required kills wins; several at once draw. There
//! is no level script, and crimes, the wanted level and police are off.
//!
//! Controls per player (setup screen; `--multiplayer` uses keyboard sets
//! 1..n):
//!
//! | Set | Steer / accelerate / reverse | Fire | Brake | In/out | Weapon −/+ |
//! |---|---|---|---|---|---|
//! | Keyboard 1 | arrow keys | Right Ctrl | Right Shift | Enter | Delete / Page Down |
//! | Keyboard 2 | W A S D | Left Ctrl | Left Shift | Tab | Q / E |
//! | Keyboard 3 | numpad 8 4 5 6 | numpad 0 | numpad . | numpad Enter | numpad − / + |
//! | Keyboard 4 | I J K L | Space | N | H | U / O |
//! | Joystick n | left stick or d-pad (up = accelerate), RT / LT | South (A) | East (B) | North (Y) | LB / RB |
//!
//! Debug: `--multiplayer <2..4> --map <city> [--kills <n>]` starts a match
//! directly.

use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::hud::{BigTitle, Texts};
use crate::level::{AppState, GameMode, StartPoints};
use crate::mission::LevelOutcome;
use crate::objects::ObjectLibrary;
use crate::sim::combat::Arsenal;
use crate::sim::people::{self, Person, PlayerInputs, PlayerSession, REGEN_TICKS, RESPAWN_TICKS};
use crate::sim::score::LastAttacker;
use crate::sim::{Obj, Rng, State};
use bevy::prelude::*;

/// A player's input device (setup rows "Player k: ...").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Off,
    /// Keyboard with key set 0..3 ([`KEY_SETS`]).
    Keyboard(u8),
    /// Bevy gamepad n (0..3, in connection order).
    Joystick(u8),
}

impl Device {
    /// Settings-file name: `off`, `kb1`..`kb4`, `joy1`..`joy4`.
    pub fn name(self) -> String {
        match self {
            Device::Off => "off".into(),
            Device::Keyboard(k) => format!("kb{}", k + 1),
            Device::Joystick(n) => format!("joy{}", n + 1),
        }
    }

    pub fn parse(s: &str) -> Option<Device> {
        let num = |t: &str| t.parse::<u8>().ok().filter(|n| (1..=4).contains(n)).map(|n| n - 1);
        if s == "off" {
            Some(Device::Off)
        } else if let Some(t) = s.strip_prefix("kb") {
            num(t).map(Device::Keyboard)
        } else {
            s.strip_prefix("joy").and_then(num).map(Device::Joystick)
        }
    }
}

/// One keyboard key set.
pub struct KeySet {
    /// Short description for the setup screen (F fire, B brake, In
    /// in/out of vehicles, W previous/next weapon).
    pub hint: &'static str,
    pub left: KeyCode,
    pub right: KeyCode,
    pub accel: KeyCode,
    pub reverse: KeyCode,
    pub fire: KeyCode,
    pub brake: KeyCode,
    pub in_out: KeyCode,
    pub prev: KeyCode,
    pub next: KeyCode,
}

/// Keyboard key sets, so that up to four players can share a keyboard.
pub const KEY_SETS: [KeySet; 4] = [
    KeySet {
        hint: "Arrows F:RCtrl B:RShift In:Enter W:Del/PgDn",
        left: KeyCode::ArrowLeft,
        right: KeyCode::ArrowRight,
        accel: KeyCode::ArrowUp,
        reverse: KeyCode::ArrowDown,
        fire: KeyCode::ControlRight,
        brake: KeyCode::ShiftRight,
        in_out: KeyCode::Enter,
        prev: KeyCode::Delete,
        next: KeyCode::PageDown,
    },
    KeySet {
        hint: "WASD F:LCtrl B:LShift In:Tab W:Q/E",
        left: KeyCode::KeyA,
        right: KeyCode::KeyD,
        accel: KeyCode::KeyW,
        reverse: KeyCode::KeyS,
        fire: KeyCode::ControlLeft,
        brake: KeyCode::ShiftLeft,
        in_out: KeyCode::Tab,
        prev: KeyCode::KeyQ,
        next: KeyCode::KeyE,
    },
    KeySet {
        hint: "Numpad 8456 F:0 B:. In:Enter W:-/+",
        left: KeyCode::Numpad4,
        right: KeyCode::Numpad6,
        accel: KeyCode::Numpad8,
        reverse: KeyCode::Numpad5,
        fire: KeyCode::Numpad0,
        brake: KeyCode::NumpadDecimal,
        in_out: KeyCode::NumpadEnter,
        prev: KeyCode::NumpadSubtract,
        next: KeyCode::NumpadAdd,
    },
    KeySet {
        hint: "IJKL F:Space B:N In:H W:U/O",
        left: KeyCode::KeyJ,
        right: KeyCode::KeyL,
        accel: KeyCode::KeyI,
        reverse: KeyCode::KeyK,
        fire: KeyCode::Space,
        brake: KeyCode::KeyN,
        in_out: KeyCode::KeyH,
        prev: KeyCode::KeyU,
        next: KeyCode::KeyO,
    },
];

/// Setup-screen hint for joystick players.
pub const JOYSTICK_HINT: &str = "Stick/D-pad F:A B:B In:Y W:LB/RB";

/// Devices of the players in the game, by player number.
#[derive(Resource, Clone, Debug)]
pub struct MpDevices(pub Vec<Device>);

impl Default for MpDevices {
    fn default() -> Self {
        MpDevices((0..4).map(Device::Keyboard).collect())
    }
}

/// The match in progress.
#[derive(Resource, Default, Clone, Debug)]
pub struct Match {
    /// Kills per player (`DAT_012a6730[4]`).
    pub kills: [i32; 4],
    /// Death timers (`DAT_012a65e0[p]`).
    pub respawn: [i32; 4],
    pub regen: [i32; 4],
    /// Once decided: the winner, or -1 for a draw.
    pub result: Option<i32>,
}

/// Won / drawn / lost per player, added up until the setup screen is
/// opened again (results screen `FUN_01027330`).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct MpTotals {
    pub won: [i32; 4],
    pub drawn: [i32; 4],
    pub lost: [i32; 4],
}

const YOU_WIN: u32 = 2186;
const YOU_LOSE: u32 = 2187;
const YOU_DRAW: u32 = 2188;

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        if let Some(n) = crate::arg("--multiplayer").and_then(|v| v.parse::<u8>().ok()) {
            let kills = crate::arg("--kills").and_then(|v| v.parse::<u8>().ok()).unwrap_or(5);
            app.insert_resource(GameMode::Multiplayer {
                players: n.clamp(2, 4),
                kills: kills.clamp(1, 50),
            });
        }
        app.init_resource::<MpDevices>()
            .init_resource::<Match>()
            .init_resource::<MpTotals>()
            .add_systems(OnEnter(AppState::Playing), |mut game: ResMut<Match>| {
                *game = Match::default();
            })
            .add_systems(
                OnExit(AppState::Playing),
                |mut others: ResMut<crate::audio::OtherListeners>| others.0.clear(),
            )
            .add_systems(PreUpdate, read_devices.run_if(is_multiplayer))
            .add_systems(
                FixedUpdate,
                deaths_and_respawns
                    .run_if(is_multiplayer)
                    .after(crate::sim::score::track_attackers),
            );
    }
}

fn is_multiplayer(mode: Res<GameMode>) -> bool {
    mode.is_multiplayer()
}

/// A start point for a player: the best of 8 random ones, as far as
/// possible from `avoid` (the other players).
pub fn pick_start(starts: &StartPoints, avoid: &[Vec3], rng: &mut Rng) -> Option<Vec3> {
    if starts.list.is_empty() {
        // Only a single-player start: side by side.
        return starts.level_start.map(|p| p + Vec3::new(600.0 * avoid.len() as f32, 0.0, 0.0));
    }
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..8 {
        let c = starts.list[rng.rand(starts.list.len() as i32) as usize];
        let d = avoid
            .iter()
            .map(|a| a.truncate().distance_squared(c.truncate()))
            .fold(f32::MAX, f32::min);
        if best.is_none_or(|(bd, _)| d > bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

/// The next carried weapon slot from the current one in direction `d`.
fn cycle_weapon(ars: &Arsenal, d: i32) -> Option<u8> {
    let cur = ars.current as i32;
    (1..8)
        .map(|k| (cur + d * k).rem_euclid(8) as usize)
        .find(|&s| s == 0 || ars.ammo[s] > 0 || (s == 7 && ars.bomb.is_some()))
        .map(|s| s as u8)
}

/// Each player's device into their controls (single player:
/// `people::read_keyboard`).
fn read_devices(
    mode: Res<GameMode>,
    devices: Res<MpDevices>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad)>,
    players: Query<(&Obj, &Arsenal), With<CameraTarget>>,
    mut inputs: ResMut<PlayerInputs>,
) {
    let mut pads: Vec<(Entity, &Gamepad)> = pads.iter().collect();
    pads.sort_by_key(|p| p.0);
    for p in 0..mode.players() {
        let input = &mut inputs.0[p];
        let mut step = 0;
        match devices.0.get(p).copied().unwrap_or(Device::Off) {
            Device::Off => {
                input.left = false;
                input.right = false;
                input.accel = false;
                input.reverse = false;
                input.fire = false;
                input.brake = false;
            }
            Device::Keyboard(set) => {
                let k = &KEY_SETS[set as usize % KEY_SETS.len()];
                input.left = keys.pressed(k.left);
                input.right = keys.pressed(k.right);
                input.accel = keys.pressed(k.accel);
                input.reverse = keys.pressed(k.reverse);
                input.fire = keys.pressed(k.fire);
                input.brake = keys.pressed(k.brake);
                input.use_pressed |= keys.just_pressed(k.in_out);
                if keys.just_pressed(k.prev) {
                    step = -1;
                }
                if keys.just_pressed(k.next) {
                    step = 1;
                }
            }
            Device::Joystick(n) => {
                let Some((_, g)) = pads.get(n as usize) else {
                    *input = Default::default();
                    continue;
                };
                let s = g.left_stick() + g.dpad();
                input.left = s.x < -0.5;
                input.right = s.x > 0.5;
                input.accel = s.y > 0.5 || g.pressed(GamepadButton::RightTrigger2);
                input.reverse = s.y < -0.5 || g.pressed(GamepadButton::LeftTrigger2);
                input.fire = g.pressed(GamepadButton::South);
                input.brake = g.pressed(GamepadButton::East);
                input.use_pressed |= g.just_pressed(GamepadButton::North);
                if g.just_pressed(GamepadButton::LeftTrigger) {
                    step = -1;
                }
                if g.just_pressed(GamepadButton::RightTrigger) {
                    step = 1;
                }
            }
        }
        if step != 0 {
            let ars = players.iter().find(|(o, _)| o.player == Some(p as u8)).map(|(_, a)| a);
            if let Some(slot) = ars.and_then(|a| cycle_weapon(a, step)) {
                input.weapon_pressed = Some(slot);
            }
        }
    }
}

/// Deaths, kill credit, regeneration, respawns and the end of the match.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn deaths_and_respawns(
    mut commands: Commands,
    mode: Res<GameMode>,
    mut game: ResMut<Match>,
    mut totals: ResMut<MpTotals>,
    mut session: ResMut<PlayerSession>,
    data: Res<GameData>,
    starts: Option<Res<StartPoints>>,
    library: Option<ResMut<ObjectLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: ResMut<Rng>,
    last: Res<LastAttacker>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut outcome: MessageWriter<LevelOutcome>,
    mut players: Query<(Entity, &mut Obj, Has<Person>), With<CameraTarget>>,
) {
    let (Some(starts), Some(mut library)) = (starts, library) else {
        return;
    };
    let GameMode::Multiplayer { kills: required, .. } = *mode else {
        return;
    };
    let n = mode.players();
    let mut present = [false; 4];
    let mut alive_at: Vec<Vec3> = Vec::new();
    let mut deaths: Vec<(u8, Option<u8>)> = Vec::new();
    for (e, mut obj, on_foot) in &mut players {
        let Some(p) = obj.player.filter(|&p| (p as usize) < n) else {
            continue;
        };
        present[p as usize] = true;
        if matches!(obj.state, State::Dead | State::Removed) {
            // The corpse stays; the view waits for the respawn.
            deaths.push((p, last.0.get(&e).copied().flatten()));
            commands.entity(e).remove::<CameraTarget>();
            obj.player = None;
            continue;
        }
        alive_at.push(obj.posf());
        // +1 health every 250 ticks (on-foot health while driving).
        let regen = &mut game.regen[p as usize];
        *regen += 1;
        if *regen >= REGEN_TICKS {
            *regen = 0;
            let health = if on_foot { &mut obj.health } else { session.foot_health_mut(Some(p)) };
            if (1..100).contains(health) {
                *health += 1;
            }
        }
    }
    // Gone without a corpse (drowned): a death with no one to blame.
    for p in 0..n {
        if !present[p] && game.respawn[p] == 0 {
            deaths.push((p as u8, None));
        }
    }
    for (p, killer) in deaths {
        game.respawn[p as usize] = RESPAWN_TICKS;
        if game.result.is_some() {
            continue;
        }
        match killer {
            Some(k) if k != p && (k as usize) < n => game.kills[k as usize] += 1,
            _ => game.kills[p as usize] -= 1,
        }
    }

    for p in 0..n {
        if game.respawn[p] == 0 || present[p] {
            continue;
        }
        game.respawn[p] -= 1;
        if game.respawn[p] > 0 {
            continue;
        }
        let Some(pos) = pick_start(&starts, &alive_at, &mut rng) else {
            continue;
        };
        if let Some(e) = people::spawn_person(
            &mut commands,
            &data,
            &mut library,
            &mut meshes,
            0,
            pos.as_ivec3(),
            0,
            State::Player,
        ) {
            people::make_player(&mut commands, e, p as u8);
            alive_at.push(pos);
        }
    }

    // First to the required kills wins; several at once draw.
    if game.result.is_some() {
        return;
    }
    let winners: Vec<usize> = (0..n).filter(|&p| game.kills[p] >= required as i32).collect();
    if winners.is_empty() {
        return;
    }
    let result = if winners.len() == 1 { winners[0] as i32 } else { -1 };
    game.result = Some(result);
    info!("multiplayer: match over, kills {:?}, result {result}", &game.kills[..n]);
    for p in 0..n {
        if result == p as i32 {
            totals.won[p] += 1;
        } else if result == -1 && winners.contains(&p) {
            totals.drawn[p] += 1;
        } else {
            totals.lost[p] += 1;
        }
    }
    if let Some(t) = &texts {
        let each = (0..n)
            .map(|p| {
                t.global(if result == p as i32 {
                    YOU_WIN
                } else if result == -1 && winners.contains(&p) {
                    YOU_DRAW
                } else {
                    YOU_LOSE
                })
            })
            .collect();
        title.show_each(each);
    }
    commands.spawn(Sound::once(id::MISSION_COMPLETE, 255.0, 11025.0, SoundPos::Global));
    outcome.write(LevelOutcome::MatchOver);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names() {
        for d in [Device::Off, Device::Keyboard(0), Device::Keyboard(3), Device::Joystick(2)] {
            assert_eq!(Device::parse(&d.name()), Some(d));
        }
        assert_eq!(Device::parse("kb5"), None);
    }

    #[test]
    fn key_sets_distinct() {
        let mut all: Vec<KeyCode> = KEY_SETS
            .iter()
            .flat_map(|k| [k.left, k.right, k.accel, k.reverse, k.fire, k.brake, k.in_out, k.prev, k.next])
            .collect();
        let n = all.len();
        all.sort_by_key(|k| format!("{k:?}"));
        all.dedup();
        assert_eq!(all.len(), n, "a key is in two sets");
    }

    #[test]
    fn layout() {
        use crate::camera::view_rects;
        assert_eq!(view_rects(2, 1280, 800), vec![(0, 0, 638, 800), (642, 0, 638, 800)]);
        let r = view_rects(3, 1280, 800);
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].0, (1280 - r[2].2) / 2, "third view centred");
        assert_eq!(view_rects(4, 1280, 800)[3], (642, 402, 638, 398));
    }
}
