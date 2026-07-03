// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{
    config::{Config, PERM_UPDATE_NOTIFY},
    messages::{self, MessageKind},
    PLUGIN_VERSION,
};
use pumpkin_plugin_api::{
    events::{EventData, EventHandler, PlayerJoinEvent},
    player::Player,
    Server,
};
use serde_json::Value;
use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
};

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/NicDevTV/WorldPumpkin/releases/latest";
const RELEASES_URL: &str = "https://github.com/NicDevTV/WorldPumpkin/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStatus {
    pub latest_version: String,
    pub current_version: String,
    pub release_url: String,
}

impl UpdateStatus {
    fn message(&self) -> String {
        format!(
            "Update available: WorldPumpkin {} is loaded, {} is latest. Download: {}",
            self.current_version, self.latest_version, self.release_url
        )
    }
}

#[derive(Clone, Default)]
pub struct UpdateState {
    status: Option<UpdateStatus>,
}

impl UpdateState {
    pub fn status(&self) -> Option<UpdateStatus> {
        self.status.clone()
    }

    fn replace_status(&mut self, status: Option<UpdateStatus>) {
        self.status = status;
    }
}

pub fn check_on_startup(config: &Config, state: &Arc<Mutex<UpdateState>>) {
    if !config.update_check_enabled {
        println!("WorldPumpkin update check disabled in config.");
        state.lock().unwrap().replace_status(None);
        return;
    }

    match fetch_latest_release() {
        Ok(Some(status)) => {
            println!("{}", status.message());
            state.lock().unwrap().replace_status(Some(status));
        }
        Ok(None) => {
            println!("WorldPumpkin {PLUGIN_VERSION} is up to date.");
            state.lock().unwrap().replace_status(None);
        }
        Err(err) => {
            println!("WorldPumpkin update check failed: {err}");
            state.lock().unwrap().replace_status(None);
        }
    }
}

pub struct UpdateJoinHandler {
    pub config_state: Arc<Mutex<crate::state::PluginState>>,
    pub update_state: Arc<Mutex<UpdateState>>,
}

impl EventHandler<PlayerJoinEvent> for UpdateJoinHandler {
    fn handle(
        &self,
        _server: Server,
        event: EventData<PlayerJoinEvent>,
    ) -> EventData<PlayerJoinEvent> {
        let config = self.config_state.lock().unwrap().config().clone();
        if config.update_check_enabled && config.update_notify_on_join {
            if let Some(status) = self.update_state.lock().unwrap().status() {
                notify_player(&event.player, &status);
            }
        }
        event
    }
}

fn notify_player(player: &Player, status: &UpdateStatus) {
    if player.has_permission(PERM_UPDATE_NOTIFY) {
        player.send_system_message(
            messages::prefixed(MessageKind::Info, &status.message()),
            false,
        );
    }
}

fn fetch_latest_release() -> Result<Option<UpdateStatus>, String> {
    let response = ureq::get(LATEST_RELEASE_URL)
        .header("accept", "application/vnd.github+json")
        .header("user-agent", "WorldPumpkin")
        .call()
        .map_err(|err| err.to_string())?;
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    let release: Value = serde_json::from_str(&body).map_err(|err| err.to_string())?;
    let latest_version = release
        .get("tag_name")
        .and_then(Value::as_str)
        .map(normalize_version)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| "GitHub latest release response did not include tag_name".to_owned())?;

    let current_version = normalize_version(PLUGIN_VERSION);
    if compare_versions(&latest_version, &current_version) != Ordering::Greater {
        return Ok(None);
    }

    Ok(Some(UpdateStatus {
        latest_version,
        current_version,
        release_url: release
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or(RELEASES_URL)
            .to_owned(),
    }))
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let left = left_parts.get(index).copied().unwrap_or(0);
        let right = right_parts.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    Ordering::Equal
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, normalize_version};
    use std::cmp::Ordering;

    #[test]
    fn strips_release_tag_prefix() {
        assert_eq!(normalize_version("v0.1.0"), "0.1.0");
    }

    #[test]
    fn compares_versions_by_number_parts() {
        assert_eq!(compare_versions("1.10.0", "1.2.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
    }
}
