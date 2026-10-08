//! `--autoplay`: a debug bot that plays a story level's missions to the
//! end, to check that every level can be completed.
//!
//! Usage: `payback --map <c> --level <l> --autoplay [--fast <n>]
//! [--autoplay-ticks <n>]`. The bot reads the mission state (phones, the
//! live triggers, the current slot) every tick and moves the player to
//! whatever the current objective needs: answer a phone, get into a
//! vehicle, reach a checkpoint or garage, kill a target, collect a crate.
//! It teleports instead of driving, but the triggers still fire through the
//! normal game logic (trigger evaluation, boarding, pickups, deaths). Small
//! per-level hints (`hint`) cover what the trigger list alone can't tell
//! (which "actor gone" trigger is a target and which a failure, when to run
//! from a bomb). Mission events, messages and dialogue are logged; the app
//! exits on level complete, game over or the tick cap.
//!
//! Bot shortcuts (none of them fires a trigger itself): teleporting the
//! player (on foot or with the vehicle), kills by an explosion credited to
//! the player at the target, steering the race car's velocity along the
//! opponents' waypoints, and moving a passenger who can't walk to the
//! player's car (another floor) next to it. Debug traces: `--autoplay-debug`.
//!
//! `--autoplay-all` holds money back between missions until every phone
//! mission has been played (the level otherwise ends at the money goal).
//!
//! `--fast n` runs the game n times faster than real time (more fixed ticks
//! per frame), for any mode.

use crate::camera::CameraTarget;
use crate::level::{AppState, LevelSpec};
use crate::mission::triggers::Trigger;
use crate::mission::{LevelClock, LevelOutcome, MissionState, Script, Triggers, kind};
use crate::objects::ObjectLibrary;
use crate::sim::boarding::{Boarding, door_point, door_y};
use crate::sim::combat::Arsenal;
use crate::sim::people::{PeopleThink, Person, PlayerInput, PlayerInputs};
use crate::sim::{Obj, State, Trig, collide, heading_of};
use crate::vehicles::Vehicle;
use crate::world::{City, z_of_h};
use bevy::prelude::*;
use payback_formats::map::MAP_SIZE;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

/// The bot is on (`--autoplay`).
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::args().any(|a| a == "--autoplay"))
}

/// Log a mission line while the bot plays.
pub fn log(what: std::fmt::Arguments) {
    if on() {
        info!("autoplay: {what}");
    }
}

pub struct AutoplayPlugin;

impl Plugin for AutoplayPlugin {
    fn build(&self, app: &mut App) {
        if let Some(n) = crate::arg("--fast").and_then(|v| v.parse::<f64>().ok()) {
            app.add_systems(Startup, move |mut t: ResMut<Time<Virtual>>| {
                t.set_relative_speed_f64(n.max(0.1));
                t.set_max_delta(std::time::Duration::from_secs_f64((n * 0.25).max(0.25)));
            });
        }
        if !on() {
            return;
        }
        app.init_resource::<Bot>()
            .add_systems(OnEnter(AppState::Playing), |mut bot: ResMut<Bot>| *bot = Bot::default())
            .add_systems(
                FixedUpdate,
                bot_tick
                    .after(crate::replay::ReplayInput)
                    .before(PeopleThink)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, watch_end);
    }
}

/// What the bot is trying to do this tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Goal {
    Wait,
    /// Answer map phone n (on foot).
    Phone(i32),
    /// Get into this vehicle.
    Enter(Entity),
    /// Get into any vehicle of this type (-1: any).
    EnterModel(i32),
    /// Be at (x, y) on this floor; `foot`: Some(true) on foot, Some(false)
    /// in a vehicle, None either.
    Reach {
        x: i32,
        y: i32,
        level: usize,
        foot: Option<bool>,
    },
    /// Get within about `r` of (x, y) on `level`, from the current side.
    Approach { x: i32, y: i32, level: usize, r: i32 },
    /// Drive into a garage (square, floor).
    Garage { sx: i32, sy: i32, level: usize },
    /// Kill this actor.
    Kill(Entity),
    /// Be within `dist` of this actor (on foot if `foot`).
    Near(Entity, i32, Option<bool>),
    /// Trail this actor at a distance (out of its sight).
    Follow(Entity),
    /// Smash and collect the crate / pickup on this cell.
    Collect { sx: i32, sy: i32, level: usize },
    /// Fire weapon slot (HUD numbering).
    Fire(u8),
    /// Kill bystanders for points (kill frenzies).
    Rampage,
    /// Kill police officers.
    KillCops,
    /// Drive this vehicle into the water.
    Dump(Entity),
    /// Drive the current vehicle round this loop of points (a race
    /// course), each leg at most at its speed (units per tick).
    Drive { route: Vec<(IVec2, i32)> },
    /// Shoot `target` with weapon `slot` standing at `from`; `charge` > 0
    /// throws grenades with that power.
    Shoot {
        target: Entity,
        slot: u8,
        from: (i32, i32),
        charge: i32,
    },
    /// Get out of the vehicle.
    Exit,
    /// Leave the player where it is but out of the vehicle, `dist` away
    /// from this point (run from a bomb).
    Flee { x: i32, y: i32 },
}

#[derive(Resource, Default)]
pub struct Bot {
    tick: u32,
    /// Ticks to wait before the next action (after pressing "use").
    cooldown: u32,
    /// Mission signature (slot, triggers, money) and when it last changed.
    sig: u64,
    changed: u32,
    stuck_reported: bool,
    goal: Option<Goal>,
    /// Recent positions of scripted actors.
    trails: HashMap<Entity, VecDeque<(IVec3, usize)>>,
    health: HashMap<Entity, i32>,
    /// Passengers far from the player since this tick.
    far_since: HashMap<Entity, u32>,
    anchors: HashMap<Entity, (IVec3, u32)>,
    /// Tick each scripted actor last moved.
    moved: HashMap<Entity, u32>,
    /// Exit requested (end reached).
    done: bool,
    fire_phase: bool,
    shot_hold: i32,
    /// Next point of a `Drive` route.
    route_idx: usize,
    race_start: Option<u32>,
    player_up: bool,
    /// Where the driven car was and since when (stuck check).
    drive_anchor: Option<(IVec3, u32)>,
    /// Explosions sent at each kill target.
    blasts: HashMap<Entity, u32>,
    /// `--autoplay-all`: money held back so the level doesn't end before
    /// every phone mission has been played.
    bank: i32,
}

/// The player this tick.
#[derive(Clone, Copy)]
struct Me {
    e: Entity,
    pos: IVec3,
    level: usize,
    on_foot: bool,
    boarding: bool,
    vehicle: i32,
}

fn me(world: &mut World) -> Option<Me> {
    let mut q = world.query_filtered::<(
        Entity,
        &Obj,
        Has<Person>,
        Has<Boarding>,
        Option<&Vehicle>,
    ), With<CameraTarget>>();
    let (e, o, on_foot, boarding, veh) = q.iter(world).next()?;
    if o.state != State::Player {
        return None;
    }
    Some(Me {
        e,
        pos: o.pos,
        level: o.level,
        on_foot,
        boarding,
        vehicle: veh.map_or(-1, |v| v.kind as i32),
    })
}

