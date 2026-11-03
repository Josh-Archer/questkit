//! Tracked physical keyboard: the passthrough cutout around it and typing input suppression.
//!
//! A runtime keyboard tracker (`XR_FB_keyboard_tracking`) reports a pose and a bounding box.
//! This module has no OpenXR calls. It turns that pose and box into:
//!
//! - [`TrackedKeyboard::cutout_corners`]: the four world-space corners of a passthrough hole
//!   above the keyboard, so the real keys stay visible in immersive mode;
//! - [`TypingSuppressor`]: per-hand "is typing" state. While a hand's fingertips are over the
//!   keyboard, plus a short hold time after they leave, the client drops that hand's ray and
//!   pinch input so typing does not cause stray clicks.

use super::pose::{ScreenPose, Vec3};
use std::time::{Duration, Instant};

/// Default extra border around the keyboard, in meters.
pub const DEFAULT_KEYBOARD_MARGIN_M: f32 = 0.03;
/// Height above the key tops that still counts as typing, in meters.
pub const DEFAULT_HOVER_HEIGHT_M: f32 = 0.06;
/// How long suppression lasts after the fingertips leave the keyboard.
pub const DEFAULT_TYPING_HOLD: Duration = Duration::from_millis(400);

/// A keyboard pose from the tracker. Local axes: +X along the keys (width), +Y up from the
/// key tops, +Z toward the user (depth). `position` is the center of the key top surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackedKeyboard {
    pub pose: ScreenPose,
    /// Width (X) and depth (Z) in meters.
    pub width_m: f32,
    pub depth_m: f32,
}

impl TrackedKeyboard {
    pub fn new(pose: ScreenPose, width_m: f32, depth_m: f32) -> Self {
        Self {
            pose,
            width_m,
            depth_m,
        }
    }

    /// Converts a world-space point into keyboard-local coordinates.
    pub fn to_local(&self, world: Vec3) -> Vec3 {
        self.pose
            .orientation
            .conjugate()
            .rotate_vec(world - self.pose.position)
    }

    /// Whether `world` is over the keyboard: inside the box grown by `margin_m` on each side,
    /// and between slightly below the key tops and `hover_m` above them.
    pub fn is_over(&self, world: Vec3, margin_m: f32, hover_m: f32) -> bool {
        let p = self.to_local(world);
        p.x.abs() <= self.width_m / 2.0 + margin_m
            && p.z.abs() <= self.depth_m / 2.0 + margin_m
            && p.y >= -0.02
            && p.y <= hover_m
    }

    /// World-space corners of the passthrough cutout, counter-clockwise seen from above:
    /// back-left, front-left, front-right, back-right.
    pub fn cutout_corners(&self, margin_m: f32) -> [Vec3; 4] {
        let hx = self.width_m / 2.0 + margin_m;
        let hz = self.depth_m / 2.0 + margin_m;
        [(-hx, -hz), (-hx, hz), (hx, hz), (hx, -hz)].map(|(x, z)| {
            self.pose.position + self.pose.orientation.rotate_vec(Vec3::new(x, 0.0, z))
        })
    }
}

/// Which hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Per-hand typing detection with a hold time.
#[derive(Debug, Clone)]
pub struct TypingSuppressor {
    pub margin_m: f32,
    pub hover_m: f32,
    pub hold: Duration,
    last_typing: [Option<Instant>; 2],
}

impl Default for TypingSuppressor {
    fn default() -> Self {
        Self {
            margin_m: DEFAULT_KEYBOARD_MARGIN_M,
            hover_m: DEFAULT_HOVER_HEIGHT_M,
            hold: DEFAULT_TYPING_HOLD,
            last_typing: [None; 2],
        }
    }
}

impl TypingSuppressor {
    fn slot(hand: Hand) -> usize {
        match hand {
            Hand::Left => 0,
            Hand::Right => 1,
        }
    }

