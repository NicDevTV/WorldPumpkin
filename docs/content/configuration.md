---
title: Configuration
description: WorldPumpkin config and permission reference.
navigation:
  order: 4
---

# Configuration

WorldPumpkin creates `config.toml` in its plugin data folder on first startup.

## Defaults

```toml
max_blocks_per_operation = 250000
blocks_per_tick = 8192
max_queued_operations = 4
max_queued_blocks = 1000000
max_history_entries = 15
max_history_blocks = 500000
fast_mode = true
notify_clients = true
```

## Options

| Option | Default | Description |
| --- | ---: | --- |
| `max_blocks_per_operation` | `250000` | Maximum size of a single edit for normal players. |
| `blocks_per_tick` | `8192` | Number of blocks processed per server tick. |
| `max_queued_operations` | `4` | Maximum number of queued edits. |
| `max_queued_blocks` | `1000000` | Maximum total blocks waiting in the edit queue. |
| `max_history_entries` | `15` | Maximum undo entries kept per player. |
| `max_history_blocks` | `500000` | Maximum number of history blocks kept per player. |
| `fast_mode` | `true` | Uses direct chunk writes and disables physics side effects where possible. |
| `notify_clients` | `true` | Sends client updates for changed blocks. |

Use `/worldpumpkin reload` or `/wp reload` after editing the file.

## Permissions

| Permission | Default | Description |
| --- | --- | --- |
| `WorldPumpkin:command.set` | OP | Allows `//set`. |
| `WorldPumpkin:command.replace` | OP | Allows `//replace`. |
| `WorldPumpkin:command.walls` | OP | Allows `//walls`. |
| `WorldPumpkin:command.pos` | OP | Allows selection commands. |
| `WorldPumpkin:command.undo` | OP | Allows `//undo`. |
| `WorldPumpkin:command.redo` | OP | Allows `//redo`. |
| `WorldPumpkin:command.reload` | OP | Allows config reload. |
| `WorldPumpkin:command.status` | OP | Allows status and info commands. |
| `WorldPumpkin:limit.bypass` | Off | Allows bypassing configured edit limits. |
