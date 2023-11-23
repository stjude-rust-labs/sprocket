//! A builder for a lint [`Warning`].

use crate::core::lint::Group;
use crate::core::lint::Level;
use crate::core::lint::Warning;
use crate::core::Code;
use crate::core::Location;

/// An error related to building a lint warning.
#[derive(Debug)]
pub enum MissingError {
    /// A code was not provided.
    Code,

    /// A lint level was not provided.
    Level,

    /// A lint group was not provided.
    Group,

    /// A location was not provided.
    Location,

    /// A subject was not provided.
    Subject,

    /// A body was not provided.
    Body,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Code => write!(f, "missing code"),
            MissingError::Level => write!(f, "missing level"),
            MissingError::Group => write!(f, "missing group"),
            MissingError::Location => write!(f, "missing location"),
            MissingError::Subject => write!(f, "missing subject"),
            MissingError::Body => write!(f, "missing body"),
        }
    }
}

impl std::error::Error for MissingError {}

/// A [`Result`](std::result::Result) with a [`MissingError`].
pub type Result<T> = std::result::Result<T, MissingError>;

/// A builder for a [`Warning`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The code.
    code: Option<Code>,

    /// The lint level.
    level: Option<Level>,

    /// The lint group.
    group: Option<Group>,

    /// The location.
    location: Option<Location>,

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
    /// use wdl_grammar as grammar;
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .location(Location::File)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     warning.location(),
    ///     &Location::File
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Sets the subject for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// use wdl_grammar as grammar;
    ///
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::core::Location;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .subject("Hello, world!")
    ///     .location(Location::File)
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
    /// assert_eq!(warning.to_string(), "[v1::001::Style/High] Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn try_build(self) -> Result<Warning> {
        let code = self.code.map(Ok).unwrap_or(Err(MissingError::Code))?;
        let level = self.level.map(Ok).unwrap_or(Err(MissingError::Level))?;
        let group = self.group.map(Ok).unwrap_or(Err(MissingError::Group))?;
        let location = self
            .location
            .map(Ok)
            .unwrap_or(Err(MissingError::Location))?;
        let subject = self.subject.map(Ok).unwrap_or(Err(MissingError::Subject))?;
        let body = self.body.map(Ok).unwrap_or(Err(MissingError::Body))?;

        Ok(Warning {
            code,
            level,
            group,
            location,
            subject,
            body,
            fix: self.fix,
        })
    }
}
