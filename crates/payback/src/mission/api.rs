//! The script API (docs/re/script_api.md §5), as methods on [`Script`].
//!
//! Names follow the original functions; 68k addresses are in the docs.
//! Coordinates are original units (0x800 per square), floors are cube
//! levels 0..2, angles 5760 per turn, phrase numbers are level phrases
//! unless noted.

use super::triggers::{Trigger, Triggers, kind};
use super::{MissionState, PhoneSlot};
use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::hud::HudData;
use crate::hud::{BigTitle, Dialog, Speaker, Texts, Voice};
use crate::mapedit::{CityRender, EditOp, MapEdit, RegionData, Regions};
use crate::objects::{ObjectLibrary, SetScenery};
use crate::sim::combat::Explode;
use crate::sim::people::{self, PEOPLE_MODELS, PlayerSession};
use crate::sim::{Obj, Rng, State};
use crate::vehicles;
use crate::world::{City, z_of_h};
use bevy::prelude::*;
use payback_formats::map::MAP_SIZE;

/// Global phrase numbers used by the API.
pub const MISSION_COMPLETE: u32 = 2022;
pub const LEVEL_COMPLETE: u32 = 2023;
pub const MISSION_FAILED: u32 = 2028;

/// Script access to the game world.
pub struct Script<'w> {
    pub world: &'w mut World,
}

impl<'w> Script<'w> {
    pub fn new(world: &'w mut World) -> Script<'w> {
        Script { world }
    }

    fn state(&mut self) -> Mut<'_, MissionState> {
        self.world.resource_mut::<MissionState>()
    }

