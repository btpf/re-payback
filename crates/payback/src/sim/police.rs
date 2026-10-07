//! Wanted level, crimes and police on foot (docs/re/combat_police.md §2;
//! 68k `FUN_000ccc96` Crime, decay in `FUN_000e3a58`, cop AI ~121928).

use super::combat::{Arsenal, Weapon};
use super::driving::Drive;
use super::people::{Person, PlayerSession};
use super::{Obj, Rng, State, Stick, Trig, angle_diff, heading_of};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::hud::{BigTitle, Dialog, Speaker, Texts, Voice};
use crate::level::{AppState, LevelSpec};
use crate::mission::{Triggers, kind};
use crate::objects::ObjectLibrary;
use crate::vehicles::Vehicle;
use bevy::prelude::*;

/// People model index of `policemale1` (PEOPLE_MODELS).
pub const COP: usize = 7;

/// Police aggressiveness 0..5: the level number in story mode, 2 on user
/// maps, the menu setting in rampage (§2.0).
#[derive(Resource, Clone, Copy, Debug)]
pub struct Aggressiveness(pub i32);

/// Ticks since the wanted level last changed (`DAT_0030c3b8`).
#[derive(Resource, Default)]
pub struct WantedTimer(pub i32);

/// The player's crime record (counters +0x458..+0x470).
#[derive(Resource, Default, Clone, Debug)]
pub struct Crimes {
    pub shunt: i32,
    pub carjacking: i32,
    pub murder: i32,
    pub gbh: i32,
    pub speeding: i32,
    pub criminal_damage: i32,
    pub firearms: i32,
    /// Cops killed this level (every 11th: "Copkiller bonus!").
    pub cops_killed: i32,
}

/// Something was hurt or destroyed (from every damage site).
#[derive(Message, Clone, Copy, Debug)]
pub struct Hurt {
    pub victim: Entity,
    /// The attacker is the player (or the player's projectile/vehicle).
    pub by_player: bool,
    /// Weapon or explosion (true) vs collision or punch (false).
    pub by_weapon: bool,
}

/// Each victim counts once for injury and once for death.
#[derive(Component, Default)]
pub struct CrimeCounted {
    pub injured: bool,
    pub killed: bool,
}

/// The player is under arrest (frozen until respawn).
#[derive(Component)]
pub struct Busted;

pub struct PolicePlugin;

impl Plugin for PolicePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Hurt>()
            .init_resource::<WantedTimer>()
            .init_resource::<Crimes>()
            .add_systems(OnEnter(AppState::Playing), |mut commands: Commands, spec: Res<LevelSpec>| {
                let city: i32 = spec.map.parse().unwrap_or(-1);
                let aggr = if (0..3).contains(&city) {
                    city * 2 + spec.level as i32
                } else {
                    2
                };
                commands.insert_resource(Aggressiveness(aggr));
            })
                        .add_systems(OnExit(AppState::Playing), reset_police)
            .add_systems(
                FixedUpdate,
                (crimes, decay_wanted)
                    .chain()
                    .after(super::combat::CombatSet),
            )
            // Cops steer after the ambient ped AI has reset their sticks.
            .add_systems(
                FixedUpdate,
                cops.in_set(super::people::PeopleThink)
                    .after(super::ai::people_orders),
            );
    }
}

fn raise(w: i32, table: [i32; 7]) -> i32 {
    table[w.clamp(0, 6) as usize]
}