/// End of the run: log the outcome and exit.
fn watch_end(
    mut outcomes: MessageReader<LevelOutcome>,
    clock: Res<LevelClock>,
    spec: Res<LevelSpec>,
    state: Option<Res<bevy::state::state::State<AppState>>>,
    mut bot: ResMut<Bot>,
    players: Query<&Obj, With<CameraTarget>>,
    session: Res<crate::sim::people::PlayerSession>,
    mut exit: MessageWriter<AppExit>,
    mut wait: Local<u32>,
) {
    for o in outcomes.read() {
        // While the player is down, the score stored at death.
        let money = match players.iter().next() {
            Some(p) if session.respawn_timer == 0 => p.score,
            _ => session.saved_score,
        };
        info!(
            "autoplay: RESULT map {} level {}: {:?} after {} ticks (money {money})",
            spec.map, spec.level, o, clock.0
        );
        bot.done = true;
    }
    let cap = crate::arg("--autoplay-ticks").and_then(|v| v.parse().ok()).unwrap_or(200_000u32);
    let playing = state.is_some_and(|s| *s.get() == AppState::Playing);
    if playing && clock.0 >= cap && !bot.done {
        info!(
            "autoplay: RESULT map {} level {}: STUCK at tick cap {cap} (goal {:?})",
            spec.map, spec.level, bot.goal
        );
        bot.done = true;
    }
    if bot.done {
        *wait += 1;
        if *wait > 30 {
            exit.write(AppExit::Success);
        }
    }
}

/// Everything a hint can look at.
pub struct Ctx<'a> {
    pub slot: i32,
    pub triggers: &'a [Trigger],
    pub city: u8,
    pub level: u8,
}

impl Ctx<'_> {
    pub fn has(&self, ev: i32) -> bool {
        self.triggers.iter().any(|t| t.event == ev)
    }
}

fn bot_tick(world: &mut World) {
    let mut bot = world.remove_resource::<Bot>().unwrap_or_default();
    bot.tick += 1;
    world.resource_mut::<PlayerInputs>().0[0] = PlayerInput::default();
    if !bot.done && world.resource::<crate::mission::ActiveLevel>().0.is_some() {
        record_trails(world, &mut bot);
        step(world, &mut bot);
    }
    world.insert_resource(bot);
}

fn record_trails(world: &mut World, bot: &mut Bot) {
    if std::env::args().any(|a| a == "--autoplay-debug") {
        let mut q = world.query::<(Entity, &Obj)>();
        let all: Vec<(Entity, IVec3, i32, i32, i32)> =
            q.iter(world).map(|(e, o)| (e, o.pos, o.health, o.script_id, o.speed)).collect();
        let cops: Vec<Entity> = world
            .query::<(Entity, &Person)>()
            .iter(world)
            .filter(|(_, p)| p.kind == crate::sim::police::COP)
            .map(|(e, _)| e)
            .collect();
        let player: Option<Entity> = world.query_filtered::<Entity, With<CameraTarget>>().iter(world).next();
        for &(e, pos, h, id, _) in &all {
            if id < 0 && !cops.contains(&e) && Some(e) != player {
                continue;
            }
            let prev = bot.health.insert(e, h);
            if prev.is_some_and(|p| p != h) {
                let near: Vec<String> = all
                    .iter()
                    .filter(|o| o.0 != e && (o.1 - pos).truncate().abs().max_element() < 2500)
                    .map(|o| format!("{:?} id {:#x} d {:?} speed {}", o.0, o.3, (o.1 - pos).truncate(), o.4))
                    .collect();
                log(format_args!("health dbg: {e:?} id {id:#x} {} -> {h} at {pos:?}; near [{}]", prev.unwrap(), near.join("; ")));
            }
        }
    }
    let mut q = world.query::<(Entity, &Obj)>();
    let live: Vec<(Entity, IVec3, usize)> = q
        .iter(world)
        .filter(|(_, o)| o.script_id >= 0 && o.state != State::Player)
        .map(|(e, o)| (e, o.pos, o.level))
        .collect();
    bot.trails.retain(|e, _| live.iter().any(|l| l.0 == *e));
    bot.anchors.retain(|e, _| live.iter().any(|l| l.0 == *e));
    for (e, pos, level) in live {
        // "Moved" = got 400 away from where it last stood (jittering
        // against an obstacle doesn't count).
        let anchor = bot.anchors.entry(e).or_insert((pos, bot.tick));
        if (anchor.0 - pos).truncate().abs().max_element() > 400 {
            *anchor = (pos, bot.tick);
        }
        let since = anchor.1;
        bot.moved.insert(e, since);
        let t = bot.trails.entry(e).or_default();
        if t.back().is_none_or(|&(p, _)| (p - pos).abs().max_element() > 8) {
            t.push_back((pos, level));
            if t.len() > 600 {
                t.pop_front();
            }
        }
    }
}

fn signature(world: &mut World) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    world.resource::<MissionState>().cur_slot.hash(&mut h);
    for t in &world.resource::<Triggers>().list {
        (t.event, t.kind).hash(&mut h);
    }
    h.finish()
}

fn step(world: &mut World, bot: &mut Bot) {
    let sig = signature(world);
    if sig != bot.sig {
        bot.sig = sig;
        bot.changed = bot.tick;
        bot.stuck_reported = false;
    } else if bot.tick - bot.changed > 25 * 120 && !bot.stuck_reported {
        bot.stuck_reported = true;
        let st = world.resource::<MissionState>();
        let slot = st.cur_slot;
        let list: Vec<String> = world
            .resource::<Triggers>()
            .list
            .iter()
            .map(|t| format!("{:#x}:{:#x}({:#x},{:#x},{:#x},{:#x})", t.event, t.kind, t.a, t.b, t.c, t.d))
            .collect();
        info!(
            "autoplay: no progress for 120 s: slot {slot}, goal {:?}, triggers [{}]",
            bot.goal,
            list.join(" ")
        );
        let actors: Vec<Entity> = world.resource::<Triggers>().list.iter().filter_map(|t| t.actor).collect();
        for a in actors {
            if let Some(o) = world.get::<Obj>(a) {
                info!("autoplay:   actor {a:?} id {:#x} at {:?} level {} state {:?} speed {}", o.script_id, o.pos, o.level, o.state, o.speed);
            }
        }
        let mut q = world.query_filtered::<&Obj, With<CameraTarget>>();
        if let Some(o) = q.iter(world).next() {
            info!("autoplay:   player at {:?} level {} state {:?}", o.pos, o.level, o.state);
        }
    }
    let Some(me) = me(world) else {
        if std::mem::replace(&mut bot.player_up, false) {
            let mut q = world.query_filtered::<&Obj, With<CameraTarget>>();
            let o = q.iter(world).next().map(|o| (o.pos, o.state, o.health, o.in_water));
            log(format_args!("player down: {o:?}"));
        }
        bot.goal = Some(Goal::Wait);
        return; // dead / respawning
    };
    bot.player_up = true;
    if bot.cooldown > 0 {
        bot.cooldown -= 1;
        return;
    }
    // On foot with the police after us: shoot the cops who come close.
    if me.on_foot && !me.boarding && defend(world, bot, &me) {
        return;
    }
    if me.boarding {
        // Walking to a moving car never arrives: keep at its door.
        let b = world.get::<Boarding>(me.e).copied();
        if let Some(b) = b.filter(|b| !b.entering) {
            stand_at_door(world, &me, b.vehicle);
        }
        return;
    }
    if std::env::args().any(|a| a == "--autoplay-all") {
        hold_money(world, bot, &me);
    }
    let goal = choose(world, bot, &me);
    if bot.goal.as_ref() != Some(&goal) {
        log(format_args!("goal {goal:?}"));
        bot.goal = Some(goal.clone());
    }
    act(world, bot, &me, &goal);
}

/// Between missions, keep the money below the level goal while phones
/// are left; give it all back once every phone has been answered.
fn hold_money(world: &mut World, bot: &mut Bot, me: &Me) {
    let st = world.resource::<MissionState>();
    if st.cur_slot != -1 || st.goal <= 0 {
        return;
    }
    let goal = st.goal;
    let phones_left = st.phones.iter().any(|p| p.event != -1);
    let Some(mut o) = world.get_mut::<Obj>(me.e) else {
        return;
    };
    // A mission pays at most 300,000 x 4.
    if phones_left && o.score > goal - 1_300_000 {
        let take = o.score - 100_000;
        o.score -= take;
        bot.bank += take;
        log(format_args!("holding back {take} (banked {})", bot.bank));
    } else if !phones_left && bot.bank > 0 {
        o.score += bot.bank;
        log(format_args!("all phones done; returning {} (money {})", bot.bank, o.score));
        bot.bank = 0;
    }
}

