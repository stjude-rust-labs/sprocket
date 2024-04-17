//! WDL files must end with a newline.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Group;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::Code;
use wdl_core::file::Location;
use wdl_core::Version;

use crate::v1;

/// Detects missing newline at the EOF
#[derive(Debug)]
pub struct NewlineEOF;

impl<'a> NewlineEOF {
    /// Creates a warning for a file not ending with a newline
    fn missing_newline_at_eof(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(self.group())
            .push_location(location)
            .subject("missing newline at the end of the file")
            .body("There should always be a newline at the end of a WDL file.")
            .fix("Add a newline at the end of the file.")
            .try_build()
            .unwrap()
    }

    /// Creates a warning for a file ending with more than one newline
    fn no_empty_line_at_eof(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .group(self.group())
            .push_location(location)
            .subject("multiple empty lines at the end of file")
            .body("There should only be one newline at the end of a WDL file.")
            .fix("Remove all but one empty line at the end of the file.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a Pair<'a, v1::Rule>> for NewlineEOF {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 7).unwrap()
    }

    fn group(&self) -> Group {
        Group::Spacing
    }

    fn check(&self, tree: &'a Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings: VecDeque<_> = VecDeque::new();

        let mut iter = tree.clone().into_inner().rev();

        if let (Some(node_eof), Some(node1), Some(node2)) = (iter.next(), iter.next(), iter.next())
        {
            if node1.as_str() != "\n" {
                let location =
                    Location::try_from(node_eof.as_span()).map_err(lint::Error::Location)?;
                warnings.push_back(self.missing_newline_at_eof(location))
            }

            if node1.as_str() == "\n" && node2.as_str() == "\n" {
                let location =
                    Location::try_from(node1.as_span()).map_err(lint::Error::Location)?;
                warnings.push_back(self.no_empty_line_at_eof(location))
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
    fn it_catches_no_trailing_newline() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0
workflow test {}"#,
        )?
        .next()
        .unwrap();

        let warnings = NewlineEOF.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W007::Spacing/Low] missing newline at the end of the file (2:17-2:17)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_an_empty_newline_eof() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0
workflow test {}

"#,
        )?
        .next()
        .unwrap();

        let warnings = NewlineEOF.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W007::Spacing/Low] multiple empty lines at the end of file (3:1-4:1)"
        );
        Ok(())
    }

    #[test]
    fn it_ignores_a_correctly_formatted_eof() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0
workflow test {}
"#,
        )?
        .next()
        .unwrap();

        assert!(NewlineEOF.check(&tree)?.is_none());
        Ok(())
    }
}
