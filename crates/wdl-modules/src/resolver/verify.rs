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
use crate::signing::SignerIdentity;
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
    pub signer: Option<VerifiedSigner>,
}

/// Verified signer metadata extracted from `module.sig`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedSigner {
    /// The signer key found in `module.sig`.
    pub key: VerifyingKey,
    /// Optional signer identity found in `module.sig`.
    pub identity: Option<SignerIdentity>,
}

/// Runs all verification checks on a materialized module root.
///
/// Checks run in order: tree walk (large-file warnings and resource
/// limits), content hashing, then signature verification. Each step
/// short-circuits on failure.
pub(crate) fn verify(
    policy: &ResolverPolicy,
    _trust: &TrustStore,
    name: &DependencyName,
    module_root: &Path,
    _source_id: Option<(&str, Option<&str>)>,
) -> Result<VerifiedModule, ResolverError> {
    check_materialized_tree(policy, name, module_root)?;
    check_quoted_imports(name, module_root)?;
    let checksum = crate::hash::hash_directory(module_root)?;
    let signer = read_and_verify_signature(policy, name, module_root, &checksum)?;
    Ok(VerifiedModule { checksum, signer })
}

/// Validates a local path module's structure and resource limits
/// without recording a checksum or verifying a signature.
///
/// Local path sources are read as-is and are not subject to checksum or
/// signature verification (see the module specification's lockfile and
/// signing sections), but they must still be structurally valid modules:
/// no symbolic links, no reserved filenames outside the root, and within
/// the configured resource limits. Hashing runs only to exercise those
/// structural checks; the digest is discarded.
pub(crate) fn verify_structure(
    policy: &ResolverPolicy,
    name: &DependencyName,
    module_root: &Path,
) -> Result<(), ResolverError> {
    check_materialized_tree(policy, name, module_root)?;
    check_quoted_imports(name, module_root)?;
    crate::hash::hash_directory(module_root)?;
    Ok(())
}

