//! Lockfile generation and diff helpers.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::DependencyMap;
use crate::lockfile::ResolvedSource;
use crate::resolver::types::ResolvedDependency;
use crate::resolver::types::ResolvedTree;
use crate::signing::SignerIdentity;
use crate::signing::VerifyingKey;

/// Signer identities keyed by dependency chain.
pub type SignerIdentityMap = BTreeMap<Vec<DependencyName>, SignerIdentity>;

/// The diff between an existing lockfile and a freshly-computed one.
///
/// CLI commands that write a lockfile (`lock`, `add`, `update`, etc.)
/// inspect this to enforce explicit signer trust:
///
/// - [`new_signers`](Self::new_signers): a brand-new signed dependency.
/// - [`changed_signers`](Self::changed_signers): a dependency whose recorded
///   signer key changed. Must be refused until the new key is explicitly
///   trusted (spec trust-model rule 3).
/// - [`removed_signers`](Self::removed_signers): a previously signed dependency
///   that now resolves unsigned. Must be refused until explicitly accepted
///   (rule 5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockfileDiff {
    /// Signed modules whose signer key is being introduced for the first
    /// time (no key was previously recorded).
    pub new_signers: Vec<NewSigner>,
    /// Signed modules whose recorded signer key changed to a different key.
    pub changed_signers: Vec<ChangedSigner>,
    /// Previously signed modules that now resolve without a signature.
    pub removed_signers: Vec<RemovedSigner>,
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
    /// Optional signer identity metadata.
    pub identity: Option<SignerIdentity>,
}

impl NewSigner {
    /// The leaf dependency name (last element of `dep_chain`).
    pub fn dep(&self) -> &DependencyName {
        // SAFETY: `dep_chain` is non-empty for every `NewSigner` produced
        // by `LockfileDiff::compute`.
        self.dep_chain.last().unwrap()
    }
}

/// A module entry whose recorded signer key changed since the previous
/// lockfile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedSigner {
    /// The chain of dependency names from the consumer down to the
    /// module entry.
    pub dep_chain: Vec<DependencyName>,
    /// The signer key recorded in the previous lockfile.
    pub old_key: Option<VerifyingKey>,
    /// The signer key recorded in the refreshed lockfile.
    pub new_key: VerifyingKey,
    /// Optional new signer identity metadata.
    pub identity: Option<SignerIdentity>,
}

impl ChangedSigner {
    /// The leaf dependency name (last element of `dep_chain`).
    pub fn dep(&self) -> &DependencyName {
        // SAFETY: `dep_chain` is non-empty for every `ChangedSigner`
        // produced by `LockfileDiff::compute`.
        self.dep_chain.last().unwrap()
    }
}

/// A module entry whose previously recorded signature has been removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedSigner {
    /// The chain of dependency names from the consumer down to the entry.
    pub dep_chain: Vec<DependencyName>,
    /// The signer key recorded before removal.
    pub key: VerifyingKey,
}

impl RemovedSigner {
    /// The leaf dependency name (last element of `dep_chain`).
    pub fn dep(&self) -> &DependencyName {
        // SAFETY: `dep_chain` is non-empty for every `RemovedSigner`
        // produced by `LockfileDiff::compute`.
        self.dep_chain.last().unwrap()
    }
}

impl LockfileDiff {
    /// Computes the diff that the prompt would render.
    ///
    /// Recursively walks all `dependencies` maps (top-level and nested)
    /// so transitive signer changes are detected.
    pub fn compute(previous: &Lockfile, new: &Lockfile) -> Self {
        Self::compute_with_identities(previous, new, &SignerIdentityMap::new())
    }

    /// Computes the diff and attaches signer identity metadata gathered
    /// from freshly verified `module.sig` files.
    pub fn compute_with_identities(
        previous: &Lockfile,
        new: &Lockfile,
        identities: &SignerIdentityMap,
    ) -> Self {
        let mut diff = Self::default();
        walk_dep_map(
            Some(&previous.dependencies),
            &new.dependencies,
            &mut Vec::new(),
            &mut diff,
            identities,
        );
        diff
    }

    /// Returns true if the diff introduces a signer not present in the
    /// previous lockfile.
    pub fn has_new_signers(&self) -> bool {
        !self.new_signers.is_empty()
    }

    /// Returns true if the diff changes or removes a previously recorded
    /// signer. These are security-relevant transitions that must be
    /// refused until explicitly accepted, regardless of `trust_mode`.
    pub fn has_signer_changes(&self) -> bool {
        !self.changed_signers.is_empty() || !self.removed_signers.is_empty()
    }
}

