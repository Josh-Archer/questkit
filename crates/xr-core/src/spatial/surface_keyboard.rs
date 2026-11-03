//! Virtual QWERTY keyboard laid flat on the desk, typed with fingertip contact.
//!
//! Pure logic, no OpenXR:
//! - [`SurfaceKeyboard::place`] fixes the keyboard on the desk in front of the calibrated
//!   palm position (beyond the touchpad, away from the user). It is not grabbable.
//! - [`SurfaceKeyboard::key_at`] hit-tests a world point against the key grid.
//! - [`KeyPresser`] turns fingertip heights into evdev key down/up, with press/release
//!   hysteresis so a resting finger does not chatter.
//! - [`NearDeskVisibility`] shows the keyboard only while a hand is near the desk surface,
//!   with show/hide hysteresis.
//!
//! Like the touchpad, the keyboard is axis-aligned in world X/Z with the user facing -Z.

use super::desk::{DeskSurface, TOUCHPAD_DEPTH_M};
use super::pose::{Quat, ScreenPose, Vec3};

/// Center-to-center key spacing, in meters (a standard 19 mm keyboard pitch).
pub const KEY_PITCH_M: f32 = 0.019;
/// Gap between the far edge of the touchpad and the near edge of the keyboard, in meters.
pub const KEYBOARD_GAP_BEYOND_TOUCHPAD_M: f32 = 0.02;
/// A fingertip this close to the desk over a key presses it, in meters.
pub const KEY_PRESS_HEIGHT_M: f32 = 0.008;
/// A pressed key releases once its fingertip rises above this, in meters.
pub const KEY_RELEASE_HEIGHT_M: f32 = 0.015;
/// Show the keyboard when any hand point is within this height of the desk, in meters.
pub const KEYBOARD_SHOW_HEIGHT_M: f32 = 0.10;
/// Hide it again only once every hand point is above this height (hysteresis), in meters.
pub const KEYBOARD_HIDE_HEIGHT_M: f32 = 0.16;
/// Hand points further than this horizontally from the keyboard do not count as near.
pub const KEYBOARD_SHOW_REACH_M: f32 = 0.35;
/// Extra horizontal reach allowed before hiding (hysteresis), in meters.
pub const KEYBOARD_HIDE_REACH_EXTRA_M: f32 = 0.10;
/// The drawn keyboard floats this far above the desk to avoid z-fighting, in meters.
pub const KEYBOARD_VISUAL_LIFT_M: f32 = 0.002;

/// evdev key codes used by the layout (linux/input-event-codes.h).
mod ev {
    pub const SHIFT: u32 = 42;
    pub const BACKSPACE: u32 = 14;
    pub const ENTER: u32 = 28;
    pub const SPACE: u32 = 57;
}

/// One key: evdev code, label and width in key units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyDef {
    pub code: u32,
    pub label: &'static str,
    pub units: f32,
}
const fn k(code: u32, label: &'static str) -> KeyDef {
    KeyDef {
        code,
        label,
        units: 1.0,
    }
}
const fn wide(code: u32, label: &'static str, units: f32) -> KeyDef {
    KeyDef { code, label, units }
}

/// Rows from the far edge (away from the user) to the near edge.
pub const LAYOUT: [&[KeyDef]; 5] = [
    &[
        k(2, "1"),
        k(3, "2"),
        k(4, "3"),
        k(5, "4"),
        k(6, "5"),
        k(7, "6"),
        k(8, "7"),
        k(9, "8"),
        k(10, "9"),
        k(11, "0"),
        k(12, "-"),
        wide(ev::BACKSPACE, "<-", 1.5),
    ],
    &[
        k(16, "Q"),
        k(17, "W"),
        k(18, "E"),
        k(19, "R"),
        k(20, "T"),
        k(21, "Y"),
        k(22, "U"),
        k(23, "I"),
        k(24, "O"),
        k(25, "P"),
    ],
    &[
        k(30, "A"),
        k(31, "S"),
        k(32, "D"),
        k(33, "F"),
        k(34, "G"),
        k(35, "H"),
        k(36, "J"),
        k(37, "K"),
        k(38, "L"),
        wide(ev::ENTER, "ENT", 1.5),
    ],
    &[
        wide(ev::SHIFT, "SHF", 1.5),
        k(44, "Z"),
        k(45, "X"),
        k(46, "C"),
        k(47, "V"),
        k(48, "B"),
        k(49, "N"),
        k(50, "M"),
        k(51, ","),
        k(52, "."),
    ],
    &[wide(ev::SPACE, "", 6.0)],
];

