//! Module for the lexer implementation.

use logos::Logos;

use super::parser::ParserToken;
use super::tree::SyntaxKind;
use super::Span;

pub mod v1;

/// Represents a set of tokens as a bitset.
///
/// As Rust does not currently support const functions in traits,
/// `TokenSet` operates on "raw" forms of tokens (i.e. `u8`).
///
/// This allows `TokenSet` to work with different token types but also
/// allow for the sets to be created in const contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenSet(u128);

impl TokenSet {
    /// An empty token set.
    pub const EMPTY: Self = Self(0);

    /// Constructs a token set from a slice of tokens.
    pub const fn new(tokens: &[u8]) -> Self {
        let mut bits = 0u128;
        let mut i = 0;
        while i < tokens.len() {
            bits |= Self::mask(tokens[i]);
            i += 1;
        }
        Self(bits)
    }

    /// Unions two token sets together.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Checks if the token is contained in the set.
    pub const fn contains(&self, token: u8) -> bool {
        self.0 & Self::mask(token) != 0
    }

    /// Gets the count of tokens in the set.
    pub const fn count(&self) -> usize {
        self.0.count_ones() as usize
    }

    /// Iterates the raw tokens in the set.
    pub fn iter(&self) -> impl Iterator<Item = u8> {
        let mut bits = self.0;
        std::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }

            let token = u8::try_from(bits.trailing_zeros())
                .expect("the maximum token value should be less than 128");

            bits ^= bits & bits.overflowing_neg().0;
            Some(token)
        })
    }

    /// Masks the given token to a `u128`.
    const fn mask(token: u8) -> u128 {
        1u128 << (token as usize)
    }
}

/// Represents a token for lexing WDL document preambles.
///
/// A WDL parser may initially use this token to lex the version
/// statement at the start of a WDL document.
///
/// Once the version statement has been parsed, the parser will then
/// [morph][Lexer::morph] the lexer to the appropriate token for the
/// document's WDL version and pass the lexer to the matching version
/// of the WDL grammar.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PreambleToken {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\r\n]*")]
    Comment,

    /// The `version` keyword.
    #[token("version")]
    VersionKeyword,

    /// Any other token that isn't whitespace, comment, or the `version`
    /// keyword.
    #[regex("[^ \t\r\n#]")]
    Any,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum token value.
    MAX,
}

/// Asserts that PreambleToken can fit in a TokenSet.
const _: () = assert!(PreambleToken::MAX as u8 <= 128);

impl<'a> ParserToken<'a> for PreambleToken {
    fn into_syntax(self) -> SyntaxKind {
        match self {
            Self::Whitespace => SyntaxKind::Whitespace,
            Self::Comment => SyntaxKind::Comment,
            Self::VersionKeyword => SyntaxKind::VersionKeyword,
            Self::Any | Self::MAX => unreachable!(),
        }
    }

    fn into_raw(self) -> u8 {
        self as u8
    }

    fn from_raw(token: u8) -> Self {
        assert!(token < Self::MAX as u8, "invalid token value");
        unsafe { std::mem::transmute(token) }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Whitespace => "whitespace",
            Self::Comment => "comment",
            Self::VersionKeyword => "`version` keyword",
            Self::Any | Self::MAX => unreachable!(),
        }
    }

    fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
    }
}

/// Represents a token for lexing WDL version statements.
///
/// This exists as a separate token type because WDL versions and
/// identifiers overlap on their regex.
///
/// Therefore, version statements are tokenized separately from the rest
/// of the WDL document.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum VersionStatementToken {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\r\n]*")]
    Comment,

    /// A WDL version.
    #[regex(r"[a-zA-Z0-9][a-zA-Z0-9.\-]*")]
    Version,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum token value.
    MAX,
}

/// Asserts that VersionStatementToken can fit in a TokenSet.
const _: () = assert!(VersionStatementToken::MAX as u8 <= 128);

impl<'a> ParserToken<'a> for VersionStatementToken {
    fn into_syntax(self) -> SyntaxKind {
        match self {
            Self::Whitespace => SyntaxKind::Whitespace,
            Self::Comment => SyntaxKind::Comment,
            Self::Version => SyntaxKind::Version,
            Self::MAX => unreachable!(),
        }
    }

    fn into_raw(self) -> u8 {
        self as u8
    }

    fn from_raw(token: u8) -> Self {
        assert!(token < Self::MAX as u8, "invalid token value");
        unsafe { std::mem::transmute(token) }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Whitespace => "whitespace",
            Self::Comment => "comment",
            Self::Version => "version",
            Self::MAX => unreachable!(),
        }
    }

    fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
    }
}

/// The result type for the lexer.
pub type LexerResult<T> = Result<T, ()>;

/// Records information for a lexer peek operation.
///
/// See the [Lexer::peek] method.
#[derive(Debug, Clone, Copy)]
struct Peeked<T> {
    /// The result of the peek operation.
    result: LexerResult<T>,
    /// The span of the result.
    span: Span,
    /// The offset *before* the peek.
    ///
    /// This is used to discard the peek for morphing lexers.
    offset: usize,
}

