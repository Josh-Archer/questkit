//! Spatial screen model, 6DoF transforms, pointing rays, gestures, and layout persistence.
//!
//! The placement, gesture and hit-testing logic lives in the [`xr_spatial`] crate and is
//! re-exported here; layout persistence stays in `xr-core` because it reports [`crate::XrError`].

pub mod layout;

pub use xr_spatial::{
    desk, grab, interaction, keyboard, keyboard_summon, limits, pose, ray, scroll, transform,
};

pub use layout::{SpatialLayoutStore, SpatialScreenLayout, CURRENT_LAYOUT_VERSION};
pub use xr_spatial::limits::*;
pub use xr_spatial::{
    corner_at, ray_intersects_cylinder, ray_intersects_quad, ControllerInputState, Corner,
    CylinderLayerParams, DeskDetector, DeskSurface, GrabMode, GrabSession, GrabStateMachine, Hand,
    HandAimState, HandGestureDetector, HandGestureState, HandJoint, HandJointData,
    HandTrackingData, Handedness, InteractionEvent, InteractionInput, InteractionSource,
    InteractionState, KeyboardSummon, QuadLayerParams, Quat, Ray, RayHit, ScreenPose,
    ScreenTransform, SurfaceTouchpad, TouchpadEvent, TrackedKeyboard, TwoHandScale,
    TypingSuppressor, Vec3, CORNER_ZONE, DEFAULT_GRAB_RELEASE_THRESHOLD_M,
    DEFAULT_GRAB_START_THRESHOLD_M, DEFAULT_PINCH_RELEASE_THRESHOLD_M,
    DEFAULT_PINCH_START_THRESHOLD_M, DESK_SCREEN_GAP_M, HAND_JOINT_COUNT,
};