    fn triggers(&mut self) -> Mut<'_, Triggers> {
        self.world.resource_mut::<Triggers>()
    }

    // ---- Triggers --------------------------------------------------------

    /// `AddTrigger(ev, type, a, b, c, d)` (untracked).
    pub fn add_trigger(&mut self, ev: i32, ty: u8, a: i32, b: i32, c: i32, d: i32) {
        self.add_trigger_ex(ev, ty, a, b, c, d, 0);
    }

    /// `AddTriggerEx` with the 7th argument (type 0x39 rectangles).
    #[allow(clippy::too_many_arguments)]
    pub fn add_trigger_ex(&mut self, ev: i32, ty: u8, a: i32, b: i32, c: i32, d: i32, e: i32) {
        let mut t = Trigger::new(ev, ty, [a, b, c, d, e]);
        // Type-specific setup at add time.
        match ty {
            kind::MONEY_GOAL => {
                self.state().goal = a;
                if a <= 0 {
                    return;
                }
            }
            kind::COUNTDOWN | kind::TIMER => t.counter = a as i64 * 30,
            kind::TIMER_TICKS => {
                t.kind = kind::TIMER;
                t.counter = a as i64;
            }
            kind::MONEY_GAIN => t.counter = self.money() as i64 + a as i64,
            kind::VEHICLE_CHANGED => t.counter = self.player_vehicle_type() as i64,
            kind::ACTOR_GOTO_DIRECT => t.kind = kind::ACTOR_GOTO,
            kind::ACTOR_X_LE | kind::ACTOR_X_GT | kind::ACTOR_Y_LE | kind::ACTOR_Y_GT
                if a == -1 =>
            {
                t.kind = ty - 0x15; // 0x33..0x36 -> 0x1e..0x21
            }
            kind::PHONE | kind::SECRET_PHONE => {
                if let Some((pos, _)) = self.phone_coords(a) {
                    let s = self
                        .world
                        .spawn(Sound::looping(
                            id::TELEPHONE_RING,
                            10_000_000.0,
                            17000.0,
                            SoundPos::At(pos.as_vec3()),
                        ))
                        .id();
                    t.sound = Some(s);
                }
            }
            _ => {}
        }
        // Actor-referencing types: one trigger per matching actor.
        if actor_trigger(t.kind) {
            let actors = self.actors(a);
            for actor in actors {
                let mut ta = t.clone();
                ta.actor = Some(actor);
                self.apply_actor_setup(&ta, actor);
                if ty == kind::ACTOR_GOTO_DIRECT {
                    // 0x32 also sets the sticky "race driver" flag +0x49e.
                    crate::sim::ai::set_direct(self.world, actor);
                }
                self.triggers().queue_add(ta);
            }
            return;
        }
        self.triggers().queue_add(t);
    }

    /// AI side effects of adding actor triggers (goto, board, flee).
    fn apply_actor_setup(&mut self, t: &Trigger, actor: Entity) {
        let goal = IVec2::new(t.b, t.c);
        let order = match t.kind {
            kind::ACTOR_GOTO_VANISH => Some(crate::sim::ai::Order::GotoVanish(goal)),
            kind::ACTOR_GOTO => Some(crate::sim::ai::Order::Goto(
                goal,
                if t.d > 0 { t.d } else { 1000 },
            )),
            kind::ACTOR_GOTO_15 => Some(crate::sim::ai::Order::Goto15(goal)),
            kind::ACTOR_BOARDS_PLAYER => Some(crate::sim::ai::Order::BoardPlayer),
            kind::ACTOR_FLEES => Some(crate::sim::ai::Order::FleeFar),
            kind::ACTOR_BOARDS_ACTOR => self
                .actors(t.b)
                .first()
                .map(|&target| crate::sim::ai::Order::BoardActor(target)),
            _ => None,
        };
        if let Some(order) = order {
            crate::sim::ai::give_order(self.world, actor, order);
        }
    }

    /// `AddMissionTrigger`: tracked so the mission can be cancelled at once.
    pub fn t(&mut self, ev: i32, ty: u8, a: i32, b: i32, c: i32, d: i32) {
        self.t_ex(ev, ty, a, b, c, d, 0);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn t_ex(&mut self, ev: i32, ty: u8, a: i32, b: i32, c: i32, d: i32, e: i32) {
        self.state().tracked.retain(|&x| x != ev);
        self.add_trigger_ex(ev, ty, a, b, c, d, e);
        self.state().tracked.push(ev);
    }

    /// `KillMissionTrigger`.
    pub fn kill_mt(&mut self, ev: i32) {
        self.triggers().queue_kill(ev);
        self.state().tracked.retain(|&x| x != ev);
    }

    /// `KillAllMissionTriggers`.
    pub fn kill_all_mt(&mut self) {
        let tracked = std::mem::take(&mut self.state().tracked);
        for ev in tracked {
            self.triggers().queue_kill(ev);
        }
    }

    /// `KillTrigger` (untracked).
    pub fn kill_trigger(&mut self, ev: i32) {
        self.triggers().queue_kill(ev);
    }

    // ---- Dialogue and messages -------------------------------------------

    fn phrase(&self, n: u32) -> String {
        self.world.resource::<Texts>().level(n)
    }

    pub fn global_phrase(&self, n: u32) -> String {
        self.world.resource::<Texts>().global(n)
    }

    pub fn dialog_begin(&mut self, speaker: i32) {
        let s = match speaker {
            0 => Speaker::Phone,
            1 => Speaker::Lips,
            2 => Speaker::Mobile,
            3 => Speaker::Pager,
            _ => Speaker::Info,
        };
        self.world.resource_mut::<Dialog>().begin(s);
    }

    /// `Dialog_Say1`: the player (Travis) speaks.
    pub fn say1(&mut self, n: u32) {
        let text = self.phrase(n);
        crate::autoplay::log(format_args!("player says {n}: {text}"));
        self.world.resource_mut::<Dialog>().say(text, Voice::You);
    }

    /// `Dialog_Say2`: the other party speaks.
    pub fn say2(&mut self, n: u32) {
        let text = self.phrase(n);
        crate::autoplay::log(format_args!("says {n}: {text}"));
        self.world.resource_mut::<Dialog>().say(text, Voice::Them);
    }

    pub fn dialog_end(&mut self) {
        self.world.resource_mut::<Dialog>().end();
    }

    pub fn dialog_flush(&mut self) {
        self.world.resource_mut::<Dialog>().flush();
    }

    /// `Say(s, n)` = begin(s); say2(n); end().
    pub fn say(&mut self, speaker: i32, n: u32) {
        self.dialog_begin(speaker);
        self.say2(n);
        self.dialog_end();
    }

    /// `Hint(n)` = begin(4); say1(n); end().
    pub fn hint(&mut self, n: u32) {
        self.dialog_begin(4);
        self.say1(n);
        self.dialog_end();
    }

    /// `ShowMessage(phrase)`: the big centre message. Level phrase numbers
    /// below 2000; global ones (2022 complete, 2028 failed, ...) above.
    pub fn show_message(&mut self, n: u32) {
        let text = if n >= 2000 {
            self.global_phrase(n)
        } else {
            self.phrase(n)
        };
        let money = self.money();
        crate::autoplay::log(format_args!("message {n}: {text} (money {money})"));
        self.world.resource_mut::<BigTitle>().show(text);
        match n {
            MISSION_COMPLETE => {
                let mut s = self.world.resource_mut::<PlayerSession>();
                s.multiplier = (s.multiplier + 1).min(4);
                self.world.spawn(Sound::once(
                    id::MISSION_COMPLETE,
                    255.0,
                    11025.0,
                    SoundPos::Global,
                ));
            }
            MISSION_FAILED => {
                self.world.spawn(Sound::once(
                    id::MISSION_FAIL,
                    255.0,
                    11025.0,
                    SoundPos::Global,
                ));
            }
            _ => {}
        }
    }

    // ---- Money, lives, wanted --------------------------------------------

    pub fn player(&mut self) -> Option<Entity> {
        self.world
            .query_filtered::<Entity, With<CameraTarget>>()
            .iter(self.world)
            .next()
    }

    fn player_obj(&mut self) -> Option<Mut<'_, Obj>> {
        let p = self.player()?;
        self.world.get_mut::<Obj>(p)
    }

    pub fn money(&mut self) -> i32 {
        self.player_obj().map(|o| o.score).unwrap_or(0)
    }

    /// `AddMoney(n)`: n × the bonus multiplier.
    pub fn add_money(&mut self, n: i32) {
        let mult = self.world.resource::<PlayerSession>().multiplier;
        if let Some(mut o) = self.player_obj() {
            o.score += n * mult;
        }
    }

    pub fn add_lives(&mut self, n: i32) {
        self.world.resource_mut::<PlayerSession>().lives += n;
    }

    pub fn add_health(&mut self, n: i32) {
        if let Some(mut o) = self.player_obj() {
            o.health = (o.health + n).max(0);
        }
    }

    pub fn set_wanted_level(&mut self, n: i32) {
        if let Some(mut o) = self.player_obj() {
            o.wanted = n;
        }
    }

    /// `LevelComplete()`.
    pub fn level_complete(&mut self) {
        self.state().finished = true;
        self.show_message(LEVEL_COMPLETE);
        self.world.spawn(Sound::once(
            id::MISSION_COMPLETE,
            255.0,
            11025.0,
            SoundPos::Global,
        ));
        self.world.write_message(super::LevelOutcome::Complete);
    }

    /// Type of the vehicle the player drives, or -1 on foot.
    pub fn player_vehicle_type(&mut self) -> i32 {
        let Some(p) = self.player() else { return -1 };
        self.world
            .get::<vehicles::Vehicle>(p)
            .map(|v| v.kind as i32)
            .unwrap_or(-1)
    }

    pub fn random(&mut self, n: i32) -> i32 {
        self.world.resource_mut::<Rng>().rand(n)
    }

    // ---- Phones ----------------------------------------------------------

    /// Position (square centre, floor z) and floor of map phone `n`:
    /// the cell with scenery 14 (phone) or 19 (flashing) and special n.
    pub fn phone_coords(&self, n: i32) -> Option<(IVec3, usize)> {
        let city = self.world.get_resource::<City>()?;
        for x in 0..MAP_SIZE {
            for y in 0..MAP_SIZE {
                for (lvl, c) in city.map.square(x, y).cubes.iter().enumerate() {
                    if matches!(c.scenery, 14 | 19) && c.special as i32 == n {
                        let top = city.square_cubes(x, y)[lvl].top_at(0.5, 0.5);
                        let pos = IVec3::new(
                            (x as i32) << 11 | 0x400,
                            (y as i32) << 11 | 0x400,
                            z_of_h(top) as i32,
                        );
                        return Some((pos, lvl));
                    }
                }
            }
        }
        None
    }

    pub fn set_phone(&mut self, slot: usize, event: i32, phone: i32, hidden: bool) {
        self.state().phones[slot] = PhoneSlot {
            event,
            phone,
            hidden,
        };
    }

    pub fn clear_phone(&mut self, slot: usize) {
        self.state().phones[slot].event = -1;
    }

    pub fn phone_event(&mut self, slot: usize) -> i32 {
        self.state().phones[slot].event
    }

    pub fn cur_slot(&mut self) -> i32 {
        self.state().cur_slot
    }

    pub fn set_cur_slot(&mut self, slot: i32) {
        self.state().cur_slot = slot;
    }

    pub fn set_intro_phrase(&mut self, n: u32) {
        self.state().intro_phrase = n;
    }

    /// `RefreshPhones()` (68k `0xb4df8`): ring every filled slot; on the
    /// level's first call show the intro, later a random "head back to the
    /// phones" line if a visible phone rings.
    pub fn refresh_phones(&mut self) {
        let phones = self.state().phones;
        let mut visible = false;
        for slot in phones.iter().filter(|s| s.event != -1) {
            let ty = if slot.hidden {
                kind::SECRET_PHONE
            } else {
                kind::PHONE
            };
            self.add_trigger(slot.event, ty, slot.phone, 0, 0, 0);
            visible |= !slot.hidden;
        }
        let first = std::mem::replace(&mut self.state().first_phones, false);
        if first {
            let n = self.state().intro_phrase;
            self.say(2, n);
        } else if visible {
            let n = 2150 + self.random(9) as u32;
            let text = self.global_phrase(n);
            self.dialog_begin(2);
            self.world.resource_mut::<Dialog>().say(text, Voice::Them);
            self.dialog_end();
        }
    }

    /// `KillAllPhones()` (68k `0xb4ed8`).
    pub fn kill_all_phones(&mut self) {
        let phones = self.state().phones;
        for slot in phones.iter().filter(|s| s.event != -1) {
            self.kill_trigger(slot.event);
        }
    }

    /// Clear the answered slot, then refresh phones (the common tail of the
    /// per-level unlock routines).
    pub fn consume_slot(&mut self) {
        let slot = self.state().cur_slot;
        if (0..super::PHONE_SLOTS as i32).contains(&slot) {
            self.clear_phone(slot as usize);
        }
    }

    // ---- Actors ----------------------------------------------------------

    /// Every live actor with script id `id`.
    pub fn actors(&mut self, id: i32) -> Vec<Entity> {
        if id == -1 {
            return self.player().into_iter().collect();
        }
        self.world
            .query::<(Entity, &Obj)>()
            .iter(self.world)
            .filter(|(_, o)| o.script_id == id && o.state != State::Removed)
            .map(|(e, _)| e)
            .collect()
    }

    /// `CreateActor(id, model, x, y, floor, angle, colour)`: model < 0 is
    /// person `~model`, 0..999 vehicle, >= 1000 special object.
    #[allow(clippy::too_many_arguments)]
    pub fn create_actor(
        &mut self,
        id: i32,
        model: i32,
        x: i32,
        y: i32,
        floor: i32,
        angle: i32,
        colour: i32,
    ) -> Option<Entity> {
        let z = self.floor_z(x, y, floor as usize);
        let pos = IVec3::new(x, y, z);
        let e = if model < 0 {
            let kind = (!model) as usize;
            if kind >= PEOPLE_MODELS.len() {
                return None;
            }
            self.world
                .resource_scope(|world, mut library: Mut<ObjectLibrary>| {
                    let data = world.resource::<GameData>().clone();
                    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                        let mut commands = world.commands();
                        people::spawn_person(
                            &mut commands,
                            &data,
                            &mut library,
                            &mut meshes,
                            kind,
                            pos,
                            angle,
                            State::Idle,
                        )
                    })
                })
        } else if model >= 1000 {
            vehicles::spawn_prop_world(self.world, model - 1000, pos, angle)
        } else {
            vehicles::spawn_vehicle_world(self.world, model, pos, angle, colour)
        }?;
        self.world.flush();
        if let Some(mut o) = self.world.get_mut::<Obj>(e) {
            o.script_id = id;
            o.level = floor.clamp(0, 2) as usize;
        }
        // Every new object starts with skill `+0x346 = Random(100)` (68k
        // object constructor `FUN_000a2b80`); scripts change it with
        // `Actor_SetAggro`.
        let skill = self.world.resource_mut::<crate::sim::Rng>().rand(100);
        if self.world.get::<crate::sim::ai::Ai>(e).is_none() {
            self.world
                .entity_mut(e)
                .insert(crate::sim::ai::Ai { aggro: skill, ..Default::default() });
        }
        Some(e)
    }

    /// `CreateActorFx`: as create_actor with sparkles, ObjectAppear sound and
    /// a damage filter.
    #[allow(clippy::too_many_arguments)]
    pub fn create_actor_fx(
        &mut self,
        id: i32,
        model: i32,
        x: i32,
        y: i32,
        floor: i32,
        angle: i32,
        colour: i32,
        filter: i32,
    ) {
        if let Some(e) = self.create_actor(id, model, x, y, floor, angle, colour) {
            if let Some(mut o) = self.world.get_mut::<Obj>(e) {
                o.filter = filter;
            }
            self.world.spawn(Sound::once(
                id::OBJECT_APPEAR,
                20e6,
                8000.0,
                SoundPos::At(Vec3::new(x as f32, y as f32, 0.0)),
            ));
        }
    }

    fn floor_z(&self, x: i32, y: i32, floor: usize) -> i32 {
        let Some(city) = self.world.get_resource::<City>() else {
            return 0x4dc0;
        };
        let (sx, sy) = ((x >> 11) as usize, (y >> 11) as usize);
        if sx >= MAP_SIZE || sy >= MAP_SIZE {
            return 0x4dc0;
        }
        let geom = city.square_cubes(sx, sy)[floor.min(2)];
        z_of_h(geom.top_at(0.5, 0.5)) as i32
    }

    fn for_actors(&mut self, id: i32, f: impl Fn(&mut Obj)) {
        for e in self.actors(id) {
            if let Some(mut o) = self.world.get_mut::<Obj>(e) {
                if o.state != State::Player {
                    f(&mut o);
                }
            }
        }
    }

    /// `Actor_Release(id)`: becomes inert/parked, id cleared.
    pub fn actor_release(&mut self, id: i32) {
        for e in self.actors(id) {
            crate::sim::ai::give_order(self.world, e, crate::sim::ai::Order::Park);
        }
        self.for_actors(id, |o| o.script_id = -1);
    }

    /// `Actor_ReleaseIdle(id)`: becomes ambient (wandering / traffic).
    pub fn actor_release_idle(&mut self, id: i32) {
        for e in self.actors(id) {
            crate::sim::ai::give_order(self.world, e, crate::sim::ai::Order::Ambient);
        }
        self.for_actors(id, |o| o.script_id = -1);
    }

    /// `Actor_Remove(id)`.
    pub fn actor_remove(&mut self, id: i32) {
        for e in self.actors(id) {
            let player = self
                .world
                .get::<Obj>(e)
                .is_some_and(|o| o.state == State::Player);
            if !player {
                self.world.entity_mut(e).despawn();
            }
        }
    }

    /// `Actor_Kill(id)`: health -71 (blown apart).
    pub fn actor_kill(&mut self, id: i32) {
        self.for_actors(id, |o| o.health = -71);
    }

    /// `Actor_Attack(id, mode, target)`.
    pub fn actor_attack(&mut self, id: i32, mode: i32, target: i32) {
        let target_e = if target == -1 {
            None
        } else {
            self.actors(target).last().copied()
        };
        for e in self.actors(id) {
            let order = if mode == 0 {
                crate::sim::ai::Order::Idle
            } else if mode > 0 {
                crate::sim::ai::Order::Attack {
                    aggro: mode,
                    target: target_e,
                }
            } else {
                crate::sim::ai::Order::Flee
            };
            crate::sim::ai::give_order(self.world, e, order);
        }
    }

    /// `Actor_Follow(id, leader)`: -1 follow the player, other negatives stop.
    pub fn actor_follow(&mut self, id: i32, leader: i32) {
        let lead = if leader == -1 {
            self.player()
        } else if leader < 0 {
            None
        } else {
            self.actors(leader).first().copied()
        };
        for e in self.actors(id) {
            crate::sim::ai::give_order(self.world, e, crate::sim::ai::Order::Follow(lead));
        }
    }

    /// `Actor_SetAggro(id, v)`: fire rate / driver aggression.
    pub fn actor_set_aggro(&mut self, id: i32, v: i32) {
        for e in self.actors(id) {
            crate::sim::ai::set_aggro(self.world, e, v);
        }
    }

    /// `Actor_SetMaxSpeed(id, v)` (`+0x386`).
    pub fn actor_set_max_speed(&mut self, id: i32, v: i32) {
        for e in self.actors(id) {
            crate::sim::ai::set_max_speed(self.world, e, v);
        }
    }

    /// `SetPlayerId(id)`: tags the vehicle the player drives.
    pub fn set_player_id(&mut self, id: i32) {
        let Some(p) = self.player() else { return };
        if self.world.get::<vehicles::Vehicle>(p).is_some() {
            if let Some(mut o) = self.world.get_mut::<Obj>(p) {
                o.script_id = id;
            }
        }
    }

    /// `RemoveAllActors()`.
    pub fn remove_all_actors(&mut self) {
        let all: Vec<Entity> = self
            .world
            .query::<(Entity, &Obj)>()
            .iter(self.world)
            .filter(|(_, o)| o.state != State::Player && o.class != crate::sim::Class::Projectile)
            .map(|(e, _)| e)
            .collect();
        for e in all {
            self.world.entity_mut(e).despawn();
        }
    }

    // ---- Map edits ---------------------------------------------------------

    /// `SetScenery(sx, sy, cube, scenery, rotation, special)`.
    pub fn set_scenery(
        &mut self,
        sx: i32,
        sy: i32,
        cube: i32,
        scenery: i32,
        rotation: i32,
        special: i32,
    ) {
        if !(0..MAP_SIZE as i32).contains(&sx) || !(0..MAP_SIZE as i32).contains(&sy) {
            return;
        }
        self.world.write_message(SetScenery {
            sx: sx as usize,
            sy: sy as usize,
            level: cube.clamp(0, 2) as usize,
            scenery: scenery as u8,
            special: Some(special as u8),
            rotation: Some(rotation as u8),
        });
        if scenery != 0 && scenery != 23 {
            let at = Vec3::new(
                ((sx << 11) | 0x400) as f32,
                ((sy << 11) | 0x400) as f32,
                0.0,
            );
            self.world.spawn(Sound::once(
                id::OBJECT_APPEAR,
                20e6,
                8000.0,
                SoundPos::At(at),
            ));
        }
    }

    /// `SetCube(sx, sy, cube, h1, h2, scenery, rotation, slope)`; slope 0
    /// removes the cube.
    #[allow(clippy::too_many_arguments)]
    pub fn set_cube(
        &mut self,
        sx: i32,
        sy: i32,
        cube: i32,
        h1: i32,
        h2: i32,
        scenery: i32,
        rotation: i32,
        slope: i32,
    ) {
        self.map_edit(
            sx,
            sy,
            cube,
            EditOp::Cube {
                h1: h1 as i16,
                h2: h2 as i16,
                scenery: scenery as u8,
                rotation: rotation as u8,
                slope: slope as u8,
            },
        );
        // The scenery entity follows the cell.
        if (0..MAP_SIZE as i32).contains(&sx) && (0..MAP_SIZE as i32).contains(&sy) {
            self.world.write_message(SetScenery {
                sx: sx as usize,
                sy: sy as usize,
                level: cube.clamp(0, 2) as usize,
                scenery: scenery as u8,
                special: None,
                rotation: Some(rotation as u8),
            });
        }
    }

    /// `SetFaceTop/N/E/S/W(sx, sy, cube, type, texture, flip)`: `face` 0 top,
    /// 1 N, 2 E, 3 S, 4 W.
    #[allow(clippy::too_many_arguments)]
    pub fn set_face(
        &mut self,
        face: usize,
        sx: i32,
        sy: i32,
        cube: i32,
        kind: i32,
        texture: i32,
        flip: i32,
    ) {
        self.map_edit(
            sx,
            sy,
            cube,
            EditOp::Face {
                face: face.min(4),
                kind: kind as u8,
                texture: texture as u16,
                flip: flip != 0,
            },
        );
    }

    fn map_edit(&mut self, sx: i32, sy: i32, cube: i32, op: EditOp) {
        if !(0..MAP_SIZE as i32).contains(&sx)
            || !(0..MAP_SIZE as i32).contains(&sy)
            || !(0..3).contains(&cube)
        {
            return;
        }
        self.world.write_message(MapEdit {
            sx: sx as usize,
            sy: sy as usize,
            cube: cube as usize,
            op,
        });
    }

    /// `StoreRegion(x0, y0, x1, y1, cube, slot)`: copy up to 40x40 cubes.
    #[allow(clippy::too_many_arguments)]
    pub fn store_region(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, cube: i32, slot: usize) {
        let Some(city) = self.world.get_resource::<City>() else {
            return;
        };
        let (w, h) = (
            (x1 - x0 + 1).clamp(0, 40) as usize,
            (y1 - y0 + 1).clamp(0, 40) as usize,
        );
        let mut cubes = Vec::with_capacity(w * h);
        for dx in 0..w {
            for dy in 0..h {
                let x = (x0.max(0) as usize + dx).min(MAP_SIZE - 1);
                let y = (y0.max(0) as usize + dy).min(MAP_SIZE - 1);
                cubes.push(city.map.square(x, y).cubes[cube.clamp(0, 2) as usize]);
            }
        }
        if let Some(r) = self.world.resource_mut::<Regions>().0.get_mut(slot) {
            *r = Some(RegionData { w, h, cubes });
        }
    }

    /// `PasteRegion(x, y, cube, slot)`.
    pub fn paste_region(&mut self, x: i32, y: i32, cube: i32, slot: usize) {
        let Some(Some(r)) = self.world.resource::<Regions>().0.get(slot).cloned() else {
            return;
        };
        for dx in 0..r.w {
            for dy in 0..r.h {
                let c = r.cubes[dx * r.h + dy];
                let (sx, sy) = (x + dx as i32, y + dy as i32);
                let slope = slope_code(c.slope);
                self.set_cube(
                    sx,
                    sy,
                    cube,
                    c.height1 as i32,
                    c.height2 as i32,
                    c.scenery as i32,
                    c.rotation as i32,
                    slope,
                );
                for (f, face) in c.faces.iter().enumerate() {
                    self.set_face(
                        f,
                        sx,
                        sy,
                        cube,
                        face_code(face.kind),
                        face.texture as i32,
                        face.flipped as i32,
                    );
                }
            }
        }
    }

    /// `SetVisibleLayers(n)`: draw only cube layers below n (roofs cut away
    /// indoors).
    pub fn set_visible_layers(&mut self, n: i32) {
        if let Some(mut r) = self.world.get_resource_mut::<CityRender>() {
            r.visible_layers = n.clamp(0, 3) as usize;
        }
    }

    /// `SetRaceLights(n)`: HUD start lights 0..3, -1 hides; beeps for 1-3.
    pub fn set_race_lights(&mut self, n: i32) {
        self.world.resource_mut::<HudData>().start_lights = (n >= 0).then_some(n as usize);
        match n {
            1 | 2 => {
                self.world
                    .spawn(Sound::once(id::BEEP1, 255.0, 4182.0, SoundPos::Global));
            }
            3 => {
                self.world
                    .spawn(Sound::once(id::BEEP2, 255.0, 8363.0, SoundPos::Global));
            }
            _ => {}
        }
    }

    /// `SetLap(cur, total)`: HUD "Lap %d of %d"; cur 0 hides.
    pub fn set_lap(&mut self, cur: i32, total: i32) {
        self.world.resource_mut::<HudData>().lap = (cur > 0).then_some((cur, total));
        if cur > 1 {
            self.world
                .spawn(Sound::once(id::BEEP3, 255.0, 8363.0, SoundPos::Global));
        }
    }

    /// `ShowCheckpointMarker(b)`: draw the flashing marker at the target.
    pub fn show_checkpoint_marker(&mut self, on: bool) {
        self.world.resource_mut::<HudData>().checkpoint = on;
    }

    /// `SetRespawnPoint(sx, sy, floor)`; -1 = the map start points.
    pub fn set_respawn_point(&mut self, sx: i32, sy: i32, floor: i32) {
        self.world.resource_mut::<PlayerSession>().respawn_square =
            (sx >= 0).then_some((sx as usize, sy as usize, floor.clamp(0, 2) as usize));
    }

    /// Helicopter take-off (`0xb4666`, up) / landing (`0xb46dc`).
    pub fn heli_take_off(&mut self, id: i32, up: bool) {
        for e in self.actors(id) {
            vehicles::set_flying(self.world, e, up);
        }
    }

    /// `AddEffect(x, y, z, vx, vy, vz, life)`: a Gas spray particle.
    #[allow(clippy::too_many_arguments)]
    pub fn add_effect(&mut self, x: i32, y: i32, z: i32, vx: i32, vy: i32, vz: i32, life: i32) {
        self.world.resource_mut::<crate::effects::Effects>().add(
            9,
            crate::effects::Tex::Gas,
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new(vx as f32, vy as f32, vz as f32),
            life as f32,
            3,
            crate::effects::Blend::Add,
        );
    }

    /// `SetForcedTraffic(m)` (`0x11a734`): traffic spawns vehicle type m
    /// whenever none exists; -1 = random.
    pub fn set_forced_traffic(&mut self, m: i32) {
        self.state().forced_traffic = m;
    }

    /// `SetProjectileId(id)`: script id given to bombs the player drops.
    pub fn set_projectile_id(&mut self, id: i32) {
        self.state().projectile_id = id;
    }

    /// `Actor_SetControls(id, on)` (`+0x49c`): the player can control that
    /// vehicle.
    pub fn actor_set_controls(&mut self, id: i32, on: bool) {
        for e in self.actors(id) {
            vehicles::set_controls(self.world, e, on);
        }
    }

    /// Pedestrian `model` (negative = person) with script id `new_id` steps
    /// out of vehicle `veh_id` (-1: the vehicle the player drives).
    /// `side` 0 = the driver's door (model -X), 1 = the other side, -1 =
    /// alternating (`0xb43d4`/`0xb4414` call `0xdb03e`, which flips
    /// `0x34c1b0` each time; `0xb448e` passes the side to `0xdb196`). The
    /// toggle is reset to 1 before each frame's triggers, so the first
    /// alternating exit of a frame uses the driver's door, the second the
    /// other side. The ped stands (car half-width + 192)·6/5 out at
    /// the door's longitudinal position, facing 90° left of the car; its
    /// previous position is the car's centre, so walls between push it back.
    pub fn ped_exits_vehicle(&mut self, new_id: i32, veh_id: i32, model: i32, side: i32) {
        let veh = if veh_id == -1 {
            self.player()
        } else {
            self.actors(veh_id).first().copied()
        };
        let Some(veh) = veh else { return };
        let Some(v) = self.world.get::<Obj>(veh).cloned() else {
            return;
        };
        let kind = self.world.get::<vehicles::Vehicle>(veh).map_or(0, |k| k.kind);
        let info = self.world.resource::<ObjectLibrary>().model(v.model).info.clone();
        let right = match side {
            -1 => {
                let mut st = self.state();
                st.exit_toggle = !st.exit_toggle;
                !st.exit_toggle
            }
            0 => true,
            _ => false,
        };
        let lat = (info.radius + people::PERSON_RADIUS) * 6 / 5;
        let fwd = crate::sim::boarding::door_y(kind, info.max_y);
        let at = {
            let trig = self.world.resource::<crate::sim::Trig>();
            crate::sim::boarding::car_point(trig, &v, if right { -lat } else { lat }, fwd)
        };
        let heading = crate::sim::wrap(v.heading - 0x5a0);
        crate::autoplay::log(format_args!(
            "ped {new_id:#x} gets out of {veh_id:#x} at {at:?} (car at {:?} heading {:#x} speed {}, half-width {})",
            v.pos, v.heading, v.speed, info.radius
        ));
        if let Some(e) = self.create_actor(new_id, model, at.x, at.y, v.level as i32, heading, 0) {
            if let Some(mut o) = self.world.get_mut::<Obj>(e) {
                o.pos.z = v.pos.z;
                o.prev_pos = IVec3::new(v.pos.x, v.pos.y, v.pos.z);
                o.level = v.level;
            }
        }
    }

    /// `Explosion(x, y, z, owner, size)`.
    pub fn explosion(&mut self, x: i32, y: i32, z: i32, size: i32) {
        self.world.write_message(Explode {
            pos: IVec3::new(x, y, z),
            power: size as i64 * 20000,
            layer: 0,
            source: None,
            by_player: false,
            player: None,
        });
    }

    /// `AddSound(ref, sample, loop, vol, hz, x, y, z)` (68k `FUN_000b4bba`,
    /// OS4 `FUN_010cda74`): passes its arguments straight to the engine's
    /// AddSound (68k `FUN_000170ce`). The 8th argument is the sound's
    /// downward z, not a range; it matters because the mixer's loudness is
    /// `vol / d²` over the 3D distance (frontend_audio.md §4.3). The
    /// shipped calls use 20000 (street level) and 19000. `y == -1` is
    /// non-positional.
    #[allow(clippy::too_many_arguments)]
    pub fn add_sound(
        &mut self,
        slot: usize,
        sample: u16,
        looping: bool,
        vol: f32,
        hz: f32,
        x: i32,
        y: i32,
        z: i32,
    ) {
        self.delete_sound(slot);
        let pos = if y == -1 {
            SoundPos::Global
        } else {
            SoundPos::At(Vec3::new(x as f32, y as f32, z as f32))
        };
        let sound = if looping {
            Sound::looping(sample, vol, hz, pos)
        } else {
            Sound::once(sample, vol, hz, pos)
        };
        let e = self.world.spawn(sound).id();
        if let Some(s) = self.state().sounds.get_mut(slot) {
            *s = Some(e);
        }
    }

    pub fn delete_sound(&mut self, slot: usize) {
        if let Some(Some(e)) = self.state().sounds.get_mut(slot).map(Option::take) {
            if let Ok(em) = self.world.get_entity_mut(e) {
                em.despawn();
            }
        }
    }
}

