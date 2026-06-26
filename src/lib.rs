// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

mod commands;
mod config;
mod engine;
mod messages;
mod state;

use config::{Config, PERMISSION_NODES};
use engine::EditQueue;
use pumpkin_plugin_api::{
    events::{EventPriority, PlayerCommandSendEvent},
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions, register_plugin,
    scheduler::SchedulerExt,
    Context, Plugin, PluginMetadata,
};
use state::PluginState;
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) const PLUGIN_NAME: &str = "WorldPumpkin";
pub(crate) const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const PLUGIN_AUTHOR: &str = "NicDevTV";
pub(crate) const PLUGIN_DESCRIPTION: &str = "Fast world editing tools for Pumpkin servers.";
pub(crate) const PUMPKIN_API_VERSION: &str = env!("WORLDPUMPKIN_PUMPKIN_API_VERSION");
pub(crate) const PUMPKIN_API_REV: &str = env!("WORLDPUMPKIN_PUMPKIN_API_REV");
pub(crate) const PUMPKIN_API_GIT: &str = env!("WORLDPUMPKIN_PUMPKIN_API_GIT");
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_ORANGE: &str = "\x1b[38;2;255;128;32m";
const ANSI_GOLD: &str = "\x1b[38;2;255;196;64m";

const WORLD_BANNER: &[&str] = &[
    r"__        _____  ____  _     ____  ",
    r"\ \      / / _ \|  _ \| |   |  _ \ ",
    r" \ \ /\ / / | | | |_) | |   | | | |",
    r"  \ V  V /| |_| |  _ <| |___| |_| |",
    r"   \_/\_/  \___/|_| \_\_____|____/ ",
];

const PUMPKIN_BANNER: &[&str] = &[
    r" ____  _   _ __  __ ____  _  _____ _   _ ",
    r"|  _ \| | | |  \/  |  _ \| |/ /_ _| \ | |",
    r"| |_) | | | | |\/| | |_) | ' / | ||  \| |",
    r"|  __/| |_| | |  | |  __/| . \ | || |\  |",
    r"|_|    \___/|_|  |_|_|   |_|\_\___|_| \_|",
];

static STATE: OnceLock<Arc<Mutex<PluginState>>> = OnceLock::new();
static QUEUE: OnceLock<Arc<Mutex<EditQueue>>> = OnceLock::new();

struct WorldPumpkin {
    state: Arc<Mutex<PluginState>>,
    queue: Arc<Mutex<EditQueue>>,
}

impl Plugin for WorldPumpkin {
    fn new() -> Self {
        let state = Arc::new(Mutex::new(PluginState::new(Config::default())));
        let queue = Arc::new(Mutex::new(EditQueue::default()));

        let _ = STATE.set(Arc::clone(&state));
        let _ = QUEUE.set(Arc::clone(&queue));

        Self { state, queue }
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: PLUGIN_NAME.to_owned(),
            version: PLUGIN_VERSION.to_owned(),
            authors: vec![PLUGIN_AUTHOR.to_owned()],
            description: PLUGIN_DESCRIPTION.to_owned(),
            dependencies: vec![],
            permissions: vec![
                permissions::FS_READ_DATA.to_owned(),
                permissions::FS_WRITE_DATA.to_owned(),
            ],
        }
    }

    fn on_load(&mut self, context: Context) -> pumpkin_plugin_api::Result<()> {
        let config = Config::load_or_create(context.get_data_folder())?;
        self.state.lock().unwrap().replace_config(config);

        register_permissions(&context)?;
        commands::register(&context, Arc::clone(&self.state), Arc::clone(&self.queue));
        context.register_event_handler::<PlayerCommandSendEvent, _>(
            commands::DoubleSlashCommandHandler {
                state: Arc::clone(&self.state),
                queue: Arc::clone(&self.queue),
            },
            EventPriority::High,
            true,
        )?;

        let state = Arc::clone(&self.state);
        let queue = Arc::clone(&self.queue);
        context.schedule_repeating_task(1, 1, move |server| {
            let config = state.lock().unwrap().config().clone();
            queue.lock().unwrap().process_tick(&server, &config);
        });

        print_startup_banner();
        Ok(())
    }
}

fn print_startup_banner() {
    for line in WORLD_BANNER {
        println!("{ANSI_ORANGE}{line}{ANSI_RESET}");
    }
    for line in PUMPKIN_BANNER {
        println!("{ANSI_GOLD}{line}{ANSI_RESET}");
    }
    println!("{ANSI_GOLD}WorldPumpkin {PLUGIN_VERSION} loaded{ANSI_RESET}");
}

fn register_permissions(context: &Context) -> pumpkin_plugin_api::Result<()> {
    for node in PERMISSION_NODES {
        context
            .register_permission(&Permission {
                node: node.node.to_owned(),
                description: node.description.to_owned(),
                default: if node.default_op {
                    PermissionDefault::Op(PermissionLevel::Two)
                } else {
                    PermissionDefault::Deny
                },
                children: vec![],
            })
            .map_err(|err| format!("failed to register permission {}: {err}", node.node))?;
    }
    Ok(())
}

register_plugin!(WorldPumpkin);
