//! Module for the parser implementation.
//!
//! The parser consumes a token stream from a lexer and produces
//! a list of parser events that can be used to construct a CST.
//!
//! The design of this is very much based on `rust-analyzer`.

use std::fmt;

use logos::Logos;
use miette::Diagnostic;
use miette::SourceSpan;

use super::lexer;
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
        span: SourceSpan,
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

/// Utility type for displaying "found" tokens in a parser expectation error.
struct Found {
    /// The raw token that was found (or `None` for end-of-input).
    token: Option<u8>,
    /// The function used to describe a raw token.
    describe: fn(u8) -> &'static str,
}

impl Found {
    /// Constructs a new `Found`.
    fn new(token: Option<u8>, describe: fn(u8) -> &'static str) -> Self {
        Self { token, describe }
    }
}

impl fmt::Display for Found {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self.token {
                Some(t) => (self.describe)(t),
                None => "end of input",
            }
        )
    }
}

/// Utility type for displaying "expected" items in a parser expectation error.
struct Expected<'a> {
    /// The set of expected items.
    items: &'a [&'static str],
}

impl<'a> Expected<'a> {
    /// Constructs a new `Expected`.
    fn new(items: &'a [&'static str]) -> Self {
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

/// Represents a parse error.
#[derive(thiserror::Error, Diagnostic, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A lexer error occurred.
    #[error("{error}")]
    Lexer {
        /// The lexer error that occurred.
        error: lexer::Error,
        /// The span where the error occurred.
        #[label(primary, "{text}")]
        span: SourceSpan,
        /// The error text corresponding to the span.
        text: &'static str,
    },
    /// An unexpected token was encountered when a single item was expected.
    #[error("expected {expected}, but found {found}", found = Found::new(*.found, *.describe))]
    Expected {
        /// The expected item.
        expected: &'static str,
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label(primary, "unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
    },
    /// An unexpected token was encountered when one of multiple items was
    /// expected.
    #[error("expected {expected}, but found {found}", expected = Expected::new(.expected), found = Found::new(*.found, *.describe))]
    ExpectedOneOf {
        /// The expected items.
        expected: &'static [&'static str],
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label(primary, "unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
    },
    /// An unsupported WDL document version was encountered.
    #[error("unsupported WDL version `{version}`")]
    UnsupportedVersion {
        /// The version that was not supported.
        version: String,
        /// The span of the unsupported version.
        #[label(primary, "this version of WDL is not supported")]
        span: SourceSpan,
    },
    /// A WDL document must start with a version statement.
    #[error("a WDL document must start with a version statement")]
    VersionRequired {
        /// The span where the version statement must precede.
        #[label(primary, "a version statement must come before this")]
        span: Option<SourceSpan>,
    },
    /// A placeholder was encountered in a metadata string.
    #[error("a metadata string cannot contain a placeholder")]
    MetadataStringPlaceholder {
        /// The span where the string placeholder was encountered.
        #[label(primary, "consider escaping this placeholder")]
        span: SourceSpan,
    },
    /// An unterminated placeholder was encountered.
    #[error("an unterminated string was encountered")]
    UnterminatedString {
        /// The span of the opening quote of the string.
        #[label(primary, "this quote is not matched")]
        span: SourceSpan,
    },
    /// An unmatched brace was encountered.
    #[error("expected `}}`, but found {found}", found = Found::new(*.found, *.describe))]
    UnmatchedBrace {
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label(primary, "unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
        /// The span of the opening brace.
        #[label("this brace is not matched")]
        opening: SourceSpan,
    },
    /// An unmatched bracket was encountered.
    #[error("expected `]`, but found {found}", found = Found::new(*.found, *.describe))]
    UnmatchedBracket {
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label("unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
        /// The span of the opening bracket.
        #[label(primary, "this bracket is not matched")]
        opening: SourceSpan,
    },
    /// An unmatched placeholder was encountered.
    #[error("expected `}}`, but found {found}", found = Found::new(*.found, *.describe))]
    UnmatchedPlaceholder {
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label("unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
        /// The span of the opening bracket.
        #[label(primary, "this placeholder opening is not matched")]
        opening: SourceSpan,
    },
    /// An unmatched parenthesis was encountered.
    #[error("expected `)`, but found {found}", found = Found::new(*.found, *.describe))]
    UnmatchedParen {
        /// The found raw token (`None` for end of input).
        found: Option<u8>,
        /// The span of the found token.
        #[label("unexpected {found}", found = Found::new(*.found, *.describe))]
        span: SourceSpan,
        /// The function used to describe the raw token.
        describe: fn(u8) -> &'static str,
        /// The span of the opening bracket.
        #[label(primary, "this parenthesis is not matched")]
        opening: SourceSpan,
    },
}

/// A trait implemented by parser tokens.
pub trait ParserToken<'a>:
    Eq + Copy + Logos<'a, Source = str, Error = lexer::Error, Extras = ()>
{
    /// Converts the token into its syntax representation.
    fn into_syntax(self) -> SyntaxKind;

    /// Converts the token into its "raw" representation.
    fn into_raw(self) -> u8;

    /// Converts from a raw token into the parser token.
    fn from_raw(token: u8) -> Self;

    /// Describes a raw token.
    fn describe(token: u8) -> &'static str;

    /// Determines if the token is trivia that should be skipped over
    /// by the parser.
    ///
    /// Trivia tokens are still added to the concrete syntax tree.
    fn is_trivia(self) -> bool;
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
    /// The parser errors.
    errors: Vec<Error>,
}

impl<'a, T> Interpolator<'a, T>
where
    T: Logos<'a, Source = str, Error = lexer::Error, Extras = ()> + Copy,
{
    /// Adds an event to the parser event list.
    pub fn event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Adds an error to the parser error list.
    pub fn error(&mut self, error: Error) {
        self.errors.push(error);
    }

    /// Starts a new node event.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::NodeStarted {
            kind: SyntaxKind::Abandoned,
            forward_parent: None,
        });
        Marker::new(pos)
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
            errors: self.errors,
        }
    }
}

