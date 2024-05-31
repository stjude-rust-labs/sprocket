//! Incorrect document preamble

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::Tag;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::Location;
use wdl_core::Version;

use crate::v1;

/// Detects improper document preamble.
/// Checks for whitespace, comments, and version declaration placement.
#[derive(Debug)]
pub struct DocumentPreamble;

impl<'a> DocumentPreamble {
    /// Generates a lint warning for an improperly placed version
    /// declaration.
    fn misplaced_version(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .subject("Improperly placed version declaration")
            .body(self.body())
            .push_location(location)
            .fix(
                "Move the version declaration to the first line of the WDL document or \
                 immediately following any preamble comments and exactly one blank line.",
            )
            .try_build()
            .unwrap()
    }

    /// Generates a lint warning for leading whitespace.
    fn leading_whitespace(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .subject("Leading whitespace detected")
            .body(self.body())
            .push_location(location)
            .fix("Remove leading whitespace.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&Pair<'a, v1::Rule>> for DocumentPreamble {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Error, Version::V1, 9).unwrap()
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style])
    }

    fn body(&self) -> &'static str {
        "The document preamble is defined as anything before the version declaration statement and \
         the version declaration statement itself. Only comments and whitespace are permitted \
         before the version declaration. If there are no comments, the version declaration must be \
         the first line of the document. If there are comments, there must be exactly one blank \
         line between the last comment and the version declaration."
    }

    fn check(&self, tree: &Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        // Optionally consume comment nodes, if found, allow exactly one empty line
        // between the comment and the version declaration.
        let mut comment = 0;
        let mut newline = 0;

        // This will never get used. Validation rules require a version statement.
        let mut location: Location = Location::Unplaced;

        for node in tree.clone().into_inner() {
            match node.as_rule() {
                v1::Rule::COMMENT => {
                    comment += 1;
                }
                v1::Rule::WHITESPACE => {
                    if node.as_str() == "\n" && comment > 0 {
                        newline += 1;
                    } else {
                        warnings.push_back(self.leading_whitespace(
                            Location::try_from(node.as_span()).map_err(lint::Error::Location)?,
                        ));
                    }
                }
                v1::Rule::version => {
                    location = Location::try_from(node.as_span()).map_err(lint::Error::Location)?;
                    break;
                }
                _ => {
                    unreachable!(
                        "Only comments and whitespace should precede version declaration."
                    );
                }
            }
        }

        // If comments detected, there should be one empty line between comments and
        // version.
        if (comment > 0 && newline != (comment + 1))
            // If no comments detected, version should be the first line.
            || (comment == 0 && newline != 0)
        {
            warnings.push_back(self.misplaced_version(location))
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
    fn it_catches_missing_newline() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## Preamble comment
version 1.0"#,
        )?
        .next()
        .unwrap();

        let warnings = DocumentPreamble.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Improperly placed version declaration (2:1-2:12)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_leading_newline_with_comment() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"
## Preamble comment
version 1.0"#,
        )?
        .next()
        .unwrap();

        let warnings = DocumentPreamble.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Leading whitespace detected (1:1-2:1)"
        );
        assert_eq!(
            warnings.last().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Improperly placed version declaration (3:1-3:12)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_leading_newline_with_comment_and_proper_newline()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"
## Preamble comment

version 1.0"#,
        )?
        .next()
        .unwrap();

        let warnings = DocumentPreamble.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Leading whitespace detected (1:1-2:1)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_leading_newline() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"
version 1.0"#,
        )?
        .next()
        .unwrap();

        let warnings = DocumentPreamble.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Leading whitespace detected (1:1-2:1)"
        );
        Ok(())
    }

    #[test]
    fn it_handles_correct() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## Preamble comment

version 1.0"#,
        )?
        .next()
        .unwrap();

        assert!(DocumentPreamble.check(&tree)?.is_none());
        Ok(())
    }

    #[test]
    fn it_handles_multiple_comments_correct() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## Preamble comment
## Another comment

version 1.0"#,
        )?
        .next()
        .unwrap();

        assert!(DocumentPreamble.check(&tree)?.is_none());
        Ok(())
    }

    #[test]
    fn it_handles_basic_version_correct() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, r#"version 1.0"#)?
            .next()
            .unwrap();

        assert!(DocumentPreamble.check(&tree)?.is_none());
        Ok(())
    }

    #[test]
    fn it_catches_too_many_newlines() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## Preamble comment


version 1.0"#,
        )?
        .next()
        .unwrap();

        let warnings = DocumentPreamble.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::E009::[Spacing, Style]::Low] Improperly placed version declaration (4:1-4:12)"
        );
        Ok(())
    }

    #[test]
    fn it_handles_normal_comments_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"#normal comments should
# be fine in this Rule

version 1.1"#,
        )?
        .next()
        .unwrap();

        assert!(DocumentPreamble.check(&tree)?.is_none());
        Ok(())
    }
}
