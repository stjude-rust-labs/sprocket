//! Lockfile relocking algorithms and change statistics.

use super::BTreeSet;
use super::DependencyChange;
use super::DependencyEntry;
use super::DependencyName;
use super::DependencySource;
use super::DependencyUpdate;
use super::GitSelector;
use super::Lockfile;
use super::Manifest;
use super::ResolvedDependency;
use super::ResolvedSource;
use super::ResolvedTree;

/// The outcome of a [`partial_relock`] call. Contains the merged
/// lockfile and a per-dependency summary the CLI uses to print
/// "Updating x v1.0.0 -> v1.5.0" style output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelockOutcome {
    /// The merged lockfile.
    pub lockfile: Lockfile,
    /// Per-dependency change summary.
    pub stats: RelockStats,
}

/// Per-dependency change summary produced by [`partial_relock`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelockStats {
    /// Number of dependencies whose existing locked entry still
    /// satisfied the consumer's source and was kept as-is.
    pub kept: usize,
    /// Dependencies introduced since the previous lockfile.
    pub added: Vec<DependencyChange>,
    /// Dependencies dropped from the previous lockfile because the
    /// consumer no longer declares them.
    pub removed: Vec<DependencyChange>,
    /// Dependencies refreshed during update whose resolved commit was
    /// already current.
    pub skipped: Vec<DependencyChange>,
    /// Dependencies whose locked entry changed.
    pub updated: Vec<DependencyUpdate>,
}

/// Performs a partial relock against an existing lockfile.
///
/// For each dependency declared by `consumer`
///
/// - If `existing` has an entry that still satisfies the (possibly updated)
///   source, keep it as-is. The dependency is not refetched.
/// - Otherwise (new dep, removed dep, tightened constraint that excludes the
///   locked version, source changed), use the freshly resolved entry from
///   `freshly_resolved`.
///
/// Dependencies declared by `consumer` but absent from
/// `freshly_resolved` are dropped, matching the case where a dep was
/// removed from `module.json` since the last resolution.
///
/// The returned [`RelockOutcome`] carries both the merged lockfile and a
/// per-dependency summary suitable for CLI output.
pub fn partial_relock(
    consumer: &Manifest,
    existing: &Lockfile,
    freshly_resolved: &ResolvedTree,
) -> Result<RelockOutcome, crate::resolver::error::ResolverError> {
    let mut lockfile = Lockfile::default();
    let mut stats = RelockStats::default();

    for (name, source) in &consumer.dependencies {
        let existing_entry = existing.dependencies.get(name);
        if let Some(prev) = existing_entry
            && satisfies(prev, source)
        {
            lockfile.dependencies.insert(name.clone(), prev.clone());
            stats.kept += 1;
            continue;
        }
        let Some(resolved) = freshly_resolved.dependencies.get(name) else {
            return Err(
                crate::resolver::error::ResolverError::MissingFreshDependency {
                    dep: name.manifest().to_string(),
                },
            );
        };
        let new_entry = resolved_to_lockfile_entry(resolved);
        match existing_entry {
            Some(prev) if prev == &new_entry => {
                lockfile.dependencies.insert(name.clone(), prev.clone());
                stats.kept += 1;
            }
            Some(prev) => stats.updated.push((name, prev, &new_entry).into()),
            None => stats.added.push((name, &new_entry).into()),
        }
        lockfile.dependencies.insert(name.clone(), new_entry);
    }

    for (name, entry) in &existing.dependencies {
        if !consumer.dependencies.contains_key(name) {
            stats.removed.push((name, entry).into());
        }
    }

    Ok(RelockOutcome { lockfile, stats })
}

