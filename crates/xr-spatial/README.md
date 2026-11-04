# xr-spatial

Pure-Rust spatial placement math for XR screens: poses (`Vec3`, `Quat`, `ScreenPose`),
`ScreenTransform`, ray/screen intersection, grab and resize interaction, scroll
handling, keyboard placement/summon logic, desk placement and size limits.

It has no networking, rendering or protocol dependencies. OpenXR conversions
(`From` impls for `openxr::Posef`/`Vector3f`/`Quaternionf`, quad/cylinder layer
parameters and `DesktopLayerConfig`) are behind the optional `openxr` feature,
which is off by default and does not enable OpenXR's dynamic loader.

```toml
[dependencies]
xr-spatial = "0.1"
# with OpenXR conversions:
# xr-spatial = { version = "0.1", features = ["openxr"] }
```

`xr-core` re-exports these types under `xr_core::spatial`, so existing users of
`xr-core` need no changes.
