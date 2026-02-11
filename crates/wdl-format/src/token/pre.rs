//! Tokens emitted during the formatting of particular elements.

use std::collections::HashSet;
use std::rc::Rc;

use wdl_ast::DIRECTIVE_COMMENT_PREFIX;
use wdl_ast::DOC_COMMENT_PREFIX;
use wdl_ast::Directive;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxTokenExt;

use crate::Comment;
use crate::NEWLINE;
use crate::Token;
use crate::TokenStream;
use crate::Trivia;
use crate::TriviaBlankLineSpacingPolicy;

/// A token that can be written by elements.
///
/// These are tokens that are intended to be written directly by elements to a
/// [`TokenStream`](super::TokenStream) consisting of [`PreToken`]s. Note that
/// this will transformed into a [`TokenStream`](super::TokenStream) of
/// [`PostToken`](super::PostToken)s by a
/// [`Postprocessor`](super::Postprocessor) (authors of elements are never
/// expected to write [`PostToken`](super::PostToken)s directly).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreToken {
    /// A non-trivial blank line.
    ///
    /// This will not be ignored by the postprocessor (unlike
    /// [`Trivia::BlankLine`] which is potentially ignored).
    BlankLine,

    /// The end of a line.
    LineEnd,

    /// The end of a word.
    WordEnd,

    /// The start of an indented block.
    IndentStart,

    /// The end of an indented block.
    IndentEnd,

    /// How to handle trivial blank lines from this point onwards.
    LineSpacingPolicy(TriviaBlankLineSpacingPolicy),

    /// Literal text.
    Literal(Rc<String>, SyntaxKind),

    /// Trivia.
    Trivia(Trivia),

    /// A temporary indent start. Used in command section formatting.
    ///
    /// Command sections must account for indentation from both the
    /// WDL context and the embedded Bash context, so this is used to
    /// add additional indentation from the Bash context.
    TempIndentStart,

    /// A temporary indent end. Used in command section formatting.
    ///
    /// See [`PreToken::TempIndentStart`] for more information.
    TempIndentEnd,
}

impl std::fmt::Display for PreToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreToken::BlankLine => write!(f, "<BlankLine>"),
            PreToken::LineEnd => write!(f, "<EndOfLine>"),
            PreToken::WordEnd => write!(f, "<WordEnd>"),
            PreToken::IndentStart => write!(f, "<IndentStart>"),
            PreToken::IndentEnd => write!(f, "<IndentEnd>"),
            PreToken::LineSpacingPolicy(policy) => {
                write!(f, "<LineSpacingPolicy@{policy:?}>")
            }
            PreToken::Literal(value, kind) => {
                write!(f, "<Literal-{kind:?}@{value}>")
            }
            PreToken::Trivia(trivia) => match trivia {
                Trivia::BlankLine => {
                    write!(f, "<OptionalBlankLine>")
                }
                Trivia::Comment(comment) => match comment {
                    Comment::Directive(directive) => {
                        write!(f, "<Comment-Directive@{directive:?}>")
                    }
                    Comment::Documentation(documentation) => {
                        write!(f, "<Comment-Documentation@{documentation}>")
                    }
                    Comment::Preceding(value) => {
                        write!(f, "<Comment-Preceding@{value}>")
                    }
                    Comment::Inline(value) => {
                        write!(f, "<Comment-Inline@{value}>")
                    }
                },
            },
            PreToken::TempIndentStart => write!(f, "<TempIndentStart>"),
            PreToken::TempIndentEnd => write!(f, "<TempIndentEnd>"),
        }
    }
}

impl Token for PreToken {
    /// Returns a displayable version of the token.
    fn display<'a>(&'a self, _config: &'a crate::Config) -> impl std::fmt::Display {
        self
    }
}

impl TokenStream<PreToken> {
    /// Inserts a blank line token to the stream if the stream does not already
    /// end with a blank line. This will replace any [`Trivia::BlankLine`]
    /// tokens with [`PreToken::BlankLine`].
    pub fn blank_line(&mut self) {
        self.trim_while(|t| matches!(t, PreToken::BlankLine | PreToken::Trivia(Trivia::BlankLine)));
        self.0.push(PreToken::BlankLine);
    }

    /// Inserts an end of line token to the stream if the stream does not
    /// already end with an end of line token.
    ///
    /// This will also trim any trailing [`PreToken::WordEnd`] tokens.
    pub fn end_line(&mut self) {
        self.trim_while(|t| matches!(t, PreToken::WordEnd | PreToken::LineEnd));
        self.0.push(PreToken::LineEnd);
    }

    /// Inserts a word end token to the stream if the stream does not already
    /// end with a word end token.
    pub fn end_word(&mut self) {
        self.trim_end(&PreToken::WordEnd);
        self.0.push(PreToken::WordEnd);
    }

    /// Inserts an indent start token to the stream. This will also end the
    /// current line.
    pub fn increment_indent(&mut self) {
        self.end_line();
        self.0.push(PreToken::IndentStart);
    }

