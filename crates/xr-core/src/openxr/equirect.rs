//! Equirectangular layer stub for future 180/360 playback.
//!
//! The application must enable XR_KHR_composition_layer_equirect2 on its instance,
//! supply the decoded image (including the chosen stereo eye region), and submit the layer.
//! DeskQuest does not enable or submit this layer, preserving its current compositor path.
use openxr::{
    CompositionLayerEquirect2KHR, EyeVisibility, Graphics, Posef, Space, SwapchainSubImage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquirectCoverage {
    Hemisphere180,
    Sphere360,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquirectLayerConfig {
    pub coverage: EquirectCoverage,
    pub pose: Posef,
    /// Sphere radius in meters; zero means an infinitely distant sphere.
    pub radius: f32,
    pub eye_visibility: EyeVisibility,
}
impl Default for EquirectLayerConfig {
    fn default() -> Self {
        Self {
            coverage: EquirectCoverage::Sphere360,
            pose: Posef::IDENTITY,
            radius: 0.0,
            eye_visibility: EyeVisibility::BOTH,
        }
    }
}
impl EquirectCoverage {
    /// Horizontal extent, upper elevation and lower elevation, in radians.
    pub fn angles(self) -> (f32, f32, f32) {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        (
            match self {
                Self::Hemisphere180 => PI,
                Self::Sphere360 => TAU,
            },
            FRAC_PI_2,
            -FRAC_PI_2,
        )
    }
}
/// Constructs a layer without acquiring images or submitting a frame.
pub fn build_equirect_layer<'a, G: Graphics>(
    space: &'a Space,
    sub_image: SwapchainSubImage<'a, G>,
    config: &EquirectLayerConfig,
) -> CompositionLayerEquirect2KHR<'a, G> {
    let (horizontal, upper, lower) = config.coverage.angles();
    CompositionLayerEquirect2KHR::new()
        .space(space)
        .eye_visibility(config.eye_visibility)
        .sub_image(sub_image)
        .pose(config.pose)
        .radius(config.radius)
        .central_horizontal_angle(horizontal)
        .upper_vertical_angle(upper)
        .lower_vertical_angle(lower)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn covers_180_or_360_degrees_with_full_polar_extent() {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        assert_eq!(
            EquirectCoverage::Hemisphere180.angles(),
            (PI, FRAC_PI_2, -FRAC_PI_2)
        );
        assert_eq!(
            EquirectCoverage::Sphere360.angles(),
            (TAU, FRAC_PI_2, -FRAC_PI_2)
        );
        let config = EquirectLayerConfig::default();
        assert_eq!(config.radius, 0.0);
        assert_eq!(config.eye_visibility, EyeVisibility::BOTH);
    }
}
