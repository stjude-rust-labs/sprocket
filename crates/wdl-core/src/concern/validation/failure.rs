//! Validation failures.

mod builder;

pub use builder::Builder;
use nonempty::NonEmpty;
use serde::Deserialize;
use serde::Serialize;

use crate::concern::Code;
use crate::display;
use crate::fs::Location;

/// A validation failure.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Failure {
    /// The code.
    code: Code,

    /// The locations.
    locations: NonEmpty<Location>,

    /// The subject.
    subject: String,

    /// The body.
    body: String,

    /// The (optional) text to describe how to fix the issue.
    fix: Option<String>,
}

impl Failure {
    /// Gets the code for this [`Failure`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(error.code().grammar(), &Version::V1);
    /// assert_eq!(error.code().index().get(), 1);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Gets the location for this [`Failure`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(error.locations().first(), &Location::Unplaced);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn locations(&self) -> &NonEmpty<Location> {
        &self.locations
    }

    /// Gets the subject for this [`Failure`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(error.subject(), "Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn subject(&self) -> &str {
        self.subject.as_str()
    }

    /// Gets the body for this [`Failure`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(error.body(), "A body.");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn body(&self) -> &str {
        self.body.as_str()
    }

    /// Gets the fix for this [`Failure`] (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("How to fix the issue.")
    ///     .try_build()?;
    ///
    /// assert_eq!(error.fix().unwrap(), "How to fix the issue.");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn fix(&self) -> Option<&str> {
        self.fix.as_deref()
    }

    /// Displays an error according to the `mode` specified.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fmt::Write as _;
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::validation::failure::Builder;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    /// use wdl_core::display;
    ///
    /// let code = Code::try_new(Kind::Error, Version::V1, 1)?;
    /// let error = Builder::default()
    ///     .code(code)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("Apply ample foobar.")
    ///     .try_build()?;
    ///
    /// let mut result = String::new();
    /// error.display(&mut result, display::Mode::OneLine)?;
    /// assert_eq!(result, String::from("[v1::E001] Hello, world!"));
    ///
    /// result.clear();
    /// error.display(&mut result, display::Mode::Full)?;
    /// assert_eq!(result, String::from("[v1::E001] Hello, world!\n\nA body.\n\nTo fix this error, apply ample foobar."));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn display(&self, f: &mut impl std::fmt::Write, mode: display::Mode) -> std::fmt::Result {
        match mode {
            display::Mode::OneLine => display_one_line(self, f),
            display::Mode::Full => display_full(self, f),
        }
    }
}

/// Displays the error as a single line.
fn display_one_line(error: &Failure, f: &mut impl std::fmt::Write) -> std::fmt::Result {
    write!(f, "[{}] {}", error.code, error.subject)?;

    let locations = error
        .locations
        .iter()
        .flat_map(|location| location.to_string())
        .collect::<Vec<_>>();

    if !locations.is_empty() {
        write!(f, " ({})", locations.join(", "))?;
    }

    Ok(())
}

/// Displays all information about the error.
fn display_full(error: &Failure, f: &mut impl std::fmt::Write) -> std::fmt::Result {
    display_one_line(error, f)?;
    write!(f, "\n\n{}", error.body)?;

    if let Some(fix) = error.fix() {
        write!(f, "\n\nTo fix this error, {}", fix.to_ascii_lowercase())?;
    }

    Ok(())
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.display(f, display::Mode::OneLine)
    }
}

impl std::error::Error for Failure {}

/// A [`Result`](std::error::Error) with a zero or more validation [`Failure`]s.
pub type Result = std::result::Result<(), Vec<Failure>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let code = Code::try_new(crate::concern::code::Kind::Error, crate::Version::V1, 1)?;
        let error = Builder::default()
            .code(code)
            .push_location(Location::Unplaced)
            .subject("Hello, world!")
            .body("A body.")
            .fix("How to fix the issue.")
            .try_build()?;

        assert_eq!(error.to_string(), "[v1::E001] Hello, world!");

        Ok(())
    }
}
