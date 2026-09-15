//! Wrapper over `git2` covering the operations the resolver needs.
//! Handles credential delegation, Git fetches, and sparse checkout of
//! selected module folders within the cloned tree.

//! ## Cache layout
//!
//! Each resolved Git dependency is materialized under the resolver's
//! `cache_root`. [`CacheKey`](crate::resolver::cache::CacheKey) derives the
//! directory structure from the Git URL and commit SHA.
//!
//! ```text
//! <cache_root>/
//!   <host>/                                        # structured layout
//!     <org>/
//!       <repo>-<digest8>/
//!         .<commit_sha>.lock                       # advisory file lock
//!         .<commit_sha>.sparse.json                # sparse-checkout metadata
//!         <commit_sha>/                            # the "cache leaf", a clean Git checkout
//!           .git/
//!           csvkit/                                # a materialized module folder
//!             module.json
//!             index.wdl
//!           spellbook/                             # another module folder (added by extend)
//!             module.json
//!             index.wdl
//!   _opaque/                                       # fallback for URLs without host/org/repo
//!     <sha256(url)>/
//!       .<commit_sha>.lock
//!       .<commit_sha>.sparse.json
//!       <commit_sha>/
//!         .git/
//!         ...
//! ```
//!
//! The structured layout is used when the Git URL has a host and at least two
//! path segments. Additional path segments remain part of the URL digest, which
//! prevents nested repository URLs from colliding. Other URLs fall back to the
//! `_opaque/` layout keyed by a SHA-256 digest of the URL.
//!
//! Both `.<commit>.lock` and `.<commit>.sparse.json` live next to the
//! cache leaf (in its parent directory), keeping the Git checkout
//! clean. `.sparse.json` tracks which module folders have been checked
//! out so far; when a second dependency in the same repository needs
//! a different folder, the existing checkout is extended rather than
//! re-cloned. The `.lock` file serializes concurrent operations via
//! `File::lock()`.

mod cache_store;
mod checkout;
mod creds;
mod error;
mod remote;
#[cfg(test)]
mod test_support;

#[cfg(test)]
pub(crate) use cache_store::CACHE_MARKER_FILENAME;
pub(crate) use cache_store::CacheLocation;
pub(crate) use cache_store::initialize_cache_root;
pub(crate) use cache_store::remove_cache_leaves;
pub(crate) use cache_store::remove_cache_root;
pub(crate) use checkout::TreeLimits;
pub(crate) use checkout::ensure_materialized;
pub(crate) use creds::CredentialMode;
pub(crate) use creds::FetchPolicy;
pub(crate) use error::GitError;
pub(crate) use remote::discover_default_branch;
pub(crate) use remote::list_advertised_refs;
pub(crate) use remote::resolve_commit_prefix;
pub(crate) use remote::unique_ref_prefix_match;
