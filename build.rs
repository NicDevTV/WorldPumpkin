// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use pumpkin_data::Block;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let mut blocks = BTreeSet::new();
    for raw_id in 0..=4096 {
        let block = Block::from_id(raw_id);
        blocks.insert(block.id);
    }

    let mut states = BTreeMap::new();
    for raw_id in blocks {
        let block = Block::from_id(raw_id);
        insert_state(&mut states, block.name, block.default_state.id);
        insert_state(
            &mut states,
            &format!("minecraft:{}", block.name),
            block.default_state.id,
        );

        for state in block.states {
            let Some(properties) = block.properties(state.id) else {
                continue;
            };
            let properties = properties.to_props();
            if properties.is_empty() {
                continue;
            }
            let suffix = properties
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(",");
            insert_state(&mut states, &format!("{}[{suffix}]", block.name), state.id);
            insert_state(
                &mut states,
                &format!("minecraft:{}[{suffix}]", block.name),
                state.id,
            );
        }
    }

    let mut generated = String::from(
        "// Copyright (c) 2026 NicDevTV\n\
         // SPDX-License-Identifier: MIT\n\n\
         pub static BLOCK_STATES: &[(&str, u16)] = &[\n",
    );
    for (name, state_id) in states {
        generated.push_str(&format!("    ({name:?}, {state_id}),\n"));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    fs::write(out_dir.join("block_states.rs"), generated).expect("write generated block states");
}

fn insert_state(states: &mut BTreeMap<String, u16>, name: &str, state_id: u16) {
    states.entry(name.to_owned()).or_insert(state_id);
}
