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
        }
    }

    let mut generated = String::from(
        "// Copyright (c) 2026 NicDevTV\n\
         // SPDX-License-Identifier: MIT\n\n\
         pub static BLOCK_STATES: &[(u64, u16)] = &[\n",
    );
    for (key, (_, state_id)) in states {
        generated.push_str(&format!("    ({key}, {state_id}),\n"));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    fs::write(out_dir.join("block_states.rs"), generated).expect("write generated block states");
}

fn insert_state(states: &mut BTreeMap<u64, (String, u16)>, name: &str, state_id: u16) {
    let key = block_state_key(name);
    match states.get(&key) {
        Some((existing, _)) if existing != name => {
            panic!("block state hash collision between `{existing}` and `{name}`")
        }
        Some(_) => {}
        None => {
            states.insert(key, (name.to_owned(), state_id));
        }
    }
}

fn block_state_key(input: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    input
        .as_bytes()
        .iter()
        .fold(OFFSET, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(PRIME))
}
