//! `GET /Items` browse types. Field names follow Jellyfin's PascalCase models.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::json::null_as_default;

/// Jellyfin ticks per second. One tick is 100 nanoseconds.
pub const TICKS_PER_SECOND: i64 = 10_000_000;

/// Fields the 3D player always requests on `GET /Items`.
pub const ITEM_FIELDS: &str = "Video3DFormat,MediaStreams,Path";

/// Stereoscopic layout Jellyfin stored on the item.
///
/// Unknown server values are kept in [`Video3DFormat::Other`] so a newer server
/// does not fail library browse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Video3DFormat {
    /// Left and right eyes packed into the left and right halves of the frame.
    HalfSideBySide,
    /// Full-resolution left and right eyes side by side.
    FullSideBySide,
    /// Full-resolution left eye above the right eye.
    FullTopAndBottom,
    /// Left and right eyes packed into the top and bottom halves of the frame.
    HalfTopAndBottom,
    /// Blu-ray MVC. Jellyfin spells this `MVC`.
    Mvc,
    /// A format string this client does not know yet.
    Other(String),
}

impl Video3DFormat {
    fn parse(value: &str) -> Self {
        match value {
            "HalfSideBySide" => Self::HalfSideBySide,
            "FullSideBySide" => Self::FullSideBySide,
            "FullTopAndBottom" => Self::FullTopAndBottom,
            "HalfTopAndBottom" => Self::HalfTopAndBottom,
            "MVC" => Self::Mvc,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl<'de> Deserialize<'de> for Video3DFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value))
    }
}

impl std::fmt::Display for Video3DFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::HalfSideBySide => "HalfSideBySide",
            Self::FullSideBySide => "FullSideBySide",
            Self::FullTopAndBottom => "FullTopAndBottom",
            Self::HalfTopAndBottom => "HalfTopAndBottom",
            Self::Mvc => "MVC",
            Self::Other(value) => value.as_str(),
        };
        formatter.write_str(label)
    }
}

/// One entry in `MediaStreams`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaStreamType {
    /// Picture track.
    Video,
    /// Audio track.
    Audio,
    /// Subtitle track.
    Subtitle,
    /// Embedded image, such as a cover attached to the file.
    EmbeddedImage,
    /// Non-media data track.
    Data,
    /// Timed lyrics.
    Lyric,
    /// A stream type string this client does not know yet.
    Other,
}

impl MediaStreamType {
    fn parse(value: &str) -> Self {
        match value {
            "Video" => Self::Video,
            "Audio" => Self::Audio,
            "Subtitle" => Self::Subtitle,
            "EmbeddedImage" => Self::EmbeddedImage,
            "Data" => Self::Data,
            "Lyric" => Self::Lyric,
            _ => Self::Other,
        }
    }
}

impl<'de> Deserialize<'de> for MediaStreamType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value))
    }
}

/// Video, audio, or subtitle stream from `MediaStreams`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct MediaStream {
    /// Stream index inside the container.
    pub index: i32,
    /// Video, audio, subtitle, or another Jellyfin stream type.
    #[serde(rename = "Type")]
    pub stream_type: Option<MediaStreamType>,
    /// Codec name, for example `hevc` or `aac`.
    pub codec: Option<String>,
    /// Language tag when Jellyfin has one.
    pub language: Option<String>,
    /// Stream title from the container.
    pub title: Option<String>,
    /// Jellyfin's display label for the stream.
    pub display_title: Option<String>,
    /// Codec profile, for example `Main 10`.
    pub profile: Option<String>,
    /// Codec level.
    pub level: Option<i32>,
    /// Encoded width in pixels.
    pub width: Option<i32>,
    /// Encoded height in pixels.
    pub height: Option<i32>,
    /// Bitrate in bits per second.
    pub bit_rate: Option<i64>,
    /// Average frame rate.
    pub average_frame_rate: Option<f64>,
    /// Real frame rate when it differs from the average.
    pub real_frame_rate: Option<f64>,
    /// SDR, HDR, or another range label.
    pub video_range: Option<String>,
    /// Bits per color sample.
    pub bit_depth: Option<i32>,
    /// Pixel format, for example `yuv420p10le`.
    pub pixel_format: Option<String>,
    /// Display aspect ratio, for example `32:9`.
    pub aspect_ratio: Option<String>,
    /// Audio channel count.
    pub channels: Option<i32>,
    /// Audio sample rate in Hz.
    pub sample_rate: Option<i32>,
    /// Channel layout name.
    pub channel_layout: Option<String>,
    /// Path for an external subtitle or stream file.
    pub path: Option<String>,
    /// Whether the video stream is interlaced.
    pub is_interlaced: Option<bool>,
    /// Whether Jellyfin marked the stream as the default.
    pub is_default: Option<bool>,
    /// Whether a subtitle is forced.
    pub is_forced: Option<bool>,
    /// Whether the stream is an external file rather than inside the container.
    pub is_external: Option<bool>,
}

