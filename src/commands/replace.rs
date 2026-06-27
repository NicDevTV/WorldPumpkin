// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    command_failed, enforce_limit, parse_block_pattern, parse_block_state, queued_message,
    selection_context, send_ok, string_arg, ARG_FROM, ARG_TO,
};
use crate::{
    config::PERM_REPLACE,
    engine::{EditOperation, EditQueue},
    state::PluginState,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{ArgumentType, StringType},
    commands::CommandHandler,
    Context, Server,
};
use std::sync::{Arc, Mutex};

pub(super) fn register(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
) {
    let to_arg = CommandNode::argument(ARG_TO, &ArgumentType::String(StringType::Greedy)).execute(
        ReplaceCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        },
    );
    let from_arg = CommandNode::argument(ARG_FROM, &ArgumentType::BlockState);
    from_arg.then(to_arg);
    let names = ["/replace".to_owned()];
    let command = Command::new(&names, "Replaces blocks in a selection");
    command.then(from_arg);
    context.register_command(command, PERM_REPLACE);
}

struct ReplaceCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for ReplaceCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let from = parse_block_state(&string_arg(&args, ARG_FROM)?).map_err(command_failed)?;
        let to = parse_block_pattern(&string_arg(&args, ARG_TO)?).map_err(command_failed)?;
        let (owner, world, cuboid) = selection_context(&sender, &self.state)?;
        enforce_limit(&sender, &server, cuboid.volume(), &self.state)?;
        let config = self.state.lock().unwrap().config().clone();
        let mut queue = self.queue.lock().unwrap();
        queue
            .can_enqueue(cuboid.volume(), &config)
            .map_err(command_failed)?;
        queue.enqueue(EditOperation::replace(
            owner,
            world,
            cuboid,
            from,
            to,
            Arc::clone(&self.state),
        ));
        send_ok(&sender, &queued_message(cuboid.volume()));
        Ok(1)
    }
}
