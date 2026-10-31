//! QuickConnect and library browse against a local mock of the Jellyfin HTTP API.
use std::sync::Arc;
use std::time::Duration;

use jellyfin_client::{
    ClientIdentity, Error, ItemsQuery, JellyfinClient, MediaStreamType, MemoryTokenStore,
    QuickConnectPoll, StoredSession, TokenStore, Video3DFormat,
};
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn identity() -> ClientIdentity {
    ClientIdentity::new("DeskQuest", "Quest 3", "device-1", "0.1.0").unwrap()
}

fn client(base_url: &str) -> (JellyfinClient, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    let client = JellyfinClient::new(base_url, identity(), store.clone()).unwrap();
    (client, store)
}

fn header(request: &Request) -> String {
    request
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned()
}

fn unsigned_header() -> &'static str {
    "MediaBrowser Client=\"DeskQuest\", Device=\"Quest%203\", DeviceId=\"device-1\", Version=\"0.1.0\""
}

fn challenge() -> serde_json::Value {
    json!({
        "Secret": "secret-1",
        "Code": "482913",
        "Authenticated": false,
        "DateAdded": "2026-10-29T00:00:00Z",
        "DeviceId": "device-1",
        "DeviceName": "Quest 3",
        "AppName": "DeskQuest",
        "AppVersion": "0.1.0"
    })
}

fn authorized_state() -> serde_json::Value {
    json!({
        "Secret": "secret-1",
        "Code": "482913",
        "Authenticated": true
    })
}

fn auth_result() -> serde_json::Value {
    json!({
        "AccessToken": "test-token",
        "ServerId": "server-1",
        "User": {"Id": "user-1", "Name": "josh", "ServerId": "server-1"},
        "SessionInfo": {"Id": "session-1"}
    })
}

struct ConnectThenAuthorize {
    hits: std::sync::atomic::AtomicUsize,
}

impl Respond for ConnectThenAuthorize {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let hit = self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if hit == 0 {
            ResponseTemplate::new(200).set_body_json(challenge())
        } else {
            ResponseTemplate::new(200).set_body_json(authorized_state())
        }
    }
}

#[tokio::test]
async fn quick_connect_polls_then_stores_the_exchanged_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/QuickConnect/Initiate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .and(query_param("secret", "secret-1"))
        .respond_with(ConnectThenAuthorize {
            hits: std::sync::atomic::AtomicUsize::new(0),
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateWithQuickConnect"))
        .and(body_json(json!({"Secret": "secret-1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(auth_result()))
        .expect(1)
        .mount(&server)
        .await;

    let (client, store) = client(&server.uri());
    let started = client.initiate_quick_connect().await.unwrap();
    assert_eq!(started.code, "482913");
    assert_eq!(started.secret, "secret-1");

    let pending = client.poll_quick_connect(&started.secret).await.unwrap();
    assert!(matches!(pending, QuickConnectPoll::Pending { code } if code == "482913"));
    assert!(client.session().is_none());

    let authorized = client.poll_quick_connect(&started.secret).await.unwrap();
    let QuickConnectPoll::Authorized(session) = authorized else {
        panic!("expected an access token after the server authorized the code");
    };
    assert_eq!(session.access_token, "test-token");
    assert_eq!(session.user_id, "user-1");
    assert_eq!(session.user_name, "josh");
    assert_eq!(session.server_id, "server-1");
    assert_eq!(session.device_id, "device-1");
    assert_eq!(session.server_url, server.uri());
    assert_eq!(store.load().unwrap().unwrap().access_token, "test-token");
    let debug = format!("{session:?}");
    assert!(!debug.contains("test-token"));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(header(&requests[0]), unsigned_header());
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(requests[0].url.path(), "/QuickConnect/Initiate");
    assert!(requests[0].body.is_empty());
    assert_eq!(header(&requests[1]), unsigned_header());
    assert_eq!(header(&requests[2]), unsigned_header());
    assert_eq!(header(&requests[3]), unsigned_header());
    assert_eq!(
        requests[3].url.path(),
        "/Users/AuthenticateWithQuickConnect"
    );
}

#[tokio::test]
async fn wait_for_quick_connect_retries_until_authorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .respond_with(ConnectThenAuthorize {
            hits: std::sync::atomic::AtomicUsize::new(0),
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateWithQuickConnect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(auth_result()))
        .mount(&server)
        .await;

    let (client, _) = client(&server.uri());
    let session = client
        .wait_for_quick_connect("secret-1", Duration::from_millis(1), 2)
        .await
        .unwrap();
    assert_eq!(session.access_token, "test-token");
}

#[tokio::test]
async fn wait_for_quick_connect_times_out_while_the_code_is_pending() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge()))
        .mount(&server)
        .await;
    let (client, store) = client(&server.uri());
    let error = client
        .wait_for_quick_connect("secret-1", Duration::ZERO, 1)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::QuickConnectTimeout { attempts: 1 }));
    assert!(store.load().unwrap().is_none());
}

