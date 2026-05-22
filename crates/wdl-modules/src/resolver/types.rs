//! Value types returned by the [`Resolver`](super::Resolver) trait.

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use crate::ContentHash;
use crate::DependencyName;
use crate::ResolvedSource;
use crate::VerifyingKey;

/// A symbolic import resolved to a concrete file on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFile {
    /// Absolute path to the resolved file.
    pub path: PathBuf,
    /// The source the file's owning module came from.
    pub source: ResolvedSource,
    /// The parsed manifest of the dependency that owns this file.
    pub manifest: std::sync::Arc<crate::Manifest>,
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
    /// The version declared in the module's `module.json`.
    pub version: Version,
    /// The module's content hash.
    pub checksum: ContentHash,
    /// The signer's public key, if the module was signed.
    pub signer: Option<VerifyingKey>,
    /// The module's transitive resolved dependencies.
    pub dependencies: BTreeMap<DependencyName, ResolvedDependency>,
}

/// One resolved module inside a [`ResolvedDependency`].
///
/// This type is produced during resolution before being folded into
/// [`ResolvedDependency`].
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

/// A resolver that rejects every materialization request. Used when
/// the analyzer runs without a module context.
pub struct NullResolver;

#[async_trait::async_trait]
impl super::Resolver for NullResolver {
    async fn materialize(
        &self,
        _: &crate::Module,
        _: &crate::SymbolicPath,
    ) -> Result<MaterializedFile, super::error::ResolverError> {
        Err(super::error::ResolverError::NotADependency {
            name: "symbolic imports require a module context".into(),
        })
    }

    async fn resolve_tree(
        &self,
        _: &crate::Module,
    ) -> Result<ResolvedTree, super::error::ResolverError> {
        Ok(ResolvedTree::default())
    }

    async fn discover_versions(
        &self,
        _: &crate::DependencyName,
        _: &crate::DependencySource,
        _: super::scope::DependencyScope,
    ) -> Result<Vec<semver::Version>, super::error::ResolverError> {
        Ok(Vec::new())
    }
}
