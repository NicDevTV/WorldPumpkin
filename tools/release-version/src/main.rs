use semver::Version;
use std::{env, fs, path::Path, process::ExitCode};
use toml_edit::{value, DocumentMut, Item};

fn release_version(tag: &str) -> Result<String, String> {
    let version = tag
        .strip_prefix('v')
        .ok_or("release tags must start with v")?;
    Version::parse(version)
        .map(|version| version.to_string())
        .map_err(|error| format!("invalid release tag: {error}"))
}

fn set_version(root: &Path, tag: &str) -> Result<String, String> {
    let version = release_version(tag)?;
    let manifest_path = root.join("Cargo.toml");
    let lock_path = root.join("Cargo.lock");
    let read = |path: &Path| {
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let mut manifest = read(&manifest_path)?
        .parse::<DocumentMut>()
        .map_err(|error| error.to_string())?;
    let mut lock = read(&lock_path)?
        .parse::<DocumentMut>()
        .map_err(|error| error.to_string())?;
    let package = manifest
        .get_mut("package")
        .and_then(Item::as_table_mut)
        .ok_or("missing package table")?;
    let name = package
        .get("name")
        .and_then(Item::as_str)
        .ok_or("missing package name")?
        .to_owned();
    let packages = lock
        .get_mut("package")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or("missing lockfile packages")?;
    let mut matching = packages.iter_mut().filter(|package| {
        package.get("name").and_then(Item::as_str) == Some(&name) && !package.contains_key("source")
    });
    let locked = matching
        .next()
        .ok_or("root package is missing from Cargo.lock")?;
    if matching.next().is_some() {
        return Err("multiple root packages in Cargo.lock".to_owned());
    }
    package["version"] = value(&version);
    locked["version"] = value(&version);
    drop(matching);
    fs::write(&manifest_path, manifest.to_string()).map_err(|error| error.to_string())?;
    fs::write(&lock_path, lock.to_string()).map_err(|error| error.to_string())?;
    Ok(version)
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("Usage: release-version <vMAJOR.MINOR.PATCH[-PRERELEASE]>");
        return ExitCode::FAILURE;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    match set_version(root, &args[0]) {
        Ok(version) => {
            println!("{version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = env::temp_dir().join(format!(
                "worldpumpkin-version-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            fs::write(root.join("Cargo.toml"), "# keep this comment\n[package]\nname = \"world-pumpkin\"\nversion = \"0.1.0-dev.0\"\n\n[dependencies]\nother = \"0.0.2\"\n").unwrap();
            fs::write(root.join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"other\"\nversion = \"0.0.2\"\nsource = \"registry+example\"\n\n[[package]]\nname = \"world-pumpkin\"\nversion = \"0.1.0-dev.0\"\n").unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("Cargo.toml"));
            let _ = fs::remove_file(self.0.join("Cargo.lock"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn validates_semver_tags_and_preserves_prereleases() {
        for (tag, version) in [
            ("v0.1.0", "0.1.0"),
            ("v0.2.0-rc.1", "0.2.0-rc.1"),
            ("v0.2.0+build.1", "0.2.0+build.1"),
        ] {
            assert_eq!(release_version(tag).unwrap(), version);
        }
        for tag in [
            "0.1.0",
            "v1.0",
            "v01.0.0",
            "v0.1.0-01",
            "v0.1.0;echo bad",
            "v0.1.0\n",
        ] {
            assert!(release_version(tag).is_err(), "{tag}");
        }
    }

    #[test]
    fn changes_only_the_root_version_in_both_files() {
        let fixture = Fixture::new();
        assert_eq!(
            set_version(&fixture.0, "v0.2.0-rc.1").unwrap(),
            "0.2.0-rc.1"
        );
        let manifest = fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
        let lock = fs::read_to_string(fixture.0.join("Cargo.lock")).unwrap();
        assert!(manifest.starts_with("# keep this comment"));
        assert!(manifest.contains("version = \"0.2.0-rc.1\""));
        assert!(manifest.contains("other = \"0.0.2\""));
        assert!(lock.contains("name = \"other\"\nversion = \"0.0.2\""));
        assert!(lock.contains("name = \"world-pumpkin\"\nversion = \"0.2.0-rc.1\""));
    }

    #[test]
    fn rerunning_for_the_same_tag_is_safe() {
        let fixture = Fixture::new();
        set_version(&fixture.0, "v0.1.0").unwrap();
        let first = fs::read_to_string(fixture.0.join("Cargo.lock")).unwrap();
        set_version(&fixture.0, "v0.1.0").unwrap();
        assert_eq!(
            first,
            fs::read_to_string(fixture.0.join("Cargo.lock")).unwrap()
        );
    }

    #[test]
    fn invalid_tags_and_missing_lock_entries_leave_the_manifest_unchanged() {
        let fixture = Fixture::new();
        let original = fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
        assert!(set_version(&fixture.0, "bad-tag").is_err());
        fs::write(fixture.0.join("Cargo.lock"), "version = 4\n").unwrap();
        assert!(set_version(&fixture.0, "v0.1.0").is_err());
        assert_eq!(
            original,
            fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap()
        );
    }
}
