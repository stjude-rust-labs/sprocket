//! Lockfile signer and dependency diffing.

use super::BTreeMap;
use super::DependencyEntry;
use super::DependencyMap;
use super::DependencyName;
use super::Lockfile;
use super::ResolvedDependency;
use super::ResolvedTree;
use super::SignerIdentity;
use super::VerifyingKey;

/// Signer identities keyed by dependency chain.
pub type SignerIdentityMap = BTreeMap<Vec<DependencyName>, SignerIdentity>;

/// The diff between an existing lockfile and a freshly-computed one.
///
/// CLI commands that write a lockfile (`lock`, `add`, `update`, etc.)
/// inspect this to enforce signer trust policy:
///
/// - [`new_signers`](Self::new_signers): a brand-new signed dependency.
/// - [`changed_signers`](Self::changed_signers): a dependency whose recorded
///   signer key changed.
/// - [`removed_signers`](Self::removed_signers): a previously signed dependency
///   that now resolves unsigned.
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
    /// signer. The configured trust mode determines whether these
    /// security-relevant transitions are refused, prompted, or accepted.
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
            path: entry.source_path().map(str::to_string),
            selector: entry.git_selector().map(ToString::to_string),
            commit: entry.git_sha().map(ToString::to_string),
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
            from_path: previous.source_path().map(str::to_string),
            from_selector: previous.git_selector().map(ToString::to_string),
            from_commit: previous.git_sha().map(ToString::to_string),
            to_path: next.source_path().map(str::to_string),
            to_selector: next.git_selector().map(ToString::to_string),
            to_commit: next.git_sha().map(ToString::to_string),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::dependency::GitSelector;
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

    fn key(seed: u64) -> VerifyingKey {
        crate::signing::test_utils::signing_key_from_seed(seed).verifying_key()
    }

    #[test]
    fn empty_diff_for_identical_lockfiles() {
        let mut lock = Lockfile::default();
        lock.dependencies.insert(dn("openwdl"), entry(None));
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
        new.dependencies.insert(dn("openwdl"), entry(Some(key(7))));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.new_signers.len(), 1);
        assert_eq!(diff.unsigned_added, 0);
        assert!(diff.has_new_signers());
        assert!(!diff.has_signer_changes());
    }

    #[test]
    fn lists_changed_signer() {
        let mut prev = Lockfile::default();
        prev.dependencies.insert(dn("openwdl"), entry(Some(key(7))));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("openwdl"), entry(Some(key(99))));

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
        let signed = entry(Some(key(7)));
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
        prev.dependencies.insert(dn("openwdl"), entry(None));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("openwdl"), entry(Some(key(7))));

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
        prev.dependencies.insert(dn("openwdl"), entry(Some(key(7))));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("openwdl"), entry(None));

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
        let new_nested = entry(None);
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
        prev.dependencies.insert(dn("kept"), entry(None));
        let mut new = Lockfile::default();
        new.dependencies.insert(dn("kept"), entry(None));
        new.dependencies.insert(dn("added"), entry(None));

        let diff = LockfileDiff::compute(&prev, &new);
        assert_eq!(diff.unsigned_added, 1);
        assert!(diff.new_signers.is_empty());
    }
}
