//! Keycloak OpenID Connect (OIDC) authentication stub and interface.

use crate::error::{Error, JellyfinResult as Result};
use serde::{Deserialize, Serialize};
use url::Url;

/// Configuration for Keycloak or standard OpenID Connect provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Base issuer URL (e.g. `https://auth.archerfamily.io/realms/archer`).
    pub issuer_url: Url,
    /// Client identifier registered in Keycloak.
    pub client_id: String,
    /// Optional client secret (for confidential clients).
    pub client_secret: Option<String>,
    /// Redirect URI registered with the OIDC provider (e.g. `quest-jellyfin-3d://oauth/callback`).
    pub redirect_uri: Url,
    /// Scopes requested (defaults to `openid`, `profile`, `email`).
    pub scopes: Vec<String>,
}

impl OidcConfig {
    /// Creates a new `OidcConfig` with standard defaults.
    pub fn new(issuer_url: Url, client_id: impl Into<String>, redirect_uri: Url) -> Self {
        Self {
            issuer_url,
            client_id: client_id.into(),
            client_secret: None,
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    /// Sets an optional client secret.
    pub fn with_client_secret(mut self, secret: impl Into<String>) -> Self {
        self.client_secret = Some(secret.into());
        self
    }

    /// Sets custom scopes.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }
}

/// Token response returned by the OIDC token endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcTokenResponse {
    /// Bearer access token.
    pub access_token: String,
    /// Token type (usually "Bearer").
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Optional identity token containing user claims.
    pub id_token: Option<String>,
    /// Optional refresh token for refreshing the access token.
    pub refresh_token: Option<String>,
    /// Lifetime in seconds.
    pub expires_in: Option<u64>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

/// User info claims returned by the OIDC userinfo endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OidcUserInfo {
    /// Subject identifier (unique user ID).
    pub sub: String,
    /// Preferred username.
    pub preferred_username: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Full name.
    pub name: Option<String>,
}

/// Trait defining OIDC authentication operations.
pub trait OidcProvider: Send + Sync {
    /// Generates the authorization URL to redirect the user or launch in browser.
    fn authorization_url(&self, state: &str, code_challenge: Option<&str>) -> Result<Url>;

    /// Keycloak token endpoint URL.
    fn token_endpoint(&self) -> Result<Url>;

    /// Keycloak user info endpoint URL.
    fn userinfo_endpoint(&self) -> Result<Url>;
}

/// Concrete Keycloak OpenID Connect client.
#[derive(Debug, Clone)]
pub struct KeycloakOidcClient {
    config: OidcConfig,
    http: reqwest::Client,
}

impl KeycloakOidcClient {
    /// Creates a new Keycloak client from configuration.
    pub fn new(config: OidcConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    /// Returns a reference to the OIDC configuration.
    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    /// Helper to construct well-known Keycloak endpoint URLs.
    fn build_endpoint(&self, path_suffix: &str) -> Result<Url> {
        let base_str = self.config.issuer_url.as_str().trim_end_matches('/');
        let full = format!("{}/protocol/openid-connect/{}", base_str, path_suffix);
        Url::parse(&full).map_err(|err| Error::Oidc(err.to_string()))
    }

    /// Exchanges an authorization code (and optional PKCE code verifier) for tokens.
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: Option<&str>,
    ) -> Result<OidcTokenResponse> {
        let token_url = self.token_endpoint()?;
        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("client_id", self.config.client_id.as_str()),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];
        if let Some(verifier) = code_verifier {
            params.push(("code_verifier", verifier));
        }

        let mut req = self.http.post(token_url).form(&params);
        if let Some(secret) = &self.config.client_secret {
            req = req.basic_auth(&self.config.client_id, Some(secret));
        }

        let res = req.send().await.map_err(oidc_err)?;
        if !res.status().is_success() {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            return Err(Error::Oidc(format!(
                "OIDC token exchange failed: HTTP {status}: {body}"
            )));
        }

        let token_resp = res.json::<OidcTokenResponse>().await.map_err(oidc_err)?;
        Ok(token_resp)
    }

