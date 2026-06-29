//! Module for the parser implementation.
//!
//! The parser consumes a token stream from a lexer and produces
//! a list of parser events that can be used to construct a CST.
//!
//! The design of this is very much based on `rust-analyzer`.

use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use indexmap::IndexSet;
use logos::Logos;

use super::Diagnostic;
use super::Span;
use super::SupportedVersion;
use super::lexer::Lexer;
use super::lexer::LexerResult;
use super::lexer::TokenSet;
use super::tree::SyntaxKind;

/// Represents an event produced by the parser.
///
/// The parser produces a stream of events that can be used to construct
/// a CST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A new node has started.
    NodeStarted {
        /// The kind of the node.
        kind: SyntaxKind,
        /// For left-recursive syntactic constructs, the parser produces
        /// a child node before it sees a parent. `forward_parent`
        /// saves the position of current event's parent.
        forward_parent: Option<usize>,
    },

    /// A node has finished.
    NodeFinished,

    /// A token was encountered.
    Token {
        /// The syntax kind of the token.
        kind: SyntaxKind,
        /// The source span of the token.
        span: Span,
    },
}

impl Event {
    /// Gets an start node event for an abandoned node.
    pub fn abandoned() -> Self {
        Self::NodeStarted {
            kind: SyntaxKind::Abandoned,
            forward_parent: None,
        }
    }
}

/// Utility type for displaying "expected" items in a parser expectation
/// diagnostic.
struct Expected<'a> {
    /// The set of expected items.
    items: &'a [&'a str],
}

impl<'a> Expected<'a> {
    /// Constructs a new `Expected`.
    fn new(items: &'a [&'a str]) -> Self {
        Self { items }
    }
}

impl fmt::Display for Expected<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.items.len();
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                if count == 2 {
                    write!(f, " or ")?;
                } else if i == count - 1 {
                    write!(f, ", or ")?;
                } else {
                    write!(f, ", ")?;
                }
            }

            write!(f, "{item}")?;
        }

        Ok(())
    }
}

/// [`Diagnostic`] wrapper with [`Parser`]-specific metadata.
#[derive(Debug)]
#[must_use]
pub struct ParseDiagnostic {
    /// The actual diagnostic.
    inner: Diagnostic,
    /// Whether the diagnostic was caused by reaching the end of the input.
    ///
    /// This is used in [`Parser::diagnostic()`] to guard against emitting
    /// multiple EOF errors in nested structures.
    eof: bool,
}

impl From<Diagnostic> for ParseDiagnostic {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            inner: diagnostic,
            eof: false,
        }
    }
}

impl From<ParseDiagnostic> for Diagnostic {
    fn from(diagnostic: ParseDiagnostic) -> Self {
        diagnostic.inner
    }
}

impl ParseDiagnostic {
    /// Set the end-of-file flag.
    fn with_eof(mut self, eof: bool) -> Self {
        self.eof = eof;
        self
    }
}

/// Creates an "unterminated string" diagnostic error.
pub(crate) fn unterminated_string(span: Span) -> ParseDiagnostic {
    Diagnostic::error("an unterminated string was encountered")
        .with_label("this quote is not matched", span)
        .into()
}

/// Creates an "unterminated heredoc" diagnostic error.
pub(crate) fn unterminated_heredoc(opening: &str, span: Span, command: bool) -> ParseDiagnostic {
    Diagnostic::error(format!(
        "an unterminated {kind} was encountered",
        kind = if command {
            "heredoc command"
        } else {
            "multi-line string"
        }
    ))
    .with_label(format!("this {opening} is not matched"), span)
    .into()
}

/// Creates an "unterminated braced command" diagnostic error.
pub(crate) fn unterminated_braced_command(opening: &str, span: Span) -> ParseDiagnostic {
    Diagnostic::error("an unterminated braced command was encountered")
        .with_label(format!("this {opening} is not matched"), span)
        .into()
}

/// A trait implemented by parser tokens.
pub trait ParserToken<'a>: Eq + Copy + Logos<'a, Source = str, Error = (), Extras = ()> {
    /// Converts the token into its syntax representation.
    fn into_syntax(self) -> SyntaxKind;

    /// Converts the token into its "raw" representation.
    fn into_raw(self) -> u8;

    /// Converts from a raw token into the parser token.
    fn from_raw(token: u8) -> Self;

    /// Describes a raw token.
    fn describe(self) -> &'static str;

    /// Determines if the token is trivia that should be skipped over
    /// by the parser.
    ///
    /// Trivia tokens are still added to the concrete syntax tree.
    fn is_trivia(self) -> bool;

    /// A helper for recovering at an interpolation point.
    #[allow(unused_variables)]
    fn recover_interpolation(self, start: Span, parser: &mut Parser<'a, Self>) -> bool {
        false
    }
}

/// Marks the start of a node in the event list.
///
/// # Panics
///
/// Markers must either be completed or abandoned before being dropped;
/// otherwise, a panic will occur.
#[derive(Debug)]
pub struct Marker(usize);

impl Marker {
    /// Constructs a new `Marker`.
    fn new(pos: usize) -> Marker {
        Self(pos)
    }

