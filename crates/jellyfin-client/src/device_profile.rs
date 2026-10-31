//! Meta Quest 3+ device profile for Jellyfin direct play negotiation.

use crate::library::{BaseItem, MediaStreamType};
use serde::{Deserialize, Serialize};

/// Condition for codec limitations (resolution, framerate, bit depth).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProfileCondition {
    /// Condition operator (e.g. "LessThanEqual", "Equals").
    pub condition: String,
    /// Target property name (e.g. "Width", "Height", "VideoFramerate").
    pub property: String,
    /// Value threshold as string.
    pub value: String,
    /// Whether this condition is mandatory.
    pub is_required: bool,
}

/// Codec capability profile in Jellyfin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CodecProfile {
    /// Target media stream type ("Video", "Audio").
    #[serde(rename = "Type")]
    pub profile_type: String,
    /// Codec name (e.g. "hevc", "av1", "h264").
    pub codec: String,
    /// Rules defining supported resolutions and framerates.
    #[serde(default)]
    pub conditions: Vec<ProfileCondition>,
}

/// Direct play profile specifying allowed containers and codecs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectPlayProfile {
    /// Comma-separated list of container extensions (e.g. "mkv,mp4,webm").
    pub container: String,
    /// Media type ("Video", "Audio").
    #[serde(rename = "Type")]
    pub profile_type: String,
    /// Optional comma-separated video codecs.
    pub video_codec: Option<String>,
    /// Optional comma-separated audio codecs.
    pub audio_codec: Option<String>,
}

/// A server-side streaming fallback supported by the Android media player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TranscodingProfile {
    pub container: String,
    #[serde(rename = "Type")]
    pub profile_type: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub protocol: String,
    pub context: String,
}

/// Device playback profile sent to Jellyfin PlaybackInfo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProfile {
    /// Friendly name of device profile.
    pub name: String,
    /// Maximum streaming bitrate in bps (120 Mbps).
    pub max_streaming_bitrate: u64,
    /// Maximum static stream bitrate in bps (120 Mbps).
    pub max_static_bitrate: u64,
    /// Maximum audio bitrate in bps.
    pub music_streaming_transcoding_bitrate: u64,
    /// Direct play format specifications.
    #[serde(default)]
    pub direct_play_profiles: Vec<DirectPlayProfile>,
    /// Streaming fallbacks the server can use when direct play is unavailable.
    #[serde(default)]
    pub transcoding_profiles: Vec<TranscodingProfile>,
    /// Fine-grained codec constraints (8K30 HEVC cap, AV1, AVC).
    #[serde(default)]
    pub codec_profiles: Vec<CodecProfile>,
}

/// Generates the standard Meta Quest 3+ device profile.
///
/// Hardware specifications:
/// - Snapdragon XR2 Gen 2 GPU/VPU
/// - HEVC Main/Main10 direct play up to 8192x4320@30fps (~8K30) or 4096x4096@60fps
/// - AV1 Main direct play up to 8192x4320@30fps / 3840x2160@60fps
/// - AVC/H.264 up to 4096x4096@60fps
/// - Audio: Opus, AAC, FLAC, AC-3, E-AC-3, Vorbis, PCM
/// - Containers: MKV, MP4, M4V, MOV, WebM, TS
pub fn quest_device_profile() -> DeviceProfile {
    DeviceProfile {
        name: "Meta Quest 3".to_string(),
        max_streaming_bitrate: 120_000_000,
        max_static_bitrate: 120_000_000,
        music_streaming_transcoding_bitrate: 320_000,
        direct_play_profiles: vec![
            DirectPlayProfile {
                container: "mkv,mp4,m4v,mov,webm,ts".to_string(),
                profile_type: "Video".to_string(),
                video_codec: Some("hevc,h265,av1,avc,h264,vp9".to_string()),
                audio_codec: Some("aac,mp3,opus,flac,ac3,eac3,dts,vorbis,pcm".to_string()),
            },
            DirectPlayProfile {
                container: "mp3,flac,aac,ogg,m4a,opus,wav".to_string(),
                profile_type: "Audio".to_string(),
                video_codec: None,
                audio_codec: None,
            },
        ],
        transcoding_profiles: vec![TranscodingProfile {
            container: "ts".into(),
            profile_type: "Video".into(),
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            protocol: "hls".into(),
            context: "Streaming".into(),
        }],
        codec_profiles: vec![
            CodecProfile {
                profile_type: "Video".to_string(),
                codec: "hevc".to_string(),
                conditions: vec![
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Width".to_string(),
                        value: "8192".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Height".to_string(),
                        value: "4320".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "VideoFramerate".to_string(),
                        value: "30".to_string(),
                        is_required: false,
                    },
                ],
            },
            CodecProfile {
                profile_type: "Video".to_string(),
                codec: "av1".to_string(),
                conditions: vec![
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Width".to_string(),
                        value: "8192".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Height".to_string(),
                        value: "4320".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "VideoFramerate".to_string(),
                        value: "30".to_string(),
                        is_required: false,
                    },
                ],
            },
            CodecProfile {
                profile_type: "Video".to_string(),
                codec: "h264".to_string(),
                conditions: vec![
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Width".to_string(),
                        value: "4096".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "Height".to_string(),
                        value: "4096".to_string(),
                        is_required: true,
                    },
                    ProfileCondition {
                        condition: "LessThanEqual".to_string(),
                        property: "VideoFramerate".to_string(),
                        value: "60".to_string(),
                        is_required: false,
                    },
                ],
            },
        ],
    }
}

