// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

mod pos;
mod redo;
mod replace;
mod set;
mod undo;
mod walls;
mod worldpumpkin;

use crate::{
    config::{
        PERM_LIMIT_BYPASS, PERM_POS, PERM_REDO, PERM_REPLACE, PERM_SET, PERM_UNDO, PERM_WALLS,
    },
    engine::{
        parse_block_pattern, parse_block_state, BlockPattern, BlockPos, EditOperation, EditQueue,
        Selection,
    },
    messages::{self, MessageKind},
    state::{PluginState, SelectionSlot},
};
use pumpkin_plugin_api::{
    command::{CommandError, CommandSender, ConsumedArgs},
    command_wit::Arg,
    events::{EventData, EventHandler, PlayerCommandSendEvent},
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

const ARG_POS: &str = "pos";
const ARG_PATTERN: &str = "pattern";
const ARG_FROM: &str = "from";
const ARG_TO: &str = "to";

pub fn register(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    pos::register(context, Arc::clone(&state), SelectionSlot::Pos1);
    pos::register(context, Arc::clone(&state), SelectionSlot::Pos2);
    set::register(context, Arc::clone(&state), Arc::clone(&queue));
    replace::register(context, Arc::clone(&state), Arc::clone(&queue));
    walls::register(context, Arc::clone(&state), Arc::clone(&queue));
    undo::register(context, Arc::clone(&state), Arc::clone(&queue));
    redo::register(context, Arc::clone(&state), Arc::clone(&queue));
    worldpumpkin::register(context, state, queue);
}

pub struct DoubleSlashCommandHandler {
    pub state: Arc<Mutex<PluginState>>,
    pub queue: Arc<Mutex<EditQueue>>,
}

impl EventHandler<PlayerCommandSendEvent> for DoubleSlashCommandHandler {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<PlayerCommandSendEvent>,
    ) -> EventData<PlayerCommandSendEvent> {
        let Some(command) = event.command.strip_prefix('/') else {
            return event;
        };

        if !is_worldpumpkin_command(command) {
            return event;
        }

        event.cancelled = true;
        if let Err(message) =
            handle_double_slash_command(&event.player, command, &self.state, &self.queue)
        {
            send_player_error(&event.player, &message);
        }
        event
    }
}

fn is_worldpumpkin_command(command: &str) -> bool {
    matches!(
        command.split_whitespace().next(),
        Some("pos1" | "pos2" | "set" | "replace" | "walls" | "undo" | "redo")
    )
}

fn handle_double_slash_command(
    player: &Player,
    command: &str,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
) -> Result<(), String> {
    let mut parts = command.split_whitespace();
    match parts.next() {
        Some("pos1") => handle_player_pos(player, state, SelectionSlot::Pos1),
        Some("pos2") => handle_player_pos(player, state, SelectionSlot::Pos2),
        Some("set") => {
            require_player_permission(player, PERM_SET)?;
            let to = parse_required_pattern(&mut parts, "Usage: //set <block>")?;
            queue_player_edit(player, state, queue, |owner, world, cuboid| {
                EditOperation::set(owner, world, cuboid, to, Arc::clone(state))
            })
        }
        Some("replace") => {
            require_player_permission(player, PERM_REPLACE)?;
            let from = parts
                .next()
                .ok_or_else(|| "Usage: //replace <from> <to>".to_owned())
                .and_then(parse_block_state)?;
            let to = parse_required_pattern(&mut parts, "Usage: //replace <from> <to>")?;
            queue_player_edit(player, state, queue, |owner, world, cuboid| {
                EditOperation::replace(owner, world, cuboid, from, to, Arc::clone(state))
            })
        }
        Some("walls") => {
            require_player_permission(player, PERM_WALLS)?;
            let to = parse_required_pattern(&mut parts, "Usage: //walls <block>")?;
            let (owner, world, cuboid) = player_selection_context(player, state)?;
            let blocks = cuboid.wall_volume();
            enforce_player_limit(player, blocks, state)?;
            let config = state.lock().unwrap().config().clone();
            let mut queue = queue.lock().unwrap();
            queue.can_enqueue(blocks, &config)?;
            queue.enqueue(EditOperation::walls(
                owner,
                world,
                cuboid,
                to,
                Arc::clone(state),
            ));
            send_player_ok(player, &queued_message(blocks));
            Ok(())
        }
        Some("undo") => {
            require_player_permission(player, PERM_UNDO)?;
            undo::handle_player(player, state, queue)
        }
        Some("redo") => {
            require_player_permission(player, PERM_REDO)?;
            redo::handle_player(player, state, queue)
        }
        _ => Ok(()),
    }
}

fn parse_required_pattern<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    usage: &str,
) -> Result<BlockPattern, String> {
    let block = parts.next().ok_or_else(|| usage.to_owned())?;
    ensure_no_extra_args(parts)?;
    parse_block_pattern(block)
}

fn queue_player_edit(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    queue: &Arc<Mutex<EditQueue>>,
    operation: impl FnOnce(
        String,
        pumpkin_plugin_api::world::World,
        crate::engine::Cuboid,
    ) -> EditOperation,
) -> Result<(), String> {
    let (owner, world, cuboid) = player_selection_context(player, state)?;
    let blocks = cuboid.volume();
    enforce_player_limit(player, blocks, state)?;
    let config = state.lock().unwrap().config().clone();
    let mut queue = queue.lock().unwrap();
    queue.can_enqueue(blocks, &config)?;
    queue.enqueue(operation(owner, world, cuboid));
    send_player_ok(player, &queued_message(blocks));
    Ok(())
}

