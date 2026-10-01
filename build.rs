// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use std::{env, fs, path::PathBuf};

/// Exports the Pumpkin dependency metadata shown by /wp info.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    export_pumpkin_dependency_info();
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
