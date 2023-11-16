//! An inner representation for the configuration object.
//!
//! This struct holds the configuration values.
use std::collections::HashMap;
use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use wdl_grammar as grammar;

mod repr;

pub use repr::ErrorsAsReprs;

use crate::commands::gauntlet::document;
use crate::commands::gauntlet::repository;

/// Parsing errors as [`String`]s associated with a [document
/// identifier](document::Identifier).
pub type Errors = HashMap<document::Identifier, String>;

/// A unique set of [repository identifiers](repository::Identifier).
pub type Repositories = HashSet<repository::Identifier>;

/// The inner configuration object for a [`Config`](super::Config).
///
/// This object stores the actual configuration values for this subcommand.
#[serde_as]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Inner {
    /// The WDL version.
    pub(super) version: grammar::Version,

    /// The repositories.
    #[serde(default)]
    pub(super) repositories: Repositories,

    /// The ignored errors.
    #[serde_as(as = "ErrorsAsReprs")]
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(super) ignored_errors: Errors,
}

impl From<grammar::Version> for Inner {
    fn from(version: grammar::Version) -> Self {
        Self {
            version,
            repositories: Default::default(),
            ignored_errors: Default::default(),
        }
    }
}
