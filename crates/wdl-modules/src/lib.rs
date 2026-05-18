//! Implementation of the WDL module specification.
//!
//! `wdl-modules` provides the local, deterministic pieces of WDL module
//! handling: `module.json` manifest parsing, `module-lock.json` lockfile
//! parsing, symbolic import paths, deterministic content hashing, Ed25519
//! `module.sig` signing and verification, SPDX license validation, and
//! module file-tree checks.
//!
//! # Quickstart
//!
//! Parse `module.json` with [`Manifest::parse`], parse `module-lock.json` with
//! [`Lockfile::parse`], and compute a content hash with
//! [`hash::hash_directory`]. These entry points reject duplicate JSON object
//! keys, invalid relative paths, invalid dependency declarations, and module
//! trees that violate the reserved-filename or Unicode-normalization rules.
//!
//! ```rust
//! use wdl_modules::Manifest;
//!
//! let manifest = Manifest::parse(
//!     br#"{
//!         "name": "spellbook",
//!         "version": "1.0.0",
//!         "license": "MIT"
//!     }"#,
//! )?;
//!
//! assert_eq!(manifest.name, "spellbook");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod dependency;
pub mod hash;
pub mod license;
pub mod lockfile;
pub mod manifest;
pub mod module_walk;
pub mod relative_path;
pub mod resolver;
pub mod signing;
mod strict_json;
pub mod symbolic_path;
pub mod tree;
pub mod version_requirement;

pub use crate::dependency::DependencyName;
pub use crate::dependency::DependencyNameError;
pub use crate::dependency::DependencySource;
pub use crate::dependency::DependencySourceError;
pub use crate::dependency::GitModulePath;
pub use crate::dependency::GitModulePathError;
pub use crate::dependency::GitSelector;
pub use crate::hash::ContentHash;
pub use crate::hash::HashError;
pub use crate::hash::Hasher;
pub use crate::license::LicenseError;
pub use crate::license::LicenseExpression;
pub use crate::lockfile::DependencyEntry;
pub use crate::lockfile::DependencyMap;
pub use crate::lockfile::GitCommit;
pub use crate::lockfile::GitCommitError;
pub use crate::lockfile::LOCKFILE_VERSION;
pub use crate::lockfile::Lockfile;
pub use crate::lockfile::LockfileError;
pub use crate::lockfile::ResolvedSource;
pub use crate::manifest::Manifest;
pub use crate::manifest::ManifestError;
pub use crate::manifest::Readme;
pub use crate::manifest::Tool;
pub use crate::relative_path::RelativePath;
pub use crate::relative_path::RelativePathError;
#[cfg(feature = "resolver")]
pub use crate::resolver::DependencyChange;
pub use crate::resolver::DependencyScope;
#[cfg(feature = "resolver")]
pub use crate::resolver::DependencyUpdate;
pub use crate::resolver::GitRefKind;
#[cfg(feature = "resolver")]
pub use crate::resolver::GitResolver;
#[cfg(feature = "resolver")]
pub use crate::resolver::LargeFileWarning;
#[cfg(feature = "resolver")]
pub use crate::resolver::LargeFileWarningError;
#[cfg(feature = "resolver")]
pub use crate::resolver::LockfileDiff;
pub use crate::resolver::MaterializedFile;
pub use crate::resolver::MissingFileKind;
#[cfg(feature = "resolver")]
pub use crate::resolver::ModulesConfig;
#[cfg(feature = "resolver")]
pub use crate::resolver::NewSigner;
pub use crate::resolver::NullResolver;
#[cfg(feature = "resolver")]
pub use crate::resolver::RelockOutcome;
#[cfg(feature = "resolver")]
pub use crate::resolver::RelockStats;
pub use crate::resolver::ResolvedDependency;
pub use crate::resolver::ResolvedModule;
pub use crate::resolver::ResolvedTree;
pub use crate::resolver::Resolver;
pub use crate::resolver::ResolverError;
#[cfg(feature = "resolver")]
pub use crate::resolver::ResolverPolicy;
#[cfg(feature = "resolver")]
pub use crate::resolver::TrustEntry;
#[cfg(feature = "resolver")]
pub use crate::resolver::TrustMode;
#[cfg(feature = "resolver")]
pub use crate::resolver::TrustStore;
#[cfg(feature = "resolver")]
pub use crate::resolver::TrustStoreError;
#[cfg(feature = "resolver")]
pub use crate::resolver::partial_relock;
pub use crate::signing::KeyError;
pub use crate::signing::ModuleSignature;
pub use crate::signing::Signature;
pub use crate::signing::SignatureError;
pub use crate::signing::SignatureFileError;
pub use crate::signing::SigningKey;
pub use crate::signing::VerifyError;
pub use crate::signing::VerifyingKey;
pub use crate::symbolic_path::SymbolicPath;
pub use crate::symbolic_path::SymbolicPathError;
pub use crate::tree::TreeError;
pub use crate::tree::validate_tree;
pub use crate::version_requirement::VersionRequirement;
pub use crate::version_requirement::VersionRequirementError;

/// The filename of a module manifest.
pub const MANIFEST_FILENAME: &str = "module.json";

/// The filename of a module lockfile.
pub const LOCKFILE_FILENAME: &str = "module-lock.json";

/// The filename of a module signature.
pub const SIGNATURE_FILENAME: &str = "module.sig";

/// The default filename of a module entrypoint, used when
/// `Manifest::entrypoint` is not set.
pub const DEFAULT_ENTRYPOINT_FILENAME: &str = "index.wdl";

/// The default filename of a module readme, used when `Manifest::readme` is
/// `Readme::Default`.
pub const DEFAULT_README_FILENAME: &str = "README.md";

/// Returns `true` if `s` begins with a Windows-style drive letter (e.g.
/// `C:`, `c:\\`, `Z:/`). The spec rejects these as cross-platform unsafe
/// even on non-Windows hosts where `Path::is_absolute` does not flag
/// them.
pub(crate) fn starts_with_windows_drive(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(
        (bytes.next(), bytes.next()),
        (Some(b'A'..=b'Z' | b'a'..=b'z'), Some(b':'))
    )
}
