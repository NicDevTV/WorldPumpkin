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
    println!("cargo:rerun-if-changed=Cargo.toml");
    export_pumpkin_dependency_info();

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

fn export_pumpkin_dependency_info() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let manifest =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read Cargo.toml manifest");
    let Some(line) = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("pumpkin-plugin-api ="))
    else {
        return;
    };

    if let Some(version) = extract_manifest_value(line, "version") {
        println!("cargo:rustc-env=WORLDPUMPKIN_PUMPKIN_API_VERSION={version}");
    }
    if let Some(rev) = extract_manifest_value(line, "rev") {
        println!("cargo:rustc-env=WORLDPUMPKIN_PUMPKIN_API_REV={rev}");
    }
    if let Some(git) = extract_manifest_value(line, "git") {
        println!("cargo:rustc-env=WORLDPUMPKIN_PUMPKIN_API_GIT={git}");
    }
}

fn extract_manifest_value(line: &str, key: &str) -> Option<String> {
    let key = format!("{key} = \"");
    let start = line.find(&key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
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

    input.as_bytes().iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
