---
title: Development
description: Build, size, CI and release workflow for WorldPumpkin.
navigation:
  order: 6
---

# Development

## Build

Use a rustup-managed Cargo with the toolchain from
[Build From Source](/installation#build-from-source), so `rust-toolchain.toml` and the
`wasm32-wasip2` target apply before running these commands.

```bash
rustup component add rustfmt clippy
cargo test --locked --lib
cargo clippy --locked --target wasm32-wasip2 --all-targets -- -D warnings
cargo build --locked --release --target wasm32-wasip2
```

The release profile is tuned for a small plugin file: `opt-level = "z"`,
fat LTO, one codegen unit, `panic = "abort"` and stripped symbols. The Wasm file
is about 770 KiB. Other opt levels were measured and are larger (`s` about
806 KiB, `3` about 843 KiB).

## Version Names

Local and CI builds use a version such as `0.1.0-dev.ge7bc7c2`. The base is the
newer of the package version and the nearest version tag; the suffix is the Git
commit. Without Git the suffix is `dev.0`.

## CI

| Workflow | Trigger | What it does |
| --- | --- | --- |
| CI | push to `main`, pull requests | Format, tests, clippy, Wasm build, size report, artifact upload. |
| Build Release | manual | Builds a release and creates or updates a draft; publishes only when `publish` is enabled (see below). |
| Deploy Wiki | push to `docs/**`, release published, manual | Builds the Nuxt wiki and deploys it to GitHub Pages. |

## Releasing

1. Open **Actions, Build Release, Run workflow**.
2. Enter a tag such as `v0.1.0` or `v0.2.0-rc.1`. Tags with a `-` suffix become
   prereleases.
3. Leave **publish** off to get a draft you can review, or turn it on to publish
   after the build. Publishing runs in a separate job that waits for approval of
   the `release` environment, then triggers the wiki deploy.

The workflow sets the version in `Cargo.toml` and `Cargo.lock` in its checkout,
runs all checks, builds the Wasm file and attaches `world_pumpkin.wasm` plus
`world_pumpkin.wasm.sha256` to the release with generated notes. The run summary
shows the file size and hash. Rerunning updates an existing draft; published
releases are never replaced. Before building, the workflow checks that an
existing tag points at the built commit. Before publishing, the publish job
downloads the draft assets, verifies the checksum and compares the Wasm hash with
the one the build job produced, so only the built file can be published. Rerunning with `publish` for an already published release only retries the
wiki deploy, which always builds from `main`.

From the command line:

```bash
gh workflow run build.yml -f tag=v0.1.0 -f publish=false
gh workflow run docs.yml
```

## Dependency Updates

Dependabot checks Cargo (the plugin and the release version tool), the wiki's npm
packages and GitHub Actions every Monday. Minor and patch updates are grouped into one
pull request per ecosystem, and new releases wait three days (seven for major
versions) before being proposed. Security updates are not delayed. The Pumpkin plugin
API is pinned to a commit in `Cargo.toml` and is bumped by hand.

## Pumpkin Market

GitHub Releases are the only publishing channel. [Pumpkin Market](https://market.pumpkinmc.org)
is not synced yet: its documented REST API (`/api/v1/rest`) can search, check updates
and download, but has no endpoint to publish a version. An automatic sync workflow
will be added once the market offers one.

## Repository Protection

The rulesets are active on the repository, and their definitions are kept in
`.github/rulesets/` for reference and for re-importing (Settings, Rules, Rulesets,
Import ruleset).

- **Protect main:** changes need a pull request with one approval, a passing `Rust` check, squash
  merges only, linear history, no force pushes or deletion. Admins cannot push
  directly; they can only bypass the checks inside a pull request.
  Paths that can change the build or the protection itself (`.github/`,
  `.coderabbit.yaml`, `SECURITY.md`, `build.rs`, `build/`, `tools/`) are listed in
  `CODEOWNERS` and need the owner's approval; a review bot approval does not
  count for them. This stops a pull request from loosening its own review rules.
- **Protect release tags:** `v*` tags cannot be moved or deleted. Creating them is
  allowed so the release workflow can tag.

Secret scanning with push protection, Dependabot alerts and security updates are
enabled, and workflows get read-only tokens by default.

Publishing goes through the `release` environment: it requires an approval and only
runs from `main`. Immutable releases are enabled, so once a release is published its
tag and assets can no longer be changed or deleted.
