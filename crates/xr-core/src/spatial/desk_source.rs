//! Where the desk comes from: a Quest Space Setup TABLE plane, or palm calibration.
//!
//! Palm calibration ([`super::desk::DeskDetector`]) always wins when the user has done it,
//! because it is where the user actually put their hands. Otherwise the nearest plausible
//! TABLE plane from the headset's Space Setup is used. With neither, there is no desk.
//! Pure logic: the OpenXR scene query lives in `openxr::scene_desk`.

use super::desk::{DeskSurface, DESK_MAX_HEIGHT_M, DESK_MIN_HEIGHT_M};
use super::pose::{ScreenPose, Vec3};

/// A TABLE plane must face up within this many degrees to be used as a desk.
pub const TABLE_MAX_TILT_DEG: f32 = 15.0;
/// Smallest TABLE plane, in meters (width x depth), worth treating as a desk.
pub const TABLE_MIN_WIDTH_M: f32 = 0.30;
pub const TABLE_MIN_DEPTH_M: f32 = 0.20;
/// How far in from the table's near edge the desk center (where hands would rest) sits.
pub const TABLE_EDGE_INSET_M: f32 = 0.25;

/// Which sensor produced the desk in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeskSource {
    /// Calibrated from resting palms (this session, or restored from the saved layout).
    Palms,
    /// A TABLE plane from the headset's Space Setup.
    TablePlane,
}

/// A horizontal scene plane in stage space, in client axes: local +Y is the plane normal,
/// local X is the width axis and local Z the depth axis. `pose.position` is the plane center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TablePlane {
    pub pose: ScreenPose,
    pub width_m: f32,
    pub depth_m: f32,
}

impl TablePlane {
    fn is_usable(&self) -> bool {
        let up = self.pose.orientation.rotate_vec(Vec3::UNIT_Y);
        let finite = [self.width_m, self.depth_m, up.y, self.pose.position.y]
            .iter()
            .all(|v| v.is_finite());
        finite
            && up.y >= TABLE_MAX_TILT_DEG.to_radians().cos()
            && self.width_m >= TABLE_MIN_WIDTH_M
            && self.depth_m >= TABLE_MIN_DEPTH_M
            && (DESK_MIN_HEIGHT_M..=DESK_MAX_HEIGHT_M).contains(&self.pose.position.y)
    }

    /// The point on this table nearest the user, inset from the edges, as a desk surface.
    fn desk_near(&self, head: Vec3) -> DeskSurface {
        let inv = self.pose.orientation.conjugate();
        let local = inv.rotate_vec(head - self.pose.position);
        let clamp = |v: f32, half: f32| {
            let lim = (half - TABLE_EDGE_INSET_M).max(0.0);
            v.clamp(-lim, lim)
        };
        let on_plane = Vec3::new(
            clamp(local.x, self.width_m / 2.0),
            0.0,
            clamp(local.z, self.depth_m / 2.0),
        );
        DeskSurface {
            center: self.pose.position + self.pose.orientation.rotate_vec(on_plane),
        }
    }
}

/// Picks the TABLE plane closest to the user (horizontal distance from `head`) among the
/// usable ones, and returns the desk on it nearest the user.
pub fn desk_from_table_planes(planes: &[TablePlane], head: Vec3) -> Option<DeskSurface> {
    let horizontal = |p: Vec3| {
        let d = p - head;
        (d.x * d.x + d.z * d.z).sqrt()
    };
    planes
        .iter()
        .filter(|p| p.is_usable())
        .map(|p| p.desk_near(head))
        .min_by(|a, b| horizontal(a.center).total_cmp(&horizontal(b.center)))
}

/// Palm calibration overrides a TABLE plane; a TABLE plane is used when palms never ran.
pub fn select_desk(
    palms: Option<DeskSurface>,
    table: Option<DeskSurface>,
) -> Option<(DeskSurface, DeskSource)> {
    palms
        .map(|d| (d, DeskSource::Palms))
        .or(table.map(|d| (d, DeskSource::TablePlane)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::Quat;

    fn plane(center: Vec3, w: f32, d: f32) -> TablePlane {
        TablePlane {
            pose: ScreenPose::new(center, Quat::IDENTITY),
            width_m: w,
            depth_m: d,
        }
    }
    const HEAD: Vec3 = Vec3::new(0.0, 1.2, 0.0);

    #[test]
    fn palms_override_the_table_plane_and_the_table_is_the_fallback() {
        let palm = DeskSurface {
            center: Vec3::new(0.0, 0.74, -0.4),
        };
        let table = DeskSurface {
            center: Vec3::new(0.1, 0.72, -0.5),
        };
        assert_eq!(
            select_desk(Some(palm), Some(table)),
            Some((palm, DeskSource::Palms))
        );
        assert_eq!(
            select_desk(None, Some(table)),
            Some((table, DeskSource::TablePlane))
        );
        assert_eq!(
            select_desk(Some(palm), None),
            Some((palm, DeskSource::Palms))
        );
        assert_eq!(select_desk(None, None), None);
    }

    #[test]
    fn table_desk_sits_inset_from_the_edge_nearest_the_user() {
        // A 1.2 x 0.6 m table centred 0.6 m in front of the user.
        let t = plane(Vec3::new(0.0, 0.73, -0.6), 1.2, 0.6);
        let desk = desk_from_table_planes(&[t], HEAD).unwrap();
        assert!((desk.height() - 0.73).abs() < 1e-5);
        assert!((desk.center.x).abs() < 1e-5);
        // Near edge is z=-0.3; inset 0.25 puts the desk at z=-0.55.
        assert!((desk.center.z - (-0.3 - TABLE_EDGE_INSET_M)).abs() < 1e-5);
    }

    #[test]
    fn unusable_planes_are_ignored_and_the_nearest_table_wins() {
        let tilted = TablePlane {
            pose: ScreenPose::new(
                Vec3::new(0.0, 0.73, -0.5),
                Quat::from_axis_angle(Vec3::UNIT_X, 30f32.to_radians()),
            ),
            width_m: 1.0,
            depth_m: 0.6,
        };
        let tiny = plane(Vec3::new(0.0, 0.73, -0.4), 0.2, 0.1);
        let floor_level = plane(Vec3::new(0.0, 0.1, -0.4), 1.0, 1.0);
        let far = plane(Vec3::new(0.0, 0.75, -3.0), 1.0, 0.6);
        let near = plane(Vec3::new(0.5, 0.72, -0.8), 1.0, 0.6);
        assert_eq!(
            desk_from_table_planes(&[tilted, tiny, floor_level], HEAD),
            None
        );
        let desk = desk_from_table_planes(&[far, near], HEAD).unwrap();
        assert!((desk.height() - 0.72).abs() < 1e-5);
        assert!(desk_from_table_planes(&[], HEAD).is_none());
    }
}
