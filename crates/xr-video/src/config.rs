use serde::{Deserialize, Serialize};

/// Preferred video compression codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    /// AV1 hardware decoding (primary for Quest 3 / Snapdragon XR2 Gen 2).
    Av1,
    /// HEVC / H.265 hardware decoding (fallback).
    Hevc,
}

impl VideoCodec {
    /// Returns the Android MIME type for this codec.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Av1 => "video/av01",
            Self::Hevc => "video/hevc",
        }
    }

    /// Returns the Qualcomm Snapdragon XR2 Gen 2 low-latency decoder name.
    pub fn qti_low_latency_name(&self) -> &'static str {
        match self {
            Self::Av1 => "c2.qti.av1.decoder.low_latency",
            Self::Hevc => "c2.qti.hevc.decoder.low_latency",
        }
    }
}

/// Platform decoder selection, independent of application settings or network transport.
#[derive(Debug, Clone, Copy)]
pub struct DecoderConfig {
    pub codec: VideoCodec,
    pub force_mock_decoder: bool,
}
impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            codec: VideoCodec::Av1,
            force_mock_decoder: !cfg!(target_os = "android"),
        }
    }
}
