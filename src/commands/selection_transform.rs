// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, selection::Direction, selection_message, send_ok, send_player_ok};
use crate::{
    config::PERM_POS,
    engine::{BlockPos, Selection},
    state::{PluginState, SelectionSlot},
};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    commands::CommandHandler,
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

const ARG: &str = "selection_transform";

#[derive(Clone, Copy)]
enum Action {
    Shift,
    Contract,
}

pub(super) fn register(context: &Context, state: Arc<Mutex<PluginState>>) {
    for (name, action) in [("shift", Action::Shift), ("contract", Action::Contract)] {
        let args = CommandNode::argument(ARG, &ArgumentType::String(StringType::Greedy)).execute(
            TransformCommand {
                state: Arc::clone(&state),
                action,
            },
        );
        context.register_command(
            Command::new(&[format!("/{name}")], "Changes the current selection").then(args),
            PERM_POS,
        );
    }
}

struct TransformCommand {
    state: Arc<Mutex<PluginState>>,
    action: Action,
}

impl CommandHandler for TransformCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = sender
            .as_player()
            .ok_or_else(|| command_failed("Only players can change their selection."))?;
        let raw = match args.get_value(ARG) {
            Arg::Simple(value) | Arg::Block(value) | Arg::ResourceLocation(value) => value,
            _ => String::new(),
        };
        let selection = run(&player, &self.state, self.action, &raw).map_err(command_failed)?;
        send_ok(&sender, &selection_message(selection));
        Ok(1)
    }
}

pub(super) fn handle_player(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    command: &str,
    args: &str,
) -> Result<(), String> {
    let action = if command == "shift" {
        Action::Shift
    } else {
        Action::Contract
    };
    let selection = run(player, state, action, args)?;
    send_player_ok(player, &selection_message(selection));
    Ok(())
}

fn run(
    player: &Player,
    state: &Arc<Mutex<PluginState>>,
    action: Action,
    args: &str,
) -> Result<Selection, String> {
    let request = parse_request(action, args, || super::selection::player_direction(player))?;
    let owner = player.get_name();
    let world = player.get_world().get_id();
    let mut state = state.lock().unwrap();
    let selection = state
        .selection(&owner, &world)
        .ok_or("Select two positions first.")?;
    let selection = transform(selection, action, request)?;
    state.set_position(
        owner.clone(),
        world.clone(),
        SelectionSlot::Pos1,
        selection.pos1.unwrap(),
    );
    Ok(state.set_position(owner, world, SelectionSlot::Pos2, selection.pos2.unwrap()))
}

struct Request {
    amount: i32,
    reverse: i32,
    direction: Direction,
}

fn parse_request(
    action: Action,
    args: &str,
    default_direction: impl FnOnce() -> Direction,
) -> Result<Request, String> {
    let usage = match action {
        Action::Shift => "Usage: //shift <amount> [direction]",
        Action::Contract => "Usage: //contract <amount> [reverseAmount] [direction]",
    };
    let mut parts = args.split_whitespace();
    let amount = parse_amount(parts.next().ok_or(usage)?)?;
    let mut reverse = 0;
    let mut direction = None;
    if let Some(second) = parts.next() {
        match super::selection::parse_direction(second) {
            Ok(value) => direction = Some(value),
            Err(_) if matches!(action, Action::Contract) => {
                reverse = parse_amount(second)?;
                if let Some(value) = parts.next() {
                    direction = Some(super::selection::parse_direction(value)?);
                }
            }
            Err(error) => return Err(error),
        }
    }
    if parts.next().is_some() {
        return Err(usage.to_owned());
    }
    Ok(Request {
        amount,
        reverse,
        direction: direction.unwrap_or_else(default_direction),
    })
}

fn parse_amount(value: &str) -> Result<i32, String> {
    let amount = value
        .parse::<i32>()
        .map_err(|_| format!("Invalid amount `{value}`."))?;
    if amount < 0 {
        return Err("Amount must not be negative.".to_owned());
    }
    Ok(amount)
}

