//! Tokens used during formatting.

mod post;
mod pre;

use std::fmt::Display;
use std::rc::Rc;

pub use post::*;
pub use pre::*;

use crate::Config;

/// Tokens that are streamable.
pub trait Token: Eq + PartialEq {
    /// Returns a displayable version of the token.
    fn display<'a>(&'a self, config: &'a Config) -> impl Display + 'a;
}

/// A stream of tokens. Tokens in this case are either [`PreToken`]s or
/// [`PostToken`]s. Note that, unless you are working on formatting
/// specifically, you should never need to work with [`PostToken`]s.
#[derive(Debug, Clone)]
pub struct TokenStream<T: Token>(Vec<T>);

impl<T: Token> Default for TokenStream<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: Token> TokenStream<T> {
    /// Pushes a token into the stream.
    pub fn push(&mut self, token: T) {
        self.0.push(token);
    }

    /// Removes any number of `token`s at the end of the stream.
    pub fn trim_end(&mut self, token: &T) {
        while Some(token) == self.0.last() {
            let _ = self.0.pop();
        }
    }

    /// Removes any number of `token`s at the end of the stream.
    pub fn trim_while<F: Fn(&T) -> bool>(&mut self, predicate: F) {
        while let Some(token) = self.0.last() {
            if !predicate(token) {
                break;
            }

            let _ = self.0.pop();
        }
    }

    /// Returns whether the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the tokens in the stream.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Clears the stream.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Extends the stream with the tokens from another stream.
    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

impl<T: Token> IntoIterator for TokenStream<T> {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = T;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// The kind of comment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Comment {
    /// A directive comment.
    Directive(Rc<wdl_ast::Directive>),
    /// A doc comment block.
    Documentation(Rc<String>),
    /// A comment on its own line.
    Preceding(Rc<String>),
    /// A comment on the same line as the code preceding it.
    Inline(Rc<String>),
}

/// Trivia.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Trivia {
    /// A blank line. This may be ignored by the postprocessor.
    BlankLine,
    /// A comment.
    Comment(Comment),
}

/// The policy for [`Trivia::BlankLine`] line spacing.
///
/// Blank lines before comments and between comments are always permitted.
#[derive(Eq, PartialEq, Default, Debug, Clone, Copy)]
pub enum TriviaBlankLineSpacingPolicy {
    /// Blank lines are allowed before and between comments, but not after.
    ///
    /// e.g. a comment, then a blank line, then code, followed by another blank
    /// line, would have the trailing blank (between the comment and the code)
    /// removed but any blank line before the comment would be preserved.
    RemoveTrailingBlanks,
    /// Blank lines are always allowed.
    #[default]
    Always,
}
