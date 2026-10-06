//! Path-aware persistence for the module trust store.
//!
//! This module owns loading and saving for the user trust store. Signer-change
//! policy, prompting, and rendering live in [`super::signer_policy`].

use std::path::PathBuf;

use anyhow::Context as _;
use wdl_modules::Lockfile;
use wdl_modules::resolver::TrustStore;

/// A trust store bound to the filesystem path it was loaded from.
///
/// The path travels with the store so callers persist their mutations back to
/// the location they came from without threading a separate path argument.
#[derive(Clone, Debug)]
pub(super) struct TrustStoreFile {
    /// The filesystem path backing this trust store.
    path: PathBuf,
    /// The in-memory trust store loaded from `path`.
    store: TrustStore,
}

impl TrustStoreFile {
    /// Loads the trust store at `path`, defaulting to an empty store when the
    /// file does not yet exist.
    pub(super) fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let store = TrustStore::load_or_default(&path)
            .with_context(|| format!("loading module trust store `{}`", path.display()))?;
        tracing::debug!(
            trust_store = %path.display(),
            keys = store.keys.len(),
            "loaded module trust store"
        );
        Ok(Self { path, store })
    }

    /// Returns a shared reference to the underlying trust store.
    pub(super) fn store(&self) -> &TrustStore {
        &self.store
    }

    /// Returns a mutable reference to the underlying trust store.
    pub(super) fn store_mut(&mut self) -> &mut TrustStore {
        &mut self.store
    }

    /// Consumes the file, returning the loaded trust store.
    pub(super) fn into_store(self) -> TrustStore {
        self.store
    }

    /// Persists the trust store back to its configured path.
    pub(super) fn save(&self) -> anyhow::Result<()> {
        self.store
            .save(&self.path)
            .with_context(|| format!("saving module trust store `{}`", self.path.display()))?;
        tracing::debug!(
            trust_store = %self.path.display(),
            keys = self.store.keys.len(),
            "wrote module trust store"
        );
        Ok(())
    }

    /// Trusts every signer key recorded in a lockfile and saves the store,
    /// returning the number of newly added keys.
    pub(super) fn accept_lockfile_signers(&mut self, lockfile: &Lockfile) -> anyhow::Result<usize> {
        let accepted = self.store.trust_lockfile_signers(lockfile);
        self.save()?;
        Ok(accepted)
    }
}

#[cfg(test)]
mod tests {
    use wdl_modules::resolver::TrustStore;
    use wdl_modules::signing::VerifyingKey;

    use super::*;

    /// Builds a deterministic verifying key for a test seed.
    fn vkey(seed: u64) -> VerifyingKey {
        wdl_modules::signing::test_utils::signing_key_from_seed(seed).verifying_key()
    }

    /// Builds a one-entry lockfile whose Git dependency `dep` from `url`
    /// carries the given optional signer.
    fn signed_lockfile(dep: &str, url: &str, signer: Option<VerifyingKey>) -> Lockfile {
        use wdl_modules::lockfile::DependencyEntry;
        use wdl_modules::lockfile::ResolvedSource;

        let mut dependencies = std::collections::BTreeMap::new();
        dependencies.insert(
            dep.parse().unwrap(),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: url.parse().unwrap(),
                    sha: "0000000000000000000000000000000000000000".parse().unwrap(),
                    selector: wdl_modules::dependency::GitSelector::Version("^1".parse().unwrap()),
                    path: None,
                },
                checksum: Some(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .parse()
                        .unwrap(),
                ),
                signer,
                dependencies: std::collections::BTreeMap::new(),
            },
        );
        Lockfile {
            version: wdl_modules::lockfile::LOCKFILE_VERSION,
            dependencies,
        }
    }

    #[test]
    fn trust_store_file_round_trips_its_configured_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("modules-trust.toml");
        let mut file = TrustStoreFile::load(path.clone()).unwrap();
        let key = wdl_modules::signing::test_utils::signing_key_from_seed(7).verifying_key();
        file.store_mut().insert_key(key);
        file.save().unwrap();

        assert_eq!(
            TrustStore::load_or_default(&path).unwrap(),
            file.store().clone()
        );
    }

    #[test]
    fn accept_lockfile_signers_counts_only_new_keys() {
        let url = "https://example.com/repo";
        let lockfile = signed_lockfile("dep", url, Some(vkey(1)));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trust.toml");
        let mut file = TrustStoreFile::load(path).unwrap();

        assert_eq!(file.accept_lockfile_signers(&lockfile).unwrap(), 1);
        assert!(file.store().contains_key(&vkey(1)));
        assert_eq!(file.accept_lockfile_signers(&lockfile).unwrap(), 0);
    }

    #[test]
    fn accept_lockfile_signers_persists_trust() {
        let url = "https://example.com/repo";
        let lockfile = signed_lockfile("dep", url, Some(vkey(2)));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trust.toml");
        let mut file = TrustStoreFile::load(path.clone()).unwrap();
        assert_eq!(file.accept_lockfile_signers(&lockfile).unwrap(), 1);

        let reloaded = TrustStore::load_or_default(&path).unwrap();
        assert!(reloaded.contains_key(&vkey(2)));
    }
}
