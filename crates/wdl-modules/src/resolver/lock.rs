//! Lockfile API façade.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::DependencyMap;
use crate::lockfile::ResolvedSource;
use crate::resolver::types::ResolvedDependency;
use crate::resolver::types::ResolvedTree;
use crate::signing::SignerIdentity;
use crate::signing::VerifyingKey;

mod diff;
mod relock;

pub use diff::ChangedSigner;
pub use diff::DependencyChange;
pub use diff::DependencyUpdate;
pub use diff::LockfileDiff;
pub use diff::NewSigner;
pub use diff::RemovedSigner;
pub use diff::SignerIdentityMap;
pub use diff::signer_identity_map;
pub use relock::RelockOutcome;
pub use relock::RelockStats;
pub use relock::partial_relock;
pub(crate) use relock::satisfies;
pub use relock::update_relock;

#[cfg(test)]
mod facade_tests {
    use super::LockfileDiff;
    use super::RelockOutcome;
    use super::RelockStats;
    use super::SignerIdentityMap;
    use super::partial_relock;
    use super::signer_identity_map;
    use super::update_relock;

    #[test]
    fn curated_lock_api_is_reexported() {
        fn consume<T>() {}
        consume::<LockfileDiff>();
        consume::<RelockOutcome>();
        consume::<RelockStats>();
        consume::<SignerIdentityMap>();
        let _ = partial_relock;
        let _ = signer_identity_map;
        let _ = update_relock;
    }
}
