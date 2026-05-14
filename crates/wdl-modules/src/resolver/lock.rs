//! Lockfile generation and diff helpers.

use semver::Version;

use crate::DependencyEntry;
use crate::DependencyMap;
use crate::DependencyName;
use crate::DependencySource;
use crate::GitSelector;
use crate::Lockfile;
use crate::Manifest;
use crate::ResolvedSource;
use crate::VerifyingKey;
use crate::resolver::types::ResolvedDependency;
use crate::resolver::types::ResolvedTree;

/// The diff between an existing lockfile and a freshly-computed one.
///
/// CLI commands that write a lockfile (`lock`, `add`, `update`, etc.)
/// inspect this to render the confirm-mode prompt. The prompt fires
/// only when the diff introduces new `signer` entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockfileDiff {
    /// Signed modules whose signer key is being introduced or changed.
    pub new_signers: Vec<NewSigner>,
    /// Count of unsigned modules being newly added (no `signer` field).
    pub unsigned_added: usize,
}

/// A module entry whose signer key is new or changed since the previous
/// lockfile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSigner {
    /// The chain of dependency names from the consumer down to the
    /// module entry, e.g. `["foo", "bar"]` for a transitive signer one
    /// level deep.
    pub dep_chain: Vec<DependencyName>,
    /// The new signer key.
    pub key: VerifyingKey,
}

impl NewSigner {
    /// The leaf dependency name (last element of `dep_chain`).
    pub fn dep(&self) -> &DependencyName {
        // SAFETY: `dep_chain` is non-empty for every `NewSigner` produced
        // by `LockfileDiff::compute`.
        self.dep_chain.last().unwrap()
    }
}

impl LockfileDiff {
    /// Computes the diff that the prompt would render.
    ///
    /// Recursively walks all `dependencies` maps (top-level and nested)
    /// so transitive signer changes are detected.
    pub fn compute(previous: &Lockfile, new: &Lockfile) -> Self {
        let mut diff = Self::default();
        walk_dep_map(
            Some(&previous.dependencies),
            &new.dependencies,
            &mut Vec::new(),
            &mut diff,
        );
        diff
    }

    /// Returns true if the diff would trigger a `trust_mode = "confirm"`
    /// prompt.
    pub fn requires_confirmation(&self) -> bool {
        !self.new_signers.is_empty()
    }
}

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
    pub added: Vec<DependencyAddition>,
    /// Dependencies dropped from the previous lockfile because the
    /// consumer no longer declares them.
    pub removed: Vec<DependencyName>,
    /// Dependencies whose locked entry changed.
    pub updated: Vec<DependencyUpdate>,
}

/// A dependency added to the lockfile during relock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyAddition {
    /// The dependency name.
    pub name: DependencyName,
    /// The version the new entry pins to. `None` when the dependency
    /// has no recorded modules (an unusual edge case).
    pub version: Option<Version>,
}

/// A dependency whose locked entry changed during relock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyUpdate {
    /// The dependency name.
    pub name: DependencyName,
    /// The version pinned in the previous lockfile. `None` when no
    /// prior version was recorded.
    pub from: Option<Version>,
    /// The version the new entry pins to. `None` when the dependency
    /// has no recorded modules.
    pub to: Option<Version>,
}

/// Recursive helper for [`LockfileDiff::compute`].
fn walk_dep_map(
    prev: Option<&DependencyMap>,
    new: &DependencyMap,
    chain: &mut Vec<DependencyName>,
    diff: &mut LockfileDiff,
) {
    for (dep, entry) in new {
        chain.push(dep.clone());
        let prev_entry = prev.and_then(|p| p.get(dep));
        let prev_signer = prev_entry.and_then(|e| e.signer);
        match (entry.signer, prev_signer) {
            (Some(new_key), Some(prev_key)) if new_key == prev_key => {}
            (Some(new_key), _) => diff.new_signers.push(NewSigner {
                dep_chain: chain.clone(),
                key: new_key,
            }),
            (None, _) => {
                if prev_entry.is_none() {
                    diff.unsigned_added += 1;
                }
            }
        }
        let prev_nested = prev_entry.map(|e| &e.dependencies);
        walk_dep_map(prev_nested, &entry.dependencies, chain, diff);
        chain.pop();
    }
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
                crate::resolver::error::ResolverError::MissingFreshDependency { dep: name.manifest().to_string() },
            );
        };
        let new_entry = resolved_to_lockfile_entry(resolved);
        let new_version = primary_version(&new_entry);
        match existing_entry {
            Some(prev) => stats.updated.push(DependencyUpdate {
                name: name.clone(),
                from: primary_version(prev),
                to: new_version,
            }),
            None => stats.added.push(DependencyAddition {
                name: name.clone(),
                version: new_version,
            }),
        }
        lockfile.dependencies.insert(name.clone(), new_entry);
    }

    for name in existing.dependencies.keys() {
        if !consumer.dependencies.contains_key(name) {
            stats.removed.push(name.clone());
        }
    }

    Ok(RelockOutcome { lockfile, stats })
}