/// Apply the crime table to each hurt/killed NPC (§2.1).
#[allow(clippy::type_complexity)]
fn crimes(
    mut commands: Commands,
    mut hurt: MessageReader<Hurt>,
    mut timer: ResMut<WantedTimer>,
    mut record: ResMut<Crimes>,
    session: Res<PlayerSession>,
    library: Option<Res<ObjectLibrary>>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut triggers: ResMut<Triggers>,
    mut player: Query<&mut Obj, With<CameraTarget>>,
    victims: Query<
        (&Obj, Has<Person>, Option<&Vehicle>, Option<&CrimeCounted>),
        Without<CameraTarget>,
    >,
) {
    let Some(library) = library else {
        hurt.clear();
        return;
    };
    let Ok(mut p) = player.single_mut() else {
        hurt.clear();
        return;
    };
    let mult = session.multiplier;
    for h in hurt.read() {
        if !h.by_player {
            continue;
        }
        triggers.report_actor(kind::ACTOR_ATTACKED, h.victim);
        let Ok((v, is_person, veh, counted)) = victims.get(h.victim) else {
            continue;
        };
        let counted = counted
            .map(|c| (c.injured, c.killed))
            .unwrap_or((false, false));
        let police = library.model(v.model).info.police;
        let dead = v.health <= 0;
        let w = p.wanted;
        let before = w;
        if is_person {
            if dead && !counted.1 {
                record.murder += 1;
                if police {
                    p.wanted = raise(w, [3, 3, 4, 5, 6, 6, 6]);
                    p.score += 2500 * mult;
                    record.cops_killed += 1;
                    triggers.report(kind::COP_KILLED, 0, 0, 0);
                    if record.cops_killed % 11 == 0 {
                        p.score += 100_000 * mult;
                        if let Some(t) = &texts {
                            title.show(t.global(2037));
                        }
                    }
                } else {
                    p.wanted = raise(w, [3, 3, 3, 4, 5, 6, 6]);
                    p.score += 1500 * mult;
                }
                commands.entity(h.victim).insert(CrimeCounted {
                    injured: true,
                    killed: true,
                });
            } else if !dead && !counted.0 {
                record.gbh += 1;
                if w < 5 {
                    p.wanted = (w + 1).min(4);
                }
                commands.entity(h.victim).insert(CrimeCounted {
                    injured: true,
                    killed: counted.1,
                });
            }
        } else if veh.is_some() {
            if !h.by_weapon && p.speed == 0 {
                continue;
            }
            let driven = v.state != State::Parked;
            if dead && !counted.1 {
                record.criminal_damage += 1;
                p.wanted = match (police, driven) {
                    (false, false) => w.max(3),
                    (true, false) => w.max(4),
                    (false, true) => raise(w, [3, 3, 3, 4, 5, 6, 6]),
                    (true, true) => raise(w, [4, 4, 4, 5, 6, 6, 6]),
                };
                if driven {
                    record.murder += 1;
                    p.score += if police { 2500 } else { 1500 } * mult;
                }
                commands.entity(h.victim).insert(CrimeCounted {
                    injured: true,
                    killed: true,
                });
            } else if !dead {
                if h.by_weapon {
                    record.criminal_damage += 1;
                } else {
                    record.shunt += 1;
                }
                p.wanted = if police {
                    w.max(3)
                } else if w < 3 {
                    (w + 1).max(2)
                } else {
                    w
                };
            }
        }
        if p.wanted != before {
            timer.0 = 0;
        }
    }
}

/// One wanted level drops every `aggr*100 + 150` ticks without new crimes.
fn decay_wanted(
    aggr: Option<Res<Aggressiveness>>,
    mut timer: ResMut<WantedTimer>,
    mut player: Query<&mut Obj, With<CameraTarget>>,
) {
    let aggr = aggr.map(|a| a.0).unwrap_or(2);
    timer.0 += 1;
    if timer.0 >= aggr * 100 + 150 {
        timer.0 = 0;
        if let Ok(mut p) = player.single_mut() {
            if p.wanted > 0 {
                p.wanted -= 1;
            }
        }
    }
}