#[tokio::test]
async fn disabled_quick_connect_and_unknown_secret_are_distinct() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/QuickConnect/Initiate"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Quick connect is disabled"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Unknown secret"))
        .mount(&server)
        .await;

    let (client, _) = client(&server.uri());
    let disabled = client.initiate_quick_connect().await.unwrap_err();
    assert!(matches!(disabled, Error::QuickConnectDisabled));
    assert_eq!(disabled.status(), Some(401));
    let missing = client
        .poll_quick_connect("missing-secret")
        .await
        .unwrap_err();
    assert!(matches!(missing, Error::UnknownSecret));
    assert!(!missing.to_string().contains("missing-secret"));
}

#[tokio::test]
async fn transport_errors_do_not_include_the_quick_connect_secret() {
    let client = JellyfinClient::with_memory_store("http://127.0.0.1:9", identity()).unwrap();
    let error = client
        .poll_quick_connect("super-secret-value")
        .await
        .unwrap_err();
    let rendered = error.to_string();
    assert!(matches!(error, Error::Transport(_)));
    assert!(!rendered.contains("super-secret-value"));
    assert!(rendered.contains("<redacted>"));
}

#[tokio::test]
async fn items_request_pages_3d_fields_and_sends_the_stored_token() {
    let server = MockServer::start().await;
    let page = |start: i64, name: &str, layout: &str| {
        json!({
            "Items": [{
                "Name": name,
                "Id": format!("item-{start}"),
                "Type": "Movie",
                "Path": format!("/media/{name}.mkv"),
                "ParentId": "library-1",
                "IsFolder": false,
                "MediaType": "Video",
                "Container": "mkv",
                "ProductionYear": 2009,
                "RunTimeTicks": 1000,
                "Overview": null,
                "Video3DFormat": layout,
                "MediaStreams": [
                    {
                        "Index": 0,
                        "Type": "Video",
                        "Codec": "hevc",
                        "Width": 3840,
                        "Height": 1080,
                        "AverageFrameRate": 24.0,
                        "BitRate": 20_000_000,
                        "BitDepth": 10,
                        "VideoRange": "SDR",
                        "IsDefault": true,
                        "Language": null
                    },
                    {
                        "Index": 1,
                        "Type": "Audio",
                        "Codec": "aac",
                        "Channels": 6,
                        "Language": "eng",
                        "IsDefault": true
                    }
                ]
            }],
            "StartIndex": start,
            "TotalRecordCount": 2
        })
    };
    Mock::given(method("GET"))
        .and(path("/jellyfin/Items"))
        .and(query_param("StartIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(0, "Avatar", "FullSideBySide")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jellyfin/Items"))
        .and(query_param("StartIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(1, "Sphere", "MVC")))
        .mount(&server)
        .await;

    let base = format!("{}/jellyfin/", server.uri());
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: base.clone(),
            access_token: "test-token".to_owned(),
            user_id: "user-1".to_owned(),
            user_name: "josh".to_owned(),
            server_id: "server-1".to_owned(),
            device_id: "device-1".to_owned(),
        })
        .unwrap();
    let client = JellyfinClient::new(&base, identity(), store.clone()).unwrap();
    assert_eq!(client.base_url(), format!("{}/jellyfin", server.uri()));
    assert_eq!(client.access_token().as_deref(), Some("test-token"));

    let first = client
        .list_items(&ItemsQuery {
            limit: 1,
            parent_id: Some("library-1".to_owned()),
            include_item_types: vec!["Movie".to_owned(), "Video".to_owned()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(first.has_more());
    assert_eq!(first.next_start_index(), Some(1));
    assert_eq!(first.total_record_count, 2);
    assert_eq!(first.items[0].name, "Avatar");
    assert_eq!(first.items[0].path.as_deref(), Some("/media/Avatar.mkv"));
    assert_eq!(
        first.items[0].video_3d_format,
        Some(Video3DFormat::FullSideBySide)
    );
    let video = first.items[0].video_stream().unwrap();
    assert_eq!(video.codec.as_deref(), Some("hevc"));
    assert_eq!(video.width, Some(3840));
    assert_eq!(video.height, Some(1080));
    assert_eq!(video.bit_depth, Some(10));
    assert_eq!(
        first.items[0].media_streams[1].stream_type,
        Some(MediaStreamType::Audio)
    );

    let second = client
        .list_items(&ItemsQuery {
            start_index: first.next_start_index().unwrap(),
            limit: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!second.has_more());
    assert_eq!(second.items[0].video_3d_format, Some(Video3DFormat::Mvc));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            header(request),
            format!("{}, Token=\"test-token\"", unsigned_header())
        );
        let query: Vec<(String, String)> = request
            .url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert!(query.contains(&(
            "Fields".to_owned(),
            "Video3DFormat,MediaStreams,Path".to_owned()
        )));
        assert!(query.contains(&("Limit".to_owned(), "1".to_owned())));
        assert!(query.contains(&("UserId".to_owned(), "user-1".to_owned())));
        assert!(query.contains(&("Recursive".to_owned(), "false".to_owned())));
    }
    assert!(requests[0]
        .url
        .query_pairs()
        .any(|(key, value)| key == "ParentId" && value == "library-1"));
    assert!(requests[0]
        .url
        .query_pairs()
        .any(|(key, value)| key == "IncludeItemTypes" && value == "Movie,Video"));
    assert_eq!(requests[0].url.path(), "/jellyfin/Items");

    client.clear_session().unwrap();
    assert!(client.session().is_none());
    assert!(store.load().unwrap().is_none());
    let signed_out = client.list_items(&ItemsQuery::default()).await.unwrap_err();
    assert!(matches!(signed_out, Error::NotSignedIn));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn camel_case_item_page_deserializes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "name": "Dome",
                "id": "item-9",
                "type": "Video",
                "path": "/media/dome.mkv",
                "video3DFormat": "HalfTopAndBottom",
                "mediaStreams": [{"index": 0, "type": "Subtitle", "codec": "subrip"}]
            }],
            "startIndex": 0,
            "totalRecordCount": 1
        })))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: server.uri(),
            access_token: "test-token".to_owned(),
            user_id: String::new(),
            user_name: String::new(),
            server_id: String::new(),
            device_id: "device-1".to_owned(),
        })
        .unwrap();
    let client = JellyfinClient::new(&server.uri(), identity(), store).unwrap();
    let page = client.list_items(&ItemsQuery::default()).await.unwrap();
    assert_eq!(
        page.items[0].video_3d_format,
        Some(Video3DFormat::HalfTopAndBottom)
    );
    assert_eq!(
        page.items[0].media_streams[0].stream_type,
        Some(MediaStreamType::Subtitle)
    );
    let query: Vec<(String, String)> = server.received_requests().await.unwrap()[0]
        .url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert!(!query.iter().any(|(key, _)| key == "UserId"));
}

