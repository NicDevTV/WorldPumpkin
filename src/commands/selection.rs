// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, selection_message, send_ok, send_player_ok, set_player_pos};
use crate::{
    config::PERM_POS,
    engine::{BlockPos, Selection},
    state::{PluginState, SelectionSlot},
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    command_wit::{ArgumentType, StringType},
    commands::CommandHandler,
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

const HPOS_RANGE: f64 = 300.0;
const ARG_EXPAND: &str = "expand";

pub(super) fn register_chunk(context: &Context, state: Arc<Mutex<PluginState>>) {
    let names = ["/chunk".to_owned()];
    let command = Command::new(&names, "Selects the current chunk").execute(ChunkCommand { state });
    context.register_command(command, PERM_POS);
}

pub(super) fn register_expand(context: &Context, state: Arc<Mutex<PluginState>>) {
    let names = ["/expand".to_owned()];
    let command = Command::new(&names, "Expands the current selection").execute(ExpandCommand {
        state: Arc::clone(&state),
    });
    let args = pumpkin_plugin_api::command::CommandNode::argument(
        ARG_EXPAND,
        &ArgumentType::String(StringType::Greedy),
    )
    .execute(ExpandCommand { state });
    command.then(args);
    context.register_command(command, PERM_POS);
}

pub(super) fn register_hpos(
    context: &Context,
    state: Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
) {
    let name = match slot {
        SelectionSlot::Pos1 => "hpos1",
        SelectionSlot::Pos2 => "hpos2",
    };
    let names = [format!("/{name}")];
    let command = Command::new(&names, "Sets a selection position to the targeted block")
        .execute(HposCommand { state, slot });
    context.register_command(command, PERM_POS);
}

pub(super) fn handle_player_chunk(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
) -> Result<(), String> {
    let (x, _, z) = player.get_position();
    let min = BlockPos {
        x: (x.floor() as i32).div_euclid(16) * 16,
        y: player.get_world().get_min_y(),
        z: (z.floor() as i32).div_euclid(16) * 16,
    };
    let max = BlockPos {
        x: min.x + 15,
        y: chunk_top_y(&player.get_world(), min),
        z: min.z + 15,
    };
    set_player_selection(player.get_name(), state, min, max, |message| {
        send_player_ok(player, message)
    });
    Ok(())
}

pub(super) fn handle_player_hpos(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
) -> Result<(), String> {
    let pos = target_block(player)?;
    set_player_pos(player, state, slot, pos);
    Ok(())
}

pub(super) fn handle_player_expand(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    args: &str,
) -> Result<(), String> {
    let request = parse_expand(args, || player_direction(player))?;
    let selection = expand_selection(player.get_name(), state, request, Some(&player.get_world()))?;
    send_player_ok(player, &selection_message(selection));
    Ok(())
}

struct ChunkCommand {
    state: Arc<Mutex<PluginState>>,
}

impl CommandHandler for ChunkCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = sender
            .as_player()
            .ok_or_else(|| command_failed("Only players can select their current chunk."))?;
        let (x, _, z) = player.get_position();
        let world = player.get_world();
        let min = BlockPos {
            x: (x.floor() as i32).div_euclid(16) * 16,
            y: world.get_min_y(),
            z: (z.floor() as i32).div_euclid(16) * 16,
        };
        let max = BlockPos {
            x: min.x + 15,
            y: chunk_top_y(&world, min),
            z: min.z + 15,
        };
        set_player_selection(sender.get_name(), &self.state, min, max, |message| {
            send_ok(&sender, message)
        });
        Ok(1)
    }
}

struct ExpandCommand {
    state: Arc<Mutex<PluginState>>,
}

impl CommandHandler for ExpandCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = sender
            .as_player()
            .ok_or_else(|| command_failed("Only players can expand a selection."))?;
        let raw = match args.get_value(ARG_EXPAND) {
            pumpkin_plugin_api::command_wit::Arg::Simple(value)
            | pumpkin_plugin_api::command_wit::Arg::Block(value)
            | pumpkin_plugin_api::command_wit::Arg::ResourceLocation(value) => value,
            _ => String::new(),
        };
        let request = parse_expand(&raw, || player_direction(&player)).map_err(command_failed)?;
        let selection = expand_selection(
            sender.get_name(),
            &self.state,
            request,
            Some(&player.get_world()),
        )
        .map_err(command_failed)?;
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
}

struct HposCommand {
    state: Arc<Mutex<PluginState>>,
    slot: SelectionSlot,
}

impl CommandHandler for HposCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = sender
            .as_player()
            .ok_or_else(|| command_failed("Only players can select a targeted block."))?;
        let pos = target_block(&player).map_err(command_failed)?;
        let selection = self
            .state
            .lock()
            .unwrap()
            .set_position(sender.get_name(), self.slot, pos);
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
}

fn set_player_selection(
    owner: String,
    state: &Arc<Mutex<PluginState>>,
    min: BlockPos,
    max: BlockPos,
    send: impl FnOnce(&str),
) {
    let mut state = state.lock().unwrap();
    state.set_position(owner.clone(), SelectionSlot::Pos1, min);
    let selection = state.set_position(owner, SelectionSlot::Pos2, max);
    drop(state);
    send(&selection_message(selection));
}

fn target_block(player: &Player) -> Result<BlockPos, String> {
    let result = player
        .as_entity()
        .raycast(HPOS_RANGE, false)
        .ok_or_else(|| "No block in sight.".to_owned())?;
    Ok(BlockPos::from(result.pos))
}

