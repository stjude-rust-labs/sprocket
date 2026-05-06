//! Resolver layer.
//!
//! Gated behind the `resolver` cargo feature. Pulls in `git2`, `tokio`,
//! `dirs`, `bytesize`, `toml`, and `tracing`. Consumers that only need
//! the manifest/lockfile/hashing types (e.g. `wdl-doc`) do not enable
//! this feature and therefore do not pay for those deps.

pub mod cache;
pub mod config;
pub mod error;
// NOTE: items inside `git` are wired up by the forthcoming `GitResolver`;
// `#[expect(dead_code)]` would error in test builds where its own tests
// already exercise the items.
#[allow(dead_code)]
mod git;
pub mod lock;
pub mod trust;
pub mod types;
pub mod versions;

use async_trait::async_trait;
use semver::Version;

use crate::DependencySource;
use crate::Manifest;
use crate::SymbolicPath;
pub use crate::resolver::cache::CacheKey;
pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
pub use crate::resolver::lock::DependencyAddition;
pub use crate::resolver::lock::DependencyUpdate;
pub use crate::resolver::lock::LockfileDiff;
pub use crate::resolver::lock::NewSigner;
pub use crate::resolver::lock::RelockOutcome;
pub use crate::resolver::lock::RelockStats;
pub use crate::resolver::lock::partial_relock;
pub use crate::resolver::trust::TrustEntry;
pub use crate::resolver::trust::TrustStore;
pub use crate::resolver::trust::TrustStoreError;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;

/// Resolves WDL module imports to concrete files on disk.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Materializes a single symbolic import on disk and returns the path to
    /// the resulting file.
    ///
    /// The primary call site for `wdl-analysis`. When the analyzer encounters a
    /// symbolic import like `import openwdl/csvkit/cut`, it asks the resolver
    /// for the file path that statement should route to, then parses the result
    /// with the existing import machinery as if the user had written `import
    /// "<that path>"`.
    ///
    /// - `consumer` is the manifest of the importing module.
    /// - `path` is the parsed symbolic path.
    ///
    /// The resolver looks up the head component in `consumer.dependencies`,
    /// materializes the dep's module folder if not yet cached, and resolves
    /// either the manifest's `entrypoint` (when the symbolic path has no
    /// sub-path) or `<sub-path>.wdl` under the module folder.
    async fn materialize(
        &self,
        consumer: &Manifest,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError>;

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// Walks the consumer's `dependencies` map, recurses into each dep's own
    /// manifest, and records every module visited along the way. Detects
    /// cycles.
    async fn resolve_tree(&self, consumer: &Manifest) -> Result<ResolvedTree, ResolverError>;

    /// Lists discovered versions for a dependency source that satisfy the
    /// requirement, in descending semver order.
    ///
    /// Used by CLI commands that surface available versions to the user and
    /// internally by `resolve_tree` to select the version a Git dep resolves
    /// to.
    async fn discover_versions(
        &self,
        source: &DependencySource,
    ) -> Result<Vec<Version>, ResolverError>;
}