    /// Inserts an indent end token to the stream. This will also end the
    /// current line.
    pub fn decrement_indent(&mut self) {
        self.end_line();
        self.0.push(PreToken::IndentEnd);
    }

    /// Inserts a trivial blank lines "always allowed" context change.
    pub fn allow_blank_lines(&mut self) {
        self.0.push(PreToken::LineSpacingPolicy(
            TriviaBlankLineSpacingPolicy::Always,
        ));
    }

    /// Inserts a trivial blank lines "not allowed after comments" context
    /// change.
    pub fn ignore_trailing_blank_lines(&mut self) {
        self.0.push(PreToken::LineSpacingPolicy(
            TriviaBlankLineSpacingPolicy::RemoveTrailingBlanks,
        ));
    }

    /// Inserts any preceding trivia into the stream.
    ///
    /// This will consolidate all doc comments and directive comments which
    /// precede this token.
    ///
    /// # Panics
    ///
    /// This will panic if the provided token is itself trivia, as trivia
    /// cannot have trivia.
    fn push_preceding_trivia(&mut self, token: &wdl_ast::Token) {
        assert!(!token.inner().kind().is_trivia());
        let preceding_trivia = token.inner().preceding_trivia();
        let mut documentation = String::new();
        let mut exceptions = HashSet::new();
        for token in preceding_trivia {
            match token.kind() {
                SyntaxKind::Whitespace => {
                    if !self.0.last().is_some_and(|t| {
                        matches!(t, PreToken::BlankLine | PreToken::Trivia(Trivia::BlankLine))
                    }) {
                        self.0.push(PreToken::Trivia(Trivia::BlankLine));
                    }
                }
                SyntaxKind::Comment => {
                    if let Some(t) = token.text().strip_prefix(DOC_COMMENT_PREFIX) {
                        // do not `trim()` the token as the whitespace may
                        // have syntactical meaning in markdown
                        documentation.push_str(t);
                        documentation.push_str(NEWLINE);
                    } else if let Some(remainder) =
                        token.text().strip_prefix(DIRECTIVE_COMMENT_PREFIX)
                        && let Ok(directive) = remainder.parse::<Directive>()
                    {
                        match directive {
                            Directive::Except(e) => exceptions.extend(e),
                            _ => {
                                todo!("handle other directives")
                            }
                        }
                    } else {
                        let comment = PreToken::Trivia(Trivia::Comment(Comment::Preceding(
                            Rc::new(token.text().trim_end().to_string()),
                        )));
                        self.0.push(comment);
                    }
                }
                _ => unreachable!("unexpected trivia: {:?}", token),
            };
        }
        if !documentation.is_empty() {
            let comment = PreToken::Trivia(Trivia::Comment(Comment::Documentation(Rc::new(
                documentation,
            ))));
            self.0.push(comment);
        }
        if !exceptions.is_empty() {
            let comment = PreToken::Trivia(Trivia::Comment(Comment::Directive(Rc::new(
                Directive::Except(exceptions),
            ))));
            self.0.push(comment);
        }
    }

    /// Inserts any inline trivia into the stream.
    ///
    /// # Panics
    ///
    /// This will panic if the provided token is itself trivia, as trivia
    /// cannot have trivia.
    fn push_inline_trivia(&mut self, token: &wdl_ast::Token) {
        assert!(!token.inner().kind().is_trivia());
        if let Some(token) = token.inner().inline_comment() {
            let inline_comment = PreToken::Trivia(Trivia::Comment(Comment::Inline(Rc::new(
                token.text().trim_end().to_owned(),
            ))));
            self.0.push(inline_comment);
        }
    }

    /// Pushes an AST token into the stream.
    ///
    /// This will also push any preceding or inline trivia into the stream.
    /// Any token may have preceding or inline trivia, unless that token is
    /// itself trivia (i.e. trivia cannot have trivia).
    ///
    /// # Panics
    ///
    /// This will panic if the provided token is trivia.
    pub fn push_ast_token(&mut self, token: &wdl_ast::Token) {
        self.push_preceding_trivia(token);
        self.0.push(PreToken::Literal(
            Rc::new(token.inner().text().to_owned()),
            token.inner().kind(),
        ));
        self.push_inline_trivia(token);
    }

    /// Pushes a literal string into the stream in place of an AST token.
    ///
    /// This will insert any trivia that would have been inserted with the AST
    /// token.
    ///
    /// # Panics
    ///
    /// This will panic if the provided token is trivia.
    pub fn push_literal_in_place_of_token(&mut self, token: &wdl_ast::Token, replacement: String) {
        self.push_preceding_trivia(token);
        self.0.push(PreToken::Literal(
            Rc::new(replacement),
            token.inner().kind(),
        ));
        self.push_inline_trivia(token);
    }

    /// Pushes a literal string into the stream.
    ///
    /// This will not insert any trivia.
    pub fn push_literal(&mut self, value: String, kind: SyntaxKind) {
        self.0.push(PreToken::Literal(Rc::new(value), kind));
    }
}
