//! HTTP calls for QuickConnect, library browse, PlaybackInfo, and session reporting.
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;

use crate::device_profile::{quest_device_profile, DeviceProfile};
use crate::error::{scrub_secret, truncate_body, Error, JellyfinResult};
use crate::identity::{authorization_header, ClientIdentity};
use crate::json::parse;
use crate::library::{BaseItem, ItemPage, ItemsQuery, ItemsResponse};
use crate::playback::{build_direct_play_url, PlaybackInfoRequest, PlaybackInfoResponse};
use crate::quick_connect::{
    AuthenticationResult, QuickConnectChallenge, QuickConnectPoll, QuickConnectResult,
};
use crate::sessions::{PlaybackProgressRequest, PlaybackStartRequest, PlaybackStopRequest};
use crate::store::{StoredSession, TokenStore};

const INITIATE_PATH: &str = "/QuickConnect/Initiate";
const CONNECT_PATH: &str = "/QuickConnect/Connect";
const EXCHANGE_PATH: &str = "/Users/AuthenticateWithQuickConnect";
const ITEMS_PATH: &str = "/Items";
const PLAYBACK_INFO_PATH: &str = "/Items/{id}/PlaybackInfo";
const PLAYING_PATH: &str = "/Sessions/Playing";
const PROGRESS_PATH: &str = "/Sessions/Playing/Progress";
const STOPPED_PATH: &str = "/Sessions/Playing/Stopped";

/// Headless Jellyfin client for the 3D player.
///
/// QuickConnect is `POST /QuickConnect/Initiate`, then `GET /QuickConnect/Connect`
/// until `Authenticated` is true. Jellyfin 10.8 and later do not put the access
/// token on that poll. The client then `POST`s the secret to
/// `/Users/AuthenticateWithQuickConnect` and saves the token through [`TokenStore`].
#[derive(Clone)]
pub struct JellyfinClient {
    http: reqwest::Client,
    base_url: String,
    identity: ClientIdentity,
    session: Arc<Mutex<Option<StoredSession>>>,
    store: Arc<dyn TokenStore>,
}

impl std::fmt::Debug for JellyfinClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JellyfinClient")
            .field("base_url", &self.base_url)
            .field("identity", &self.identity)
            .field("signed_in", &self.access_token().is_some())
            .finish()
    }
}

