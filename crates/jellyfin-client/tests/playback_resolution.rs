use jellyfin_client::{quest_device_profile, resolve_playback, PlayMethod, PlaybackInfoResponse};
use serde_json::json;

fn info(sources: serde_json::Value) -> PlaybackInfoResponse {
    serde_json::from_value(json!({"MediaSources":sources})).unwrap()
}
#[test]
fn profile_advertises_hls_transcoding() {
    let profile = serde_json::to_value(quest_device_profile()).unwrap();
    assert_eq!(profile["TranscodingProfiles"][0]["Protocol"], "hls");
    assert_eq!(profile["TranscodingProfiles"][0]["VideoCodec"], "h264");
}
#[test]
fn selects_direct_play_then_remux_then_transcode() {
    let res = resolve_playback("https://example.org/jellyfin", "item", &info(json!([
        {"Id":"trans", "SupportsTranscoding":true, "TranscodingUrl":"/Videos/item/master.m3u8?api_key=secret"},
        {"Id":"direct", "SupportsDirectPlay":true}
    ]))).unwrap();
    assert_eq!(res.method, PlayMethod::DirectPlay);
    assert_eq!(res.source_id, "direct");
    let res = resolve_playback("https://example.org/jellyfin", "item", &info(json!([
        {"Id":"s", "SupportsDirectStream":true, "DirectStreamUrl":"/Videos/item/stream?api_key=secret&x=1"}
    ]))).unwrap();
    assert_eq!(res.method, PlayMethod::DirectStream);
    assert_eq!(
        res.url.as_str(),
        "https://example.org/jellyfin/Videos/item/stream?x=1"
    );
    let res = resolve_playback("https://example.org/jellyfin", "item", &info(json!([
        {"Id":"s", "SupportsTranscoding":true, "TranscodingUrl":"/Videos/item/master.m3u8?api_key=secret&DeviceId=d"}
    ]))).unwrap();
    assert_eq!(res.method, PlayMethod::Transcode);
    assert_eq!(
        res.url.as_str(),
        "https://example.org/jellyfin/Videos/item/master.m3u8?DeviceId=d"
    );
}
#[test]
fn rejects_untrusted_urls_before_authorization_is_attached() {
    for url in [
        "https://evil.example/steal",
        "//evil.example/path",
        "https://example.org/other",
        "../../other",
        "https://u:p@example.org/jellyfin/stream",
        "/Videos/x#fragment",
    ] {
        let response = info(json!([{"Id":"s", "SupportsTranscoding":true, "TranscodingUrl":url}]));
        assert!(
            resolve_playback("https://example.org/jellyfin", "item", &response).is_err(),
            "{url}"
        );
    }
    assert!(resolve_playback("https://example.org/jellyfin", "item", &info(json!([]))).is_err());
}
