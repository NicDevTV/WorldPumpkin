// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    command_failed, enforce_limit, parse_block_pattern, queued_message, selection_context, send_ok,
    string_arg, ARG_PATTERN,
};
use crate::{
    config::PERM_SET,
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
        .execute(SetCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        });
    let names = ["/set".to_owned()];
    let command = Command::new(&names, "Fills a WorldPumpkin selection");
    command.then(pattern_arg);
    context.register_command(command, PERM_SET);
}

struct SetCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for SetCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let to = parse_block_pattern(&string_arg(&args, ARG_PATTERN)?).map_err(command_failed)?;
        let (owner, world, cuboid) = selection_context(&sender, &self.state)?;
        enforce_limit(&sender, &server, cuboid.volume(), &self.state)?;
        let config = self.state.lock().unwrap().config().clone();
        let mut queue = self.queue.lock().unwrap();
        queue
            .can_enqueue(cuboid.volume(), &config)
            .map_err(command_failed)?;
        queue.enqueue(EditOperation::set(
            owner,
            world,
            cuboid,
            to,
            Arc::clone(&self.state),
        ));
        send_ok(&sender, &queued_message(cuboid.volume()));
        Ok(1)
    }
}
