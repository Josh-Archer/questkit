//! Desk calibration, screen snapping above the desk, and a desk-surface touchpad.
//!
//! None of this needs OpenXR. Callers pass in world-space hand-joint positions:
//!
//! - [`DeskDetector`] watches both palms: resting them still, flat and level on the desk
//!   for [`DESK_HOLD_SECONDS`] calibrates a [`DeskSurface`] (see [`DeskSurface::calibrate`]).
//!   The Quest Scene API is not requested, so this palm hold is the desk detection the
//!   runtime runs every frame; the result serializes into the saved spatial layout.
//! - [`DeskSurface::snap_screen`] places a screen of a given height so its bottom edge sits a
//!   gap above the desk.
//! - [`SurfaceTouchpad`] turns fingertip contacts with a region of the desk into tap, drag and
//!   two-finger scroll events for the application to consume.

use super::pose::Vec3;
use serde::{Deserialize, Serialize};

/// A fingertip within this distance of the desk counts as touching it, in meters.
pub const TOUCH_THRESHOLD_M: f32 = 0.012;
/// Maximum palm height spread accepted during calibration, in meters.
pub const MAX_CALIBRATION_SPREAD_M: f32 = 0.03;
/// A contact shorter than this many seconds that barely moves is a tap.
pub const TAP_MAX_SECONDS: f32 = 0.25;
/// Movement below this counts as "did not move" for taps, in meters.
pub const TAP_MAX_TRAVEL_M: f32 = 0.008;
/// How far a resting palm sits above the desk surface, in meters.
pub const PALM_THICKNESS_M: f32 = 0.02;
/// Gap left between a snapped screen's bottom edge and the desk, in meters.
pub const DESK_SCREEN_GAP_M: f32 = 0.05;
/// How long both palms must rest still and level before a desk is detected, in seconds.
pub const DESK_HOLD_SECONDS: f32 = 0.8;
/// A resting palm may drift at most this far from where the hold started, in meters.
pub const PALM_STILL_TOLERANCE_M: f32 = 0.015;
/// Plausible desk heights in stage space (floor at y=0), in meters.
pub const DESK_MIN_HEIGHT_M: f32 = 0.35;
pub const DESK_MAX_HEIGHT_M: f32 = 1.25;
/// A re-detected desk must differ by more than this to replace the current one, in meters.
pub const DESK_CHANGE_TOLERANCE_M: f32 = 0.01;
/// Consecutive frames a palm may drop out of tracking without resetting a hold in progress.
/// Missing frames add no hold time; any longer gap starts the hold over.
pub const PALM_TRACKING_GRACE_FRAMES: u32 = 5;
/// Size of the desk touchpad region, in meters (width x depth).
pub const TOUCHPAD_WIDTH_M: f32 = 0.28;
pub const TOUCHPAD_DEPTH_M: f32 = 0.20;
/// Touchpad travel converted to relative mouse pixels per meter.
pub const TOUCHPAD_MOVE_PX_PER_M: f32 = 3000.0;
/// Touchpad travel converted to hi-res scroll units per meter (matches hand scroll).
pub const TOUCHPAD_SCROLL_PER_M: f32 = 40.0;

/// A calibrated horizontal desk. `center` is on the desk surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeskSurface {
    pub center: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// Fewer than 2 palm samples.
    NotEnoughSamples,
    /// The samples are not level enough to be one desk.
    Uneven,
}

impl DeskSurface {
    /// Fits the desk from palm positions (meters). Palms rest about `palm_thickness_m` above
    /// the surface.
    pub fn calibrate(palms: &[Vec3], palm_thickness_m: f32) -> Result<Self, CalibrationError> {
        if palms.len() < 2 {
            return Err(CalibrationError::NotEnoughSamples);
        }
        let (lo, hi) = palms.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
        if hi - lo > MAX_CALIBRATION_SPREAD_M {
            return Err(CalibrationError::Uneven);
        }
        let sum = palms.iter().fold(Vec3::ZERO, |acc, &p| acc + p);
        let mut center = sum / palms.len() as f32;
        center.y -= palm_thickness_m;
        Ok(Self { center })
    }

