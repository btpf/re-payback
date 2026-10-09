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
//! 1..n). The defaults below can be redefined in the control setup
//! (`frontend::bindings`, columns "Multiplayer – Keyboard n" and
//! "Multiplayer – Joystick n"; a joystick's stick and d-pad always steer):
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
//! directly. `--hijack-test` adds scripted input: player 1 gets into the
//! nearest car, then player 2 is put beside it and hijacks it (the victim
//! is thrown out and keeps playing on foot; logged as `hijack test:`).

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

/// Keyboard key sets, so that up to four players can share a keyboard
/// (the bindings are `Controls::mp_keys`, redefinable in the control
/// setup).
pub const KEY_SETS: usize = 4;

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
    /// Score at death, restored on respawn (`DAT_1017c524[p]`, Mac
    /// `amiga_levelloop` @ 10100f50).
    pub score: [i32; 4],
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
                hijack_test
                    .run_if(is_multiplayer)
                    .run_if(|| crate::args().any(|a| a == "--hijack-test"))
                    .before(crate::sim::people::PeopleThink),
            )
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
pub(crate) fn cycle_weapon(ars: &Arsenal, d: i32) -> Option<u8> {
    let cur = ars.current as i32;
    (1..8)
        .map(|k| (cur + d * k).rem_euclid(8) as usize)
        .find(|&s| s == 0 || ars.ammo[s] > 0 || (s == 7 && ars.bomb.is_some()))
        .map(|s| s as u8)
}