fn transform(selection: Selection, action: Action, request: Request) -> Result<Selection, String> {
    let a = selection.pos1.ok_or("Select two positions first.")?;
    let b = selection.pos2.ok_or("Select two positions first.")?;
    if matches!(action, Action::Shift) {
        let offset = request.direction.offset(request.amount);
        return Ok(Selection {
            pos1: Some(
                a.checked_offset(offset)
                    .ok_or("Selection is outside the supported coordinate range.")?,
            ),
            pos2: Some(
                b.checked_offset(offset)
                    .ok_or("Selection is outside the supported coordinate range.")?,
            ),
        });
    }
    let mut min = BlockPos {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        z: a.z.min(b.z),
    };
    let mut max = BlockPos {
        x: a.x.max(b.x),
        y: a.y.max(b.y),
        z: a.z.max(b.z),
    };
    let (lower, upper, positive) = match request.direction {
        Direction::North => (&mut min.z, &mut max.z, false),
        Direction::South => (&mut min.z, &mut max.z, true),
        Direction::West => (&mut min.x, &mut max.x, false),
        Direction::East => (&mut min.x, &mut max.x, true),
        Direction::Down => (&mut min.y, &mut max.y, false),
        Direction::Up => (&mut min.y, &mut max.y, true),
    };
    let (from_lower, from_upper) = if positive {
        (request.amount, request.reverse)
    } else {
        (request.reverse, request.amount)
    };
    let new_lower = i64::from(*lower) + i64::from(from_lower);
    let new_upper = i64::from(*upper) - i64::from(from_upper);
    if new_lower > new_upper {
        return Err("Contraction would remove the entire selection.".to_owned());
    }
    *lower = i32::try_from(new_lower)
        .map_err(|_| "Selection is outside the supported coordinate range.")?;
    *upper = i32::try_from(new_upper)
        .map_err(|_| "Selection is outside the supported coordinate range.")?;
    Ok(Selection {
        pos1: Some(min),
        pos2: Some(max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }
    fn selection() -> Selection {
        Selection {
            pos1: Some(p(5, 9, 3)),
            pos2: Some(p(1, 4, -1)),
        }
    }
    #[test]
    fn shift_preserves_corners_and_rejects_overflow() {
        let shifted = transform(
            selection(),
            Action::Shift,
            parse_request(Action::Shift, "3 west", || Direction::Up).unwrap(),
        )
        .unwrap();
        assert_eq!(shifted.pos1, Some(p(2, 9, 3)));
        assert_eq!(shifted.pos2, Some(p(-2, 4, -1)));
        let extreme = Selection {
            pos1: Some(p(i32::MAX, 0, 0)),
            pos2: Some(p(0, 0, 0)),
        };
        assert!(transform(
            extreme,
            Action::Shift,
            parse_request(Action::Shift, "1 east", || Direction::Up).unwrap()
        )
        .is_err());
    }
    #[test]
    fn contraction_respects_direction_reverse_and_bounds() {
        let east = transform(
            selection(),
            Action::Contract,
            parse_request(Action::Contract, "2 1 east", || Direction::Up).unwrap(),
        )
        .unwrap();
        assert_eq!(east.pos1, Some(p(3, 4, -1)));
        assert_eq!(east.pos2, Some(p(4, 9, 3)));
        let west = transform(
            selection(),
            Action::Contract,
            parse_request(Action::Contract, "2 1 west", || Direction::Up).unwrap(),
        )
        .unwrap();
        assert_eq!(west.pos1, Some(p(2, 4, -1)));
        assert_eq!(west.pos2, Some(p(3, 9, 3)));
        let down = transform(
            selection(),
            Action::Contract,
            parse_request(Action::Contract, "2 down", || Direction::Up).unwrap(),
        )
        .unwrap();
        assert_eq!(down.pos1, Some(p(1, 4, -1)));
        assert_eq!(down.pos2, Some(p(5, 7, 3)));
        assert!(transform(
            selection(),
            Action::Contract,
            parse_request(Action::Contract, "5 east", || Direction::Up).unwrap()
        )
        .is_err());
        for args in ["", "-1 east", "1 east extra", "1 2 east extra"] {
            assert!(
                parse_request(Action::Contract, args, || Direction::Up).is_err(),
                "{args}"
            );
        }
    }
}
