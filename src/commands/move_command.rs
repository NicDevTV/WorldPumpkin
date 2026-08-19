// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    command_failed, enforce_player_limit, player_selection_context, selection::Direction,
    send_player_ok,
};
use crate::{
    config::PERM_MOVE,
    engine::{EditOperation, EditQueue},
    state::PluginState,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    commands::CommandHandler,
    player::Player,
    Context,
};
use std::sync::{Arc, Mutex};

const ARG_MOVE: &str = "move";

pub(super) fn register(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
) {
    let handler = MoveCommand {
        state: Arc::clone(&state),
        queue: Arc::clone(&queue),
    };
    let args =
        CommandNode::argument(ARG_MOVE, &ArgumentType::String(StringType::Greedy)).execute(handler);
    let names = ["/move".to_owned()];
    let command = Command::new(&names, "Moves the contents of a WorldPumpkin selection");
    command.then(args);
    context.register_command(command, PERM_MOVE);
}

pub(super) fn handle_player(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
    args: &str,
) -> Result<(), String> {
    let request = parse_request(args, || super::selection::player_direction(player))?;
    let (owner, world, cuboid) = player_selection_context(player, state)?;
    let offset = request.direction.offset(request.amount);
    cuboid
        .translated(offset)
        .ok_or_else(|| "Move destination is outside the supported coordinate range.".to_owned())?;
    enforce_player_limit(player, cuboid.volume(), state)?;

    let config = state.lock().unwrap().config().clone();
    let mut queue_guard = queue.lock().unwrap();
    queue_guard.can_enqueue(cuboid.volume(), &config)?;
    queue_guard.enqueue(EditOperation::move_blocks(
        owner,
        world,
        cuboid,
        offset,
        Arc::clone(state),
    ));
    send_player_ok(
        player,
        &format!("Move queued ({} blocks).", cuboid.volume()),
    );
    Ok(())
}

struct MoveCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for MoveCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = sender
            .as_player()
            .ok_or_else(|| command_failed("Only players can move a selection."))?;
        let raw = match args.get_value(ARG_MOVE) {
            Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => value,
            _ => String::new(),
        };
        handle_player(&player, &self.state, &self.queue, &raw).map_err(command_failed)?;
        Ok(1)
    }
}

struct MoveRequest {
    amount: i32,
    direction: Direction,
}

fn parse_request(
    args: &str,
    default_direction: impl FnOnce() -> Direction,
) -> Result<MoveRequest, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 2 {
        return Err("Usage: //move <amount> [direction]".to_owned());
    }

    let amount = parts[0]
        .parse::<i32>()
        .map_err(|err| format!("failed to parse move amount `{}`: {err}", parts[0]))?;
    if amount < 0 {
        return Err("Move amount must not be negative.".to_owned());
    }
    let direction = parts.get(1).map_or_else(
        || Ok(default_direction()),
        |input| super::selection::parse_direction(input),
    )?;
    Ok(MoveRequest { amount, direction })
}
