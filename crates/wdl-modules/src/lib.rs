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
#[cfg(feature = "resolver")]
pub mod resolver;
pub mod signing;
mod strict_json;
pub mod symbolic_path;
pub mod tree;
pub mod version_requirement;

pub use crate::lockfile::Lockfile;
pub use crate::manifest::Manifest;
#[cfg(feature = "resolver")]
pub use crate::resolver::Resolver;

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