#[tokio::test]
async fn a_failed_token_save_does_not_sign_the_client_in() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(authorized_state()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateWithQuickConnect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(auth_result()))
        .mount(&server)
        .await;

    let store = Arc::new(FailStore);
    let client = JellyfinClient::new(&server.uri(), identity(), store).unwrap();
    let error = client.poll_quick_connect("secret-1").await.unwrap_err();
    assert!(matches!(error, Error::Store(_)));
    assert!(client.session().is_none());
}

#[tokio::test]
async fn a_store_that_cannot_load_refuses_to_construct() {
    let error = JellyfinClient::new("http://127.0.0.1:8096", identity(), Arc::new(LoadFailStore))
        .unwrap_err();
    assert!(matches!(error, Error::Store(_)));
}

#[tokio::test]
async fn list_items_without_a_token_does_not_open_a_connection() {
    let client = JellyfinClient::with_memory_store("http://127.0.0.1:9", identity()).unwrap();
    let error = client.list_items(&ItemsQuery::default()).await.unwrap_err();
    assert!(matches!(error, Error::NotSignedIn));
}

#[tokio::test]
async fn an_authorized_poll_without_an_access_token_is_a_missing_field() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/QuickConnect/Connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(authorized_state()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateWithQuickConnect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"User": {"Name": "josh"}})))
        .mount(&server)
        .await;
    let (client, _) = client(&server.uri());
    let error = client.poll_quick_connect("secret-1").await.unwrap_err();
    assert!(matches!(error, Error::MissingField("AccessToken")));
}

