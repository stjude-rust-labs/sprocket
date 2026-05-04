//! Value types returned by the [`Resolver`](super::Resolver) trait.

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use crate::ContentHash;
use crate::DependencyName;
use crate::ModulePath;
use crate::ResolvedSource;
use crate::VerifyingKey;

/// A symbolic import resolved to a concrete file on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFile {
    /// Absolute path to the resolved file.
    pub path: PathBuf,
    /// The source the file's owning module came from.
    pub source: ResolvedSource,
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
    /// The modules discovered within the source, keyed by their
    /// directory's relative path from the source root, with
    /// [`ModulePath::Root`] for the source root itself.
    pub modules: BTreeMap<ModulePath, ResolvedModule>,
}

/// One resolved module inside a [`ResolvedDependency`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModule {
    /// The version declared in the module's `module.json`.
    pub version: Version,
    /// The module's content hash.
    pub checksum: ContentHash,
    /// The signer's public key, if the module was signed.
    pub signer: Option<VerifyingKey>,
    /// The module's transitive resolved dependencies.
    pub dependencies: BTreeMap<DependencyName, ResolvedDependency>,
}
