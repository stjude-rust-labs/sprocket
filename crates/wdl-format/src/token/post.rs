//! Postprocessed tokens.
//!
//! Generally speaking, unless you are working with the internals of code
//! formatting, you're not going to be working with these.

use std::fmt::Display;

use wdl_ast::SyntaxKind;

use crate::Comment;
use crate::LineSpacingPolicy;
use crate::NEWLINE;
use crate::PreToken;
use crate::SPACE;
use crate::Token;
use crate::TokenStream;
use crate::Trivia;
use crate::config::Indent;

/// A postprocessed token.
#[derive(Eq, PartialEq)]
pub enum PostToken {
    /// A space.
    Space,

    /// A newline.
    Newline,

    /// One indentation.
    Indent,

    /// A string literal.
    Literal(String),
}

impl std::fmt::Debug for PostToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space => write!(f, "<SPACE>"),
            Self::Newline => write!(f, "<NEWLINE>"),
            Self::Indent => write!(f, "<INDENT>"),
            Self::Literal(value) => write!(f, "<LITERAL> {value}"),
        }
    }
}

impl Token for PostToken {
    /// Returns a displayable version of the token.
    fn display<'a>(&'a self, config: &'a crate::Config) -> impl Display + 'a {
        /// A displayable version of a [`PostToken`].
        struct Display<'a> {
            /// The token to display.
            token: &'a PostToken,
            /// The configuration to use.
            config: &'a crate::Config,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.token {
                    PostToken::Space => write!(f, "{SPACE}"),
                    PostToken::Newline => write!(f, "{NEWLINE}"),
                    PostToken::Indent => {
                        let (c, n) = match self.config.indent() {
                            Indent::Spaces(n) => (' ', n),
                            Indent::Tabs(n) => ('\t', n),
                        };

                        for _ in 0..n.get() {
                            write!(f, "{c}")?;
                        }

                        Ok(())
                    }
                    PostToken::Literal(value) => write!(f, "{value}"),
                }
            }
        }

        Display {
            token: self,
            config,
        }
    }
}

/// Current position in a line.
#[derive(Default, Eq, PartialEq)]
enum LinePosition {
    /// The start of a line.
    #[default]
    StartOfLine,

    /// The middle of a line.
    MiddleOfLine,
}

/// A postprocessor of [tokens](PreToken).
#[derive(Default)]
pub struct Postprocessor {
    /// The current position in the line.
    position: LinePosition,

    /// The current indentation level.
    indent_level: usize,

    /// Whether the current line has been interrupted by trivia.
    interrupted: bool,

    /// Whether blank lines are allowed in the current context.
    line_spacing_policy: LineSpacingPolicy,

    /// Whether temporary indentation is needed.
    temp_indent_needed: bool,

    /// Temporary indentation to add while formatting command blocks.
    temp_indent: String,
}

impl Postprocessor {
    /// Runs the postprocessor.
    pub fn run(&mut self, input: TokenStream<PreToken>) -> TokenStream<PostToken> {
        let mut output = TokenStream::<PostToken>::default();

        let mut stream = input.into_iter().peekable();
        while let Some(token) = stream.next() {
            self.step(token, stream.peek(), &mut output);
        }

        self.trim_whitespace(&mut output);
        output.push(PostToken::Newline);

        output
    }

