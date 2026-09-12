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
    name: &DependencyName,
    module_root: &Path,
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
/// Absolute non-file URIs are not file-relative and are not subject to this
/// check. File URLs are decoded and checked like relative file imports.
///
/// Every included `.wdl` file is parsed with the WDL grammar. Local imports are
/// followed recursively regardless of file extension because WDL import URIs
/// do not require one. Actual import statements are inspected, so `import`
/// appearing in a command block or after a definition cannot bypass the check.
/// Files that fail to parse yield no imports here; analysis reports their
/// syntax errors separately.
fn check_quoted_imports(name: &DependencyName, module_root: &Path) -> Result<(), ResolverError> {
    use std::collections::HashSet;
    use std::collections::VecDeque;

    // Symbolic links are already forbidden by the tree walk, so a
    // lexical comparison of cleaned paths is sufficient; the walk yields
    // paths under `module_root`.
    let root = if module_root.is_absolute() {
        path_clean::clean(module_root)
    } else {
        path_clean::clean(
            std::env::current_dir()
                .map_err(|source| ResolverError::Io {
                    path: module_root.to_path_buf(),
                    source,
                })?
                .join(module_root),
        )
    };
    let (manifest, exclusions) = exclusions_from_root(module_root)?;
    let canonical_root = std::fs::canonicalize(&root).map_err(|source| ResolverError::Io {
        path: root.clone(),
        source,
    })?;
    let mut queue = VecDeque::new();
    module_walk::walk_module_content_tree(module_root, &exclusions, &mut |path: &Path, _size| {
        if path.extension().and_then(|extension| extension.to_str()) == Some("wdl") {
            let relative = path.strip_prefix(module_root).unwrap_or(path);
            queue.push_back(path_clean::clean(root.join(relative)));
        }
        Ok::<_, ResolverError>(())
    })
    .map_err(|error| match error {
        module_walk::WalkError::Walk(error) => ResolverError::Walk(error),
        module_walk::WalkError::Visitor(error) => error,
    })?;
    if let Some(manifest) = manifest {
        let entrypoint = manifest.entrypoint_filename();
        if !crate::hash::path_is_excluded_from_hash(entrypoint)
            && !exclusions.is_excluded(entrypoint)
        {
            let path = path_clean::clean(root.join(entrypoint));
            if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
                let canonical =
                    std::fs::canonicalize(&path).map_err(|source| ResolverError::Io {
                        path: path.clone(),
                        source,
                    })?;
                if canonical.starts_with(&canonical_root) {
                    let actual = canonical.strip_prefix(&canonical_root).unwrap();
                    if !crate::hash::path_is_excluded_from_hash(actual)
                        && !exclusions.is_excluded(actual)
                    {
                        queue.push_back(path);
                    }
                }
            }
        }
    }

    // Start with every included `.wdl` file to preserve whole-module
    // validation, then follow local imports regardless of file extension.
    // This catches import chains through extensionless documents without
    // loading unrelated binary module assets into memory.
    let mut visited = HashSet::new();
    while let Some(path) = queue.pop_front() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let contents = std::fs::read_to_string(&path).map_err(|source| ResolverError::Io {
            path: path.clone(),
            source,
        })?;
        let base = url::Url::from_file_path(&path).map_err(|()| ResolverError::Io {
            path: path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "could not convert module path to a file URL",
            ),
        })?;
        for import in quoted_imports(&contents) {
            let Ok(resolved_url) = base.join(&import) else {
                // Analysis reports malformed import URIs separately.
                continue;
            };
            // Absolute non-file URIs are not paths within module content.
            if resolved_url.scheme() != "file" {
                continue;
            }
            let resolved = match resolved_url.to_file_path() {
                Ok(path) => path_clean::clean(path),
                Err(()) => {
                    return Err(ResolverError::QuotedImportEscapesRoot {
                        dep: name.manifest().to_string(),
                        file: module_relative_path(&root, &path),
                        import,
                    });
                }
            };
            if !resolved.starts_with(&root) {
                return Err(ResolverError::QuotedImportEscapesRoot {
                    dep: name.manifest().to_string(),
                    file: module_relative_path(&root, &path),
                    import,
                });
            }
            // SAFETY: containment was checked immediately above.
            let target = resolved.strip_prefix(&root).unwrap();
            if crate::hash::path_is_excluded_from_hash(target) || exclusions.is_excluded(target) {
                return Err(ResolverError::QuotedImportExcluded {
                    dep: name.manifest().to_string(),
                    file: module_relative_path(&root, &path),
                    import,
                });
            }
            match std::fs::symlink_metadata(&resolved) {
                Ok(metadata) if metadata.is_file() => {
                    let canonical =
                        std::fs::canonicalize(&resolved).map_err(|source| ResolverError::Io {
                            path: resolved.clone(),
                            source,
                        })?;
                    if !canonical.starts_with(&canonical_root) {
                        return Err(ResolverError::QuotedImportEscapesRoot {
                            dep: name.manifest().to_string(),
                            file: module_relative_path(&root, &path),
                            import,
                        });
                    }
                    // Match again using the target's actual on-disk spelling.
                    // Case-insensitive filesystems may resolve a differently
                    // cased URI to an excluded file.
                    let canonical_target = canonical.strip_prefix(&canonical_root).unwrap();
                    if crate::hash::path_is_excluded_from_hash(canonical_target)
                        || exclusions.is_excluded(canonical_target)
                    {
                        return Err(ResolverError::QuotedImportExcluded {
                            dep: name.manifest().to_string(),
                            file: module_relative_path(&root, &path),
                            import,
                        });
                    }
                    // Keep the lexical root spelling for subsequent URL joins;
                    // `canonical_root` may use a platform alias such as
                    // `/private/var` for a lexical `/var` root.
                    queue.push_back(resolved);
                }
                Ok(_) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(ResolverError::Io {
                        path: resolved,
                        source,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Returns a module-root-relative path using portable separators.
fn module_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Loads the root manifest's compiled content exclusions, when present.
fn exclusions_from_root(
    root: &Path,
) -> Result<(Option<crate::Manifest>, module_walk::ExclusionSet), ResolverError> {
    let path = root.join(crate::MANIFEST_FILENAME);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            let exclusions = module_walk::ExclusionSet::new(&[]).map_err(|error| {
                ResolverError::InvalidExclude {
                    pattern: error.pattern,
                    source: error.source,
                }
            })?;
            return Ok((None, exclusions));
        }
        Err(source) => return Err(ResolverError::Io { path, source }),
    };
    if metadata.file_type().is_symlink() {
        return Err(ResolverError::Walk(module_walk::ModuleWalkError::Symlink(
            path.display().to_string(),
        )));
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(source) => return Err(ResolverError::Io { path, source }),
    };
    let manifest = crate::Manifest::parse(&bytes)?;
    let exclusions = module_walk::ExclusionSet::new(&manifest.exclude).map_err(|error| {
        ResolverError::InvalidExclude {
            pattern: error.pattern,
            source: error.source,
        }
    })?;
    Ok((Some(manifest), exclusions))
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
                path: locked_entry.source_path().map(ToString::to_string),
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

    fn write_manifest(dir: &std::path::Path, exclusions: &[&str]) {
        let body = serde_json::json!({
            "name": "foo",
            "license": "MIT",
            "readme": false,
            "exclude": exclusions,
        });
        fs::write(
            dir.join(crate::MANIFEST_FILENAME),
            serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
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
        let result = verify(&policy, &test_dep(), dir.path());
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
        let err = verify(&policy, &test_dep(), dir.path()).unwrap_err();
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
        let err = verify(&policy, &test_dep(), dir.path()).unwrap_err();
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
        assert!(verify(&policy, &test_dep(), dir.path()).is_ok());
    }

    #[test]
    fn verify_accepts_relative_module_root() {
        let current = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&current).unwrap();
        let relative = dir.path().strip_prefix(&current).unwrap();
        write_manifest(dir.path(), &[]);
        write_module(dir.path(), "version 1.3\nimport \"helper.wdl\"\n");
        fs::write(dir.path().join("helper.wdl"), "version 1.3\n").unwrap();

        let result = verify(&ResolverPolicy::default(), &test_dep(), relative);
        assert!(
            result.is_ok(),
            "relative module root should verify: {result:?}"
        );
    }

    #[test]
    fn verify_ignores_escaping_import_in_excluded_file() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["testrun.wdl"]);
        write_module(dir.path(), "version 1.3\n");
        fs::write(
            dir.path().join("testrun.wdl"),
            "version 1.3\nimport \"../shared.wdl\"\n",
        )
        .unwrap();

        let result = verify(&ResolverPolicy::default(), &test_dep(), dir.path());
        assert!(
            result.is_ok(),
            "excluded harness should be ignored: {result:?}"
        );
    }

    #[test]
    fn verify_rejects_included_import_targeting_excluded_content() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["private/**"]);
        write_module(dir.path(), "version 1.3\nimport \"private/helper.wdl\"\n");
        fs::create_dir(dir.path().join("private")).unwrap();
        fs::write(dir.path().join("private/helper.wdl"), "version 1.3\n").unwrap();

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_checks_imports_in_wdl_documents_without_wdl_extension() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["private.wdl"]);
        write_module(dir.path(), "version 1.3\nimport \"helper.txt\"\n");
        fs::write(
            dir.path().join("helper.txt"),
            "version 1.3\nimport \"private.wdl\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("private.wdl"), "version 1.3\n").unwrap();

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_checks_custom_entrypoint_without_wdl_extension() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(crate::MANIFEST_FILENAME),
            br#"{
                "name":"foo",
                "license":"MIT",
                "readme":false,
                "entrypoint":"main.txt",
                "exclude":["private.wdl"]
            }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("main.txt"),
            "version 1.3\nimport \"private.wdl\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("private.wdl"), "version 1.3\n").unwrap();

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_rejects_import_of_unhashed_metadata() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &[]);
        write_module(dir.path(), "version 1.3\nimport \"module-lock.json\"\n");
        fs::write(dir.path().join(crate::LOCKFILE_FILENAME), "version 1.3\n").unwrap();

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_decodes_file_uri_before_checking_exclusions() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["private files/**"]);
        fs::create_dir(dir.path().join("private files")).unwrap();
        let target = dir.path().join("private files/helper.wdl");
        fs::write(&target, "version 1.3\n").unwrap();
        let target = url::Url::from_file_path(&target).unwrap();
        write_module(dir.path(), &format!("version 1.3\nimport \"{target}\"\n"));

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_decodes_relative_uri_before_checking_exclusions() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["private.wdl"]);
        write_module(
            dir.path(),
            "version 1.3\nimport \"private%2Ewdl#fragment\"\n",
        );
        fs::write(dir.path().join("private.wdl"), "version 1.3\n").unwrap();

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn verify_matches_actual_path_case_on_case_insensitive_filesystems() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["private.wdl"]);
        fs::write(dir.path().join("private.wdl"), "version 1.3\n").unwrap();
        if !dir.path().join("PRIVATE.wdl").exists() {
            return;
        }
        write_module(dir.path(), "version 1.3\nimport \"PRIVATE.wdl\"\n");

        let error = verify(&ResolverPolicy::default(), &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(error, ResolverError::QuotedImportExcluded { .. }));
    }

    #[test]
    fn physical_limits_count_excluded_files() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), &["testrun.wdl"]);
        write_module(dir.path(), "version 1.3\n");
        fs::write(dir.path().join("testrun.wdl"), "version 1.3\n").unwrap();
        let policy = ResolverPolicy::try_from(&ModulesConfig {
            max_materialized_files: Some(2),
            ..ModulesConfig::default()
        })
        .unwrap();

        let error = verify(&policy, &test_dep(), dir.path()).unwrap_err();
        assert!(matches!(
            error,
            ResolverError::MaterializedTreeLimitExceeded { files: 3, .. }
        ));
    }

    #[test]
    fn verify_signed_module() {
        let dir = tempdir().unwrap();
        write_signed_module(dir.path(), "version 1.3\n", 0xAB);
        let policy = ResolverPolicy::default();
        let result = verify(&policy, &test_dep(), dir.path());
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
        let err = verify(&policy, &test_dep(), dir.path()).unwrap_err();
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
        let err = verify(&policy, &test_dep(), dir.path()).unwrap_err();
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
        let err = verify(&policy, &test_dep(), dir.path()).unwrap_err();
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
}
