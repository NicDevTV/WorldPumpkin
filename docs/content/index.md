---
title: WorldPumpkin
description: Documentation for the WorldPumpkin Pumpkin server plugin.
navigation:
  title: Overview
  order: 1
---

# WorldPumpkin

WorldPumpkin is a small world editing plugin for Pumpkin Minecraft servers.

It provides WorldEdit-like selection commands, queued block edits, undo and redo history, and configurable edit limits.

## Quick Start

1. Download the latest `world_pumpkin.wasm` release artifact.
2. Put the plugin file into your Pumpkin server plugin folder.
3. Start or restart the server.
4. Select two positions with `//pos1` and `//pos2`.
5. Run an edit command such as `//set stone`.

## Common Workflow

```text
//pos1
//pos2
//set stone
//undo
//redo
```

Commands can be typed with the double-slash form shown above. WorldPumpkin also registers slash commands internally, so command completion can work where Pumpkin exposes it.

## Pages

- [Installation](/installation)
- [Commands](/commands)
- [Configuration](/configuration)
- [Examples](/examples)
