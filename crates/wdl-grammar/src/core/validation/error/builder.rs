//! A builder for a validation [`Error`](super::Error).

use crate::core::validation;
use crate::core::Code;

/// An error related to building a validation error.
#[derive(Debug)]
pub enum MissingError {
    /// A code was not provided.
    Code,

    /// A message was not provided.
    Message,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Code => write!(f, "missing code"),
            MissingError::Message => write!(f, "missing message"),
        }
    }
}

impl std::error::Error for MissingError {}

/// A [`Result`](std::result::Result) with a [`MissingError`].
pub type Result<T> = std::result::Result<T, MissingError>;

/// A builder for an [`Error`](validation::Error).
#[derive(Debug, Default)]
pub struct Builder {
    /// The code.
    code: Option<Code>,

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
    /// use grammar::core::validation::error::Builder;
    /// use grammar::core::Code;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .message("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn code(mut self, code: Code) -> Self {
        self.code = Some(code);
        self
    }

    /// Sets the message for this [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use grammar::core::validation::error::Builder;
    /// use grammar::core::Code;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
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

    /// Consumes `self` to attempt to build an [`Error`](validation::Error).
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use grammar::core::validation::error::Builder;
    /// use grammar::core::Code;
    /// use grammar::Version;
    ///
    /// let code = Code::try_new(Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .message("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    /// assert_eq!(warning.message(), "Hello, world!");
    /// assert_eq!(warning.to_string(), "[v1::001] Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn try_build(self) -> Result<validation::Error> {
        let code = self.code.map(Ok).unwrap_or(Err(MissingError::Code))?;
        let message = self.message.map(Ok).unwrap_or(Err(MissingError::Message))?;

        Ok(validation::Error { code, message })
    }
}
