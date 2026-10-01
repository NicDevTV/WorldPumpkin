// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, queued_redo_message, queued_undo_message, send_ok, send_player_ok};
use crate::{
    config::{PERM_REDO, PERM_UNDO},
    engine::{EditOperation, EditQueue, ReplayDirection},
    state::PluginState,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    player::Player,
    world::World,
    Context, Server,
};
use std::sync::{Arc, Mutex};

/// Registers the undo or redo command with its corresponding permission and shared queue.
pub(super) fn register(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
    direction: ReplayDirection,
) {
    let (name, permission, description) = match direction {
        ReplayDirection::Undo => ("/undo", PERM_UNDO, "Undoes the latest WorldPumpkin edit"),
        ReplayDirection::Redo => ("/redo", PERM_REDO, "Redoes the latest WorldPumpkin edit"),
    };
    context.register_command(
        Command::new(&[name.to_owned()], description).execute(HistoryCommand {
            state,
            queue,
            direction,
        }),
        permission,
    );
}

/// Queues the player's latest undo or redo entry and reports the admitted block count.
pub(super) fn handle_player(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
    direction: ReplayDirection,
) -> Result<(), String> {
    let blocks = enqueue(
        player.get_name(),
        player.get_world(),
        state,
        queue,
        direction,
    )?;
    send_player_ok(player, &message(direction, blocks));
    Ok(())
}

struct HistoryCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
    direction: ReplayDirection,
}

impl CommandHandler for HistoryCommand {
    /// Queues the sender's requested history replay, requiring a world, and reports success.
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let world = sender
            .world()
            .ok_or_else(|| command_failed("Only players in a world can undo or redo."))?;
        let blocks = enqueue(
            sender.get_name(),
            world,
            &self.state,
            &self.queue,
            self.direction,
        )
        .map_err(command_failed)?;
        send_ok(&sender, &message(self.direction, blocks));
        Ok(1)
    }
}

/// Validates world and queue limits before removing and queuing the latest history entry.
///
/// Locks the queue before state so admission and history removal are atomic.
/// Returns the queued block count, or an error without consuming history.
fn enqueue(
    owner: String,
    world: World,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
    direction: ReplayDirection,
) -> Result<u64, String> {
    let world_id = world.get_id();
    // Tick processing uses the same lock order. Admission and history removal are atomic.
    let mut queue = queue.lock().unwrap();
    let mut state_guard = state.lock().unwrap();
    let empty = match direction {
        ReplayDirection::Undo => "Nothing to undo.",
        ReplayDirection::Redo => "Nothing to redo.",
    };
    let info = match direction {
        ReplayDirection::Undo => state_guard.latest_undo_history(&owner),
        ReplayDirection::Redo => state_guard.latest_redo_history(&owner),
    }
    .ok_or(empty)?;
    if info.world_id != world_id {
        return Err("That edit was made in another world.".to_owned());
    }
    let blocks = info.blocks as u64;
    queue.can_enqueue(blocks, state_guard.config())?;
    let history = match direction {
        ReplayDirection::Undo => state_guard.pop_undo_history(&owner),
        ReplayDirection::Redo => state_guard.pop_redo_history(&owner),
    }
    .ok_or(empty)?;
    drop(state_guard);
    queue.enqueue(EditOperation::replay(
        owner,
        world,
        history,
        direction,
        Arc::clone(state),
    ));
    Ok(blocks)
}

/// Formats the queue confirmation for the selected replay direction.
fn message(direction: ReplayDirection, blocks: u64) -> String {
    match direction {
        ReplayDirection::Undo => queued_undo_message(blocks),
        ReplayDirection::Redo => queued_redo_message(blocks),
    }
}
