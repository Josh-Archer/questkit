//! Spatial screen placement for XR apps: 6DoF screen transforms, pointing rays and hit testing,
//! grab/resize/scroll gestures, desk and keyboard detection.
//!
//! It is plain Rust math and state: no runtime, no graphics and no device APIs. Enable the
//! `openxr` feature to convert to and from [`openxr`] pose types and to build OpenXR composition
//! layer parameters from a [`ScreenTransform`].

use serde::{Deserialize, Serialize};

pub mod desk;
pub mod grab;
pub mod interaction;
pub mod keyboard;
pub mod keyboard_summon;
pub mod limits;
pub mod pose;
pub mod ray;
pub mod scroll;
pub mod transform;

#[cfg(feature = "openxr")]
mod layer_config;

/// Virtual display projection mode in XR space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    /// Flat virtual monitor quad (`XrCompositionLayerQuad`).
    FlatQuad,
    /// Curved virtual monitor wrapping user field of view (`XrCompositionLayerCylinderKHR`).
    CurvedCylinder,
}

pub use desk::{DeskDetector, DeskSurface, SurfaceTouchpad, TouchpadEvent, DESK_SCREEN_GAP_M};
pub use grab::{
    corner_at, Corner, GrabMode, GrabSession, GrabStateMachine, InteractionEvent, InteractionState,
    TwoHandScale, CORNER_ZONE,
};
pub use interaction::{
    ControllerInputState, HandAimState, HandGestureDetector, HandGestureState, HandJoint,
    HandJointData, HandTrackingData, Handedness, InteractionInput, InteractionSource,
    DEFAULT_GRAB_RELEASE_THRESHOLD_M, DEFAULT_GRAB_START_THRESHOLD_M,
    DEFAULT_PINCH_RELEASE_THRESHOLD_M, DEFAULT_PINCH_START_THRESHOLD_M, HAND_JOINT_COUNT,
};
pub use keyboard::{Hand, TrackedKeyboard, TypingSuppressor};
pub use keyboard_summon::KeyboardSummon;
#[cfg(feature = "openxr")]
pub use layer_config::DesktopLayerConfig;
pub use limits::*;
pub use pose::{Quat, ScreenPose, Vec3};
pub use ray::{ray_intersects_cylinder, ray_intersects_quad, Ray, RayHit};
pub use transform::ScreenTransform;
#[cfg(feature = "openxr")]
pub use transform::{CylinderLayerParams, QuadLayerParams};
