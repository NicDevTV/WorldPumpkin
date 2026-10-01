---
title: Commands
description: WorldPumpkin command reference.
navigation:
  order: 3
---

# Commands

WorldPumpkin supports selection, edit, history, and administration commands.

## Selection

| Command | Description |
| --- | --- |
| `//pos1` | Set the first selection position to your current block position. |
| `//pos1 <x y z>` | Set the first selection position to explicit coordinates. |
| `//pos2` | Set the second selection position to your current block position. |
| `//pos2 <x y z>` | Set the second selection position to explicit coordinates. |
| `//hpos1` | Set the first position to the block you are looking at. |
| `//hpos2` | Set the second position to the block you are looking at. |
| `//chunk` | Select the current chunk from world minimum height to the highest detected chunk block. |
| `//expand <amount>` | Expand in the direction you are facing. |
| `//expand <amount> <direction>` | Expand in a specific direction. |
| `//expand <amount> <reverseAmount> <direction>` | Expand in both directions on one axis. |
| `//expand vert` | Expand from world minimum height to world maximum height. |

Directions are `north`, `south`, `east`, `west`, `up`, and `down`. Short forms are `n`, `s`, `e`, `w`, `u`, and `d`.

## Edits

| Command | Description |
| --- | --- |
| `//set <block>` | Fill the selected cuboid with a block. |
| `//replace <from> <to>` | Replace matching blocks in the selection. |
| `//walls <block>` | Build the outer walls of the selection. |
| `//move <amount> [direction]` | Move the selection contents, defaulting to the direction you are facing. |

Block arguments are resolved through the running server's block registry. Examples:
`stone`, `minecraft:stone`, or `oak_stairs[facing=north,half=top]`. Properties can
be given in any order; omitted properties use the server's defaults. Weighted
patterns work too: `//set 50%stone,50%dirt`.

## History

| Command | Description |
| --- | --- |
| `//undo` | Undo your latest WorldPumpkin edit in the current world. |
| `//redo` | Redo your latest undone WorldPumpkin edit in the current world. |

Undo and redo are per player and must be used in the same world as the original edit.
They restore block-entity NBT, including inventories and sign text, for `set`,
`replace`, `walls`, and `move`. History is limited by the configuration and is
lost when the server restarts.

## Administration

| Command | Description |
| --- | --- |
| `/worldpumpkin info`, `/wp info` | Show plugin version, authors, and Pumpkin API source. |
| `/worldpumpkin status`, `/wp status` | Show queue status, configured limits, edit speed, fast mode, and server TPS. |
| `/worldpumpkin reload`, `/wp reload` | Reload `config.toml`. |
