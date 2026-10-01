// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{selection_message, send_ok, sender_position, ARG_POS};
use crate::{config::PERM_POS, engine::BlockPos, state::PluginState, state::SelectionSlot};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType},
    commands::CommandHandler,
    Context, Server,
};
use std::sync::{Arc, Mutex};

/// Registers a command that stores one endpoint of a player's selection.
pub(super) fn register(context: &Context, state: Arc<Mutex<PluginState>>, slot: SelectionSlot) {
    let name = match slot {
        SelectionSlot::Pos1 => "pos1",
        SelectionSlot::Pos2 => "pos2",
    };
    let arg_handler = PosCommand {
        state: Arc::clone(&state),
        slot,
    };
    let names = [format!("/{name}")];
    let pos_arg = CommandNode::argument(ARG_POS, &ArgumentType::BlockPos).execute(arg_handler);
    let command = Command::new(&names, "Sets a WorldPumpkin selection position")
        .execute(PosCommand { state, slot })
        .then(pos_arg);
    context.register_command(command, PERM_POS);
}

struct PosCommand {
    state: Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
}

impl CommandHandler for PosCommand {
    /// Stores the supplied endpoint, falling back to the sender position, and reports the selection.
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let pos = match args.get_value(ARG_POS) {
            Arg::BlockPos(pos) => BlockPos::from(pos),
            _ => sender_position(&sender)?,
        };

        let owner = sender.get_name();
        let world_id = sender
            .world()
            .map(|world| world.get_id())
            .unwrap_or_default();
        let selection = self
            .state
            .lock()
            .unwrap()
            .set_position(owner, world_id, self.slot, pos);
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
}

/// Parses three integer coordinates with optional `~` offsets, or uses the current position.
///
/// Rejects extra or missing coordinates, invalid integers, and relative coordinate overflow.
pub(super) fn parse_position(args: &str, current: BlockPos) -> Result<BlockPos, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return Ok(current);
    }
    if parts.len() != 3 {
        return Err("Usage: //pos1 or //pos2 [x y z]".to_owned());
    }
    /// Parses an absolute coordinate or a checked `~` offset from the current coordinate.
    fn coordinate(input: &str, current: i32) -> Result<i32, String> {
        if let Some(offset) = input.strip_prefix('~') {
            let offset = if offset.is_empty() {
                0
            } else {
                offset
                    .parse::<i32>()
                    .map_err(|_| format!("invalid coordinate `{input}`"))?
            };
            current
                .checked_add(offset)
                .ok_or_else(|| "coordinate is out of range".to_owned())
        } else {
            input
                .parse::<i32>()
                .map_err(|_| format!("invalid coordinate `{input}`"))
        }
    }
    Ok(BlockPos {
        x: coordinate(parts[0], current.x)?,
        y: coordinate(parts[1], current.y)?,
        z: coordinate(parts[2], current.z)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Checks absolute and relative positions, the empty-input default, and invalid coordinates.
    #[test]
    fn positions_accept_explicit_and_relative_coordinates() {
        let current = BlockPos {
            x: 10,
            y: 64,
            z: -5,
        };
        assert_eq!(parse_position("", current).unwrap(), current);
        assert_eq!(
            parse_position("1 2 3", current).unwrap(),
            BlockPos { x: 1, y: 2, z: 3 }
        );
        assert_eq!(
            parse_position("~ ~10 ~-2", current).unwrap(),
            BlockPos {
                x: 10,
                y: 74,
                z: -7
            }
        );
        for input in ["1 2", "1 2 3 4", "bad 2 3", "~2147483647 ~ ~"] {
            assert!(parse_position(input, current).is_err(), "{input}");
        }
    }
}
