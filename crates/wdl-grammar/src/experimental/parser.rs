//! Module for the parser implementation.
//!
//! The parser consumes a token stream from the lexer and produces
//! a stream of parser events that can be used to construct a CST.
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
    NodeStarted(SyntaxKind),

    /// A node has finished.
    NodeFinished,

    /// A token was encountered.
    Token {
        /// The syntax kind of the token.
        kind: SyntaxKind,
        /// The source span of the token.
        span: SourceSpan,
    },

    /// An error was encountered.
    Error(Error),
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

/// Utility type for displaying "expected" token sets in a parser expectation
/// error.
struct Expected {
    /// The set of expected tokens.
    set: TokenSet,
    /// The function used to describe a raw token.
    describe: fn(u8) -> &'static str,
}

impl Expected {
    /// Constructs a new `Expected`.
    fn new(set: TokenSet, describe: fn(u8) -> &'static str) -> Self {
        Self { set, describe }
    }
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.set.count();
        for (i, token) in self.set.iter().enumerate() {
            if i > 0 {
                if count == 2 {
                    write!(f, " or ")?;
                } else if i == count - 1 {
                    write!(f, ", or ")?;
                } else {
                    write!(f, ", ")?;
                }
            }

            write!(f, "{}", (self.describe)(token))?;
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
        #[label(primary, "this is not a WDL token")]
        span: SourceSpan,
    },
    /// An unexpected token was encountered when a single token was expected.
    #[error("expected {expected}, but found {found}", expected = Expected::new(*.expected, *.describe), found = Found::new(*.found, *.describe))]
    Expected {
        /// The expected token set.
        expected: TokenSet,
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
    pub fn complete<'a, T>(self, parser: &mut Parser<'a, T>, kind: SyntaxKind)
    where
        T: ParserToken<'a>,
    {
        // Update the node kind and push a finished event
        match &mut parser.events[self.0] {
            Event::NodeStarted(k) => {
                *k = kind;
            }
            _ => unreachable!(),
        }

        parser.events.push(Event::NodeFinished);
        std::mem::forget(self);
    }

    /// Abandons the node due to an error.
    pub fn abandon<'a, T>(self, parser: &mut Parser<'a, T>)
    where
        T: ParserToken<'a>,
    {
        // If the current node has no children, just pop it from the event list
        if self.0 == parser.events.len() - 1 {
            match parser.events.pop() {
                Some(Event::NodeStarted(SyntaxKind::Abandoned)) => (),
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
    lexer: Lexer<'a, T>,
    /// The events produced by the parser.
    events: Vec<Event>,
}

impl<'a, T> Parser<'a, T>
where
    T: ParserToken<'a>,
{
    /// Construct a new parser from the given lexer.
    pub fn new(lexer: Lexer<'a, T>) -> Self {
        Self {
            lexer,
            events: Default::default(),
        }
    }

    /// Gets the current span of the parser.
    pub fn span(&self) -> SourceSpan {
        self.lexer.span()
    }

    /// Peeks at the next token from the lexer without consuming it.
    ///
    /// The token is not added to the event list.
    pub fn peek(&mut self) -> Option<(T, SourceSpan)> {
        while let Some((res, span)) = self.lexer.peek() {
            if let Some(t) = self.consume_trivia(res, span, true) {
                return Some(t);
            }
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
    /// The parsing stops when it encounters the `until` token or if
    /// the callback returns `Ok(false)`.
    pub fn delimited<F>(
        &mut self,
        delimiter: Option<T>,
        until: TokenSet,
        recovery: TokenSet,
        mut cb: F,
    ) where
        F: FnMut(&mut Self, Marker) -> Result<bool, (Marker, Error)>,
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
            match cb(self, marker) {
                Ok(true) => {}
                Ok(false) => break,
                Err((marker, e)) => {
                    self.error(e);
                    self.recover(recovery);
                    marker.abandon(self);
                }
            }

            if let Some(delimiter) = delimiter {
                self.next_if(delimiter);
            }

            next = self.peek();
        }
    }

    /// Adds an error event to the event list.
    pub fn error(&mut self, error: Error) {
        self.events.push(Event::Error(error));
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
        self.events.push(Event::NodeStarted(SyntaxKind::Abandoned));
        Marker::new(pos)
    }

    /// Requires that the current token is the given token.
    ///
    /// Panics if the token is not the given token.
    pub fn require(&mut self, token: T) {
        match self.next() {
            Some((t, _)) if t == token => {}
            _ => panic!(
                "lexer not at required token {token}",
                token = T::describe(token.into_raw())
            ),
        }
    }

    /// Requires that the current token is in the given token set.
    ///
    /// Panics if the token is not in the token set.
    pub fn require_in(&mut self, tokens: TokenSet) {
        match self.next() {
            Some((t, _)) if tokens.contains(t.into_raw()) => {}
            _ => panic!(
                "expected {expected}",
                expected = Expected::new(tokens, T::describe),
            ),
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
                expected: TokenSet::new(&[token.into_raw()]),
                found: Some(t.into_raw()),
                span,
                describe: T::describe,
            }),
            None => Err(Error::Expected {
                expected: TokenSet::new(&[token.into_raw()]),
                found: None,
                span: self.span(),
                describe: T::describe,
            }),
        }
    }

    /// Expects the next token to be in the given token set.
    ///
    /// Returns an error if the token is not the given set.
    pub fn expect_in(&mut self, tokens: TokenSet) -> Result<(T, SourceSpan), Error> {
        match self.peek() {
            Some((t, span)) if tokens.contains(t.into_raw()) => {
                self.next();
                Ok((t, span))
            }
            Some((t, span)) => Err(Error::Expected {
                expected: tokens,
                found: Some(t.into_raw()),
                span,
                describe: T::describe,
            }),
            None => Err(Error::Expected {
                expected: tokens,
                found: None,
                span: self.span(),
                describe: T::describe,
            }),
        }
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
            lexer: self.lexer.morph(),
            events: self.events,
        }
    }

    /// Consumes the parser and returns the list of parser events.
    pub fn into_events(self) -> Vec<Event> {
        self.events
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
            Err(e) => Event::Error(Error::Lexer { error: e, span }),
        };

        if peeked {
            self.lexer.next();
        }

        self.events.push(event);
        None
    }
}

impl<'a, T> Iterator for Parser<'a, T>
where
    T: ParserToken<'a>,
{
    type Item = (T, SourceSpan);

    fn next(&mut self) -> Option<(T, SourceSpan)> {
        while let Some((res, span)) = self.lexer.next() {
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
