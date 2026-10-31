//! Failures from the Jellyfin HTTP client and the token store.
use crate::store::StoreError;
use thiserror::Error;

/// Result of a Jellyfin client operation.
pub type JellyfinResult<T> = Result<T, Error>;

/// A pairing, browse, transport, or token-store failure.
#[derive(Debug, Error)]
pub enum Error {
    /// `base_url` was empty or was not `http` or `https`.
    #[error("server url must be http or https, got {0}")]
    InvalidBaseUrl(String),

    /// A required client identity field was empty.
    #[error("client identity is missing {0}")]
    InvalidIdentity(&'static str),

    /// The TCP/TLS request failed before a Jellyfin status was available.
    #[error("jellyfin request failed: {0}")]
    Transport(String),

    /// The response body was not the JSON shape this client understands.
    #[error("jellyfin response could not be decoded: {0}")]
    Decode(String),

    /// Jellyfin returned a non-success status that is not a pairing state.
    #[error("jellyfin returned HTTP {status} for {path}: {body}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Request path, without the query string.
        path: &'static str,
        /// Truncated response body.
        body: String,
    },

    /// `POST /QuickConnect/Initiate` (or a later pairing call) returned 401.
    ///
    /// Jellyfin does this when QuickConnect is turned off for the server.
    #[error("quick connect is disabled on this server")]
    QuickConnectDisabled,

    /// The quick-connect secret is unknown, expired, or not authorized yet.
    #[error("quick connect secret is unknown or expired")]
    UnknownSecret,

    /// Polling ended before another device authorized the code.
    #[error("quick connect was not authorized after {attempts} polls")]
    QuickConnectTimeout {
        /// How many `GET /QuickConnect/Connect` calls were made.
        attempts: u32,
    },

    /// A required property was missing or empty in a success response.
    #[error("jellyfin response is missing {0}")]
    MissingField(&'static str),

    /// Library calls need an access token from QuickConnect or the token store.
    #[error("not signed in")]
    NotSignedIn,

    /// Keycloak/OpenID Connect discovery, token, or userinfo call failed.
    #[error("oidc: {0}")]
    Oidc(String),

    /// A URL could not be built from the server base URL and an item id.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// [`crate::TokenStore`] failed while loading, saving, or clearing a session.
    #[error("token store: {0}")]
    Store(#[source] StoreError),
}

impl Error {
    /// HTTP status when this error came from a Jellyfin response.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            Self::QuickConnectDisabled => Some(401),
            Self::UnknownSecret => Some(404),
            _ => None,
        }
    }
}

pub(crate) fn truncate_body(body: &str) -> String {
    const MAX_CHARS: usize = 512;
    let mut truncated = String::new();
    for (index, ch) in body.chars().enumerate() {
        if index == MAX_CHARS {
            truncated.push_str("...");
            break;
        }
        truncated.push(ch);
    }
    truncated
}

pub(crate) fn scrub_secret(message: &str, secret: Option<&str>) -> String {
    let Some(secret) = secret.filter(|value| !value.is_empty()) else {
        return message.to_owned();
    };
    let encoded = crate::identity::encode_component(secret);
    message
        .replace(secret, "<redacted>")
        .replace(&encoded, "<redacted>")
}