    pub fn height(&self) -> f32 {
        self.center.y
    }

    /// Signed height of `p` above the desk.
    pub fn height_above(&self, p: Vec3) -> f32 {
        p.y - self.center.y
    }

    /// Center position of a screen `screen_height_m` tall whose bottom edge is `gap_m` above
    /// the desk. Keeps the screen's horizontal position.
    pub fn snap_screen(&self, screen_center: Vec3, screen_height_m: f32, gap_m: f32) -> Vec3 {
        Vec3::new(
            screen_center.x,
            self.center.y + gap_m + screen_height_m / 2.0,
            screen_center.z,
        )
    }
}

impl DeskSurface {
    /// The touchpad region on this desk: a [`TOUCHPAD_WIDTH_M`] x [`TOUCHPAD_DEPTH_M`]
    /// rectangle centred where the palms rested.
    pub fn touchpad(&self) -> SurfaceTouchpad {
        SurfaceTouchpad::new(
            *self,
            (
                self.center.x - TOUCHPAD_WIDTH_M / 2.0,
                self.center.z - TOUCHPAD_DEPTH_M / 2.0,
            ),
            (
                self.center.x + TOUCHPAD_WIDTH_M / 2.0,
                self.center.z + TOUCHPAD_DEPTH_M / 2.0,
            ),
        )
    }
}

/// Detects a desk from tracked palms: both palms resting still, level, and at a plausible
/// desk height for [`DESK_HOLD_SECONDS`] calibrate a [`DeskSurface`]. This is the only desk
/// sensor the client has on device (Quest Scene API planes are not requested), so resting
/// both hands flat on the desk is the calibration gesture.
#[derive(Debug, Clone, Default)]
pub struct DeskDetector {
    candidate: Option<([Vec3; 2], f32)>,
    detected: Option<DeskSurface>,
    /// The current hold already emitted its desk; cleared when the hold ends.
    hold_emitted: bool,
    /// Consecutive frames with fewer than two tracked palms during a hold.
    missing_frames: u32,
}

impl DeskDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// The desk currently in effect, detected or restored from a saved layout.
    pub fn detected(&self) -> Option<DeskSurface> {
        self.detected
    }

    /// Adopts a desk restored from a saved layout.
    pub fn restore(&mut self, desk: DeskSurface) {
        self.detected = Some(desk);
        self.reset_hold();
    }

    fn reset_hold(&mut self) {
        self.candidate = None;
        self.hold_emitted = false;
        self.missing_frames = 0;
    }

    /// Feeds one frame of palm positions (0, 1 or 2 tracked palms, meters, stage space).
    /// Returns the desk once per completed hold, even when it matches the current desk, so
    /// every palms-flat gesture re-snaps the screens. A brief tracking dropout (up to
    /// [`PALM_TRACKING_GRACE_FRAMES`]) pauses the hold instead of resetting it.
    pub fn update(&mut self, palms: &[Vec3], dt: f32) -> Option<DeskSurface> {
        let (Some(&left), Some(&right)) = (palms.first(), palms.get(1)) else {
            if self.candidate.is_some() {
                self.missing_frames += 1;
                if self.missing_frames > PALM_TRACKING_GRACE_FRAMES {
                    self.reset_hold();
                }
            }
            return None;
        };
        self.missing_frames = 0;
        let pair = [left, right];
        let plausible = pair
            .iter()
            .all(|p| (DESK_MIN_HEIGHT_M..=DESK_MAX_HEIGHT_M).contains(&(p.y - PALM_THICKNESS_M)))
            && DeskSurface::calibrate(&pair, PALM_THICKNESS_M).is_ok();
        if !plausible {
            self.reset_hold();
            return None;
        }
        match &mut self.candidate {
            Some((start, held))
                if pair
                    .iter()
                    .zip(start.iter())
                    .all(|(p, s)| (*p - *s).length() <= PALM_STILL_TOLERANCE_M) =>
            {
                *held += dt.max(0.0);
            }
            _ => {
                // A new hold starts from a clean state; reset_hold is the only place the
                // emitted latch is cleared.
                self.reset_hold();
                self.candidate = Some((pair, 0.0));
            }
        }
        let held = self.candidate.map(|(_, h)| h).unwrap_or(0.0);
        if held < DESK_HOLD_SECONDS || self.hold_emitted {
            return None;
        }
        let desk = DeskSurface::calibrate(&pair, PALM_THICKNESS_M).ok()?;
        self.hold_emitted = true;
        // Keep the current desk when the new reading is within tolerance, so the surface
        // does not jitter, but still emit it so the caller re-snaps.
        let changed = self
            .detected
            .is_none_or(|d| (d.center - desk.center).length() > DESK_CHANGE_TOLERANCE_M);
        if changed {
            self.detected = Some(desk);
        }
        self.detected
    }
}