/// Each player's device into their controls (single player:
/// `people::read_keyboard`), through the bindings of the player's key set
/// or of the player's gamepad column.
fn read_devices(
    mode: Res<GameMode>,
    devices: Res<MpDevices>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad)>,
    (slots, axes): (Res<crate::frontend::bindings::PadSlots>, Res<crate::frontend::bindings::PadAxes>),
    players: Query<(&Obj, &Arsenal), With<CameraTarget>>,
    mut inputs: ResMut<PlayerInputs>,
    smooth: Res<crate::frame::Smooth>,
    mut latch: Local<(u64, [[bool; 6]; 4])>,
) {
    // Presses shorter than a tick are kept until a tick has seen them (as
    // `people::read_keyboard`).
    if latch.0 != smooth.ticks() {
        *latch = (smooth.ticks(), [[false; 6]; 4]);
    }
    use crate::frontend::bindings::{self as b, key_held, key_just, pad_held, pad_just};
    let defaults;
    let controls = match &settings {
        Some(s) => &s.controls,
        None => {
            defaults = b::Controls::default();
            &defaults
        }
    };
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
                let k = &controls.mp_keys[set as usize % KEY_SETS];
                let held = |a: usize| key_held(&keys, k[a], false);
                let just = |a: usize| key_just(&keys, k[a], false);
                input.left = held(b::LEFT);
                input.right = held(b::RIGHT);
                input.accel = held(b::ACCELERATE);
                input.reverse = held(b::REVERSE);
                input.fire = held(b::SHOOT) || held(b::SHOOT_ALT);
                input.brake = held(b::BRAKE);
                input.use_pressed |= just(b::IN_OUT);
                if just(b::PREV_WEAPON) {
                    step = -1;
                }
                if just(b::NEXT_WEAPON) {
                    step = 1;
                }
                // Digits 0..7 pick a weapon for every keyboard player (Mac
                // `UpdateInputStatus` @ 100f8490 fixed keys, OS4
                // `FUN_0103f3d0`).
                if let Some(d) = (0..8u8).find(|&d| keys.just_pressed(b::digit_key(d))) {
                    input.weapon_pressed = Some(d);
                }
            }
            Device::Joystick(n) => {
                // Joystick n is gamepad number n (`bindings::PadSlots`).
                let Some((e, g)) = slots.get(n as usize).and_then(|e| pads.get(e).ok()) else {
                    *input = Default::default();
                    continue;
                };
                // The picked device's table (Mac `UpdateInputStatus` @ 100f8490).
                let t = &controls.pads[n as usize % 4];
                let pad = axes.state(e, g);
                let held = |a: usize| pad_held(&pad, t[a]);
                let s = g.left_stick() + g.dpad();
                input.left = s.x < -0.5 || held(b::LEFT);
                input.right = s.x > 0.5 || held(b::RIGHT);
                input.accel = s.y > 0.5 || held(b::ACCELERATE);
                input.reverse = s.y < -0.5 || held(b::REVERSE);
                input.fire = held(b::SHOOT) || held(b::SHOOT_ALT);
                input.brake = held(b::BRAKE);
                input.use_pressed |= pad_just(&pad, t[b::IN_OUT]);
                if pad_just(&pad, t[b::PREV_WEAPON]) {
                    step = -1;
                }
                if pad_just(&pad, t[b::NEXT_WEAPON]) {
                    step = 1;
                }
            }
        }
        if let Some(held_since) = latch.1.get_mut(p) {
            let now = [&mut input.left, &mut input.right, &mut input.accel, &mut input.reverse, &mut input.fire, &mut input.brake];
            for (h, v) in held_since.iter_mut().zip(now) {
                *h |= *v;
                *v = *h;
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

/// `--hijack-test`: player 1 presses in/out 10 ticks after both players
/// appear and walks to the
/// nearest car; once he drives it, player 2 is moved next to its door and
/// presses in/out. Logs who drives what each 25 ticks until tick 600.
#[allow(clippy::type_complexity)]
fn hijack_test(
    mut tick: Local<u32>,
    mut stage: Local<u8>,
    trig: Res<crate::sim::Trig>,
    library: Option<Res<ObjectLibrary>>,
    mut inputs: ResMut<PlayerInputs>,
    mut q: Query<
        (
            Entity,
            &mut Obj,
            Has<Person>,
            Option<&crate::sim::boarding::Boarding>,
            Option<&crate::vehicles::Vehicle>,
            Option<&Person>,
        ),
        With<CameraTarget>,
    >,
    cars: Query<(&Obj, &crate::vehicles::Vehicle), Without<CameraTarget>>,
) {
    let Some(library) = library else { return };
    // Count from the first tick both players exist.
    if q.iter().filter(|(_, o, ..)| o.player.is_some()).count() < 2 && *tick == 0 {
        return;
    }
    *tick += 1;
    let t = *tick;
    if t == 10 {
        inputs.0[0].use_pressed = true;
        info!("hijack test: player 1 presses in/out");
    }
    // Player 1 is kept at the door of the car he goes for (no long walk).
    for (_, mut o, _, boarding, ..) in &mut q {
        let Some((v, veh)) = boarding.and_then(|b| cars.get(b.vehicle).ok()) else {
            continue;
        };
        if o.player == Some(0) {
            let info = &library.model(v.model).info;
            let door = crate::sim::boarding::door_point(
                &trig,
                v,
                info.radius,
                crate::sim::boarding::door_y(veh.kind, info.max_y),
                192,
            );
            o.pos = door;
            o.prev_pos = door;
        }
    }
    let car = q
        .iter()
        .find(|(_, o, foot, ..)| !foot && o.player == Some(0))
        .map(|(e, o, _, _, v, _)| (e, o.clone(), v.map_or(0, |v| v.kind)));
    if *stage == 0 {
        if let Some((_, v, kind)) = &car {
            let info = &library.model(v.model).info;
            let door = crate::sim::boarding::door_point(
                &trig,
                v,
                info.radius,
                crate::sim::boarding::door_y(*kind, info.max_y),
                192 * 3,
            );
            for (_, mut o, foot, ..) in &mut q {
                if foot && o.player == Some(1) {
                    o.pos = door;
                    o.prev_pos = door;
                }
            }
            inputs.0[1].use_pressed = true;
            info!("hijack test: tick {t}: player 1 drives {:?}; player 2 goes for it", car.as_ref().map(|c| c.0));
            *stage = 1;
        }
    }
    if t % 25 == 0 && t <= 600 {
        for (e, o, foot, boarding, _, p) in &q {
            let boarding = boarding.is_some();
            info!(
                "hijack test: tick {t}: player {:?} {}{} {e:?} at {:?} state {:?} health {} stun {}",
                o.player.map(|n| n + 1),
                if foot { "on foot" } else { "driving" },
                if boarding { " (boarding)" } else { "" },
                o.pos,
                o.state,
                o.health,
                p.map_or(0, |p| p.stun),
            );
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
    for e__ in crate::sim::table_order(players.iter().map(|(e, o, ..)| (e, o))) {
        let Ok((e, mut obj, on_foot)) = players.get_mut(e__) else { continue };
        let Some(p) = obj.player.filter(|&p| (p as usize) < n) else {
            continue;
        };
        present[p as usize] = true;
        if matches!(obj.state, State::Dead | State::Removed) {
            // The corpse stays; the view waits for the respawn.
            deaths.push((p, last.0.get(&e).copied().flatten()));
            game.score[p as usize] = obj.score;
            commands.entity(e).remove::<CameraTarget>();
            obj.player = None;
            continue;
        }
        alive_at.push(obj.posf());
        // Kept current, so a player gone without a corpse keeps it too.
        game.score[p as usize] = obj.score;
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
            let score = game.score[p];
            commands.queue(move |world: &mut World| {
                if let Some(mut o) = world.get_mut::<Obj>(e) {
                    o.score = score;
                }
            });
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
    fn layout() {
        use crate::camera::view_rects;
        assert_eq!(view_rects(2, 1280, 800), vec![(0, 0, 638, 800), (642, 0, 638, 800)]);
        let r = view_rects(3, 1280, 800);
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].0, (1280 - r[2].2) / 2, "third view centred");
        assert_eq!(view_rects(4, 1280, 800)[3], (642, 402, 638, 398));
    }
}
