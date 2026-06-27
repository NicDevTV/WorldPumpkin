// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, queued_redo_message, send_ok, send_player_ok};
use crate::{
    config::PERM_REDO,
    engine::{EditOperation, EditQueue},
    state::PluginState,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

pub(super) fn register(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
) {
    let names = ["/redo".to_owned()];
    let command = Command::new(&names, "Redoes the latest WorldPumpkin edit")
        .execute(RedoCommand { state, queue });
    context.register_command(command, PERM_REDO);
}

pub(super) fn handle_player(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
) -> Result<(), String> {
    let owner = player.get_name();
    let info = state
        .lock()
        .unwrap()
        .latest_redo_history(&owner)
        .ok_or_else(|| "Nothing to redo.".to_owned())?;
    let world = player.get_world();
    if world.get_id() != info.world_id {
        return Err("That edit was made in another world.".to_owned());
    }
    let history_blocks = info.blocks as u64;
    let config = state.lock().unwrap().config().clone();
    queue.lock().unwrap().can_enqueue(history_blocks, &config)?;
    let history = state
        .lock()
        .unwrap()
        .pop_redo_history(&owner)
        .ok_or_else(|| "Nothing to redo.".to_owned())?;
    let mut queue = queue.lock().unwrap();
    queue.enqueue(EditOperation::redo(
        owner,
        world,
        history,
        Arc::clone(state),
    ));
    send_player_ok(player, &queued_redo_message(history_blocks));
    Ok(())
}

struct RedoCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for RedoCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let owner = sender.get_name();
        let info = self
            .state
            .lock()
            .unwrap()
            .latest_redo_history(&owner)
            .ok_or_else(|| command_failed("Nothing to redo."))?;
        let world = sender
            .world()
            .ok_or_else(|| command_failed("Only players in a world can redo."))?;
        if world.get_id() != info.world_id {
            return Err(command_failed("That edit was made in another world."));
        }

        let history_blocks = info.blocks as u64;
        let config = self.state.lock().unwrap().config().clone();
        self.queue
            .lock()
            .unwrap()
            .can_enqueue(history_blocks, &config)
            .map_err(command_failed)?;
        let history = self
            .state
            .lock()
            .unwrap()
            .pop_redo_history(&owner)
            .ok_or_else(|| command_failed("Nothing to redo."))?;
        let mut queue = self.queue.lock().unwrap();
        queue.enqueue(EditOperation::redo(
            owner,
            world,
            history,
            Arc::clone(&self.state),
        ));
        send_ok(&sender, &queued_redo_message(history_blocks));
        Ok(1)
    }
}
