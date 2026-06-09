//! Post-materialization verification of module content.
//!
//! After a module tree is materialized on disk, the [`verify`] function
//! runs every integrity and policy check before the content is accepted:
//! resource-limit enforcement (file count and byte budget), content
//! hashing, large-file warnings, signature parsing and Ed25519
//! verification, trust-store key comparison, and lockfile checksum and
//! signer matching.

use std::path::Path;

use crate::Lockfile;
use crate::dependency::DependencyName;
use crate::hash::ContentHash;
use crate::module_walk;
use crate::resolver::config::LargeFileWarning;
use crate::resolver::error::ResolverError;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::trust::TrustStore;
use crate::signing::VerifyingKey;

/// Walks every regular file under `root` using the shared safe
/// module-content walker. Converts errors to [`ResolverError`].
fn walk_module_tree(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), ResolverError>,
) -> Result<module_walk::TreeStats, ResolverError> {
    module_walk::walk_module_tree(root, visitor).map_err(|e| match e {
        module_walk::WalkError::Walk(w) => ResolverError::Walk(w),
        module_walk::WalkError::Visitor(r) => r,
    })
}

/// Walks `module_root`, emits large-file warnings, and rejects the
/// tree if it exceeds configured file-count or byte-size limits.
fn check_materialized_tree(
    policy: &ResolverPolicy,
    name: &DependencyName,
    module_root: &Path,
) -> Result<(), ResolverError> {
    let large_file_threshold = match policy.large_file_warning {
        LargeFileWarning::Threshold(t) => Some(t),
        LargeFileWarning::Disabled => None,
    };
    let has_limits =
        policy.max_materialized_files.is_some() || policy.max_materialized_bytes.is_some();

    if large_file_threshold.is_none() && !has_limits {
        return Ok(());
    }

    let stats = walk_module_tree(module_root, &mut |entry, size| {
        if let Some(threshold) = large_file_threshold
            && size >= threshold
        {
            tracing::warn!(
                dep = name.manifest(),
                file = %entry.display(),
                size,
                threshold,
                "module contains a large file",
            );
        }
        Ok(())
    })?;

    if policy
        .max_materialized_files
        .is_some_and(|limit| stats.files > limit)
        || policy
            .max_materialized_bytes
            .is_some_and(|limit| stats.bytes > limit)
    {
        return Err(ResolverError::MaterializedTreeLimitExceeded {
            dep: name.manifest().to_string(),
            files: stats.files,
            bytes: stats.bytes,
        });
    }
    Ok(())
}

/// Artifacts produced by [`verify`].
#[derive(Debug)]
pub(crate) struct VerifiedModule {
    /// The module's content hash.
    pub checksum: ContentHash,
    /// The signer's public key, if the module was signed.
    pub signer: Option<VerifyingKey>,
}

/// Runs all verification checks on a materialized module root.
///
/// Checks run in order: tree walk (large-file warnings and resource
/// limits), content hashing, then signature verification. Each step
/// short-circuits on failure.
pub(crate) fn verify(
    policy: &ResolverPolicy,
    trust: &TrustStore,
    name: &DependencyName,
    module_root: &Path,
    source_id: Option<(&str, Option<&str>)>,
) -> Result<VerifiedModule, ResolverError> {
    check_materialized_tree(policy, name, module_root)?;
    let checksum = crate::hash::hash_directory(module_root)?;
    let signer = read_and_verify_signature(policy, trust, name, module_root, &checksum, source_id)?;
    Ok(VerifiedModule { checksum, signer })
}

/// Reads the signature file from `module_root` and verifies it.
fn read_and_verify_signature(
    policy: &ResolverPolicy,
    trust: &TrustStore,
    name: &DependencyName,
    module_root: &Path,
    checksum: &ContentHash,
    source_id: Option<(&str, Option<&str>)>,
) -> Result<Option<VerifyingKey>, ResolverError> {
    let sig_path = module_root.join(crate::SIGNATURE_FILENAME);
    let bytes = match std::fs::read(&sig_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if policy.require_signed {
                return Err(ResolverError::RequireSignedViolation {
                    dep: name.manifest().to_string(),
                });
            }
            return Ok(None);
        }
        Err(source) => {
            return Err(ResolverError::Io {
                path: sig_path,
                source,
            });
        }
    };
    let sig = crate::signing::ModuleSignature::parse(&bytes).map_err(|source| {
        ResolverError::SignatureParse {
            dep: name.manifest().to_string(),
            source,
        }
    })?;
    sig.verify(checksum)
        .map_err(|_| ResolverError::SignatureVerificationFailed {
            dep: name.manifest().to_string(),
            signer: Box::new(sig.public_key),
        })?;
    if let Some((source_url, source_path)) = source_id
        && let Some(trusted) = trust.lookup(name, source_url, source_path)
        && &sig.public_key != trusted
    {
        return Err(ResolverError::SignerKeyMismatch {
            dep: name.manifest().to_string(),
            expected: Box::new(*trusted),
            observed: Box::new(sig.public_key),
        });
    }
    Ok(Some(sig.public_key))
}