/// Result of evaluating an item against the device profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectPlayDecision {
    /// Hardware can decode and play the stream directly.
    DirectPlay,
    /// Transcoding is needed because the media exceeds hardware caps or has unsupported codecs.
    TranscodeRequired { reason: String },
}

/// Evaluates whether a given media item can be directly played on Quest 3.
pub fn evaluate_direct_play(profile: &DeviceProfile, item: &BaseItem) -> DirectPlayDecision {
    // Check container
    let container = item
        .container
        .as_deref()
        .or_else(|| item.path.as_deref().and_then(|p| p.rsplit('.').next()))
        .unwrap_or("")
        .to_lowercase();

    let video_profile = profile
        .direct_play_profiles
        .iter()
        .find(|p| p.profile_type == "Video");

    if let Some(vp) = video_profile {
        let allowed_containers: Vec<&str> = vp.container.split(',').map(|s| s.trim()).collect();
        if !container.is_empty() && !allowed_containers.contains(&container.as_str()) {
            return DirectPlayDecision::TranscodeRequired {
                reason: format!("Container '{}' not supported for direct play", container),
            };
        }
    }

    // Check video stream
    let video_stream = item
        .media_streams
        .iter()
        .find(|s| s.stream_type == Some(MediaStreamType::Video));

    if let Some(vs) = video_stream {
        if let Some(codec) = &vs.codec {
            let codec_lower = codec.to_lowercase();
            // Check codec supported
            let supported = matches!(
                codec_lower.as_str(),
                "hevc" | "h265" | "av1" | "h264" | "avc" | "vp9"
            );
            if !supported {
                return DirectPlayDecision::TranscodeRequired {
                    reason: format!("Video codec '{}' not supported for direct play", codec),
                };
            }

            // Check resolution caps
            let width = vs.width.unwrap_or(0);
            let height = vs.height.unwrap_or(0);
            let framerate = vs.real_frame_rate.or(vs.average_frame_rate).unwrap_or(0.0);

            if codec_lower.contains("hevc")
                || codec_lower.contains("h265")
                || codec_lower.contains("av1")
            {
                if width > 8192 || height > 4320 {
                    return DirectPlayDecision::TranscodeRequired {
                        reason: format!(
                            "Resolution {}x{} exceeds Quest 3 8K limit (8192x4320)",
                            width, height
                        ),
                    };
                }
                // Cap around 8K30: if both dimensions are > 4K and framerate > 35fps
                if (width > 4096 || height > 2160) && framerate > 35.0 {
                    return DirectPlayDecision::TranscodeRequired {
                        reason: format!(
                            "Framerate {:.1}fps at {}x{} exceeds Quest 3 8K30 hardware cap",
                            framerate, width, height
                        ),
                    };
                }
            } else if (codec_lower.contains("h264") || codec_lower.contains("avc"))
                && (width > 4096 || height > 4096)
            {
                return DirectPlayDecision::TranscodeRequired {
                    reason: format!(
                        "H.264 resolution {}x{} exceeds 4K hardware limit (4096x4096)",
                        width, height
                    ),
                };
            }
        }
    }

    DirectPlayDecision::DirectPlay
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::MediaStream;

    #[test]
    fn test_quest_profile_hevc_cap() {
        let profile = quest_device_profile();
        let hevc = profile
            .codec_profiles
            .iter()
            .find(|c| c.codec == "hevc")
            .unwrap();
        assert_eq!(hevc.conditions.len(), 3);
        assert_eq!(hevc.conditions[0].value, "8192");
        assert_eq!(hevc.conditions[1].value, "4320");
        assert_eq!(hevc.conditions[2].value, "30");
    }

    #[test]
    fn test_evaluate_direct_play_valid_8k30() {
        let profile = quest_device_profile();
        let item = BaseItem {
            id: "1".to_string(),
            name: "8K VR Video".to_string(),
            path: Some("/media/8k.mkv".to_string()),
            video_3d_format: None,
            media_streams: vec![MediaStream {
                codec: Some("hevc".to_string()),
                stream_type: Some(MediaStreamType::Video),
                width: Some(7680),
                height: Some(3840),
                real_frame_rate: Some(29.97),
                ..Default::default()
            }],
            container: Some("mkv".to_string()),
            run_time_ticks: None,
            item_type: Some("Movie".to_string()),
            ..Default::default()
        };

        let decision = evaluate_direct_play(&profile, &item);
        assert_eq!(decision, DirectPlayDecision::DirectPlay);
    }

    #[test]
    fn test_evaluate_direct_play_exceeds_8k30_framerate() {
        let profile = quest_device_profile();
        let item = BaseItem {
            id: "1".to_string(),
            name: "8K 60fps Video".to_string(),
            path: Some("/media/8k60.mkv".to_string()),
            video_3d_format: None,
            media_streams: vec![MediaStream {
                codec: Some("hevc".to_string()),
                stream_type: Some(MediaStreamType::Video),
                width: Some(7680),
                height: Some(3840),
                real_frame_rate: Some(60.0),
                ..Default::default()
            }],
            container: Some("mkv".to_string()),
            run_time_ticks: None,
            item_type: Some("Movie".to_string()),
            ..Default::default()
        };

        let decision = evaluate_direct_play(&profile, &item);
        assert!(matches!(
            decision,
            DirectPlayDecision::TranscodeRequired { .. }
        ));
    }
}
