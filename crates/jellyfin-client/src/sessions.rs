//! Jellyfin session reporting for playback start, progress, and stop.

use serde::{Deserialize, Serialize};

/// Method used to play the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayMethod {
    #[default]
    DirectPlay,
    DirectStream,
    Transcode,
}

/// Request body for reporting playback started (`POST /Sessions/Playing`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStartRequest {
    /// Jellyfin item ID being played.
    pub item_id: String,
    /// Play session identifier obtained from PlaybackInfo.
    pub play_session_id: Option<String>,
    /// Play method used.
    pub play_method: PlayMethod,
    /// Whether the client supports seeking.
    pub can_seek: bool,
    /// Whether playback is initially paused.
    pub is_paused: bool,
    /// Playback start position in Jellyfin ticks.
    pub position_ticks: u64,
    /// Selected media source ID.
    pub media_source_id: Option<String>,
    /// Selected audio stream index.
    pub audio_stream_index: Option<i32>,
    /// Selected subtitle stream index.
    pub subtitle_stream_index: Option<i32>,
}

impl PlaybackStartRequest {
    /// Creates a new direct-play start request.
    pub fn direct_play(
        item_id: impl Into<String>,
        play_session_id: Option<String>,
        position_ticks: u64,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            play_session_id,
            play_method: PlayMethod::DirectPlay,
            can_seek: true,
            is_paused: false,
            position_ticks,
            media_source_id: None,
            audio_stream_index: None,
            subtitle_stream_index: None,
        }
    }
}

/// Request body for reporting playback progress (`POST /Sessions/Playing/Progress`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackProgressRequest {
    /// Jellyfin item ID.
    pub item_id: String,
    /// Play session identifier.
    pub play_session_id: Option<String>,
    /// Play method.
    pub play_method: PlayMethod,
    /// Whether client can seek.
    pub can_seek: bool,
    /// Whether video is currently paused.
    pub is_paused: bool,
    /// Current playback position in ticks.
    pub position_ticks: u64,
    /// Event name (e.g. "TimeUpdate", "Pause", "Unpause").
    pub event_name: Option<String>,
    /// Active media source ID.
    pub media_source_id: Option<String>,
}

impl PlaybackProgressRequest {
    /// Creates a periodic time-update progress report.
    pub fn time_update(
        item_id: impl Into<String>,
        play_session_id: Option<String>,
        position_ticks: u64,
        is_paused: bool,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            play_session_id,
            play_method: PlayMethod::DirectPlay,
            can_seek: true,
            is_paused,
            position_ticks,
            event_name: Some("TimeUpdate".to_string()),
            media_source_id: None,
        }
    }
}

/// Request body for reporting playback stopped (`POST /Sessions/Playing/Stopped`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStopRequest {
    /// Jellyfin item ID.
    pub item_id: String,
    /// Play session identifier.
    pub play_session_id: Option<String>,
    /// Final position in ticks at stop.
    pub position_ticks: u64,
    /// Active media source ID.
    pub media_source_id: Option<String>,
}

impl PlaybackStopRequest {
    /// Creates a playback stopped request.
    pub fn new(
        item_id: impl Into<String>,
        play_session_id: Option<String>,
        position_ticks: u64,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            play_session_id,
            position_ticks,
            media_source_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_playback_start() {
        let req = PlaybackStartRequest::direct_play("item-1", Some("sess-1".to_string()), 0);
        let val = serde_json::to_value(&req).unwrap();
        assert_eq!(val["ItemId"], "item-1");
        assert_eq!(val["PlayMethod"], "DirectPlay");
        assert_eq!(val["CanSeek"], true);
        assert_eq!(val["IsPaused"], false);
        assert_eq!(val["PositionTicks"], 0);
    }

    #[test]
    fn test_serialize_playback_progress() {
        let req = PlaybackProgressRequest::time_update(
            "item-1",
            Some("sess-1".to_string()),
            10_000_000,
            false,
        );
        let val = serde_json::to_value(&req).unwrap();
        assert_eq!(val["EventName"], "TimeUpdate");
        assert_eq!(val["PositionTicks"], 10_000_000);
    }

    #[test]
    fn test_serialize_playback_stop() {
        let req = PlaybackStopRequest::new("item-1", Some("sess-1".to_string()), 50_000_000);
        let val = serde_json::to_value(&req).unwrap();
        assert_eq!(val["ItemId"], "item-1");
        assert_eq!(val["PositionTicks"], 50_000_000);
    }
}
