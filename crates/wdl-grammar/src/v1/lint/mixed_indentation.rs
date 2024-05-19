//! Mixed indentation within commands.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::Location;
use wdl_core::str::whitespace;
use wdl_core::str::whitespace::Whitespace;
use wdl_core::Version;

use crate::v1;

/// Detects mixed indentation within command contents.
#[derive(Debug)]
pub struct MixedIndentation;

impl<'a> MixedIndentation {
    /// Generates a validation error for mixed indentation characters within a
    /// command.
    fn mixed_indentation_characters(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::High)
            .tags(self.tags())
            .subject("mixed indentation characters")
            .body(
                "Mixed indentation characters were found within a command. This causes leading \
                 whitespace stripping to be skipped.",
            )
            .push_location(location)
            .fix("Use the same whitespace character within the command.")
            .try_build()
            .unwrap()
    }
}

impl Rule<&Pair<'_, v1::Rule>> for MixedIndentation {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 4).unwrap()
    }

    fn tags(&self) -> lint::TagSet {
        TagSet::new(&[lint::Tag::Style, lint::Tag::Spacing, lint::Tag::Clarity])
    }

    fn check(&self, tree: &Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            match node.as_rule() {
                crate::v1::Rule::command_heredoc_contents
                | crate::v1::Rule::command_curly_contents => {
                    if let Err(whitespace::Error::MixedIndentationCharacters) =
                        Whitespace::get_indent(&node.as_str().lines().collect::<Vec<_>>())
                    {
                        let location =
                            Location::try_from(node.as_span()).map_err(lint::Error::Location)?;
                        warnings.push_back(self.mixed_indentation_characters(location));
                    }
                }
                _ => {}
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
    fn it_catches_mixed_indentation() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::command_heredoc,
            "command <<<
        \techo 'hello'
        echo 'world'
>>>",
        )?
        .next()
        .unwrap();
        let errors = MixedIndentation.check(&tree).unwrap().unwrap();

        assert_eq!(
            errors.first().to_string(),
            String::from(
                "[v1::W004::[Spacing, Style, Clarity]::High] mixed indentation characters \
                 (1:12-4:1)"
            )
        );

        Ok(())
    }

    #[test]
    fn it_ignores_a_command_correctly_indented_with_spaces()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::command_heredoc,
            "command <<<
        echo 'hello'
        echo 'world'
>>>",
        )?
        .next()
        .unwrap();
        assert!(MixedIndentation.check(&tree)?.is_none());

        Ok(())
    }

    #[test]
    fn it_ignores_a_command_correctly_indented_with_tabs() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::command_heredoc,
            "command <<<
\t\t\techo 'hello'
\t\t\techo 'world'
>>>",
        )?
        .next()
        .unwrap();
        assert!(MixedIndentation.check(&tree)?.is_none());

        Ok(())
    }
}