/// Pick the objective: the level hint first, then the live triggers by
/// kind.
fn choose(world: &mut World, bot: &mut Bot, me: &Me) -> Goal {
    let st = world.resource::<MissionState>();
    let (slot, city, level) = (st.cur_slot, st.city, st.level);
    let triggers = world.resource::<Triggers>().list.clone();
    let ctx = Ctx {
        slot,
        triggers: &triggers,
        city,
        level,
    };
    if let Some(g) = hint(world, bot, me, &ctx) {
        return g;
    }
    let tracked = world.resource::<MissionState>().tracked.clone();
    let mut best: Option<(u32, Goal)> = None;
    for t in &triggers {
        // Untracked rectangles are level ambience (roof cut-aways, sound);
        // untracked frenzies and quiet "get in" triggers are optional
        // bonuses (level 11's RC truck, level 01's booby-trapped car).
        if matches!(t.kind, kind::IN_RECT | kind::MONEY_GAIN | kind::ENTER_ACTOR_QUIET)
            && !tracked.contains(&t.event)
        {
            continue;
        }
        if let Some((p, g)) = generic(world, t) {
            if best.as_ref().is_none_or(|b| p < b.0) {
                best = Some((p, g));
            }
        }
    }
    let goal = best.map(|b| b.1).unwrap_or(Goal::Wait);
    // Approaching a point where someone must not see us: come up behind
    // them instead.
    if let Goal::Reach { x, y, .. } = goal {
        for t in &triggers {
            if !matches!(t.kind, kind::ACTOR_SEES_PLAYER | kind::ACTOR_SEES_PLAYER_QUIET) {
                continue;
            }
            let near = t
                .actor
                .and_then(|e| world.get::<Obj>(e))
                .is_some_and(|o| (o.pos.truncate() - IVec2::new(x, y)).abs().max_element() < 4000);
            if near {
                return Goal::Follow(t.actor.unwrap());
            }
        }
    }
    goal
}

/// The generic objective for one trigger, with a priority (lower first).
fn generic(world: &mut World, t: &Trigger) -> Option<(u32, Goal)> {
    use kind::*;
    let reach = |x, y, level: i32, foot| Goal::Reach {
        x,
        y,
        level: level.clamp(0, 2) as usize,
        foot,
    };
    Some(match t.kind {
        ENTER_ACTOR | ENTER_ACTOR_QUIET => (1, Goal::Enter(t.actor?)),
        ACTOR_KILLED | ACTOR_ATTACKED => (2, Goal::Kill(t.actor?)),
        GARAGE => (
            3,
            Goal::Garage {
                sx: t.a,
                sy: t.b,
                level: (t.c - 1).clamp(0, 2) as usize,
            },
        ),
        AT_POINT | STOPPED_AT_POINT | AT_POINT_400 => (3, reach(t.a, t.b, t.c, None)),
        // "Within ±r": stop well short, on the side we come from.
        NEAR_POINT_12000 | NEAR_POINT_10000 | NEAR_POINT_6000 => {
            let r = match t.kind {
                NEAR_POINT_12000 => 12000,
                NEAR_POINT_10000 => 10000,
                _ => 6000,
            };
            (3, Goal::Approach { x: t.a, y: t.b, level: t.c.clamp(0, 2) as usize, r: r * 6 / 10 })
        }
        ON_FOOT_AT_POINT | ON_FOOT_AT_POINT_QUIET | ON_FOOT_NEAR_POINT => {
            (3, reach(t.a, t.b, t.c, Some(true)))
        }
        IN_RECT => (3, reach((t.a + t.c) / 2, (t.b + t.d) / 2, t.e, None)),
        NEAR_ACTOR => (4, Goal::Near(t.actor?, 3000, None)),
        NEAR_ACTOR_1000 => (4, Goal::Near(t.actor?, 500, None)),
        AT_ACTOR_ON_FOOT => (4, Goal::Near(t.actor?, 400, Some(true))),
        ACTOR_NEAR_PLAYER => (4, Goal::Near(t.actor?, 3000, None)),
        IN_VEHICLE_MODEL => (5, Goal::EnterModel(t.a)),
        IN_ANY_VEHICLE => (5, Goal::EnterModel(-1)),
        SCENERY_GONE => (6, Goal::Collect {
            sx: t.a,
            sy: t.b,
            level: t.c.clamp(0, 2) as usize,
        }),
        MONEY_GAIN => (8, Goal::Rampage),
        COP_KILLED => (8, Goal::KillCops),
        ACTOR_IN_WATER => (3, Goal::Dump(t.actor?)),
        // Passengers walking to the player's car: hold still.
        ACTOR_BOARDS_PLAYER => (6, Goal::Wait),
        // A random phone (-1, Rampage's Random missions): its square.
        PHONE if t.a == -1 => {
            let pos = Script::new(world).phone_square_pos(t.b as usize, t.c as usize, t.d as usize);
            (7, reach(pos.x, pos.y, t.d, Some(true)))
        }
        PHONE => (7, Goal::Phone(t.a)),
        SECRET_PHONE => (8, Goal::Phone(t.a)),
        ACTOR_LOST | ACTOR_FLEES | ACTOR_SEES_PLAYER | ACTOR_SEES_PLAYER_QUIET => {
            (9, Goal::Follow(t.actor?))
        }
        SCENERY_GONE_QUIET => {
            // Info pickups are only hints.
            let info = world.get_resource::<City>().is_some_and(|c| {
                (0..MAP_SIZE as i32).contains(&t.a)
                    && (0..MAP_SIZE as i32).contains(&t.b)
                    && c.map.square(t.a as usize, t.b as usize).cubes[t.c.clamp(0, 2) as usize].scenery
                        == 24
            });
            if info {
                return None;
            }
            (10, Goal::Collect {
                sx: t.a,
                sy: t.b,
                level: t.c.clamp(0, 2) as usize,
            })
        }
        _ => return None,
    })
}

// ---- Actions ------------------------------------------------------------

fn input(world: &mut World) -> Mut<'_, PlayerInput> {
    world.resource_mut::<PlayerInputs>().map_unchanged(|i| &mut i.0[0])
}

fn press_use(world: &mut World, bot: &mut Bot) {
    input(world).use_pressed = true;
    bot.cooldown = 3;
}

/// Floor z of (x, y) on cube level `level`, as the movement code finds it.
fn floor_at(world: &World, x: i32, y: i32, level: usize) -> Option<(i32, usize, bool)> {
    let city = world.get_resource::<City>()?;
    if !(0..(MAP_SIZE as i32) << 11).contains(&x) || !(0..(MAP_SIZE as i32) << 11).contains(&y) {
        return None;
    }
    let (sx, sy) = ((x >> 11) as usize, (y >> 11) as usize);
    let fx = (x & 0x7ff) as f32 / 2048.0;
    let fy = (y & 0x7ff) as f32 / 2048.0;
    let top = city.square_cubes(sx, sy)[level.min(2)].top_at(fx, fy);
    let z = z_of_h(top) as i32;
    collide::floor(city, x, y, z)
}

/// Move the player (or any object) to (x, y) on `level`, at rest.
pub fn teleport(world: &mut World, e: Entity, x: i32, y: i32, level: usize) {
    let found = floor_at(world, x, y, level);
    let Some(mut o) = world.get_mut::<Obj>(e) else {
        return;
    };
    let (z, lvl, water) = found.unwrap_or((o.pos.z, level, false));
    o.pos = IVec3::new(x, y, z);
    o.prev_pos = o.pos;
    o.floor_z = z;
    o.level = lvl;
    o.in_water = water;
    o.vel = IVec3::ZERO;
    o.speed = 0;
    o.on_ground = true;
}

