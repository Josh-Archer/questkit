//! PlaybackInfo, direct play URL, and session reporting against a local mock.
use std::sync::Arc;

use jellyfin_client::{
    ClientIdentity, Error, JellyfinClient, MemoryTokenStore, PlaybackProgressRequest,
    PlaybackStartRequest, PlaybackStopRequest, StoredSession, TokenStore,
};
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn identity() -> ClientIdentity {
    ClientIdentity::new("quest-jellyfin-3d", "Quest 3", "device-1", "0.1.0").unwrap()
}

fn signed_in(base_url: &str) -> JellyfinClient {
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: base_url.to_owned(),
            access_token: "token-abc".to_owned(),
            user_id: "user-1".to_owned(),
            user_name: "josh".to_owned(),
            server_id: "server-1".to_owned(),
            device_id: "device-1".to_owned(),
        })
        .unwrap();
    JellyfinClient::new(base_url, identity(), store).unwrap()
}

#[tokio::test]
async fn playback_info_and_direct_play_url() {
    let server = MockServer::start().await;
    let fixture: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/playback_info_response.json")).unwrap();
    Mock::given(method("POST"))
        .and(path("/Items/item-123/PlaybackInfo"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&fixture))
        .expect(1)
        .mount(&server)
        .await;

    let client = signed_in(&server.uri());
    let info = client.playback_info("item-123", None).await.unwrap();
    assert_eq!(
        info.play_session_id.as_deref(),
        Some("play-session-quest-3d-888999")
    );
    assert_eq!(info.media_sources.len(), 1);
    assert!(info.media_sources[0].supports_direct_play);

    let url = client.direct_play_url("item-123").unwrap();
    assert!(url
        .as_str()
        .ends_with("/Videos/item-123/stream?static=true&api_key=token-abc"));
}

#[tokio::test]
async fn direct_play_url_keeps_a_subpath() {
    let client =
        JellyfinClient::with_memory_store("http://nas:8096/jellyfin/", identity()).unwrap();
    assert_eq!(
        client.direct_play_url("abc").unwrap().as_str(),
        "http://nas:8096/jellyfin/Videos/abc/stream?static=true"
    );
}

#[tokio::test]
async fn session_reporting_lifecycle() {
    let server = MockServer::start().await;
    for route in [
        "/Sessions/Playing",
        "/Sessions/Playing/Progress",
        "/Sessions/Playing/Stopped",
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = signed_in(&server.uri());
    client
        .report_playing(&PlaybackStartRequest::direct_play(
            "movie-1",
            Some("sess-1".to_owned()),
            0,
        ))
        .await
        .unwrap();
    client
        .report_progress(&PlaybackProgressRequest::time_update(
            "movie-1",
            Some("sess-1".to_owned()),
            10_000_000,
            false,
        ))
        .await
        .unwrap();
    client
        .report_stopped(&PlaybackStopRequest::new(
            "movie-1",
            Some("sess-1".to_owned()),
            50_000_000,
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn playback_calls_need_a_session_and_surface_api_errors() {
    let server = MockServer::start().await;
    let anonymous = JellyfinClient::with_memory_store(&server.uri(), identity()).unwrap();
    assert!(matches!(
        anonymous.playback_info("x", None).await,
        Err(Error::NotSignedIn)
    ));
    assert!(matches!(
        anonymous
            .report_stopped(&PlaybackStopRequest::new("x", None, 0))
            .await,
        Err(Error::NotSignedIn)
    ));

    Mock::given(method("POST"))
        .and(path("/Items/missing/PlaybackInfo"))
        .respond_with(ResponseTemplate::new(404).set_body_string("nope"))
        .mount(&server)
        .await;
    let err = signed_in(&server.uri())
        .playback_info("missing", None)
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(404));
}
