//! An inner representation for the configuration object.
//!
//! This struct holds the configuration values.

use std::path::Path;

use indexmap::IndexMap;
use indexmap::IndexSet;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;

use crate::config::ReportableConcern;
use crate::repository;

mod repr;

pub use repr::ReportableConcernsRepr;

/// A set of concerns serialized into their string form for storage within a
/// configuration file.
pub type ReportableConcerns = IndexSet<ReportableConcern>;

/// The  configuration object for a [`Config`](super::Config).
///
/// This object stores the actual configuration values for this subcommand.
#[serde_as]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Inner {
    /// The WDL version.
    version: wdl_core::Version,

    /// The repositories.
    #[serde(default)]
    repositories: IndexMap<repository::Identifier, repository::Repository>,

    /// The reportable concerns.
    #[serde_as(as = "ReportableConcernsRepr")]
    #[serde(default, skip_serializing_if = "IndexSet::is_empty")]
    concerns: ReportableConcerns,
}

impl Inner {
    /// Gets the [`Version`](wdl_core::Version) for this [`Inner`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use gauntlet::config::Inner;
    /// use wdl_gauntlet as gauntlet;
    ///
    /// let config = r#"version = "v1""#;
    ///
    /// let inner: Inner = toml::from_str(&config).unwrap();
    /// assert_eq!(inner.version(), &wdl_core::Version::V1);
    /// ```
    pub fn version(&self) -> &wdl_core::Version {
        &self.version
    }

    /// Gets the [`Repositories`] for this [`Inner`] by reference.
    pub fn repositories(&self) -> &IndexMap<repository::Identifier, repository::Repository> {
        &self.repositories
    }

    /// Extends the `repositories` for this [`Inner`] with the given items.
    pub fn extend_repositories(
        &mut self,
        items: IndexMap<repository::Identifier, repository::Repository>,
    ) {
        self.repositories.extend(items);
        self.repositories.sort_by(|a, _, b, _| a.cmp(b));
    }

    /// Update the `repositories` for this [`Inner`].
    pub fn update_repositories(&mut self, work_dir: &Path) {
        for repository in self.repositories.values_mut() {
            repository.update(work_dir);
        }
    }

    /// Gets the [`ReportableConcerns`] for this [`Inner`] by reference.
    pub fn concerns(&self) -> &ReportableConcerns {
        &self.concerns
    }

    /// Replaces the [`ReportableConcerns`] for this [`Inner`].
    pub fn set_concerns(&mut self, concerns: ReportableConcerns) {
        self.concerns = concerns;
        self.concerns.sort();
    }

    /// Sorts the [`Repositories`] and the [`ReportableConcerns`] by key.
    pub fn sort(&mut self) {
        self.repositories.sort_by(|a, _, b, _| a.cmp(b));
        self.concerns.sort();
    }
}

impl From<wdl_core::Version> for Inner {
    fn from(version: wdl_core::Version) -> Self {
        Self {
            version,
            repositories: Default::default(),
            concerns: Default::default(),
        }
    }
}
