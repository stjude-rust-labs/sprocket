//! An inner representation for the configuration object.
//!
//! This struct holds the configuration values.

use std::path::Path;

use indexmap::IndexMap;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::parse_string;

use crate::document;
use crate::normalize_diagnostic;
use crate::repository;
use crate::repository::RawHash;

/// Helper functions for `IndexMap` TOML serialization.
mod index_map {
    use indexmap::IndexMap;
    use toml_spanner::Arena;
    use toml_spanner::Context;
    use toml_spanner::Failed;
    use toml_spanner::FromToml;
    use toml_spanner::Item;
    use toml_spanner::Key;
    use toml_spanner::Table;
    use toml_spanner::ToToml;
    use toml_spanner::ToTomlError;

    use crate::repository::Identifier;

    /// Helper function for `IndexMap` TOML deserialization.
    pub fn from_toml<'de, V>(
        ctx: &mut Context<'de>,
        item: &Item<'de>,
    ) -> Result<IndexMap<Identifier, V>, Failed>
    where
        V: FromToml<'de>,
    {
        let table = item.require_table(ctx)?;
        let mut map = IndexMap::default();
        let mut had_error = false;
        for (key, item) in table {
            let identifier = match key.name.parse::<Identifier>() {
                Ok(id) => id,
                Err(e) => {
                    ctx.report_custom_error(
                        format!(
                            "invalid repository identifier `{name}`: {e}",
                            name = key.name
                        ),
                        item,
                    );
                    had_error = true;
                    continue;
                }
            };

            match V::from_toml(ctx, item) {
                Ok(v) => {
                    map.insert(identifier, v);
                }
                Err(_) => had_error = true,
            }
        }

        if had_error { Err(Failed) } else { Ok(map) }
    }

    /// Helper function for `IndexMap` serialization.
    pub fn to_toml<'a, V>(
        value: &'a IndexMap<Identifier, V>,
        arena: &'a Arena,
    ) -> Result<Item<'a>, ToTomlError>
    where
        V: ToToml,
    {
        let Some(mut table) = Table::try_with_capacity(value.len(), arena) else {
            return Err(ToTomlError::from(
                "length of table exceeded maximum capacity",
            ));
        };

        for (k, v) in value {
            table.insert_unique(
                Key::new(arena.alloc_str(&k.to_string())),
                v.to_toml(arena)?,
                arena,
            );
        }

        Ok(table.into_item())
    }
}

/// Represents a diagnostic reported for a document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Toml)]
#[toml(Toml)]
pub struct Diagnostic {
    /// The identifier of the document containing the diagnostic.
    #[toml(FromToml with = parse_string, ToToml with = display)]
    document: document::Identifier,
    /// The short-form diagnostic message.
    message: String,
    /// Permalink to the source of the diagnostic.
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
            "https://github.com/{doc}/blob/{hash}{path}",
            doc = document.repository(),
            path = document.path()
        );
        let url = if let Some(line_no) = line_no {
            format!("{url}/#L{line_no}")
        } else {
            url
        };
        Self {
            document,
            message: normalize_diagnostic(&message),
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
#[derive(Debug, Default, Toml)]
#[toml(Toml)]
pub struct Inner {
    /// The repositories.
    #[toml(default, with = index_map)]
    repositories: IndexMap<repository::Identifier, repository::Repository>,

    /// The expected diagnostics across all repositories.
    #[toml(default)]
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
