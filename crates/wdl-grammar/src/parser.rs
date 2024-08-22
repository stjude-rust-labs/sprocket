//! Module for the parser implementation.
//!
//! The parser consumes a token stream from a lexer and produces
//! a list of parser events that can be used to construct a CST.
//!
//! The design of this is very much based on `rust-analyzer`.

use std::fmt;

use logos::Logos;

use super::lexer::Lexer;
use super::lexer::LexerResult;
use super::lexer::TokenSet;
use super::tree::SyntaxKind;
use super::Diagnostic;
use super::Span;

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

/// Creates an "expected, but found" diagnostic error.
pub(crate) fn expected_found(expected: &str, found: Option<&str>, span: Span) -> Diagnostic {
    let found = found.unwrap_or("end of input");
    Diagnostic::error(format!("expected {expected}, but found {found}"))
        .with_label(format!("unexpected {found}"), span)
}

/// Creates an "expected one of, but found" diagnostic error.
pub(crate) fn expected_one_of(expected: &[&str], found: Option<&str>, span: Span) -> Diagnostic {
    let found = found.unwrap_or("end of input");
    Diagnostic::error(format!(
        "expected {expected}, but found {found}",
        expected = Expected::new(expected)
    ))
    .with_label(format!("unexpected {found}"), span)
}

/// Creates an "unterminated string" diagnostic error.
pub(crate) fn unterminated_string(span: Span) -> Diagnostic {
    Diagnostic::error("an unterminated string was encountered")
        .with_label("this quote is not matched", span)
}

/// Creates an "unterminated heredoc" diagnostic error.
pub(crate) fn unterminated_heredoc(opening: &str, span: Span, command: bool) -> Diagnostic {
    Diagnostic::error(format!(
        "an unterminated {kind} was encountered",
        kind = if command {
            "heredoc command"
        } else {
            "multi-line string"
        }
    ))
    .with_label(format!("this {opening} is not matched"), span)
}

/// Creates an "unterminated braced command" diagnostic error.
pub(crate) fn unterminated_braced_command(opening: &str, span: Span) -> Diagnostic {
    Diagnostic::error("an unterminated braced command was encountered")
        .with_label(format!("this {opening} is not matched"), span)
}