/// Recursive helper for [`LockfileDiff::compute`].
fn walk_dep_map(
    prev: Option<&DependencyMap>,
    new: &DependencyMap,
    chain: &mut Vec<DependencyName>,
    diff: &mut LockfileDiff,
    identities: &SignerIdentityMap,
) {
    for (dep, entry) in new {
        chain.push(dep.clone());
        let prev_entry = prev.and_then(|p| p.get(dep));
        let prev_signer = prev_entry.and_then(|e| e.signer);
        match (entry.signer, prev_signer) {
            // Unchanged signer.
            (Some(new_key), Some(prev_key)) if new_key == prev_key => {}
            // A changed key requires explicit acceptance (rule 3).
            (Some(new_key), Some(prev_key)) => diff.changed_signers.push(ChangedSigner {
                dep_chain: chain.clone(),
                old_key: Some(prev_key),
                new_key,
                identity: identities.get(chain).cloned(),
            }),
            // A brand-new dependency that ships signed requires explicit
            // signer trust. An existing unsigned dependency that gains a
            // signature is also security-relevant and is treated as a
            // signer change.
            (Some(new_key), None) if prev_entry.is_none() => diff.new_signers.push(NewSigner {
                dep_chain: chain.clone(),
                key: new_key,
                identity: identities.get(chain).cloned(),
            }),
            (Some(new_key), None) => diff.changed_signers.push(ChangedSigner {
                dep_chain: chain.clone(),
                old_key: None,
                new_key,
                identity: identities.get(chain).cloned(),
            }),
            // A previously recorded signature has been removed; this is a
            // downgrade and requires explicit acceptance.
            (None, Some(prev_key)) => diff.removed_signers.push(RemovedSigner {
                dep_chain: chain.clone(),
                key: prev_key,
            }),
            (None, None) => {
                if prev_entry.is_none() {
                    diff.unsigned_added += 1;
                }
            }
        }
        let prev_nested = prev_entry.map(|e| &e.dependencies);
        walk_dep_map(prev_nested, &entry.dependencies, chain, diff, identities);
        chain.pop();
    }
}

/// Collects signer identity metadata from a resolved dependency tree.
pub fn signer_identity_map(tree: &ResolvedTree) -> SignerIdentityMap {
    let mut identities = SignerIdentityMap::new();
    collect_signer_identities(&tree.dependencies, &mut Vec::new(), &mut identities);
    identities
}

/// Recursively records signer identities by dependency chain.
fn collect_signer_identities(
    deps: &BTreeMap<DependencyName, ResolvedDependency>,
    chain: &mut Vec<DependencyName>,
    identities: &mut SignerIdentityMap,
) {
    for (name, dep) in deps {
        chain.push(name.clone());
        if let Some(identity) = dep.signer_identity.clone() {
            identities.insert(chain.clone(), identity);
        }
        collect_signer_identities(&dep.dependencies, chain, identities);
        chain.pop();
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

/// A dependency added or removed during relock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyChange {
    /// The dependency name.
    pub name: DependencyName,
    /// The source path. `None` means the source root.
    pub path: Option<String>,
    /// The Git selector. `None` for local dependencies.
    pub selector: Option<String>,
    /// The resolved Git commit. `None` for local dependencies.
    pub commit: Option<String>,
}

impl From<(&DependencyName, &DependencyEntry)> for DependencyChange {
    fn from((name, entry): (&DependencyName, &DependencyEntry)) -> Self {
        Self {
            name: name.clone(),
            path: entry.source.source_path().map(str::to_string),
            selector: entry.source.git_selector().map(ToString::to_string),
            commit: entry.source.git_sha().map(ToString::to_string),
        }
    }
}

/// A dependency whose locked entry changed during relock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyUpdate {
    /// The dependency name.
    pub name: DependencyName,
    /// The previous source path. `None` means the source root.
    pub from_path: Option<String>,
    /// The previous Git selector. `None` for local dependencies.
    pub from_selector: Option<String>,
    /// The previous resolved Git commit. `None` for local dependencies.
    pub from_commit: Option<String>,
    /// The new source path. `None` means the source root.
    pub to_path: Option<String>,
    /// The new Git selector. `None` for local dependencies.
    pub to_selector: Option<String>,
    /// The new resolved Git commit. `None` for local dependencies.
    pub to_commit: Option<String>,
}