/// Playback progress Jellyfin attaches when the response includes user data.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct UserData {
    /// Resume position in Jellyfin ticks. See [`TICKS_PER_SECOND`].
    #[serde(default, deserialize_with = "null_as_default")]
    pub playback_position_ticks: i64,
    /// How many times this user has played the item.
    #[serde(default, deserialize_with = "null_as_default")]
    pub play_count: i32,
    /// Whether the user marked the item as a favorite.
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_favorite: bool,
    /// Whether Jellyfin considers the item played.
    #[serde(default, deserialize_with = "null_as_default")]
    pub played: bool,
    /// When the user last played the item, as a timestamp string.
    pub last_played_date: Option<String>,
}

/// One library item from `GET /Items`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct BaseItem {
    /// Jellyfin item id.
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    /// Display name.
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    /// Server that owns the item.
    pub server_id: Option<String>,
    /// Jellyfin `Type`, for example `Movie`, `Episode`, `Series`, or `Folder`.
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
    /// `Video`, `Audio`, or another media type.
    pub media_type: Option<String>,
    /// Server filesystem path. Present because browse requests `Path`.
    pub path: Option<String>,
    /// Parent folder or library id.
    pub parent_id: Option<String>,
    /// Whether this item is a folder the player can open.
    pub is_folder: Option<bool>,
    /// Plot or description.
    pub overview: Option<String>,
    /// Release year.
    pub production_year: Option<i32>,
    /// Premiere timestamp string.
    pub premiere_date: Option<String>,
    /// Content rating, for example `PG-13`.
    pub official_rating: Option<String>,
    /// Container format, for example `mkv`.
    pub container: Option<String>,
    /// Duration in Jellyfin ticks. See [`TICKS_PER_SECOND`].
    pub run_time_ticks: Option<i64>,
    /// Episode number.
    pub index_number: Option<i32>,
    /// Season number.
    pub parent_index_number: Option<i32>,
    /// Series title for an episode.
    pub series_name: Option<String>,
    /// Series id for an episode.
    pub series_id: Option<String>,
    /// Season title.
    pub season_name: Option<String>,
    /// Season id.
    pub season_id: Option<String>,
    /// Item width when Jellyfin reports one outside the streams.
    pub width: Option<i32>,
    /// Item height when Jellyfin reports one outside the streams.
    pub height: Option<i32>,
    /// Stereoscopic layout. `None` when the file is flat or the field was null.
    #[serde(rename = "Video3DFormat")]
    pub video_3d_format: Option<Video3DFormat>,
    /// Tracks requested with the `MediaStreams` field.
    #[serde(default, deserialize_with = "null_as_default")]
    pub media_streams: Vec<MediaStream>,
    /// Primary and backdrop image tags.
    #[serde(default, deserialize_with = "null_as_default")]
    pub image_tags: BTreeMap<String, String>,
    /// Series poster tag when an episode inherits its artwork.
    pub series_primary_image_tag: Option<String>,
    /// Per-user progress when the response includes it.
    pub user_data: Option<UserData>,
}

impl BaseItem {
    /// First video stream, if `MediaStreams` contained one.
    #[must_use]
    pub fn video_stream(&self) -> Option<&MediaStream> {
        self.media_streams
            .iter()
            .find(|stream| stream.stream_type == Some(MediaStreamType::Video))
    }
}

/// One page of [`BaseItem`]s.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPage {
    /// Items in this page, in server order.
    pub items: Vec<BaseItem>,
    /// `StartIndex` the server echoed for this page.
    pub start_index: u32,
    /// Total items matching the query, not just this page.
    pub total_record_count: u64,
}

impl ItemPage {
    /// Whether another page exists after this one.
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.next_start_index().is_some()
    }

    /// `StartIndex` to request for the next page.
    ///
    /// An empty page does not advance. That stops a caller from looping if the
    /// server reports a total it will not return.
    #[must_use]
    pub fn next_start_index(&self) -> Option<u32> {
        let page_len = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        if page_len == 0 {
            return None;
        }
        let next = self.start_index.saturating_add(page_len);
        if u64::from(next) < self.total_record_count {
            Some(next)
        } else {
            None
        }
    }
}

