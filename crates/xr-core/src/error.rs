//! XR and placement operation errors.
use thiserror::Error;
pub type XrResult<T> = Result<T, XrError>;
#[derive(Debug, Error)]
pub enum XrError {
    #[error("Vulkan error: {0}")]
    Vulkan(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("OpenXR error: {0}")]
    OpenXr(String),
    #[error("Spatial error: {0}")]
    Spatial(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Media(#[from] xr_video::error::MediaError),
}
impl From<serde_json::Error> for XrError {
    fn from(value: serde_json::Error) -> Self {
        Self::Spatial(format!("JSON serialization error: {value}"))
    }
}
impl From<openxr::sys::Result> for XrError {
    fn from(value: openxr::sys::Result) -> Self {
        Self::OpenXr(format!("OpenXR error code: {value:?}"))
    }
}

impl From<ash::vk::Result> for XrError {
    fn from(value: ash::vk::Result) -> Self {
        Self::Vulkan(format!("Vulkan error code: {value:?}"))
    }
}
