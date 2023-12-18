//! Various lints for undesired whitespace.

use std::collections::VecDeque;
use std::num::NonZeroUsize;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Group;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::Code;
use wdl_core::file::location::Position;
use wdl_core::file::Location;
use wdl_core::str::LinesWithOffsetsExt as _;
use wdl_core::Version;

use crate::v1;

/// Various lints for undesired whitespace.
#[derive(Debug)]
pub struct Whitespace;

impl<'a> Whitespace {
    /// Creates an error corresponding to a line being filled only with blank
    /// spaces.
    fn empty_line(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(self.group())
            .push_location(location)
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
    fn trailing_space(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(self.group())
            .push_location(location)
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
    fn trailing_tab(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(self.group())
            .push_location(location)
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

impl<'a> Rule<&Pair<'a, v1::Rule>> for Whitespace {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 1).unwrap()
    }

    fn group(&self) -> lint::Group {
        Group::Style
    }

    fn check(&self, tree: &Pair<'a, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        for (line_no, start_byte_no, end_byte_no, line) in tree.as_str().lines_with_offsets() {
            if line.is_empty() {
                continue;
            }

            // SAFETY: empty lines will always start at the first column of the
            // line, so the column is hardcoded to one (`1`). As such, a literal
            // `1` will always unwrap.
            let start = Position::new(line_no, NonZeroUsize::try_from(1).unwrap(), start_byte_no);

            // SAFETY: we just ensured above that the line is not empty. As
            // such, at least one character exists on the line, and the
            // [`NonZeroUsize`] will always unwrap.
            let end = Position::new(
                line_no,
                NonZeroUsize::try_from(line.len()).unwrap(),
                end_byte_no,
            );

            let trimmed_line = line.trim();

            if trimmed_line.is_empty() && line != trimmed_line {
                warnings.push_back(self.empty_line(Location::Span { start, end }));
            } else if line.ends_with(' ') {
                warnings.push_back(self.trailing_space(Location::Position(end)));
            } else if line.ends_with('\t') {
                warnings.push_back(self.trailing_tab(Location::Position(end)));
            }
        }

        match warnings.pop_front() {
            Some(front) => {
                let mut results = NonEmpty::new(front);
                results.extend(warnings);
                Ok(Some(results))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;
    use wdl_core::concern::lint::Rule as _;

    use super::*;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    #[test]
    fn it_catches_an_empty_line() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1\n   \n")?
            .next()
            .unwrap();
        let warnings = Whitespace.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W001::Style/Low] line contains only whitespace (2:1-2:3)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_trailing_space() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1 ")?
            .next()
            .unwrap();
        let warnings = Whitespace.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W001::Style/Low] trailing space (1:12)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_trailing_tab() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1\t")?
            .next()
            .unwrap();
        let warnings = Whitespace.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W001::Style/Low] trailing tab (1:12)"
        );

        Ok(())
    }

    #[test]
    fn it_unwraps_a_trailing_space_error() {
        let warning = Whitespace.trailing_space(Location::Position(Position::new(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
            0,
        )));
        assert_eq!(
            warning.to_string(),
            "[v1::W001::Style/Low] trailing space (1:1)"
        )
    }

    #[test]
    fn it_unwraps_a_trailing_tab_error() {
        let warning = Whitespace.trailing_tab(Location::Position(Position::new(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
            0,
        )));
        assert_eq!(
            warning.to_string(),
            "[v1::W001::Style/Low] trailing tab (1:1)"
        )
    }

    #[test]
    fn it_unwraps_an_empty_line_error() {
        let warning = Whitespace.empty_line(Location::Span {
            start: Position::new(
                NonZeroUsize::try_from(1).unwrap(),
                NonZeroUsize::try_from(1).unwrap(),
                0,
            ),
            end: Position::new(
                NonZeroUsize::try_from(1).unwrap(),
                NonZeroUsize::try_from(1).unwrap(),
                0,
            ),
        });
        assert_eq!(
            warning.to_string(),
            "[v1::W001::Style/Low] line contains only whitespace (1:1-1:1)"
        )
    }
}
