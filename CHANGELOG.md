# Changelog

All notable changes to every crate in this workspace. Versions move together.

## Unreleased

- `xr-core`: the Android packaging harness can build for an x86_64 emulator (`APP_ABI=x86_64`) as well as the Quest (`arm64-v8a`, the default).

## 0.1.0

First release. `xr-core` and `xr-video` were extracted from DeskQuest (crates/xr-core and
crates/xr-video) and `jellyfin-client` from quest-jellyfin-3d (crates/jellyfin).

- `jellyfin-client`: the OIDC module is an experimental stub and is not wired into the client.
