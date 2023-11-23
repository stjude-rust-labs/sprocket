//! Replace curly command blocks with heredoc command blocks.

use std::num::NonZeroUsize;

use pest::iterators::Pairs;

use crate::core::lint;
use crate::core::lint::Group;
use crate::core::lint::Rule;
use crate::core::Code;
use crate::core::Location;
use crate::v1;
use crate::Version;

/// Replace curly command blocks with heredoc command blocks.
///
/// Curly command blocks are no longer considered idiomatic WDL
/// ([link](https://github.com/openwdl/wdl/blob/main/versions/1.1/SPEC.md#command-section)).
/// Idiomatic WDL code uses heredoc command blocks instead.
#[derive(Debug)]
pub struct NoCurlyCommands;

impl NoCurlyCommands {
    /// Creates an error corresponding to a line with a trailing tab.
    fn no_curly_commands(&self, line_no: NonZeroUsize, col_no: NonZeroUsize) -> lint::Warning
    where
        Self: Rule<v1::Rule>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Medium)
            .group(lint::Group::Pedantic)
            .location(Location::LineCol { line_no, col_no })
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

impl Rule<v1::Rule> for NoCurlyCommands {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(Version::V1, 2).unwrap()
    }

    fn group(&self) -> lint::Group {
        Group::Style
    }

    fn check(&self, tree: Pairs<'_, v1::Rule>) -> lint::Result {
        let mut results = Vec::new();

        for node in tree.flatten() {
            if node.as_rule() == v1::Rule::command_curly {
                let (line, col) = node.line_col();
                results.push(self.no_curly_commands(
                    NonZeroUsize::try_from(line)?,
                    NonZeroUsize::try_from(col)?,
                ));
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
    fn it_catches_a_curly_command() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::command_curly, "command {}")?;
        let warning = NoCurlyCommands.check(tree)?.unwrap();

        assert_eq!(warning.len(), 1);
        assert_eq!(
            warning.first().unwrap().to_string(),
            "[v1::002::Pedantic/Medium] curly command found at 1:1"
        );

        Ok(())
    }

    #[test]
    fn it_does_not_catch_a_heredoc_command() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::command_heredoc, "command <<<>>>")?;
        assert!(NoCurlyCommands.check(tree)?.is_none());

        Ok(())
    }

    #[test]
    fn it_unwraps_a_no_curly_commands_error() {
        let warning = NoCurlyCommands.no_curly_commands(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
        );
        assert_eq!(
            warning.to_string(),
            "[v1::002::Pedantic/Medium] curly command found at 1:1"
        )
    }
}