fn row_units(row: &[KeyDef]) -> f32 {
    row.iter().map(|key| key.units).sum()
}

/// Keyboard width in meters (the widest row).
pub fn keyboard_width_m() -> f32 {
    LAYOUT.iter().map(|r| row_units(r)).fold(0.0, f32::max) * KEY_PITCH_M
}
/// Keyboard depth in meters.
pub fn keyboard_depth_m() -> f32 {
    LAYOUT.len() as f32 * KEY_PITCH_M
}

/// A keyboard fixed on the desk. `center` is on the desk surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceKeyboard {
    pub desk: DeskSurface,
    pub center: Vec3,
}

impl SurfaceKeyboard {
    /// Places the keyboard in front of the calibrated palm position (the desk center), past
    /// the touchpad region and centred on it in X.
    pub fn place(desk: DeskSurface) -> Self {
        let offset =
            TOUCHPAD_DEPTH_M / 2.0 + KEYBOARD_GAP_BEYOND_TOUCHPAD_M + keyboard_depth_m() / 2.0;
        Self {
            desk,
            center: Vec3::new(desk.center.x, desk.center.y, desk.center.z - offset),
        }
    }

    /// Keyboard-local (x right, z toward the user) offset of `p` from the keyboard center.
    fn local(&self, p: Vec3) -> (f32, f32) {
        (p.x - self.center.x, p.z - self.center.z)
    }

    /// The key under `p` (height ignored), if any.
    pub fn key_at(&self, p: Vec3) -> Option<&'static KeyDef> {
        let (x, z) = self.local(p);
        let depth = keyboard_depth_m();
        let from_far = z + depth / 2.0;
        if !(0.0..depth).contains(&from_far) {
            return None;
        }
        let row = LAYOUT[(from_far / KEY_PITCH_M) as usize];
        let mut left = -row_units(row) * KEY_PITCH_M / 2.0;
        row.iter().find(|key| {
            let right = left + key.units * KEY_PITCH_M;
            let hit = (left..right).contains(&x);
            left = right;
            hit
        })
    }

    /// Horizontal distance from `p` to the keyboard rectangle (0 when over it).
    pub fn horizontal_distance(&self, p: Vec3) -> f32 {
        let (x, z) = self.local(p);
        let dx = (x.abs() - keyboard_width_m() / 2.0).max(0.0);
        let dz = (z.abs() - keyboard_depth_m() / 2.0).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }

    /// Pose of the drawn keyboard quad: lying flat on the desk, facing up, its top edge
    /// away from the user. A quad faces its local +Z, so tilt it -90 degrees about X.
    pub fn visual_pose(&self) -> ScreenPose {
        ScreenPose::new(
            self.center + Vec3::new(0.0, KEYBOARD_VISUAL_LIFT_M, 0.0),
            Quat::from_axis_angle(Vec3::UNIT_X, -std::f32::consts::FRAC_PI_2),
        )
    }

    /// Size of the drawn quad, in meters.
    pub fn visual_size(&self) -> (f32, f32) {
        (keyboard_width_m(), keyboard_depth_m())
    }
}

/// A key press or release, as an evdev code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChange {
    pub code: u32,
    pub pressed: bool,
}

/// Number of fingertip slots tracked (5 per hand).
pub const FINGERTIP_SLOTS: usize = 10;

/// Per-fingertip key state with press/release hysteresis.
#[derive(Debug, Clone, Default)]
pub struct KeyPresser {
    held: [Option<u32>; FINGERTIP_SLOTS],
}

