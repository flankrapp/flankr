//! One timer for every held press in the app
//! (docs/design/20261028_06_BOTTOM_BAR_UPDATE_PLAN.md, decision 7 and the
//! 2026-11-03 12:13 EST amendment): the centre (✦ Gemini) and right
//! (record) buttons, volume, the text input's +, and a question card's
//! record (plan 20261022_03, the L3 amendment of 2026-11-03 14:09 EST);
//! the drawer's remote cards and the overview's plugin cards use its
//! [`HOLD`] too.
//!
//! A press fires after [`HOLD`], 500 ms; [`SLOP`] is 10 pt. A release
//! before then is a tap; at the hold the press fires at once, before the
//! release, and the release is then no tap. A finger that moves more than
//! the slop, or a cancelled touch, ends the press with neither. Nothing is
//! drawn while it is timed: no progress bar and no press colour (the
//! owner, 2026-11-03: "it's bad ux, the time should be adjusted instead").

use std::time::Duration;

use crate::agent::AgentHit;

/// How long every press in the app is held to fire (the 2026-11-03 12:13
/// EST amendment: "make them all 500ms").
pub const HOLD: Duration = Duration::from_millis(500);
/// How far a finger moves, in points, before a press is no tap.
pub const SLOP: f32 = 10.0; // points

/// A press on a control that a [`HOLD`] gives a second action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hold {
    /// The touch.
    pub id: u64,
    /// The control pressed.
    pub hit: AgentHit,
    start: Duration,
    at: [f32; 2],
    fired: bool,
    moved: bool,
}

impl Hold {
    /// A press of touch `id` on `hit` at `at`, from `now` (any monotonic
    /// clock, the same as later calls are given).
    pub fn start(id: u64, hit: AgentHit, at: [f32; 2], now: Duration) -> Self {
        Hold {
            id,
            hit,
            start: now,
            at,
            fired: false,
            moved: false,
        }
    }

    /// True once, when its hold has passed unmoved.
    pub fn fire(&mut self, now: Duration) -> bool {
        if self.fired || self.moved || now.saturating_sub(self.start) < HOLD {
            return false;
        }
        self.fired = true;
        true
    }

    /// A move: past SLOP (scaled by `s`) the press is no tap and never
    /// fires.
    pub fn moved_to(&mut self, at: [f32; 2], s: f32) {
        let slop = SLOP * s;
        if (at[0] - self.at[0]).abs() > slop || (at[1] - self.at[1]).abs() > slop {
            self.moved = true;
        }
    }

    /// The release: a tap if it neither fired nor moved.
    pub fn is_tap(&self) -> bool {
        !self.fired && !self.moved
    }

    /// Whether it fired.
    pub fn fired(&self) -> bool {
        self.fired
    }

    /// Whether it moved past the slop before firing: no tap and no hold.
    pub fn moved_off(&self) -> bool {
        self.moved && !self.fired
    }

    /// Where the touch started.
    pub fn at(&self) -> [f32; 2] {
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_secs(10);

    #[test]
    fn every_control_fires_once_at_500_ms_and_a_release_before_is_a_tap() {
        let ms = Duration::from_millis;
        assert_eq!(HOLD, ms(500));
        for hit in [
            AgentHit::GeneralAgent,
            AgentHit::Message,
            AgentHit::Volume,
            AgentHit::PromptAttach(crate::prompt::PromptKind::Message),
        ] {
            let mut h = Hold::start(1, hit, [100.0, 100.0], T0);
            assert!(!h.fire(T0 + ms(499)), "{hit:?}");
            assert!(h.is_tap(), "released before 500 ms: a tap");
            assert!(h.fire(T0 + HOLD), "{hit:?}");
            assert!(!h.fire(T0 + ms(2000)), "once");
            assert!(!h.is_tap(), "the release after it fired is no tap");
            assert!(h.fired());
        }
    }

    #[test]
    fn a_move_past_the_slop_cancels_both() {
        let s = 2.0;
        let mut h = Hold::start(1, AgentHit::GeneralAgent, [100.0, 100.0], T0);
        // 10 pt is still the press.
        h.moved_to([100.0 + 10.0 * s, 100.0], s);
        assert!(h.is_tap());
        // 11 pt is not.
        h.moved_to([100.0, 100.0 - 11.0 * s], s);
        assert!(!h.is_tap());
        assert!(!h.fire(T0 + HOLD));
    }
}
