//! `PAYBACK_DET_CHECK=1`: a debug guard for replay determinism. Logs every
//! [`Obj`] entity whose component set changed between the end of one game
//! tick and the start of the next, i.e. outside the simulation (render,
//! audio or UI code in `Update`/`PostUpdate`). Such a change moves the
//! entity to another ECS table at a frame-rate-dependent moment; the
//! simulation must not depend on table order ([`super::table_order`]), but
//! the rule is that render code never adds or removes components on
//! simulated objects (see [`Obj`]). Costs nothing when off (the systems are
//! not added).
//!
//! Without Bevy's `debug` feature component names are stripped; the log
//! gives component ids, which the startup line maps for the usual suspects.

use super::Obj;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default)]
struct Snapshot(HashMap<Entity, Vec<ComponentId>>);

fn components(world: &mut World) -> HashMap<Entity, Vec<ComponentId>> {
    let mut q = world.query_filtered::<Entity, With<Obj>>();
    let es: Vec<Entity> = q.iter(world).collect();
    es.into_iter()
        .map(|e| (e, world.entity(e).archetype().components().iter().copied().collect()))
        .collect()
}

fn tick_end(world: &mut World) {
    let s = components(world);
    world.insert_resource(Snapshot(s));
}

fn tick_start(world: &mut World) {
    let now = components(world);
    let Some(before) = world.get_resource::<Snapshot>() else {
        return;
    };
    for (e, comps) in &now {
        let Some(prev) = before.0.get(e) else { continue };
        if prev != comps {
            let added: Vec<_> = comps.iter().filter(|c| !prev.contains(c)).collect();
            let removed: Vec<_> = prev.iter().filter(|c| !comps.contains(c)).collect();
            warn!(
                "determinism: {e:?} changed components outside the game tick: added {added:?}, removed {removed:?}"
            );
        }
    }
}

fn name_ids(world: &mut World) {
    macro_rules! ids {
        ($($t:ty),*) => {
            let v = vec![$((world.register_component::<$t>(), stringify!($t))),*];
            info!("determinism check on; component ids: {v:?}");
        };
    }
    ids!(
        Children,
        ChildOf,
        Visibility,
        Mesh3d,
        bevy::camera::primitives::Aabb,
        bevy::pbr::PreviousGlobalTransform,
        crate::objects::OverlayChildren,
        crate::objects::DoorPanel
    );
}

pub fn plugin(app: &mut App) {
    if std::env::var_os("PAYBACK_DET_CHECK").is_none() {
        return;
    }
    app.add_systems(Startup, name_ids)
        .add_systems(FixedFirst, tick_start)
        .add_systems(FixedLast, tick_end);
}
