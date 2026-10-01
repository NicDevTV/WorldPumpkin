// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::engine::{position_hash, BlockPos};
use pumpkin_plugin_api::world;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Debug)]
pub struct BlockPattern {
    choices: Vec<WeightedBlock>,
    total_weight: u32,
}

#[derive(Clone, Copy, Debug)]
struct WeightedBlock {
    state: u16,
    weight: u32,
}

impl BlockPattern {
    pub(crate) fn choose(&self, pos: BlockPos, seed: u64) -> u16 {
        let mut cursor = position_hash(pos, seed) % u64::from(self.total_weight);
        for choice in &self.choices {
            if cursor < u64::from(choice.weight) {
                return choice.state;
            }
            cursor -= u64::from(choice.weight);
        }
        self.choices
            .last()
            .map(|choice| choice.state)
            .unwrap_or_default()
    }
}

trait BlockRegistry {
    fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16>;
    fn properties(&self, state: u16) -> Vec<(String, String)>;
    fn contains(&self, state: u16) -> bool;
}

struct ServerRegistry;

impl BlockRegistry for ServerRegistry {
    fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16> {
        world::resolve_block_state(name, properties)
    }

    fn properties(&self, state: u16) -> Vec<(String, String)> {
        world::get_block_properties(state)
    }

    fn contains(&self, state: u16) -> bool {
        world::get_block_state_by_id(state).is_some()
    }
}

pub fn parse_block_state(input: &str) -> Result<u16, String> {
    resolve_state(input, &ServerRegistry)
}

fn resolve_state(input: &str, registry: &impl BlockRegistry) -> Result<u16, String> {
    let input = input.trim();
    if let Ok(state) = input.parse::<u16>() {
        return registry
            .contains(state)
            .then_some(state)
            .ok_or_else(|| format!("unknown block state ID `{state}`"));
    }

    let ParsedBlock { name, properties } = parse_state_parts(input)?;
    let state = registry
        .resolve(name, &properties)
        .ok_or_else(|| format!("unknown block state `{input}`"))?;
    // Pumpkin ignores unknown property names; reject them rather than editing the wrong state.
    if !properties.is_empty() {
        let resolved = registry.properties(state);
        if properties
            .iter()
            .any(|property| !resolved.contains(property))
        {
            return Err(format!("invalid block properties in `{input}`"));
        }
    }
    Ok(state)
}

struct ParsedBlock<'a> {
    name: &'a str,
    properties: Vec<(String, String)>,
}

fn parse_state_parts(input: &str) -> Result<ParsedBlock<'_>, String> {
    let (name, properties) = match input.split_once('[') {
        Some((name, suffix)) => {
            let properties = suffix
                .strip_suffix(']')
                .ok_or_else(|| "missing closing `]` in block state".to_owned())?;
            (name.trim(), Some(properties))
        }
        None => (input, None),
    };
    if name.is_empty() || name.contains(['[', ']', ',']) {
        return Err("invalid block name".to_owned());
    }
    let mut result: Vec<(String, String)> = Vec::new();
    if let Some(properties) = properties.filter(|properties| !properties.trim().is_empty()) {
        for property in properties.split(',') {
            let (key, value) = property
                .split_once('=')
                .ok_or_else(|| format!("invalid block property `{property}`"))?;
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty()
                || value.is_empty()
                || key.contains(['[', ']'])
                || value.contains(['[', ']', '='])
                || result.iter().any(|(existing, _)| existing == key)
            {
                return Err(format!("invalid or duplicate block property `{property}`"));
            }
            result.push((key.to_owned(), value.to_owned()));
        }
    }
    Ok(ParsedBlock {
        name,
        properties: result,
    })
}

pub fn parse_block_pattern(input: &str) -> Result<BlockPattern, String> {
    resolve_pattern(input, &ServerRegistry)
}

fn resolve_pattern(input: &str, registry: &impl BlockRegistry) -> Result<BlockPattern, String> {
    let mut choices = Vec::new();
    let mut total_weight = 0_u32;
    for part in pattern_tokens(input)? {
        let part = part.trim();
        if part.is_empty() {
            return Err("empty block in pattern".to_owned());
        }
        let (weight, block) = parse_weighted_block(part)?;
        let state = resolve_state(block, registry)?;
        total_weight = total_weight
            .checked_add(weight)
            .ok_or_else(|| "pattern weights are too large".to_owned())?;
        choices.push(WeightedBlock { state, weight });
    }
    Ok(BlockPattern {
        choices,
        total_weight,
    })
}

fn pattern_tokens(input: &str) -> Result<Vec<&str>, String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut in_properties = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' if !in_properties => in_properties = true,
            ']' if in_properties => in_properties = false,
            '[' | ']' => return Err("invalid brackets in block pattern".to_owned()),
            ',' if !in_properties => {
                tokens.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if in_properties {
        return Err("missing closing `]` in block pattern".to_owned());
    }
    tokens.push(&input[start..]);
    Ok(tokens)
}

fn parse_weighted_block(input: &str) -> Result<(u32, &str), String> {
    let Some((weight, block)) = input.split_once('%') else {
        return Ok((1, input));
    };
    let weight = weight
        .trim()
        .parse::<u32>()
        .map_err(|err| format!("invalid pattern weight `{}`: {err}", weight.trim()))?;
    if weight == 0 {
        return Err("pattern weights must be greater than 0".to_owned());
    }
    let block = block.trim();
    if block.is_empty() {
        return Err("missing block after pattern weight".to_owned());
    }
    Ok((weight, block))
}