pub(super) fn handle_player_pos(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
) -> Result<(), String> {
    require_player_permission(player, PERM_POS)?;
    let (x, y, z) = player.get_position();
    let pos = BlockPos {
        x: x.floor() as i32,
        y: y.floor() as i32,
        z: z.floor() as i32,
    };
    set_player_pos(player, state, slot, pos);
    Ok(())
}

pub(super) fn set_player_pos(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
    pos: BlockPos,
) {
    let selection = state
        .lock()
        .unwrap()
        .set_position(player.get_name(), slot, pos);
    send_player_ok(player, &selection_message(selection));
}

pub(super) fn player_selection_context(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
) -> Result<
    (
        String,
        pumpkin_plugin_api::world::World,
        crate::engine::Cuboid,
    ),
    String,
> {
    let owner = player.get_name();
    let selection = state
        .lock()
        .unwrap()
        .selection(&owner)
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    Ok((owner, player.get_world(), cuboid))
}

pub(super) fn enforce_player_limit(
    player: &Player,
    volume: u64,
    state: &Arc<Mutex<PluginState>>,
) -> Result<(), String> {
    let max = state.lock().unwrap().config().max_blocks_per_operation;
    if volume <= max || player.has_permission(PERM_LIMIT_BYPASS) {
        return Ok(());
    }

    Err(format!(
        "Selection is too large: {volume} blocks, limit is {max}."
    ))
}

pub(super) fn require_player_permission(player: &Player, permission: &str) -> Result<(), String> {
    if player.has_permission(permission) {
        Ok(())
    } else {
        Err("You don't have permission for that.".to_owned())
    }
}

pub(super) fn ensure_no_extra_args<'a>(
    mut parts: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    if parts.next().is_some() {
        Err("Too many arguments.".to_owned())
    } else {
        Ok(())
    }
}

pub(super) fn send_player_ok(player: &Player, message: &str) {
    player.send_system_message(messages::prefixed(MessageKind::Info, message), false);
}

pub(super) fn send_player_error(player: &Player, message: &str) {
    player.send_system_message(messages::prefixed(MessageKind::Error, message), false);
}

pub(super) fn selection_context(
    sender: &CommandSender,
    state: &Arc<Mutex<PluginState>>,
) -> Result<
    (
        String,
        pumpkin_plugin_api::world::World,
        crate::engine::Cuboid,
    ),
    CommandError,
> {
    let owner = owner_id(sender);
    let selection = state
        .lock()
        .unwrap()
        .selection(&owner)
        .ok_or_else(|| command_failed("Select two positions first."))?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| command_failed("Select two positions first."))?;
    let world = sender
        .world()
        .ok_or_else(|| command_failed("Only players in a world can edit blocks."))?;
    Ok((owner, world, cuboid))
}

pub(super) fn enforce_limit(
    sender: &CommandSender,
    server: &Server,
    volume: u64,
    state: &Arc<Mutex<PluginState>>,
) -> Result<(), CommandError> {
    let max = state.lock().unwrap().config().max_blocks_per_operation;
    if volume <= max || sender.has_permission(server, PERM_LIMIT_BYPASS) {
        return Ok(());
    }

    Err(command_failed(format!(
        "Selection is too large: {volume} blocks, limit is {max}."
    )))
}

pub(super) fn selection_message(selection: Selection) -> String {
    match selection.cuboid() {
        Some(cuboid) => format!(
            "Selection: {} -> {} ({} blocks)",
            format_selection_pos(selection.pos1),
            format_selection_pos(selection.pos2),
            cuboid.volume()
        ),
        None => format!(
            "Selection: {} -> {}",
            format_selection_pos(selection.pos1),
            format_selection_pos(selection.pos2)
        ),
    }
}

fn format_selection_pos(pos: Option<BlockPos>) -> String {
    pos.map_or_else(
        || "-".to_owned(),
        |pos| format!("{}, {}, {}", pos.x, pos.y, pos.z),
    )
}

pub(super) fn string_arg(args: &ConsumedArgs, key: &str) -> Result<String, CommandError> {
    match args.get_value(key) {
        Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => Ok(value),
        _ => Err(command_failed(format!("Missing `{key}` argument."))),
    }
}

pub(super) fn sender_position(sender: &CommandSender) -> Result<BlockPos, CommandError> {
    let (x, y, z) = sender
        .position()
        .ok_or_else(|| command_failed("Console needs a block position."))?;
    Ok(BlockPos {
        x: x.floor() as i32,
        y: y.floor() as i32,
        z: z.floor() as i32,
    })
}

pub(super) fn owner_id(sender: &CommandSender) -> String {
    sender.get_name()
}

pub(super) fn send_ok(sender: &CommandSender, message: &str) {
    sender.send_system_message(messages::prefixed(MessageKind::Info, message));
}

pub(super) fn queued_message(blocks: u64) -> String {
    format!("Queued {blocks} blocks.")
}

pub(super) fn queued_undo_message(blocks: u64) -> String {
    format!("Undo queued ({blocks} blocks).")
}

pub(super) fn queued_redo_message(blocks: u64) -> String {
    format!("Redo queued ({blocks} blocks).")
}

pub(super) fn enabled_text(enabled: bool) -> &'static str {
    if enabled {
        "on"
    } else {
        "off"
    }
}

pub(super) fn command_failed(message: impl Into<String>) -> CommandError {
    CommandError::CommandFailed(messages::prefixed(MessageKind::Error, &message.into()))
}
