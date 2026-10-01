use crate::blocks::{pattern_token_start, suggestions};
use pumpkin_plugin_api::{
    command::{CommandSender, CommandSuggestion, CommandSuggestions, SuggestionRequest},
    commands::CommandSuggestionHandler,
    Server,
};
const MAX_SUGGESTIONS: usize = 100;

pub(crate) struct PatternSuggestionHandler;

impl CommandSuggestionHandler for PatternSuggestionHandler {
    /// Suggests up to 100 block-state names that match the current pattern token.
    ///
    /// The replacement range preserves any preceding weight and `minecraft:` namespace.
    fn suggest(
        &self,
        _sender: CommandSender,
        _server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let input = request.input.as_str();
        let token_start = pattern_token_start(input, request.start as usize);
        let block_start = input[token_start..]
            .rfind('%')
            .map_or(token_start, |index| token_start + index + 1);
        let raw_prefix = &input[block_start..];
        let prefix = raw_prefix.strip_prefix("minecraft:").unwrap_or(raw_prefix);
        let replacement_start = if raw_prefix.starts_with("minecraft:") {
            block_start + "minecraft:".len()
        } else {
            block_start
        };

        let values = suggestions(prefix, MAX_SUGGESTIONS)
            .into_iter()
            .map(|name| CommandSuggestion {
                value: name,
                tooltip: None,
            })
            .collect();

        CommandSuggestions {
            start: replacement_start as u32,
            length: (input.len() - replacement_start) as u32,
            values,
        }
    }
}