/// Paging and filters for [`crate::JellyfinClient::list_items`].
///
/// `Fields` always includes [`ITEM_FIELDS`]. Add further Jellyfin field names in
/// [`ItemsQuery::extra_fields`]. Build queries with `..Default::default()` so
/// new filters can grow without breaking callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemsQuery {
    /// Zero-based offset. Sent as `StartIndex`.
    pub start_index: u32,
    /// Page size. Sent as `Limit`. Defaults to 100.
    pub limit: u32,
    /// Folder or library id. Sent as `ParentId` when set.
    pub parent_id: Option<String>,
    /// Overrides the signed-in user. When unset, the paired user id is sent.
    pub user_id: Option<String>,
    /// Whether to include items under `parent_id`. Sent as `Recursive`.
    pub recursive: bool,
    /// Jellyfin item types, joined into `IncludeItemTypes`.
    pub include_item_types: Vec<String>,
    /// Free-text search. Sent as `SearchTerm` when non-empty.
    pub search_term: Option<String>,
    /// Return only favorites when specified.
    pub is_favorite: Option<bool>,
    /// Server sort fields, for example SortName or DateCreated.
    pub sort_by: Option<String>,
    /// Sort in descending order.
    pub descending: bool,
    /// Extra `Fields` names appended after [`ITEM_FIELDS`].
    pub extra_fields: Vec<String>,
}

impl Default for ItemsQuery {
    fn default() -> Self {
        Self {
            start_index: 0,
            limit: 100,
            parent_id: None,
            user_id: None,
            recursive: false,
            include_item_types: Vec::new(),
            search_term: None,
            extra_fields: Vec::new(),
            is_favorite: None,
            sort_by: None,
            descending: false,
        }
    }
}

impl ItemsQuery {
    pub(crate) fn query_pairs(&self, session_user_id: Option<&str>) -> Vec<(String, String)> {
        let mut pairs = vec![
            ("StartIndex".to_owned(), self.start_index.to_string()),
            ("Limit".to_owned(), self.limit.to_string()),
            ("Fields".to_owned(), fields_param(&self.extra_fields)),
            (
                "Recursive".to_owned(),
                if self.recursive { "true" } else { "false" }.to_owned(),
            ),
        ];
        pairs.push(("EnableImages".into(), "true".into()));
        pairs.push(("EnableUserData".into(), "true".into()));
        pairs.push(("EnableImageTypes".into(), "Primary,Backdrop".into()));
        if let Some(favorite) = self.is_favorite {
            pairs.push(("IsFavorite".into(), favorite.to_string()));
        }
        if let Some(sort) = &self.sort_by {
            pairs.push(("SortBy".into(), sort.clone()));
            pairs.push((
                "SortOrder".into(),
                if self.descending {
                    "Descending"
                } else {
                    "Ascending"
                }
                .into(),
            ));
        }
        if let Some(parent) = &self.parent_id {
            pairs.push(("ParentId".to_owned(), parent.clone()));
        }
        let user_id = self
            .user_id
            .as_deref()
            .or(session_user_id)
            .filter(|user_id| !user_id.is_empty());
        if let Some(user_id) = user_id {
            pairs.push(("UserId".to_owned(), user_id.to_owned()));
        }
        if !self.include_item_types.is_empty() {
            pairs.push((
                "IncludeItemTypes".to_owned(),
                self.include_item_types.join(","),
            ));
        }
        if let Some(term) = self.search_term.as_deref().filter(|term| !term.is_empty()) {
            pairs.push(("SearchTerm".to_owned(), term.to_owned()));
        }
        pairs
    }
}

