// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, enabled_text, send_ok};
use crate::{
    config::{Config, PERM_STATUS},
    engine::EditQueue,
    state::PluginState,
    PLUGIN_AUTHORS, PLUGIN_VERSION, PUMPKIN_API_GIT, PUMPKIN_API_REV, PUMPKIN_API_VERSION,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    Context, Server,
};
use std::sync::{Arc, Mutex};

pub(super) fn register(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
) {
    let reload = CommandNode::literal("reload").execute(ReloadCommand {
        data_folder: context.get_data_folder(),
        state: Arc::clone(&state),
    });
    let status = CommandNode::literal("status").execute(StatusCommand { state, queue });
    let info = CommandNode::literal("info").execute(InfoCommand);
    let names = ["worldpumpkin".to_owned(), "wp".to_owned()];
    let command = Command::new(&names, "WorldPumpkin administration");
    command.then(reload);
    command.then(status);
    command.then(info);
    context.register_command(command, PERM_STATUS);
}

struct ReloadCommand {
    data_folder: String,
    state: Arc<Mutex<PluginState>>,
}

impl CommandHandler for ReloadCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let config = Config::load_or_create(self.data_folder.clone()).map_err(command_failed)?;
        self.state.lock().unwrap().replace_config(config);
        send_ok(&sender, "Config reloaded.");
        Ok(1)
    }
}

struct StatusCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

struct InfoCommand;

impl CommandHandler for InfoCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        send_ok(
            &sender,
            &format!(
                "\nVersion: {PLUGIN_VERSION}\nAuthors: {authors}\nPumpkin API: {PUMPKIN_API_VERSION} ({short_rev})\nSource: {PUMPKIN_API_GIT}",
                authors = PLUGIN_AUTHORS.join(", "),
                short_rev = short_rev(PUMPKIN_API_REV)
            ),
        );
        Ok(1)
    }
}

impl CommandHandler for StatusCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let config = self.state.lock().unwrap().config().clone();
        let queue = self.queue.lock().unwrap();
        let queued = queue.len();
        let queued_blocks = queue.queued_blocks();
        let work = if queued == 0 {
            "Idle".to_owned()
        } else {
            format!("{queued} edits waiting ({queued_blocks} blocks)")
        };
        send_ok(
            &sender,
            &format!(
                "\nWork: {work}\nLimit: {} blocks\nSpeed: {} blocks/tick\nFast edits: {}\nServer: {:.1} TPS",
                config.max_blocks_per_operation,
                config.blocks_per_tick,
                enabled_text(config.fast_mode),
                server.get_tps()
            ),
        );
        Ok(1)
    }
}

fn short_rev(rev: &str) -> &str {
    rev.get(..8).unwrap_or(rev)
}
