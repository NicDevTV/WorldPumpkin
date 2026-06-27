// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use std::{
    fs,
    path::{Path, PathBuf},
};

pub const CONFIG_FILE_NAME: &str = "config.toml";

pub const PERM_SET: &str = "WorldPumpkin:command.set";
pub const PERM_REPLACE: &str = "WorldPumpkin:command.replace";
pub const PERM_WALLS: &str = "WorldPumpkin:command.walls";
pub const PERM_POS: &str = "WorldPumpkin:command.pos";
pub const PERM_UNDO: &str = "WorldPumpkin:command.undo";
pub const PERM_REDO: &str = "WorldPumpkin:command.redo";
pub const PERM_RELOAD: &str = "WorldPumpkin:command.reload";
pub const PERM_STATUS: &str = "WorldPumpkin:command.status";
pub const PERM_LIMIT_BYPASS: &str = "WorldPumpkin:limit.bypass";

#[derive(Clone, Copy)]
pub struct PermissionNode {
    pub node: &'static str,
    pub description: &'static str,
    pub default_op: bool,
}

pub const PERMISSION_NODES: &[PermissionNode] = &[
    PermissionNode {
        node: PERM_SET,
        description: "Allows filling a WorldPumpkin selection.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_REPLACE,
        description: "Allows replacing blocks in a WorldPumpkin selection.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_WALLS,
        description: "Allows building WorldPumpkin selection walls.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_POS,
        description: "Allows setting WorldPumpkin selection positions.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_UNDO,
        description: "Allows undoing WorldPumpkin edits.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_REDO,
        description: "Allows redoing WorldPumpkin edits.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_RELOAD,
        description: "Allows reloading WorldPumpkin config.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_STATUS,
        description: "Allows reading WorldPumpkin status.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_LIMIT_BYPASS,
        description: "Allows bypassing configured WorldPumpkin edit limits.",
        default_op: false,
    },
];

#[derive(Clone, Debug)]
pub struct Config {
    pub max_blocks_per_operation: u64,
    pub blocks_per_tick: usize,
    pub max_queued_operations: usize,
    pub max_queued_blocks: u64,
    pub max_history_entries: usize,
    pub max_history_blocks: usize,
    /// Fast mode uses direct chunk writes and disables physics side effects where possible.
    pub fast_mode: bool,
    pub notify_clients: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_blocks_per_operation: 250_000,
            blocks_per_tick: 8_192,
            max_queued_operations: 4,
            max_queued_blocks: 1_000_000,
            max_history_entries: 15,
            max_history_blocks: 500_000,
            fast_mode: true,
            notify_clients: true,
        }
    }
}

impl Config {
    pub fn load_or_create(data_folder: String) -> Result<Self, String> {
        let data_folder = PathBuf::from(data_folder);
        fs::create_dir_all(&data_folder).map_err(|err| {
            format!(
                "failed to create data folder {}: {err}",
                data_folder.display()
            )
        })?;

        let config_path = data_folder.join(CONFIG_FILE_NAME);
        if !config_path.exists() {
            let default_config = Self::default();
            write_config(&config_path, &default_config)?;
            return Ok(default_config);
        }

        let raw = fs::read_to_string(&config_path)
            .map_err(|err| format!("failed to read {}: {err}", config_path.display()))?;
        let config = parse_config(&raw)
            .map_err(|err| format!("failed to parse {}: {err}", config_path.display()))?;
        config.validate()?;
        let normalized = config_toml(&config)?;
        if raw != normalized {
            fs::write(&config_path, normalized)
                .map_err(|err| format!("failed to write {}: {err}", config_path.display()))?;
        }
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_blocks_per_operation == 0 {
            return Err("max_blocks_per_operation must be greater than 0".to_owned());
        }
        if self.blocks_per_tick == 0 {
            return Err("blocks_per_tick must be greater than 0".to_owned());
        }
        if self.max_queued_operations == 0 {
            return Err("max_queued_operations must be greater than 0".to_owned());
        }
        if self.max_queued_blocks == 0 {
            return Err("max_queued_blocks must be greater than 0".to_owned());
        }
        if self.max_history_entries == 0 {
            return Err("max_history_entries must be greater than 0".to_owned());
        }
        if self.max_history_blocks == 0 {
            return Err("max_history_blocks must be greater than 0".to_owned());
        }
        Ok(())
    }
}