/// A free spot about `d` from `pos` on the same floor (no wall between).
fn spot_near(world: &World, pos: IVec3, level: usize, d: i32) -> (i32, i32) {
    // Pavement (pedestrian flag 0x20) first: standing in the road gets
    // the bot run over.
    let pavement = |x: i32, y: i32| {
        world.get_resource::<City>().is_some_and(|c| {
            (0..(MAP_SIZE as i32) << 11).contains(&x)
                && (0..(MAP_SIZE as i32) << 11).contains(&y)
                && c.map.square((x >> 11) as usize, (y >> 11) as usize).cubes[level.min(2)].flags & 0x20 != 0
        })
    };
    for need_pavement in [true, false] {
        for dist in [d, d / 2, d / 4] {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let x = pos.x + (a.cos() * dist as f32) as i32;
                let y = pos.y + (a.sin() * dist as f32) as i32;
                if (!need_pavement || pavement(x, y)) && free(world, pos, level, x, y) {
                    return (x, y);
                }
            }
        }
    }
    (pos.x, pos.y)
}

fn act(world: &mut World, bot: &mut Bot, me: &Me, goal: &Goal) {
    match goal.clone() {
        Goal::Wait => {
            if bot.tick % 25 == 0 && std::env::args().any(|a| a == "--autoplay-debug") {
                let actors: Vec<Entity> = world.resource::<Triggers>().list.iter().filter_map(|t| t.actor).collect();
                for a in actors {
                    if let Some(o) = world.get::<Obj>(a) {
                        log(format_args!("wait dbg: {a:?} id {:#x} pos {:?} lvl {} h {:#x} speed {} vel {:?} state {:?}", o.script_id, o.pos, o.level, o.heading, o.speed, o.vel, o.state));
                    }
                }
            }
            if !defend(world, bot, me) && !bring_passengers(world, bot, me) {
                unblock(world, bot, me)
            }
            // Waiting on foot: off the road.
            if me.on_foot && bot.tick % 25 == 0 {
                let on_road = world.get_resource::<City>().is_some_and(|c| {
                    let (sx, sy) = ((me.pos.x >> 11) as usize, (me.pos.y >> 11) as usize);
                    sx < MAP_SIZE && sy < MAP_SIZE && c.map.square(sx, sy).cubes[me.level.min(2)].flags & 0x20 == 0
                });
                if on_road {
                    let (x, y) = spot_near(world, me.pos, me.level, 2000);
                    teleport(world, me.e, x, y, me.level);
                }
            }
        }
        Goal::Exit => {
            // A helicopter takes three presses (engine on, engine off and
            // armed, out; `boarding.rs`).
            if !me.on_foot {
                press_use(world, bot);
            }
        }
        Goal::Phone(n) => {
            if !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            let Some((pos, lvl)) = Script::new(world).phone_coords(n) else {
                return;
            };
            if me.pos.truncate() != pos.truncate() {
                teleport(world, me.e, pos.x, pos.y, lvl);
            }
        }
        Goal::Enter(v) => enter(world, bot, me, v),
        Goal::EnterModel(m) => {
            if me.vehicle >= 0 && (m < 0 || me.vehicle == m) {
                return;
            }
            let mut q = world.query::<(Entity, &Obj, &Vehicle)>();
            let best = q
                .iter(world)
                .filter(|(_, o, v)| {
                    (m < 0 || v.kind as i32 == m)
                        && !matches!(o.state, State::Dead | State::Removed | State::Player)
                        && o.burn == 0
                        && !o.in_water
                        && o.health > 30
                        && o.script_id < 0
                })
                .min_by_key(|(_, o, _)| {
                    let d = (o.pos - me.pos).truncate().as_i64vec2();
                    // Ties: object-table order, never the ECS order.
                    (d.x * d.x + d.y * d.y, o.slot)
                })
                .map(|(e, ..)| e);
            if bot.tick % 250 == 0 {
                let n = world.query::<&Vehicle>().iter(world).count();
                log(format_args!("EnterModel({m}): {n} vehicles, picked {best:?} at {:?}, me at {:?}", best.and_then(|v| world.get::<Obj>(v).map(|o| (o.pos, o.state))), me.pos));
            }
            if let Some(v) = best {
                enter(world, bot, me, v);
            }
        }
        Goal::Reach { x, y, level, foot } => {
            if foot == Some(true) && !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            if foot == Some(false) && me.on_foot {
                return act(world, bot, me, &Goal::EnterModel(-1));
            }
            let d = (me.pos.truncate() - IVec2::new(x, y)).abs();
            if d.x > 64 || d.y > 64 || me.level != level {
                teleport(world, me.e, x, y, level);
            } else if let Some(mut o) = world.get_mut::<Obj>(me.e) {
                o.vel = IVec3::ZERO;
                o.speed = 0;
            }
        }
        Goal::Garage { sx, sy, level } => {
            if me.on_foot {
                return act(world, bot, me, &Goal::EnterModel(-1));
            }
            teleport(world, me.e, sx << 11 | 0x400, sy << 11 | 0x400, level);
        }
        Goal::Kill(e) => kill(world, bot, me, e),
        Goal::Approach { x, y, level, r } => {
            let d = (me.pos.truncate() - IVec2::new(x, y)).abs().max_element();
            if d <= r + 1000 && me.level == level {
                return;
            }
            let dir = (me.pos.truncate() - IVec2::new(x, y)).as_vec2().normalize_or_zero();
            let p = IVec3::new(x, y, me.pos.z);
            let mut spot = None;
            for f in [1.0, 0.7, 0.4, 0.0] {
                let c = IVec2::new(x, y) + (dir * r as f32 * f).as_ivec2();
                if f == 0.0 || floor_at(world, c.x, c.y, level).is_some_and(|(_, l, w)| l == level && !w) {
                    spot = Some(c);
                    break;
                }
            }
            let c = spot.unwrap_or(p.truncate());
            teleport(world, me.e, c.x, c.y, level);
        }
        Goal::KillCops => {
            let mut q = world.query::<(Entity, &Obj, Has<Person>)>();
            let library = world.resource::<ObjectLibrary>();
            let cops: Vec<(Entity, IVec3, u32)> = q
                .iter(world)
                .filter(|(_, o, person)| {
                    *person
                        && o.health > 0
                        && !matches!(o.state, State::Dead | State::Removed)
                        && library.model(o.model).info.police
                })
                .map(|(e, o, _)| (e, o.pos, o.slot))
                .collect();
            let target = cops.into_iter().min_by_key(|(_, p, slot)| {
                let d = (*p - me.pos).truncate().as_i64vec2();
                (d.x * d.x + d.y * d.y, *slot)
            });
            if bot.tick % 250 == 0 {
                let mut q = world.query::<(&Obj, Option<&Vehicle>)>();
                let library = world.resource::<ObjectLibrary>();
                let police: Vec<String> = q
                    .iter(world)
                    .filter(|(o, _)| library.model(o.model).info.police)
                    .map(|(o, v)| format!("{}{:?}/{:?}", if v.is_some() { "car " } else { "" }, o.pos.truncate(), o.state))
                    .collect();
                let mut q2 = world.query::<(&Obj, &Person)>();
                let cops: Vec<String> = q2.iter(world).filter(|(_, p)| p.kind == crate::sim::police::COP).map(|(o, _)| format!("{:?}/{:?}/h{}", o.pos.truncate(), o.state, o.health)).collect();
                log(format_args!("KillCops: cop people [{}]", cops.join(", ")));
                let wanted = world.get::<Obj>(me.e).map_or(0, |o| o.wanted);
                log(format_args!("KillCops: wanted {wanted}, police [{}], me {:?}", police.join(", "), me.pos));
            }
            if let Some((t, ..)) = target {
                kill(world, bot, me, t);
            }
        }
        Goal::Drive { route } => {
            if me.on_foot || route.is_empty() {
                return;
            }
            let (target, leg_speed) = route[bot.route_idx % route.len()];
            let speed = leg_speed.min(crate::vehicles::TYPES[me.vehicle.max(0) as usize].max_speed);
            let d = target - me.pos.truncate();
            let dist = d.as_vec2().length();
            if dist < 900.0 {
                bot.route_idx += 1;
                return;
            }
            // Steer: point the car at the next waypoint at the leg's speed
            // and let the driving physics move it (walls, jumps, other
            // cars all apply). A car close ahead is passed on the side
            // with more room.
            let fwd = d.as_vec2() / dist;
            let left = Vec2::new(-fwd.y, fwd.x);
            let mut aim = d.as_vec2();
            let mut q = world.query::<(Entity, &Obj, &Vehicle)>();
            let ahead: Vec<Vec2> = q
                .iter(world)
                .filter(|(e, o, _)| *e != me.e && o.state != State::Removed)
                .map(|(_, o, _)| (o.pos - me.pos).truncate().as_vec2())
                .filter(|r| {
                    let along = r.dot(fwd);
                    (0.0..3000.0).contains(&along) && r.dot(left).abs() < 1500.0
                })
                .collect();
            if let Some(r) = ahead.iter().min_by(|a, b| a.length().total_cmp(&b.length())) {
                let side = if r.dot(left) > 0.0 { -left } else { left };
                aim = fwd * 2500.0 + side * 1800.0;
            }
            let h = heading_of(aim.x as i32, aim.y as i32);
            let (sn, cs) = {
                let trig = world.resource::<Trig>();
                (trig.sin(h), trig.cos(h))
            };
            if std::env::args().any(|a| a == "--autoplay-debug") && bot.tick % 10 == 0 {
                let hp = world.get::<Obj>(me.e).map_or(0, |o| o.health);
                log(format_args!("drive dbg: {:?} sq {:?} -> wp {} health {hp}", me.pos, me.pos.truncate() / 2048, bot.route_idx));
            }
            if let Some(mut o) = world.get_mut::<Obj>(me.e) {
                o.heading = h;
                o.turn_rate = 0;
                o.speed = speed;
                o.vel.x = sn * speed >> 8;
                o.vel.y = cs * speed >> 8;
            }
            // Wedged (against a wall or a car) for 2 s: hop ahead.
            let (anchor, since) = bot.drive_anchor.unwrap_or((me.pos, bot.tick));
            if (anchor - me.pos).truncate().abs().max_element() > 600 {
                bot.drive_anchor = Some((me.pos, bot.tick));
            } else if bot.tick - since > 50 {
                let step = (d.as_vec2() * 1500f32.min(dist) / dist).as_ivec2();
                let p = me.pos.truncate() + step;
                log(format_args!("drive: stuck at {:?}, hopping ahead", me.pos.truncate() / 2048));
                teleport(world, me.e, p.x, p.y, me.level);
                bot.drive_anchor = None;
            } else {
                bot.drive_anchor = Some((anchor, since));
            }
        }
        Goal::Dump(v) => {
            if me.e != v {
                return enter(world, bot, me, v);
            }
            if std::env::args().any(|a| a == "--autoplay-debug") {
                if let Some(o) = world.get::<Obj>(v) {
                    log(format_args!("dump dbg: pos {:?} floor {} lvl {} water {} ground {}", o.pos, o.floor_z, o.level, o.in_water, o.on_ground));
                }
            }
            if world.get::<Obj>(v).is_some_and(|o| o.in_water) {
                return;
            }
            if let Some((x, y, level)) = nearest_water(world, me.pos) {
                // Already there, dropping in: let it fall.
                if (me.pos.truncate() - IVec2::new(x, y)).abs().max_element() < 600 {
                    return;
                }
                log(format_args!("driving into the water at ({x},{y})"));
                teleport(world, me.e, x, y, level);
                // Arrive dry so the physics sees the splash.
                if let Some(mut o) = world.get_mut::<Obj>(me.e) {
                    o.in_water = false;
                    o.pos.z -= 0x400;
                    o.prev_pos = o.pos;
                }
            }
        }
        Goal::Rampage => {
            // The nearest living bystander (blowing up cars sets off
            // chains of explosions the bot doesn't survive).
            let mut q = world.query::<(Entity, &Obj, Has<Person>)>();
            let target = q
                .iter(world)
                .filter(|(t, o, person)| {
                    *t != me.e
                        && *person
                        && o.script_id < 0
                        && o.health > 0
                        && !matches!(o.state, State::Dead | State::Removed | State::Player)
                })
                .min_by_key(|(_, o, _)| {
                    let d = (o.pos - me.pos).truncate().as_i64vec2();
                    // Ties: object-table order, never the ECS order.
                    (d.x * d.x + d.y * d.y, o.slot)
                })
                .map(|(t, ..)| t);
            if let Some(t) = target {
                kill(world, bot, me, t);
            }
        }
        Goal::Shoot { target, slot, from, charge } => {
            if !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            let Some(tp) = world.get::<Obj>(target).map(|o| o.pos) else {
                return;
            };
            let d = (me.pos.truncate() - IVec2::new(from.0, from.1)).abs();
            if d.max_element() > 64 {
                teleport(world, me.e, from.0, from.1, me.level);
                bot.shot_hold = 0;
                return;
            }
            if let Some(mut o) = world.get_mut::<Obj>(me.e) {
                o.heading = heading_of(tp.x - o.pos.x, tp.y - o.pos.y);
                o.turn_rate = 0;
            }
            let cur = world.get::<Arsenal>(me.e).map(|a| a.current as u8);
            if cur != Some(slot) {
                input(world).weapon_pressed = Some(slot);
                return;
            }
            if std::env::args().any(|a| a == "--autoplay-debug") {
                let (h, ft) = (world.get::<Obj>(me.e).map_or(0, |o| o.health), world.get::<Arsenal>(me.e).map_or(0, |a| a.fire_timer));
                let mut q = world.query::<(&Obj, &crate::sim::combat::Projectile)>();
                let pr: Vec<IVec3> = q.iter(world).map(|(o, _)| o.pos).collect();
                log(format_args!("shoot dbg: me {:?} h {h} ft {ft} hold {} target {tp:?} proj {pr:?}", me.pos, bot.shot_hold));
            }
            if charge > 0 {
                // Grenade: hold for the throw power, release, wait.
                let timer = world.get::<Arsenal>(me.e).map_or(0, |a| a.fire_timer);
                if bot.shot_hold == 0 {
                    // Charging: the throw power is the fire timer.
                    if timer < charge {
                        input(world).fire = true;
                    } else {
                        bot.shot_hold = 1;
                    }
                } else {
                    bot.shot_hold += 1;
                    if bot.shot_hold > 80 {
                        bot.shot_hold = 0;
                    }
                }
            } else {
                input(world).fire = true;
            }
        }
        Goal::Near(e, dist, foot) => {
            if foot == Some(true) && !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            let Some((pos, level)) = world.get::<Obj>(e).map(|o| (o.pos, o.level)) else {
                return;
            };
            let d = (pos - me.pos).truncate().abs();
            if d.x > dist || d.y > dist || level != me.level {
                let (x, y) = spot_near(world, pos, level, dist.min(600).max(300));
                teleport(world, me.e, x, y, level);
            }
        }
        Goal::Follow(e) => follow(world, bot, me, e),
        Goal::Collect { sx, sy, level } => {
            if !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            let (cx, cy) = (sx << 11 | 0x400, sy << 11 | 0x400);
            let scenery = world.get_resource::<City>().map_or(0, |c| {
                c.map.square(sx.clamp(0, MAP_SIZE as i32 - 1) as usize, sy.clamp(0, MAP_SIZE as i32 - 1) as usize).cubes[level].scenery
            });
            let at = IVec2::new(cx, cy - 300);
            if me.pos.truncate() != at {
                teleport(world, me.e, at.x, at.y, level);
                if let Some(mut o) = world.get_mut::<Obj>(me.e) {
                    o.heading = 0;
                }
            }
            if scenery == 6 {
                // Punch the crate open.
                let fist = world.get::<Arsenal>(me.e).is_some_and(|a| a.current as u8 == 0);
                if !fist {
                    input(world).weapon_pressed = Some(0);
                } else {
                    bot.fire_phase = !bot.fire_phase;
                    input(world).fire = bot.fire_phase;
                }
            }
        }
        Goal::Fire(slot) => {
            let cur = world.get::<Arsenal>(me.e).map(|a| a.current as u8);
            if cur != Some(slot) {
                input(world).weapon_pressed = Some(slot);
            } else {
                bot.fire_phase = !bot.fire_phase;
                input(world).fire = bot.fire_phase;
            }
        }
        Goal::Flee { x, y } => {
            if !me.on_foot {
                return act(world, bot, me, &Goal::Exit);
            }
            let d = (me.pos.truncate() - IVec2::new(x, y)).abs();
            if d.max_element() < 14000 {
                let (fx, fy) = spot_near(world, IVec3::new(x, y, me.pos.z), me.level, 16000);
                teleport(world, me.e, fx, fy, me.level);
            }
        }
    }
}

