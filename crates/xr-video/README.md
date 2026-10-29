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

`format::detect_format` performs pure-Rust stereo/projection detection without
opening files or initializing a decoder. It accepts an optional Jellyfin
`Video3DFormat` string, filename/local path, and normalized `ContainerHints` from
the application's metadata probe. Stereo and projection resolve independently:
Jellyfin first, filename second, container hints third, then mono/flat defaults.
The result records the source of each decision. Unknown Jellyfin values fall
through; Jellyfin's stereo field does not suppress projection hints.

Filename tags are case-insensitive, delimited alphanumeric tokens in the basename
without its extension. Explicit HSBS/FSBS/HOU/FOU/HTAB/FTAB tags take precedence
over SBS/OU/TAB and underscore-prefixed LR/TB (which assume half packing). A lone
3D tag reports `Unspecified3d`, requiring a later user choice. MVC and MVHEVC tags
are also recognized. Projection tags are 180, 360, and fisheye; fisheye wins over
angular tags. Equal-specificity conflicts use the first tag. Directory names,
1080p, and x360 are not tags. Container adapters should supply only known metadata
hints, not guess multiview support from a generic HEVC codec or frame dimensions.

```rust
use xr_video::format::{detect_format, ContainerHints, Projection, StereoFormat};

let format = detect_format(
    Some("FullSideBySide"),
    Some("Vacation_180_LR.mp4"),
    ContainerHints::default(),
);
assert_eq!(format.stereo, StereoFormat::FullSideBySide);
assert_eq!(format.projection, Projection::Equirectangular180);
```

Detection preserves half/full packing and MVC/MV-HEVC distinctions separately
from `StereoLayout`'s texture regions. It does not enable multiview decoding,
projection rendering, or Jellyfin network/playback integration.

Audio traits consume decoded/encoded samples without knowledge of packet formats.
The app owns timing/packet reception and feeds `OpusAudioDecoder`, `AvSynchronizer`
and an `AudioSink`. Mock video/audio implementations support host tests.

`xr-core` owns static OpenXR loader linking when both crates are used together.
Android apps using only this crate's surface bridge can enable `static-loader`.
See [xr-core's README](../xr-core/README.md) for shared Android packaging and NDK
setup. Run `cargo test -p xr-video` for decoder, audio and stereo unit tests.
