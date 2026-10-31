//! Hook the player implements to keep a Jellyfin access token.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Failure from a [`TokenStore`] implementation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{0}")]
pub struct StoreError(pub String);

impl StoreError {
    /// Store error with a display message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Paired Jellyfin session. The access token is what later requests send.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    /// Server base URL this token was issued for, with no trailing slash.
    pub server_url: String,
    /// Jellyfin access token. Sent as `Token` in the MediaBrowser header.
    pub access_token: String,
    /// User id from the authentication result. Empty when the server omitted it.
    pub user_id: String,
    /// User name from the authentication result.
    pub user_name: String,
    /// Jellyfin server id.
    pub server_id: String,
    /// Device id that initiated QuickConnect.
    pub device_id: String,
}

impl std::fmt::Debug for StoredSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoredSession")
            .field("server_url", &self.server_url)
            .field("access_token", &"<redacted>")
            .field("user_id", &self.user_id)
            .field("user_name", &self.user_name)
            .field("server_id", &self.server_id)
            .field("device_id", &self.device_id)
            .finish()
    }
}

/// Persistence hook for the paired access token.
///
/// [`crate::JellyfinClient`] calls [`TokenStore::load`] when it is created,
/// [`TokenStore::save`] after QuickConnect yields an access token, and
/// [`TokenStore::clear`] on sign-out. The methods are synchronous so a player
/// can write a file or Android preferences without an async runtime in the trait.
/// Implementations must not call back into the client.
pub trait TokenStore: Send + Sync {
    /// Previously saved session, or `Ok(None)` when the player has not paired.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing storage cannot be read.
    fn load(&self) -> Result<Option<StoredSession>, StoreError>;

    /// Replace any saved session with `session`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the session cannot be written.
    fn save(&self, session: &StoredSession) -> Result<(), StoreError>;

    /// Forget the saved session.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing storage cannot be updated.
    fn clear(&self) -> Result<(), StoreError>;
}

/// In-memory [`TokenStore`] for tests and for a process that does not persist yet.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    session: Mutex<Option<StoredSession>>,
}

impl MemoryTokenStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        Ok(self.session.lock().clone())
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        *self.session.lock() = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        *self.session.lock() = None;
        Ok(())
    }
}
