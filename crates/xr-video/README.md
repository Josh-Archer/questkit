# xr-video

Reusable AV1/HEVC AMediaCodec decoding, Vulkan resources, stereo texture regions,
Opus/PCM decoding, A/V synchronization, and AAudio output. There are no DeskQuest
protocol, transport, pairing or network dependencies. Packet reception/jitter
buffering and application playback orchestration remain with the application.

```toml
[dependencies]
xr-video = { path = "../DeskQuest/crates/xr-video" }
# Or: xr-video = { git = "https://github.com/Josh-Archer/DeskQuest", rev = "<commit>" }
```

Use `decoder::create_decoder` with `config::DecoderConfig`, dimensions and the
native-window pointer from an OpenXR Android surface swapchain. The app must keep
that window alive while decoding and release the decoder before destroying it.
The `graphics::surface::create_surface_swapchain` bridge is available on Android;
`xr-core` uses it for primary and additional video surfaces.

The existing hardware path is AMediaCodec -> Android native Surface/BufferQueue
(backed by AHardwareBuffer) -> OpenXR compositor. Android and the runtime own the
buffer import and synchronization. This extraction preserves that path; it does
not add an application-managed AHardwareBuffer import/copy stage. Applications
must **not** acquire/wait/release images of an Android surface swapchain.

`StereoLayout::{Mono, SideBySide, OverUnder}::eye_region` returns pixel regions for
the left/right eye. SBS uses left then right; OU uses top then bottom. Reverse the
eye argument for reversed sources. Empty, overflowing and indivisible packed
frames are rejected. The app chooses the corresponding composition layer and
per-eye visibility; DeskQuest's existing mono presentation is unchanged.

Audio traits consume decoded/encoded samples without knowledge of packet formats.
The app owns timing/packet reception and feeds `OpusAudioDecoder`, `AvSynchronizer`
and an `AudioSink`. Mock video/audio implementations support host tests.

`xr-core` owns static OpenXR loader linking when both crates are used together.
Android apps using only this crate's surface bridge can enable `static-loader`.
See [xr-core's README](../xr-core/README.md) for shared Android packaging and NDK
setup. Run `cargo test -p xr-video` for decoder, audio and stereo unit tests.
