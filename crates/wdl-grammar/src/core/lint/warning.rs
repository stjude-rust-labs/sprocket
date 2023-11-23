//! Lint warnings.

use crate::core::lint::Group;
use crate::core::lint::Level;
use crate::core::Code;
use crate::core::Location;

mod builder;
pub mod display;

pub use builder::Builder;

/// A lint warning.
#[derive(Clone, Debug)]
pub struct Warning {
    /// The code.
    code: Code,

    /// The lint level.
    level: Level,

    /// The lint group.
    group: Group,

    /// The location.
    location: Location,

    /// The subject.
    subject: String,

    /// The body.
    body: String,

    /// The (optional) text to describe how to fix the issue.
    fix: Option<String>,
}

impl Warning {
    /// Gets the code for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// assert_eq!(warning.code().grammar(), &Version::V1);
    /// assert_eq!(warning.code().index().get(), 1);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Gets the lint level for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// assert_eq!(warning.level(), &Level::High);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn level(&self) -> &Level {
        &self.level
    }

    /// Gets the lint group for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// ```
    pub fn group(&self) -> &Group {
        &self.group
    }

    /// Gets the location for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// assert_eq!(warning.group(), &Group::Style);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Gets the subject for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// ```
    pub fn subject(&self) -> &str {
        self.subject.as_ref()
    }

    /// Gets the body for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// assert_eq!(warning.body(), "A body.");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn body(&self) -> &str {
        self.body.as_str()
    }

    /// Gets the fix text for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::path::PathBuf;
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
    /// assert_eq!(warning.fix(), Some("How to fix the issue."));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn fix(&self) -> Option<&str> {
        self.fix.as_deref()
    }

    /// Displays a warning according to the `mode` specified.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_grammar as grammar;
    ///
    /// use std::fmt::Write as _;
    /// use std::path::PathBuf;
    ///
    /// use grammar::core::lint::warning::Builder;
    /// use grammar::core::lint::warning::display;
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
    ///     .fix("Apply ample foobar.")
    ///     .try_build()?;
    ///
    /// let mut result = String::new();
    /// warning.display(&mut result, display::Mode::OneLine)?;
    /// assert_eq!(result, String::from("[v1::001::Style/High] Hello, world!"));
    ///
    /// result.clear();
    /// warning.display(&mut result, display::Mode::Full)?;
    /// assert_eq!(result, String::from("[v1::001::Style/High] Hello, world!\n\nA body.\n\nTo fix this warning, apply ample foobar."));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn display(&self, f: &mut impl std::fmt::Write, mode: display::Mode) -> std::fmt::Result {
        match mode {
            display::Mode::OneLine => display_one_line(self, f),
            display::Mode::Full => display_full(self, f),
        }
    }
}

/// Displays the warning as a single line.
fn display_one_line(warning: &Warning, f: &mut impl std::fmt::Write) -> std::fmt::Result {
    write!(
        f,
        "[{}::{}/{:?}] {}",
        warning.code, warning.group, warning.level, warning.subject
    )?;

    if let Some(location) = warning.location.to_string() {
        write!(f, " at {}", location)?;
    }

    Ok(())
}

/// Displays all information about the warning.
fn display_full(warning: &Warning, f: &mut impl std::fmt::Write) -> std::fmt::Result {
    display_one_line(warning, f)?;
    write!(f, "\n\n{}", warning.body)?;

    if let Some(fix) = warning.fix() {
        write!(f, "\n\nTo fix this warning, {}", fix.to_ascii_lowercase())?;
    }

    Ok(())
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.display(f, display::Mode::OneLine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() -> Result<(), Box<dyn std::error::Error>> {
        let code = Code::try_new(crate::Version::V1, 1)?;
        let warning = Builder::default()
            .code(code)
            .level(Level::Medium)
            .group(Group::Style)
            .location(Location::File)
            .subject("Hello, world!")
            .body("A body.")
            .fix("How to fix the issue.")
            .try_build()?;

        assert_eq!(warning.to_string(), "[v1::001::Style/Medium] Hello, world!");

        Ok(())
    }
}
