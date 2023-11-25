//! A set of [`Concern`]s.

use std::collections::VecDeque;

use nonempty::NonEmpty;

use crate::concern::lint;
use crate::concern::parse;
use crate::concern::validation;
use crate::concern::Concern;

mod builder;

pub use builder::Builder;

/// The inner type for [`Concerns`].
pub type Inner = NonEmpty<Concern>;

/// A non-empty list of [`Concern`]s.
#[derive(Clone, Debug)]
pub struct Concerns(Inner);

impl Concerns {
    /// Gets the [inner value](Inner) by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern.clone()).build().unwrap();
    ///
    /// assert_eq!(concerns.inner().len(), 1);
    /// assert_eq!(concerns.inner().first(), &concern);
    /// ```
    pub fn inner(&self) -> &Inner {
        &self.0
    }

    /// Consumes `self` and returns the [inner value](Inner).
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern.clone()).build().unwrap();
    ///
    /// let inner = concerns.into_inner();
    /// assert_eq!(inner.len(), 1);
    /// assert_eq!(inner.into_iter().next().unwrap(), concern);
    /// ```
    pub fn into_inner(self) -> Inner {
        self.0
    }

    /// Returns the [`lint::Warning`]s contained with the [`Concerns`] by
    /// reference.
    ///
    /// * If lint warnings exist within the [`Concerns`], a [`NonEmpty`] of
    ///   references to the warnings will be returned wrapped in [`Some`].
    /// * If no lint warnings exist within the [`Concerns`], [`None`] will be
    ///   returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::concerns::Builder;
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    ///
    /// assert!(concerns.lint_warnings().is_none());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.lint_warnings().is_none());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.lint_warnings().is_some());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn lint_warnings(&self) -> Option<NonEmpty<&lint::Warning>> {
        let mut warnings = self
            .inner()
            .iter()
            .flat_map(|concern| concern.as_lint_warning())
            .collect::<VecDeque<_>>();

        warnings.pop_front().map(|front| {
            let mut results = NonEmpty::new(front);
            results.extend(warnings);
            results
        })
    }

    /// Returns the [`validation::Failure`]s contained with the [`Concerns`] by
    /// reference.
    ///
    /// * If validation failures exist within the [`Concerns`], a [`NonEmpty`]
    ///   of references to the warnings will be returned wrapped in [`Some`].
    /// * If no validation failures exist within the [`Concerns`], [`None`] will
    ///   be returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::concerns::Builder;
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    ///
    /// assert!(concerns.validation_failures().is_none());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.validation_failures().is_some());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.validation_failures().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn validation_failures(&self) -> Option<NonEmpty<&validation::Failure>> {
        let mut failures = self
            .inner()
            .iter()
            .flat_map(|concern| concern.as_validation_failure())
            .collect::<VecDeque<_>>();

        failures.pop_front().map(|front| {
            let mut results = NonEmpty::new(front);
            results.extend(failures);
            results
        })
    }

    /// Returns the [`parse::Error`]s contained with the [`Concerns`] by
    /// reference.
    ///
    /// * If parse errors exist within the [`Concerns`], a [`NonEmpty`] of
    ///   references to the warnings will be returned wrapped in [`Some`].
    /// * If no parse errors exist within the [`Concerns`], [`None`] will be
    ///   returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::concerns::Builder;
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    ///
    /// assert!(concerns.parse_errors().is_some());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.parse_errors().is_none());
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
    /// let concerns = Builder::default().push(concern).build().unwrap();
    /// assert!(concerns.parse_errors().is_none());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_errors(&self) -> Option<NonEmpty<&parse::Error>> {
        let mut errors = self
            .inner()
            .iter()
            .flat_map(|concern| concern.as_parse_error())
            .collect::<VecDeque<_>>();

        errors.pop_front().map(|front| {
            let mut results = NonEmpty::new(front);
            results.extend(errors);
            results
        })
    }
}

impl From<Inner> for Concerns {
    fn from(inner: Inner) -> Self {
        Concerns(inner)
    }
}
