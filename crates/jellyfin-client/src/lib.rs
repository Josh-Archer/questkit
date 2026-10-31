//! Headless Jellyfin HTTP client for the Quest 3D player.
//!
//! Pairing is `POST /QuickConnect/Initiate` and `GET /QuickConnect/Connect`.
//! On Jellyfin 10.8 and later the poll only flips `Authenticated`. This client
//! then exchanges the secret with `POST /Users/AuthenticateWithQuickConnect`
//! and hands the access token to a [`TokenStore`].
//!
//! Library browse is `GET /Items?Fields=Video3DFormat,MediaStreams,Path` with
//! `StartIndex` and `Limit`. Every request sends
//! `Authorization: MediaBrowser` with the client, device, device id, version,
//! and token once one exists.
//!
//! Playback: `POST /Items/{id}/PlaybackInfo` with the Quest 3 device profile
//! ([`quest_device_profile`], ~8K30 HEVC cap), a direct play stream URL, and
//! `/Sessions/Playing`, `/Progress`, `/Stopped` reporting. [`KeycloakOidcClient`]
//! is an OpenID Connect stub behind the [`OidcProvider`] trait. Stereo/projection
//! detection lives in `xr_video::format` (DeskQuest#220), not here.
//!
//! ```
//! # fn main() -> jellyfin_client::JellyfinResult<()> {
//! use std::sync::Arc;
//! use jellyfin_client::{ClientIdentity, JellyfinClient, MemoryTokenStore};
//!
//! let identity = ClientIdentity::new("DeskQuest", "Quest 3", "device-1", "0.1.0")?;
//! let client = JellyfinClient::new(
//!     "http://127.0.0.1:8096",
//!     identity,
//!     Arc::new(MemoryTokenStore::new()),
//! )?;
//! assert!(client.session().is_none());
//! # Ok(())
//! # }
//! ```

mod client;
mod device_profile;
mod error;
mod identity;
mod json;
mod library;
mod oidc;
mod playback;
mod quick_connect;
mod sessions;
mod store;

pub use client::JellyfinClient;
pub use device_profile::{
    evaluate_direct_play, quest_device_profile, CodecProfile, DeviceProfile, DirectPlayDecision,
    DirectPlayProfile, ProfileCondition, TranscodingProfile,
};
pub use error::{Error, JellyfinResult};
pub use identity::{authorization_header, ClientIdentity};
pub use library::{
    BaseItem, ItemPage, ItemsQuery, MediaStream, MediaStreamType, UserData, Video3DFormat,
    ITEM_FIELDS, TICKS_PER_SECOND,
};
pub use oidc::{
    KeycloakOidcClient, MockOidcProvider, OidcConfig, OidcProvider, OidcTokenResponse, OidcUserInfo,
};
pub use playback::{
    build_direct_play_url, resolve_playback, MediaSource, PlaybackInfoRequest,
    PlaybackInfoResponse, ResolvedPlayback,
};
pub use quick_connect::{QuickConnectChallenge, QuickConnectPoll};
pub use sessions::{
    PlayMethod, PlaybackProgressRequest, PlaybackStartRequest, PlaybackStopRequest,
};
pub use store::{MemoryTokenStore, StoreError, StoredSession, TokenStore};