/// Validates that every quoted `import` in the module's `.wdl` files
/// resolves to a location inside the module root.
///
/// A quoted import such as `import "../shared.wdl"` that escapes the
/// module root makes the module invalid, even if the target exists.
/// Imports that name an absolute URI (with a scheme) are not
/// file-relative and are not subject to this check.
///
/// Each file is parsed with the WDL grammar and its actual import
/// statements are inspected, so `import` appearing in a command block or
/// after a definition cannot bypass the check. Files that fail to parse
/// are skipped here; analysis reports their syntax errors separately.
fn check_quoted_imports(name: &DependencyName, module_root: &Path) -> Result<(), ResolverError> {
    // Symbolic links are already forbidden by the tree walk, so a
    // lexical comparison of cleaned paths is sufficient; the walk yields
    // paths under `module_root`, so the root is used as-is.
    let root = path_clean::clean(module_root);

    walk_module_tree(module_root, &mut |path: &Path, _size| {
        if path.extension().and_then(|e| e.to_str()) != Some("wdl") {
            return Ok(());
        }
        let contents = std::fs::read_to_string(path).map_err(|source| ResolverError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let file_dir = path.parent().unwrap_or(&root);
        for import in quoted_imports(&contents) {
            // Absolute URIs (with a scheme) are not file-relative.
            if url::Url::parse(&import).is_ok() {
                continue;
            }
            let resolved = path_clean::clean(file_dir.join(&import));
            if !resolved.starts_with(&root) {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                return Err(ResolverError::QuotedImportEscapesRoot {
                    dep: name.manifest().to_string(),
                    file: rel,
                    import,
                });
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Extracts the target of each quoted (URI) `import` statement from WDL
/// source by parsing it and walking the real import nodes.
///
/// Symbolic module-path imports are not quoted and are resolved through
/// the module system, so they are ignored here. A file that does not
/// parse yields no imports (analysis surfaces the syntax error).
fn quoted_imports(source: &str) -> Vec<String> {
    use wdl_ast::Ast;
    use wdl_ast::Document;
    use wdl_ast::v1::ImportSource;

    let (document, _) = Document::parse(source, None);
    let Ast::V1(ast) = document.ast() else {
        return Vec::new();
    };

    ast.imports()
        .filter_map(|import| match import.source() {
            ImportSource::Uri(uri) => uri.text().map(|t| t.text().to_string()),
            ImportSource::ModulePath(_) => None,
        })
        .collect()
}

/// Reads the signature file from `module_root` and verifies it.
fn read_and_verify_signature(
    policy: &ResolverPolicy,
    name: &DependencyName,
    module_root: &Path,
    checksum: &ContentHash,
) -> Result<Option<VerifiedSigner>, ResolverError> {
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
            signer: Box::new(sig.public_key()),
        })?;
    Ok(Some(VerifiedSigner {
        key: sig.public_key(),
        identity: sig.identity().cloned(),
    }))
}

/// Checks a dependency's content hash and signer against the lockfile.
///
/// Called only by the `materialize` path, where a lockfile already
/// exists and the materialized content must match the locked
/// expectations.
pub(crate) fn verify_against_lockfile(
    lockfile: &Lockfile,
    trust: &TrustStore,
    scope: &[DependencyName],
    name: &DependencyName,
    checksum: &ContentHash,
    signer: Option<&VerifyingKey>,
    signer_identity: Option<&SignerIdentity>,
) -> Result<(), ResolverError> {
    let locked_entry =
        lockfile
            .find_scoped(scope, name)
            .ok_or_else(|| ResolverError::NotInLockfile {
                dep: name.manifest().to_string(),
            })?;
    // A Git-sourced entry always records a checksum; a local path entry
    // records none and is verified only by re-reading its content.
    if let Some(expected) = locked_entry.checksum
        && expected != *checksum
    {
        return Err(ResolverError::ChecksumMismatch {
            dep: name.manifest().to_string(),
            expected,
            observed: *checksum,
        });
    }
    match (locked_entry.signer, signer) {
        (None, Some(observed)) => {
            return Err(ResolverError::UnexpectedSigner {
                dep: name.manifest().to_string(),
                observed: Box::new(*observed),
                identity: signer_identity.cloned(),
            });
        }
        (Some(expected), None) => {
            return Err(ResolverError::SignatureDowngrade {
                dep: name.manifest().to_string(),
                expected_signer: Box::new(expected),
            });
        }
        (Some(expected), Some(observed)) if expected != *observed => {
            return Err(ResolverError::SignerKeyMismatch {
                dep: name.manifest().to_string(),
                source_url: Some(locked_entry.source.source_url()),
                path: locked_entry.source.source_path().map(ToString::to_string),
                expected: Box::new(expected),
                observed: Box::new(*observed),
            });
        }
        _ => {}
    }
    if let Some(signer) = locked_entry.signer
        && !trust.contains_key(&signer)
    {
        return Err(ResolverError::UntrustedSigner {
            dep: name.manifest().to_string(),
            signer: Box::new(signer),
            identity: signer_identity.cloned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::dependency::GitSelector;
    use crate::lockfile::DependencyEntry;
    use crate::lockfile::GitCommit;
    use crate::lockfile::ResolvedSource;
    use crate::resolver::config::ModulesConfig;
    use crate::signing::test_utils::signing_key_from_seed;

    fn test_dep() -> DependencyName {
        "foo".parse().unwrap()
    }

    fn test_source() -> ResolvedSource {
        ResolvedSource::Git {
            git: "https://github.com/example/foo".parse().unwrap(),
            sha: GitCommit::try_from("a".repeat(40)).unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: None,
        }
    }

    fn trust_with(key: VerifyingKey) -> TrustStore {
        let mut trust = TrustStore::default();
        trust.insert_key(key);
        trust
    }

    fn write_module(dir: &std::path::Path, content: &str) {
        fs::write(dir.join("index.wdl"), content).unwrap();
    }

    fn write_signed_module(dir: &std::path::Path, content: &str, seed: u64) {
        write_module(dir, content);
        let checksum = crate::hash::hash_directory(dir).unwrap();
        let signing_key = signing_key_from_seed(seed);
        // SAFETY: `None` contains no invalid signer identity fields.
        let sig = crate::signing::ModuleSignature::new(&signing_key, &checksum, None).unwrap();
        let mut buf = Vec::new();
        sig.write(&mut buf).unwrap();
        fs::write(dir.join(crate::SIGNATURE_FILENAME), buf).unwrap();
    }

    #[test]
    fn verify_unsigned_module() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.3\n");
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let result = verify(&policy, &trust, &test_dep(), dir.path(), None);
        assert!(result.is_ok(), "unsigned module should verify: {result:?}");
        assert!(result.unwrap().signer.is_none());
    }

    #[test]
    fn quoted_imports_uses_real_import_nodes() {
        // Parsing (not line scanning) means `import` inside a command
        // block is ignored, and an import *after* a definition is still
        // found — both cases the old line scanner mishandled.
        let src = "version 1.3\nimport \"sort.wdl\"\nimport \"https://example.com/lib.wdl\" as \
                   lib\ntask t {\ncommand <<< import \"not-an-import.wdl\" >>>\n}\nimport \
                   \"grep.wdl\"\n";
        assert_eq!(
            quoted_imports(src),
            vec![
                "sort.wdl".to_string(),
                "https://example.com/lib.wdl".to_string(),
                "grep.wdl".to_string(),
            ]
        );
    }

    #[test]
    fn verify_rejects_escaping_import_after_a_definition() {
        // An escaping import placed after a task definition (which the
        // old line scanner stopped at) is still rejected.
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("index.wdl"),
            "version 1.3\ntask t {\n    command <<<>>>\n}\nimport \"../shared.wdl\"\n",
        )
        .unwrap();
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let err = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap_err();
        assert!(
            matches!(err, ResolverError::QuotedImportEscapesRoot { .. }),
            "expected `QuotedImportEscapesRoot`, got: {err}"
        );
    }

    #[test]
    fn verify_rejects_quoted_import_escaping_module_root() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("index.wdl"),
            "version 1.3\nimport \"../shared.wdl\"\n",
        )
        .unwrap();
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let err = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap_err();
        assert!(
            matches!(err, ResolverError::QuotedImportEscapesRoot { .. }),
            "expected `QuotedImportEscapesRoot`, got: {err}"
        );
    }

    #[test]
    fn verify_allows_in_root_and_absolute_uri_imports() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/helper.wdl"), "version 1.3\n").unwrap();
        fs::write(
            dir.path().join("index.wdl"),
            "version 1.3\nimport \"sub/helper.wdl\"\nimport \"https://example.com/remote.wdl\"\n",
        )
        .unwrap();
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        assert!(verify(&policy, &trust, &test_dep(), dir.path(), None).is_ok());
    }

    #[test]
    fn verify_signed_module() {
        let dir = tempdir().unwrap();
        write_signed_module(dir.path(), "version 1.3\n", 0xAB);
        let policy = ResolverPolicy::default();
        let trust = TrustStore::default();
        let result = verify(&policy, &trust, &test_dep(), dir.path(), None);
        assert!(result.is_ok(), "signed module should verify: {result:?}");
        let verified = result.unwrap();
        assert!(verified.signer.is_some());
        assert_eq!(
            verified.signer.unwrap().key,
            signing_key_from_seed(0xAB).verifying_key()
        );
    }

    #[test]
    fn require_signed_rejects_unsigned() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.3\n");
        let config = ModulesConfig {
            require_signed: true,
            ..Default::default()
        };
        let policy = ResolverPolicy::try_from(&config).unwrap();
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
        write_module(dir.path(), "version 1.3\n");
        let checksum = crate::hash::hash_directory(dir.path()).unwrap();
        let wrong_checksum = ContentHash::from([0xFFu8; 32]);
        assert_ne!(checksum, wrong_checksum);

        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                checksum: Some(wrong_checksum),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            None,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ResolverError::ChecksumMismatch { .. }),
            "expected `ChecksumMismatch`, got: {err}"
        );
    }

    #[test]
    fn lockfile_checksum_match() {
        let dir = tempdir().unwrap();
        write_module(dir.path(), "version 1.3\n");
        let checksum = crate::hash::hash_directory(dir.path()).unwrap();

        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                checksum: Some(checksum),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let result = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            None,
            None,
        );
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
                checksum: Some(checksum),
                signer: Some(key),
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            None,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignatureDowngrade { .. }),
            "expected `SignatureDowngrade`, got: {err}"
        );
    }

    #[test]
    fn unexpected_signer_detected() {
        let key = signing_key_from_seed(0xAB).verifying_key();
        let checksum = ContentHash::from([0x01u8; 32]);
        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                checksum: Some(checksum),
                signer: None,
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let result = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            Some(&key),
            None,
        );

        assert!(
            matches!(result, Err(ResolverError::UnexpectedSigner { .. })),
            "expected `UnexpectedSigner`, got: {result:?}"
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
                checksum: Some(checksum),
                signer: Some(key_a),
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };
        let err = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            Some(&key_b),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "expected `SignerKeyMismatch`, got: {err}"
        );
    }

    #[test]
    fn locked_signer_must_be_trusted() {
        let key = signing_key_from_seed(0xAB).verifying_key();
        let checksum = ContentHash::from([0x01u8; 32]);
        let dep = test_dep();
        let mut deps = BTreeMap::new();
        deps.insert(
            dep.clone(),
            DependencyEntry {
                source: test_source(),
                checksum: Some(checksum),
                signer: Some(key),
                dependencies: BTreeMap::new(),
            },
        );
        let lockfile = Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies: deps,
        };

        let err = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            Some(&key),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ResolverError::UntrustedSigner { .. }),
            "expected `UntrustedSigner`, got: {err}"
        );

        verify_against_lockfile(
            &lockfile,
            &trust_with(key),
            &[],
            &dep,
            &checksum,
            Some(&key),
            None,
        )
        .expect("trusted locked signer should verify");
    }

    #[test]
    fn file_count_limit_exceeded() {
        let dir = tempdir().unwrap();
        for i in 0..5 {
            fs::write(dir.path().join(format!("file_{i}.wdl")), "version 1.3\n").unwrap();
        }
        let config = ModulesConfig {
            max_materialized_files: Some(2),
            ..Default::default()
        };
        let policy = ResolverPolicy::try_from(&config).unwrap();
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
        let policy = ResolverPolicy::try_from(&config).unwrap();
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
        let err = verify_against_lockfile(
            &lockfile,
            &TrustStore::default(),
            &[],
            &dep,
            &checksum,
            None,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ResolverError::NotInLockfile { .. }),
            "expected `NotInLockfile`, got: {err}"
        );
    }

    #[test]
    fn trust_store_does_not_pin_sources() {
        let dir = tempdir().unwrap();
        write_signed_module(dir.path(), "version 1.3\n", 0xAB);

        let trust = TrustStore::default();
        let policy = ResolverPolicy::default();
        let verified = verify(&policy, &trust, &test_dep(), dir.path(), None).unwrap();
        assert!(verified.signer.is_some());
    }
}
