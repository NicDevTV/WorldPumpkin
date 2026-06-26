// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{
    config::{Config, PERM_LIMIT_BYPASS, PERM_POS, PERM_REPLACE, PERM_SET, PERM_STATUS, PERM_UNDO},
    engine::{parse_block_state, BlockPos, EditOperation, EditQueue, Selection},
    messages::{self, MessageKind},
    state::{PluginState, SelectionSlot},
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType},
    commands::CommandHandler,
    events::{EventData, EventHandler, PlayerCommandSendEvent},
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

const ARG_POS: &str = "pos";
const ARG_BLOCK: &str = "block";
const ARG_FROM: &str = "from";
const ARG_TO: &str = "to";

pub fn register(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    register_pos(context, Arc::clone(&state), SelectionSlot::Pos1);
    register_pos(context, Arc::clone(&state), SelectionSlot::Pos2);
    register_set(context, Arc::clone(&state), Arc::clone(&queue));
    register_replace(context, Arc::clone(&state), Arc::clone(&queue));
    register_undo(context, Arc::clone(&state), Arc::clone(&queue));
    register_admin(context, state, queue);
}

fn register_pos(context: &Context, state: Arc<Mutex<PluginState>>, slot: SelectionSlot) {
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

fn register_set(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    let block_arg = CommandNode::argument(ARG_BLOCK, &ArgumentType::BlockState)
        .execute(SetCommand { state, queue });
    let names = ["/set".to_owned()];
    let command = Command::new(&names, "Fills a WorldPumpkin selection");
    command.then(block_arg);
    context.register_command(command, PERM_SET);
}

fn register_replace(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
) {
    let to_arg = CommandNode::argument(ARG_TO, &ArgumentType::BlockState).execute(ReplaceCommand {
        state: Arc::clone(&state),
        queue: Arc::clone(&queue),
    });
    let from_arg = CommandNode::argument(ARG_FROM, &ArgumentType::BlockState);
    from_arg.then(to_arg);
    let names = ["/replace".to_owned()];
    let command = Command::new(&names, "Replaces blocks in a selection");
    command.then(from_arg);
    context.register_command(command, PERM_REPLACE);
}

fn register_undo(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    let names = ["/undo".to_owned()];
    let command = Command::new(&names, "Undoes the latest WorldPumpkin edit")
        .execute(UndoCommand { state, queue });
    context.register_command(command, PERM_UNDO);
}

fn register_admin(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    let reload = CommandNode::literal("reload").execute(ReloadCommand {
        data_folder: context.get_data_folder(),
        state: Arc::clone(&state),
    });
    let status = CommandNode::literal("status").execute(StatusCommand { state, queue });
    let names = ["worldpumpkin".to_owned(), "wp".to_owned()];
    let command = Command::new(&names, "WorldPumpkin administration");
    command.then(reload);
    command.then(status);
    context.register_command(command, PERM_STATUS);
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
        Some("pos1" | "pos2" | "set" | "replace" | "undo")
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
            let block = parts
                .next()
                .ok_or_else(|| "Usage: //set <block>".to_owned())?;
            ensure_no_extra_args(parts)?;
            let to = parse_block_state(block)?;
            let (owner, world, cuboid) = player_selection_context(player, state)?;
            enforce_player_limit(player, cuboid.volume(), state)?;
            let config = state.lock().unwrap().config().clone();
            let mut queue = queue.lock().unwrap();
            queue.can_enqueue(cuboid.volume(), &config)?;
            queue.enqueue(EditOperation::set(
                owner,
                world,
                cuboid,
                to,
                Arc::clone(state),
            ));
            send_player_ok(player, "Queued //set operation.");
            Ok(())
        }
        Some("replace") => {
            require_player_permission(player, PERM_REPLACE)?;
            let from = parts
                .next()
                .ok_or_else(|| "Usage: //replace <from> <to>".to_owned())
                .and_then(parse_block_state)?;
            let to = parts
                .next()
                .ok_or_else(|| "Usage: //replace <from> <to>".to_owned())
                .and_then(parse_block_state)?;
            ensure_no_extra_args(parts)?;
            let (owner, world, cuboid) = player_selection_context(player, state)?;
            enforce_player_limit(player, cuboid.volume(), state)?;
            let config = state.lock().unwrap().config().clone();
            let mut queue = queue.lock().unwrap();
            queue.can_enqueue(cuboid.volume(), &config)?;
            queue.enqueue(EditOperation::replace(
                owner,
                world,
                cuboid,
                from,
                to,
                Arc::clone(state),
            ));
            send_player_ok(player, "Queued //replace operation.");
            Ok(())
        }
        Some("undo") => {
            require_player_permission(player, PERM_UNDO)?;
            ensure_no_extra_args(parts)?;
            let owner = player.get_name();
            let history = state
                .lock()
                .unwrap()
                .pop_history(&owner)
                .ok_or_else(|| "No WorldPumpkin history entry to undo.".to_owned())?;
            let world = player.get_world();
            if world.get_id() != history.world_id() {
                return Err("The latest history entry belongs to a different world.".to_owned());
            }
            let history_blocks = history.len() as u64;
            let config = state.lock().unwrap().config().clone();
            let mut queue = queue.lock().unwrap();
            queue.can_enqueue(history_blocks, &config)?;
            queue.enqueue(EditOperation::undo(owner, world, history));
            send_player_ok(player, "Queued //undo operation.");
            Ok(())
        }
        _ => Ok(()),
    }
}

