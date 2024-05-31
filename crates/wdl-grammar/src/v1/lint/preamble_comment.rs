//! Preamble comments are full line comments starting with a double pound sign
//! and must occur before the version declaration

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::location::Position;
use wdl_core::file::Location;
use wdl_core::Version;

use crate::v1;

/// Detects preamble comments declaration
#[derive(Debug)]
pub struct PreambleComment;

impl<'a> PreambleComment {
    /// Creates a warning for preamble comments without double pound sign
    fn missing_double_pound_sign(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .subject("preamble comment(s) without a double pound sign")
            .body(self.body())
            .push_location(location)
            .fix(
                "Add a pound sign at the start of each offending comment OR move any single pound \
                 sign comments to after the version declaration.",
            )
            .try_build()
            .unwrap()
    }

    /// Create a warning for preamble comments with 3 or more pound signs
    fn triple_pound_sign_preamble_comment(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .subject("triple pound sign comments are not permitted before the version declaration")
            .body(self.body())
            .push_location(location)
            .fix("Change the triple pound signs to double pound signs.")
            .try_build()
            .unwrap()
    }

    /// Creates a warning for preamble comments after version declaration
    fn preamble_comment_after_version(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .subject("double pound signs are reserved for preamble comments")
            .body(self.body())
            .push_location(location)
            .fix("Remove a pound sign at the start of the comment.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&Pair<'a, v1::Rule>> for PreambleComment {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 10).unwrap()
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[lint::Tag::Style])
    }

    fn body(&self) -> &'static str {
        "Preamble comments are full line comments before the version declaration and they start \
         with a double pound sign. These comments are reserved for documentation that doesn't fit \
         within any of the WDL-defined documentation elements (such as `meta` and `parameter_meta` \
         sections). They may provide context for a collection of tasks or structs, or they may \
         provide a high-level overview of the workflow. Double-pound-sign comments are not allowed \
         after the version declaration. All comments before the version declaration should start \
         with a double pound sign (or if they are not suitable as preamble comments they should be \
         moved to _after_ the version declaration). Comments beginning with 3 or more pound signs \
         are permitted after the version declaration, as they are not considered preamble \
         comments. Comments beginning with 3 or more pound signs before the version declaration \
         are not permitted."
    }

    fn check(&self, tree: &Pair<'a, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        let mut is_preamble = true;
        let mut start_of_comment_block = None;
        let mut end_of_comment_block = None;

        for node in tree.clone().into_inner().flatten() {
            match node.as_rule() {
                v1::Rule::version => {
                    is_preamble = false;
                    if start_of_comment_block.is_some() {
                        let location = Location::Span {
                            start: Position::try_from(start_of_comment_block.unwrap()).unwrap(),
                            end: Position::try_from(end_of_comment_block.unwrap()).unwrap(),
                        };
                        warnings.push_back(self.missing_double_pound_sign(location));

                        start_of_comment_block = None;
                        end_of_comment_block = None;
                    }
                }
                v1::Rule::COMMENT => {
                    // Catches preamble comment with too many pound signs
                    if is_preamble & node.as_str().trim().starts_with("###") {
                        let location = Location::try_from(node.as_span()).unwrap();
                        warnings.push_back(self.triple_pound_sign_preamble_comment(location));
                    }

                    // Catches missing double pound sign
                    if is_preamble & !node.as_str().trim().starts_with("##") {
                        if start_of_comment_block.is_none() {
                            start_of_comment_block = Some(node.as_span().start_pos());
                        }
                        end_of_comment_block = Some(node.as_span().end_pos());
                    }

                    // Catches preamble comment after version declaration
                    if !is_preamble
                        & node.as_str().trim().starts_with("##")
                        & !node.as_str().trim().starts_with("###")
                    {
                        if start_of_comment_block.is_none() {
                            start_of_comment_block = Some(node.as_span().start_pos());
                        }
                        end_of_comment_block = Some(node.as_span().end_pos());
                    }
                }
                v1::Rule::WHITESPACE | v1::Rule::NEWLINE => {} // Whitespace shouldn't reset the
                // comment block detection
                _ => {
                    if start_of_comment_block.is_some() {
                        let location = Location::Span {
                            start: Position::try_from(start_of_comment_block.unwrap()).unwrap(),
                            end: Position::try_from(end_of_comment_block.unwrap()).unwrap(),
                        };
                        warnings.push_back(self.preamble_comment_after_version(location));

                        start_of_comment_block = None;
                        end_of_comment_block = None;
                    }
                }
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
    fn it_catches_badly_formatted_preamble_comment() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"# a comment
version 1.0
"#,
        )?
        .next()
        .unwrap();

        let warnings = PreambleComment.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W010::[Style]::Low] preamble comment(s) without a double pound sign (1:1-1:12)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_badly_formatted_preamble_comment_block() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::document,
            r#"# a comment
# that continues
# over several lines
version 1.0
"#,
        )?
        .next()
        .unwrap();

        let warnings = PreambleComment.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W010::[Style]::Low] preamble comment(s) without a double pound sign (1:1-3:21)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_triple_pound_sign_preamble_comment() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"### a comment
version 1.0
"#,
        )?
        .next()
        .unwrap();

        let warnings = PreambleComment.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W010::[Style]::Low] triple pound sign comments are not permitted before the \
             version declaration (1:1-1:14)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_preamble_comment_after_version() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## a comment
version 1.0

## a wrong comment
"#,
        )?
        .next()
        .unwrap();

        let warnings = PreambleComment.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W010::[Style]::Low] double pound signs are reserved for preamble comments \
             (4:1-4:19)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_preamble_comment_block_after_version() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"## a comment
version 1.0

## a wrong comment
## that continues
## over several lines
"#,
        )?
        .next()
        .unwrap();

        let warnings = PreambleComment.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W010::[Style]::Low] double pound signs are reserved for preamble comments \
             (4:1-6:22)"
        );
        Ok(())
    }

    #[test]
    fn it_ignores_a_properly_formatted_preamble_comment() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::document,
            r#"## a comment
version 1.0
"#,
        )?
        .next()
        .unwrap();
        let warnings = PreambleComment.check(&tree)?;
        assert!(warnings.is_none());
        Ok(())
    }
}
