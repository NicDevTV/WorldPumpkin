// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{command_failed, selection_context, send_ok, send_player_ok};
use crate::{config::PERM_POS, engine::Cuboid, state::PluginState};
use pumpkin_plugin_api::{
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    player::Player,
    Context, Server,
};
use std::sync::{Arc, Mutex};

pub(super) fn register(context: &Context, state: Arc<Mutex<PluginState>>) {
    context.register_command(
        Command::new(&["/size".to_owned()], "Shows the selection dimensions").execute(
            InspectCommand {
                state: Arc::clone(&state),
                clear: false,
            },
        ),
        PERM_POS,
    );
    context.register_command(
        Command::new(
            &["/desel".to_owned(), "/deselect".to_owned()],
            "Clears the selection",
        )
        .execute(InspectCommand { state, clear: true }),
        PERM_POS,
    );
}

struct InspectCommand {
    state: Arc<Mutex<PluginState>>,
    clear: bool,
}

impl CommandHandler for InspectCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        if self.clear {
            let world = sender
                .world()
                .ok_or_else(|| command_failed("Only players in a world can clear a selection."))?;
            let cleared = self
                .state
                .lock()
                .unwrap()
                .clear_selection(&sender.get_name(), &world.get_id());
            send_ok(&sender, clear_message(cleared));
        } else {
            let (_, _, cuboid) = selection_context(&sender, &self.state)?;
            send_ok(&sender, &size_message(cuboid));
        }
        Ok(1)
    }
}

pub(super) fn size_player(player: &Player, state: &Arc<Mutex<PluginState>>) -> Result<(), String> {
    let (_, _, cuboid) = super::player_selection_context(player, state)?;
    send_player_ok(player, &size_message(cuboid));
    Ok(())
}

pub(super) fn clear_player(player: &Player, state: &Arc<Mutex<PluginState>>) -> Result<(), String> {
    let cleared = state
        .lock()
        .unwrap()
        .clear_selection(&player.get_name(), &player.get_world().get_id());
    send_player_ok(player, clear_message(cleared));
    Ok(())
}

fn size_message(cuboid: Cuboid) -> String {
    let [x, y, z] = cuboid.dimensions();
    format!(
        "Selection size: {x} x {y} x {z} ({} blocks).",
        cuboid.volume()
    )
}

fn clear_message(cleared: bool) -> &'static str {
    if cleared {
        "Selection cleared."
    } else {
        "No selection to clear in this world."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BlockPos;

    #[test]
    fn size_uses_inclusive_normalized_dimensions() {
        let cuboid = Cuboid::new(
            BlockPos { x: 4, y: 7, z: -1 },
            BlockPos { x: 2, y: 7, z: 2 },
        );
        assert_eq!(
            size_message(cuboid),
            "Selection size: 3 x 1 x 4 (12 blocks)."
        );
    }
}