/// Implements a WDL lexer.
///
/// A lexer produces a stream of tokens from a WDL source string.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Lexer<'a, T>
where
    T: Logos<'a, Extras = ()>,
{
    /// The underlying logos lexer.
    lexer: logos::Lexer<'a, T>,
    /// The peeked token.
    peeked: Option<Peeked<T>>,
}

impl<'a, T> Lexer<'a, T>
where
    T: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
{
    /// Creates a new lexer for the given source string.
    pub fn new(source: &'a str) -> Self
    where
        T::Extras: Default,
    {
        Self {
            lexer: T::lexer(source),
            peeked: None,
        }
    }

    /// Gets the source string of the given span.
    pub fn source(&self, span: Span) -> &'a str {
        &self.lexer.source()[span.start()..span.end()]
    }

    /// Gets the length of the source.
    pub fn source_len(&self) -> usize {
        self.lexer.source().len()
    }

    /// Gets the current span of the lexer.
    pub fn span(&self) -> Span {
        self.lexer.span().into()
    }

    /// Peeks at the next token.
    pub fn peek(&mut self) -> Option<(LexerResult<T>, Span)> {
        if self.peeked.is_none() {
            let offset = self.lexer.span().start;
            self.peeked = self.lexer.next().map(|r| Peeked {
                result: r,
                span: self.lexer.span().into(),
                offset,
            });
        }

        self.peeked.map(|p| (p.result, p.span))
    }

    /// Morph this lexer into a lexer for a new token type.
    ///
    /// The returned lexer continues to point at the same span
    /// as the current lexer.
    pub fn morph<T2>(self) -> Lexer<'a, T2>
    where
        T2: Logos<'a, Source = str, Error = (), Extras = ()> + Copy,
    {
        // If the lexer has peeked, we need to "reset" the lexer so that it is no longer
        // peeked; this allows the morphed lexer to lex the previously peeked
        // span
        let lexer = match self.peeked {
            Some(peeked) => {
                let mut lexer = T2::lexer(self.lexer.source());
                if peeked.offset > 0 {
                    lexer.bump(peeked.offset);
                    lexer.next();
                }

                lexer
            }
            None => self.lexer.morph(),
        };

        Lexer {
            lexer,
            peeked: None,
        }
    }

    /// Consumes the remainder of the source, returning the span
    /// of the consumed text.
    pub fn consume_remainder(&mut self) -> Option<Span> {
        // Reset the lexer if we've peeked
        if let Some(peeked) = self.peeked.take() {
            self.lexer = T::lexer(self.lexer.source());
            if peeked.offset > 0 {
                self.lexer.bump(peeked.offset);
                self.lexer.next();
            }
        }

        // Bump the remaining source
        self.lexer.next();
        self.lexer.bump(self.lexer.remainder().len());
        let span = self.lexer.span();
        assert!(self.next().is_none(), "lexer should be completed");
        if span.is_empty() {
            None
        } else {
            Some(span.into())
        }
    }
}

impl<'a, T> Iterator for Lexer<'a, T>
where
    T: Logos<'a, Error = (), Extras = ()> + Copy,
{
    type Item = (LexerResult<T>, Span);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(peeked) = self.peeked.take() {
            return Some((peeked.result, peeked.span));
        }

        self.lexer.next().map(|r| (r, self.lexer.span().into()))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    pub(crate) fn map<T>(
        (t, s): (LexerResult<T>, Span),
    ) -> (LexerResult<T>, std::ops::Range<usize>) {
        (t, s.start()..s.end())
    }

    #[test]
    fn test_version_1_0() {
        let mut lexer = Lexer::<PreambleToken>::new(
            "
# Test for 1.0 documents
version 1.0",
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 0..1)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Comment), 1..25),
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 25..26),
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::VersionKeyword), 26..33),
        );

        let mut lexer: Lexer<'_, VersionStatementToken> = lexer.morph();
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Whitespace), 33..34),
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Version), 34..37)
        );
    }

    #[test]
    fn test_version_1_1() {
        let mut lexer = Lexer::<PreambleToken>::new(
            "
# Test for 1.1 documents
version 1.1",
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 0..1)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Comment), 1..25)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 25..26)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::VersionKeyword), 26..33)
        );

        let mut lexer: Lexer<'_, VersionStatementToken> = lexer.morph();
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Whitespace), 33..34)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Version), 34..37)
        );
    }

    #[test]
    fn test_version_draft3() {
        // Note: draft-3 documents aren't supported by `wdl`, but
        // the lexer needs to ensure it can lex any valid version
        // token so that the parser may gracefully reject parsing
        // the document.
        let mut lexer = Lexer::<PreambleToken>::new(
            "
# Test for draft-3 documents
version draft-3",
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 0..1)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Comment), 1..29)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::Whitespace), 29..30)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(PreambleToken::VersionKeyword), 30..37)
        );

        let mut lexer: Lexer<'_, VersionStatementToken> = lexer.morph();
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Whitespace), 37..38)
        );
        assert_eq!(
            lexer.next().map(map).unwrap(),
            (Ok(VersionStatementToken::Version), 38..45)
        );
    }
}
