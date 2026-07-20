//! Public resolver API.
//!
//! The trait definitions (`Resolver`, `ResolverError`, `MaterializedFile`,
//! etc.) are always available. The Git-backed implementation and supporting
//! infrastructure (cache, config, fetch, lock, trust, etc.) are gated
//! behind the `resolver` cargo feature so that consumers like `wdl-doc`
//! that only need the manifest/lockfile/hashing types do not pay for
//! `git2` and friends.

#[cfg(feature = "git-resolver")]
pub(crate) mod cache;
#[cfg(feature = "git-resolver")]
pub(crate) mod config;
pub(crate) mod error;
#[cfg(feature = "git-resolver")]
pub(crate) mod fetch;
#[cfg(feature = "git-resolver")]
mod git;
#[cfg(feature = "git-resolver")]
mod git_resolver;
#[cfg(feature = "git-resolver")]
pub mod lock;
#[cfg(feature = "git-resolver")]
pub(crate) mod policy;
pub(crate) mod scope;
#[cfg(feature = "git-resolver")]
pub(crate) mod trust;
pub(crate) mod types;
#[cfg(feature = "git-resolver")]
pub(crate) mod verify;
#[cfg(feature = "git-resolver")]
pub(crate) mod versions;

use async_trait::async_trait;
use semver::Version;

use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::module::Module;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::GitPlatform;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::LargeFileWarning;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::LargeFileWarningError;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::ModulesConfig;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::GitRefKind;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::git_resolver::CacheCleanStats;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::git_resolver::GitResolver;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::git_resolver::GitResolverBuilder;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::git_resolver::VerifyLockedReport;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::policy::ResolverPolicy;
pub use crate::resolver::scope::DependencyScope;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustStore;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustStoreError;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustedIdentity;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;
use crate::symbolic_path::SymbolicPath;

/// Resolves WDL module imports to concrete files on disk.
#[async_trait]
pub trait Resolver: std::fmt::Debug + Send + Sync {
    /// Materializes a single symbolic import on disk and returns the path
    /// to the resulting file.
    ///
    /// The primary call site for `wdl-analysis`. When the analyzer
    /// encounters a symbolic import like `import openwdl/csvkit/cut`, it
    /// asks the resolver for the file path that statement should route
    /// to, then parses the result with the existing import machinery as
    /// if the user had written `import "<that path>"`.
    ///
    /// `consumer` is the importing module: its manifest declares the
    /// symbolic path's head component, and its root rebases any
    /// relative `LocalPath` dependencies. `path` is the parsed
    /// symbolic path. The resolver looks up the head component in
    /// `consumer.manifest.dependencies`, materializes the dep's module
    /// folder if not yet cached, and resolves either the manifest's
    /// `entrypoint` (when the symbolic path has no sub-path) or
    /// `<sub-path>.wdl` under the module folder.
    async fn materialize(
        &self,
        consumer: &Module,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError>;

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// Walks `consumer.manifest.dependencies`, recurses into each dep's
    /// own manifest, and records every module visited along the way.
    /// Relative `LocalPath` entries are resolved against the declaring
    /// module's root. Detects cycles.
    async fn resolve_tree(&self, consumer: &Module) -> Result<ResolvedTree, ResolverError>;

    /// Lists discovered versions for a dependency source that satisfy
    /// the requirement, in descending semver order.
    ///
    /// Used by CLI commands that surface available versions to the user
    /// and internally by `resolve_tree` to select the version a Git dep
    /// resolves to.
    async fn discover_versions(
        &self,
        name: &DependencyName,
        source: &DependencySource,
        scope: DependencyScope,
    ) -> Result<Vec<Version>, ResolverError>;
}
