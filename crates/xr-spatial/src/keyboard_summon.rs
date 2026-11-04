//! The gesture that opens and closes the typing keyboard in immersive mode, for people with no
//! physical keyboard to press Super+K on.
//!
//! Pinch (hands) or squeeze the trigger (controllers) with both hands at once while neither
//! points at a window, and hold for [`HOLD`]. Pointing at a window is a click or a grab, never
//! this, so the gesture cannot fire while using the screen.

use std::time::{Duration, Instant};

/// How long both hands hold the gesture before the keyboard toggles.
pub const HOLD: Duration = Duration::from_millis(600);

#[derive(Debug, Default)]
pub struct KeyboardSummon {
    held_since: Option<Instant>,
    fired: bool,
}

impl KeyboardSummon {
    /// Feeds one frame: `held` is true while both hands select with no window under either ray.
    /// Returns true once per hold, when it reaches [`HOLD`]; letting go re-arms it.
    pub fn update(&mut self, now: Instant, held: bool) -> bool {
        if !held {
            *self = Self::default();
            return false;
        }
        let since = *self.held_since.get_or_insert(now);
        if !self.fired && now.duration_since(since) >= HOLD {
            self.fired = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_after_the_hold_and_rearms_on_release() {
        let t0 = Instant::now();
        let mut summon = KeyboardSummon::default();
        assert!(!summon.update(t0, true));
        assert!(!summon.update(t0 + HOLD / 2, true), "not held long enough");
        assert!(summon.update(t0 + HOLD, true));
        assert!(
            !summon.update(t0 + HOLD * 3, true),
            "keeps holding without repeating"
        );
        assert!(!summon.update(t0 + HOLD * 4, false));
        assert!(!summon.update(t0 + HOLD * 5, true));
        assert!(summon.update(t0 + HOLD * 6, true), "a new hold fires again");
    }

    #[test]
    fn a_dropped_pinch_restarts_the_hold() {
        let t0 = Instant::now();
        let mut summon = KeyboardSummon::default();
        assert!(!summon.update(t0, true));
        assert!(!summon.update(t0 + HOLD / 2, false));
        assert!(!summon.update(t0 + HOLD, true));
        assert!(!summon.update(t0 + HOLD + HOLD / 2, true));
        assert!(summon.update(t0 + HOLD * 2, true));
    }
}
