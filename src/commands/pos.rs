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
    let command = Command::new(&names, "Sets a WorldPumpkin selection position")
        .execute(PosCommand { state, slot });

    let pos_arg = CommandNode::argument(ARG_POS, &ArgumentType::BlockPos).execute(arg_handler);
    command.then(pos_arg);
    context.register_command(command, PERM_POS);
}

struct PosCommand {
    state: Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
}

impl CommandHandler for PosCommand {
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

        let selection = self
            .state
            .lock()
            .unwrap()
            .set_position(sender.get_name(), self.slot, pos);
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
}
