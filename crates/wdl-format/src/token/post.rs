//! Postprocessed tokens.
//!
//! Generally speaking, unless you are working with the internals of code
//! formatting, you're not going to be working with these.

use std::collections::HashSet;
use std::fmt::Display;
use std::rc::Rc;

use wdl_ast::SyntaxKind;

use crate::Comment;
use crate::Config;
use crate::NEWLINE;
use crate::PreToken;
use crate::SPACE;
use crate::Token;
use crate::TokenStream;
use crate::Trivia;
use crate::TriviaBlankLineSpacingPolicy;

/// [`PostToken`]s that precede an inline comment.
const INLINE_COMMENT_PRECEDING_TOKENS: [PostToken; 2] = [PostToken::Space, PostToken::Space];

/// A postprocessed token.
#[derive(Clone, Eq, PartialEq)]
pub enum PostToken {
    /// A space.
    Space,

    /// A newline.
    Newline,

    /// One indentation.
    Indent,

    /// A temporary indent.
    ///
    /// This is added after a [`PostToken::Indent`] during the formatting of
    /// command sections.
    TempIndent(Rc<String>),

    /// A string literal.
    Literal(Rc<String>),
}

impl std::fmt::Debug for PostToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space => write!(f, "<SPACE>"),
            Self::Newline => write!(f, "<NEWLINE>"),
            Self::Indent => write!(f, "<INDENT>"),
            Self::TempIndent(value) => write!(f, "<TEMP_INDENT@{value}>"),
            Self::Literal(value) => write!(f, "<LITERAL@{value}>"),
        }
    }
}