/// Finds the current pattern token without treating property commas as separators.
pub(crate) fn pattern_token_start(input: &str, start: usize) -> usize {
    let mut token_start = start.min(input.len());
    let mut in_properties = false;
    for (offset, ch) in input[token_start..].char_indices() {
        match ch {
            '[' => in_properties = true,
            ']' => in_properties = false,
            ',' | ' ' if !in_properties => token_start = start + offset + 1,
            _ => {}
        }
    }
    token_start
}

pub(crate) fn suggestions(prefix: &str, limit: usize) -> Vec<String> {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static STATES: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();

    if let Some((name, _)) = prefix.split_once('[') {
        let states = STATES.get_or_init(Mutex::default);
        if let Some(names) = states.lock().unwrap().get(name) {
            return matching_names(names, prefix, limit);
        }
        // Host calls can re-enter the plugin, so don't hold the cache lock while fetching.
        let mut names = world::get_block_by_name(name).map_or_else(Vec::new, |block| {
            world::get_state_ids_for_block_id(block.id)
                .into_iter()
                .map(|state| {
                    let properties = world::get_block_properties(state)
                        .into_iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("{name}[{properties}]")
                })
                .collect::<Vec<_>>()
        });
        names.sort_unstable();
        names.dedup();
        let result = matching_names(&names, prefix, limit);
        states.lock().unwrap().insert(name.to_owned(), names);
        result
    } else {
        let names = NAMES.get_or_init(|| {
            let mut names = world::get_all_block_names();
            names.sort_unstable();
            names.dedup();
            names
        });
        matching_names(names, prefix, limit)
    }
}

fn matching_names(names: &[String], prefix: &str, limit: usize) -> Vec<String> {
    let start = names.partition_point(|name| name.as_str() < prefix);
    names[start..]
        .iter()
        .take_while(|name| name.starts_with(prefix))
        .take(limit)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Registry;
    impl BlockRegistry for Registry {
        fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16> {
            match name.strip_prefix("minecraft:").unwrap_or(name) {
                "stone" => Some(1),
                "dirt" => Some(2),
                "stairs"
                    if properties
                        .iter()
                        .any(|(key, value)| key == "half" && value == "top") =>
                {
                    Some(4)
                }
                "stairs" => Some(3),
                _ => None,
            }
        }
        fn properties(&self, state: u16) -> Vec<(String, String)> {
            match state {
                3 => vec![
                    ("facing".into(), "north".into()),
                    ("half".into(), "bottom".into()),
                ],
                4 => vec![
                    ("facing".into(), "north".into()),
                    ("half".into(), "top".into()),
                ],
                _ => vec![],
            }
        }
        fn contains(&self, state: u16) -> bool {
            (1..=4).contains(&state)
        }
    }

    #[test]
    fn resolves_names_and_valid_numeric_ids() {
        assert_eq!(resolve_state("minecraft:stone", &Registry).unwrap(), 1);
        assert_eq!(resolve_state("2", &Registry).unwrap(), 2);
        assert!(resolve_state("65535", &Registry).is_err());
        assert!(resolve_state("missing", &Registry).is_err());
    }

    #[test]
    fn resolves_properties_in_any_order_and_keeps_defaults() {
        assert_eq!(
            resolve_state("stairs[half=top,facing=north]", &Registry).unwrap(),
            4
        );
        assert_eq!(
            resolve_state("stairs[facing=north,half=top]", &Registry).unwrap(),
            4
        );
        assert_eq!(resolve_state("stairs[half=top]", &Registry).unwrap(), 4);
    }

    #[test]
    fn rejects_properties_the_host_ignores() {
        for input in [
            "stone[foo=bar]",
            "stairs[facing=invalid]",
            "stairs[half=top,half=bottom]",
            "stairs[half=top",
            "stone]",
            "stairs[half=top]=",
        ] {
            assert!(resolve_state(input, &Registry).is_err(), "{input}");
        }
    }

    #[test]
    fn weighted_patterns_keep_property_commas_together() {
        let pattern =
            resolve_pattern("50%stairs[half=top,facing=north],50%dirt", &Registry).unwrap();
        assert_eq!(pattern.total_weight, 100);
        assert_eq!(pattern.choices.len(), 2);
        assert_eq!(pattern.choices[0].state, 4);
    }

    #[test]
    fn rejects_empty_zero_weight_and_overflowing_patterns() {
        for input in [
            "",
            "stone,",
            ",stone",
            "0%stone",
            "4294967295%stone,1%dirt",
            "stairs[[half=top]]",
        ] {
            assert!(resolve_pattern(input, &Registry).is_err(), "{input}");
        }
    }

    #[test]
    fn single_block_patterns_keep_the_same_state() {
        let pattern = resolve_pattern("stone", &Registry).unwrap();
        assert_eq!(pattern.total_weight, 1);
        assert_eq!(pattern.choose(BlockPos { x: 0, y: 0, z: 0 }, 1), 1);
    }

    #[test]
    fn suggestion_ranges_preserve_weight_namespace_and_properties() {
        let input = "//set 50%stone,50%minecraft:stairs[facing=north,half=t";
        assert_eq!(
            &input[pattern_token_start(input, 6)..],
            "50%minecraft:stairs[facing=north,half=t"
        );
    }

    #[test]
    fn matching_suggestions_respect_prefix_and_limit() {
        let names = vec![
            "dirt".into(),
            "stone".into(),
            "stone_bricks".into(),
            "water".into(),
        ];
        assert_eq!(matching_names(&names, "sto", 1), ["stone"]);
        assert!(matching_names(&names, "missing", 100).is_empty());
    }
}
