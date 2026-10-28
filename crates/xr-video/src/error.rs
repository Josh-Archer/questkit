//! Media operation errors.
use thiserror::Error;
pub type MediaResult<T> = Result<T, MediaError>;
#[derive(Debug, Error)]
pub enum MediaError {
    #[error("OpenXR error: {0}")]
    OpenXr(String),
    #[error("Vulkan error: {0}")]
    Vulkan(String),
    #[error("Decoder error: {0}")]
    Decoder(String),
}
impl From<ash::vk::Result> for MediaError {
    fn from(value: ash::vk::Result) -> Self {
        Self::Vulkan(format!("Vulkan error code: {value:?}"))
    }
}
