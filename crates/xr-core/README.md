# xr-core

Reusable OpenXR 0.22 instance/loader/session setup, frame pacing, Vulkan composition,
`XR_FB_passthrough`, hand/controller and keyboard tracking, and spatial placement.
The crate has no DeskQuest protocol, transport, pairing, or host dependencies.
`xr-video` supplies the Vulkan resources and Android surface swapchain bridge used by
this runtime. Desktop/headless callers can use `OpenXrContext::new_simulated`.

```toml
[dependencies]
xr-core = "0.1"
xr-video = "0.1"
```

Create an `XrConfig` with your application name and projection settings. On Android,
register the VM with `ndk-context`, then use `unsafe OpenXrContext::try_new_android`
with the actual Activity reference. Keep that JNI global reference alive until the
OpenXR context is dropped. Poll events, wait for a running session, and create the
swapchains before binding their native windows to media decoders. Use the existing
frame loop for pacing; submit an empty frame when `should_render` is false.
The client facade in `crates/client/src/openxr/context.rs` shows the integration.

Quad and cylinder builders consume `ScreenTransform` or layer configuration.
`build_equirect_layer` is an **opt-in stub** for 180/360 video: it constructs an
`XR_KHR_composition_layer_equirect2` layer using an application-supplied sub-image
and eye visibility. The application must enable that extension and supply its own
video submission path. DeskQuest does not enable or submit equirect layers.

Applications supply their own RGBA view-space overlays and optional billboard
textures. DeskQuest keeps HUD/settings rendering, connection prompt text, host
arrangements and mouse-event conversion in its client crate.

## Android packaging

The `android/` directory ships a shared `build.sh`, a native-activity manifest
template and NDK CMake toolchain. Use these files from a source checkout when using
path or git dependencies; Cargo does not run this packaging harness automatically.
Install cargo-ndk, JDK 17, SDK build-tools/platform 34, NDK 27.2.12479018, and the
`aarch64-linux-android` Rust target. For example, from your app repository:

```bash
export APP_DIR="$PWD/crates/player"             # cdylib crate containing Cargo.toml
export APP_LIB_NAME=video_player                # libvideo_player.so
export APP_PACKAGE=org.example.player
export APP_LABEL='Video Player'
export APP_RES_DIR="$APP_DIR/android/res"       # mipmap-nodpi/ic_launcher.png
export ANDROID_SDK_ROOT="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/27.2.12479018"
bash "$(dirname "$(cargo metadata --format-version 1 | python3 -c 'import json,sys;print(next(p["manifest_path"] for p in json.load(sys.stdin)["packages"] if p["name"]=="xr-core"))')")/android/build.sh" --release
```

The default manifest uses `android.app.NativeActivity`; export the appropriate
`android_main`/NativeActivity entry point from your cdylib. For Java activity apps,
set `APP_ACTIVITY` and put Java sources in `$APP_DIR/android/src`. Override
`APP_MANIFEST` with a complete app manifest or an `.in` template using the provided
`@APP_*@` tokens. Other overrides: `APP_CARGO_FEATURES` (comma-separated),
`APP_APK_NAME`, `APP_APK_OUT_DIR`, `APP_JAVA_DIR`, `APP_KEYSTORE`, and
`APP_MASTER_LOGO`. `CARGO_TARGET_DIR` controls native compilation output. The APK
defaults to `$APP_DIR/target/android-apk/<name>-release.apk`; signing uses a persistent
debug keystore per app. A consumer supplies its own Cargo.lock for the locked build.

The crate enables `openxr`'s **static** feature only on Android, compiling the
Khronos loader into the app's cdylib. No separate `libopenxr_loader.so` is needed.
The shared toolchain sets API 30 and the OpenXR header installation directory;
`build.sh` also provides the CMake policy setting needed by bundled Opus. Other
platforms use the system OpenXR loader dynamically. If integrating without the
harness, supply those same CMake toolchain/policy environment variables yourself.

Run `cargo test -p xr-core` for pure spatial, tracking, pacing and passthrough tests.
`bash scripts/ci/check-xr-reuse.sh` at the repository root exercises an independent
consumer and checks its full resolved graph for DeskQuest dependencies. Hardware
validation belongs to the app's Quest QA process.