impl JellyfinClient {
    /// Client for `base_url`. A saved session is loaded before the client is returned.
    ///
    /// `base_url` may include a Jellyfin subpath (`http://host:8096/jellyfin`).
    /// A trailing slash is removed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidBaseUrl`] for a non-HTTP URL and [`Error::Store`]
    /// when the token store cannot be read.
    pub fn new(
        base_url: &str,
        identity: ClientIdentity,
        store: Arc<dyn TokenStore>,
    ) -> JellyfinResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(format!("{}/{}", identity.client(), identity.version()))
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
                headers
            })
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        let client = Self {
            http,
            base_url: normalize_base_url(base_url)?,
            identity,
            session: Arc::new(Mutex::new(None)),
            store,
        };
        client.restore()?;
        Ok(client)
    }

    /// Client with an empty [`crate::MemoryTokenStore`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidBaseUrl`] for a non-HTTP URL.
    pub fn with_memory_store(base_url: &str, identity: ClientIdentity) -> JellyfinResult<Self> {
        Self::new(
            base_url,
            identity,
            Arc::new(crate::store::MemoryTokenStore::new()),
        )
    }

    /// Server base URL with no trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Identity attached to every request.
    #[must_use]
    pub fn identity(&self) -> &ClientIdentity {
        &self.identity
    }

    /// Session held in memory, if the client is signed in.
    #[must_use]
    pub fn session(&self) -> Option<StoredSession> {
        self.session.lock().clone()
    }

    /// Access token held in memory.
    #[must_use]
    pub fn access_token(&self) -> Option<String> {
        self.session().map(|session| session.access_token)
    }

    /// Reload the session from the token store.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store cannot be read.
    pub fn restore(&self) -> JellyfinResult<Option<StoredSession>> {
        let loaded = self.store.load().map_err(Error::Store)?;
        // Never send a saved token to a different server or device identity.
        let loaded = loaded.filter(|session| {
            normalize_base_url(&session.server_url).ok().as_deref() == Some(&self.base_url)
                && session.device_id == self.identity.device_id()
        });
        *self.session.lock() = loaded.clone();
        Ok(loaded)
    }

    /// Forget the in-memory session and clear the token store.
    ///
    /// The in-memory session is kept when the store cannot be cleared, so a
    /// retry does not disagree with what was saved.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store cannot be cleared.
    pub fn clear_session(&self) -> JellyfinResult<()> {
        self.store.clear().map_err(Error::Store)?;
        *self.session.lock() = None;
        Ok(())
    }

    /// `POST /QuickConnect/Initiate`.
    ///
    /// Show [`QuickConnectChallenge::code`] to the user. Keep the secret and pass
    /// it to [`JellyfinClient::poll_quick_connect`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::QuickConnectDisabled`] when the server answers 401.
    pub async fn initiate_quick_connect(&self) -> JellyfinResult<QuickConnectChallenge> {
        let result: QuickConnectResult = self
            .request(Method::POST, INITIATE_PATH, &[], None, None)
            .await?;
        if result.secret.is_empty() {
            return Err(Error::MissingField("Secret"));
        }
        if result.code.is_empty() {
            return Err(Error::MissingField("Code"));
        }
        Ok(QuickConnectChallenge {
            secret: result.secret,
            code: result.code,
        })
    }

    /// `GET /QuickConnect/Connect` for one secret.
    ///
    /// When the server reports the request as authorized, this also exchanges
    /// the secret for an access token and saves it through the token store.
    /// The in-memory session is updated only after that save succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownSecret`] for an unknown or expired secret,
    /// [`Error::MissingField`] when the token exchange omits `AccessToken`, and
    /// [`Error::Store`] when the token cannot be saved.
    pub async fn poll_quick_connect(&self, secret: &str) -> JellyfinResult<QuickConnectPoll> {
        if secret.is_empty() {
            return Err(Error::MissingField("Secret"));
        }
        let query = [("secret".to_owned(), secret.to_owned())];
        let result: QuickConnectResult = self
            .request(Method::GET, CONNECT_PATH, &query, None, Some(secret))
            .await?;
        if !result.authenticated {
            return Ok(QuickConnectPoll::Pending { code: result.code });
        }
        let session = self.exchange_secret(secret).await?;
        self.store.save(&session).map_err(Error::Store)?;
        *self.session.lock() = Some(session.clone());
        Ok(QuickConnectPoll::Authorized(session))
    }

    /// Poll until the code is authorized or `attempts` is exhausted.
    ///
    /// The interval is the wait between polls. The last attempt does not sleep.
    ///
    /// # Errors
    ///
    /// Returns [`Error::QuickConnectTimeout`] when every poll is still pending.
    /// Other poll errors are returned as-is.
    pub async fn wait_for_quick_connect(
        &self,
        secret: &str,
        interval: Duration,
        attempts: u32,
    ) -> JellyfinResult<StoredSession> {
        for attempt in 0..attempts {
            if let QuickConnectPoll::Authorized(session) = self.poll_quick_connect(secret).await? {
                return Ok(session);
            }
            if attempt + 1 < attempts {
                tokio::time::sleep(interval).await;
            }
        }
        Err(Error::QuickConnectTimeout { attempts })
    }

    /// `GET /Items` with `Fields=Video3DFormat,MediaStreams,Path` and paging.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotSignedIn`] when no access token is loaded.
    pub async fn list_items(&self, query: &ItemsQuery) -> JellyfinResult<ItemPage> {
        if self.access_token().is_none() {
            return Err(Error::NotSignedIn);
        }
        let user_id = self.session().map(|session| session.user_id);
        let pairs = query.query_pairs(user_id.as_deref());
        let response: ItemsResponse = self
            .request(Method::GET, ITEMS_PATH, &pairs, None, None)
            .await?;
        Ok(response.into_page())
    }

    /// Library views available to the signed-in user, honoring server permissions.
    pub async fn library_views(&self) -> JellyfinResult<ItemPage> {
        let user = self.session().ok_or(Error::NotSignedIn)?.user_id;
        let path = format!("/Users/{}/Views", crate::identity::encode_component(&user));
        let value = self
            .send(Method::GET, "/Users/{userId}/Views", &path, &[], None, None)
            .await?
            .ok_or_else(|| Error::Decode("empty library response".into()))?;
        let response: ItemsResponse = parse(value).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(response.into_page())
    }

    /// Resumable videos for Continue Watching.
    pub async fn resume_items(&self) -> JellyfinResult<ItemPage> {
        let user = self.session().ok_or(Error::NotSignedIn)?.user_id;
        let path = format!(
            "/Users/{}/Items/Resume",
            crate::identity::encode_component(&user)
        );
        let query = vec![
            ("Limit".into(), "20".into()),
            ("MediaTypes".into(), "Video".into()),
            ("Fields".into(), "Overview,PrimaryImageAspectRatio".into()),
            ("EnableUserData".into(), "true".into()),
        ];
        let value = self
            .send(
                Method::GET,
                "/Users/{userId}/Items/Resume",
                &path,
                &query,
                None,
                None,
            )
            .await?
            .ok_or_else(|| Error::Decode("empty resume response".into()))?;
        let response: ItemsResponse = parse(value).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(response.into_page())
    }

    /// Set the signed-in user's favorite flag on a title.
    pub async fn set_favorite(&self, item_id: &str, favorite: bool) -> JellyfinResult<()> {
        let user = self.session().ok_or(Error::NotSignedIn)?.user_id;
        let path = format!(
            "/Users/{}/FavoriteItems/{}",
            crate::identity::encode_component(&user),
            crate::identity::encode_component(item_id)
        );
        self.send(
            if favorite {
                Method::POST
            } else {
                Method::DELETE
            },
            "/Users/{userId}/FavoriteItems/{itemId}",
            &path,
            &[],
            None,
            None,
        )
        .await?;
        Ok(())
    }

    /// Fetch one item for the signed-in user, including resume and stream metadata.
    pub async fn get_item(&self, item_id: &str) -> JellyfinResult<BaseItem> {
        let user = self.session().ok_or(Error::NotSignedIn)?.user_id;
        if user.is_empty() {
            return Err(Error::MissingField("User.Id"));
        }
        let path = format!(
            "/Users/{}/Items/{}",
            crate::identity::encode_component(&user),
            crate::identity::encode_component(item_id)
        );
        let value = self
            .send(
                Method::GET,
                "/Users/{userId}/Items/{itemId}",
                &path,
                &[],
                None,
                None,
            )
            .await?
            .ok_or(Error::Decode("empty item response".into()))?;
        parse(value).map_err(|err| Error::Decode(err.to_string()))
    }

    /// `POST /Items/{id}/PlaybackInfo` with the Quest 3 device profile
    /// ([`crate::quest_device_profile`]) unless `profile_override` is given.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotSignedIn`] when no access token is loaded.
    pub async fn playback_info(
        &self,
        item_id: &str,
        profile_override: Option<DeviceProfile>,
    ) -> JellyfinResult<PlaybackInfoResponse> {
        self.playback_info_with_policy(item_id, profile_override, None, true)
            .await
    }

    /// Negotiate with an optional user-selected bitrate ceiling and transcoding policy.
    pub async fn playback_info_with_policy(
        &self,
        item_id: &str,
        profile_override: Option<DeviceProfile>,
        max_bitrate: Option<u64>,
        allow_transcoding: bool,
    ) -> JellyfinResult<PlaybackInfoResponse> {
        if self.access_token().is_none() {
            return Err(Error::NotSignedIn);
        }
        let mut profile = profile_override.unwrap_or_else(quest_device_profile);
        if let Some(bitrate) = max_bitrate {
            profile.max_streaming_bitrate = bitrate;
        }
        let user_id = self
            .session()
            .map(|session| session.user_id)
            .filter(|id| !id.is_empty());
        let mut request = PlaybackInfoRequest::new(user_id, profile);
        if let Some(bitrate) = max_bitrate {
            request.max_streaming_bitrate = Some(bitrate);
        }
        request.enable_transcoding = allow_transcoding;
        let body = serde_json::to_value(&request).map_err(|err| Error::Decode(err.to_string()))?;
        let url_path = format!(
            "/Items/{}/PlaybackInfo",
            crate::identity::encode_component(item_id)
        );
        let value = self
            .send(
                Method::POST,
                PLAYBACK_INFO_PATH,
                &url_path,
                &[],
                Some(&body),
                None,
            )
            .await?
            .ok_or(Error::Decode("empty PlaybackInfo response".to_owned()))?;
        parse(value).map_err(|err| Error::Decode(err.to_string()))
    }

    /// Direct play URL (`/Videos/{id}/stream?static=true&api_key=..`) for the
    /// hardware decoder.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidUrl`] if the URL cannot be built.
    pub fn direct_play_url(&self, item_id: &str) -> JellyfinResult<url::Url> {
        build_direct_play_url(&self.base_url, item_id, self.access_token().as_deref())
    }

    /// `POST /Sessions/Playing`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotSignedIn`] when no access token is loaded.
    pub async fn report_playing(&self, request: &PlaybackStartRequest) -> JellyfinResult<()> {
        self.post_session_report(PLAYING_PATH, request).await
    }

    /// `POST /Sessions/Playing/Progress` (time updates, pause, unpause).
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotSignedIn`] when no access token is loaded.
    pub async fn report_progress(&self, request: &PlaybackProgressRequest) -> JellyfinResult<()> {
        self.post_session_report(PROGRESS_PATH, request).await
    }

    /// `POST /Sessions/Playing/Stopped`.
    ///
    /// Fetch a bounded primary poster using the authenticated Jellyfin image API.
    /// Returns `None` when the item has no primary image.
    ///
    /// # Errors
    /// Returns an error for unauthenticated, failed, or oversized responses.
    pub async fn primary_image(&self, item_id: &str) -> JellyfinResult<Option<Vec<u8>>> {
        if self.access_token().is_none() {
            return Err(Error::NotSignedIn);
        }
        let path = format!(
            "/Items/{}/Images/Primary",
            crate::identity::encode_component(item_id)
        );
        let mut response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .header(AUTHORIZATION, self.authorization_value())
            .header(ACCEPT, "image/jpeg")
            .query(&[("maxWidth", "400"), ("format", "Jpg")])
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_error(
                "/Items/{id}/Images/Primary",
                response.status(),
                "",
            ));
        }
        const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_IMAGE_BYTES as u64)
        {
            return Err(Error::Decode("Poster exceeds size limit".into()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?
        {
            if chunk.len() > MAX_IMAGE_BYTES - bytes.len() {
                return Err(Error::Decode("Poster exceeds size limit".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Some(bytes))
    }

    /// # Errors
    ///
    /// Returns [`Error::NotSignedIn`] when no access token is loaded.
    pub async fn report_stopped(&self, request: &PlaybackStopRequest) -> JellyfinResult<()> {
        self.post_session_report(STOPPED_PATH, request).await
    }

    async fn exchange_secret(&self, secret: &str) -> JellyfinResult<StoredSession> {
        let body = serde_json::json!({ "Secret": secret });
        let result: AuthenticationResult = self
            .request(Method::POST, EXCHANGE_PATH, &[], Some(&body), Some(secret))
            .await?;
        if result.access_token.is_empty() {
            return Err(Error::MissingField("AccessToken"));
        }
        let user = result.user.unwrap_or_default();
        let server_id = if result.server_id.is_empty() {
            user.server_id
        } else {
            result.server_id
        };
        Ok(StoredSession {
            server_url: self.base_url.clone(),
            access_token: result.access_token,
            user_id: user.id,
            user_name: user.name,
            server_id,
            device_id: self.identity.device_id().to_owned(),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &'static str,
        query: &[(String, String)],
        body: Option<&serde_json::Value>,
        secret: Option<&str>,
    ) -> JellyfinResult<T> {
        let value = self
            .send(method, path, path, query, body, secret)
            .await?
            .ok_or(Error::Decode("empty response body".to_owned()))?;
        parse(value).map_err(|err| Error::Decode(scrub_secret(&err.to_string(), secret)))
    }

    /// Send a request to `url_path` (may embed an item id). `label` is the
    /// static route used in errors and logs. Returns `None` for an empty body
    /// (for example `204 No Content` from the session endpoints).
    async fn send(
        &self,
        method: Method,
        label: &'static str,
        url_path: &str,
        query: &[(String, String)],
        body: Option<&serde_json::Value>,
        secret: Option<&str>,
    ) -> JellyfinResult<Option<serde_json::Value>> {
        let url = format!("{}{url_path}", self.base_url);
        let mut builder = self
            .http
            .request(method.clone(), &url)
            .header(AUTHORIZATION, self.authorization_value());
        if !query.is_empty() {
            builder = builder.query(query);
        }
        if let Some(body) = body {
            builder = builder.json(body);
        }
        let response = builder
            .send()
            .await
            .map_err(|err| Error::Transport(scrub_secret(&err.to_string(), secret)))?;
        let status = response.status();
        tracing::debug!(%method, path = label, status = status.as_u16(), "jellyfin response");
        let body_text = response
            .text()
            .await
            .map_err(|err| Error::Transport(scrub_secret(&err.to_string(), secret)))?;
        if !status.is_success() {
            return Err(status_error(
                label,
                status,
                &scrub_secret(&body_text, secret),
            ));
        }
        if body_text.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&body_text)
            .map(Some)
            .map_err(|err| Error::Decode(scrub_secret(&err.to_string(), secret)))
    }

    async fn post_session_report<B: serde::Serialize>(
        &self,
        path: &'static str,
        body: &B,
    ) -> JellyfinResult<()> {
        if self.access_token().is_none() {
            return Err(Error::NotSignedIn);
        }
        let body = serde_json::to_value(body).map_err(|err| Error::Decode(err.to_string()))?;
        self.send(Method::POST, path, path, &[], Some(&body), None)
            .await?;
        Ok(())
    }

    fn authorization_value(&self) -> HeaderValue {
        let token = self.access_token();
        let header = authorization_header(&self.identity, token.as_deref());
        HeaderValue::from_str(&header).unwrap_or_else(|_| HeaderValue::from_static("MediaBrowser"))
    }
}

fn status_error(path: &'static str, status: StatusCode, body: &str) -> Error {
    if status == StatusCode::UNAUTHORIZED
        && (path == INITIATE_PATH || path == CONNECT_PATH || path == EXCHANGE_PATH)
    {
        return Error::QuickConnectDisabled;
    }
    if status == StatusCode::NOT_FOUND && (path == CONNECT_PATH || path == EXCHANGE_PATH) {
        return Error::UnknownSecret;
    }
    Error::Api {
        status: status.as_u16(),
        path,
        body: truncate_body(body),
    }
}

fn normalize_base_url(url: &str) -> Result<String, Error> {
    let parsed =
        url::Url::parse(url.trim()).map_err(|_| Error::InvalidBaseUrl("invalid URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(Error::InvalidBaseUrl(
            "expected an HTTP(S) server URL without credentials, query, or fragment".into(),
        ));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_keeps_a_subpath_and_drops_a_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://nas.example:8096/jellyfin/").unwrap(),
            "http://nas.example:8096/jellyfin"
        );
        assert!(normalize_base_url("nas.example:8096").is_err());
        assert!(normalize_base_url("http://").is_err());
    }

    #[test]
    fn scrub_removes_the_raw_and_encoded_secret() {
        let message = "failed for secret abc/def and abc%2Fdef";
        assert_eq!(
            scrub_secret(message, Some("abc/def")),
            "failed for secret <redacted> and <redacted>"
        );
    }
}