/// Centre of the nearest open water (a 3x3 block of squares whose floor
/// the movement code reads as water) and its floor level.
fn nearest_water(world: &World, from: IVec3) -> Option<(i32, i32, usize)> {
    let city = world.get_resource::<City>()?;
    let water_at = |x: i32, y: i32| -> Option<usize> {
        if !(0..MAP_SIZE as i32).contains(&x) || !(0..MAP_SIZE as i32).contains(&y) {
            return None;
        }
        let (cx, cy) = (x << 11 | 0x400, y << 11 | 0x400);
        // Probe from high up: the topmost surface there.
        let (_, lvl, water) = collide::floor(city, cx, cy, 0)?;
        water.then_some(lvl)
    };
    let mut best: Option<(i64, (i32, i32, usize))> = None;
    for x in 1..MAP_SIZE as i32 - 1 {
        for y in 1..MAP_SIZE as i32 - 1 {
            let Some(lvl) = water_at(x, y) else { continue };
            let open = (-1..=1).all(|dx| (-1..=1).all(|dy| water_at(x + dx, y + dy).is_some()));
            if !open {
                continue;
            }
            let (cx, cy) = (x << 11 | 0x400, y << 11 | 0x400);
            let d = IVec2::new(cx - from.x, cy - from.y).as_i64vec2();
            let d = d.x * d.x + d.y * d.y;
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, (cx, cy, lvl)));
            }
        }
    }
    best.map(|b| b.1)
}

