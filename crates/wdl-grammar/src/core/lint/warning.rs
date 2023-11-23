//! Lint warnings.

use crate::core::lint::Group;
use crate::core::lint::Level;
use crate::core::Code;

mod builder;

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

    /// The subject.
    subject: String,
}

impl Warning {
    /// Gets the code for this [`Warning`].
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
    ///     .subject("Hello, world!")
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
    ///     .subject("Hello, world!")
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
    ///     .subject("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.group(), &Group::Style);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn group(&self) -> &Group {
        &self.group
    }

    /// Gets the subject for this [`Warning`].
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
    ///     .subject("Hello, world!")
    ///     .try_build()?;
    ///
    /// assert_eq!(warning.subject(), "Hello, world!");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn subject(&self) -> &str {
        self.subject.as_ref()
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}::{}/{:?}] {}",
            self.code, self.group, self.level, self.subject
        )
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
            .subject("Hello, world!")
            .try_build()?;

        assert_eq!(warning.to_string(), "[v1::001::Style/Medium] Hello, world!");

        Ok(())
    }
}
