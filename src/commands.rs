// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{
    config::{
        Config, PERM_LIMIT_BYPASS, PERM_POS, PERM_REDO, PERM_REPLACE, PERM_SET, PERM_STATUS,
        PERM_UNDO,
    },
    engine::{
        parse_block_pattern, parse_block_state, BlockPattern, BlockPos, EditOperation, EditQueue,
        Selection,
    },
    messages::{self, MessageKind},
    state::{PluginState, SelectionSlot},
    PLUGIN_VERSION, PUMPKIN_API_GIT, PUMPKIN_API_REV, PUMPKIN_API_VERSION,
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    commands::CommandHandler,
    events::{EventData, EventHandler, PlayerCommandSendEvent},
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

const ARG_POS: &str = "pos";
const ARG_BLOCK: &str = "block";
const ARG_PATTERN: &str = "pattern";
const ARG_FROM: &str = "from";
const ARG_TO: &str = "to";

pub fn register(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    register_pos(context, Arc::clone(&state), SelectionSlot::Pos1);
    register_pos(context, Arc::clone(&state), SelectionSlot::Pos2);
    register_set(context, Arc::clone(&state), Arc::clone(&queue));
    register_replace(context, Arc::clone(&state), Arc::clone(&queue));
    register_undo(context, Arc::clone(&state), Arc::clone(&queue));
    register_redo(context, Arc::clone(&state), Arc::clone(&queue));
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
    let block_arg =
        CommandNode::argument(ARG_BLOCK, &ArgumentType::BlockState).execute(SetCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        });
    let pattern_arg = CommandNode::argument(ARG_PATTERN, &ArgumentType::String(StringType::Greedy))
        .execute(SetCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        });
    let pattern = CommandNode::literal("pattern");
    pattern.then(pattern_arg);
    let names = ["/set".to_owned()];
    let command = Command::new(&names, "Fills a WorldPumpkin selection");
    command.then(block_arg);
    command.then(pattern);
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
    let pattern_arg = CommandNode::argument(ARG_PATTERN, &ArgumentType::String(StringType::Greedy))
        .execute(ReplaceCommand {
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
        });
    let pattern = CommandNode::literal("pattern");
    pattern.then(pattern_arg);
    let from_arg = CommandNode::argument(ARG_FROM, &ArgumentType::BlockState);
    from_arg.then(to_arg);
    from_arg.then(pattern);
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

fn register_redo(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
    let names = ["/redo".to_owned()];
    let command = Command::new(&names, "Redoes the latest WorldPumpkin edit")
        .execute(RedoCommand { state, queue });
    context.register_command(command, PERM_REDO);
}

fn register_admin(context: &Context, state: Arc<Mutex<PluginState>>, queue: Arc<Mutex<EditQueue>>) {
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
        Some("pos1" | "pos2" | "set" | "replace" | "undo" | "redo")
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
            let to = parse_block_pattern(block)?;
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
            send_player_ok(player, &queued_message(cuboid.volume()));
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
                .and_then(parse_block_pattern)?;
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
            send_player_ok(player, &queued_message(cuboid.volume()));
            Ok(())
        }
        Some("undo") => {
            require_player_permission(player, PERM_UNDO)?;
            ensure_no_extra_args(parts)?;
            let owner = player.get_name();
            let info = state
                .lock()
                .unwrap()
                .latest_undo_history(&owner)
                .ok_or_else(|| "Nothing to undo.".to_owned())?;
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
                .pop_undo_history(&owner)
                .ok_or_else(|| "Nothing to undo.".to_owned())?;
            let mut queue = queue.lock().unwrap();
            queue.enqueue(EditOperation::undo(
                owner,
                world,
                history,
                Arc::clone(state),
            ));
            send_player_ok(player, &queued_undo_message(history_blocks));
            Ok(())
        }
        Some("redo") => {
            require_player_permission(player, PERM_REDO)?;
            ensure_no_extra_args(parts)?;
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
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| "Select two positions first.".to_owned())?;
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

    Err(format!(
        "Selection is too large: {volume} blocks, limit is {max}."
    ))
}

fn require_player_permission(player: &Player, permission: &str) -> Result<(), String> {
    if player.has_permission(permission) {
        Ok(())
    } else {
        Err("You don't have permission for that.".to_owned())
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
        let to = block_pattern_arg(&args, ARG_BLOCK)?;
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
        let to = block_pattern_arg(&args, ARG_TO)?;
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
        let info = self
            .state
            .lock()
            .unwrap()
            .latest_undo_history(&owner)
            .ok_or_else(|| command_failed("Nothing to undo."))?;
        let world = sender
            .world()
            .ok_or_else(|| command_failed("Only players in a world can undo."))?;
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
            .pop_undo_history(&owner)
            .ok_or_else(|| command_failed("Nothing to undo."))?;
        let mut queue = self.queue.lock().unwrap();
        queue.enqueue(EditOperation::undo(
            owner,
            world,
            history,
            Arc::clone(&self.state),
        ));
        send_ok(&sender, &queued_undo_message(history_blocks));
        Ok(1)
    }
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
        let owner = owner_id(&sender);
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
                "WorldPumpkin {PLUGIN_VERSION}. Pumpkin API {PUMPKIN_API_VERSION} ({short_rev}). Source: {PUMPKIN_API_GIT}.",
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
                "Status: {work}. Limit: {} blocks. Speed: {} blocks/tick. Fast edits: {}. Server: {:.1} TPS.",
                config.max_blocks_per_operation,
                config.blocks_per_tick,
                enabled_text(config.fast_mode),
                server.get_tps()
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
        .ok_or_else(|| command_failed("Select two positions first."))?;
    let cuboid = selection
        .cuboid()
        .ok_or_else(|| command_failed("Select two positions first."))?;
    let world = sender
        .world()
        .ok_or_else(|| command_failed("Only players in a world can edit blocks."))?;
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
        "Selection is too large: {volume} blocks, limit is {max}."
    )))
}

fn selection_message(selection: Selection) -> String {
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

fn string_arg(args: &ConsumedArgs, key: &str) -> Result<String, CommandError> {
    match args.get_value(key) {
        Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => Ok(value),
        _ => Err(command_failed(format!("Missing `{key}` argument."))),
    }
}

fn block_pattern_arg(args: &ConsumedArgs, block_key: &str) -> Result<BlockPattern, CommandError> {
    match args.get_value(ARG_PATTERN) {
        Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => {
            parse_block_pattern(&value).map_err(command_failed)
        }
        _ => {
            let state = parse_block_state(&string_arg(args, block_key)?).map_err(command_failed)?;
            Ok(BlockPattern::single(state))
        }
    }
}

fn sender_position(sender: &CommandSender) -> Result<BlockPos, CommandError> {
    let (x, y, z) = sender
        .position()
        .ok_or_else(|| command_failed("Console needs a block position."))?;
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

fn queued_message(blocks: u64) -> String {
    format!("Queued {blocks} blocks.")
}

fn queued_undo_message(blocks: u64) -> String {
    format!("Undo queued ({blocks} blocks).")
}

fn queued_redo_message(blocks: u64) -> String {
    format!("Redo queued ({blocks} blocks).")
}

fn enabled_text(enabled: bool) -> &'static str {
    if enabled {
        "on"
    } else {
        "off"
    }
}

fn short_rev(rev: &str) -> &str {
    rev.get(..8).unwrap_or(rev)
}

fn command_failed(message: impl Into<String>) -> CommandError {
    CommandError::CommandFailed(messages::prefixed(MessageKind::Error, &message.into()))
}