/// Touchpad output, with deltas in touchpad-local meters (x right, y away from the user).
#[derive(Debug, Clone, PartialEq)]
pub enum TouchpadEvent {
    Tap,
    /// One finger moving on the pad.
    Move {
        dx: f32,
        dy: f32,
    },
    /// Two fingers moving together.
    Scroll {
        dx: f32,
        dy: f32,
    },
}

/// A rectangular desk region used as a touchpad, axis-aligned in world X/Z.
#[derive(Debug, Clone)]
pub struct SurfaceTouchpad {
    pub desk: DeskSurface,
    pub min: (f32, f32),
    pub max: (f32, f32),
    contact: Option<Contact>,
}

#[derive(Debug, Clone)]
struct Contact {
    fingers: usize,
    start: (f32, f32),
    last: (f32, f32),
    elapsed: f32,
    moved: bool,
}

impl SurfaceTouchpad {
    /// `min`/`max` are world (x, z) corners of the pad region.
    pub fn new(desk: DeskSurface, min: (f32, f32), max: (f32, f32)) -> Self {
        Self {
            desk,
            min,
            max,
            contact: None,
        }
    }

    fn touching(&self, tip: Vec3) -> bool {
        self.desk.height_above(tip).abs() <= TOUCH_THRESHOLD_M
            && (self.min.0..=self.max.0).contains(&tip.x)
            && (self.min.1..=self.max.1).contains(&tip.z)
    }