impl KeyPresser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keys currently held down.
    pub fn held(&self) -> impl Iterator<Item = u32> + '_ {
        self.held.iter().flatten().copied()
    }

    /// Releases every held key (for example when the keyboard hides).
    pub fn release_all(&mut self) -> Vec<KeyChange> {
        self.held
            .iter_mut()
            .filter_map(|slot| slot.take())
            .map(|code| KeyChange {
                code,
                pressed: false,
            })
            .collect()
    }

    /// Feeds one frame of fingertips, one slot per finger (`None` when untracked).
    /// A finger presses the key under it when it comes within [`KEY_PRESS_HEIGHT_M`] of the
    /// desk; it keeps that key until it rises above [`KEY_RELEASE_HEIGHT_M`] or is lost.
    /// A key already held by another finger is not pressed twice.
    pub fn update(
        &mut self,
        keyboard: &SurfaceKeyboard,
        tips: &[Option<Vec3>; FINGERTIP_SLOTS],
    ) -> Vec<KeyChange> {
        let mut changes = Vec::new();
        for (i, &tip) in tips.iter().enumerate() {
            let height = tip.map(|t| keyboard.desk.height_above(t));
            if let Some(code) = self.held[i] {
                if height.is_none_or(|h| h > KEY_RELEASE_HEIGHT_M) {
                    self.held[i] = None;
                    changes.push(KeyChange {
                        code,
                        pressed: false,
                    });
                }
                continue;
            }
            let (Some(tip), Some(h)) = (tip, height) else {
                continue;
            };
            if h > KEY_PRESS_HEIGHT_M {
                continue;
            }
            if let Some(key) = keyboard.key_at(tip) {
                if !self.held.contains(&Some(key.code)) {
                    self.held[i] = Some(key.code);
                    changes.push(KeyChange {
                        code: key.code,
                        pressed: true,
                    });
                }
            }
        }
        changes
    }
}

/// Shows the keyboard while a hand is near the desk, with hysteresis on height and reach.
#[derive(Debug, Clone, Default)]
pub struct NearDeskVisibility {
    visible: bool,
}

