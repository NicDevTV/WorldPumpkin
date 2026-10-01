# WorldPumpkin

World editing plugin for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) Minecraft servers. Select an area, fill or replace blocks, build walls, move blocks, and undo or redo edits.

Edits run through a queue with configurable limits and a block budget per tick.

## Install

1. Download `world_pumpkin.wasm` from [Releases](https://github.com/NicDevTV/WorldPumpkin/releases).
2. Stop the server and copy the file into its plugin folder.
3. Start the server and run `/wp info`.

The plugin is built against Pumpkin revision `742beaf6f9f84fb99a04402434f6c6feb86667d1` (0.2.0+26.3-26.51). Use a server with a compatible plugin API. Block names and IDs come from the running server.

## Use

Stand at one corner and run `//pos1`, then at the opposite corner and run `//pos2`.

```text
//set stone
//replace stone dirt
//walls oak_planks
//move 5 north
//undo
//redo
```

Use `//hpos1` and `//hpos2` to select the blocks you are looking at. `//chunk` selects your current chunk; `//expand` extends the selection.

Block patterns also work: `//set 50%stone,50%dirt`. Properties can be combined, for example `//set oak_stairs[facing=north,half=top]`.

Commands require OP level 2 by default. Selections belong to the world where they were made. Undo and redo keep block-entity data, including chest inventories and sign text. History is kept per player in memory and is lost when the server restarts.

## Configuration

On first startup, the plugin creates `config.toml` in its data folder. Defaults allow 250,000 blocks per edit and process 8,192 blocks per tick. Fast mode is enabled by default and skips physics side effects where possible.

Run `/wp reload` after changing the config. `/wp status` shows queue status and limits. The update check reports new releases; it does not install them.

See the docs for [all commands](docs/content/commands.md) and [config options and permissions](docs/content/configuration.md).

## License

[MIT](LICENSE)
