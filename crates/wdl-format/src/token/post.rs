//! Postprocessed tokens.
//!
//! Generally speaking, unless you are working with the internals of code
//! formatting, you're not going to be working with these.

use std::collections::HashMap;
use std::fmt::Display;
use std::rc::Rc;

use wdl_ast::DIRECTIVE_COMMENT_PREFIX;
use wdl_ast::DIRECTIVE_DELIMITER;
use wdl_ast::DOC_COMMENT_PREFIX;
use wdl_ast::Directive;
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

    /// A doc comment block.
    Documentation {
        /// The current indent level.
        num_indents: usize,
        /// The contents of the doc comment block.
        contents: Rc<String>,
    },

    /// A directive comment.
    Directive {
        /// The current indent level.
        num_indents: usize,
        /// The directive.
        directive: Rc<Directive>,
    },
}

impl std::fmt::Debug for PostToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space => write!(f, "<SPACE>"),
            Self::Newline => write!(f, "<NEWLINE>"),
            Self::Indent => write!(f, "<INDENT>"),
            Self::TempIndent(value) => write!(f, "<TEMP_INDENT@{value}>"),
            Self::Literal(value) => write!(f, "<LITERAL@{value}>"),
            Self::Directive { directive, .. } => write!(f, "<DIRECTIVE@{directive:?}>"),
            Self::Documentation { contents, .. } => write!(f, "<DOCUMENTATION@{contents}>"),
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

        fn write_indents(
            f: &mut std::fmt::Formatter<'_>,
            indent: &str,
            num_indents: usize,
        ) -> std::fmt::Result {
            for _ in 0usize..num_indents {
                write!(f, "{indent}")?;
            }
            Ok(())
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.token {
                    PostToken::Space => write!(f, "{SPACE}"),
                    PostToken::Newline => write!(f, "{NEWLINE}"),
                    PostToken::Indent => {
                        write!(f, "{indent}", indent = self.config.indent.string())
                    }
                    PostToken::TempIndent(value) => write!(f, "{value}"),
                    PostToken::Literal(value) => write!(f, "{value}"),
                    PostToken::Documentation {
                        num_indents,
                        contents: markdown,
                    } => {
                        let prefix = DOC_COMMENT_PREFIX;
                        write!(f, "{prefix}")?;
                        let mut lines = markdown.lines().peekable();
                        while let Some(cur) = lines.next() {
                            write!(f, "{cur}")?;
                            if lines.peek().is_some() {
                                write!(f, "{NEWLINE}")?;
                                write_indents(f, &self.config.indent.string(), *num_indents)?;
                                write!(f, "{prefix}")?;
                            }
                        }
                        Ok(())
                    }
                    PostToken::Directive {
                        num_indents,
                        directive,
                    } => {
                        let mut prefix = format!("{} ", DIRECTIVE_COMMENT_PREFIX);
                        match &**directive {
                            Directive::Except(exceptions) => {
                                prefix.push_str("except");
                                prefix.push_str(DIRECTIVE_DELIMITER);
                                prefix.push(' ');
                                let mut rules: Vec<String> = exceptions.iter().cloned().collect();
                                rules.sort();
                                write!(f, "{prefix}")?;
                                if let Some(max) = self.config.max_line_length.get() {
                                    let indent_width = self.config.indent.num() * num_indents;
                                    let start_width = indent_width + prefix.len();
                                    let mut remaining = max.saturating_sub(start_width);
                                    let mut written_to_cur_line = 0usize;
                                    for rule in rules {
                                        let cur_len = rule.len();
                                        if written_to_cur_line == 0 {
                                            write!(f, "{rule}")?;
                                            remaining = remaining.saturating_sub(cur_len);
                                            written_to_cur_line += 1;
                                        } else if remaining.saturating_sub(cur_len + 2) > 0 {
                                            // Current rule fits
                                            write!(f, ", {rule}")?;
                                            remaining = remaining.saturating_sub(cur_len + 2);
                                            written_to_cur_line += 1;
                                        } else {
                                            // Current rule does not fit
                                            write!(f, "{NEWLINE}")?;
                                            write_indents(
                                                f,
                                                &self.config.indent.string(),
                                                *num_indents,
                                            )?;
                                            write!(f, "{prefix}{rule}")?;
                                            written_to_cur_line = 1;
                                            remaining = max.saturating_sub(start_width + cur_len);
                                        }
                                    }
                                    Ok(())
                                } else {
                                    write!(f, "{rules}", rules = rules.join(", "))
                                }
                            }
                        }
                    }
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
    /// As such, newlines are considered zero-width tokens. Similarly, doc
    /// comments and directive comments are considered zero-width as they always
    /// appear on their own lines.
    fn width(&self, config: &crate::Config) -> usize {
        match self {
            Self::Space => SPACE.len(), // 1 character
            Self::Newline => 0,
            Self::Indent => config.indent.num(),
            Self::TempIndent(value) => value.len(),
            Self::Literal(value) => value.len(),
            Self::Directive { .. } => 0,
            Self::Documentation { .. } => 0,
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

/// Gets the corresponding [`SyntaxKind`] that should be line broken in tandem
/// with the provided [`SyntaxKind`].
fn tandem_line_break(kind: SyntaxKind) -> Option<SyntaxKind> {
    match kind {
        SyntaxKind::OpenBrace => Some(SyntaxKind::CloseBrace),
        SyntaxKind::OpenBracket => Some(SyntaxKind::CloseBracket),
        SyntaxKind::OpenParen => Some(SyntaxKind::CloseParen),
        SyntaxKind::OpenHeredoc => Some(SyntaxKind::CloseHeredoc),
        SyntaxKind::PlaceholderOpen => Some(SyntaxKind::CloseBracket),
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

        output
    }

    /// Takes a step of a [`PreToken`] stream and processes the appropriate
    /// [`PostToken`]s.
    fn step(
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
                                self.end_line(stream);
                            }
                            stream.push(PostToken::Literal(value));
                        }
                        Comment::Inline(value) => {
                            assert!(self.position == LinePosition::MiddleOfLine);
                            if let Some(next) = next
                                && next != &PreToken::LineEnd
                            {
                                self.interrupted = true;
                            }
                            self.trim_last_line(stream);
                            for token in INLINE_COMMENT_PRECEDING_TOKENS.iter() {
                                stream.push(token.clone());
                            }
                            stream.push(PostToken::Literal(value));
                        }
                        Comment::Documentation(contents) => {
                            if !matches!(
                                stream.0.last(),
                                Some(&PostToken::Newline) | Some(&PostToken::Indent) | None
                            ) {
                                self.interrupted = true;
                                self.end_line(stream);
                            }
                            stream.push(PostToken::Documentation {
                                num_indents: self.indent_level,
                                contents,
                            });
                        }
                        Comment::Directive(directive) => {
                            if !matches!(
                                stream.0.last(),
                                Some(&PostToken::Newline) | Some(&PostToken::Indent) | None
                            ) {
                                self.interrupted = true;
                                self.end_line(stream);
                            }
                            stream.push(PostToken::Directive {
                                num_indents: self.indent_level,
                                directive,
                            });
                        }
                    }
                    self.position = LinePosition::MiddleOfLine;
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
        if config.max_line_length.get().is_none()
            || post_buffer.max_width(config) <= config.max_line_length.get().unwrap()
        {
            out_stream.extend(post_buffer);
            return;
        }

        // At least one line in the post_buffer is too long.
        // We iterate through the in_stream to find potential line breaks,
        // and then we iterate through the in_stream again to actually insert
        // them in the proper places.

        let max_length = config.max_line_length.get().unwrap();

        let mut potential_line_breaks: HashMap<usize, SyntaxKind> = HashMap::new();
        for (i, token) in in_stream.iter().enumerate() {
            if let PreToken::Literal(_, kind) = token {
                match can_be_line_broken(*kind) {
                    Some(LineBreak::Before) => {
                        potential_line_breaks.insert(i, *kind);
                    }
                    Some(LineBreak::After) => {
                        potential_line_breaks.insert(i + 1, *kind);
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

        let mut break_stack = Vec::new();

        while let Some((i, token)) = pre_buffer.next() {
            let mut cache = None;
            if let Some(break_kind) = potential_line_breaks.get(&i) {
                if let Some(top_of_stack) = break_stack.last()
                    && top_of_stack == break_kind
                {
                    break_stack.pop();
                    self.interrupted = false;
                    self.end_line(&mut post_buffer);
                } else if post_buffer.last_line_width(config) > max_length {
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

            if let Some(cache) = cache
                && post_buffer.last_line_width(config) > max_length
            {
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

                if let Some(also_break_on) =
                    tandem_line_break(*potential_line_breaks.get(&i).unwrap())
                {
                    break_stack.push(also_break_on);
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