/// Trigger kinds whose `a` is an actor id (one trigger per actor).
fn actor_trigger(ty: u8) -> bool {
    use kind::*;
    matches!(
        ty,
        AT_ACTOR_ON_FOOT
            | ENTER_ACTOR
            | ACTOR_GONE
            | ACTOR_BOARDS_PLAYER
            | NEAR_ACTOR
            | ACTOR_GOTO_VANISH
            | ACTOR_FASTER
            | ACTOR_SLOWER
            | ACTOR_OUTSIDE
            | ACTOR_GOTO
            | ACTOR_NEAR_PLAYER
            | ACTOR_LOST
            | ACTOR_SEES_PLAYER
            | ACTOR_NEAR_ACTOR
            | ACTOR_KILLED
            | ACTOR_IN_WATER
            | ENTER_ACTOR_QUIET
            | ACTOR_ATTACKED
            | ACTOR_ATTACKED_QUIET
            | ACTOR_SEES_PLAYER_QUIET
            | ACTOR_X_LE
            | ACTOR_X_GT
            | ACTOR_Y_LE
            | ACTOR_Y_GT
            | ACTOR_INSIDE
            | NEAR_ACTOR_1000
            | ACTOR_BOARDS_ACTOR
            | ACTOR_FLEES
            | ACTOR_GOTO_15
    )
}