    /// Completes the syntax tree node.
    pub fn complete<'a, T>(self, parser: &mut Parser<'a, T>, kind: SyntaxKind) -> CompletedMarker
    where
        T: ParserToken<'a>,
    {
        // Update the node kind and push a finished event
        match &mut parser.events[self.0] {
            Event::NodeStarted { kind: existing, .. } => {
                *existing = kind;
            }
            _ => unreachable!(),
        }

        parser.events.push(Event::NodeFinished);
        let m = CompletedMarker::new(self.0, kind);
        std::mem::forget(self);
        m
    }

    /// Abandons the node due to an error.
    pub fn abandon<'a, T>(self, parser: &mut Parser<'a, T>)
    where
        T: ParserToken<'a>,
    {
        // If the current node has no children, just pop it from the event list
        if self.0 == parser.events.len() - 1 {
            match parser.events.pop() {
                Some(Event::NodeStarted {
                    kind: SyntaxKind::Abandoned,
                    forward_parent: None,
                }) => (),
                _ => unreachable!(),
            }
        }

        std::mem::forget(self);
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            panic!("marker was dropped without it being completed or abandoned");
        }
    }
}

/// Represents a marker for a node that has been completed.
#[derive(Debug, Clone, Copy)]
pub struct CompletedMarker {
    /// Marks the position in the event list where the node was started.
    pos: usize,
    /// The kind of the completed node.
    kind: SyntaxKind,
}

impl CompletedMarker {
    /// Constructs a new completed marker with the given start position and
    /// syntax kind.
    fn new(pos: usize, kind: SyntaxKind) -> Self {
        CompletedMarker { pos, kind }
    }

    /// Creates a new node that precedes the completed node.
    pub fn precede<'a, T>(self, parser: &mut Parser<'a, T>) -> Marker
    where
        T: ParserToken<'a>,
    {
        let new_pos = parser.start();
        match &mut parser.events[self.pos] {
            Event::NodeStarted { forward_parent, .. } => {
                *forward_parent = Some(new_pos.0 - self.pos);
            }
            _ => unreachable!(),
        }
        new_pos
    }

    /// Extends the completed marker to the left up to `marker`.
    pub fn extend_to<'a, T>(self, parser: &mut Parser<'a, T>, marker: Marker) -> CompletedMarker
    where
        T: ParserToken<'a>,
    {
        let pos = marker.0;
        std::mem::forget(marker);
        match &mut parser.events[pos] {
            Event::NodeStarted { forward_parent, .. } => {
                *forward_parent = Some(self.pos - pos);
            }
            _ => unreachable!(),
        }
        self
    }

    /// Gets the kind of the completed marker.
    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }
}

/// A utility type used during string interpolation.
///
/// See the [Parser::interpolate] method.
#[allow(missing_debug_implementations)]
pub struct Interpolator<'a, T>
where
    T: Logos<'a, Extras = ()>,
{
    /// The version of the document being parsed.
    version: SupportedVersion,
    /// The lexer to use for the interpolation.
    lexer: Lexer<'a, T>,
    /// The parser events.
    events: Vec<Event>,
    /// The recovery token set stack.
    recovery: Vec<TokenSet>,
    /// The context for diagnostics produced by the parser.
    diagnostic_context: DiagnosticContext,
    /// The buffered events from a peek operation.
    buffered: Vec<Event>,
    /// The current expression depth of the parser.
    expr_depth: usize,
}

impl<'a, T> Interpolator<'a, T>
where
    T: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
{
    /// Adds an event to the parser event list.
    pub fn event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Adds a diagnostic to the parser error list.
    pub fn diagnostic(&mut self, diagnostic: ParseDiagnostic) {
        if diagnostic.eof {
            if self.diagnostic_context.eof {
                return;
            }
            self.diagnostic_context.eof = true;
        }

        self.diagnostic_context.diagnostics.insert(diagnostic.inner);
    }

    /// Starts a new node event.
    pub fn start(&mut self) -> Marker {
        // Append any buffered trivia before we start this node
        if !self.buffered.is_empty() {
            self.events.append(&mut self.buffered);
        }

        let pos = self.events.len();
        self.events.push(Event::NodeStarted {
            kind: SyntaxKind::Abandoned,
            forward_parent: None,
        });
        Marker::new(pos)
    }

    /// Gets the current span of the interpolator.
    pub fn span(&self) -> Span {
        self.lexer.span()
    }

    /// Consumes the interpolator and returns a parser.
    pub fn into_parser<T2>(self) -> Parser<'a, T2>
    where
        T2: ParserToken<'a>,
        T::Extras: Into<T2::Extras>,
    {
        Parser {
            version: self.version,
            lexer: Some(self.lexer.morph()),
            events: self.events,
            recovery: self.recovery,
            diagnostic_context: self.diagnostic_context,
            buffered: Default::default(),
            expr_depth: self.expr_depth,
        }
    }
}

impl<'a, T> Iterator for Interpolator<'a, T>
where
    T: Logos<'a, Error = (), Extras = ()> + Copy,
{
    type Item = (LexerResult<T>, Span);

    fn next(&mut self) -> Option<Self::Item> {
        self.lexer.next()
    }
}

