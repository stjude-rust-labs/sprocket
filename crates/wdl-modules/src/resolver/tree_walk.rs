//! Resolver-layer tree walk delegating to the shared
//! [`module_walk`](crate::module_walk) implementation.

use std::path::Path;

use crate::module_walk;
/// Statistics collected during a tree walk.
pub(crate) use crate::module_walk::TreeStats;
use crate::resolver::error::ResolverError;

/// Walks every regular file under `root` using the shared safe
/// module-content walker. Converts errors to [`ResolverError`].
pub(crate) fn walk_module_tree(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), ResolverError>,
) -> Result<TreeStats, ResolverError> {
    module_walk::walk_module_tree(root, visitor).map_err(|e| match e {
        module_walk::WalkError::Hash(h) => ResolverError::Hash(h),
        module_walk::WalkError::Visitor(r) => r,
    })
}
