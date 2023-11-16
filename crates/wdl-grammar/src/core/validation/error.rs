//! Validation errors.

mod builder;

pub use builder::Builder;

use crate::core::Code;

/// A validation error.
#[derive(Clone, Debug)]
pub struct Error {
    /// The code.
    code: Code,

    /// The message.
    message: String,
}

impl Error {
    /// Gets the code for this [`Error`].
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
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Gets the message for this [`Error`].
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
    pub fn message(&self) -> &str {
        self.message.as_ref()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::error::Error) with a zero or more validation [`Error`]s.
pub type Result = std::result::Result<(), Vec<Error>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let code = Code::try_new(crate::Version::V1, 1)?;
        let error = Builder::default()
            .code(code)
            .message("Hello, world!")
            .try_build()?;

        assert_eq!(error.to_string(), "[v1::001] Hello, world!");

        Ok(())
    }
}
