//! A reportable concern.
//!
//! Reportable concerns are [`Concern`](wdl_core::Concern)s that have been
//! simplified and serialized for reporting within a [configuration
//! file](crate::config::Inner). Reportable concerns ignore `LintWarning`s,
//! only focusing on `ParseError`s and `ValidationFailure`s.

use serde::Deserialize;
use serde::Serialize;

/// A kind of reportable concern.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Kind {
    /// A parse error.
    ParseError,

    /// A validation failure.
    ValidationFailure,
}

/// A representation of an error to ignore as serialized in the configuration
/// file.
///
/// In short, I wanted to convert the
/// [`Concerns`](crate::config::inner::ReportableConcerns) object to something
/// more visually understandable in the configuration file. Thus, the only
/// purpose of this struct is to serialize and deserialize entries in that
/// [`HashMap`](std::collections::HashMap) in a prettier way.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ReportableConcern {
    /// The kind.
    kind: Kind,

    /// The document containing the concern.
    document: String,

    /// The concern message.
    message: String,
}

impl ReportableConcern {
    /// Create a new [`ReportableConcern`].
    ///
    /// **Note:** most often, you'll use [`ReportableConcern::from_concern()`]
    /// to convert a [`Concern`](wdl_core::Concern) to a [`ReportableConcern`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let reportable_concern =
    ///     ReportableConcern::new(Kind::ValidationFailure, "Foo/Bar:quux.wdl", "Hello, world!");
    ///
    /// assert_eq!(reportable_concern.kind(), &Kind::ValidationFailure);
    /// assert_eq!(reportable_concern.document(), "Foo/Bar:quux.wdl");
    /// assert_eq!(reportable_concern.message(), "Hello, world!");
    /// ```
    pub fn new(kind: Kind, document: impl Into<String>, message: impl Into<String>) -> Self {
        let document = document.into();
        let message = message.into();

        Self {
            kind,
            document,
            message,
        }
    }

    /// Create a new [`ReportableConcern`] from a document name and a
    /// [`Concern`](wdl_core::Concern).
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::Concern;
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    ///
    /// let reportable_concern = ReportableConcern::from_concern("Foo/Bar:quux.wdl", concern).unwrap();
    ///
    /// assert_eq!(reportable_concern.message(), "Hello, world!");
    /// assert_eq!(reportable_concern.document(), "Foo/Bar:quux.wdl");
    /// assert_eq!(reportable_concern.kind(), &Kind::ParseError);
    /// ```
    pub fn from_concern(document: impl Into<String>, concern: wdl_core::Concern) -> Option<Self> {
        let document = document.into();
        let message = concern.to_string();

        match &concern {
            wdl_core::Concern::LintWarning(_) => None,
            wdl_core::Concern::ParseError(_) => Some(ReportableConcern {
                kind: Kind::ParseError,
                document,
                message,
            }),
            wdl_core::Concern::ValidationFailure(_) => Some(ReportableConcern {
                kind: Kind::ValidationFailure,
                document,
                message,
            }),
        }
    }

    /// Gets the [`Kind`] from the [`ReportableConcern`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let reportable_concern =
    ///     ReportableConcern::new(Kind::ValidationFailure, "Foo/Bar:quux.wdl", "Hello, world!");
    ///
    /// assert_eq!(reportable_concern.kind(), &Kind::ValidationFailure);
    /// ```
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Gets the document name from the [`ReportableConcern`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let reportable_concern =
    ///     ReportableConcern::new(Kind::ValidationFailure, "Foo/Bar:quux.wdl", "Hello, world!");
    ///
    /// assert_eq!(reportable_concern.document(), "Foo/Bar:quux.wdl");
    /// ```
    pub fn document(&self) -> &str {
        self.document.as_ref()
    }

    /// Gets the concern message from the [`ReportableConcern`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_gauntlet::config::reportable_concern::Kind;
    /// use wdl_gauntlet::config::ReportableConcern;
    ///
    /// let reportable_concern =
    ///     ReportableConcern::new(Kind::ValidationFailure, "Foo/Bar:quux.wdl", "Hello, world!");
    ///
    /// assert_eq!(reportable_concern.message(), "Hello, world!");
    /// ```
    pub fn message(&self) -> &str {
        self.message.as_ref()
    }
}

impl std::fmt::Display for ReportableConcern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "|{}| {}", self.document, self.message)
    }
}
