//! A builder for a lint [`Warning`].

use nonempty::NonEmpty;

use crate::concern::lint::Group;
use crate::concern::lint::Level;
use crate::concern::lint::Warning;
use crate::concern::Code;
use crate::file::Location;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A code was not provided to the [`Builder`].
    Code,

    /// A lint level was not provided to the [`Builder`].
    Level,

    /// A lint group was not provided to the [`Builder`].
    Group,

    /// A location was not provided to the [`Builder`].
    Location,

    /// A subject was not provided to the [`Builder`].
    Subject,

    /// A body was not provided to the [`Builder`].
    Body,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Code => write!(f, "code"),
            MissingError::Level => write!(f, "level"),
            MissingError::Group => write!(f, "group"),
            MissingError::Location => write!(f, "location"),
            MissingError::Subject => write!(f, "subject"),
            MissingError::Body => write!(f, "body"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error related to a [`Builder`].
#[derive(Debug)]
pub enum Error {
    /// A required field was missing at build time.
    Missing(MissingError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing(err) => write!(f, "missing value for field: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with a [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// A builder for a [`Warning`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The code.
    code: Option<Code>,

    /// The lint level.
    level: Option<Level>,

    /// The lint group.
    group: Option<Group>,

    /// The locations.
    locations: Option<NonEmpty<Location>>,

    /// The subject.
    subject: Option<String>,

    /// The body.
    body: Option<String>,

    /// The (optional) text to describe how to fix the issue.
    fix: Option<String>,
}

impl Builder {
    /// Sets the code for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn code(mut self, code: Code) -> Self {
        self.code = Some(code);
        self
    }

    /// Sets the lint level for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.level(), &Level::High);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn level(mut self, level: Level) -> Self {
        self.level = Some(level);
        self
    }

    /// Sets the lint group for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.group(), &Group::Style);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn group(mut self, group: Group) -> Self {
        self.group = Some(group);
        self
    }

    /// Sets the location for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     warning.locations().first(),
    ///     &Location::Unplaced
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn push_location(mut self, location: Location) -> Self {
        let locations = match self.locations {
            Some(mut locations) => {
                locations.push(location);
                locations
            }
            None => NonEmpty::new(location),
        };

        self.locations = Some(locations);
        self
    }

    /// Sets the subject for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.subject(), "Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        let subject = subject.into();
        self.subject = Some(subject);
        self
    }

    /// Sets the body for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.body(), "A body.");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn body(mut self, body: impl Into<String>) -> Self {
        let body = body.into();
        self.body = Some(body);
        self
    }

    /// Sets the fix text for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.fix(), Some("How to fix the issue."));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn fix(mut self, fix: impl Into<String>) -> Self {
        let fix = fix.into();
        self.fix = Some(fix);
        self
    }

    /// Consumes `self` to attempt to build a [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::file::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .push_location(Location::Unplaced)
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    /// assert_eq!(warning.level(), &Level::High);
    /// assert_eq!(warning.group(), &Group::Style);
    /// assert_eq!(warning.subject(), "Hello, world!");
    /// assert_eq!(warning.body(), "A body.");
    /// assert_eq!(warning.fix(), Some("How to fix the issue."));
    /// assert_eq!(warning.to_string(), "[v1::W001::Style/High] Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn try_build(self) -> Result<Warning> {
        let code = self
            .code
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Code)))?;

        let level = self
            .level
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Level)))?;

        let group = self
            .group
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Group)))?;

        let locations = self
            .locations
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Location)))?;

        let subject = self
            .subject
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Subject)))?;

        let body = self
            .body
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Body)))?;

        Ok(Warning {
            code,
            level,
            group,
            locations,
            subject,
            body,
            fix: self.fix,
        })
    }
}
