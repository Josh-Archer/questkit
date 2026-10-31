//! PlaybackInfo request and response models, direct play URL formatting.

use crate::device_profile::DeviceProfile;
use crate::library::MediaStream;
use serde::{Deserialize, Serialize};
use url::Url;

/// Request payload sent to `POST /Items/{id}/PlaybackInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoRequest {
    /// Authenticated Jellyfin User ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Optional resume timestamp in ticks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_ticks: Option<u64>,
    /// Negotiated client device profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_profile: Option<DeviceProfile>,
    /// Max allowable streaming bitrate in bps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_streaming_bitrate: Option<u64>,
    /// Whether to enable direct play if supported.
    #[serde(default = "default_true")]
    pub enable_direct_play: bool,
    /// Whether to enable direct stream if supported.
    #[serde(default = "default_true")]
    pub enable_direct_stream: bool,
    /// Whether to enable transcoding if supported.
    #[serde(default = "default_true")]
    pub enable_transcoding: bool,
}

fn default_true() -> bool {
    true
}

impl PlaybackInfoRequest {
    /// Creates a default PlaybackInfo request for a user with the Quest device profile.
    pub fn new(user_id: Option<String>, profile: DeviceProfile) -> Self {
        Self {
            user_id,
            start_time_ticks: None,
            device_profile: Some(profile),
            max_streaming_bitrate: Some(120_000_000),
            enable_direct_play: true,
            enable_direct_stream: true,
            enable_transcoding: true,
        }
    }
}

/// A media source returned by Jellyfin PlaybackInfo.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSource {
    /// Media source ID.
    pub id: String,
    /// Container format (e.g. "mkv", "mp4").
    pub container: Option<String>,
    /// Server filesystem path.
    pub path: Option<String>,
    /// Whether the Jellyfin server confirmed this source can be direct played.
    #[serde(default)]
    pub supports_direct_play: bool,
    /// Whether direct streaming (remuxing) is supported.
    #[serde(default)]
    pub supports_direct_stream: bool,
    /// Whether transcoding is supported.
    #[serde(default)]
    pub supports_transcoding: bool,
    /// Direct stream URL provided by server if direct play is not possible.
    pub direct_stream_url: Option<String>,
    /// Server-generated remux or transcoding URL.
    pub transcoding_url: Option<String>,
    /// Total bitrate in bps.
    pub bitrate: Option<u64>,
    /// List of media streams.
    #[serde(default)]
    pub media_streams: Vec<MediaStream>,
}

/// Response returned from `POST /Items/{id}/PlaybackInfo`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponse {
    /// Candidate media sources for playback.
    #[serde(default)]
    pub media_sources: Vec<MediaSource>,
    /// Unique session identifier assigned by Jellyfin for this playback attempt.
    pub play_session_id: Option<String>,
    /// Error code if playback is not allowed.
    pub error_code: Option<String>,
}

/// The server-selected stream and the method reported in Jellyfin sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPlayback {
    pub url: Url,
    pub source_id: String,
    pub method: crate::PlayMethod,
}

/// Prefer direct play, then a server-provided direct-stream or transcoding URL.
/// Refuse cross-origin URLs before a caller attaches authorization headers.
pub fn resolve_playback(
    base_url: &str,
    item_id: &str,
    info: &PlaybackInfoResponse,
) -> crate::JellyfinResult<ResolvedPlayback> {
    if let Some(source) = info.media_sources.iter().find(|s| s.supports_direct_play) {
        let mut url = build_direct_play_url(base_url, item_id, None)?;
        url.query_pairs_mut()
            .append_pair("MediaSourceId", &source.id);
        return Ok(ResolvedPlayback {
            url,
            source_id: source.id.clone(),
            method: crate::PlayMethod::DirectPlay,
        });
    }
    for method in [
        crate::PlayMethod::DirectStream,
        crate::PlayMethod::Transcode,
    ] {
        for source in &info.media_sources {
            let raw = match method {
                crate::PlayMethod::DirectStream if source.supports_direct_stream => {
                    source.direct_stream_url.as_deref()
                }
                crate::PlayMethod::Transcode if source.supports_transcoding => {
                    source.transcoding_url.as_deref()
                }
                _ => None,
            };
            if let Some(raw) = raw {
                let url = resolve_server_url(base_url, raw)?;
                return Ok(ResolvedPlayback {
                    url,
                    source_id: source.id.clone(),
                    method,
                });
            }
        }
    }
    Err(crate::Error::InvalidUrl(
        "No supported playback source".into(),
    ))
}