/// Kill an actor with a blast credited to the player (as a grenade or
/// rocket would), from a safe distance; scripted damage filters that
/// ignore blasts get the health set to zero after a few tries.
fn kill(world: &mut World, bot: &mut Bot, me: &Me, e: Entity) {
    let Some((pos, level, alive, person)) = world
        .get::<Obj>(e)
        .map(|o| (o.pos, o.level, o.health > 0, world.get::<Person>(e).is_some()))
    else {
        return;
    };
    if !alive {
        return;
    }
    let (range, power) = if person { (2500, 400_000) } else { (6000, 60_000_000) };
    let d = (pos - me.pos).truncate().abs().max_element();
    if d > range + 2500 || d < range / 2 {
        let (x, y) = spot_near(world, pos, level, range);
        teleport(world, me.e, x, y, level);
        return;
    }
    let n = bot.blasts.entry(e).or_default();
    *n += 1;
    if *n > 6 {
        if let Some(mut o) = world.get_mut::<Obj>(e) {
            log(format_args!("blasts do not hurt {e:?} (id {:#x}); setting its health to 0", o.script_id));
            o.health = 0;
        }
        return;
    }
    log(format_args!("blasting {e:?} (id {:#x})", world.get::<Obj>(e).map_or(-1, |o| o.script_id)));
    world.write_message(crate::sim::combat::Explode {
        pos,
        power,
        layer: level,
        source: None,
        by_player: true,
        player: Some(0),
    });
    bot.cooldown = 2;
}

/// While waiting with the police after us, shoot cops who come close
/// (they would drag the player out and arrest him).
fn defend(world: &mut World, bot: &mut Bot, me: &Me) -> bool {
    if world.get::<Obj>(me.e).is_none_or(|o| o.wanted < 3) {
        return false;
    }
    let mut q = world.query::<(Entity, &Obj, Has<Person>)>();
    let library = world.resource::<ObjectLibrary>();
    let cop = q
        .iter(world)
        .filter(|(_, o, person)| {
            *person
                && o.health > 0
                && !matches!(o.state, State::Dead | State::Removed)
                && o.script_id < 0
                && library.model(o.model).info.police
                && (o.pos - me.pos).truncate().abs().max_element() < 5000
        })
        .min_by_key(|(_, o, _)| o.slot)
        .map(|(e, ..)| e);
    let Some(cop) = cop else { return false };
    // Shoot from where we stand.
    let pos = world.get::<Obj>(cop).map(|o| (o.pos, o.level));
    if let Some((pos, level)) = pos {
        log(format_args!("defending against cop {cop:?}"));
        world.write_message(crate::sim::combat::Explode {
            pos,
            power: 400_000,
            layer: level,
            source: None,
            by_player: true,
            player: Some(0),
        });
        bot.cooldown = 2;
    }
    true
}

/// Bot shortcut: a passenger who should board the player's vehicle but is
/// stuck somewhere it cannot walk from in a straight line (another floor,
/// round a building) is moved next to the vehicle; the boarding itself is
/// the normal AI.
fn bring_passengers(world: &mut World, bot: &mut Bot, me: &Me) -> bool {
    if me.on_foot {
        return false;
    }
    let riders: Vec<Entity> = world
        .resource::<Triggers>()
        .list
        .iter()
        .filter(|t| t.kind == kind::ACTOR_BOARDS_PLAYER)
        .filter_map(|t| t.actor)
        .collect();
    for r in riders {
        let Some(o) = world.get::<Obj>(r) else { continue };
        let far = (o.pos - me.pos).truncate().abs().max_element() > 1500 || o.level != me.level;
        if !far {
            bot.far_since.remove(&r);
            // Close but walking into the car: move the car a little.
            let stuck = bot.moved.get(&r).is_some_and(|&t| bot.tick - t > 150);
            if stuck {
                let (x, y) = spot_near(world, me.pos, me.level, 2000);
                log(format_args!("moving the car: passenger {r:?} stuck next to it"));
                teleport(world, me.e, x, y, me.level);
                bot.anchors.remove(&r);
                return true;
            }
            continue;
        }
        let since = *bot.far_since.entry(r).or_insert(bot.tick);
        if bot.tick - since > 150 {
            bot.far_since.remove(&r);
            let (x, y) = spot_near(world, me.pos, me.level, 1200);
            log(format_args!("shortcut: moving passenger {r:?} (id {:#x}) next to the player", o.script_id));
            teleport(world, r, x, y, me.level);
            bot.anchors.remove(&r);
            return true;
        }
    }
    false
}

/// Waiting for a scripted walker that is stuck against the player's car:
/// move the car out of its way (a little ahead).
fn unblock(world: &mut World, bot: &mut Bot, me: &Me) {
    if me.on_foot {
        return;
    }
    let walkers: Vec<Entity> = world
        .resource::<Triggers>()
        .list
        .iter()
        .filter(|t| matches!(t.kind, kind::ACTOR_GOTO | kind::ACTOR_GOTO_VANISH | kind::ACTOR_GOTO_15))
        .filter_map(|t| t.actor)
        .collect();
    for w in walkers {
        let Some(o) = world.get::<Obj>(w) else { continue };
        if world.get::<Person>(w).is_none() || (o.pos - me.pos).truncate().abs().max_element() > 1500 {
            continue;
        }
        let stuck = bot.moved.get(&w).is_some_and(|&t| bot.tick - t > 40);
        if !stuck {
            continue;
        }
        let (x, y) = spot_near(world, me.pos, me.level, 2500);
        log(format_args!("moving the car out of the way of {w:?}"));
        teleport(world, me.e, x, y, me.level);
        bot.anchors.remove(&w);
        return;
    }
}

