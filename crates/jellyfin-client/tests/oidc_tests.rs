use jellyfin_client::{KeycloakOidcClient, MockOidcProvider, OidcConfig, OidcProvider};
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn test_keycloak_endpoint_construction() {
    let config = OidcConfig::new(
        Url::parse("https://auth.archerfamily.io/realms/archer").unwrap(),
        "quest-jellyfin-3d",
        Url::parse("quest-jellyfin-3d://oauth/callback").unwrap(),
    );
    let client = KeycloakOidcClient::new(config);

    assert_eq!(
        client.token_endpoint().unwrap().as_str(),
        "https://auth.archerfamily.io/realms/archer/protocol/openid-connect/token"
    );
    assert_eq!(
        client.userinfo_endpoint().unwrap().as_str(),
        "https://auth.archerfamily.io/realms/archer/protocol/openid-connect/userinfo"
    );
}

#[tokio::test]
async fn test_oidc_token_exchange_and_userinfo_flow() {
    let mock_server = MockServer::start().await;
    let issuer_url = Url::parse(&mock_server.uri()).unwrap();

    // Mock token exchange
    let token_json = serde_json::json!({
        "access_token": "mock-access-token-12345",
        "token_type": "Bearer",
        "id_token": "mock-id-token-xyz",
        "refresh_token": "mock-refresh-token-67890",
        "expires_in": 3600
    });
    Mock::given(method("POST"))
        .and(path("/protocol/openid-connect/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&token_json))
        .mount(&mock_server)
        .await;

    // Mock userinfo
    let userinfo_json = serde_json::json!({
        "sub": "user-guid-josh",
        "preferred_username": "josh",
        "email": "jarcher1200@gmail.com",
        "name": "Josh Archer"
    });
    Mock::given(method("GET"))
        .and(path("/protocol/openid-connect/userinfo"))
        .and(header("authorization", "Bearer mock-access-token-12345"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&userinfo_json))
        .mount(&mock_server)
        .await;

    let config = OidcConfig::new(
        issuer_url,
        "quest-jellyfin-3d",
        Url::parse("quest-jellyfin-3d://oauth/callback").unwrap(),
    );
    let client = KeycloakOidcClient::new(config);

    // 1. Code exchange
    let tokens = client
        .exchange_code("auth-code-test", Some("pkce-verifier-abc"))
        .await
        .expect("Token exchange succeeded");

    assert_eq!(tokens.access_token, "mock-access-token-12345");
    assert_eq!(
        tokens.refresh_token.as_deref(),
        Some("mock-refresh-token-67890")
    );

    // 2. User info fetch
    let user_info = client
        .fetch_user_info(&tokens.access_token)
        .await
        .expect("Userinfo fetch succeeded");

    assert_eq!(user_info.sub, "user-guid-josh");
    assert_eq!(user_info.preferred_username.as_deref(), Some("josh"));
    assert_eq!(user_info.email.as_deref(), Some("jarcher1200@gmail.com"));

    // 3. Refresh token
    let refreshed = client
        .refresh_access_token(&tokens.refresh_token.unwrap())
        .await
        .expect("Token refresh succeeded");
    assert_eq!(refreshed.access_token, "mock-access-token-12345");
}

#[test]
fn test_mock_oidc_provider() {
    let mock = MockOidcProvider::default();
    let auth_url = mock.authorization_url("state-test", None).unwrap();
    assert!(auth_url.as_str().contains("state=state-test"));
}
