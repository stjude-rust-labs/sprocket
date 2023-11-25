//! Lint warnings.

use crate::concern::lint::Group;
use crate::concern::lint::Level;
use crate::concern::Code;
use crate::display;
use crate::fs::Location;

mod builder;

pub use builder::Builder;
use nonempty::NonEmpty;
use serde::Deserialize;
use serde::Serialize;

/// A lint warning.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Warning {
    /// The code.
    code: Code,

    /// The lint level.
    level: Level,

    /// The lint group.
    group: Group,

    /// The locations.
    locations: NonEmpty<Location>,

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
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// ```
    pub fn group(&self) -> &Group {
        &self.group
    }

    /// Gets the locations for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// assert_eq!(warning.locations().first(), &Location::Unplaced);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn locations(&self) -> &NonEmpty<Location> {
        &self.locations
    }

    /// Gets the subject for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// ```
    pub fn subject(&self) -> &str {
        self.subject.as_ref()
    }

    /// Gets the body for this [`Warning`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::fs::Location;
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
    /// assert_eq!(warning.fix(), Some("How to fix the issue."));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn fix(&self) -> Option<&str> {
        self.fix.as_deref()
    }

    /// Displays a [`Warning`] according to the `mode` specified.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fmt::Write as _;
    /// use std::path::PathBuf;
    ///
    /// use wdl_core::concern::lint::warning::Builder;
    /// use wdl_core::concern::lint::Group;
    /// use wdl_core::concern::lint::Level;
    /// use wdl_core::concern::Code;
    /// use wdl_core::concern::code::Kind;
    /// use wdl_core::fs::Location;
    /// use wdl_core::Version;
    /// use wdl_core::display;
    ///
    /// let code = Code::try_new(Kind::Warning, Version::V1, 1)?;
    /// let warning = Builder::default()
    ///     .code(code)
    ///     .level(Level::High)
    ///     .group(Group::Style)
    ///     .push_location(Location::Unplaced)
    ///     .subject("Hello, world!")
    ///     .body("A body.")
    ///     .fix("Apply ample foobar.")
    ///     .try_build()?;
    ///
    /// let mut result = String::new();
    /// warning.display(&mut result, display::Mode::OneLine)?;
    /// assert_eq!(result, String::from("[v1::W001::Style/High] Hello, world!"));
    ///
    /// result.clear();
    /// warning.display(&mut result, display::Mode::Full)?;
    /// assert_eq!(result, String::from("[v1::W001::Style/High] Hello, world!\n\nA body.\n\nTo fix this warning, apply ample foobar."));
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

    let locations = warning
        .locations
        .iter()
        .flat_map(|location| location.to_string())
        .collect::<Vec<_>>();

    if !locations.is_empty() {
        write!(f, " ({})", locations.join(", "))?;
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
        let code = Code::try_new(crate::concern::code::Kind::Warning, crate::Version::V1, 1)?;
        let warning = Builder::default()
            .code(code)
            .level(Level::Medium)
            .group(Group::Style)
            .push_location(Location::Unplaced)
            .subject("Hello, world!")
            .body("A body.")
            .fix("How to fix the issue.")
            .try_build()?;

        assert_eq!(
            warning.to_string(),
            "[v1::W001::Style/Medium] Hello, world!"
        );

        Ok(())
    }
}
