//! Resolver layer.
//!
//! Gated behind the `resolver` cargo feature. Pulls in `git2`, `tokio`,
//! `dirs`, `bytesize`, `toml`, and `tracing`. Consumers that only need
//! the manifest/lockfile/hashing types (e.g. `wdl-doc`) do not enable
//! this feature and therefore do not pay for those deps.

pub mod config;
pub mod error;

pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