/// The output of a parse.
#[allow(missing_debug_implementations)]
pub struct Output<'a, T>
where
    T: ParserToken<'a>,
{
    /// The parser's lexer.
    pub lexer: Lexer<'a, T>,
    /// The parser events.
    pub events: Vec<Event>,
    /// The parser diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Represents the result of a `peek2` operation.
///
/// See [Parser::peek2].
#[derive(Debug, Copy, Clone)]
pub struct Peek2<T> {
    /// The first peeked token.
    pub first: (T, Span),
    /// The second peeked token.
    pub second: (T, Span),
}

/// Context for managing parse diagnostics.
#[derive(Default, Debug)]
struct DiagnosticContext {
    /// The diagnostics encountered so far.
    diagnostics: IndexSet<Diagnostic>,
    /// Whether the parser has reached the end of the input.
    eof: bool,
    /// Stack of open delimiters and their spans.
    open_delimiters: Vec<(Span, &'static str)>,
    /// Spans of matching (Open, Close) braces.
    matching_block_spans: Vec<(Span, Span)>,
    /// Whether the parser has encountered a fatal error.
    halt: bool,
}

/// Implements a WDL parser.
///
/// The parser produces a list of events that can be used to
/// construct a CST.
#[allow(missing_debug_implementations)]
pub struct Parser<'a, T>
where
    T: ParserToken<'a>,
{
    /// The version of the document being parsed.
    version: SupportedVersion,
    /// The lexer that returns a stream of tokens for the parser.
    ///
    /// This may temporarily be `None` during string interpolation.
    ///
    /// See the [interpolate][Self::interpolate] method.
    lexer: Option<Lexer<'a, T>>,
    /// The events produced by the parser.
    events: Vec<Event>,
    /// The recovery token set stack.
    recovery: Vec<TokenSet>,
    /// The context for diagnostics produced by the parser.
    diagnostic_context: DiagnosticContext,
    /// The buffered events from a peek operation.
    buffered: Vec<Event>,
    /// The current expression depth.
    expr_depth: usize,
}

/// The maximum recursion depth for nested expressions.
const MAX_DEPTH: usize = 128;

/// Guard for limiting the depth of recursive expression parsing.
#[allow(missing_debug_implementations)]
pub struct RecursionGuard<'a, 'b, T>
where
    T: ParserToken<'a>,
{
    /// The parser that is being guarded.
    parser: &'b mut Parser<'a, T>,
}

impl<'a, 'b, T> Drop for RecursionGuard<'a, 'b, T>
where
    T: ParserToken<'a>,
{
    fn drop(&mut self) {
        self.parser.expr_depth -= 1;
    }
}

impl<'a, 'b, T> Deref for RecursionGuard<'a, 'b, T>
where
    T: ParserToken<'a>,
{
    type Target = Parser<'a, T>;

    fn deref(&self) -> &Self::Target {
        self.parser
    }
}

impl<'a, 'b, T> DerefMut for RecursionGuard<'a, 'b, T>
where
    T: ParserToken<'a>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parser
    }
}