fn handle_player_pos(
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
    let selection = state
        .lock()
        .unwrap()
        .set_position(player.get_name(), slot, pos);
    send_player_ok(player, &selection_message(selection));
    Ok(())
}

fn player_selection_context(
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
        .ok_or_else(|| "Set both //pos1 and //pos2 first.".to_owned())?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| "Set both //pos1 and //pos2 first.".to_owned())?;
    Ok((owner, player.get_world(), cuboid))
}

fn enforce_player_limit(
    player: &Player,
    volume: u64,
    state: &Arc<Mutex<PluginState>>,
) -> Result<(), String> {
    let max = state.lock().unwrap().config().max_blocks_per_operation;
    if volume <= max || player.has_permission(PERM_LIMIT_BYPASS) {
        return Ok(());
    }

    Err(format!("Selection has {volume} blocks, limit is {max}."))
}

fn require_player_permission(player: &Player, permission: &str) -> Result<(), String> {
    if player.has_permission(permission) {
        Ok(())
    } else {
        Err("I'm sorry, but you do not have permission to perform this command.".to_owned())
    }
}

fn ensure_no_extra_args<'a>(mut parts: impl Iterator<Item = &'a str>) -> Result<(), String> {
    if parts.next().is_some() {
        Err("Too many arguments.".to_owned())
    } else {
        Ok(())
    }
}

fn send_player_ok(player: &Player, message: &str) {
    player.send_system_message(messages::prefixed(MessageKind::Info, message), false);
}

fn send_player_error(player: &Player, message: &str) {
    player.send_system_message(messages::prefixed(MessageKind::Error, message), false);
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
            .set_position(owner_id(&sender), self.slot, pos);
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
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
        let to = parse_block_state(&string_arg(&args, ARG_BLOCK)?).map_err(command_failed)?;
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
        send_ok(&sender, "Queued //set operation.");
        Ok(1)
    }
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
        let to = parse_block_state(&string_arg(&args, ARG_TO)?).map_err(command_failed)?;
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
        send_ok(&sender, "Queued //replace operation.");
        Ok(1)
    }
}

struct UndoCommand {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl CommandHandler for UndoCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let owner = owner_id(&sender);
        let history = self
            .state
            .lock()
            .unwrap()
            .pop_history(&owner)
            .ok_or_else(|| command_failed("No WorldPumpkin history entry to undo."))?;
        let world = sender
            .world()
            .ok_or_else(|| command_failed("Only an in-world sender can undo edits."))?;
        if world.get_id() != history.world_id() {
            return Err(command_failed(
                "The latest history entry belongs to a different world.",
            ));
        }

        let history_blocks = history.len() as u64;
        let config = self.state.lock().unwrap().config().clone();
        let mut queue = self.queue.lock().unwrap();
        queue
            .can_enqueue(history_blocks, &config)
            .map_err(command_failed)?;
        queue.enqueue(EditOperation::undo(owner, world, history));
        send_ok(&sender, "Queued //undo operation.");
        Ok(1)
    }
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
        send_ok(
            &sender,
            &format!(
                "queued={queued}, queued_blocks={queued_blocks}, blocks/tick={}, max/op={}, fast_mode={}, TPS={:.2}, MSPT={:.2}",
                config.blocks_per_tick,
                config.max_blocks_per_operation,
                config.fast_mode,
                server.get_tps(),
                server.get_mspt()
            ),
        );
        Ok(1)
    }
}

fn selection_context(
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
        .ok_or_else(|| command_failed("Set both //pos1 and //pos2 first."))?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| command_failed("Set both //pos1 and //pos2 first."))?;
    let world = sender
        .world()
        .ok_or_else(|| command_failed("Only an in-world sender can edit blocks."))?;
    Ok((owner, world, cuboid))
}

fn enforce_limit(
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
        "Selection has {volume} blocks, limit is {max}."
    )))
}

fn selection_message(selection: Selection) -> String {
    match selection.cuboid() {
        Some(cuboid) => format!(
            "Selection updated: pos1={}, pos2={}, blocks={}",
            format_selection_pos(selection.pos1),
            format_selection_pos(selection.pos2),
            cuboid.volume()
        ),
        None => format!(
            "Selection updated: pos1={}, pos2={}, blocks=not complete",
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

fn string_arg(args: &ConsumedArgs, key: &str) -> Result<String, CommandError> {
    match args.get_value(key) {
        Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => Ok(value),
        _ => Err(command_failed(format!("Missing `{key}` argument."))),
    }
}

fn sender_position(sender: &CommandSender) -> Result<BlockPos, CommandError> {
    let (x, y, z) = sender
        .position()
        .ok_or_else(|| command_failed("Console must pass an explicit block position."))?;
    Ok(BlockPos {
        x: x.floor() as i32,
        y: y.floor() as i32,
        z: z.floor() as i32,
    })
}

fn owner_id(sender: &CommandSender) -> String {
    sender.get_name()
}

fn send_ok(sender: &CommandSender, message: &str) {
    sender.send_system_message(messages::prefixed(MessageKind::Info, message));
}

fn command_failed(message: impl Into<String>) -> CommandError {
    CommandError::CommandFailed(messages::prefixed(MessageKind::Error, &message.into()))
}