impl<'a, T> Iterator for Interpolator<'a, T>
where
    T: Logos<'a, Error = lexer::Error, Extras = ()> + Copy,
{
    type Item = (LexerResult<T>, SourceSpan);

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
    /// The parser errors.
    pub errors: Vec<Error>,
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
    /// The errors encountered so far.
    errors: Vec<Error>,
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
            errors: Default::default(),
        }
    }

    /// Gets the current span of the parser.
    pub fn span(&self) -> SourceSpan {
        self.lexer
            .as_ref()
            .map(|l| l.span())
            .unwrap_or(SourceSpan::new(0.into(), 0))
    }

    /// Gets the source being parsed at the given span.
    pub fn source(&self, span: SourceSpan) -> &'a str {
        self.lexer.as_ref().expect("expected a lexer").source(span)
    }

    /// Peeks at the next token (i.e. lookahead 1) from the lexer without
    /// consuming it.
    ///
    /// The token is not added to the event list.
    pub fn peek(&mut self) -> Option<(T, SourceSpan)> {
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
    /// The tokens are not added to the event list.
    pub fn peek2(&mut self) -> Option<((T, SourceSpan), (T, SourceSpan))> {
        let first = self.peek()?;

        // We have to clone the lexer here since it only supports a single lookahead.
        // The clone is cheap, but it does mean we'll re-tokenize this second lookahead
        // eventually.
        let mut lexer = self.lexer.clone()?;
        lexer
            .next()
            .unwrap()
            .0
            .expect("should have peeked at a valid token");
        while let Some((Ok(token), span)) = lexer.peek() {
            if token.is_trivia() {
                // Do not consume trivia here as we're between peeked tokens.
                continue;
            }

            return Some((first, (token, span)));
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

    /// Parses a delimited list of nodes via a callback.
    ///
    /// The parsing stops when it encounters the `until` token.
    pub fn delimited<F>(
        &mut self,
        delimiter: Option<T>,
        until: TokenSet,
        recovery: TokenSet,
        mut cb: F,
    ) where
        F: FnMut(&mut Self, Marker) -> Result<(), (Marker, Error)>,
    {
        let recovery = if let Some(delimiter) = delimiter {
            recovery
                .union(until)
                .union(TokenSet::new(&[delimiter.into_raw()]))
        } else {
            recovery.union(until)
        };

        let mut next: Option<(T, SourceSpan)> = self.peek();
        while let Some((token, _)) = next {
            if until.contains(token.into_raw()) {
                break;
            }

            let marker = self.start();
            if let Err((marker, e)) = cb(self, marker) {
                self.error(e);
                self.recover(recovery);
                marker.abandon(self);
            }

            next = self.peek();

            if let Some(delimiter) = delimiter {
                if let Some((token, _)) = next {
                    if until.contains(token.into_raw()) {
                        break;
                    }

                    if let Err(e) = self.expect(delimiter) {
                        self.error(e);
                        self.recover(recovery);
                    }

                    next = self.peek();
                }
            }
        }
    }

    /// Adds an error event to the event list.
    pub fn error(&mut self, error: Error) {
        self.errors.push(error);
    }

    /// Recovers from an error by consuming all tokens not
    /// in the given token set.
    pub fn recover(&mut self, tokens: TokenSet) {
        while let Some((token, _)) = self.peek() {
            if tokens.contains(token.into_raw()) {
                break;
            }

            self.next().unwrap();
        }
    }

    /// Starts a new node event.
    pub fn start(&mut self) -> Marker {
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
    pub fn require(&mut self, token: T) -> SourceSpan {
        match self.next() {
            Some((t, span)) if t == token => span,
            _ => panic!(
                "lexer not at required token {token}",
                token = T::describe(token.into_raw())
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
                let found = found.map(|(t, _)| t.into_raw());
                panic!(
                    "unexpected token {found}",
                    found = Found::new(found, T::describe)
                );
            }
        }
    }

    /// Expects the next token to be the given token.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect(&mut self, token: T) -> Result<SourceSpan, Error> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            Some((t, span)) => Err(Error::Expected {
                expected: T::describe(token.into_raw()),
                found: Some(t.into_raw()),
                span,
                describe: T::describe,
            }),
            None => Err(Error::Expected {
                expected: T::describe(token.into_raw()),
                found: None,
                span: self.span(),
                describe: T::describe,
            }),
        }
    }

    /// Expects the next token to be the given token, but uses
    /// the provided name in the error.
    ///
    /// Returns an error if the token is not the given token.
    pub fn expect_with_name(&mut self, token: T, name: &'static str) -> Result<SourceSpan, Error> {
        match self.peek() {
            Some((t, span)) if t == token => {
                self.next();
                Ok(span)
            }
            Some((t, span)) => Err(Error::Expected {
                expected: name,
                found: Some(t.into_raw()),
                span,
                describe: T::describe,
            }),
            None => Err(Error::Expected {
                expected: name,
                found: None,
                span: self.span(),
                describe: T::describe,
            }),
        }
    }

    /// Expects the next token to be in the given token set.
    ///
    /// Returns an error if the token is not the given set.
    pub fn expect_in(
        &mut self,
        tokens: TokenSet,
        expected: &'static [&'static str],
    ) -> Result<(T, SourceSpan), Error> {
        match self.peek() {
            Some((t, span)) if tokens.contains(t.into_raw()) => {
                self.next();
                Ok((t, span))
            }
            Some((t, span)) => Err(Error::ExpectedOneOf {
                expected,
                found: Some(t.into_raw()),
                span,
                describe: T::describe,
            }),
            None => Err(Error::ExpectedOneOf {
                expected,
                found: None,
                span: self.span(),
                describe: T::describe,
            }),
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
        T2: Logos<'a, Source = str, Error = lexer::Error, Extras = ()> + Copy,
        F: FnOnce(Interpolator<'a, T2>) -> (Parser<'a, T>, R),
    {
        let input = Interpolator {
            lexer: std::mem::take(&mut self.lexer)
                .expect("lexer should exist")
                .morph(),
            events: std::mem::take(&mut self.events),
            errors: std::mem::take(&mut self.errors),
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
            errors: self.errors,
        }
    }

    /// Consumes the parser and returns an interpolator.
    pub fn into_interpolator<T2>(self) -> Interpolator<'a, T2>
    where
        T2: Logos<'a, Source = str, Error = lexer::Error, Extras = ()> + Copy,
    {
        Interpolator {
            lexer: self.lexer.expect("lexer should be present").morph(),
            events: self.events,
            errors: self.errors,
        }
    }

    /// Consumes the parser and returns the output.
    pub fn finish(self) -> Output<'a, T> {
        Output {
            lexer: self.lexer.expect("lexer should be present"),
            events: self.events,
            errors: self.errors,
        }
    }

    /// Consumes any trivia tokens by adding them to the event list.
    fn consume_trivia(
        &mut self,
        res: LexerResult<T>,
        span: SourceSpan,
        peeked: bool,
    ) -> Option<(T, SourceSpan)> {
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
            Err(e) => {
                self.error(Error::Lexer {
                    error: e,
                    span,
                    text: Self::unsupported_token_text(self.source(span)),
                });
                Event::Token {
                    kind: SyntaxKind::Unknown,
                    span,
                }
            }
        };

        if peeked {
            self.lexer.as_mut().expect("should have a lexer").next();
        }

        self.events.push(event);
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
    type Item = (T, SourceSpan);

    fn next(&mut self) -> Option<(T, SourceSpan)> {
        while let Some((res, span)) = self.lexer.as_mut()?.next() {
            if let Some((token, span)) = self.consume_trivia(res, span, false) {
                self.events.push(Event::Token {
                    kind: token.into_syntax(),
                    span,
                });
                return Some((token, span));
            }
        }

        None
    }
}
