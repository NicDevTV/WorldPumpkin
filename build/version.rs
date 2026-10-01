use semver::{Prerelease, Version};

pub fn build_version(
    package: &str,
    tag: Option<&str>,
    revision: Option<&str>,
    release: bool,
) -> Result<String, String> {
    let package = Version::parse(package).map_err(|error| error.to_string())?;
    if release {
        return Ok(package.to_string());
    }
    let mut version = Version::new(package.major, package.minor, package.patch);
    if let Some(tag) = tag.and_then(|tag| Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok())
    {
        let tagged = Version::new(tag.major, tag.minor, tag.patch);
        if tagged > version {
            version = tagged;
        }
    }
    let prerelease = match revision {
        Some(revision)
            if !revision.is_empty() && revision.chars().all(|ch| ch.is_ascii_hexdigit()) =>
        {
            format!("dev.g{revision}")
        }
        Some(_) => return Err("invalid Git revision".to_owned()),
        None => "dev.0".to_owned(),
    };
    version.pre = Prerelease::new(&prerelease).map_err(|error| error.to_string())?;
    Ok(version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_builds_keep_the_upcoming_version_and_identify_the_commit() {
        assert_eq!(
            build_version("0.1.0-dev.0", Some("v0.0.2"), Some("0a12345"), false).unwrap(),
            "0.1.0-dev.g0a12345"
        );
    }

    #[test]
    fn newer_release_tags_advance_the_local_version() {
        assert_eq!(
            build_version("0.1.0-dev.0", Some("v0.2.0"), Some("abc1234"), false).unwrap(),
            "0.2.0-dev.gabc1234"
        );
    }

    #[test]
    fn source_archives_without_git_still_build_as_development_versions() {
        assert_eq!(
            build_version("0.1.0-dev.0", None, None, false).unwrap(),
            "0.1.0-dev.0"
        );
    }

    #[test]
    fn release_builds_keep_the_exact_tag_version_including_prereleases() {
        for version in ["0.1.0", "0.2.0-rc.1", "0.2.0+build.7"] {
            assert_eq!(
                build_version(version, Some("v0.0.2"), Some("abc1234"), true).unwrap(),
                version
            );
        }
    }

    #[test]
    fn invalid_versions_and_revisions_are_rejected() {
        assert!(build_version("invalid", None, None, false).is_err());
        assert!(build_version("0.1.0-dev.0", None, Some("bad revision"), false).is_err());
    }
}