fn resolve_server_url(base_url: &str, raw: &str) -> crate::JellyfinResult<Url> {
    let base =
        Url::parse(base_url).map_err(|_| crate::Error::InvalidUrl("Invalid server URL".into()))?;
    let mut prefix = base.clone();
    prefix.set_path(&format!("{}/", base.path().trim_end_matches('/')));
    let url = if raw.starts_with('/') && !raw.starts_with("//") {
        let server_path = base.path().trim_end_matches('/');
        if !server_path.is_empty() && !raw.starts_with(&format!("{server_path}/")) {
            prefix.join(raw.trim_start_matches('/'))
        } else {
            base.join(raw)
        }
    } else {
        prefix.join(raw)
    }
    .map_err(|_| crate::Error::InvalidUrl("Invalid stream URL".into()))?;
    let base_path = base.path().trim_end_matches('/');
    let in_server_path = base_path.is_empty()
        || url.path() == base_path
        || url.path().starts_with(&format!("{base_path}/"));
    if url.origin() != base.origin()
        || !in_server_path
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(crate::Error::InvalidUrl(
            "Stream URL must remain on the Jellyfin server".into(),
        ));
    }
    let mut url = url;
    let params: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("api_key"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    if !params.is_empty() {
        url.query_pairs_mut().extend_pairs(params);
    }
    Ok(url)
}

/// Direct play stream URL: `{base_url}/Videos/{id}/stream?static=true[&api_key=..]`.
///
/// `base_url` is the client's normalized base URL (no trailing slash, subpath kept).
///
/// # Errors
///
/// Returns [`crate::Error::InvalidUrl`] when the result is not a valid URL.
pub fn build_direct_play_url(
    base_url: &str,
    item_id: &str,
    token: Option<&str>,
) -> crate::JellyfinResult<Url> {
    let base = base_url.trim_end_matches('/');
    let id = crate::identity::encode_component(item_id);
    let mut url = Url::parse(&format!("{base}/Videos/{id}/stream"))
        .map_err(|err| crate::Error::InvalidUrl(err.to_string()))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("static", "true");
        if let Some(tok) = token {
            query.append_pair("api_key", tok);
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_direct_play_url_without_token() {
        let url =
            build_direct_play_url("https://jellyfin.archerfamily.io", "item-12345", None).unwrap();
        assert_eq!(
            url.as_str(),
            "https://jellyfin.archerfamily.io/Videos/item-12345/stream?static=true"
        );
    }

    #[test]
    fn test_build_direct_play_url_with_token() {
        let url = build_direct_play_url(
            "https://jellyfin.archerfamily.io",
            "item-12345",
            Some("tok-abc"),
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://jellyfin.archerfamily.io/Videos/item-12345/stream?static=true&api_key=tok-abc"
        );
    }

    #[test]
    fn test_deserialize_playback_info_response() {
        let json = r#"{
            "PlaySessionId": "sess-xyz",
            "MediaSources": [
                {
                    "Id": "source-1",
                    "Container": "mkv",
                    "SupportsDirectPlay": true,
                    "SupportsDirectStream": true,
                    "SupportsTranscoding": false,
                    "Bitrate": 45000000
                }
            ]
        }"#;
        let res: PlaybackInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(res.play_session_id.as_deref(), Some("sess-xyz"));
        assert_eq!(res.media_sources.len(), 1);
        assert!(res.media_sources[0].supports_direct_play);
    }
}
