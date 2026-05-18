//! Version discovery, tag parsing, and semver selection.
//!
//! This module bridges the Git ref advertisement layer and the
//! resolver's version-requirement matching. It discovers tags and
//! branches from a remote, parses semver version tags (optionally
//! scoped to a path prefix for multi-module repositories), and
//! selects the highest-precedence version satisfying a requirement.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use semver::Version;
use thiserror::Error;
use url::Url;

use crate::GitCommit;
use crate::VersionRequirement;
use crate::resolver::git::CredentialMode;
use crate::resolver::git::GitError;
use crate::resolver::git::list_advertised_refs;

/// Errors produced by version selection.
#[derive(Debug, Error)]
pub enum VersionError {
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

/// A semver tag scoped to an optional path prefix, e.g. `v1.2.3` or
/// `csvkit/v1.2.3`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionTag {
    /// The path prefix preceding `/v`, or `None` for a root tag.
    prefix: Option<String>,
    /// The parsed semver version.
    version: Version,
}

impl VersionTag {
    /// Builds a tag from a prefix and a version.
    pub fn new(prefix: Option<String>, version: Version) -> Self {
        Self { prefix, version }
    }

    /// Returns the path prefix, or `None` for a root tag.
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Consumes the tag and returns the version.
    pub fn into_version(self) -> Version {
        self.version
    }
}

impl fmt::Display for VersionTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.prefix {
            Some(p) => write!(f, "{p}/v{}", self.version),
            None => write!(f, "v{}", self.version),
        }
    }
}

impl FromStr for VersionTag {
    type Err = VersionTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((prefix, rest)) = s.rsplit_once("/v")
            && let Ok(version) = Version::parse(rest)
        {
            return Ok(Self {
                prefix: Some(prefix.to_string()),
                version,
            });
        }
        let stripped = s
            .strip_prefix('v')
            .ok_or_else(|| VersionTagError(s.to_string()))?;
        let version = Version::parse(stripped).map_err(|_| VersionTagError(s.to_string()))?;
        Ok(Self {
            prefix: None,
            version,
        })
    }
}

/// An error parsing a [`VersionTag`].
#[derive(Debug, Error)]
#[error("`{0}` is not a valid version tag (expected `v<semver>` or `<prefix>/v<semver>`)")]
pub struct VersionTagError(String);

/// The Git ref namespace prefix for tags.
const REF_TAG_PREFIX: &str = "refs/tags/";

/// The Git ref namespace prefix for branches (heads).
const REF_HEAD_PREFIX: &str = "refs/heads/";

/// Suffix the smart-protocol ref advertisement appends to annotated tag
/// names when reporting the underlying commit (the "peeled" form).
const PEELED_TAG_SUFFIX: &str = "^{}";

/// A map of ref name to the commit it resolves to.
pub type RemoteRefs = HashMap<String, GitCommit>;

/// Discovers the tags advertised by the remote at `url` together with
/// the commit each one points at. No clone is performed.
///
/// Annotated tags advertise twice. Once as the tag-object OID under
/// the base name, and once as the underlying commit OID under the
/// `^{}` peeled name. The peeled entry carries the commit rather than
/// the tag-object, so it overwrites any earlier base entry for the
/// same tag.
pub fn discover_remote_tags(
    url: &Url,
    max_refs: usize,
    mode: CredentialMode,
) -> Result<RemoteRefs, GitError> {
    let advertised = list_advertised_refs(url, max_refs, mode)?;
    let mut refs = HashMap::new();
    for (name, oid) in advertised {
        let Some(stripped) = name.strip_prefix(REF_TAG_PREFIX) else {
            continue;
        };
        let (base, peeled) = match stripped.strip_suffix(PEELED_TAG_SUFFIX) {
            Some(b) => (b, true),
            None => (stripped, false),
        };
        let Ok(commit) = GitCommit::try_from(oid) else {
            continue;
        };
        // Peeled entries always win; they carry the commit OID rather
        // than the tag-object OID.
        if peeled || !refs.contains_key(base) {
            refs.insert(base.to_string(), commit);
        }
    }
    Ok(refs)
}

/// Discovers the branches advertised by the remote at `url`, mapping
/// each branch name to the commit it points at. No clone is performed.
pub fn discover_remote_branches(
    url: &Url,
    max_refs: usize,
    mode: CredentialMode,
) -> Result<RemoteRefs, GitError> {
    let advertised = list_advertised_refs(url, max_refs, mode)?;
    let mut refs = HashMap::new();
    for (name, oid) in advertised {
        let Some(stripped) = name.strip_prefix(REF_HEAD_PREFIX) else {
            continue;
        };
        let Ok(commit) = GitCommit::try_from(oid) else {
            continue;
        };
        refs.insert(stripped.to_string(), commit);
    }
    Ok(refs)
}

/// Returns every semver-parsed tag matching `path_prefix` (or root tags
/// when `path_prefix` is `None`) in descending precedence order.
///
/// Tags that don't carry the expected `v` (or `<prefix>/v`) shape, or
/// don't parse as semver, are silently skipped. Use this when the
/// caller wants to display the full list of available versions; use
/// [`select_version`] when only the best match is needed.
pub fn parse_versions(refs: &RemoteRefs, path_prefix: Option<&str>) -> Vec<Version> {
    let mut versions: Vec<Version> = refs
        .keys()
        .filter_map(|tag| tag.parse::<VersionTag>().ok())
        .filter(|t| t.prefix() == path_prefix)
        .map(VersionTag::into_version)
        .collect();
    versions.sort_by(|a, b| b.cmp(a));
    versions
}

