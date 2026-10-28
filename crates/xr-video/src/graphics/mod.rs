//! Graphics subsystem providing Vulkan integration with OpenXR.

pub mod swapchain;
pub mod vulkan;

pub use swapchain::SwapchainTexture;
pub use vulkan::{ImageLayoutTransition, VulkanContext};

#[cfg(target_os = "android")]
pub mod surface;