impl<'a, T> Parser<'a, T>
where
    T: ParserToken<'a>,
{
    /// Construct a new parser from the given lexer.
    pub fn new(lexer: Lexer<'a, T>) -> Self {
        Self {
            version: Default::default(),
            lexer: Some(lexer),
            events: Default::default(),
            recovery: Default::default(),
            diagnostic_context: Default::default(),
            buffered: Default::default(),
            expr_depth: 0,
        }
    }

    /// Increase the current expression depth by 1.
    pub(super) fn recurse(&mut self) -> Result<RecursionGuard<'a, '_, T>, ParseDiagnostic> {
        self.expr_depth += 1;
        if self.expr_depth > MAX_DEPTH {
            self.diagnostic_context.halt = true;
            return Err(Diagnostic::error("expression nested too deep")
                .with_label("this exceeds the parser's nesting limit", self.span())
                .into());
        }
        Ok(RecursionGuard { parser: self })
    }

    /// Get the version of the document.
    pub fn version(&self) -> SupportedVersion {
        self.version
    }

    /// Set the version of the document.
    pub fn set_version(&mut self, version: SupportedVersion) {
        self.version = version;
    }

    /// Gets the current span of the parser.
    pub fn span(&self) -> Span {
        self.lexer.as_ref().expect("expected a lexer").span()
    }

    /// Gets the source being parsed at the given span.
    pub fn source(&self, span: Span) -> &'a str {
        self.lexer.as_ref().expect("expected a lexer").source(span)
    }

    /// Peeks at the next token (i.e. lookahead 1) from the lexer without
    /// consuming it.
    ///
    /// The token is not added to the event list.
    ///
    /// # Note
    ///
    /// Note that peeking may cause parser events to be buffered.
    ///
    /// If `peek` returns `None`, ensure all buffered events are added to the
    /// event list by calling `next` on the parser; otherwise, calling `finish`
    /// may panic.
    pub fn peek(&mut self) -> Option<(T, Span)> {
        while let Some((res, span)) = self.lexer.as_mut()?.peek() {
            if let Some(t) = self.consume_trivia(res, span, true) {
                return Some(t);
            }
        }

        None
    }

    /// Peeks at the next and next-next tokens (i.e. lookahead 2) from the lexer
    /// without consuming either token.
    ///
    /// The returned tokens are not added to the event list.
    pub fn peek2(&mut self) -> Option<Peek2<T>> {
        let first = self.peek()?;

        // We have to clone the lexer here since it only supports a single lookahead.
        // The clone is cheap, but it does mean we'll re-tokenize this second lookahead
        // eventually.
        let mut lexer = self
            .lexer
            .as_ref()
            .expect("there should be a lexer")
            .clone();
        lexer
            .next()
            .unwrap()
            .0
            .expect("should have peeked at a valid token");
        while let Some((Ok(token), span)) = lexer.next() {
            if token.is_trivia() {
                // Ignore trivia
                continue;
            }

            return Some(Peek2 {
                first,
                second: (token, span),
            });
        }

        None
    }

    /// Consumes the next token only if it matches the given token.
    ///
    /// Returns the span of the token if it was consumed, `None` if otherwise.
    pub fn next_if(&mut self, token: T) -> Option<Span> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Some(span)
            }
            _ => None,
        }
    }

    /// Parses a matching token pair that surrounds an item.
    ///
    /// This method parses the open token, calls the callback to parse the item,
    /// and then parses the close token.
    pub fn matching<F>(
        &mut self,
        open: T,
        close: T,
        allow_empty: bool,
        cb: F,
    ) -> Result<(), ParseDiagnostic>
    where
        F: FnOnce(&mut Self, Span) -> Result<(), ParseDiagnostic>,
    {
        let open_span = self.expect(open)?;

        self.diagnostic_context
            .open_delimiters
            .push((open_span, open.describe()));

        // Check to see if the close token is immediately following the opening
        if allow_empty {
            match self.peek() {
                Some((t, span)) if t == close => {
                    self.next();
                    self.diagnostic_context
                        .matching_block_spans
                        .push((open_span, span));
                    self.diagnostic_context.open_delimiters.pop();
                    return Ok(());
                }
                _ => {}
            }
        }

        cb(self, open_span)?;

        let res = match self.next() {
            Some((token, span)) if token == close => {
                self.diagnostic_context
                    .matching_block_spans
                    .push((open_span, span));
                Ok(())
            }
            found => {
                Err(self.mismatched_delim_err(open.describe(), open_span, close.describe(), found))
            }
        };

        self.diagnostic_context.open_delimiters.pop();
        res
    }

    /// Parses a matching token pair that surround a delimited list of items.
    ///
    /// This method parses the open token, calls the callback for each delimited
    /// item, and then parses the close token.
    ///
    /// The provided recovery token set is used to recover within the delimited
    /// item list. The provided termination token set, in addition to `close`,
    /// causes the loop to stop early; on early stop, [`consume_close_token`]
    /// synthesizes a zero-width close token and emits an "unmatched" diagnostic
    /// so the surrounding caller can continue parsing.
    ///
    /// [`consume_close_token`]: Self::consume_close_token
    pub fn matching_delimited<F>(
        &mut self,
        open: T,
        close: T,
        delimiter: Option<T>,
        termination: TokenSet,
        recovery: TokenSet,
        cb: F,
    ) -> Result<(), ParseDiagnostic>
    where
        F: FnMut(&mut Self, Marker) -> Result<(), (Marker, ParseDiagnostic)>,
    {
        let open_span = self.expect(open)?;
        self.diagnostic_context
            .open_delimiters
            .push((open_span, open.describe()));

        self.delimited(close, termination, delimiter, recovery, cb);
        self.consume_close_token(open, open_span, close);

        self.diagnostic_context.open_delimiters.pop();
        Ok(())
    }

    /// Create a new "unmatched opening delimiter" diagnostic.
    fn mismatched_delim_err(
        &mut self,
        open: &str,
        open_span: Span,
        close: &str,
        found: Option<(T, Span)>,
    ) -> ParseDiagnostic {
        let mut diagnostic = self.unexpected(close, found);
        diagnostic.inner = diagnostic
            .inner
            .with_label(format!("this {open} is not matched"), open_span);
        self.report_suspicious_mismatch_block(diagnostic)
    }

    /// Extend a [`Self::unexpected()`] diagnostic with information about the
    /// closest closing brace.
    fn report_suspicious_mismatch_block(&self, mut diagnostic: ParseDiagnostic) -> ParseDiagnostic {
        /// Calculates the indentation level of the line containing the start of
        /// the given span.
        fn indentation_level<'a, T>(parser: &Parser<'a, T>, span: Span) -> usize
        where
            T: ParserToken<'a>,
        {
            let prefix = parser.source(Span::new(0, span.start()));
            let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);

            prefix[line_start..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .count()
        }

        let mut matched_spans: Vec<(Span, bool)> = self
            .diagnostic_context
            .matching_block_spans
            .iter()
            .map(|&(open, close)| {
                let same_indent = indentation_level(self, open) == indentation_level(self, close);
                (
                    Span::new(open.start(), close.start() - open.start()),
                    same_indent,
                )
            })
            .collect();

        // Sort by `start` position, working outside-in
        matched_spans.sort_by_key(|(open, ..)| open.start());

        for i in 0..matched_spans.len() {
            let (outer_span, same_indent) = matched_spans[i];
            if !same_indent {
                continue;
            }

            for (inner_span, same_indent) in matched_spans.iter_mut().skip(i + 1) {
                if outer_span.contains(inner_span.start()) && outer_span.contains(inner_span.end())
                {
                    *same_indent = true;
                }
            }
        }

        // Find the innermost span candidate that still has a mismatched indentation
        let candidate_span = matched_spans
            .into_iter()
            .rev()
            .find(|&(_, same_indent)| !same_indent);

        if let Some((block_span, _)) = candidate_span {
            diagnostic.inner = diagnostic
                .inner
                .with_label(
                    "this delimiter might not be properly closed...",
                    Span::new(block_span.start(), 1),
                )
                .with_label(
                    "...as it matches this but it has different indentation",
                    Span::new(block_span.end(), 1),
                );
        } else if let Some(&(parent_open, parent_close)) =
            self.diagnostic_context.matching_block_spans.last()
        {
            // Optional fallback if we don't have a specific candidate
            diagnostic.inner = diagnostic
                .inner
                .with_label("this opening brace...", parent_open)
                .with_label("...matches this closing brace", parent_close);
        }

        diagnostic
    }

    /// Create a diagnostic reporting all unclosed delimiters up to EOF.
    fn eof_err(&mut self, open: T) {
        if self.diagnostic_context.eof {
            return;
        }

        const UNCLOSED_DELIMITER_SHOW_LIMIT: usize = 3;
        let mut diagnostic = self.unexpected_inner(open.describe(), None, false);

        let delimiters_shown = usize::min(
            UNCLOSED_DELIMITER_SHOW_LIMIT,
            self.diagnostic_context.open_delimiters.len(),
        );
        for (span, _) in &self.diagnostic_context.open_delimiters[..delimiters_shown] {
            diagnostic.inner = diagnostic.inner.with_label("unclosed delimiter", *span);
        }

        // Summarize the rest
        if let Some((span, _)) = self
            .diagnostic_context
            .open_delimiters
            .get(UNCLOSED_DELIMITER_SHOW_LIMIT)
            && self.diagnostic_context.open_delimiters.len() >= UNCLOSED_DELIMITER_SHOW_LIMIT + 2
        {
            diagnostic.inner = diagnostic.inner.with_label(
                format!(
                    "another {} unclosed delimiters begin from here",
                    self.diagnostic_context.open_delimiters.len() - UNCLOSED_DELIMITER_SHOW_LIMIT
                ),
                *span,
            );
        }

        if self.diagnostic_context.open_delimiters.last().is_some() {
            diagnostic = self.report_suspicious_mismatch_block(diagnostic);
        }

        self.diagnostic(diagnostic);

        // Prevent duplicate EOF diagnostics
        self.diagnostic_context.eof = true;
    }

    /// Consumes a close token if it is the next token to be parsed.
    ///
    /// Otherwise, emits an "unmatched" diagnostic and synthesizes the close
    /// token into the parser's list of events.
    pub fn consume_close_token(&mut self, open: T, open_span: Span, close: T) {
        if let Some(span) = self.next_if(close) {
            self.diagnostic_context
                .matching_block_spans
                .push((open_span, span));
            return;
        }

        let found = self.peek();
        if found.is_some() {
            let diagnostic =
                self.mismatched_delim_err(open.describe(), open_span, close.describe(), found);
            self.diagnostic(diagnostic);
        } else {
            self.eof_err(open);
        }

        // Synthesize a close token event of zero width
        let span = found.map(|(_, s)| s).unwrap_or_else(|| self.span());
        self.events.push(Event::Token {
            kind: close.into_syntax(),
            span: Span::new(span.start(), 0),
        });
    }

    /// Parses a delimited list of items until the given `until` token.
    ///
    /// The provided recovery token set is used to recover within the delimited
    /// item list. Any token in the termination set additionally ends the loop
    /// after a successfully-parsed item.
    ///
    /// Neither `until` nor any termination token is consumed by this method.
    pub fn delimited<F>(
        &mut self,
        until: T,
        termination: TokenSet,
        delimiter: Option<T>,
        recovery: TokenSet,
        mut cb: F,
    ) where
        F: FnMut(&mut Self, Marker) -> Result<(), (Marker, ParseDiagnostic)>,
    {
        let recovery = if let Some(delimiter) = delimiter {
            recovery
                .union(termination)
                .union(TokenSet::new(&[until.into_raw(), delimiter.into_raw()]))
        } else {
            recovery
                .union(termination)
                .union(TokenSet::new(&[until.into_raw()]))
        };

        let parent = self.recovery.last().copied();
        self.recovery.push(recovery);

        let mut next: Option<(T, Span)> = self.peek();
        while let Some((token, _)) = next {
            if token == until || self.diagnostic_context.halt {
                break;
            }

            let mut lexer = self.lexer.clone();
            let marker = self.start();
            if let Err((marker, e)) = cb(self, marker) {
                if let Some((Ok(token), _)) = lexer.as_mut().expect("should have a lexer").peek()
                    && !recovery.contains(token.into_raw())
                {
                    // Determine if the token is recoverable in the parent recovery set
                    // If so, we'll restart where we first attempted to parse this item
                    if let Some(parent) = &parent
                        && parent.contains(token.into_raw())
                    {
                        // Truncate the event list and abandon the marker
                        self.events.truncate(marker.0);
                        marker.abandon(self);

                        // Clear any buffered events and reset the lexer
                        self.buffered.clear();
                        self.lexer = lexer;
                        break;
                    }
                }

                self.recover(e);
                marker.abandon(self);

                if self.diagnostic_context.halt {
                    break;
                }
            }

            next = self.peek();

            if let Some(delimiter) = delimiter
                && let Some((token, _)) = next
            {
                if token == until || termination.contains(token.into_raw()) {
                    break;
                }

                if let Err(mut e) = self.expect(delimiter) {
                    // Attach a label to the diagnostic hinting at where we expected the
                    // delimiter to be; to do this, look back at the last non-trivia token event
                    // in the parser events and use its span for the label.
                    let span = self.events.iter().rev().find_map(|e| match e {
                        Event::Token { kind, span }
                            if *kind != SyntaxKind::Whitespace && *kind != SyntaxKind::Comment =>
                        {
                            Some(*span)
                        }
                        _ => None,
                    });

                    let e = if let Some(span) = span {
                        e.inner = e.inner.with_label(
                            format!(
                                "consider adding a {desc} after this",
                                desc = delimiter.describe()
                            ),
                            Span::new(span.end() - 1, 1),
                        );
                        e
                    } else {
                        e
                    };

                    self.recover(e);
                    self.next_if(delimiter);
                }

                next = self.peek();
            }
        }

        self.recovery.pop();
    }

    /// Adds a diagnostic to the parser output.
    pub fn diagnostic(&mut self, diagnostic: ParseDiagnostic) {
        if diagnostic.eof {
            if self.diagnostic_context.eof {
                return;
            }
            self.diagnostic_context.eof = true;
        }

        self.diagnostic_context.diagnostics.insert(diagnostic.inner);
    }

    /// Pushes a token set to the parser's recovery token set stack.
    pub fn push_recovery_set(&mut self, tokens: TokenSet) {
        self.recovery.push(tokens);
    }

    /// Pops a token set from the parser's recovery token set stack.
    ///
    /// # Panics
    ///
    /// Panics if the parser's recovery set is empty.
    pub fn pop_recovery_set(&mut self) {
        self.recovery.pop().expect("should pop");
    }

    /// Recovers from an error by consuming all tokens not in the top-most
    /// recovery set.
    ///
    /// # Panics
    ///
    /// Panics if a recovery set was not pushed with [Self::push_recovery_set].
    pub fn recover(&mut self, mut diagnostic: ParseDiagnostic) {
        let tokens = *self.recovery.last().expect("expected a top recovery set");

        while let Some((token, span)) = self.peek() {
            if tokens.contains(token.into_raw()) {
                break;
            }

            self.next().unwrap();

            // If the token starts an interpolation, then we need
            // to move past the entire set of tokens that are part
            // of the interpolation
            if T::recover_interpolation(token, span, self) {
                // If the diagnostic label started at this token, we need to extend its length
                // to cover the interpolation
                for label in diagnostic.inner.labels_mut() {
                    let label_span = label.span();
                    if label_span.start() != span.start() {
                        continue;
                    }

                    // The label should include everything up to the current start
                    label.set_span(Span::new(
                        label_span.start(),
                        self.lexer
                            .as_ref()
                            .expect("should have a lexer")
                            .span()
                            .end()
                            - label_span.end()
                            + 1,
                    ));
                }
            }
        }

        self.diagnostic(diagnostic);
    }

    /// Performs recovery with the given recovery token set.
    pub fn recover_with_set(&mut self, diagnostic: ParseDiagnostic, recovery: TokenSet) {
        self.recovery.push(recovery);
        self.recover(diagnostic);
        self.recovery.pop();
    }

    /// Starts a new node event.
    pub fn start(&mut self) -> Marker {
        // Peek before starting the node so that any trivia appears as siblings to this
        // node
        if !self.events.is_empty() {
            self.peek();

            // Append any buffered trivia before we start this node
            if !self.buffered.is_empty() {
                self.events.append(&mut self.buffered);
            }
        }

        let pos = self.events.len();
        self.events.push(Event::NodeStarted {
            kind: SyntaxKind::Abandoned,
            forward_parent: None,
        });
        Marker::new(pos)
    }

    /// Requires that the current token is the given token.
    ///
    /// Panics if the token is not the given token.
    pub fn require(&mut self, token: T) -> Span {
        match self.next() {
            Some((t, span)) if t == token => span,
            _ => panic!(
                "lexer not at required token {token}",
                token = token.describe()
            ),
        }
    }

    /// Requires that the current token is in the given token set.
    ///
    /// # Panics
    ///
    /// Panics if the token is not in the token set.
    pub fn require_in(&mut self, tokens: TokenSet) {
        match self.next() {
            Some((t, _)) if tokens.contains(t.into_raw()) => {}
            found => {
                let found = found.map(|(t, _)| t.describe());
                panic!(
                    "unexpected token {found}",
                    found = found.unwrap_or("end of input")
                );
            }
        }
    }

    /// Determines if the `found` token is EOF, which is used by
    /// [`Self::diagnostic()`] for deduplication.
    fn maybe_eof_diagnostic(
        &mut self,
        found: Option<(T, Span)>,
    ) -> (Option<&'static str>, Span, bool) {
        let (found, span) = found
            .map(|(t, s)| (Some(t.describe()), s))
            .unwrap_or_else(|| (None, self.span()));

        let eof = found.is_none();
        (found, span, eof)
    }

    /// Creates an "expected, but found" diagnostic error.
    pub(crate) fn unexpected(
        &mut self,
        expected: &str,
        found: Option<(T, Span)>,
    ) -> ParseDiagnostic {
        self.unexpected_inner(expected, found, true)
    }

    /// [`Self::unexpected()`] with the ability to toggle the label on the
    /// `found` token.
    fn unexpected_inner(
        &mut self,
        expected: &str,
        found: Option<(T, Span)>,
        label_found: bool,
    ) -> ParseDiagnostic {
        let (found, span, eof) = self.maybe_eof_diagnostic(found);

        let found = found.unwrap_or("end of input");
        let mut diagnostic = Diagnostic::error(format!("expected {expected}, but found {found}"));
        if label_found {
            diagnostic = diagnostic.with_label(format!("unexpected {found}"), span)
        }

        Into::<ParseDiagnostic>::into(diagnostic).with_eof(eof)
    }

    /// Creates an "expected one of, but found" diagnostic error.
    pub(crate) fn unexpected_many(
        &mut self,
        expected: &[&str],
        found: Option<(T, Span)>,
    ) -> ParseDiagnostic {
        let (found, span, eof) = self.maybe_eof_diagnostic(found);

        let found = found.unwrap_or("end of input");
        let diagnostic: ParseDiagnostic = Diagnostic::error(format!(
            "expected {expected}, but found {found}",
            expected = Expected::new(expected)
        ))
        .with_label(format!("unexpected {found}"), span)
        .into();

        diagnostic.with_eof(eof)
    }

    /// Creates an "unmatched token" diagnostic error.
    pub(crate) fn unmatched(
        &mut self,
        open: &str,
        open_span: Span,
        close: &str,
        found: Option<(T, Span)>,
    ) -> ParseDiagnostic {
        let mut diagnostic = self.unexpected(close, found);
        diagnostic.inner = diagnostic
            .inner
            .with_label(format!("this {open} is not matched"), open_span);

        diagnostic
    }

    /// Expects the next token to be the given token.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect(&mut self, token: T) -> Result<Span, ParseDiagnostic> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            found => Err(self.unexpected(token.describe(), found)),
        }
    }

    /// Expects the next token to be the given token, but uses
    /// the provided name in the error.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect_with_name(
        &mut self,
        token: T,
        name: &'static str,
    ) -> Result<Span, ParseDiagnostic> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            found => Err(self.unexpected(name, found)),
        }
    }

    /// Expects the next token to be in the given token set.
    ///
    /// Returns an error if the token is not the given set.
    pub fn expect_in(
        &mut self,
        tokens: TokenSet,
        expected: &[&str],
    ) -> Result<(T, Span), ParseDiagnostic> {
        match self.peek() {
            Some((t, span)) if tokens.contains(t.into_raw()) => {
                self.next();
                Ok((t, span))
            }
            found => Err(self.unexpected_many(expected, found)),
        }
    }

    /// Used to interpolate strings with a different string interpolation token.
    ///
    /// The provided callback receives a [Interpolator].
    ///
    /// The callback should use [Interpolator::into_parser] for the return
    /// value.
    pub fn interpolate<T2, F, R>(&mut self, cb: F) -> R
    where
        T2: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
        F: FnOnce(Interpolator<'a, T2>) -> (Parser<'a, T>, R),
    {
        let input = Interpolator {
            version: self.version,
            lexer: std::mem::take(&mut self.lexer)
                .expect("lexer should exist")
                .morph(),
            recovery: std::mem::take(&mut self.recovery),
            events: std::mem::take(&mut self.events),
            diagnostic_context: std::mem::take(&mut self.diagnostic_context),
            buffered: std::mem::take(&mut self.buffered),
            expr_depth: self.expr_depth,
        };
        let (p, result) = cb(input);
        *self = p;
        result
    }

    /// Morph this parser into a parser for a new token type.
    ///
    /// The returned parser continues to point at the same span
    /// as the current parser.
    pub fn morph<T2>(self) -> Parser<'a, T2>
    where
        T2: ParserToken<'a>,
        T::Extras: Into<T2::Extras>,
    {
        Parser {
            version: self.version,
            lexer: self.lexer.map(|l| l.morph()),
            events: self.events,
            recovery: self.recovery,
            diagnostic_context: self.diagnostic_context,
            buffered: self.buffered,
            expr_depth: self.expr_depth,
        }
    }

    /// Consumes the parser and returns an interpolator.
    pub fn into_interpolator<T2>(self) -> Interpolator<'a, T2>
    where
        T2: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
    {
        Interpolator {
            version: self.version,
            lexer: self.lexer.expect("lexer should be present").morph(),
            events: self.events,
            recovery: self.recovery,
            diagnostic_context: self.diagnostic_context,
            buffered: self.buffered,
            expr_depth: self.expr_depth,
        }
    }

    /// Consumes the parser and returns the output.
    ///
    /// # Panics
    ///
    /// This method panics if buffered events remain in the parser.
    ///
    /// To ensure that no buffered events remain, call `next()` on the parser
    /// and verify it returns `None` before calling this method.
    pub fn finish(self) -> Output<'a, T> {
        assert!(
            self.buffered.is_empty(),
            "buffered events remain; ensure `next` was called after an unsuccessful peek"
        );

        Output {
            lexer: self.lexer.expect("lexer should be present"),
            events: self.events,
            diagnostics: self.diagnostic_context.diagnostics.into_iter().collect(),
        }
    }

    /// Updates the syntax kind of the last token event.
    ///
    /// # Panics
    ///
    /// Panics if the last event was not a token.
    pub fn update_last_token_kind(&mut self, new_kind: SyntaxKind) {
        let last = self.events.last_mut().expect("expected a last event");
        match last {
            Event::Token { kind, .. } => *kind = new_kind,
            _ => panic!("the last event is not a token"),
        }
    }

    /// Consumes the remainder of the unparsed source into a special
    /// "unparsed" token.
    ///
    /// This occurs when a source file is missing a version statement or
    /// if the version specified is unsupported.
    pub fn consume_remainder(&mut self) {
        if !self.buffered.is_empty() {
            self.events.append(&mut self.buffered);
        }

        if let Some(span) = self
            .lexer
            .as_mut()
            .expect("there should be a lexer")
            .consume_remainder()
        {
            self.events.push(Event::Token {
                kind: SyntaxKind::Unparsed,
                span,
            });
        }
    }

    /// Consumes any trivia tokens by adding them to the event list.
    fn consume_trivia(
        &mut self,
        res: LexerResult<T>,
        span: Span,
        peeked: bool,
    ) -> Option<(T, Span)> {
        // If not peeked and there are buffered events, append them now
        if !peeked && !self.buffered.is_empty() {
            self.events.append(&mut self.buffered);
        }

        let event = match res {
            Ok(token) => {
                if !token.is_trivia() {
                    return Some((token, span));
                }

                if peeked {
                    self.lexer.as_mut().expect("should have a lexer").next();
                }

                Event::Token {
                    kind: token.into_syntax(),
                    span,
                }
            }
            Err(_) => {
                let mut unknown_span = span;
                let lexer = self.lexer.as_mut().expect("should have a lexer");

                if peeked {
                    lexer.next();
                }

                // Consecutive unknown tokens of the same type get condensed into a single
                // diagnostic and event
                while let Some((Err(_), peeked_span)) = lexer.peek() {
                    unknown_span = Span::new(
                        unknown_span.start(),
                        peeked_span.end() - unknown_span.start(),
                    );
                    lexer.next();
                }

                self.diagnostic(
                    Diagnostic::error("an unknown token was encountered")
                        .with_label(
                            Self::unsupported_token_text(self.source(span)),
                            unknown_span,
                        )
                        .into(),
                );

                Event::Token {
                    kind: SyntaxKind::Unknown,
                    span: unknown_span,
                }
            }
        };

        if peeked {
            self.buffered.push(event);
        } else {
            self.events.push(event);
        }

        None
    }

    /// A helper for unsupported token error span text.
    fn unsupported_token_text(token: &str) -> &'static str {
        match token {
            "&" => "did you mean to use `&&` here?",
            "|" => "did you mean to use `||` here?",
            _ => "this is not a supported WDL token",
        }
    }
}

