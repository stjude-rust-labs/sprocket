//! Tag enumeration, semver matching, and version selection.

use std::collections::HashSet;

use semver::Version;
use thiserror::Error;
use url::Url;

use crate::VersionRequirement;

/// The Git ref namespace prefix for tags.
const REF_TAG_PREFIX: &str = "refs/tags/";

/// Suffix the smart-protocol ref advertisement appends to annotated tag
/// names when reporting the underlying commit (the "peeled" form).
const PEELED_TAG_SUFFIX: &str = "^{}";

/// Lists the tags advertised by the remote at `url` over `git ls-remote`.
/// No clone is performed.
pub fn list_remote_tags(url: &Url) -> Result<HashSet<String>, VersionError> {
    let mut remote = git2::Remote::create_detached(url.as_str()).map_err(VersionError::Git)?;
    remote
        .connect_auth(git2::Direction::Fetch, None, None)
        .map_err(VersionError::Git)?;
    let advertised = remote.list().map_err(VersionError::Git)?;
    let mut tags = HashSet::new();
    for head in advertised {
        let Some(stripped) = head.name().strip_prefix(REF_TAG_PREFIX) else {
            continue;
        };
        // Annotated tags appear twice: once as the tag object, once peeled to
        // the underlying commit. The peeled entry duplicates the base name,
        // and the `HashSet` collapses the two into a single entry.
        let base = stripped.strip_suffix(PEELED_TAG_SUFFIX).unwrap_or(stripped);
        tags.insert(base.to_string());
    }
    let _ = remote.disconnect();
    Ok(tags)
}

/// Selects the highest-precedence version among `tags` that satisfies
/// `requirement`, scoped to `path_prefix` for path-prefixed multi-module
/// repositories.
///
/// `tags` is the raw output of [`list_remote_tags`]; this function strips
/// the `v` prefix and any `<path-prefix>/` and parses each as semver,
/// ignoring entries that fail to parse or fail the requirement.
pub fn select_version(
    tags: &HashSet<String>,
    path_prefix: Option<&str>,
    requirement: &VersionRequirement,
) -> Result<Version, VersionError> {
    let mut considered = Vec::new();
    let mut matching = Vec::new();
    for tag in tags {
        let stripped = match path_prefix {
            Some(prefix) => match tag.strip_prefix(&format!("{prefix}/v")) {
                Some(s) => s,
                None => continue,
            },
            None => match tag.strip_prefix('v') {
                Some(s) if !s.contains('/') => s,
                _ => continue,
            },
        };
        let Ok(v) = Version::parse(stripped) else {
            continue;
        };
        considered.push(v.clone());
        if requirement.matches(&v) {
            matching.push(v);
        }
    }
    matching
        .into_iter()
        .max()
        .ok_or(VersionError::NoSatisfyingVersion {
            requirement: requirement.clone(),
            considered,
        })
}

/// Errors produced by the `versions` module.
#[derive(Debug, Error)]
pub enum VersionError {
    /// A `git2` operation failed.
    #[error("git ls-remote failed")]
    Git(#[source] git2::Error),

    /// No discovered tag satisfies the requirement.
    #[error(
        "no version satisfies requirement `{requirement}` (considered: {})",
        format_versions(.considered)
    )]
    NoSatisfyingVersion {
        /// The unmet version requirement.
        requirement: VersionRequirement,
        /// The versions discovered before filtering.
        considered: Vec<Version>,
    },
}

/// Renders a list of versions for error display, or `<none>` when empty.
fn format_versions(versions: &[Version]) -> String {
    if versions.is_empty() {
        return "<none>".to_string();
    }
    versions
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(s: &str) -> VersionRequirement {
        s.to_string().try_into().unwrap()
    }

    fn tags(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn selects_highest_root_version() {
        let v = select_version(
            &tags(&["v1.0.0", "v1.2.0", "v2.0.0", "csvkit/v0.5.0"]),
            None,
            &req("^1"),
        )
        .unwrap();
        assert_eq!(v, Version::parse("1.2.0").unwrap());
    }

    #[test]
    fn selects_path_prefixed_version() {
        let v = select_version(
            &tags(&["csvkit/v0.5.0", "csvkit/v0.6.0", "spellbook/v1.0.0"]),
            Some("csvkit"),
            &req(">=0.5"),
        )
        .unwrap();
        assert_eq!(v, Version::parse("0.6.0").unwrap());
    }

    #[test]
    fn ignores_non_semver_tags() {
        let v =
            select_version(&tags(&["v1.0.0", "release-2024", "vXYZ"]), None, &req("^1")).unwrap();
        assert_eq!(v, Version::parse("1.0.0").unwrap());
    }

    #[test]
    fn errors_when_no_satisfying_version() {
        let err = select_version(&tags(&["v1.0.0"]), None, &req("^2")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no version satisfies"), "got: {msg}");
        assert!(
            msg.contains("1.0.0"),
            "msg should list considered versions: {msg}"
        );
    }

    #[test]
    fn root_selector_ignores_path_prefixed_tags() {
        let err = select_version(
            &tags(&["csvkit/v1.0.0", "spellbook/v1.0.0"]),
            None,
            &req("^1"),
        )
        .unwrap_err();
        assert!(matches!(err, VersionError::NoSatisfyingVersion { .. }));
    }

    #[test]
    fn path_selector_ignores_root_tags() {
        let err =
            select_version(&tags(&["v1.0.0", "v2.0.0"]), Some("csvkit"), &req("^1")).unwrap_err();
        assert!(matches!(err, VersionError::NoSatisfyingVersion { .. }));
    }
}
