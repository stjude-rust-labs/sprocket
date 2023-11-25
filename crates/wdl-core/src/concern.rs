//! Concerns.

pub mod code;
pub mod concerns;
pub mod lint;
pub mod parse;
pub mod validation;

pub use code::Code;
pub use concerns::Concerns;
use serde::Deserialize;
use serde::Serialize;

/// A concern.
///
/// A concern is defined as any information that is important to interpretting a
/// returned parse or abstract syntax tree. There are three classes of concerns
/// that can be returned:
///
/// * **Parse errors**, which are errors that are return by Pest during the
///   building of the parse tree.
/// * **Validation failures**, which are error that are occur when validating a
///   parse tree or abstract syntax tree.
/// * **Lint warnings**, which are notifications about syntactically and
///   semantically correct code that is otherwise notable, such as stylistic
///   errors, programming mistakes, or deviations from best practices.
#[derive(Clone, Debug, Deserialize, Hash, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Concern {
    /// A lint warning.
    LintWarning(lint::Warning),

    /// A parse error.
    ParseError(parse::Error),

    /// A validation failure.
    ValidationFailure(validation::Failure),
}

impl Concern {
    /// Returns [`Some(&lint::Warning)`] if the [`Concern`] is a
    /// [`lint::Warning`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.as_lint_warning().is_none());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.as_lint_warning().is_none());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.as_lint_warning().is_some());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_lint_warning(&self) -> Option<&lint::Warning> {
        match self {
            Concern::LintWarning(warning) => Some(warning),
            _ => None,
        }
    }

    /// Consumes `self` and returns [`Some(lint::Warning)`] if the [`Concern`]
    /// is a [`lint::Warning`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.into_lint_warning().is_none());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.into_lint_warning().is_none());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.into_lint_warning().is_some());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_lint_warning(self) -> Option<lint::Warning> {
        match self {
            Concern::LintWarning(warning) => Some(warning),
            _ => None,
        }
    }

    /// Returns [`Some(&validation::Failure)`] if the [`Concern`] is a
    /// [`validation::Failure`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.as_validation_failure().is_none());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.as_validation_failure().is_some());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.as_validation_failure().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_validation_failure(&self) -> Option<&validation::Failure> {
        match self {
            Concern::ValidationFailure(failure) => Some(failure),
            _ => None,
        }
    }

    /// Consumes `self` and returns [`Some(validation::Failure)`] if the
    /// [`Concern`] is a [`validation::Failure`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.into_validation_failure().is_none());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.into_validation_failure().is_some());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.into_validation_failure().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_validation_failure(self) -> Option<validation::Failure> {
        match self {
            Concern::ValidationFailure(failure) => Some(failure),
            _ => None,
        }
    }

    /// Returns [`Some(&parse::Error)`] if the [`Concern`] is a
    /// [`parse::Error`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.as_parse_error().is_some());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.as_parse_error().is_none());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.as_parse_error().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn as_parse_error(&self) -> Option<&parse::Error> {
        match self {
            Concern::ParseError(error) => Some(error),
            _ => None,
        }
    }

    /// Consumes `self` and returns [`Some(parse::Error)`] if the [`Concern`] is
    /// a [`parse::Error`]. Otherwise, returns [`None`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::parse;
    /// use wdl_core::concern::validation;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    /// use wdl_core::Version;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// assert!(concern.into_parse_error().is_some());
    ///
    /// let failure = validation::failure::Builder::default()
    ///     .code(Code::try_new(Kind::Error, Version::V1, 1)?)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::ValidationFailure(failure);
    /// assert!(concern.into_parse_error().is_none());
    ///
    /// let warning = lint::warning::Builder::default()
    ///     .code(Code::try_new(Kind::Warning, Version::V1, 1)?)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// let concern = Concern::LintWarning(warning);
    /// assert!(concern.into_parse_error().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_parse_error(self) -> Option<parse::Error> {
        match self {
            Concern::ParseError(error) => Some(error),
            _ => None,
        }
    }
}

impl std::fmt::Display for Concern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Concern::LintWarning(warning) => write!(f, "{warning}"),
            Concern::ParseError(error) => write!(f, "{error}"),
            Concern::ValidationFailure(failure) => write!(f, "{failure}"),
        }
    }
}