/// Get into vehicle `v`: out of the current one first, then stand at its
/// door and press "use".
fn enter(world: &mut World, bot: &mut Bot, me: &Me, v: Entity) {
    if me.e == v {
        return;
    }
    if !me.on_foot {
        return act(world, bot, me, &Goal::Exit);
    }
    if stand_at_door(world, me, v) {
        press_use(world, bot);
    }
}

/// Put the player (on foot) at vehicle `v`'s driver door.
fn stand_at_door(world: &mut World, me: &Me, v: Entity) -> bool {
    let Some((vo, kind)) = world.get::<Obj>(v).cloned().zip(world.get::<Vehicle>(v).map(|x| x.kind)) else {
        return false;
    };
    let info = world.resource::<ObjectLibrary>().model(vo.model).info.clone();
    let door = door_point(world.resource::<Trig>(), &vo, info.radius, door_y(kind, info.max_y), 192);
    teleport(world, me.e, door.x, door.y, vo.level);
    if let Some(mut o) = world.get_mut::<Obj>(me.e) {
        o.pos.z = vo.pos.z;
        o.prev_pos = o.pos;
        o.heading = heading_of(vo.pos.x - door.x, vo.pos.y - door.y);
    }
    true
}

/// Trail an actor: stay within reach (the "lost" range is 33000) but out
/// of its sight (±90° of its heading, about 20000 on foot), moving to
/// where it was a little while ago when needed.
fn follow(world: &mut World, bot: &mut Bot, me: &Me, e: Entity) {
    let Some((pos, level, heading, vehicle)) = world
        .get::<Obj>(e)
        .map(|o| (o.pos, o.level, o.heading, world.get::<Vehicle>(e).is_some()))
    else {
        return;
    };
    let box_d = |p: IVec3| (p - pos).truncate().abs().max_element();
    let dist = |p: IVec3| (p - pos).truncate().as_vec2().length() as i32;
    // The "sees the player" test (type 0x1b) with some margin, ignoring
    // walls (the line of sight opens up as the actor walks on).
    let range = if vehicle { 33500 } else { 21000 };
    let seen = |_: &World, p: IVec3| {
        dist(p) < range
            && crate::sim::angle_diff(heading, heading_of(p.x - pos.x, p.y - pos.y)).abs() <= 0x5a0 + 0x300
    };
    let good = |w: &World, p: IVec3| !seen(w, p) && (1500..28000).contains(&box_d(p));
    if std::env::args().any(|a| a == "--autoplay-debug") {
        let p = me.pos;
        log(format_args!(
            "follow dbg: actor {:?} h {heading:#x} me {:?} d {} ang {} los {} seen {}",
            pos, p, dist(p),
            crate::sim::angle_diff(heading, heading_of(p.x - pos.x, p.y - pos.y)),
            world.get_resource::<City>().is_none_or(|c| collide::line_of_sight(c, pos, p)),
            seen(world, p)
        ));
    }
    if good(world, me.pos) {
        return;
    }
    let trail = bot.trails.get(&e).cloned().unwrap_or_default();
    let mut spot = trail
        .iter()
        .rev()
        .find(|(p, _)| box_d(*p) >= 3000 && good(world, *p))
        .map(|&(p, l)| (p.x, p.y, l));
    if spot.is_none() {
        let trig = world.resource::<Trig>();
        let mut cands = Vec::new();
        for dist in [5000, 3000, 8000, 12000] {
            for turn in [0, 0x200, -0x200, 0x400, -0x400, 0x5a0, -0x5a0] {
                let h = heading + 0xb40 + turn;
                cands.push((pos.x + trig.sin(h) * dist / 256, pos.y + trig.cos(h) * dist / 256));
            }
        }
        spot = cands
            .into_iter()
            .find(|&(x, y)| free(world, pos, level, x, y) && good(world, IVec3::new(x, y, pos.z)))
            .map(|(x, y)| (x, y, level));
    }
    let Some((x, y, l)) = spot else {
        return;
    };
    log(format_args!(
        "follow: actor at {:?} heading {heading:#x}; me {:?} -> ({x},{y}) [d {} box {}]",
        pos.truncate(),
        me.pos.truncate(),
        dist(IVec3::new(x, y, pos.z)),
        box_d(IVec3::new(x, y, pos.z))
    ));
    teleport(world, me.e, x, y, l);
}

/// (x, y) is open floor on `level`, reachable in a straight line from pos.
fn free(world: &World, pos: IVec3, level: usize, x: i32, y: i32) -> bool {
    let Some(city) = world.get_resource::<City>() else {
        return false;
    };
    let Some((fz, lvl, water)) = floor_at(world, x, y, level) else {
        return false;
    };
    if lvl != level || water || (fz - pos.z).abs() > 0x400 {
        return false;
    }
    let (r, hit) = collide::resolve_walls(city, IVec3::new(x, y, fz), pos, 200);
    !hit.x && !hit.y && r.x == x && r.y == y
}

// ---- Per-level hints ------------------------------------------------------

/// Objectives the trigger list alone doesn't give (docs/re/missions_*.md).
fn hint(world: &mut World, bot: &mut Bot, me: &Me, c: &Ctx) -> Option<Goal> {
    let _ = (&bot, me);
    match (c.city, c.level) {
        (0, 0) => hint_00(world, me, c),
        (0, 1) => hint_01(world, me, c),
        (1, 0) => hint_10(world, bot, me, c),
        (1, 1) => hint_11(world, me, c),
        (2, 0) => hint_20(world, me, c),
        (2, 1) => hint_21(world, bot, me, c),
        _ => None,
    }
}

/// The first live actor with script id `id`.
fn actor(world: &mut World, id: i32) -> Option<Entity> {
    Script::new(world).actors(id).first().copied()
}

fn alive(world: &World, e: Entity) -> bool {
    world.get::<Obj>(e).is_some_and(|o| o.health > 0 && !matches!(o.state, State::Dead | State::Removed))
}

/// Race driving: once the player's checkpoint trigger (20000, on car 10)
/// is armed, drive the opponents' waypoint loop at the car's top speed
/// (capped per leg where the script caps the AI's).
fn race(world: &mut World, bot: &mut Bot, me: &Me, c: &Ctx, route: Vec<(IVec2, i32)>) -> Option<Goal> {
    if !c.has(20000) {
        bot.route_idx = 0;
        bot.race_start = None;
        return Some(Goal::Wait);
    }
    if me.vehicle < 0 {
        return None;
    }
    // Let the grid pull away before setting off (the bot drives through
    // the pack badly).
    let start = *bot.race_start.get_or_insert(bot.tick);
    if bot.tick - start < 60 {
        return Some(Goal::Wait);
    }
    let _ = world;
    Some(Goal::Drive { route })
}

/// Level 10 (docs/re/missions_10.md).
fn hint_10(world: &mut World, bot: &mut Bot, me: &Me, c: &Ctx) -> Option<Goal> {
    use crate::mission::levels::level_10::{AI_X, AI_Y};
    match c.slot {
        0 if c.has(1) => None,
        0 => {
            let route = AI_X
                .iter()
                .zip(AI_Y)
                .map(|(&x, y)| (IVec2::new(x << 11 | 0x400, y << 11 | 0x400), i32::MAX))
                .collect();
            race(world, bot, me, c, route)
        }
        // M100: tail the henchman through both meetings, then kill him.
        1 if [0xa1, 0x75, 0x76, 0x77, 0x69].iter().any(|&e| c.has(e)) => kill_ids(world, &[0x65]),
        // M200: the two mutants (stay out of the gas, trigger 0xcd).
        2 if c.has(0xd5) || c.has(0xdf) => kill_ids(world, &[0xca, 0xcb]).or(Some(Goal::Wait)),
        // M300: the gunmen first, then the contact.
        3 if !c.has(0x12d) => kill_ids(world, &[0x12e, 0x12f, 0x12d]),
        // M400: once there, the two henchmen.
        4 if !c.has(0x191) => kill_ids(world, &[0x192, 0x193]),
        // M600: the tail, never Ike (his "killed" trigger is a failure).
        6 => kill_ids(world, &[0x259]).or(Some(Goal::Wait)),
        // M700: into the room (player y <= 0x36000), then the kidnappers.
        7 if c.has(0x2d1) || c.has(0x2d2) => kill_ids(world, &[0x2be, 0x2bf]).or_else(|| {
            c.has(0x2c7).then_some(Goal::Reach { x: 0x8c00, y: 0x35c00, level: 1, foot: Some(true) })
        }),
        _ => None,
    }
}

