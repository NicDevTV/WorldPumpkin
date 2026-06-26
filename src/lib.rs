use pumpkin_plugin_api::{register_plugin, Plugin, PluginMetadata};

const PLUGIN_NAME: &str = "WorldPumpkin";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_AUTHOR: &str = "NicDevTV";
const PLUGIN_DESCRIPTION: &str = "Simple world rules for Pumpkin servers.";

struct WorldPumpkin;

impl Plugin for WorldPumpkin {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: PLUGIN_NAME.to_owned(),
            version: PLUGIN_VERSION.to_owned(),
            authors: vec![PLUGIN_AUTHOR.to_owned()],
            description: PLUGIN_DESCRIPTION.to_owned(),
            dependencies: vec![],
            permissions: vec![],
        }
    }
}

register_plugin!(WorldPumpkin);