impl Token for PostToken {
    /// Returns a displayable version of the token.
    fn display<'a>(&'a self, config: &'a Config) -> impl Display + 'a {
        /// A displayable version of a [`PostToken`].
        struct Display<'a> {
            /// The token to display.
            token: &'a PostToken,
            /// The configuration to use.
            config: &'a Config,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.token {
                    PostToken::Space => write!(f, "{SPACE}"),
                    PostToken::Newline => write!(f, "{NEWLINE}"),
                    PostToken::Indent => {
                        write!(f, "{indent}", indent = self.config.indent().string())
                    }
                    PostToken::TempIndent(value) => write!(f, "{value}"),
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

impl PostToken {
    /// Gets the width of the [`PostToken`].
    ///
    /// This is used to determine how much space the token takes up _within a
    /// single line_ for the purposes of respecting the maximum line length.
    /// As such, newlines are considered zero-width tokens.
    fn width(&self, config: &crate::Config) -> usize {
        match self {
            Self::Space => SPACE.len(), // 1 character
            Self::Newline => 0,
            Self::Indent => config.indent().num(),
            Self::TempIndent(value) => value.len(),
            Self::Literal(value) => value.len(),
        }
    }
}

impl TokenStream<PostToken> {
    /// Gets the maximum width of the [`TokenStream`].
    ///
    /// This is suitable to call if the stream represents multiple lines.
    fn max_width(&self, config: &Config) -> usize {
        let mut max: usize = 0;
        let mut cur_width: usize = 0;
        for token in self.iter() {
            cur_width += token.width(config);
            if token == &PostToken::Newline {
                max = max.max(cur_width);
                cur_width = 0;
            }
        }
        max.max(cur_width)
    }

    /// Gets the width of the last line of the [`TokenStream`].
    fn last_line_width(&self, config: &Config) -> usize {
        let mut width = 0;
        for token in self.iter().rev() {
            if token == &PostToken::Newline {
                break;
            }
            width += token.width(config);
        }
        width
    }
}

/// A line break.
enum LineBreak {
    /// A line break that can be inserted before a token.
    Before,
    /// A line break that can be inserted after a token.
    After,
}

/// Returns whether a token can be line broken.
fn can_be_line_broken(kind: SyntaxKind) -> Option<LineBreak> {
    match kind {
        SyntaxKind::CloseBrace
        | SyntaxKind::CloseBracket
        | SyntaxKind::CloseParen
        | SyntaxKind::CloseHeredoc
        | SyntaxKind::Assignment
        | SyntaxKind::Plus
        | SyntaxKind::Minus
        | SyntaxKind::Asterisk
        | SyntaxKind::Slash
        | SyntaxKind::Percent
        | SyntaxKind::Exponentiation
        | SyntaxKind::Equal
        | SyntaxKind::NotEqual
        | SyntaxKind::Less
        | SyntaxKind::LessEqual
        | SyntaxKind::Greater
        | SyntaxKind::GreaterEqual
        | SyntaxKind::LogicalAnd
        | SyntaxKind::LogicalOr
        | SyntaxKind::AfterKeyword
        | SyntaxKind::AsKeyword
        | SyntaxKind::IfKeyword
        | SyntaxKind::ElseKeyword
        | SyntaxKind::ThenKeyword => Some(LineBreak::Before),
        SyntaxKind::OpenBrace
        | SyntaxKind::OpenBracket
        | SyntaxKind::OpenParen
        | SyntaxKind::OpenHeredoc
        | SyntaxKind::Colon
        | SyntaxKind::PlaceholderOpen
        | SyntaxKind::Comma => Some(LineBreak::After),
        _ => None,
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

    /// The current trivial blank line spacing policy.
    line_spacing_policy: TriviaBlankLineSpacingPolicy,

    /// Whether temporary indentation is needed.
    temp_indent_needed: bool,

    /// Temporary indentation to add.
    temp_indent: Rc<String>,
}

impl Postprocessor {
    /// Runs the postprocessor.
    pub fn run(&mut self, input: TokenStream<PreToken>, config: &Config) -> TokenStream<PostToken> {
        let mut output = TokenStream::<PostToken>::default();
        let mut buffer = TokenStream::<PreToken>::default();

        for token in input {
            match token {
                PreToken::LineEnd => {
                    self.flush(&buffer, &mut output, config);
                    self.trim_whitespace(&mut output);
                    output.push(PostToken::Newline);

                    buffer.clear();
                    self.interrupted = false;
                    self.position = LinePosition::StartOfLine;
                }
                _ => {
                    buffer.push(token);
                }
            }
        }

        // TODO: bug where trailing trivia is not processed
        // https://github.com/stjude-rust-labs/wdl/issues/497

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
        if stream.is_empty() {
            self.interrupted = false;
            self.position = LinePosition::StartOfLine;
            self.indent(stream);
        }
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
                assert!(!kind.is_trivia());

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
                    && matches!(
                        stream.0.last(),
                        Some(&PostToken::Indent) | Some(&PostToken::TempIndent(_))
                    )
                {
                    stream.0.pop();
                }

                if kind == SyntaxKind::LiteralCommandText {
                    self.temp_indent = Rc::new(
                        value
                            .chars()
                            .take_while(|c| matches!(c.to_string().as_str(), SPACE | crate::TAB))
                            .collect(),
                    );
                }

                stream.push(PostToken::Literal(value));
                self.position = LinePosition::MiddleOfLine;
            }
            PreToken::Trivia(trivia) => match trivia {
                Trivia::BlankLine => match self.line_spacing_policy {
                    TriviaBlankLineSpacingPolicy::Always => {
                        self.blank_line(stream);
                    }
                    TriviaBlankLineSpacingPolicy::RemoveTrailingBlanks => {
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
                                Some(&PostToken::Newline)
                                    | Some(&PostToken::Indent)
                                    | Some(&PostToken::TempIndent(_))
                                    | None
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
                            for token in INLINE_COMMENT_PRECEDING_TOKENS.iter() {
                                stream.push(token.clone());
                            }
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

    /// Flushes the `in_stream` buffer to the `out_stream`.
    fn flush(
        &mut self,
        in_stream: &TokenStream<PreToken>,
        out_stream: &mut TokenStream<PostToken>,
        config: &Config,
    ) {
        assert!(!self.interrupted);
        assert!(self.position == LinePosition::StartOfLine);
        let mut post_buffer = TokenStream::<PostToken>::default();
        let mut pre_buffer = in_stream.iter().peekable();
        let starting_indent = self.indent_level;
        while let Some(token) = pre_buffer.next() {
            let next = pre_buffer.peek().copied();
            self.step(token.clone(), next, &mut post_buffer);
        }

        // If all lines are short enough, we can just add the post_buffer to the
        // out_stream and be done.
        if config.max_line_length().is_none()
            || post_buffer.max_width(config) <= config.max_line_length().unwrap()
        {
            out_stream.extend(post_buffer);
            return;
        }

        // At least one line in the post_buffer is too long.
        // We iterate through the in_stream to find potential line breaks,
        // and then we iterate through the in_stream again to actually insert
        // them in the proper places.

        let max_length = config.max_line_length().unwrap();

        let mut potential_line_breaks: HashSet<usize> = HashSet::new();
        for (i, token) in in_stream.iter().enumerate() {
            if let PreToken::Literal(_, kind) = token {
                match can_be_line_broken(*kind) {
                    Some(LineBreak::Before) => {
                        potential_line_breaks.insert(i);
                    }
                    Some(LineBreak::After) => {
                        potential_line_breaks.insert(i + 1);
                    }
                    None => {}
                }
            }
        }

        if potential_line_breaks.is_empty() {
            // There are no potential line breaks, so we can't do anything.
            out_stream.extend(post_buffer);
            return;
        }

        // Set up the buffers for the second pass.
        post_buffer.clear();
        let mut pre_buffer = in_stream.iter().enumerate().peekable();

        // Reset the indent level.
        self.indent_level = starting_indent;

        while let Some((i, token)) = pre_buffer.next() {
            let mut cache = None;
            if potential_line_breaks.contains(&i) {
                if post_buffer.last_line_width(config) > max_length {
                    // The line is already too long, and taking the next step
                    // can only make it worse. Insert a line break here.
                    self.interrupted = true;
                    self.end_line(&mut post_buffer);
                } else {
                    // The line is not too long yet, but it might be after the
                    // next step. Cache the current state so we can revert to it
                    // if necessary.
                    cache = Some(post_buffer.clone());
                }
            }
            self.step(
                token.clone(),
                pre_buffer.peek().map(|(_, v)| &**v),
                &mut post_buffer,
            );

            if let Some(cache) = cache {
                if post_buffer.last_line_width(config) > max_length {
                    // The line is too long after the next step. Revert to the
                    // cached state and insert a line break.
                    post_buffer = cache;
                    self.interrupted = true;
                    self.end_line(&mut post_buffer);
                    self.step(
                        token.clone(),
                        pre_buffer.peek().map(|(_, v)| &**v),
                        &mut post_buffer,
                    );
                }
            }
        }

        out_stream.extend(post_buffer);
    }

    /// Trims any and all whitespace from the end of the stream.
    fn trim_whitespace(&self, stream: &mut TokenStream<PostToken>) {
        stream.trim_while(|token| {
            matches!(
                token,
                PostToken::Space
                    | PostToken::Newline
                    | PostToken::Indent
                    | PostToken::TempIndent(_)
            )
        });
    }

    /// Trims spaces and indents (and not newlines) from the end of the stream.
    fn trim_last_line(&mut self, stream: &mut TokenStream<PostToken>) {
        stream.trim_while(|token| {
            matches!(
                token,
                PostToken::Space | PostToken::Indent | PostToken::TempIndent(_)
            )
        });
    }

    /// Ends the current line without resetting the interrupted flag.
    ///
    /// Removes any trailing spaces or indents and adds a newline only if state
    /// is not [`LinePosition::StartOfLine`]. State is then set to
    /// [`LinePosition::StartOfLine`]. Finally, indentation is added. Safe to
    /// call multiple times in a row.
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
    /// [`LinePosition::StartOfLine`]. This does not change the state.
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
            stream.push(PostToken::TempIndent(self.temp_indent.clone()));
        }
    }

    /// Creates a blank line and then indents.
    fn blank_line(&mut self, stream: &mut TokenStream<PostToken>) {
        self.trim_whitespace(stream);
        if !stream.is_empty() {
            stream.push(PostToken::Newline);
        }
        stream.push(PostToken::Newline);
        self.position = LinePosition::StartOfLine;
        self.indent(stream);
    }
}
