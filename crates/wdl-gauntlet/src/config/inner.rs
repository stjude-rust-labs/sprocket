//! An inner representation for the configuration object.
//!
//! This struct holds the configuration values.

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

/// A unique set of [repository identifiers](repository::Identifier).
pub type Repositories = IndexSet<repository::Identifier>;

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
    repositories: Repositories,

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
    /// let config = r#"version = "v1"
    ///
    /// [[repositories]]
    /// organization = "Foo"
    /// name = "Bar""#;
    ///
    /// let inner: Inner = toml::from_str(&config).unwrap();
    /// assert_eq!(inner.version(), &wdl_core::Version::V1);
    /// ```
    pub fn version(&self) -> &wdl_core::Version {
        &self.version
    }

    /// Gets the [`Repositories`] for this [`Inner`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use gauntlet::config::Inner;
    /// use wdl_gauntlet as gauntlet;
    ///
    /// let config = r#"version = "v1"
    ///
    /// [[repositories]]
    /// organization = "Foo"
    /// name = "Bar""#;
    ///
    /// let inner: Inner = toml::from_str(&config).unwrap();
    /// assert_eq!(inner.repositories().len(), 1);
    /// ```
    pub fn repositories(&self) -> &Repositories {
        &self.repositories
    }

    /// Extends the [`Repositories`] for this [`Inner`].
    ///
    /// # Examples
    ///
    /// ```
    /// use gauntlet::config::Inner;
    /// use indexmap::IndexSet;
    /// use wdl_gauntlet as gauntlet;
    ///
    /// let config = r#"version = "v1"
    ///
    /// [[repositories]]
    /// organization = "Foo"
    /// name = "Bar""#;
    ///
    /// let mut inner: Inner = toml::from_str(&config).unwrap();
    ///
    /// let mut repositories = IndexSet::new();
    /// repositories.insert(
    ///     "Foo/Baz"
    ///         .parse::<gauntlet::repository::Identifier>()
    ///         .unwrap(),
    /// );
    ///
    /// inner.extend_repositories(repositories);
    ///
    /// assert_eq!(inner.repositories().len(), 2);
    /// ```
    pub fn extend_repositories<T: IntoIterator<Item = repository::Identifier>>(
        &mut self,
        items: T,
    ) {
        self.repositories.extend(items);
        self.repositories.sort();
    }

    /// Gets the [`ReportableConcerns`] for this [`Inner`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use gauntlet::config::Inner;
    /// use wdl_gauntlet as gauntlet;
    ///
    /// let config = r#"version = "v1"
    ///
    /// [[concerns]]
    /// document = "Foo/Bar:baz.wdl"
    /// kind = "LintWarning"
    /// message = '''an error'''"#;
    ///
    /// let mut inner: Inner = toml::from_str(&config).unwrap();
    ///
    /// assert_eq!(inner.concerns().len(), 1);
    /// ```
    pub fn concerns(&self) -> &ReportableConcerns {
        &self.concerns
    }

    /// Replaces the [`ReportableConcerns`] for this [`Inner`].
    ///
    /// # Examples
    ///
    /// ```
    /// use indexmap::IndexSet;
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::Inner;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let config = r#"version = "v1"
    ///
    /// [[concerns]]
    /// document = "Foo/Bar:baz.wdl"
    /// kind = "LintWarning"
    /// message = '''an error'''"#;
    ///
    /// let mut inner: Inner = toml::from_str(&config).unwrap();
    ///
    /// let mut concerns = IndexSet::new();
    /// concerns.insert(ReportableConcern::new(
    ///     Kind::LintWarning,
    ///     "Foo/Bar:quux.wdl",
    ///     "Hello, world!",
    /// ));
    /// inner.set_concerns(concerns);
    ///
    /// assert_eq!(inner.concerns().len(), 1);
    ///
    /// let reportable_concern = inner.concerns().first().unwrap();
    /// assert_eq!(reportable_concern.kind(), &Kind::LintWarning);
    /// assert_eq!(reportable_concern.document(), "Foo/Bar:quux.wdl");
    /// assert_eq!(reportable_concern.message(), "Hello, world!");
    ///
    /// Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set_concerns(&mut self, concerns: ReportableConcerns) {
        self.concerns = concerns;
        self.concerns.sort();
    }

    /// Sorts the [`Repositories`] and the [`ReportableConcerns`] (by key).
    pub fn sort(&mut self) {
        self.repositories.sort();
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