fn slope_code(s: payback_formats::map::Slope) -> i32 {
    use payback_formats::map::Slope::*;
    match s {
        Off => 0,
        Flat => 1,
        Vert => 2,
        Horiz => 3,
        RVert => 4,
        RHoriz => 5,
        Unknown(n) => n as i32,
    }
}

fn face_code(f: payback_formats::map::FaceType) -> i32 {
    use payback_formats::map::FaceType::*;
    match f {
        Off => 0,
        Normal => 1,
        Genlocked => 2,
        Alpha => 3,
        Reflect => 4,
        Unknown(n) => n as i32,
    }
}

impl Script<'_> {
    /// Garage drop-off (trigger type 4): the player's car is taken and the
    /// player stands 0x800 outside side `d` (0 -y, 1 +x, 2 +y, 3 -x),
    /// keeping weapons, money, wanted level and on-foot health.
    pub fn garage_dropoff(&mut self, sx: i32, sy: i32, floor: i32, side: i32) {
        let Some(car) = self.player() else { return };
        let Some((score, wanted)) = self.world.get::<Obj>(car).map(|o| (o.score, o.wanted)) else {
            return;
        };
        let ars = self
            .world
            .get::<crate::sim::combat::Arsenal>(car)
            .cloned()
            .unwrap_or_default();
        let (dx, dy, heading) = match side {
            0 => (0, -0x800, 0xb40),
            1 => (0x800, 0, 0x5a0),
            2 => (0, 0x800, 0),
            _ => (-0x800, 0, 0x10e0),
        };
        let (x, y) = ((sx << 11 | 0x400) + dx, (sy << 11 | 0x400) + dy);
        self.world.entity_mut(car).despawn();
        let Some(p) = self.create_actor(-1, -1, x, y, floor, heading, 0) else {
            return;
        };
        let health = self.world.resource::<PlayerSession>().foot_health;
        if let Some(mut o) = self.world.get_mut::<Obj>(p) {
            o.state = State::Player;
            o.player = Some(0);
            o.score = score;
            o.wanted = wanted;
            o.health = health;
        }
        if let Some(mut person) = self.world.get_mut::<crate::sim::people::Person>(p) {
            person.follows_weapon = true;
            person.kind = ars.current.person_model();
        }
        self.world.entity_mut(p).insert((CameraTarget, ars));
    }
}