/// Performs an update relock for selected dependency names.
///
/// An empty `selected` set updates every declared dependency. Entries
/// whose resolved Git commit is already current are recorded as skipped.
pub fn update_relock(
    consumer: &Manifest,
    existing: &Lockfile,
    freshly_resolved: &ResolvedTree,
    selected: &BTreeSet<DependencyName>,
) -> Result<RelockOutcome, crate::resolver::error::ResolverError> {
    let mut lockfile = Lockfile::default();
    let mut stats = RelockStats::default();
    let update_all = selected.is_empty();

    for name in consumer.dependencies.keys() {
        let should_update = update_all || selected.contains(name);
        let existing_entry = existing.dependencies.get(name);

        if !should_update && let Some(prev) = existing_entry {
            lockfile.dependencies.insert(name.clone(), prev.clone());
            stats.kept += 1;
            continue;
        }

        let Some(resolved) = freshly_resolved.dependencies.get(name) else {
            return Err(
                crate::resolver::error::ResolverError::MissingFreshDependency {
                    dep: name.manifest().to_string(),
                },
            );
        };
        let new_entry = resolved_to_lockfile_entry(resolved);
        match existing_entry {
            Some(prev) => {
                let is_current = match (&prev.source, &new_entry.source) {
                    (
                        ResolvedSource::Git {
                            sha: prev_commit, ..
                        },
                        ResolvedSource::Git {
                            sha: next_commit, ..
                        },
                    ) => prev_commit == next_commit,
                    _ => prev == &new_entry,
                };
                if is_current {
                    stats.skipped.push((name, &new_entry).into());
                } else {
                    stats.updated.push((name, prev, &new_entry).into());
                }
            }
            None => stats.added.push((name, &new_entry).into()),
        }
        lockfile.dependencies.insert(name.clone(), new_entry);
    }

    for (name, entry) in &existing.dependencies {
        if !consumer.dependencies.contains_key(name) {
            stats.removed.push((name, entry).into());
        }
    }

    Ok(RelockOutcome { lockfile, stats })
}

/// Returns true if the existing lockfile entry still satisfies the
/// requirement expressed by the consumer's [`DependencySource`].
pub(crate) fn satisfies(entry: &DependencyEntry, source: &DependencySource) -> bool {
    match (source, &entry.source) {
        (
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            },
            ResolvedSource::Git {
                git,
                sha: locked_commit,
                path: locked_path,
                selector: locked_selector,
                ..
            },
        ) => {
            if url != git {
                return false;
            }
            if path.as_ref() != locked_path.as_ref() {
                return false;
            }
            match selector {
                GitSelector::Version(_) | GitSelector::Tag(_) | GitSelector::Branch(_) => {
                    selector == locked_selector
                }
                GitSelector::Commit(c) => locked_commit.as_str().starts_with(c.as_str()),
            }
        }
        // A local path dependency carries no checksum or signature, so a
        // path lockfile entry is consistent with a path declaration. The
        // path value itself is re-read from the manifest at execution
        // time and validated against the lockfile there, so a changed
        // path is caught without treating every path dependency as stale.
        (DependencySource::LocalPath { .. }, ResolvedSource::Path { .. }) => true,
        _ => false,
    }
}