impl NearDeskVisibility {
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Feeds tracked hand points (palms and fingertips). Becomes visible when any point is
    /// within [`KEYBOARD_SHOW_HEIGHT_M`] above the desk and [`KEYBOARD_SHOW_REACH_M`] of the
    /// keyboard; hides only when none is within the larger hide limits.
    pub fn update(&mut self, keyboard: &SurfaceKeyboard, points: &[Vec3]) -> bool {
        let (max_h, reach) = if self.visible {
            (
                KEYBOARD_HIDE_HEIGHT_M,
                KEYBOARD_SHOW_REACH_M + KEYBOARD_HIDE_REACH_EXTRA_M,
            )
        } else {
            (KEYBOARD_SHOW_HEIGHT_M, KEYBOARD_SHOW_REACH_M)
        };
        self.visible = points.iter().any(|&p| {
            let h = keyboard.desk.height_above(p);
            (-0.05..=max_h).contains(&h) && keyboard.horizontal_distance(p) <= reach
        });
        self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb() -> SurfaceKeyboard {
        SurfaceKeyboard::place(DeskSurface {
            center: Vec3::new(0.0, 0.75, -0.4),
        })
    }
    fn on(kb: &SurfaceKeyboard, code: u32, h: f32) -> Vec3 {
        // Find the key's center by scanning.
        let depth = keyboard_depth_m();
        let width = keyboard_width_m();
        for zi in 0..100 {
            for xi in 0..200 {
                let p = Vec3::new(
                    kb.center.x - width / 2.0 + width * (xi as f32 + 0.5) / 200.0,
                    kb.desk.center.y + h,
                    kb.center.z - depth / 2.0 + depth * (zi as f32 + 0.5) / 100.0,
                );
                if kb.key_at(p).map(|k| k.code) == Some(code) {
                    return p;
                }
            }
        }
        panic!("key {code} not found");
    }

    #[test]
    fn keyboard_sits_in_front_of_the_palms_beyond_the_touchpad() {
        let kb = kb();
        assert_eq!(kb.center.x, kb.desk.center.x);
        assert_eq!(kb.center.y, kb.desk.center.y);
        let near_edge = kb.center.z + keyboard_depth_m() / 2.0;
        let pad_far_edge = kb.desk.center.z - TOUCHPAD_DEPTH_M / 2.0;
        assert!((pad_far_edge - near_edge - KEYBOARD_GAP_BEYOND_TOUCHPAD_M).abs() < 1e-5);
        // Facing up.
        let n = kb.visual_pose().orientation.rotate_vec(Vec3::UNIT_Z);
        assert!((n - Vec3::UNIT_Y).length() < 1e-5);
    }

    #[test]
    fn key_grid_hit_testing() {
        let kb = kb();
        // Far row is digits, near row is space.
        let far = Vec3::new(
            kb.center.x,
            0.75,
            kb.center.z - keyboard_depth_m() / 2.0 + 0.001,
        );
        assert!(matches!(kb.key_at(far).map(|k| k.code), Some(2..=14)));
        let near = Vec3::new(
            kb.center.x,
            0.75,
            kb.center.z + keyboard_depth_m() / 2.0 - 0.001,
        );
        assert_eq!(kb.key_at(near).map(|k| k.code), Some(ev::SPACE));
        assert!(kb
            .key_at(Vec3::new(kb.center.x, 0.75, kb.center.z + 0.2))
            .is_none());
        assert!(kb
            .key_at(Vec3::new(kb.center.x + 0.5, 0.75, kb.center.z))
            .is_none());
        let a = on(&kb, 30, 0.0);
        assert_eq!(kb.key_at(a).unwrap().label, "A");
    }

    #[test]
    fn fingertip_press_and_release_have_hysteresis() {
        let kb = kb();
        let mut keys = KeyPresser::new();
        let mut tips = [None; FINGERTIP_SLOTS];
        tips[1] = Some(on(&kb, 30, 0.03));
        assert!(
            keys.update(&kb, &tips).is_empty(),
            "hovering is not a press"
        );
        tips[1] = Some(on(&kb, 30, 0.005));
        assert_eq!(
            keys.update(&kb, &tips),
            vec![KeyChange {
                code: 30,
                pressed: true
            }]
        );
        // Between press and release heights: still held, no chatter.
        tips[1] = Some(on(&kb, 30, 0.012));
        assert!(keys.update(&kb, &tips).is_empty());
        tips[1] = Some(on(&kb, 30, 0.004));
        assert!(keys.update(&kb, &tips).is_empty());
        // A second finger on the same key does not double-press it.
        tips[2] = Some(on(&kb, 30, 0.0));
        assert!(keys.update(&kb, &tips).is_empty());
        tips[2] = None;
        tips[1] = Some(on(&kb, 30, 0.02));
        assert_eq!(
            keys.update(&kb, &tips),
            vec![KeyChange {
                code: 30,
                pressed: false
            }]
        );
        // Losing tracking releases the key.
        tips[3] = Some(on(&kb, 44, 0.0));
        assert_eq!(
            keys.update(&kb, &tips),
            vec![KeyChange {
                code: 44,
                pressed: true
            }]
        );
        tips[3] = None;
        assert_eq!(
            keys.update(&kb, &tips),
            vec![KeyChange {
                code: 44,
                pressed: false
            }]
        );
        tips[4] = Some(on(&kb, ev::SHIFT, 0.0));
        keys.update(&kb, &tips);
        assert_eq!(
            keys.release_all(),
            vec![KeyChange {
                code: ev::SHIFT,
                pressed: false
            }]
        );
        assert_eq!(keys.held().count(), 0);
    }

    #[test]
    fn visibility_follows_hands_near_the_desk_with_hysteresis() {
        let kb = kb();
        let mut vis = NearDeskVisibility::default();
        let at = |h: f32| [Vec3::new(kb.center.x, kb.desk.center.y + h, kb.center.z)];
        assert!(!vis.update(&kb, &[]));
        assert!(!vis.update(&kb, &at(0.13)), "too high to show");
        assert!(vis.update(&kb, &at(0.08)));
        assert!(vis.update(&kb, &at(0.13)), "still inside the hide limit");
        assert!(!vis.update(&kb, &at(0.20)));
        // Far to the side: not near the keyboard.
        let side = [Vec3::new(
            kb.center.x + 0.8,
            kb.desk.center.y + 0.02,
            kb.center.z,
        )];
        assert!(!vis.update(&kb, &side));
        // Below the desk (hand under the table) does not count.
        assert!(!vis.update(&kb, &at(-0.2)));
        // Lost hands hide it.
        vis.update(&kb, &at(0.0));
        assert!(!vis.update(&kb, &[]));
    }
}
