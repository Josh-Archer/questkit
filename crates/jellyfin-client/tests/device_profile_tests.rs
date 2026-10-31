use jellyfin_client::{
    evaluate_direct_play, quest_device_profile, BaseItem, DirectPlayDecision, MediaStream,
    MediaStreamType,
};

#[test]
fn test_quest_device_profile_limits() {
    let profile = quest_device_profile();
    assert_eq!(profile.name, "Meta Quest 3");
    assert_eq!(profile.max_streaming_bitrate, 120_000_000);
    assert_eq!(profile.max_static_bitrate, 120_000_000);

    let hevc = profile
        .codec_profiles
        .iter()
        .find(|c| c.codec == "hevc")
        .expect("HEVC profile");
    assert_eq!(hevc.conditions.len(), 3);
    assert_eq!(hevc.conditions[0].property, "Width");
    assert_eq!(hevc.conditions[0].value, "8192");
    assert_eq!(hevc.conditions[1].property, "Height");
    assert_eq!(hevc.conditions[1].value, "4320");
    assert_eq!(hevc.conditions[2].property, "VideoFramerate");
    assert_eq!(hevc.conditions[2].value, "30");

    let av1 = profile
        .codec_profiles
        .iter()
        .find(|c| c.codec == "av1")
        .expect("AV1 profile");
    assert_eq!(av1.conditions.len(), 3);
    assert_eq!(av1.conditions[0].value, "8192");

    let h264 = profile
        .codec_profiles
        .iter()
        .find(|c| c.codec == "h264")
        .expect("H264 profile");
    assert_eq!(h264.conditions.len(), 3);
    assert_eq!(h264.conditions[0].value, "4096");
    assert_eq!(h264.conditions[2].value, "60");
}

#[test]
fn test_direct_play_hevc_8k30_boundary() {
    let profile = quest_device_profile();

    let mut item = BaseItem {
        id: "8k-test".to_string(),
        name: "8K Video".to_string(),
        path: Some("/media/video.mkv".to_string()),
        video_3d_format: None,
        media_streams: vec![MediaStream {
            codec: Some("hevc".to_string()),
            stream_type: Some(MediaStreamType::Video),
            width: Some(7680),
            height: Some(4320),
            real_frame_rate: Some(30.0),
            ..Default::default()
        }],
        container: Some("mkv".to_string()),
        run_time_ticks: None,
        item_type: Some("Movie".to_string()),
        ..Default::default()
    };

    // 8K 30fps: Direct Play
    assert_eq!(
        evaluate_direct_play(&profile, &item),
        DirectPlayDecision::DirectPlay
    );

    // 8K 60fps: Transcode needed due to ~8K30 hardware decoder limit
    item.media_streams[0].real_frame_rate = Some(60.0);
    match evaluate_direct_play(&profile, &item) {
        DirectPlayDecision::TranscodeRequired { reason } => {
            assert!(reason.contains("8K30 hardware cap"), "Reason: {}", reason);
        }
        other => panic!("Expected TranscodeRequired, got {:?}", other),
    }

    // >8192 width: Exceeds 8K resolution limit
    item.media_streams[0].real_frame_rate = Some(24.0);
    item.media_streams[0].width = Some(8196);
    match evaluate_direct_play(&profile, &item) {
        DirectPlayDecision::TranscodeRequired { reason } => {
            assert!(reason.contains("8K limit"), "Reason: {}", reason);
        }
        other => panic!("Expected TranscodeRequired, got {:?}", other),
    }
}

#[test]
fn test_direct_play_unsupported_container_and_codec() {
    let profile = quest_device_profile();

    // Unsupported container (.avi)
    let avi_item = BaseItem {
        id: "avi-test".to_string(),
        name: "Old Video".to_string(),
        path: Some("/media/old.avi".to_string()),
        video_3d_format: None,
        media_streams: vec![MediaStream {
            codec: Some("hevc".to_string()),
            stream_type: Some(MediaStreamType::Video),
            width: Some(1920),
            height: Some(1080),
            ..Default::default()
        }],
        container: Some("avi".to_string()),
        run_time_ticks: None,
        item_type: Some("Movie".to_string()),
        ..Default::default()
    };
    match evaluate_direct_play(&profile, &avi_item) {
        DirectPlayDecision::TranscodeRequired { reason } => {
            assert!(
                reason.contains("Container 'avi' not supported"),
                "Reason: {}",
                reason
            );
        }
        other => panic!("Expected TranscodeRequired, got {:?}", other),
    }

    // Unsupported codec (vc1)
    let vc1_item = BaseItem {
        id: "vc1-test".to_string(),
        name: "VC1 Video".to_string(),
        path: Some("/media/video.mkv".to_string()),
        video_3d_format: None,
        media_streams: vec![MediaStream {
            codec: Some("vc1".to_string()),
            stream_type: Some(MediaStreamType::Video),
            width: Some(1920),
            height: Some(1080),
            ..Default::default()
        }],
        container: Some("mkv".to_string()),
        run_time_ticks: None,
        item_type: Some("Movie".to_string()),
        ..Default::default()
    };
    match evaluate_direct_play(&profile, &vc1_item) {
        DirectPlayDecision::TranscodeRequired { reason } => {
            assert!(
                reason.contains("Video codec 'vc1' not supported"),
                "Reason: {}",
                reason
            );
        }
        other => panic!("Expected TranscodeRequired, got {:?}", other),
    }
}
