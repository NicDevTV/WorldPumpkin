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

Block arguments use Pumpkin block state parsing. Examples: `stone`, `minecraft:stone`, or block states such as `oak_stairs[facing=north]` when supported by the server parser.

## History

| Command | Description |
| --- | --- |
| `//undo` | Undo your latest WorldPumpkin edit in the current world. |
| `//redo` | Redo your latest undone WorldPumpkin edit in the current world. |

Undo and redo are per player and must be used in the same world as the original edit.

## Administration

| Command | Description |
| --- | --- |
| `/worldpumpkin info` | Show plugin version, authors, and Pumpkin API source. |
| `/worldpumpkin status` | Show queue status, configured limits, edit speed, fast mode, and server TPS. |
| `/worldpumpkin reload` | Reload `config.toml`. |
| `/wp info` | Alias for `/worldpumpkin info`. |
| `/wp status` | Alias for `/worldpumpkin status`. |
| `/wp reload` | Alias for `/worldpumpkin reload`. |
