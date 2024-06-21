//! An inner representation for the configuration object.
//!
//! This struct holds the configuration values.

use std::path::Path;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;

use crate::document;
use crate::repository;
use crate::repository::RawHash;

/// Represents a diagnostic reported for a document.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    /// The identifier of the document containing the diagnostic.
    document: document::Identifier,
    /// The short-form diagnostic message.
    message: String,
    /// Permalink to the source of the diagnostic.
    #[serde(default)]
    permalink: String,
}

impl Diagnostic {
    /// Creates a new diagnostic for the given document identifier and message.
    pub fn new(
        document: document::Identifier,
        message: String,
        hash: &RawHash,
        line_no: Option<usize>,
    ) -> Self {
        let url = format!(
            "https://github.com/{}/blob/{}{}",
            document.repository(),
            hash,
            document.path()
        );
        let url = if let Some(line_no) = line_no {
            format!("{}/#L{}", url, line_no)
        } else {
            url
        };
        Self {
            document,
            message,
            permalink: url,
        }
    }

    /// Gets the identifier of the document.
    pub fn document(&self) -> &document::Identifier {
        &self.document
    }

    /// Gets the diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Gets the permalink to the source of the diagnostic.
    pub fn permalink(&self) -> &str {
        &self.permalink
    }
}

/// The configuration object for a [`Config`](super::Config).
///
/// This object stores the actual configuration values for this subcommand.
#[serde_as]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Inner {
    /// The repositories.
    #[serde(default)]
    repositories: IndexMap<repository::Identifier, repository::Repository>,

    /// The expected diagnostics across all repositories.
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
}

impl Inner {
    /// Gets the repositories for this [`Inner`] by reference.
    pub fn repositories(&self) -> &IndexMap<repository::Identifier, repository::Repository> {
        &self.repositories
    }

    /// Gets the list of expected diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Sets the list of expected diagnostics.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }

    /// Gets the repositories for this [`Inner`] by mutable reference.
    pub fn repositories_mut(
        &mut self,
    ) -> &mut IndexMap<repository::Identifier, repository::Repository> {
        &mut self.repositories
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

    /// Sorts the configuration.
    ///
    /// This sorts the repositories by their identifiers and the diagnostics by
    /// their document identifiers and messages (lexicographically).
    pub fn sort(&mut self) {
        self.repositories.sort_by(|a, _, b, _| a.cmp(b));
        self.diagnostics.sort();
    }
}
