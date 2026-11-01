# Releasing

Each crate has its own version, changelog and tag. Releases are automated with
[release-plz](https://release-plz.dev) (`release-plz.toml`, `.github/workflows/release-plz.yml`).

## How a release happens

1. Merge changes to `main` with [conventional commit](https://www.conventionalcommits.org) messages
   (`fix:`, `feat:`, `feat!:` or a `BREAKING CHANGE:` footer). Scope them to the crate when it helps,
   for example `fix(xr-core): ...`.
2. release-plz opens (and keeps updating) one **release PR**. For each crate that changed it bumps
   the version according to SemVer, updates the version requirements of dependents inside the
   workspace, and writes `crates/<name>/CHANGELOG.md`. Crates that did not change are left alone.
3. Review and merge the release PR. On that merge the `release` job publishes every crate whose
   version is not yet on crates.io, **in dependency order**, then creates the tag
   `<crate>-v<version>` and a GitHub release for each.

Before 1.0, a breaking change bumps the minor version and other changes bump the patch version.

## Adding a crate

Add the directory under `crates/` and list it in the workspace `members` in `Cargo.toml`. Nothing
else names crates: release-plz discovers them. Give it `description`, `license`, `repository`,
`readme` and an explicit `version`. Use `publish = false` in its `[package]` to keep it unpublished.
Before its first release, reserve nothing by hand: the first release PR publishes it as the
version in its `Cargo.toml`. Check the name is free on crates.io first.

## One-time setup

- `CARGO_REGISTRY_TOKEN` in the `crates-io` environment: a crates.io token with the
  `publish-new` and `publish-update` scopes. After a crate's first publish, switch it to
  [trusted publishing](https://crates.io/docs/trusted-publishing) and drop the long-lived token.
- `RELEASE_PLZ_TOKEN` (optional but recommended): a fine-grained PAT or GitHub App token with
  contents and pull-requests write. Pull requests opened with the default `GITHUB_TOKEN` do not
  run CI, so without it the release PR has no checks.
- The workflows run on the shared `uecb-shared` runner pool.
- Optional: install `cargo-semver-checks` on the runner and set `semver_check = true` in
  `release-plz.toml` to fail a release that breaks a crate's public API without a major bump.