/// Converts a [`ResolvedDependency`] to the [`DependencyEntry`] shape
/// the lockfile records.
fn resolved_to_lockfile_entry(dep: &ResolvedDependency) -> DependencyEntry {
    DependencyEntry {
        source: dep.source.clone(),
        checksum: dep.checksum,
        signer: dep.signer,
        dependencies: dep
            .dependencies
            .iter()
            .map(|(n, d)| (n.clone(), resolved_to_lockfile_entry(d)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use semver::Version;

    use super::*;
    use crate::hash::ContentHash;
    use crate::lockfile::DependencyEntry;
    use crate::lockfile::ResolvedSource;
    use crate::signing::VerifyingKey;

    fn dn(s: &str) -> DependencyName {
        s.parse().unwrap()
    }

    fn checksum() -> ContentHash {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap()
    }

    fn git_source() -> ResolvedSource {
        ResolvedSource::Git {
            git: "https://example.com/repo".parse().unwrap(),
            sha: "0000000000000000000000000000000000000000".parse().unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: None,
        }
    }

    fn entry(signer: Option<VerifyingKey>) -> DependencyEntry {
        DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer,
            dependencies: BTreeMap::new(),
        }
    }

    fn manifest(name: &str, deps_toml: &str) -> Manifest {
        let json = format!(
            r#"{{
                "name": "{name}",
                "license": "MIT",
                "dependencies": {{ {deps_toml} }}
            }}"#
        );
        Manifest::parse(json.as_bytes()).unwrap()
    }

    #[test]
    fn relock_keeps_satisfying_entry() {
        let consumer = manifest(
            "consumer",
            r#""foo": { "git": "https://x/y", "version": "^1" }"#,
        );
        let mut existing = Lockfile::default();
        existing.dependencies.insert(
            dn("foo"),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                    selector: GitSelector::Version("^1".parse().unwrap()),
                },
                checksum: Some(checksum()),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &ResolvedTree::default()).unwrap();
        assert_eq!(outcome.stats.kept, 1);
        assert!(outcome.stats.added.is_empty());
        assert!(outcome.stats.updated.is_empty());
        assert!(outcome.stats.removed.is_empty());
    }

    #[test]
    fn relock_replaces_stale_entry() {
        let consumer = manifest(
            "consumer",
            r#""foo": { "git": "https://x/y", "version": "^2" }"#,
        );
        let mut existing = Lockfile::default();
        existing.dependencies.insert(
            dn("foo"),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                    selector: GitSelector::Version("^1".parse().unwrap()),
                },
                checksum: Some(checksum()),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let mut freshly = ResolvedTree::default();
        freshly.dependencies.insert(
            dn("foo"),
            ResolvedDependency {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000002".parse().unwrap(),
                    path: None,
                    selector: GitSelector::Version("^2".parse().unwrap()),
                },
                version: Some(Version::parse("2.0.0").unwrap()),
                checksum: Some(checksum()),
                signer: None,
                signer_identity: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly).unwrap();
        let entry = outcome.lockfile.dependencies.get(&dn("foo")).unwrap();
        assert!(matches!(entry.source, ResolvedSource::Git { .. }));

        assert_eq!(outcome.stats.updated.len(), 1);
        let update = &outcome.stats.updated[0];
        assert_eq!(update.name, dn("foo"));
        assert_eq!(update.from_path, None);
        assert_eq!(update.from_selector.as_deref(), Some("version ^1"));
        assert_eq!(
            update.from_commit.as_deref(),
            Some("0000000000000000000000000000000000000001")
        );
        assert_eq!(update.to_path, None);
        assert_eq!(update.to_selector.as_deref(), Some("version ^2"));
        assert_eq!(
            update.to_commit.as_deref(),
            Some("0000000000000000000000000000000000000002")
        );
    }

    #[test]
    fn relock_records_path_changes_in_update_stats() {
        let consumer = manifest(
            "consumer",
            r#""foo": { "git": "https://x/y", "version": "^1", "path": "modules/new" }"#,
        );
        let mut existing = Lockfile::default();
        existing.dependencies.insert(
            dn("foo"),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: Some("modules/old".parse().unwrap()),
                    selector: GitSelector::Version("^1".parse().unwrap()),
                },
                checksum: Some(checksum()),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let mut freshly = ResolvedTree::default();
        freshly.dependencies.insert(
            dn("foo"),
            ResolvedDependency {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: Some("modules/new".parse().unwrap()),
                    selector: GitSelector::Version("^1".parse().unwrap()),
                },
                version: Some(Version::parse("1.0.0").unwrap()),
                checksum: Some(checksum()),
                signer: None,
                signer_identity: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly).unwrap();

        assert_eq!(outcome.stats.updated.len(), 1);
        let update = &outcome.stats.updated[0];
        assert_eq!(update.from_path.as_deref(), Some("modules/old"));
        assert_eq!(update.from_selector.as_deref(), Some("version ^1"));
        assert_eq!(
            update.from_commit.as_deref(),
            Some("0000000000000000000000000000000000000001")
        );
        assert_eq!(update.to_path.as_deref(), Some("modules/new"));
        assert_eq!(update.to_selector.as_deref(), Some("version ^1"));
        assert_eq!(
            update.to_commit.as_deref(),
            Some("0000000000000000000000000000000000000001")
        );
    }

    #[test]
    fn relock_keeps_refreshed_branch_entry_when_unchanged() -> Result<(), Box<dyn std::error::Error>>
    {
        let consumer = manifest(
            "consumer",
            r#""foo": { "git": "https://x/y", "branch": "main", "path": "modules/foo" }"#,
        );
        let name = dn("foo");
        let entry = DependencyEntry {
            source: ResolvedSource::Git {
                git: "https://x/y".parse()?,
                sha: "0000000000000000000000000000000000000001".parse()?,
                path: Some("modules/foo".parse()?),
                selector: GitSelector::Branch("main".to_string()),
            },
            checksum: Some(checksum()),
            signer: None,
            dependencies: BTreeMap::new(),
        };
        let mut existing = Lockfile::default();
        existing.dependencies.insert(name.clone(), entry.clone());

        let mut freshly = ResolvedTree::default();
        freshly.dependencies.insert(
            name.clone(),
            ResolvedDependency {
                source: entry.source.clone(),
                version: None,
                checksum: entry.checksum,
                signer: entry.signer,
                signer_identity: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly)?;

        assert_eq!(outcome.lockfile.dependencies.get(&name), Some(&entry));
        assert_eq!(outcome.stats.kept, 1);
        assert!(outcome.stats.added.is_empty());
        assert!(outcome.stats.updated.is_empty());
        assert!(outcome.stats.removed.is_empty());
        Ok(())
    }

    #[test]
    fn relock_records_added_dep() {
        let consumer = manifest(
            "consumer",
            r#""foo": { "git": "https://x/y", "version": "^1" }"#,
        );
        let existing = Lockfile::default();
        let mut freshly = ResolvedTree::default();
        freshly.dependencies.insert(
            dn("foo"),
            ResolvedDependency {
                source: ResolvedSource::Git {
                    git: "https://x/y".parse().unwrap(),
                    sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                    selector: GitSelector::Version("^1".parse().unwrap()),
                },
                version: Some(Version::parse("1.0.0").unwrap()),
                checksum: Some(checksum()),
                signer: None,
                signer_identity: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly).unwrap();
        assert_eq!(outcome.stats.added.len(), 1);
        let added = &outcome.stats.added[0];
        assert_eq!(added.name, dn("foo"));
        assert_eq!(added.path, None);
        assert_eq!(added.selector.as_deref(), Some("version ^1"));
        assert_eq!(
            added.commit.as_deref(),
            Some("0000000000000000000000000000000000000001")
        );
    }

    #[test]
    fn relock_errors_when_consumer_dep_missing_from_fresh_tree() {
        let consumer = manifest("consumer", r#""foo": {"git":"https://x/y","tag":"v1"}"#);
        let existing = Lockfile::default();
        let fresh = ResolvedTree::default();
        let err = partial_relock(&consumer, &existing, &fresh).unwrap_err();
        assert!(
            matches!(
                err,
                crate::resolver::error::ResolverError::MissingFreshDependency { .. }
            ),
            "got: {err}"
        );
    }

    #[test]
    fn relock_drops_removed_deps_and_records_them() {
        let consumer = manifest("consumer", "");
        let mut existing = Lockfile::default();
        existing.dependencies.insert(dn("removed"), entry(None));
        let outcome = partial_relock(&consumer, &existing, &ResolvedTree::default()).unwrap();
        assert!(outcome.lockfile.dependencies.is_empty());
        assert_eq!(outcome.stats.removed.len(), 1);
        assert_eq!(outcome.stats.removed[0].name, dn("removed"));
        assert_eq!(outcome.stats.removed[0].path, None);
        assert_eq!(
            outcome.stats.removed[0].selector.as_deref(),
            Some("version ^1")
        );
        assert_eq!(
            outcome.stats.removed[0].commit.as_deref(),
            Some("0000000000000000000000000000000000000000")
        );
    }

    #[test]
    fn update_relock_skips_unchanged_local_path_dependency() {
        let consumer = manifest("consumer", r#""foo": { "path": "../foo" }"#);
        let mut existing = Lockfile::default();
        existing.dependencies.insert(
            dn("foo"),
            DependencyEntry {
                source: ResolvedSource::Path {
                    path: "../foo".into(),
                },
                checksum: Some(checksum()),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let mut freshly = ResolvedTree::default();
        freshly.dependencies.insert(
            dn("foo"),
            ResolvedDependency {
                source: ResolvedSource::Path {
                    path: "../foo".into(),
                },
                version: None,
                checksum: Some(checksum()),
                signer: None,
                signer_identity: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = update_relock(&consumer, &existing, &freshly, &BTreeSet::new()).unwrap();

        assert_eq!(outcome.stats.skipped.len(), 1);
        assert_eq!(outcome.stats.skipped[0].name, dn("foo"));
        assert_eq!(outcome.stats.skipped[0].path, None);
        assert_eq!(outcome.stats.skipped[0].selector, None);
        assert_eq!(outcome.stats.skipped[0].commit, None);
        assert!(outcome.stats.updated.is_empty());
    }
}