/// Creates an "unmatched token" diagnostic error.
pub(crate) fn unmatched(
    open: &str,
    open_span: Span,
    close: &str,
    found: &str,
    span: Span,
) -> Diagnostic {
    expected_found(close, Some(found), span)
        .with_label(format!("this {open} is not matched"), open_span)
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
    /// The lexer to use for the interpolation.
    lexer: Lexer<'a, T>,
    /// The parser events.
    events: Vec<Event>,
    /// The recovery token set stack.
    recovery: Vec<TokenSet>,
    /// The parser diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// The buffered events from a peek operation.
    buffered: Vec<Event>,
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
    pub fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
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
            lexer: Some(self.lexer.morph()),
            events: self.events,
            recovery: self.recovery,
            diagnostics: self.diagnostics,
            buffered: Default::default(),
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

/// Implements a WDL parser.
///
/// The parser produces a list of events that can be used to
/// construct a CST.
#[allow(missing_debug_implementations)]
pub struct Parser<'a, T>
where
    T: ParserToken<'a>,
{
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
    /// The diagnostics encountered so far.
    diagnostics: Vec<Diagnostic>,
    /// The buffered events from a peek operation.
    buffered: Vec<Event>,
}

impl<'a, T> Parser<'a, T>
where
    T: ParserToken<'a>,
{
    /// Construct a new parser from the given lexer.
    pub fn new(lexer: Lexer<'a, T>) -> Self {
        Self {
            lexer: Some(lexer),
            events: Default::default(),
            recovery: Default::default(),
            diagnostics: Default::default(),
            buffered: Default::default(),
        }
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
    /// Returns `true` if the token was consumed, `false` if otherwise.
    pub fn next_if(&mut self, token: T) -> bool {
        match self.peek() {
            Some((t, _)) if t == token => {
                self.next();
                true
            }
            _ => false,
        }
    }

    /// Parses a matching token pair that surrounds an item.
    ///
    /// This method parses the open token, calls the callback to parse the item,
    /// and then parses the close token.
    pub fn matching<F>(&mut self, open: T, close: T, cb: F) -> Result<(), Diagnostic>
    where
        F: FnOnce(&mut Self, Span) -> Result<(), Diagnostic>,
    {
        let open_span = match self.expect(open) {
            Ok(span) => span,
            Err(e) => return Err(e),
        };

        // Check to see if the close token is immediately following the opening
        match self.peek() {
            Some((t, _)) if t == close => {
                self.next();
                return Ok(());
            }
            _ => {}
        }

        cb(self, open_span)?;

        match self.next() {
            Some((token, _)) if token == close => Ok(()),
            found => {
                let (found, span) = found
                    .map(|(t, s)| (t.describe(), s))
                    .unwrap_or_else(|| ("end of input", self.span()));

                Err(unmatched(
                    open.describe(),
                    open_span,
                    close.describe(),
                    found,
                    span,
                ))
            }
        }
    }

    /// Parses a matching token pair that surround a delimited list of items.
    ///
    /// This method parses the open token, calls the callback for each delimited
    /// item, and then parses the close token.
    ///
    /// The provided recovery token set is used to recover within the delimited
    /// item list.
    pub fn matching_delimited<F>(
        &mut self,
        open: T,
        close: T,
        delimiter: Option<T>,
        recovery: TokenSet,
        cb: F,
    ) -> Result<(), Diagnostic>
    where
        F: FnMut(&mut Self, Marker) -> Result<(), (Marker, Diagnostic)>,
    {
        let open_span = match self.expect(open) {
            Ok(span) => span,
            Err(e) => return Err(e),
        };

        self.delimited(close, delimiter, recovery, cb);
        self.consume_close_token(open, open_span, close);
        Ok(())
    }

    /// Consumes a close token if it is the next token to be parsed.
    ///
    /// Otherwise, emits an "unmatched" diagnostic and synthesizes the close
    /// token into the parser's list of events.
    pub fn consume_close_token(&mut self, open: T, open_span: Span, close: T) {
        if self.next_if(close) {
            return;
        }

        let (found, span) = self
            .peek()
            .map(|(t, s)| (t.describe(), s))
            .unwrap_or_else(|| ("end of input", self.span()));

        self.diagnostic(unmatched(
            open.describe(),
            open_span,
            close.describe(),
            found,
            span,
        ));

        // Synthesize a close token event of zero width
        self.events.push(Event::Token {
            kind: close.into_syntax(),
            span: Span::new(span.start(), 0),
        });
    }

    /// Parses a delimited list of items until the given token.
    ///
    /// The provided recovery token set is used to recover within the delimited
    /// item list.
    ///
    /// The `until` token is not consumed.
    pub fn delimited<F>(&mut self, until: T, delimiter: Option<T>, recovery: TokenSet, mut cb: F)
    where
        F: FnMut(&mut Self, Marker) -> Result<(), (Marker, Diagnostic)>,
    {
        let recovery = if let Some(delimiter) = delimiter {
            recovery.union(TokenSet::new(&[until.into_raw(), delimiter.into_raw()]))
        } else {
            recovery.union(TokenSet::new(&[until.into_raw()]))
        };

        let parent = self.recovery.last().copied();
        self.recovery.push(recovery);

        let mut next: Option<(T, Span)> = self.peek();
        while let Some((token, _)) = next {
            if token == until {
                break;
            }

            let mut lexer = self.lexer.clone();
            let marker = self.start();
            if let Err((marker, e)) = cb(self, marker) {
                if let Some((Ok(token), _)) = lexer.as_mut().expect("should have a lexer").peek() {
                    if !recovery.contains(token.into_raw()) {
                        // Determine if the token is recoverable in the parent recovery set
                        // If so, we'll restart where we first attempted to parse this item
                        if let Some(parent) = &parent {
                            if parent.contains(token.into_raw()) {
                                // Truncate the event list and abandon the marker
                                self.events.truncate(marker.0);
                                marker.abandon(self);

                                // Clear any buffered events and reset the lexer
                                self.buffered.clear();
                                self.lexer = lexer;
                                break;
                            }
                        }
                    }
                }

                self.recover(e);
                marker.abandon(self);
            }

            next = self.peek();

            if let Some(delimiter) = delimiter {
                if let Some((token, _)) = next {
                    if token == until {
                        break;
                    }

                    if let Err(e) = self.expect(delimiter) {
                        // Attach a label to the diagnostic hinting at where we expected the
                        // delimiter to be; to do this, look back at the last non-trivia token event
                        // in the parser events and use its span for the label.
                        let e = if let Some(span) = self.events.iter().rev().find_map(|e| match e {
                            Event::Token { kind, span }
                                if *kind != SyntaxKind::Whitespace
                                    && *kind != SyntaxKind::Comment =>
                            {
                                Some(*span)
                            }
                            _ => None,
                        }) {
                            e.with_label(
                                format!(
                                    "consider adding a {desc} after this",
                                    desc = delimiter.describe()
                                ),
                                Span::new(span.end() - 1, 1),
                            )
                        } else {
                            e
                        };

                        self.recover(e);
                        self.next_if(delimiter);
                    }

                    next = self.peek();
                }
            }
        }

        self.recovery.pop();
    }

    /// Adds a diagnostic to the parser output.
    pub fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
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
    pub fn recover(&mut self, mut diagnostic: Diagnostic) {
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
                for label in diagnostic.labels_mut() {
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

        self.diagnostics.push(diagnostic);
    }

    /// Performs recovery with the given recovery token set.
    pub fn recover_with_set(&mut self, diagnostic: Diagnostic, recovery: TokenSet) {
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

    /// Expects the next token to be the given token.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect(&mut self, token: T) -> Result<Span, Diagnostic> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            found => {
                let (found, span) = found
                    .map(|(t, s)| (Some(t.describe()), s))
                    .unwrap_or_else(|| (None, self.span()));
                Err(expected_found(token.describe(), found, span))
            }
        }
    }

    /// Expects the next token to be the given token, but uses
    /// the provided name in the error.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect_with_name(&mut self, token: T, name: &'static str) -> Result<Span, Diagnostic> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            found => {
                let (found, span) = found
                    .map(|(t, s)| (Some(t.describe()), s))
                    .unwrap_or_else(|| (None, self.span()));
                Err(expected_found(name, found, span))
            }
        }
    }

    /// Expects the next token to be in the given token set.
    ///
    /// Returns an error if the token is not the given set.
    pub fn expect_in(
        &mut self,
        tokens: TokenSet,
        expected: &[&str],
    ) -> Result<(T, Span), Diagnostic> {
        match self.peek() {
            Some((t, span)) if tokens.contains(t.into_raw()) => {
                self.next();
                Ok((t, span))
            }
            found => {
                let (found, span) = found
                    .map(|(t, s)| (Some(t.describe()), s))
                    .unwrap_or_else(|| (None, self.span()));

                Err(expected_one_of(expected, found, span))
            }
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
            lexer: std::mem::take(&mut self.lexer)
                .expect("lexer should exist")
                .morph(),
            recovery: std::mem::take(&mut self.recovery),
            events: std::mem::take(&mut self.events),
            diagnostics: std::mem::take(&mut self.diagnostics),
            buffered: std::mem::take(&mut self.buffered),
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
            lexer: self.lexer.map(|l| l.morph()),
            events: self.events,
            recovery: self.recovery,
            diagnostics: self.diagnostics,
            buffered: self.buffered,
        }
    }

    /// Consumes the parser and returns an interpolator.
    pub fn into_interpolator<T2>(self) -> Interpolator<'a, T2>
    where
        T2: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
    {
        Interpolator {
            lexer: self.lexer.expect("lexer should be present").morph(),
            events: self.events,
            recovery: self.recovery,
            diagnostics: self.diagnostics,
            buffered: self.buffered,
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
            diagnostics: self.diagnostics,
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

                Event::Token {
                    kind: token.into_syntax(),
                    span,
                }
            }
            Err(_) => {
                self.diagnostic(
                    Diagnostic::error("an unknown token was encountered")
                        .with_label(Self::unsupported_token_text(self.source(span)), span),
                );
                Event::Token {
                    kind: SyntaxKind::Unknown,
                    span,
                }
            }
        };

        if peeked {
            self.lexer.as_mut().expect("should have a lexer").next();
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