/// Level 21 (docs/re/missions_21.md).
fn hint_21(world: &mut World, bot: &mut Bot, me: &Me, c: &Ctx) -> Option<Goal> {
    use crate::mission::levels::level_21::RACE_WAYPOINTS;
    match c.slot {
        // M3 race: the AI's waypoints at the best AI's speed caps.
        3 if c.has(1) => None,
        3 => {
            let route = RACE_WAYPOINTS.iter().map(|&(x, y, _, cap)| (IVec2::new(x, y), cap)).collect();
            race(world, bot, me, c, route)
        }
        // M4: wreck the surveillance van (its "destroyed" is the success).
        4 => kill_ids(world, &[400]),
        // M7: once parked in the zone (0x703 armed), get out and run.
        7 if c.has(703) => Some(Goal::Exit),
        7 if c.has(740) && me.on_foot => Some(Goal::Flee { x: 0x3dc00, y: 0x21c00 }),
        // M5: Nick must reach the mayor's limo route safely; the limo's
        // "destroyed" trigger is a failure. Deal with the gang instead.
        // Then pick Nick up in a car and wait for the limo to get there.
        5 if !c.has(501) => kill_ids(world, &[510, 511, 512, 513])
            .or_else(|| c.has(510).then_some(Goal::EnterModel(-1)))
            .or(Some(Goal::Wait)),
        _ => None,
    }
}

/// Level 20 "Escape from jail" (docs/re/missions_20.md): wait in the cell
/// for the accomplice to open the bars, then get out of the prison block
/// (squares 0..60; the bot teleports out rather than fighting through)
/// and make for the gang's garage at (107,3).
fn hint_20(_world: &mut World, _me: &Me, c: &Ctx) -> Option<Goal> {
    const GARAGE: (i32, i32) = (0x35c00, 0x1c00);
    if c.has(1) {
        return Some(Goal::Wait); // the accomplice is on his way
    }
    if c.has(2) {
        // Stand at the bars, inside the cell, next to him.
        return Some(Goal::Reach { x: 13 * 2048 + 1700, y: 20 * 2048 + 1024, level: 1, foot: Some(true) });
    }
    if c.has(200003) {
        return Some(Goal::Reach { x: GARAGE.0, y: GARAGE.1, level: 0, foot: None });
    }
    if c.has(200000) || c.has(200002) {
        return Some(Goal::Approach { x: GARAGE.0, y: GARAGE.1, level: 0, r: 6000 });
    }
    Some(Goal::Wait)
}

/// Level 11 (docs/re/missions_11.md).
fn hint_11(world: &mut World, me: &Me, c: &Ctx) -> Option<Goal> {
    // M1 apartment block centre.
    const B: (i32, i32) = (0x28400, 0x38400);
    match c.slot {
        // M1: place the remote bomb at the block (weapon 8 = HUD slot 7),
        // then get well clear (a remote bomb is 1e9) and set it off.
        1 if c.has(0x66) && c.has(0x6e) => Some(Goal::Fire(7)),
        1 if c.has(0x6a) && c.has(0x68) => {
            let d = (me.pos.truncate() - IVec2::new(B.0, B.1)).abs().max_element();
            if d < 14000 {
                Some(Goal::Flee { x: B.0, y: B.1 })
            } else {
                Some(Goal::Fire(7))
            }
        }
        // M3: at the helipad, get out.
        // (Out of the helicopter: the generic goals walk the brother to
        // the gang, trigger 0x150.)
        3 if c.has(0x14f) && !me.on_foot => Some(Goal::Exit),
        // M5: at the meeting point, get out; after the swap, kill them all.
        5 if c.has(0x1f7) => Some(Goal::Exit),
        5 if c.has(0x21c) || c.has(0x21d) => kill_ids(world, &[0x1f7, 0x1f8, 0x1f9, 0x1f6]),
        _ => None,
    }
}

/// Kill the first live actor among these ids.
fn kill_ids(world: &mut World, ids: &[i32]) -> Option<Goal> {
    for &id in ids {
        let found = actor(world, id).filter(|&e| alive(world, e));
        if let Some(e) = found {
            return Some(Goal::Kill(e));
        }
    }
    None
}

/// Level 01 (docs/re/missions_01.md).
fn hint_01(world: &mut World, _me: &Me, c: &Ctx) -> Option<Goal> {
    match c.slot {
        // M5: the pickup's driver gets out and attacks; kill him.
        5 if c.has(0x21d) => actor(world, 0x1f6).filter(|&d| alive(world, d)).map(Goal::Kill),
        _ => None,
    }
}

/// Level 00 (docs/re/missions_00.md).
fn hint_00(world: &mut World, me: &Me, c: &Ctx) -> Option<Goal> {
    match c.slot {
        // M1 range: shoot the dummies with the real weapons from behind
        // the white lines (x > 0x5400 for the shotgun, y < 0x1ec00 for
        // the grenades).
        1 => {
            if c.has(0x68) {
                return Some(Goal::Reach { x: 0x5c00, y: me.pos.y, level: 0, foot: Some(true) });
            }
            if c.has(0x75) {
                return Some(Goal::Reach { x: me.pos.x, y: 0x1e000, level: 0, foot: Some(true) });
            }
            for i in 0..3 {
                if c.has(0x6f + i) {
                    let d = actor(world, 0x65 + i).filter(|&d| alive(world, d))?;
                    let y = world.get::<Obj>(d)?.pos.y;
                    return Some(Goal::Shoot { target: d, slot: 2, from: (0x5800, y), charge: 0 });
                }
            }
            for i in 0..3 {
                if c.has(0x7c + i) {
                    let d = actor(world, 0x68 + i).filter(|&d| alive(world, d))?;
                    let x = world.get::<Obj>(d)?.pos.x;
                    // Range of a throw with power p is about 25 p^2.
                    let dist = 0x1fc00 - 0x1e400;
                    let p = ((dist as f32 / 25.0).sqrt()).round() as i32;
                    return Some(Goal::Shoot { target: d, slot: 4, from: (x, 0x1e400), charge: p });
                }
            }
            None
        }
        // M5 van bomb: once parked, get out and run.
        5 if c.has(0x1f9) => {
            let van = actor(world, 0x1f5)?;
            let p = world.get::<Obj>(van)?.pos;
            Some(Goal::Flee { x: p.x, y: p.y })
        }
        // M10 escort: take out the two gunmen before the ambush.
        // (while staying within reach of the limo).
        10 => {
            let limo = actor(world, 0x3e9).filter(|&l| world.get::<Vehicle>(l).is_some())?;
            let lp = world.get::<Obj>(limo)?.pos;
            let gunmen: Vec<Entity> = [0x3ea, 0x3eb].into_iter().filter_map(|id| actor(world, id)).collect();
            gunmen
                .into_iter()
                .find(|&g| {
                    alive(world, g)
                        && world.get::<Person>(g).is_some()
                        && world.get::<Obj>(g).is_some_and(|o| (o.pos - lp).truncate().abs().max_element() < 20000)
                })
                .map(Goal::Kill)
        }
        _ => None,
    }
}
