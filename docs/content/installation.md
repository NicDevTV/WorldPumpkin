---
title: Installation
description: Install WorldPumpkin on a Pumpkin Minecraft server.
navigation:
  order: 2
---

# Installation

WorldPumpkin builds to a WebAssembly plugin file for Pumpkin.

## Download

Download the release artifact for the current package version.

::artifact-download
::

::artifact-name
::

## Install

1. Stop your Pumpkin server.
2. Copy the `.wasm` file into the server plugin folder.
3. Start the server.
4. Run `/worldpumpkin info` or `/wp info` to check the loaded version.

## Build From Source

This repo uses Rust nightly and the `wasm32-wasip2` target.

```bash
rustup toolchain install nightly --profile minimal --target wasm32-wasip2
cargo build --locked --release --target wasm32-wasip2
```

The built plugin is written to:

```text
target/wasm32-wasip2/release/world_pumpkin.wasm
```