struct LoadFailStore;

impl TokenStore for LoadFailStore {
    fn load(&self) -> Result<Option<StoredSession>, jellyfin_client::StoreError> {
        Err(jellyfin_client::StoreError::new("unreadable"))
    }

    fn save(&self, _: &StoredSession) -> Result<(), jellyfin_client::StoreError> {
        Ok(())
    }

    fn clear(&self) -> Result<(), jellyfin_client::StoreError> {
        Ok(())
    }
}

struct FailStore;

impl TokenStore for FailStore {
    fn load(&self) -> Result<Option<StoredSession>, jellyfin_client::StoreError> {
        Ok(None)
    }

    fn save(&self, _: &StoredSession) -> Result<(), jellyfin_client::StoreError> {
        Err(jellyfin_client::StoreError::new("disk full"))
    }

    fn clear(&self) -> Result<(), jellyfin_client::StoreError> {
        Ok(())
    }
}

#[test]
fn saved_token_is_bound_to_server_and_device() {
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: "https://example.org/jellyfin".into(),
            access_token: "private".into(),
            user_id: "u".into(),
            user_name: "n".into(),
            server_id: "s".into(),
            device_id: "device-1".into(),
        })
        .unwrap();
    let other =
        JellyfinClient::new("https://example.org/other", identity(), store.clone()).unwrap();
    assert!(other.access_token().is_none());
    let other_device = ClientIdentity::new("DeskQuest", "Quest 3", "device-2", "0.1.0").unwrap();
    let other =
        JellyfinClient::new("https://example.org/jellyfin", other_device, store.clone()).unwrap();
    assert!(other.access_token().is_none());
    let same = JellyfinClient::new("https://example.org/jellyfin/", identity(), store).unwrap();
    assert_eq!(same.access_token().as_deref(), Some("private"));
}

#[tokio::test]
async fn api_does_not_forward_credentials_on_redirect() {
    let server = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/stolen", destination.uri())),
        )
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: server.uri(),
            access_token: "token".into(),
            user_id: "user".into(),
            user_name: "viewer".into(),
            server_id: "s".into(),
            device_id: "device-1".into(),
        })
        .unwrap();
    let client = JellyfinClient::new(&server.uri(), identity(), store).unwrap();
    assert!(client.list_items(&ItemsQuery::default()).await.is_err());
    assert!(destination.received_requests().await.unwrap().is_empty());
}

#[test]
fn server_url_rejects_embedded_credentials_queries_and_fragments() {
    for address in [
        "https://user:secret@example.com",
        "https://example.com/?token=secret",
        "https://example.com/#secret",
    ] {
        let err = JellyfinClient::with_memory_store(address, identity())
            .unwrap_err()
            .to_string();
        assert!(!err.contains("secret"));
    }
}

#[tokio::test]
async fn primary_poster_uses_authenticated_api_and_handles_missing_images() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: server.uri(),
            access_token: "test-token".into(),
            user_id: "user-1".into(),
            user_name: "user".into(),
            server_id: "server".into(),
            device_id: "device-1".into(),
        })
        .unwrap();
    let client = JellyfinClient::new(&server.uri(), identity(), store).unwrap();
    Mock::given(method("GET"))
        .and(path("/Items/item-1/Images/Primary"))
        .and(query_param("maxWidth", "400"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1, 2, 3]))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items/missing/Images/Primary"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client.primary_image("item-1").await.unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(client.primary_image("missing").await.unwrap(), None);
    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| header(request).contains("Token=\"test-token\"")));
    let unsigned = JellyfinClient::with_memory_store(&server.uri(), identity()).unwrap();
    assert!(matches!(
        unsigned.primary_image("item-1").await,
        Err(Error::NotSignedIn)
    ));
}

#[tokio::test]
async fn poster_rejects_oversized_response() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(&StoredSession {
            server_url: server.uri(),
            access_token: "test-token".into(),
            user_id: "user-1".into(),
            user_name: "user".into(),
            server_id: "server".into(),
            device_id: "device-1".into(),
        })
        .unwrap();
    let client = JellyfinClient::new(&server.uri(), identity(), store).unwrap();
    Mock::given(method("GET"))
        .and(path("/Items/large/Images/Primary"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0; 8 * 1024 * 1024 + 1]))
        .mount(&server)
        .await;
    assert!(client.primary_image("large").await.is_err());
}