/// Police on foot: chase, arrest and shoot when the player is wanted
/// (§2.4; 68k ~121928-122130). A driving player is arrested from the
/// driver's door: the cop opens it 100 per tick (the player's door shuts 50
/// per tick) and busts him when it is fully open.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn cops(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    trig: Res<Trig>,
    aggr: Option<Res<Aggressiveness>>,
    library: Option<ResMut<ObjectLibrary>>,
    texts: Option<Res<Texts>>,
    mut record: ResMut<Crimes>,
    mut session: ResMut<PlayerSession>,
    mut title: ResMut<BigTitle>,
    mut dialog: ResMut<Dialog>,
    mut player: Query<
        (Entity, &mut Obj, Has<Person>, Option<&mut Drive>),
        (With<CameraTarget>, Without<Busted>),
    >,
    mut q: Query<(&mut Obj, &mut Stick, &Person, Option<&mut Arsenal>), Without<CameraTarget>>,
) {
    let Some(mut library) = library else { return };
    let aggr = aggr.map(|a| a.0).unwrap_or(2);
    let Ok((pe, mut p, p_on_foot, mut door)) = player.single_mut() else {
        return;
    };
    let wanted = p.wanted;
    let p_info = library.model(p.model).info.clone();
    for (cop, mut stick, person, ars) in &mut q {
        if person.kind != COP || cop.state == State::Player {
            continue;
        }
        // policemale1 is a police model (+0x490a) for the crime table.
        library.model_mut(cop.model).info.police = true;
        if wanted < 3
            || person.stun > 0
            || cop.health < 1
            || p.health < 1
            || cop.state == State::Dead
        {
            continue;
        }
        // Head for the player (his driver's door when driving), leading
        // by (5 - aggr) * 3 ticks.
        let lead = (5 - aggr).max(0) * 3;
        let mut goal = p.pos + p.vel * lead;
        if !p_on_foot {
            let (s, c) = (trig.sin(p.heading), trig.cos(p.heading));
            let lat = -(p_info.radius + 192);
            let lon = p_info.max_y / 2;
            goal += IVec3::new((c * lat + s * lon) >> 8, (-s * lat + c * lon) >> 8, 0);
        }
        let want = heading_of(goal.x - cop.pos.x, goal.y - cop.pos.y);
        let err = angle_diff(cop.heading, want);
        *stick = Stick {
            steer: if err > 0 {
                -1
            } else if err < 0 {
                1
            } else {
                0
            },
            throttle: if err.abs() < 200 && cop.speed < 0x30 {
                -1
            } else {
                0
            },
            ..default()
        };
        // Arrest: close, the player nearly stopped, same level.
        let d = (goal - cop.pos).truncate() / 16;
        let reach = if p_on_foot {
            (192 + 192) * 3 / 32
        } else {
            192 / 8
        };
        if d.length_squared() < reach * reach && p.speed.abs() < 0x1e && p.level == cop.level {
            stick.throttle = 0;
            let open = match door.as_deref_mut() {
                Some(dr) if !p_on_foot => {
                    dr.door = (dr.door + 100).min(0x370);
                    dr.door >= 0x370
                }
                _ => true,
            };
            if open {
                info!("busted by a cop (wanted {wanted})");
                bust(
                    &mut commands,
                    pe,
                    &mut p,
                    &mut session,
                    &mut record,
                    texts.as_deref(),
                    &mut title,
                    &mut dialog,
                    &mut rng,
                );
                return;
            }
        }
        // Shooting chance per tick.
        let n = if aggr < 2 {
            (wanted > 4).then_some((9 - wanted) * 10)
        } else if aggr < 4 {
            (wanted > 3).then_some((9 - wanted) * 10)
        } else {
            (wanted > 3).then_some((7 - wanted) * 10)
        };
        if let (Some(n), Some(mut ars)) = (n, ars) {
            if ars.current != Weapon::Shotgun {
                ars.current = Weapon::Shotgun;
            }
            stick.fire = rng.rand(n.max(1)) < 1;
        }
    }
}

/// Busted: frozen, crime summary, respawn after 80 ticks (§2.5).
#[allow(clippy::too_many_arguments)]
fn bust(
    commands: &mut Commands,
    pe: Entity,
    p: &mut Obj,
    session: &mut PlayerSession,
    record: &mut Crimes,
    texts: Option<&Texts>,
    title: &mut BigTitle,
    dialog: &mut Dialog,
    rng: &mut Rng,
) {
    commands.entity(pe).insert(Busted);
    p.state = State::Dead;
    session.respawn_timer = 80;
    if let Some(t) = texts {
        title.show(t.global(2017));
        dialog.begin(Speaker::Lips);
        dialog.say(crime_summary(t, record, true), Voice::Them);
        dialog.end();
    }
    *record = Crimes {
        cops_killed: record.cops_killed,
        ..default()
    };
    commands.spawn(Sound::once(
        id::OW1 + rng.rand(3) as u16,
        5e6,
        8000.0,
        SoundPos::At(p.posf()),
    ));
}

/// "You are charged with ..." / "You committed ..." (68k `FUN_00094d52`):
/// counts in the order shunt, carjack, murder, firearms, criminal damage,
/// speeding, GBH; singular/plural phrases 2001..2014, "and" (2000).
pub fn crime_summary(t: &Texts, c: &Crimes, arrested: bool) -> String {
    let items = [
        (c.shunt, 2001),
        (c.carjacking, 2003),
        (c.murder, 2005),
        (c.firearms, 2007),
        (c.criminal_damage, 2009),
        (c.speeding, 2011),
        (c.gbh, 2013),
    ];
    let parts: Vec<String> = items
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|&(n, phrase)| {
            let word = t.global(if n == 1 { phrase } else { phrase + 1 });
            format!("{n} {word}")
        })
        .collect();
    let head = t.global(if arrested { 2015 } else { 2030 });
    let body = match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!(
            "{} {} {}",
            parts[..n - 1].join(", "),
            t.global(2000),
            parts[n - 1]
        ),
    };
    format!("{head} {body}.")
}

/// Reset per-level police state.
pub fn reset_police(mut commands: Commands) {
    commands.insert_resource(WantedTimer::default());
    commands.insert_resource(Crimes::default());
}
