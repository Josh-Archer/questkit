# Releasing

All crates share the workspace version in `Cargo.toml`.

1. Move the `Unreleased` entries in `CHANGELOG.md` under the new version.
2. Set `[workspace.package] version` in `Cargo.toml`, and the `version = "..."` on the
   `xr-video` dependency in `crates/xr-core/Cargo.toml` to match.
3. Run `cargo update -w` so `Cargo.lock` carries the new version, and open a PR. CI must pass,
   including `cargo package --workspace --locked`.
4. After it merges, tag the merge commit and push the tag:
   `git tag v0.2.0 && git push origin v0.2.0`.

The `Release` workflow checks that the tag equals the workspace version, tests, publishes
`xr-video`, `xr-core`, `jellyfin-client` in that order (skipping versions already on crates.io,
so it can be re-run), and creates a GitHub release.

## One-time setup

- Add the `CARGO_REGISTRY_TOKEN` secret, scoped to publishing these crates, to the `crates-io`
  environment. Create the token at <https://crates.io/settings/tokens>.
- After the first publish, switch each crate to [trusted publishing](https://crates.io/docs/trusted-publishing)
  and drop the long-lived token.
- CI and releases run on the `home-gh-runner-questkit` ARC runner scale set (see `.github/workflows`); pull requests from forks use GitHub-hosted runners so untrusted code never reaches the cluster.