impl From<(&DependencyName, &DependencyEntry, &DependencyEntry)> for DependencyUpdate {
    fn from((name, previous, next): (&DependencyName, &DependencyEntry, &DependencyEntry)) -> Self {
        Self {
            name: name.clone(),
            from_path: previous.source.source_path().map(str::to_string),
            from_selector: previous.source.git_selector().map(ToString::to_string),
            from_commit: previous.source.git_sha().map(ToString::to_string),
            to_path: next.source.source_path().map(str::to_string),
            to_selector: next.source.git_selector().map(ToString::to_string),
            to_commit: next.source.git_sha().map(ToString::to_string),
        }
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
            Some(prev) if refreshed_entry_is_current(prev, &new_entry) => {
                stats.skipped.push((name, &new_entry).into());
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

/// Returns true when a refreshed entry did not advance the resolved source.
fn refreshed_entry_is_current(prev: &DependencyEntry, next: &DependencyEntry) -> bool {
    match (&prev.source, &next.source) {
        (
            ResolvedSource::Git {
                sha: prev_commit, ..
            },
            ResolvedSource::Git {
                sha: next_commit, ..
            },
        ) => prev_commit == next_commit,
        _ => prev == next,
    }
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

    fn dn(s: &str) -> DependencyName {
        s.parse().unwrap()
    }

    fn checksum() -> ContentHash {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap()
    }

    /// A Git source with a fixed URL and commit. Signers and checksums
    /// only attach to Git sources, so signer-diff tests use this.
    fn git_source() -> ResolvedSource {
        ResolvedSource::Git {
            git: "https://example.com/repo".parse().unwrap(),
            sha: "0000000000000000000000000000000000000000".parse().unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: None,
        }
    }

    fn entry(_version: &str, signer: Option<VerifyingKey>) -> DependencyEntry {
        DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
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
        assert!(!diff.has_new_signers());
        assert!(!diff.has_signer_changes());
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
        assert!(diff.has_new_signers());
        assert!(!diff.has_signer_changes());
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
        // A changed key is a signer change, not a new signer.
        assert!(diff.new_signers.is_empty());
        assert_eq!(diff.changed_signers.len(), 1);
        assert_eq!(diff.changed_signers[0].old_key, Some(key(7)));
        assert_eq!(diff.changed_signers[0].new_key, key(99));
        assert!(diff.has_signer_changes());
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
        assert!(diff.changed_signers.is_empty());
    }

    #[test]
    fn unsigned_to_signed_is_signer_change() {
        let mut prev = Lockfile::default();
        prev.dependencies
            .insert(dn("openwdl"), entry("1.0.0", None));
        let mut new = Lockfile::default();
        new.dependencies
            .insert(dn("openwdl"), entry("1.0.0", Some(key(7))));

        let diff = LockfileDiff::compute(&prev, &new);
        assert!(diff.new_signers.is_empty());
        assert_eq!(diff.changed_signers.len(), 1);
        assert_eq!(diff.changed_signers[0].old_key, None);
        assert_eq!(diff.changed_signers[0].new_key, key(7));
        assert!(!diff.has_new_signers());
        assert!(diff.has_signer_changes());
    }

    #[test]
    fn removed_signature_requires_confirmation() {
        // A previously signed dependency that resolves without a
        // signature is a downgrade requiring explicit acceptance (rule 5).
        let mut prev = Lockfile::default();
        prev.dependencies
            .insert(dn("openwdl"), entry("1.0.0", Some(key(7))));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("openwdl"), entry("1.0.0", None));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.removed_signers.len(), 1);
        assert_eq!(diff.removed_signers[0].key, key(7));
        assert_eq!(diff.removed_signers[0].dep(), &dn("openwdl"));
        assert!(diff.has_signer_changes());
    }

    #[test]
    fn removed_signature_recurses_through_nested_dependencies() {
        let signer = key(11);
        let prev_nested = DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer: Some(signer),
            dependencies: BTreeMap::new(),
        };
        let prev_outer = DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer: None,
            dependencies: BTreeMap::from([(dn("bar"), prev_nested)]),
        };
        let mut prev = Lockfile::default();
        prev.dependencies.insert(dn("foo"), prev_outer);

        // The nested dependency loses its signature.
        let new_nested = entry("1.0.0", None);
        let new_outer = DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer: None,
            dependencies: BTreeMap::from([(dn("bar"), new_nested)]),
        };
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("foo"), new_outer);

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.removed_signers.len(), 1);
        assert_eq!(
            diff.removed_signers[0].dep_chain,
            vec![dn("foo"), dn("bar")]
        );
        assert!(diff.has_signer_changes());
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
        existing
            .dependencies
            .insert(dn("removed"), entry("1.0.0", None));
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

    #[test]
    fn signer_diff_recurses_through_nested_dependencies() {
        let previous = Lockfile::default();
        let signer = crate::signing::test_utils::signing_key_from_seed(11).verifying_key();
        let nested_entry = DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer: Some(signer),
            dependencies: BTreeMap::new(),
        };
        let outer_entry = DependencyEntry {
            source: git_source(),
            checksum: Some(checksum()),
            signer: None,
            dependencies: BTreeMap::from([(dn("bar"), nested_entry)]),
        };
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("foo"), outer_entry);

        let diff = LockfileDiff::compute(&previous, &new);
        assert_eq!(diff.new_signers.len(), 1);
        assert_eq!(diff.new_signers[0].dep_chain, vec![dn("foo"), dn("bar")]);
        assert!(diff.has_new_signers());
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
