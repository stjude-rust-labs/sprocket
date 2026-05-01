//! Implementation of the WDL module specification. Covers manifest and lockfile
//! parsing, symbolic-path parsing, deterministic content hashing, Ed25519
//! signing and verification, and SPDX license validation.
//!
//! The crate handles every part of the module specification that does not
//! require networking or process spawning.

pub mod dependency_name;
pub mod dependency_source;
pub mod hash;
pub mod license;
pub mod lockfile;
pub mod manifest;
pub mod relative_path;
pub mod signing;
mod strict_json;
pub mod symbolic_path;
pub mod tree;
pub mod version_requirement;

pub use crate::dependency_name::DependencyName;
pub use crate::dependency_name::DependencyNameError;
pub use crate::dependency_source::DependencySource;
pub use crate::dependency_source::DependencySourceError;
pub use crate::dependency_source::GitSelector;
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
pub use crate::lockfile::LockedModule;
pub use crate::lockfile::Lockfile;
pub use crate::lockfile::LockfileError;
pub use crate::lockfile::ModulePath;
pub use crate::lockfile::ResolvedSource;
pub use crate::manifest::Manifest;
pub use crate::manifest::ManifestError;
pub use crate::manifest::Readme;
pub use crate::manifest::Tool;
pub use crate::relative_path::RelativePath;
pub use crate::relative_path::RelativePathError;
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
