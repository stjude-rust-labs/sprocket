//! Replace curly command blocks with heredoc command blocks.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Group;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::Code;
use wdl_core::fs::Location;
use wdl_core::Version;

use crate::v1;

/// Replace curly command blocks with heredoc command blocks.
///
/// Curly command blocks are no longer considered idiomatic WDL
/// ([link](https://github.com/openwdl/wdl/blob/main/versions/1.1/SPEC.md#command-section)).
/// Idiomatic WDL code uses heredoc command blocks instead.
#[derive(Debug)]
pub struct NoCurlyCommands;

impl<'a> NoCurlyCommands {
    /// Creates an error corresponding to a line with a trailing tab.
    fn no_curly_commands(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Medium)
            .group(self.group())
            .push_location(location)
            .subject("curly command found")
            .body(
                "Command blocks using curly braces (`{}`) are considered less
                idiomatic than heredoc commands.",
            )
            .fix("Replace the curly command block with a heredoc command block.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a Pair<'a, v1::Rule>> for NoCurlyCommands {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 2).unwrap()
    }

    fn group(&self) -> lint::Group {
        Group::Style
    }

    fn check(&self, tree: &'a Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            if node.as_rule() == v1::Rule::command_curly {
                let location = Location::try_from(node.as_span()).map_err(lint::Error::Location)?;
                warnings.push_back(self.no_curly_commands(location));
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
    use std::num::NonZeroUsize;

    use pest::Parser as _;
    use wdl_core::concern::lint::Rule as _;
    use wdl_core::fs::location::Position;

    use super::*;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    #[test]
    fn it_catches_a_curly_command() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::task,
            "task hello {
    command {}
}",
        )?
        .next()
        .unwrap();
        let warnings = NoCurlyCommands.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W002::Style/Medium] curly command found (2:5-2:15)"
        );

        Ok(())
    }

    #[test]
    fn it_does_not_catch_a_heredoc_command() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::command_heredoc, "command <<<>>>")?
            .next()
            .unwrap();
        assert!(NoCurlyCommands.check(&tree)?.is_none());

        Ok(())
    }

    #[test]
    fn it_unwraps_a_no_curly_commands_error() {
        let location = Location::Position(Position::new(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
            0,
        ));

        let warnings = NoCurlyCommands.no_curly_commands(location);
        assert_eq!(
            warnings.to_string(),
            "[v1::W002::Style/Medium] curly command found (1:1)"
        )
    }
}
