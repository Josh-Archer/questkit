//! OpenXR configuration independent of app networking and settings.
use serde::{Deserialize, Serialize};
/// Virtual display projection mode in OpenXR space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    /// Flat virtual monitor quad (`XrCompositionLayerQuad`).
    FlatQuad,
    /// Curved virtual monitor wrapping user field of view (`XrCompositionLayerCylinderKHR`).
    CurvedCylinder,
}

/// Runtime and composition settings. Applications own persisted settings and validation.
#[derive(Debug, Clone)]
pub struct XrConfig {
    pub application_name: std::borrow::Cow<'static, str>,
    pub target_refresh_rate: f32,
    pub stream_width: u32,
    pub stream_height: u32,
    /// Permit ordinary swapchains when Android surface decoding is not required.
    pub force_mock_decoder: bool,
    pub enable_passthrough: bool,
    pub display_mode: DisplayMode,
    pub quad_size: (f32, f32),
    pub quad_distance: f32,
    pub cylinder_radius: f32,
    pub cylinder_central_angle: f32,
    pub cylinder_aspect_ratio: f32,
    /// Enable the view-space RGBA overlay supplied by the application.
    pub enable_hud: bool,
}
impl Default for XrConfig {
    fn default() -> Self {
        Self {
            application_name: "XR Application".into(),
            target_refresh_rate: 90.0,
            stream_width: 1920,
            stream_height: 1080,
            force_mock_decoder: !cfg!(target_os = "android"),
            enable_passthrough: true,
            display_mode: DisplayMode::CurvedCylinder,
            quad_size: (1.6, 0.9),
            quad_distance: 1.2,
            cylinder_radius: crate::spatial::limits::DEFAULT_CURVE_RADIUS_M,
            cylinder_central_angle: crate::spatial::limits::DEFAULT_CURVE_ANGLE_RAD,
            cylinder_aspect_ratio: 16.0 / 9.0,
            enable_hud: false,
        }
    }
}
