//! Command section.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::dive_one;
use wdl_macros::unwrap_one;

mod contents;

pub use contents::Contents;

/// A command withing a task.
///
/// **Note:** this crate does no inspection of the underlying command. Instead,
/// we make the command available for other tools (e.g.,
/// [shellcheck](https://www.shellcheck.net/)).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Command {
    /// A heredoc style command.
    HereDoc(Contents),

    /// A curly bracket style command.
    Curly(Contents),
}

impl Command {
    /// Gets the inner contents of the command as a reference to a [`str`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::task::command::Contents;
    /// use ast::v1::document::task::Command;
    /// use wdl_ast as ast;
    ///
    /// let contents = "echo 'Hello, world!'".parse::<Contents>().unwrap();
    /// let command = Command::HereDoc(contents);
    /// assert_eq!(command.as_str(), "echo 'Hello, world!'");
    /// ```
    pub fn as_str(&self) -> &str {
        match self {
            Command::HereDoc(contents) => contents.as_str(),
            Command::Curly(contents) => contents.as_str(),
        }
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<Pair<'_, grammar::v1::Rule>> for Command {
    fn from(node: Pair<'_, grammar::v1::Rule>) -> Self {
        check_node!(node, task_command);
        let node = unwrap_one!(node, task_command);

        match node.as_rule() {
            Rule::command_heredoc => {
                let contents_node = dive_one!(node, command_heredoc_contents, command_heredoc);
                // SAFETY: parsing [`Contents`] from a [`&str`] is infallible,
                // so this will always unwrap.
                let contents = contents_node.as_str().parse::<Contents>().unwrap();
                Command::HereDoc(contents)
            }
            Rule::command_curly => {
                let contents_node = dive_one!(node, command_curly_contents, command_curly);
                // SAFETY: parsing [`Contents`] from a [`&str`] is infallible,
                // so this will always unwrap.
                let contents = contents_node.as_str().parse::<Contents>().unwrap();
                Command::Curly(contents)
            }
            _ => {
                unreachable!(
                    "a task command's inner element must be either a heredoc or a curly command"
                )
            }
        }
    }
}
