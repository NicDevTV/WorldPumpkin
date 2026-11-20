---
title: Installation
description: Install WorldPumpkin on a Pumpkin Minecraft server.
navigation:
  order: 2
---

# Installation

WorldPumpkin builds to a WebAssembly plugin file for Pumpkin.

The current source targets Pumpkin `0.2.2+26.3-26.60` at revision
`10f1fde9367c827f168eaabdc8f343b1ce6f676c`. Use a server with a compatible
plugin API. Block names and state IDs are resolved from the running server.
Check the compatibility notes for the release you download.

## Download

Download the release artifact for the current package version.

::artifact-download
::

::artifact-name
::

Releases built with the current release workflow also have a `world_pumpkin.wasm.sha256`
file. Verify the download with `sha256sum -c world_pumpkin.wasm.sha256`. Older releases,
including v0.1.0, have no checksum file; compare against the digest GitHub shows for
the asset instead.

## Install

1. Stop your Pumpkin server.
2. Copy the `.wasm` file into the server plugin folder.
3. Start the server.
4. Run `/worldpumpkin info` or `/wp info` to check the loaded version.

## Build From Source

This repo uses stable Rust (1.96 or newer) and the `wasm32-wasip2` target.

```bash
rustup toolchain install stable --profile minimal --target wasm32-wasip2
cargo build --locked --release --target wasm32-wasip2
```

The built plugin is written to:

```text
target/wasm32-wasip2/release/world_pumpkin.wasm
```
