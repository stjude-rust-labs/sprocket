//! Various lints for invalid whitespace.

use std::num::NonZeroUsize;

use pest::iterators::Pairs;

use crate::core::lint;
use crate::core::lint::Group;
use crate::core::lint::Rule;
use crate::core::Code;
use crate::core::Location;
use crate::v1;
use crate::Version;

/// Various lints for invalid whitespace.
#[derive(Debug)]
pub struct Whitespace;

impl Whitespace {
    /// Creates an error corresponding to a line being filled only with blank
    /// spaces.
    fn empty_line(&self, line_no: NonZeroUsize) -> lint::Warning
    where
        Self: Rule<v1::Rule>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(lint::Group::Style)
            .location(Location::Line(line_no))
            .subject("line contains only whitespace")
            .body(
                "Blank lines should be completely empty with no characters 
                between newlines.",
            )
            .fix("Remove the whitespace(s).")
            .try_build()
            .unwrap()
    }

    /// Creates an error corresponding to a line with a trailing space.
    fn trailing_space(&self, line_no: NonZeroUsize, col_no: NonZeroUsize) -> lint::Warning
    where
        Self: Rule<v1::Rule>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(lint::Group::Style)
            .location(Location::LineCol { line_no, col_no })
            .subject("trailing space")
            .body(
                "This line contains one or more a trailing space(s).
                
                Blank lines should be completely empty with no characters
                between newlines.",
            )
            .fix("Remove the trailing space(s).")
            .try_build()
            .unwrap()
    }

    /// Creates an error corresponding to a line with a trailing tab.
    fn trailing_tab(&self, line_no: NonZeroUsize, col_no: NonZeroUsize) -> lint::Warning
    where
        Self: Rule<v1::Rule>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(lint::Group::Style)
            .location(Location::LineCol { line_no, col_no })
            .subject("trailing tab")
            .body(
                "This line contains one or more a trailing tab(s).
                
                Blank lines should be completely empty with no characters
                between newlines.",
            )
            .fix("Remove the trailing tab(s).")
            .try_build()
            .unwrap()
    }
}

impl Rule<v1::Rule> for Whitespace {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(Version::V1, 1).unwrap()
    }

    fn group(&self) -> lint::Group {
        Group::Style
    }

    fn check(&self, tree: Pairs<'_, v1::Rule>) -> lint::Result {
        let mut results = Vec::new();

        for (i, line) in tree.as_str().lines().enumerate() {
            if line.is_empty() {
                continue;
            }

            // SAFETY: this will always unwrap because we add one to the current
            // enumeration index. Technically it will not unwrap for usize::MAX
            // - 1, but we don't expect that any WDL document will have that
            //   many lines.
            let line_no = NonZeroUsize::try_from(i + 1).unwrap();

            // SAFETY: we just ensured above that the line is not empty. As
            // such, this will always unwrap.
            let col_no = NonZeroUsize::try_from(line.len()).unwrap();

            let trimmed_line = line.trim();

            if trimmed_line.is_empty() && line != trimmed_line {
                results.push(self.empty_line(line_no));
            } else if line.ends_with(' ') {
                results.push(self.trailing_space(line_no, col_no));
            } else if line.ends_with('\t') {
                results.push(self.trailing_tab(line_no, col_no));
            }
        }

        match results.is_empty() {
            true => Ok(None),
            false => Ok(Some(results)),
        }
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;

    use crate::core::lint::Rule as _;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    use super::*;

    #[test]
    fn it_catches_an_empty_line() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1\n \n")?;
        let warning = Whitespace.check(tree)?.unwrap();

        assert_eq!(warning.len(), 1);
        assert_eq!(
            warning.first().unwrap().to_string(),
            "[v1::001::Style/Low] line contains only whitespace at 2:*"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_trailing_space() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1 ")?;
        let warning = Whitespace.check(tree)?.unwrap();

        assert_eq!(warning.len(), 1);
        assert_eq!(
            warning.first().unwrap().to_string(),
            "[v1::001::Style/Low] trailing space at 1:12"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_trailing_tab() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1\t")?;
        let warning = Whitespace.check(tree)?.unwrap();

        assert_eq!(warning.len(), 1);
        assert_eq!(
            warning.first().unwrap().to_string(),
            "[v1::001::Style/Low] trailing tab at 1:12"
        );

        Ok(())
    }

    #[test]
    fn it_unwraps_a_trailing_space_error() {
        let warning = Whitespace.trailing_space(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
        );
        assert_eq!(
            warning.to_string(),
            "[v1::001::Style/Low] trailing space at 1:1"
        )
    }

    #[test]
    fn it_unwraps_a_trailing_tab_error() {
        let warning = Whitespace.trailing_tab(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
        );
        assert_eq!(
            warning.to_string(),
            "[v1::001::Style/Low] trailing tab at 1:1"
        )
    }

    #[test]
    fn it_unwraps_an_empty_line_error() {
        let warning = Whitespace.empty_line(NonZeroUsize::try_from(1).unwrap());
        assert_eq!(
            warning.to_string(),
            "[v1::001::Style/Low] line contains only whitespace at 1:*"
        )
    }
}
