// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use std::{env, fs, path::PathBuf, process::Command};

#[path = "build/version.rs"]
mod version;

/// Exports the Pumpkin dependency metadata shown by /wp info.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build/version.rs");
    println!("cargo:rerun-if-env-changed=WORLDPUMPKIN_RELEASE_BUILD");
    export_plugin_version();
    export_pumpkin_dependency_info();
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn export_plugin_version() {
    let package_version = env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    let release = match env::var("WORLDPUMPKIN_RELEASE_BUILD").as_deref() {
        Ok("true") => true,
        Ok("false") | Err(_) => false,
        Ok(_) => panic!("WORLDPUMPKIN_RELEASE_BUILD must be true or false"),
    };
    for name in ["HEAD", "refs", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            if PathBuf::from(&path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    let tag = git(&["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*"]);
    let revision = git(&["rev-parse", "--short=7", "HEAD"]);
    let version = version::build_version(
        &package_version,
        tag.as_deref(),
        revision.as_deref(),
        release,
    )
    .expect("valid plugin build version");
    println!("cargo:rustc-env=WORLDPUMPKIN_VERSION={version}");
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
