# questkit

Reusable Rust crates for building Meta Quest apps. They are extracted from
[DeskQuest](https://github.com/Josh-Archer/DeskQuest) and are used by it and by
[quest-jellyfin-3d](https://github.com/Josh-Archer/quest-jellyfin-3d).

| Crate | What it is |
| --- | --- |
| [`xr-video`](crates/xr-video) | Quest hardware video decoding (MediaCodec), Vulkan resources, Opus/PCM audio with A/V sync, stereo layout and 3D-format detection |
| [`xr-core`](crates/xr-core) | OpenXR instance/session/frame pacing, composition layers, passthrough, hand/controller/keyboard tracking, spatial placement and grab/resize, and the shared Android APK packaging harness |
| [`jellyfin-client`](crates/jellyfin-client) | Headless async Jellyfin client: Quick Connect, library browsing, PlaybackInfo and device profiles, session reporting |

`xr-core` depends on `xr-video`; `jellyfin-client` is independent of both. None depends on DeskQuest.

```toml
[dependencies]
xr-core = "0.1"
xr-video = "0.1"
jellyfin-client = "0.1"
```

## Versioning and releases

All crates share one version (lockstep) and follow [SemVer](https://semver.org); while the
version is `0.x`, a minor bump may break the API. See [RELEASING.md](RELEASING.md) and
[CHANGELOG.md](CHANGELOG.md). The minimum supported Rust version is in `Cargo.toml`
(`rust-version`) and checked in CI.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/ci/check-xr-reuse.sh   # an independent consumer compiles and runs against the crates
cargo package --workspace --locked  # the crates.io tarballs build on their own
```

`audiopus_sys` builds Opus from source, so `cmake` and a C toolchain are required.

## License

MIT OR Apache-2.0.