fn expand_selection(
    owner: String,
    state: &Arc<Mutex<PluginState>>,
    request: ExpandRequest,
    world: Option<&pumpkin_plugin_api::world::World>,
) -> Result<Selection, String> {
    let mut state = state.lock().unwrap();
    let selection = state
        .selection(&owner)
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    let mut cuboid = ExpandedCuboid::from_selection(selection)?;
    match request {
        ExpandRequest::Directional {
            amount,
            reverse_amount,
            direction,
        } => {
            cuboid.expand(direction, amount);
            if reverse_amount != 0 {
                cuboid.expand(direction.opposite(), reverse_amount);
            }
        }
        ExpandRequest::Vertical => {
            let world = world.ok_or("Only players in a world can expand vertically.")?;
            cuboid.min.y = world.get_min_y();
            cuboid.max.y = world_max_y(world);
        }
    }
    state.set_position(owner.clone(), SelectionSlot::Pos1, cuboid.min);
    Ok(state.set_position(owner, SelectionSlot::Pos2, cuboid.max))
}

fn chunk_top_y(world: &pumpkin_plugin_api::world::World, min: BlockPos) -> i32 {
    let mut top = world.get_min_y();
    for x in min.x..=min.x + 15 {
        for z in min.z..=min.z + 15 {
            top = top.max(world.get_top_block_y(x, z));
        }
    }
    top
}

fn world_max_y(world: &pumpkin_plugin_api::world::World) -> i32 {
    if world.get_min_y() < 0 {
        319
    } else {
        255
    }
}

fn parse_expand(
    args: &str,
    default_direction: impl FnOnce() -> Direction,
) -> Result<ExpandRequest, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(
            "Usage: //expand <amount> [reverseAmount] [direction] or //expand vert".to_owned(),
        );
    }
    if parts.len() == 1 && parts[0].eq_ignore_ascii_case("vert") {
        return Ok(ExpandRequest::Vertical);
    }
    let amount = parse_expand_amount(parts[0])?;
    let mut reverse_amount = 0;
    let mut direction = None;
    match parts.len() {
        1 => {}
        2 => {
            if let Ok(parsed_direction) = parse_direction(parts[1]) {
                direction = Some(parsed_direction);
            } else {
                reverse_amount = parse_expand_amount(parts[1])?;
            }
        }
        3 => {
            reverse_amount = parse_expand_amount(parts[1])?;
            direction = Some(parse_direction(parts[2])?);
        }
        _ => {
            return Err(
                "Usage: //expand <amount> [reverseAmount] [direction] or //expand vert".to_owned(),
            );
        }
    }
    Ok(ExpandRequest::Directional {
        amount,
        reverse_amount,
        direction: direction.unwrap_or_else(default_direction),
    })
}

fn parse_expand_amount(input: &str) -> Result<i32, String> {
    let amount = input
        .parse::<i32>()
        .map_err(|err| format!("failed to parse expand amount `{input}`: {err}"))?;
    if amount < 0 {
        return Err("Expand amount must not be negative.".to_owned());
    }
    Ok(amount)
}

pub(super) fn parse_direction(input: &str) -> Result<Direction, String> {
    match input.to_ascii_lowercase().as_str() {
        "n" | "north" => Ok(Direction::North),
        "s" | "south" => Ok(Direction::South),
        "e" | "east" => Ok(Direction::East),
        "w" | "west" => Ok(Direction::West),
        "u" | "up" => Ok(Direction::Up),
        "d" | "down" => Ok(Direction::Down),
        _ => Err(format!("unknown direction `{input}`")),
    }
}

pub(super) fn player_direction(player: &Player) -> Direction {
    direction_from_rotation(player.get_yaw(), player.get_pitch())
}

fn direction_from_rotation(yaw: f32, pitch: f32) -> Direction {
    if pitch <= -45.0 {
        return Direction::Up;
    }
    if pitch >= 45.0 {
        return Direction::Down;
    }
    match ((yaw / 90.0).round() as i32).rem_euclid(4) {
        0 => Direction::South,
        1 => Direction::West,
        2 => Direction::North,
        _ => Direction::East,
    }
}

#[derive(Clone, Copy)]
enum ExpandRequest {
    Directional {
        amount: i32,
        reverse_amount: i32,
        direction: Direction,
    },
    Vertical,
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl Direction {
    pub(super) fn offset(self, amount: i32) -> BlockPos {
        match self {
            Self::North => BlockPos {
                z: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::South => BlockPos {
                z: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::East => BlockPos {
                x: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::West => BlockPos {
                x: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::Up => BlockPos {
                y: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::Down => BlockPos {
                y: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

struct ExpandedCuboid {
    min: BlockPos,
    max: BlockPos,
}

impl ExpandedCuboid {
    fn from_selection(selection: Selection) -> Result<Self, String> {
        let pos1 = selection
            .pos1
            .ok_or_else(|| "Select two positions first.".to_owned())?;
        let pos2 = selection
            .pos2
            .ok_or_else(|| "Select two positions first.".to_owned())?;
        Ok(Self {
            min: BlockPos {
                x: pos1.x.min(pos2.x),
                y: pos1.y.min(pos2.y),
                z: pos1.z.min(pos2.z),
            },
            max: BlockPos {
                x: pos1.x.max(pos2.x),
                y: pos1.y.max(pos2.y),
                z: pos1.z.max(pos2.z),
            },
        })
    }

    fn expand(&mut self, direction: Direction, amount: i32) {
        match direction {
            Direction::North => self.min.z = self.min.z.saturating_sub(amount),
            Direction::South => self.max.z = self.max.z.saturating_add(amount),
            Direction::East => self.max.x = self.max.x.saturating_add(amount),
            Direction::West => self.min.x = self.min.x.saturating_sub(amount),
            Direction::Up => self.max.y = self.max.y.saturating_add(amount),
            Direction::Down => self.min.y = self.min.y.saturating_sub(amount),
        }
    }
}