/// Returns every parsed version that satisfies `requirement`, in
/// descending precedence order.
pub fn filter_matching(
    refs: &RemoteRefs,
    path_prefix: Option<&str>,
    requirement: &VersionRequirement,
) -> Vec<Version> {
    parse_versions(refs, path_prefix)
        .into_iter()
        .filter(|v| requirement.matches(v))
        .collect()
}

/// Selects the highest-precedence version that satisfies `requirement`,
/// scoped to `path_prefix` for path-prefixed multi-module repositories.
pub fn select_version(
    refs: &RemoteRefs,
    path_prefix: Option<&str>,
    requirement: &VersionRequirement,
) -> Result<Version, VersionError> {
    let parsed = parse_versions(refs, path_prefix);
    parsed
        .iter()
        .find(|v| requirement.matches(v))
        .cloned()
        .ok_or(VersionError::NoSatisfyingVersion {
            requirement: requirement.clone(),
            considered: parsed,
        })
}

/// Selects the highest-precedence version satisfying `requirement`
/// (scoped to `path_prefix`) and returns it together with the commit
/// the corresponding tag points at.
pub fn resolve_version_to_commit(
    refs: &RemoteRefs,
    path_prefix: Option<&str>,
    requirement: &VersionRequirement,
) -> Result<(Version, GitCommit), VersionError> {
    let version = select_version(refs, path_prefix, requirement)?;
    let tag = VersionTag::new(path_prefix.map(String::from), version.clone()).to_string();
    // SAFETY: `select_version` only returns versions parsed from keys
    // already present in `refs`, so the round-tripped tag is guaranteed
    // to exist in the map.
    let commit = refs.get(&tag).cloned().unwrap();
    Ok((version, commit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(s: &str) -> VersionRequirement {
        s.to_string().try_into().unwrap()
    }

    /// Builds a `RemoteRefs` map from tag names, using a sentinel commit
    /// for each entry.
    fn refs(items: &[&str]) -> RemoteRefs {
        let sentinel = GitCommit::try_from(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
                .to_string()
                .to_string(),
        )
        .unwrap();
        items
            .iter()
            .map(|s| (s.to_string(), sentinel.clone()))
            .collect()
    }

    #[test]
    fn version_tag_round_trips() {
        let parsed: VersionTag = "csvkit/v1.2.3".parse().unwrap();
        assert_eq!(parsed.prefix(), Some("csvkit"));
        assert_eq!(parsed.to_string(), "csvkit/v1.2.3");
        assert_eq!(parsed.into_version(), Version::parse("1.2.3").unwrap());

        let root: VersionTag = "v0.5.0".parse().unwrap();
        assert_eq!(root.prefix(), None);
        assert_eq!(root.to_string(), "v0.5.0");

        assert!("release-2026".parse::<VersionTag>().is_err());
        assert!("vXYZ".parse::<VersionTag>().is_err());
    }

    #[test]
    fn version_tag_new_displays_correctly() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(VersionTag::new(None, v.clone()).to_string(), "v1.2.3");
        assert_eq!(
            VersionTag::new(Some("csvkit".to_string()), v).to_string(),
            "csvkit/v1.2.3"
        );
    }

    #[test]
    fn selects_highest_root_version() {
        let v = select_version(
            &refs(&["v1.0.0", "v1.2.0", "v2.0.0", "csvkit/v0.5.0"]),
            None,
            &req("^1"),
        )
        .unwrap();
        assert_eq!(v, Version::parse("1.2.0").unwrap());
    }

    #[test]
    fn selects_path_prefixed_version() {
        let v = select_version(
            &refs(&["csvkit/v0.5.0", "csvkit/v0.6.0", "spellbook/v1.0.0"]),
            Some("csvkit"),
            &req(">=0.5"),
        )
        .unwrap();
        assert_eq!(v, Version::parse("0.6.0").unwrap());
    }

    #[test]
    fn ignores_non_semver_tags() {
        let v =
            select_version(&refs(&["v1.0.0", "release-2026", "vXYZ"]), None, &req("^1")).unwrap();
        assert_eq!(v, Version::parse("1.0.0").unwrap());
    }

    #[test]
    fn errors_when_no_satisfying_version() {
        let err = select_version(&refs(&["v1.0.0"]), None, &req("^2")).unwrap_err();
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
            &refs(&["csvkit/v1.0.0", "spellbook/v1.0.0"]),
            None,
            &req("^1"),
        )
        .unwrap_err();
        assert!(matches!(err, VersionError::NoSatisfyingVersion { .. }));
    }

    #[test]
    fn path_selector_ignores_root_tags() {
        let err =
            select_version(&refs(&["v1.0.0", "v2.0.0"]), Some("csvkit"), &req("^1")).unwrap_err();
        assert!(matches!(err, VersionError::NoSatisfyingVersion { .. }));
    }

    #[test]
    fn resolve_version_to_commit_round_trips_the_tag() {
        let mut refs = refs(&["v1.0.0", "v1.2.0", "v2.0.0"]);
        let target = GitCommit::try_from(
            "b1c2d3e4f5a6b1c2d3e4f5a6b1c2d3e4f5a6b1c2"
                .to_string()
                .to_string(),
        )
        .unwrap();
        refs.insert("v1.2.0".to_string(), target.clone());

        let (version, commit) = resolve_version_to_commit(&refs, None, &req("^1")).unwrap();
        assert_eq!(version, Version::parse("1.2.0").unwrap());
        assert_eq!(commit, target);
    }
}
