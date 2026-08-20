//! Value types returned by the [`Resolver`](super::Resolver) trait.

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use crate::dependency::DependencyName;
use crate::hash::ContentHash;
use crate::lockfile::ResolvedSource;
use crate::signing::SignerIdentity;
use crate::signing::VerifyingKey;

/// A symbolic import resolved to a concrete file on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFile {
    /// Absolute path to the resolved file.
    pub path: PathBuf,
    /// Absolute path to the root directory of the module that owns the file.
    pub module_root: PathBuf,
    /// The source the file's owning module came from.
    pub source: ResolvedSource,
    /// The parsed manifest of the dependency that owns this file.
    pub manifest: std::sync::Arc<crate::Manifest>,
}

impl MaterializedFile {
    /// Builds the [`Module`](crate::module::Module) that owns this file.
    ///
    /// The owning module is a child of `consumer` reached through `dep_name`,
    /// so its transitive imports resolve their own relative paths and
    /// lockfile entries correctly. Callers consume this instead of
    /// assembling a module from the file's manifest and root themselves.
    pub fn child_module(
        &self,
        consumer: &crate::module::Module,
        dep_name: DependencyName,
    ) -> crate::module::Module {
        consumer.child(dep_name, self.manifest.clone(), self.module_root.clone())
    }
}

/// A fully resolved dependency tree, suitable for `module-lock.json`
/// generation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedTree {
    /// The top-level resolved dependencies, keyed by consumer-chosen
    /// dependency name.
    pub dependencies: BTreeMap<DependencyName, ResolvedDependency>,
}

/// One resolved dependency in a [`ResolvedTree`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDependency {
    /// The resolved source.
    pub source: ResolvedSource,
    /// The resolved module version, when selected from a version tag.
    pub version: Option<Version>,
    /// The module's content hash. `None` for local path sources, which
    /// carry no checksum and are read as-is.
    pub checksum: Option<ContentHash>,
    /// The signer's public key, if the module was signed. `None` for
    /// local path sources, which are not subject to signature verification.
    pub signer: Option<VerifyingKey>,
    /// Optional signer identity metadata captured from `module.sig`.
    pub signer_identity: Option<SignerIdentity>,
    /// The module's transitive resolved dependencies.
    pub dependencies: BTreeMap<DependencyName, ResolvedDependency>,
}

/// One resolved module inside a [`ResolvedDependency`].
///
/// This type is produced during resolution before being folded into
/// [`ResolvedDependency`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModule {
    /// The resolved module version, when selected from a version tag.
    pub version: Option<Version>,
    /// The module's content hash. `None` for local path sources, which
    /// carry no checksum and are read as-is.
    pub checksum: Option<ContentHash>,
    /// The signer's public key, if the module was signed. `None` for
    /// local path sources, which are not subject to signature verification.
    pub signer: Option<VerifyingKey>,
    /// Optional signer identity metadata captured from `module.sig`.
    pub signer_identity: Option<SignerIdentity>,
    /// The module's transitive resolved dependencies.
    pub dependencies: BTreeMap<DependencyName, ResolvedDependency>,
}