/// Returns the version of a dependency entry, used to summarize
/// version transitions in [`RelockStats`].
fn primary_version(entry: &DependencyEntry) -> Option<Version> {
    Some(entry.version.clone())
}

/// Returns true if the existing lockfile entry still satisfies the
/// requirement expressed by the consumer's [`DependencySource`].
fn satisfies(entry: &DependencyEntry, source: &DependencySource) -> bool {
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
                commit: locked_commit,
                path: locked_path,
            },
        ) => {
            if url != git {
                return false;
            }
            if path.as_ref() != locked_path.as_ref() {
                return false;
            }
            match selector {
                GitSelector::Version(req) => req.matches(&entry.version),
                GitSelector::Commit(c) => c.as_str() == locked_commit.as_str(),
                // Tag and branch selectors are mutable refs; the
                // lockfile cannot know whether the remote has moved
                // them, so the lock entry is never considered
                // satisfying. The caller must re-resolve.
                GitSelector::Tag(_) | GitSelector::Branch(_) => false,
            }
        }
        // Local-path content is mutable; always re-resolve to pick
        // up changed files, dependencies, or signatures.
        (DependencySource::LocalPath { .. }, ResolvedSource::Path { .. }) => false,
        _ => false,
    }
}

/// Converts a [`ResolvedDependency`] to the [`DependencyEntry`] shape
/// the lockfile records.
fn resolved_to_lockfile_entry(dep: &ResolvedDependency) -> DependencyEntry {
    DependencyEntry {
        source: dep.source.clone(),
        version: dep.version.clone(),
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

    use semver::Version;

    use super::*;
    use crate::ContentHash;
    use crate::DependencyEntry;
    use crate::ResolvedSource;

    fn dn(s: &str) -> DependencyName {
        DependencyName::try_from(s.to_string()).unwrap()
    }

    fn checksum() -> ContentHash {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap()
    }

    fn entry(version: &str, signer: Option<VerifyingKey>) -> DependencyEntry {
        DependencyEntry {
            source: ResolvedSource::Path { path: ".".into() },
            version: Version::parse(version).unwrap(),
            checksum: checksum(),
            signer,
            dependencies: BTreeMap::new(),
        }
    }

    fn key(seed: u64) -> VerifyingKey {
        crate::signing::test_utils::signing_key_from_seed(seed).verifying_key()
    }

    #[test]
    fn empty_diff_for_identical_lockfiles() {
        let mut lock = Lockfile::default();
        lock.dependencies
            .insert(dn("openwdl"), entry("1.0.0", None));
        let diff = LockfileDiff::compute(&lock, &lock);
        assert!(diff.new_signers.is_empty());
        assert_eq!(diff.unsigned_added, 0);
        assert!(!diff.requires_confirmation());
    }

    #[test]
    fn lists_added_signer() {
        let prev = Lockfile::default();
        let mut new = Lockfile::default();
        new.dependencies
            .insert(dn("openwdl"), entry("1.0.0", Some(key(7))));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.new_signers.len(), 1);
        assert_eq!(diff.unsigned_added, 0);
        assert!(diff.requires_confirmation());
    }

    #[test]
    fn lists_changed_signer() {
        let mut prev = Lockfile::default();
        prev.dependencies
            .insert(dn("openwdl"), entry("1.0.0", Some(key(7))));
        let mut new = Lockfile::default();
        new.dependencies
            .insert(dn("openwdl"), entry("1.0.0", Some(key(99))));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.new_signers.len(), 1);
        assert_eq!(diff.new_signers[0].key, key(99));
    }

    #[test]
    fn unchanged_signer_does_not_appear() {
        let signed = entry("1.0.0", Some(key(7)));
        let mut prev = Lockfile::default();
        prev.dependencies.insert(dn("openwdl"), signed.clone());
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("openwdl"), signed);

        let diff = LockfileDiff::compute(&prev, &new);
        assert!(diff.new_signers.is_empty());
    }

    fn manifest(name: &str, deps_toml: &str) -> Manifest {
        let json = format!(
            r#"{{
                "name": "{name}",
                "version": "1.0.0",
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
                    commit: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                },
                version: Version::parse("1.0.0").unwrap(),
                checksum: checksum(),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &ResolvedTree::default()).unwrap();
        let kept = outcome.lockfile.dependencies.get(&dn("foo")).unwrap();
        assert_eq!(kept.version, Version::parse("1.0.0").unwrap());
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
                    commit: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                },
                version: Version::parse("1.0.0").unwrap(),
                checksum: checksum(),
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
                    commit: "0000000000000000000000000000000000000002".parse().unwrap(),
                    path: None,
                },
                version: Version::parse("2.0.0").unwrap(),
                checksum: checksum(),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly).unwrap();
        let entry = outcome.lockfile.dependencies.get(&dn("foo")).unwrap();
        assert_eq!(entry.version, Version::parse("2.0.0").unwrap());

        assert_eq!(outcome.stats.updated.len(), 1);
        let update = &outcome.stats.updated[0];
        assert_eq!(update.name, dn("foo"));
        assert_eq!(update.from, Some(Version::parse("1.0.0").unwrap()));
        assert_eq!(update.to, Some(Version::parse("2.0.0").unwrap()));
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
                    commit: "0000000000000000000000000000000000000001".parse().unwrap(),
                    path: None,
                },
                version: Version::parse("1.0.0").unwrap(),
                checksum: checksum(),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );

        let outcome = partial_relock(&consumer, &existing, &freshly).unwrap();
        assert_eq!(outcome.stats.added.len(), 1);
        let added = &outcome.stats.added[0];
        assert_eq!(added.name, dn("foo"));
        assert_eq!(added.version, Some(Version::parse("1.0.0").unwrap()));
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
        existing
            .dependencies
            .insert(dn("removed"), entry("1.0.0", None));
        let outcome = partial_relock(&consumer, &existing, &ResolvedTree::default()).unwrap();
        assert!(outcome.lockfile.dependencies.is_empty());
        assert_eq!(outcome.stats.removed, vec![dn("removed")]);
    }

    #[test]
    fn signer_diff_recurses_through_nested_dependencies() {
        let previous = Lockfile::default();
        let signer = crate::signing::test_utils::signing_key_from_seed(11).verifying_key();
        let nested_entry = DependencyEntry {
            source: ResolvedSource::Path {
                path: "/nested".into(),
            },
            version: Version::parse("1.0.0").unwrap(),
            checksum: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            signer: Some(signer),
            dependencies: BTreeMap::new(),
        };
        let outer_entry = DependencyEntry {
            source: ResolvedSource::Path {
                path: "/outer".into(),
            },
            version: Version::parse("1.0.0").unwrap(),
            checksum: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            signer: None,
            dependencies: BTreeMap::from([(dn("bar"), nested_entry)]),
        };
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("foo"), outer_entry);

        let diff = LockfileDiff::compute(&previous, &new);
        assert_eq!(diff.new_signers.len(), 1);
        assert_eq!(diff.new_signers[0].dep_chain, vec![dn("foo"), dn("bar")]);
        assert!(diff.requires_confirmation());
    }

    #[test]
    fn counts_unsigned_additions_only_for_new_entries() {
        let mut prev = Lockfile::default();
        prev.dependencies.insert(dn("kept"), entry("1.0.0", None));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("kept"), entry("1.0.0", None));
        new.dependencies.insert(dn("added"), entry("1.0.0", None));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.unsigned_added, 1);
        assert!(diff.new_signers.is_empty());
    }
}