impl<'a, T> Iterator for Parser<'a, T>
where
    T: ParserToken<'a>,
{
    type Item = (T, Span);

    fn next(&mut self) -> Option<(T, Span)> {
        while let Some((res, span)) = self.lexer.as_mut()?.next() {
            if let Some((token, span)) = self.consume_trivia(res, span, false) {
                self.events.push(Event::Token {
                    kind: token.into_syntax(),
                    span,
                });
                return Some((token, span));
            }
        }

        if !self.buffered.is_empty() {
            self.events.append(&mut self.buffered);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_depth_limit() {
        let ok_map_literal = format!(
            "{} : {}",
            "{".repeat(MAX_DEPTH - 1),
            "}".repeat(MAX_DEPTH - 1)
        );
        let source = format!(
            r#"task foo {{
            command <<<>>>

            Map[String, Int] woah = {ok_map_literal}
        }}"#
        );
        let mut parser = Parser::new(Lexer::new(&source));
        crate::grammar::v1::items(&mut parser);
        assert!(!parser.diagnostic_context.halt);

        let bad_map_literal = format!("{} : {}", "{".repeat(MAX_DEPTH), "}".repeat(MAX_DEPTH));
        let source = format!(
            r#"task foo {{
            command <<<>>>

            Map[String, Int] woah = {bad_map_literal}
        }}"#
        );
        let mut parser = Parser::new(Lexer::new(&source));
        crate::grammar::v1::items(&mut parser);
        assert!(parser.diagnostic_context.halt);
    }
}
