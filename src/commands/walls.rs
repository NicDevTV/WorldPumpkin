// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    command_failed, enforce_limit, parse_block_pattern, queued_message, selection_context, send_ok,
    string_arg, ARG_PATTERN,
};
use crate::{
    config::PERM_WALLS,
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
    let pattern_arg = CommandNode::argument(ARG_PATTERN, &ArgumentType::String(StringType::Greedy))
        .execute(WallsCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        });
    let names = ["/walls".to_owned()];
    let command = Command::new(&names, "Builds WorldPumpkin selection walls");
    command.then(pattern_arg);
    context.register_command(command, PERM_WALLS);
}

struct WallsCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for WallsCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let to = parse_block_pattern(&string_arg(&args, ARG_PATTERN)?).map_err(command_failed)?;
        let (owner, world, cuboid) = selection_context(&sender, &self.state)?;
        let blocks = cuboid.wall_volume();
        enforce_limit(&sender, &server, blocks, &self.state)?;
        let config = self.state.lock().unwrap().config().clone();
        let mut queue = self.queue.lock().unwrap();
        queue.can_enqueue(blocks, &config).map_err(command_failed)?;
        queue.enqueue(EditOperation::walls(
            owner,
            world,
            cuboid,
            to,
            Arc::clone(&self.state),
        ));
        send_ok(&sender, &queued_message(blocks));
        Ok(1)
    }
}
