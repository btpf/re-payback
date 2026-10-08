//! Floating labels: a text at a point of the world that drifts north and
//! fades after 60 ticks (Mac `AddLabel__FiiiiPc` @ 10094c10,
//! `AddLabelPercent__Fiiiii` @ 10094d40, `DoLabels__Fsiiii` @ 10094f20;
//! 68k `FUN_0101c9a8`). The game adds them for the score it pays and the
//! pickups it hands out:
//!
//! - `AddLabelPercent(x, y, z, 2, n)` writes `"%d"` of n, only in single
//!   player and only when |n| > 10: at the player for the kills
//!   (`CheckHitObject` @ 100542d0: 2500 / 1500 × multiplier), the script's
//!   `AddScore` (`AddPlScore` @ 10052630) and "Irony bonus!"
//!   (`CheckForCollisions`, 10000 × multiplier); at the hijacked vehicle
//!   for the carjack (`GetInOutVehicles`, 1500 police / 1000 × multiplier).
//!   The Copkiller bonus, explosion damage and punches pay without one.
//! - `AddLabel(x, y, z, 2, text)`: the pickup's name (phrases 2100–2108)
//!   at the square centre and the player's z (`CheckForWeapon` @ 100597e0).
//!
//! At most 500 (the 501st is dropped). Each is drawn per view after the
//! flares (`DrawView` @ 1006eb80) with `DisplayTextAlpha` in font 2
//! ("Them") at `(sx − 7·len/2, sy + 3)` of its projected point, the pitch
//! 7 (doubled in hi-res, the centring offset not); then, once per frame,
//! it moves 32 units north per tick and ages; at 0 it is removed (the last
//! one takes its place). Here the labels age in the fixed tick before the
//! game logic, so a new label is first drawn where it was added, as in the
//! original.

use bevy::prelude::*;

/// `MAX_LABELS`.
const MAX_LABELS: usize = 500;
/// Lifetime in ticks.
const LIFE: i32 = 60;
/// Northward drift per tick (object units).
const DRIFT: i32 = 32;

#[derive(Clone, Debug)]
pub struct Label {
    /// Object units (x, y; z down).
    pub pos: IVec3,
    pub text: String,
    life: i32,
}

/// The labels on screen.
#[derive(Resource, Default)]
pub struct Labels(pub Vec<Label>);

impl Labels {
    /// `AddLabel(x, y, z, 2, text)`.
    pub fn add(&mut self, pos: IVec3, text: impl Into<String>) {
        if self.0.len() >= MAX_LABELS {
            warn!("AddLabel(): MAX_LABELS reached");
            return;
        }
        self.0.push(Label {
            pos,
            text: text.into(),
            life: LIFE,
        });
    }

    /// `AddLabelPercent(x, y, z, 2, n)`: `"%d"`, single player only, when
    /// |n| > 10.
    pub fn add_score(&mut self, pos: IVec3, n: i32, multiplayer: bool) {
        if !multiplayer && n.abs() > 10 {
            self.add(pos, n.to_string());
        }
    }

    /// One tick of `DoLabels`: drift north, age, remove the expired.
    fn tick(&mut self) {
        let mut i = 0;
        while i < self.0.len() {
            let l = &mut self.0[i];
            l.pos.y -= DRIFT;
            l.life -= 1;
            if l.life < 1 {
                self.0.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Labels>()
        .add_systems(FixedPreUpdate, |mut labels: ResMut<Labels>| labels.tick())
        .add_systems(OnExit(crate::level::AppState::Playing), |mut labels: ResMut<Labels>| {
            labels.0.clear();
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_labels_need_more_than_ten_in_single_player() {
        let mut l = Labels::default();
        l.add_score(IVec3::ZERO, 10, false);
        l.add_score(IVec3::ZERO, -11, false);
        l.add_score(IVec3::ZERO, 1500, true);
        assert_eq!(l.0.len(), 1);
        assert_eq!(l.0[0].text, "-11");
    }

    #[test]
    fn labels_drift_north_and_expire_after_60_ticks() {
        let mut l = Labels::default();
        l.add(IVec3::new(0, 1000, 0), "Bazooka");
        for _ in 0..59 {
            l.tick();
        }
        assert_eq!(l.0[0].pos.y, 1000 - 59 * 32);
        l.tick();
        assert!(l.0.is_empty());
    }
}