    /// Feeds one frame of fingertip positions for `hand`. With no tracked keyboard, nothing is
    /// suppressed. Returns whether that hand's pointer input must be dropped this frame.
    pub fn update(
        &mut self,
        hand: Hand,
        keyboard: Option<&TrackedKeyboard>,
        fingertips: &[Vec3],
        now: Instant,
    ) -> bool {
        let slot = Self::slot(hand);
        let Some(kb) = keyboard else {
            self.last_typing[slot] = None;
            return false;
        };
        if fingertips
            .iter()
            .any(|&tip| kb.is_over(tip, self.margin_m, self.hover_m))
        {
            self.last_typing[slot] = Some(now);
        }
        self.is_suppressed(hand, now)
    }

    /// Whether `hand` is typing or was within the hold time.
    pub fn is_suppressed(&self, hand: Hand, now: Instant) -> bool {
        self.last_typing[Self::slot(hand)]
            .is_some_and(|t| now.saturating_duration_since(t) <= self.hold)
    }
}

#[cfg(test)]
mod tests {
    use super::super::pose::Quat;
    use super::*;

    fn keyboard() -> TrackedKeyboard {
        // 44 x 14 cm keyboard on a desk 0.75 m high, 0.4 m in front of the user, turned 90°.
        TrackedKeyboard::new(
            ScreenPose::new(
                Vec3::new(0.0, 0.75, -0.4),
                Quat::from_axis_angle(Vec3::UNIT_Y, std::f32::consts::FRAC_PI_2),
            ),
            0.44,
            0.14,
        )
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn cutout_follows_keyboard_pose_and_margin() {
        let kb = keyboard();
        let c = kb.cutout_corners(0.01);
        // Rotated 90° about Y: local +X maps to world -Z, local +Z to world +X.
        assert!(close(c[0], Vec3::new(-0.08, 0.75, -0.4 + 0.23)));
        assert!(close(c[2], Vec3::new(0.08, 0.75, -0.4 - 0.23)));
        for corner in c {
            assert!((corner.y - 0.75).abs() < 1e-5);
        }
    }

    #[test]
    fn over_keyboard_checks_footprint_and_height() {
        let kb = keyboard();
        assert!(kb.is_over(Vec3::new(0.0, 0.77, -0.4), 0.0, 0.06));
        // Along the keyboard's width (world Z after rotation).
        assert!(kb.is_over(Vec3::new(0.0, 0.76, -0.4 + 0.21), 0.0, 0.06));
        assert!(!kb.is_over(Vec3::new(0.0, 0.76, -0.4 + 0.25), 0.0, 0.06));
        assert!(kb.is_over(Vec3::new(0.0, 0.76, -0.4 + 0.25), 0.04, 0.06));
        // Too high above the keys: pointing at a screen, not typing.
        assert!(!kb.is_over(Vec3::new(0.0, 0.90, -0.4), 0.0, 0.06));
    }

    #[test]
    fn suppression_is_per_hand_and_holds_briefly() {
        let kb = keyboard();
        let mut s = TypingSuppressor::default();
        let t0 = Instant::now();
        let on_keys = [Vec3::new(0.0, 0.76, -0.4)];
        let away = [Vec3::new(0.5, 1.2, -0.3)];

        assert!(s.update(Hand::Right, Some(&kb), &on_keys, t0));
        assert!(!s.update(Hand::Left, Some(&kb), &away, t0));
        // Hand lifted: still suppressed within the hold time, released after.
        assert!(s.update(
            Hand::Right,
            Some(&kb),
            &away,
            t0 + Duration::from_millis(200)
        ));
        assert!(!s.update(
            Hand::Right,
            Some(&kb),
            &away,
            t0 + Duration::from_millis(600)
        ));
    }

    #[test]
    fn losing_keyboard_tracking_releases_suppression() {
        let kb = keyboard();
        let mut s = TypingSuppressor::default();
        let t0 = Instant::now();
        assert!(s.update(Hand::Left, Some(&kb), &[Vec3::new(0.0, 0.76, -0.4)], t0));
        assert!(!s.update(Hand::Left, None, &[], t0));
        assert!(!s.is_suppressed(Hand::Left, t0));
    }
}
