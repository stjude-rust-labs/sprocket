//! A builder for a lint [`Warning`].

use crate::core::lint::Group;
use crate::core::lint::Level;
use crate::core::lint::Warning;
use crate::core::Code;

/// An error related to building a lint warning.
#[derive(Debug)]
pub enum MissingError {
    /// A code was not provided.
    Code,

    /// A lint level was not provided.
    Level,

    /// A lint group was not provided.
    Group,

    /// A message was not provided.
    Message,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Code => write!(f, "missing code"),
            MissingError::Level => write!(f, "missing level"),
            MissingError::Group => write!(f, "missing group"),
            MissingError::Message => write!(f, "missing message"),
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

    /// The message.
    message: Option<String>,
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
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .message("Hello, world!")
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
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .message("Hello, world!")
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
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::Group;
    /// use grammar::core::lint::Level;
    /// use grammar::core::Code;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .message("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.group(), &Group::Style);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn group(mut self, group: Group) -> Self {
        self.group = Some(group);
        self
    }

    /// Sets the message for this [`Builder`].
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
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .message("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.message(), "Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn message(mut self, message: impl Into<String>) -> Self {
        let message = message.into();
        self.message = Some(message);
        self
    }

    /// Consumes `self` to attempt to build a [`Warning`].
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
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .message("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    /// assert_eq!(warning.level(), &Level::High);
    /// assert_eq!(warning.group(), &Group::Style);
    /// assert_eq!(warning.message(), "Hello, world!");
    /// assert_eq!(warning.to_string(), "[v1::001::Style/High] Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn try_build(self) -> Result<Warning> {
        let code = self.code.map(Ok).unwrap_or(Err(MissingError::Code))?;
        let level = self.level.map(Ok).unwrap_or(Err(MissingError::Level))?;
        let group = self.group.map(Ok).unwrap_or(Err(MissingError::Group))?;
        let message = self.message.map(Ok).unwrap_or(Err(MissingError::Message))?;

        Ok(Warning {
            code,
            level,
            group,
            message,
        })
    }
}
