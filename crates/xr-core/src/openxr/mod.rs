//! OpenXR runtime integration for Meta Quest headsets.

pub mod context;
mod dynamic_keyboard;
pub mod frame_loop;
pub mod hand_tracking;
pub mod interaction;
mod keyboard;
pub mod layers;
pub mod overlay;
pub mod passthrough;
pub mod refresh_rate;

pub use context::OpenXrContext;
pub use frame_loop::{FrameLoopEngine, FramePacingMetrics};
pub use hand_tracking::{HandTrackingBackend, MockHandTrackingBackend, OpenXrHandTrackingBackend};
pub use interaction::{InteractionBackend, MockInteractionBackend, OpenXrInteractionBackend};
pub use layers::{
    build_cylinder_layer, build_cylinder_layer_from_transform, build_hud_layer,
    build_passthrough_layer, build_quad_layer, build_quad_layer_from_transform, DesktopLayerConfig,
};
pub use passthrough::{
    CompositionLayerPassthroughFB, MockPassthroughBackend, OpenXrPassthroughBackend,
    PassthroughBackend, PassthroughState,
};
pub use refresh_rate::{RefreshRateManager, QUEST_REFRESH_RATES};

pub mod equirect;
pub use equirect::{build_equirect_layer, EquirectCoverage, EquirectLayerConfig};