    /// Refreshes an access token using a refresh token.
    pub async fn refresh_access_token(&self, refresh_token: &str) -> Result<OidcTokenResponse> {
        let token_url = self.token_endpoint()?;
        let params = vec![
            ("grant_type", "refresh_token"),
            ("client_id", self.config.client_id.as_str()),
            ("refresh_token", refresh_token),
        ];

        let mut req = self.http.post(token_url).form(&params);
        if let Some(secret) = &self.config.client_secret {
            req = req.basic_auth(&self.config.client_id, Some(secret));
        }

        let res = req.send().await.map_err(oidc_err)?;
        if !res.status().is_success() {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            return Err(Error::Oidc(format!(
                "OIDC token refresh failed: HTTP {status}: {body}"
            )));
        }

        let token_resp = res.json::<OidcTokenResponse>().await.map_err(oidc_err)?;
        Ok(token_resp)
    }

    /// Fetches user info claims using the access token.
    pub async fn fetch_user_info(&self, access_token: &str) -> Result<OidcUserInfo> {
        let userinfo_url = self.userinfo_endpoint()?;
        let res = self
            .http
            .get(userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(oidc_err)?;

        if !res.status().is_success() {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            return Err(Error::Oidc(format!(
                "OIDC userinfo failed: HTTP {status}: {body}"
            )));
        }

        let user_info = res.json::<OidcUserInfo>().await.map_err(oidc_err)?;
        Ok(user_info)
    }
}

impl OidcProvider for KeycloakOidcClient {
    fn authorization_url(&self, state: &str, code_challenge: Option<&str>) -> Result<Url> {
        let mut url = self.build_endpoint("auth")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("response_type", "code");
            query.append_pair("redirect_uri", self.config.redirect_uri.as_str());
            query.append_pair("scope", &self.config.scopes.join(" "));
            query.append_pair("state", state);

            if let Some(challenge) = code_challenge {
                query.append_pair("code_challenge", challenge);
                query.append_pair("code_challenge_method", "S256");
            }
        }
        Ok(url)
    }

    fn token_endpoint(&self) -> Result<Url> {
        self.build_endpoint("token")
    }

    fn userinfo_endpoint(&self) -> Result<Url> {
        self.build_endpoint("userinfo")
    }
}

/// Mock OIDC provider for tests and offline development.
#[derive(Debug, Clone)]
pub struct MockOidcProvider {
    pub auth_url: Url,
    pub token_url: Url,
    pub userinfo_url: Url,
}

impl Default for MockOidcProvider {
    fn default() -> Self {
        Self {
            auth_url: Url::parse("https://auth.example.com/auth").unwrap(),
            token_url: Url::parse("https://auth.example.com/token").unwrap(),
            userinfo_url: Url::parse("https://auth.example.com/userinfo").unwrap(),
        }
    }
}

impl OidcProvider for MockOidcProvider {
    fn authorization_url(&self, state: &str, _code_challenge: Option<&str>) -> Result<Url> {
        let mut url = self.auth_url.clone();
        url.query_pairs_mut().append_pair("state", state);
        Ok(url)
    }

    fn token_endpoint(&self) -> Result<Url> {
        Ok(self.token_url.clone())
    }

    fn userinfo_endpoint(&self) -> Result<Url> {
        Ok(self.userinfo_url.clone())
    }
}

fn oidc_err(err: reqwest::Error) -> Error {
    Error::Oidc(err.without_url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_url_generation() {
        let config = OidcConfig::new(
            Url::parse("https://auth.archerfamily.io/realms/archer").unwrap(),
            "quest-jellyfin-3d",
            Url::parse("quest-jellyfin-3d://oauth/callback").unwrap(),
        );
        let client = KeycloakOidcClient::new(config);

        let url = client
            .authorization_url("state-12345", Some("challenge-code-abc"))
            .unwrap();

        assert_eq!(url.path(), "/realms/archer/protocol/openid-connect/auth");
        let query_map: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query_map.get("client_id").unwrap(), "quest-jellyfin-3d");
        assert_eq!(query_map.get("response_type").unwrap(), "code");
        assert_eq!(query_map.get("state").unwrap(), "state-12345");
        assert_eq!(
            query_map.get("code_challenge").unwrap(),
            "challenge-code-abc"
        );
        assert_eq!(query_map.get("code_challenge_method").unwrap(), "S256");
        assert_eq!(
            query_map.get("redirect_uri").unwrap(),
            "quest-jellyfin-3d://oauth/callback"
        );
    }
}
