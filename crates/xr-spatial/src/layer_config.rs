use openxr::Posef;

use crate::DisplayMode;

/// Parameters defining the 3D spatial presentation of the virtual monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopLayerConfig {
    /// Display geometry mode (Flat or Curved).
    pub mode: DisplayMode,
    /// Physical dimensions in meters for FlatQuad mode (width, height).
    pub quad_size: (f32, f32),
    /// Distance from head origin in meters.
    pub distance: f32,
    /// Cylinder curve radius in meters.
    pub cylinder_radius: f32,
    /// Cylinder arc central angle in radians (e.g. 1.2 rad ≈ 68.75°).
    pub cylinder_central_angle: f32,
    /// Cylinder aspect ratio (width / height).
    pub cylinder_aspect_ratio: f32,
    /// Optional 6DoF pose override for custom spatial positioning.
    pub pose: Option<Posef>,
}

impl Default for DesktopLayerConfig {
    fn default() -> Self {
        Self {
            mode: DisplayMode::CurvedCylinder,
            quad_size: (1.6, 0.9),
            distance: 1.2,
            cylinder_radius: crate::limits::DEFAULT_CURVE_RADIUS_M,
            cylinder_central_angle: crate::limits::DEFAULT_CURVE_ANGLE_RAD,
            cylinder_aspect_ratio: 16.0 / 9.0,
            pose: None,
        }
    }
}
