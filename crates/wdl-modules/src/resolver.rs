//! Resolver layer.
//!
//! Gated behind the `resolver` cargo feature. Pulls in `git2`, `tokio`,
//! `dirs`, `bytesize`, `toml`, and `tracing`. Consumers that only need
//! the manifest/lockfile/hashing types (e.g. `wdl-doc`) do not enable
//! this feature and therefore do not pay for those deps.

pub mod config;
pub mod error;
pub mod types;

use async_trait::async_trait;
use semver::Version;

pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedFile;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;

use crate::DependencySource;
use crate::Manifest;
use crate::SymbolicPath;

/// Resolves WDL module imports to concrete files on disk.
///
/// `wdl-analysis` takes a `dyn Resolver` so its symbolic-import
/// resolution path stays free of `git2` and filesystem dependencies. The
/// CLI uses the concrete `GitResolver` directly.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolves a single symbolic import to a concrete file on disk.
    async fn resolve(
        &self,
        consumer: &Manifest,
        path: &SymbolicPath,
    ) -> Result<ResolvedFile, ResolverError>;

    /// Resolves every transitive dependency declared by `consumer`.
    async fn resolve_tree(&self, consumer: &Manifest) -> Result<ResolvedTree, ResolverError>;

    /// Lists discovered versions for a dependency source that satisfy
    /// the requirement, in descending semver order.
    async fn discover_versions(
        &self,
        source: &DependencySource,
    ) -> Result<Vec<Version>, ResolverError>;
}
