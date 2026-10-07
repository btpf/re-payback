//! Kill scoring (docs/re/combat_police.md §7; OS4 `FUN_01043924`,
//! `FUN_0104405c`, `FUN_01056c58`): 1500 per civilian, 2500 per police
//! officer or police vehicle, times the multiplier, only when the player
//! dealt the damage; every 11th cop kill adds the 100000 "Copkiller
//! bonus!" (single player).

use super::Obj;
use super::people::PlayerSession;
use super::police::Hurt;
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::hud::{BigTitle, Texts};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// A person died or an occupied vehicle was wrecked.
#[derive(Message, Clone, Copy, Debug)]
pub struct Killed {
    pub victim: Entity,
    /// Police officer or police vehicle (`model+0x490a`).
    pub police: bool,
}

/// Who last hurt each object (true = the player).
#[derive(Resource, Default)]
pub struct LastAttacker(HashMap<Entity, bool>);

/// Cop kills since the last bonus (`DAT_0114c6a4`).
#[derive(Resource, Default)]
pub struct CopKills(pub u32);

const CIVILIAN: i32 = 1500;
const POLICE: i32 = 2500;
const COPKILLER_BONUS: i32 = 100_000;
const COPKILLER_MESSAGE: u32 = 2037;

pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Killed>()
            .init_resource::<LastAttacker>()
            .init_resource::<CopKills>()
            .add_systems(
                FixedUpdate,
                (track_attackers, score_kills).chain().after(super::combat::CombatSet),
            )
            .add_systems(OnExit(crate::level::AppState::Playing), |mut commands: Commands| {
                commands.insert_resource(LastAttacker::default());
                commands.insert_resource(CopKills::default());
            });
    }
}

fn track_attackers(mut hurt: MessageReader<Hurt>, mut last: ResMut<LastAttacker>, alive: Query<(), With<Obj>>) {
    for h in hurt.read() {
        last.0.insert(h.victim, h.by_player);
    }
    last.0.retain(|e, _| alive.contains(*e));
}

#[allow(clippy::too_many_arguments)]
fn score_kills(
    mut commands: Commands,
    mut killed: MessageReader<Killed>,
    mut last: ResMut<LastAttacker>,
    mut cops: ResMut<CopKills>,
    session: Res<PlayerSession>,
    texts: Option<Res<Texts>>,
    mut title: ResMut<BigTitle>,
    mut player: Query<(Entity, &mut Obj), With<CameraTarget>>,
) {
    for k in killed.read() {
        let Ok((pe, mut p)) = player.single_mut() else { continue };
        if k.victim == pe || last.0.remove(&k.victim) != Some(true) {
            continue;
        }
        p.score += if k.police { POLICE } else { CIVILIAN } * session.multiplier;
        if !k.police {
            continue;
        }
        cops.0 += 1;
        if cops.0 > 10 {
            cops.0 = 0;
            p.score += COPKILLER_BONUS;
            if let Some(t) = &texts {
                title.show(t.global(COPKILLER_MESSAGE));
            }
            commands.spawn(Sound::once(id::MISSION_COMPLETE, 255.0, 11025.0, SoundPos::Global));
        }
    }
}