    /// Takes a step of a [`PreToken`] stream and processes the appropriate
    /// [`PostToken`]s.
    pub fn step(
        &mut self,
        token: PreToken,
        next: Option<&PreToken>,
        stream: &mut TokenStream<PostToken>,
    ) {
        match token {
            PreToken::BlankLine => {
                self.blank_line(stream);
            }
            PreToken::LineEnd => {
                self.interrupted = false;
                self.end_line(stream);
            }
            PreToken::WordEnd => {
                stream.trim_end(&PostToken::Space);

                if self.position == LinePosition::MiddleOfLine {
                    stream.push(PostToken::Space);
                } else {
                    // We're at the start of a line, so we don't need to add a
                    // space.
                }
            }
            PreToken::IndentStart => {
                self.indent_level += 1;
                self.end_line(stream);
            }
            PreToken::IndentEnd => {
                self.indent_level = self.indent_level.saturating_sub(1);
                self.end_line(stream);
            }
            PreToken::LineSpacingPolicy(policy) => {
                self.line_spacing_policy = policy;
            }
            PreToken::Literal(value, kind) => {
                assert!(kind != SyntaxKind::Comment && kind != SyntaxKind::Whitespace);

                // This is special handling for inserting the empty string.
                // We remove any indentation or spaces from the end of the
                // stream and then add the empty string as a literal.
                // Then we set the position to [`LinePosition::MiddleOfLine`]
                // in order to trigger a newline being added before the next
                // token.
                if value.is_empty() {
                    self.trim_last_line(stream);
                    stream.push(PostToken::Literal(value));
                    self.position = LinePosition::MiddleOfLine;
                    return;
                }

                if self.interrupted
                    && matches!(
                        kind,
                        SyntaxKind::OpenBrace
                            | SyntaxKind::OpenBracket
                            | SyntaxKind::OpenParen
                            | SyntaxKind::OpenHeredoc
                    )
                    && stream.0.last() == Some(&PostToken::Indent)
                {
                    stream.0.pop();
                }

                if kind == SyntaxKind::LiteralCommandText {
                    self.temp_indent = value
                        .chars()
                        .take_while(|c| matches!(c, ' ' | '\t'))
                        .collect();
                }

                stream.push(PostToken::Literal(value));
                self.position = LinePosition::MiddleOfLine;
            }
            PreToken::Trivia(trivia) => match trivia {
                Trivia::BlankLine => match self.line_spacing_policy {
                    LineSpacingPolicy::Always => {
                        self.blank_line(stream);
                    }
                    LineSpacingPolicy::BeforeComments => {
                        if matches!(next, Some(&PreToken::Trivia(Trivia::Comment(_)))) {
                            self.blank_line(stream);
                        }
                    }
                },
                Trivia::Comment(comment) => {
                    match comment {
                        Comment::Preceding(value) => {
                            if !matches!(
                                stream.0.last(),
                                Some(&PostToken::Newline) | Some(&PostToken::Indent) | None
                            ) {
                                self.interrupted = true;
                            }
                            self.end_line(stream);
                            stream.push(PostToken::Literal(value));
                            self.position = LinePosition::MiddleOfLine;
                        }
                        Comment::Inline(value) => {
                            assert!(self.position == LinePosition::MiddleOfLine);
                            if let Some(next) = next {
                                if next != &PreToken::LineEnd {
                                    self.interrupted = true;
                                }
                            }
                            self.trim_last_line(stream);
                            stream.push(PostToken::Space);
                            stream.push(PostToken::Space);
                            stream.push(PostToken::Literal(value));
                        }
                    }
                    self.end_line(stream);
                }
            },
            PreToken::TempIndentStart => {
                self.temp_indent_needed = true;
            }
            PreToken::TempIndentEnd => {
                self.temp_indent_needed = false;
            }
        }
    }

    /// Trims any and all whitespace from the end of the stream.
    fn trim_whitespace(&mut self, stream: &mut TokenStream<PostToken>) {
        stream.trim_while(|token| {
            matches!(
                token,
                PostToken::Space | PostToken::Newline | PostToken::Indent
            )
        });
    }

    /// Trims spaces and indents (and not newlines) from the end of the stream.
    fn trim_last_line(&mut self, stream: &mut TokenStream<PostToken>) {
        stream.trim_while(|token| {
            matches!(token, PostToken::Space | PostToken::Indent)
                || token == &PostToken::Literal(self.temp_indent.clone())
        });
    }

    /// Ends the current line without resetting the interrupted flag.
    ///
    /// Removes any trailing spaces or indents and adds a newline only if state
    /// is not [`LinePosition::StartOfLine`]. State is then set to
    /// [`LinePosition::StartOfLine`]. Safe to call multiple times in a row.
    fn end_line(&mut self, stream: &mut TokenStream<PostToken>) {
        self.trim_last_line(stream);
        if self.position != LinePosition::StartOfLine {
            stream.push(PostToken::Newline);
        }
        self.position = LinePosition::StartOfLine;
        self.indent(stream);
    }

    /// Pushes the current indentation level to the stream.
    /// This should only be called when the state is
    /// [`LinePosition::StartOfLine`].
    fn indent(&self, stream: &mut TokenStream<PostToken>) {
        assert!(self.position == LinePosition::StartOfLine);

        let level = if self.interrupted {
            self.indent_level + 1
        } else {
            self.indent_level
        };

        for _ in 0..level {
            stream.push(PostToken::Indent);
        }

        if self.temp_indent_needed {
            stream.push(PostToken::Literal(self.temp_indent.clone()));
        }
    }

    /// Creates a blank line and then indents.
    fn blank_line(&mut self, stream: &mut TokenStream<PostToken>) {
        self.trim_whitespace(stream);
        stream.push(PostToken::Newline);
        stream.push(PostToken::Newline);
        self.position = LinePosition::StartOfLine;
        self.indent(stream);
    }
}
