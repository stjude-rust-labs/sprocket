//! Value types returned by the [`Resolver`](super::Resolver) trait.

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use crate::dependency::DependencyName;
use crate::hash::ContentHash;
use crate::lockfile::ResolvedSource;
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
        _: &crate::module::Module,
        _: &crate::symbolic_path::SymbolicPath,
    ) -> Result<MaterializedFile, super::error::ResolverError> {
        Err(super::error::ResolverError::NoModuleContext)
    }

    async fn resolve_tree(
        &self,
        _: &crate::module::Module,
    ) -> Result<ResolvedTree, super::error::ResolverError> {
        Ok(ResolvedTree::default())
    }

    async fn discover_versions(
        &self,
        _: &crate::dependency::DependencyName,
        _: &crate::dependency::DependencySource,
        _: super::scope::DependencyScope,
    ) -> Result<Vec<semver::Version>, super::error::ResolverError> {
        Ok(Vec::new())
    }
}
