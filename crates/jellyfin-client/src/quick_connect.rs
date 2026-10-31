//! QuickConnect pairing types.
use serde::Deserialize;

use crate::store::StoredSession;

/// Code the user types on a Jellyfin client that is already signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickConnectChallenge {
    /// Secret used to poll and to exchange for an access token. Not shown to the user.
    pub secret: String,
    /// Short code shown to the user. Jellyfin uses six digits.
    pub code: String,
}

/// One answer from `GET /QuickConnect/Connect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickConnectPoll {
    /// Nobody has authorized `code` yet.
    Pending {
        /// Code to keep on screen.
        code: String,
    },
    /// The server authorized the request and the access token is stored.
    Authorized(StoredSession),
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub(crate) struct QuickConnectResult {
    pub(crate) secret: String,
    pub(crate) code: String,
    pub(crate) authenticated: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub(crate) struct AuthenticatedUser {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) server_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub(crate) struct AuthenticationResult {
    pub(crate) access_token: String,
    pub(crate) server_id: String,
    pub(crate) user: Option<AuthenticatedUser>,
}