fn write_config(path: &Path, config: &Config) -> Result<(), String> {
    fs::write(path, config_toml(config)?)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn config_toml(config: &Config) -> Result<String, String> {
    Ok(format!(
        "\
max_blocks_per_operation = {}
blocks_per_tick = {}
max_queued_operations = {}
max_queued_blocks = {}
max_history_entries = {}
max_history_blocks = {}
fast_mode = {}
notify_clients = {}
",
        config.max_blocks_per_operation,
        config.blocks_per_tick,
        config.max_queued_operations,
        config.max_queued_blocks,
        config.max_history_entries,
        config.max_history_blocks,
        config.fast_mode,
        config.notify_clients
    ))
}

fn parse_config(raw: &str) -> Result<Config, String> {
    let mut config = Config::default();

    for (line_index, line) in raw.lines().enumerate() {
        let line = line.split_once('#').map_or(line, |(value, _)| value).trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {} is missing `=`", line_index + 1));
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "max_blocks_per_operation" => config.max_blocks_per_operation = parse_u64(value, key)?,
            "blocks_per_tick" => config.blocks_per_tick = parse_usize(value, key)?,
            "max_queued_operations" => config.max_queued_operations = parse_usize(value, key)?,
            "max_queued_blocks" => config.max_queued_blocks = parse_u64(value, key)?,
            "max_history_entries" => config.max_history_entries = parse_usize(value, key)?,
            "max_history_blocks" => config.max_history_blocks = parse_usize(value, key)?,
            "fast_mode" => config.fast_mode = parse_bool(value, key)?,
            "notify_clients" => config.notify_clients = parse_bool(value, key)?,
            _ => {}
        }
    }

    Ok(config)
}

fn parse_u64(value: &str, key: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|err| format!("failed to parse `{key}` as integer: {err}"))
}

fn parse_usize(value: &str, key: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|err| format!("failed to parse `{key}` as integer: {err}"))
}

fn parse_bool(value: &str, key: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("failed to parse `{key}` as bool")),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_config, Config, PERMISSION_NODES, PERM_LIMIT_BYPASS};

    #[test]
    fn default_config_is_valid() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn default_config_uses_fast_no_physics_writes() {
        let config = Config::default();

        assert!(config.fast_mode);
        assert!(config.notify_clients);
    }

    #[test]
    fn legacy_write_flags_are_ignored() {
        let config = parse_config(
            r#"
max_blocks_per_operation = 250000
blocks_per_tick = 8192
max_queued_operations = 4
max_queued_blocks = 1000000
max_history_entries = 15
max_history_blocks = 500000
allow_chunk_direct_writes = true
notify_clients = true
skip_neighbor_updates = true
skip_block_callbacks = true
skip_block_drops = true
skip_redstone_wire_state_replacement = true
"#,
        )
        .unwrap();

        assert!(config.fast_mode);
        assert!(config.notify_clients);
    }

    #[test]
    fn config_parser_keeps_defaults_for_missing_fields() {
        let config = parse_config(
            r#"
blocks_per_tick = 4096
notify_clients = false
"#,
        )
        .unwrap();

        assert_eq!(
            config.max_blocks_per_operation,
            Config::default().max_blocks_per_operation
        );
        assert_eq!(config.blocks_per_tick, 4096);
        assert!(!config.notify_clients);
    }

    #[test]
    fn config_parser_accepts_comments_and_whitespace() {
        let config = parse_config(
            r#"
            # WorldPumpkin config
            max_history_entries = 7 # inline comment
            fast_mode = false
            "#,
        )
        .unwrap();

        assert_eq!(config.max_history_entries, 7);
        assert!(!config.fast_mode);
    }

    #[test]
    fn rejects_zero_limits() {
        let config = Config {
            blocks_per_tick: 0,
            ..Config::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn limit_bypass_is_not_granted_to_ops_by_default() {
        let bypass = PERMISSION_NODES
            .iter()
            .find(|node| node.node == PERM_LIMIT_BYPASS)
            .unwrap();

        assert!(!bypass.default_op);
    }
}
