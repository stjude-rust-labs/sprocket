//! Module verification. Hashes, signatures, trust, limits, and
//! lockfile checksum enforcement.

use std::path::Path;

use crate::ContentHash;
use crate::DependencyName;
use crate::Lockfile;
use crate::ModulePath;
use crate::VerifyingKey;
use crate::resolver::config::LargeFileWarning;
use crate::resolver::config::ModulesConfig;
use crate::resolver::error::ResolverError;
use crate::resolver::trust::TrustStore;

/// Artifacts produced by [`ModuleVerifier::verify`].
pub(crate) struct VerifiedModule {
    /// The module's content hash.
    pub checksum: ContentHash,
    /// The signer's public key, if the module was signed.
    pub signer: Option<VerifyingKey>,
}

/// Verifies materialized module content against trust, signature,
/// checksum, and resource-limit policies.
#[derive(bon::Builder)]
pub(crate) struct ModuleVerifier<'a> {
    /// The resolved modules configuration.
    config: &'a ModulesConfig,
    /// The trust store used for key lookups.
    trust: &'a TrustStore,
    /// The lockfile used for checksum verification.
    lockfile: &'a Lockfile,
}

impl ModuleVerifier<'_> {
    /// Runs all verification checks on a materialized module root.
    pub fn verify(
        &self,
        name: &DependencyName,
        module_root: &Path,
    ) -> Result<VerifiedModule, ResolverError> {
        check_materialized_tree_limits(self.config, name, module_root)?;
        let checksum = crate::hash::hash_directory(module_root)?;
        self.warn_on_large_files(name, module_root)?;
        let signer = self.read_and_verify_signature(name, module_root, &checksum)?;
        Ok(VerifiedModule { checksum, signer })
    }

    /// Verifies a dependency's content hash against the lockfile.
    pub fn verify_against_lockfile(
        &self,
        name: &DependencyName,
        checksum: &ContentHash,
    ) -> Result<(), ResolverError> {
        let locked_entry = self
            .lockfile
            .dependencies
            .get(name)
            .ok_or_else(|| ResolverError::NotInLockfile { dep: name.clone() })?;
        let locked_module = locked_entry
            .modules
            .get(&ModulePath::Root)
            .ok_or_else(|| ResolverError::NotInLockfile { dep: name.clone() })?;
        if locked_module.checksum != *checksum {
            return Err(ResolverError::ChecksumMismatch {
                dep: name.clone(),
                expected: locked_module.checksum,
                observed: *checksum,
            });
        }
        Ok(())
    }

    /// Emits a tracing warning for any file exceeding the configured size threshold.
    fn warn_on_large_files(
        &self,
        name: &DependencyName,
        module_root: &Path,
    ) -> Result<(), ResolverError> {
        let LargeFileWarning::Threshold(threshold) = self.config.large_file_warning else {
            return Ok(());
        };
        crate::resolver::tree_walk::walk_module_tree(module_root, &mut |entry, size| {
            if size >= threshold {
                tracing::warn!(
                    dep = %name,
                    file = %entry.display(),
                    size,
                    threshold,
                    "module contains a large file",
                );
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Reads the signature file from `module_root` and verifies it against `checksum`.
    fn read_and_verify_signature(
        &self,
        name: &DependencyName,
        module_root: &Path,
        checksum: &ContentHash,
    ) -> Result<Option<VerifyingKey>, ResolverError> {
        let sig_path = module_root.join(crate::SIGNATURE_FILENAME);
        let bytes = match std::fs::read(&sig_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if self.config.require_signed {
                    return Err(ResolverError::RequireSignedViolation { dep: name.clone() });
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
        let sig = crate::ModuleSignature::parse(&bytes).map_err(|source| {
            ResolverError::SignatureParse {
                dep: name.clone(),
                source,
            }
        })?;
        sig.verify(checksum)
            .map_err(|_| ResolverError::SignatureVerificationFailed {
                dep: name.clone(),
                signer: Box::new(sig.public_key),
            })?;
        if let Some(trusted) = self.trust.lookup(name)
            && &sig.public_key != trusted
        {
            return Err(ResolverError::SignerKeyMismatch {
                dep: name.clone(),
                expected: Box::new(*trusted),
                observed: Box::new(sig.public_key),
            });
        }
        Ok(Some(sig.public_key))
    }
}

/// Walks `module_root` and rejects the tree if it exceeds configured
/// file-count or byte-size limits.
fn check_materialized_tree_limits(
    config: &ModulesConfig,
    name: &DependencyName,
    module_root: &Path,
) -> Result<(), ResolverError> {
    if config.max_materialized_files.is_none() && config.max_materialized_bytes.is_none() {
        return Ok(());
    }
    let stats = crate::resolver::tree_walk::walk_module_tree(module_root, &mut |_, _| Ok(()))?;
    if config
        .max_materialized_files
        .is_some_and(|limit| stats.files > limit)
        || config
            .max_materialized_bytes
            .is_some_and(|limit| stats.bytes > limit)
    {
        return Err(ResolverError::MaterializedTreeLimitExceeded {
            dep: name.clone(),
            files: stats.files,
            bytes: stats.bytes,
        });
    }
    Ok(())
}