/// Checks a dependency's content hash and signer against the lockfile.
///
/// Called only by the `materialize` path, where a lockfile already
/// exists and the materialized content must match the locked
/// expectations.
pub(crate) fn verify_against_lockfile(
    lockfile: &Lockfile,
    name: &DependencyName,
    checksum: &ContentHash,
    signer: Option<&VerifyingKey>,
) -> Result<(), ResolverError> {
    let locked_entry =
        lockfile
            .dependencies
            .get(name)
            .ok_or_else(|| ResolverError::NotInLockfile {
                dep: name.manifest().to_string(),
            })?;
    if locked_entry.checksum != *checksum {
        return Err(ResolverError::ChecksumMismatch {
            dep: name.manifest().to_string(),
            expected: locked_entry.checksum,
            observed: *checksum,
        });
    }
    match (locked_entry.signer, signer) {
        (Some(expected), None) => {
            return Err(ResolverError::SignatureDowngrade {
                dep: name.manifest().to_string(),
                expected_signer: Box::new(expected),
            });
        }
        (Some(expected), Some(observed)) if expected != *observed => {
            return Err(ResolverError::SignerKeyMismatch {
                dep: name.manifest().to_string(),
                expected: Box::new(expected),
                observed: Box::new(*observed),
            });
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use semver::Version;
    use tempfile::tempdir;

    use super::*;
    use crate::dependency::GitSelector;
    use crate::lockfile::DependencyEntry;
    use crate::lockfile::GitCommit;
    use crate::lockfile::ResolvedSource;
    use crate::resolver::config::ModulesConfig;
    use crate::signing::test_utils::signing_key_from_seed;

    fn test_dep() -> DependencyName {
        DependencyName::try_from("foo".to_string()).unwrap()
    }

    fn test_source() -> ResolvedSource {
        ResolvedSource::Git {
            git: "https://github.com/example/foo".parse().unwrap(),
            commit: GitCommit::try_from("a".repeat(40)).unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: None,
        }
    }

    fn write_module(dir: &std::path::Path, content: &str) {
        fs::write(dir.join("index.wdl"), content).unwrap();
    }

    fn write_signed_module(dir: &std::path::Path, content: &str, seed: u64) {
        write_module(dir, content);
        let checksum = crate::hash::hash_directory(dir).unwrap();
        let signing_key = signing_key_from_seed(seed);
        let sig = crate::signing::ModuleSignature {
            public_key: signing_key.verifying_key(),
            signature: signing_key.sign(&checksum),
        };
        let mut buf = Vec::new();
        sig.write(&mut buf).unwrap();
        fs::write(dir.join(crate::SIGNATURE_FILENAME), buf).unwrap();
    }

    #[test]
    fn verify_unsigned_module() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.2\n");
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let result = verify(&policy, &trust, &test_dep(), dir.path(), None);
        assert!(result.is_ok(), "unsigned module should verify: {result:?}");
        assert!(result.unwrap().signer.is_none());
    }

    #[test]
    fn verify_signed_module() {
        let dir = tempdir().unwrap();
        write_signed_module(dir.path(), "version 1.2\n", 0xAB);
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let result = verify(&policy, &trust, &test_dep(), dir.path(), None);
        assert!(result.is_ok(), "signed module should verify: {result:?}");
        let verified = result.unwrap();
        assert!(verified.signer.is_some());
        assert_eq!(
            verified.signer.unwrap(),
            signing_key_from_seed(0xAB).verifying_key()
        );
    }

    #[test]
    fn require_signed_rejects_unsigned() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.2\n");
        let config = ModulesConfig {
            require_signed: true,
            ..Default::default()
        };
        let policy = ResolverPolicy::from(&config);
        let trust = TrustStore::default();
        let err = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap_err();
        assert!(
            matches!(err, ResolverError::RequireSignedViolation { .. }),
            "expected `RequireSignedViolation`, got: {err}"
        );
    }

    #[test]
    fn lockfile_checksum_mismatch() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.2\n");
        let checksum = crate::hash::hash_directory(dir.path()).unwrap();
        let wrong_checksum = ContentHash::from([0xFFu8; 32]);
        assert_ne!(checksum, wrong_checksum);

        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                version: Version::new(1, 0, 0),
                checksum: wrong_checksum,
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(&lockfile, &dep, &checksum, None).unwrap_err();
        assert!(
            matches!(err, ResolverError::ChecksumMismatch { .. }),
            "expected `ChecksumMismatch`, got: {err}"
        );
    }

    #[test]
    fn lockfile_checksum_match() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.2\n");
        let checksum = crate::hash::hash_directory(dir.path()).unwrap();

        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                version: Version::new(1, 0, 0),
                checksum,
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let result = verify_against_lockfile(&lockfile, &dep, &checksum, None);
        assert!(result.is_ok(), "matching checksum should pass: {result:?}");
    }

    #[test]
    fn signature_downgrade_detected() {
        let key = signing_key_from_seed(0xAB).verifying_key();
        let checksum = ContentHash::from([0x01u8; 32]);
        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                version: Version::new(1, 0, 0),
                checksum,
                signer: Some(key),
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(&lockfile, &dep, &checksum, None).unwrap_err();
        assert!(
            matches!(err, ResolverError::SignatureDowngrade { .. }),
            "expected `SignatureDowngrade`, got: {err}"
        );
    }

    #[test]
    fn signer_key_mismatch_detected() {
        let key_a = signing_key_from_seed(0xAB).verifying_key();
        let key_b = signing_key_from_seed(0xCD).verifying_key();
        let checksum = ContentHash::from([0x01u8; 32]);
        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                version: Version::new(1, 0, 0),
                checksum,
                signer: Some(key_a),
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(&lockfile, &dep, &checksum, Some(&key_b)).unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "expected `SignerKeyMismatch`, got: {err}"
        );
    }

    #[test]
    fn file_count_limit_exceeded() {
        let dir = tempdir().unwrap();
        for i in 0..5 {
            fs::write(dir.path().join(format!("file_{i}.wdl")), "version 1.2\n").unwrap();
        }
        let config = ModulesConfig {
            max_materialized_files: Some(2),
            ..Default::default()
        };
        let policy = ResolverPolicy::from(&config);
        let trust = TrustStore::default();
        let err = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap_err();
        assert!(
            matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
            "expected `MaterializedTreeLimitExceeded`, got: {err}"
        );
    }

    #[test]
    fn byte_limit_exceeded() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("big.wdl"), "x".repeat(1000)).unwrap();
        let config = ModulesConfig {
            max_materialized_bytes: Some(100),
            ..Default::default()
        };
        let policy = ResolverPolicy::from(&config);
        let trust = TrustStore::default();
        let err = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap_err();
        assert!(
            matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
            "expected `MaterializedTreeLimitExceeded`, got: {err}"
        );
    }

    #[test]
    fn not_in_lockfile() {
        let dep = test_dep();
        let checksum = ContentHash::from([0x01u8; 32]);
        let lockfile = Lockfile::default();
        let err = verify_against_lockfile(&lockfile, &dep, &checksum, None).unwrap_err();
        assert!(
            matches!(err, ResolverError::NotInLockfile { .. }),
            "expected `NotInLockfile`, got: {err}"
        );
    }

    #[test]
    fn trust_store_rejects_wrong_signer() {
        let dir = tempdir().unwrap();
        write_signed_module(dir.path(), "version 1.2\n", 0xAB);

        let trusted_key = signing_key_from_seed(0xCD).verifying_key();
        let dep = test_dep();
        let source_url = "https://github.com/example/foo";
        let trust = TrustStore {
            entries: vec![crate::resolver::trust::TrustEntry {
                dep: dep.clone(),
                source: source_url.to_string(),
                path: None,
                key: trusted_key,
            }],
        };
        let policy = ResolverPolicy::default();
        let err = verify(&policy, &trust, &dep, dir.path(), Some((source_url, None))).unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "expected `SignerKeyMismatch` from trust store, got: {err}"
        );
    }
}