    /// Feeds one frame: index and middle fingertips of the touchpad hand, and the time since
    /// the previous frame in seconds.
    pub fn update(&mut self, fingertips: &[Vec3], dt: f32) -> Option<TouchpadEvent> {
        let down: Vec<Vec3> = fingertips
            .iter()
            .copied()
            .filter(|&t| self.touching(t))
            .collect();
        if down.is_empty() {
            let c = self.contact.take()?;
            let travel = ((c.last.0 - c.start.0).powi(2) + (c.last.1 - c.start.1).powi(2)).sqrt();
            return (c.fingers == 1
                && !c.moved
                && travel < TAP_MAX_TRAVEL_M
                && c.elapsed <= TAP_MAX_SECONDS)
                .then_some(TouchpadEvent::Tap);
        }
        let n = down.len() as f32;
        let centroid = down
            .iter()
            .fold((0.0, 0.0), |a, t| (a.0 + t.x / n, a.1 + t.z / n));
        // Desk-local y points away from the user, which is world -Z.
        let fingers = down.len().min(2);
        match &mut self.contact {
            None => {
                self.contact = Some(Contact {
                    fingers,
                    start: centroid,
                    last: centroid,
                    elapsed: 0.0,
                    moved: false,
                });
                None
            }
            Some(c) => {
                c.elapsed += dt;
                if fingers != c.fingers {
                    // Finger count changed: restart from here so the jump is not a delta.
                    c.fingers = c.fingers.max(fingers);
                    c.last = centroid;
                    c.moved = true;
                    return None;
                }
                let (dx, dy) = (centroid.0 - c.last.0, -(centroid.1 - c.last.1));
                c.last = centroid;
                let travel =
                    ((centroid.0 - c.start.0).powi(2) + (centroid.1 - c.start.1).powi(2)).sqrt();
                if travel < TAP_MAX_TRAVEL_M && !c.moved {
                    return None;
                }
                c.moved = true;
                Some(if fingers == 2 {
                    TouchpadEvent::Scroll { dx, dy }
                } else {
                    TouchpadEvent::Move { dx, dy }
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desk() -> DeskSurface {
        DeskSurface::calibrate(
            &[Vec3::new(-0.2, 0.765, -0.4), Vec3::new(0.2, 0.775, -0.4)],
            0.02,
        )
        .unwrap()
    }

    #[test]
    fn calibration_fits_level_desk_and_rejects_bad_samples() {
        let d = desk();
        assert!((d.height() - 0.75).abs() < 1e-5);
        assert_eq!(
            DeskSurface::calibrate(&[Vec3::ZERO], 0.02),
            Err(CalibrationError::NotEnoughSamples)
        );
        assert_eq!(
            DeskSurface::calibrate(&[Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.0, 0.8, 0.0)], 0.02),
            Err(CalibrationError::Uneven)
        );
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<DeskSurface>(&json).unwrap(), d);
    }

    #[test]
    fn screen_snaps_bottom_edge_above_desk() {
        let p = desk().snap_screen(Vec3::new(0.1, 1.6, -0.9), 0.4, 0.05);
        assert!((p.y - (0.75 + 0.05 + 0.2)).abs() < 1e-5);
        assert_eq!((p.x, p.z), (0.1, -0.9));
    }

    fn pad() -> SurfaceTouchpad {
        SurfaceTouchpad::new(desk(), (0.2, -0.5), (0.4, -0.3))
    }

    fn tip(x: f32, z: f32) -> Vec3 {
        Vec3::new(x, 0.752, z)
    }

    #[test]
    fn short_still_touch_is_a_tap() {
        let mut p = pad();
        assert_eq!(p.update(&[tip(0.3, -0.4)], 0.011), None);
        assert_eq!(p.update(&[tip(0.301, -0.4)], 0.011), None);
        assert_eq!(p.update(&[], 0.011), Some(TouchpadEvent::Tap));
        // Fingertip hovering above the desk does not touch.
        assert_eq!(p.update(&[Vec3::new(0.3, 0.80, -0.4)], 0.011), None);
        assert_eq!(p.update(&[], 0.011), None);
    }

    #[test]
    fn one_finger_drag_moves_and_two_fingers_scroll() {
        let mut p = pad();
        p.update(&[tip(0.25, -0.35)], 0.011);
        assert!(matches!(
            p.update(&[tip(0.27, -0.37)], 0.011),
            Some(TouchpadEvent::Move { dx, dy }) if dx > 0.0 && dy > 0.0
        ));
        assert_eq!(p.update(&[], 0.011), None, "a drag is not a tap");

        p.update(&[tip(0.30, -0.40), tip(0.32, -0.40)], 0.011);
        assert!(matches!(
            p.update(&[tip(0.30, -0.38), tip(0.32, -0.38)], 0.011),
            Some(TouchpadEvent::Scroll { dy, .. }) if dy < 0.0
        ));
    }

    fn palms(y: f32) -> [Vec3; 2] {
        [Vec3::new(-0.2, y, -0.4), Vec3::new(0.2, y, -0.4)]
    }

    #[test]
    fn detector_needs_both_palms_held_still_before_detecting() {
        let mut det = DeskDetector::new();
        // One palm only: never detects.
        for _ in 0..200 {
            assert_eq!(det.update(&palms(0.77)[..1], 0.011), None);
        }
        assert!(det.detected().is_none());
        // Both palms, but only briefly: no detection yet.
        let mut det = DeskDetector::new();
        for _ in 0..10 {
            assert_eq!(det.update(&palms(0.77), 0.011), None);
        }
        assert!(det.detected().is_none());
        // Both palms held still long enough: detects once, at palm height minus thickness.
        let mut found = None;
        for _ in 0..200 {
            found = det.update(&palms(0.77), 0.011).or(found);
        }
        let desk = found.expect("held level palms should detect a desk");
        assert!((desk.height() - 0.75).abs() < 1e-5);
        assert_eq!(det.detected(), Some(desk));
        // Continuing to hold the same desk does not re-emit.
        assert_eq!(det.update(&palms(0.77), 0.011), None);
    }

    #[test]
    fn detector_rejects_moving_uneven_and_implausible_palms() {
        // Palms drifting around never settle.
        let mut det = DeskDetector::new();
        for i in 0..200 {
            let x = (i as f32) * 0.01;
            assert_eq!(
                det.update(
                    &[
                        Vec3::new(-0.2 + x, 0.77, -0.4),
                        Vec3::new(0.2 + x, 0.77, -0.4)
                    ],
                    0.011
                ),
                None
            );
        }
        // Uneven palms are not one desk.
        let mut det = DeskDetector::new();
        for _ in 0..200 {
            assert_eq!(
                det.update(
                    &[Vec3::new(-0.2, 0.70, -0.4), Vec3::new(0.2, 0.80, -0.4)],
                    0.011
                ),
                None
            );
        }
        // Knee height and head height are not desks.
        for y in [0.20, 1.60] {
            let mut det = DeskDetector::new();
            for _ in 0..200 {
                assert_eq!(det.update(&palms(y), 0.011), None, "y={y}");
            }
        }
        // Losing a hand for longer than the grace period resets the hold.
        let mut det = DeskDetector::new();
        for _ in 0..50 {
            det.update(&palms(0.77), 0.011);
        }
        for _ in 0..=PALM_TRACKING_GRACE_FRAMES {
            assert_eq!(det.update(&[], 0.011), None);
        }
        for _ in 0..10 {
            assert_eq!(det.update(&palms(0.77), 0.011), None);
        }
        assert!(det.detected().is_none());
    }

    #[test]
    fn every_completed_hold_re_emits_the_same_desk() {
        let hold = |det: &mut DeskDetector| {
            let mut found = None;
            for _ in 0..200 {
                found = det.update(&palms(0.77), 0.011).or(found);
            }
            found
        };
        let mut det = DeskDetector::new();
        let first = hold(&mut det).expect("first hold detects");
        // Lift the hands, then rest them on the same desk again: the desk is emitted again.
        for _ in 0..=PALM_TRACKING_GRACE_FRAMES {
            det.update(&[], 0.011);
        }
        assert_eq!(hold(&mut det), Some(first));
        // A restored desk is also re-emitted by a fresh hold on it.
        let mut det = DeskDetector::new();
        det.restore(first);
        assert_eq!(hold(&mut det), Some(first));
    }

    #[test]
    fn hands_leaving_and_returning_to_the_same_desk_re_emits_it() {
        let mut det = DeskDetector::new();
        let mut first = None;
        for _ in 0..200 {
            first = det.update(&palms(0.77), 0.011).or(first);
        }
        let first = first.expect("first hold detects");
        assert!(det.hold_emitted);
        // Hands gone past the grace period: the hold is reset, latch cleared.
        for _ in 0..=PALM_TRACKING_GRACE_FRAMES {
            assert_eq!(det.update(&[], 0.011), None);
        }
        assert!(det.candidate.is_none(), "the hold should have been reset");
        assert!(!det.hold_emitted, "reset_hold must clear the emitted latch");
        // A fresh hold on the same desk emits it again, exactly once.
        let emitted = (0..200)
            .filter_map(|_| det.update(&palms(0.77), 0.011))
            .collect::<Vec<_>>();
        assert_eq!(emitted, vec![first]);
    }

    #[test]
    fn brief_tracking_dropout_does_not_reset_the_hold() {
        let mut det = DeskDetector::new();
        // 0.55 s held, a 3-frame dropout, then 0.33 s more: 0.88 s total completes the hold.
        for _ in 0..50 {
            assert_eq!(det.update(&palms(0.77), 0.011), None);
        }
        for _ in 0..3 {
            assert_eq!(det.update(&palms(0.77)[..1], 0.011), None);
        }
        let mut found = None;
        for _ in 0..30 {
            found = det.update(&palms(0.77), 0.011).or(found);
        }
        assert!(
            found.is_some(),
            "a short dropout should not restart the hold"
        );
    }

    #[test]
    fn touches_outside_the_pad_are_ignored() {
        let mut p = pad();
        assert_eq!(p.update(&[tip(0.0, -0.4)], 0.011), None);
        assert_eq!(p.update(&[], 0.011), None);
    }
}