fn fields_param(extra: &[String]) -> String {
    let mut fields = vec![
        "Video3DFormat".to_owned(),
        "MediaStreams".to_owned(),
        "Path".to_owned(),
    ];
    for name in extra {
        if !name.is_empty() && !fields.iter().any(|existing| existing == name) {
            fields.push(name.clone());
        }
    }
    fields.join(",")
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub(crate) struct ItemsResponse {
    pub(crate) items: Vec<BaseItem>,
    pub(crate) start_index: i64,
    pub(crate) total_record_count: i64,
}

impl ItemsResponse {
    pub(crate) fn into_page(self) -> ItemPage {
        ItemPage {
            items: self.items,
            start_index: u32::try_from(self.start_index.max(0)).unwrap_or(u32::MAX),
            total_record_count: u64::try_from(self.total_record_count.max(0)).unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    #[test]
    fn pascal_and_camel_items_share_one_shape() {
        let pascal = serde_json::json!({
            "Name": "Avatar",
            "Id": "item-1",
            "Type": "Movie",
            "Path": "/media/avatar.mkv",
            "Overview": null,
            "IsFolder": false,
            "Video3DFormat": "HalfSideBySide",
            "RunTimeTicks": 97_200_000_000_i64,
            "MediaStreams": null,
            "ImageTags": null,
            "UserData": {
                "PlaybackPositionTicks": null,
                "Played": true
            },
            "UnrelatedFutureField": {"Keep": true}
        });
        let camel = serde_json::json!({
            "name": "Avatar",
            "id": "item-1",
            "type": "Movie",
            "path": "/media/avatar.mkv",
            "video3DFormat": "HalfSideBySide",
            "runTimeTicks": 97_200_000_000_i64,
            "mediaStreams": [{
                "index": 0,
                "type": "Video",
                "codec": "hevc",
                "width": 3840,
                "height": 1080
            }],
            "userData": {"played": false, "playbackPositionTicks": 10}
        });

        let from_pascal: BaseItem = parse(pascal).unwrap();
        assert_eq!(from_pascal.id, "item-1");
        assert_eq!(from_pascal.item_type.as_deref(), Some("Movie"));
        assert_eq!(from_pascal.path.as_deref(), Some("/media/avatar.mkv"));
        assert_eq!(from_pascal.overview, None);
        assert_eq!(
            from_pascal.video_3d_format,
            Some(Video3DFormat::HalfSideBySide)
        );
        assert!(from_pascal.media_streams.is_empty());
        assert!(from_pascal.image_tags.is_empty());
        assert_eq!(from_pascal.run_time_ticks, Some(97_200_000_000));
        let user_data = from_pascal.user_data.unwrap();
        assert!(user_data.played);
        assert_eq!(user_data.playback_position_ticks, 0);

        let from_camel: BaseItem = parse(camel).unwrap();
        assert_eq!(
            from_camel.video_stream().unwrap().codec.as_deref(),
            Some("hevc")
        );
        assert_eq!(from_camel.video_stream().unwrap().width, Some(3840));
        assert_eq!(from_camel.user_data.unwrap().playback_position_ticks, 10);
    }

    #[test]
    fn unknown_3d_format_and_stream_type_are_kept() {
        let item: BaseItem = parse(serde_json::json!({
            "Id": "item-2",
            "Name": "Future",
            "Video3DFormat": "SpatialMvHevc",
            "MediaStreams": [{"Index": 3, "Type": "Hologram", "Codec": "mv-hevc"}]
        }))
        .unwrap();
        assert_eq!(
            item.video_3d_format,
            Some(Video3DFormat::Other("SpatialMvHevc".to_owned()))
        );
        assert_eq!(
            item.media_streams[0].stream_type,
            Some(MediaStreamType::Other)
        );
        assert_eq!(item.media_streams[0].index, 3);
        assert_eq!(Video3DFormat::Mvc.to_string(), "MVC");
    }

    #[test]
    fn fields_always_lead_with_the_player_set_and_ignore_duplicates() {
        let query = ItemsQuery {
            extra_fields: vec![
                "Video3DFormat".to_owned(),
                "MediaSources".to_owned(),
                String::new(),
            ],
            ..Default::default()
        };
        let pairs = query.query_pairs(Some("user-1"));
        let fields = pairs.iter().find(|(key, _)| key == "Fields").unwrap();
        assert_eq!(fields.1, "Video3DFormat,MediaStreams,Path,MediaSources");
        assert_eq!(
            pairs.iter().find(|(key, _)| key == "UserId").unwrap().1,
            "user-1"
        );
    }

    #[test]
    fn explicit_user_id_overrides_the_session_and_paging_stops_on_an_empty_page() {
        let query = ItemsQuery {
            user_id: Some("other-user".to_owned()),
            parent_id: Some("library".to_owned()),
            recursive: true,
            include_item_types: vec!["Movie".to_owned(), "Video".to_owned()],
            search_term: Some("avatar".to_owned()),
            start_index: 20,
            limit: 10,
            ..Default::default()
        };
        let pairs = query.query_pairs(Some("signed-in"));
        assert!(pairs.contains(&("UserId".to_owned(), "other-user".to_owned())));
        assert!(pairs.contains(&("ParentId".to_owned(), "library".to_owned())));
        assert!(pairs.contains(&("Recursive".to_owned(), "true".to_owned())));
        assert!(pairs.contains(&("IncludeItemTypes".to_owned(), "Movie,Video".to_owned())));
        assert!(pairs.contains(&("SearchTerm".to_owned(), "avatar".to_owned())));
        assert!(pairs.contains(&("StartIndex".to_owned(), "20".to_owned())));
        assert!(pairs.contains(&("Limit".to_owned(), "10".to_owned())));

        let full = ItemPage {
            items: vec![BaseItem::default()],
            start_index: 0,
            total_record_count: 1,
        };
        assert!(!full.has_more());
        let more = ItemPage {
            items: vec![BaseItem::default()],
            start_index: 0,
            total_record_count: 2,
        };
        assert_eq!(more.next_start_index(), Some(1));
        let empty = ItemPage {
            items: Vec::new(),
            start_index: 0,
            total_record_count: 5,
        };
        assert!(empty.next_start_index().is_none());
    }
}
