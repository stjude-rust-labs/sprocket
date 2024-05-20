//! Module for the lexer implementation.

use logos::Logos;
use miette::SourceSpan;

use super::parser::ParserToken;
use super::tree::SyntaxKind;

pub mod v1;

/// Converts a logos `Span` into a miette `SourceSpan`.
fn to_source_span(span: logos::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.end - span.start)
}

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
#[logos(error = Error)]
pub enum PreambleToken {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\n]*")]
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

// There can only be 128 tokens in a TokenSet.
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

    fn describe(token: u8) -> &'static str {
        match Self::from_raw(token) {
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
#[logos(error = Error)]
pub enum VersionStatementToken {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\n]*")]
    Comment,

    /// A WDL version.
    #[regex(r"[a-zA-Z0-9][a-zA-Z0-9.\-]*")]
    Version,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum token value.
    MAX,
}

// There can only be 128 tokens in a TokenSet.
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

    fn describe(token: u8) -> &'static str {
        match Self::from_raw(token) {
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

/// Represents a lexer error.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Error {
    /// An unknown token was encountered.
    #[default]
    #[error("an unknown token was encountered")]
    UnknownToken,
}

/// The result type for the lexer.
pub type LexerResult<T> = Result<T, Error>;

/// Implements a WDL lexer.
///
/// A lexer produces a stream of tokens from a WDL source string.
#[allow(missing_debug_implementations)]
pub struct Lexer<'a, T>(logos::Lexer<'a, T>)
where
    T: Logos<'a>;

impl<'a, T> Lexer<'a, T>
where
    T: Logos<'a, Source = str, Error = Error, Extras = ()> + Copy,
{
    /// Creates a new lexer for the given source string.
    pub fn new(source: &'a str) -> Self
    where
        T::Extras: Default,
    {
        Self(T::lexer(source))
    }

    /// Gets the source string of the given span.
    pub fn source(&self, span: SourceSpan) -> &'a str {
        &self.0.source()[span.offset()..span.offset() + span.len()]
    }

    /// Gets the length of the source.
    pub fn source_len(&self) -> usize {
        self.0.source().len()
    }

    /// Gets the current span of the lexer.
    pub fn span(&self) -> SourceSpan {
        let mut span = self.0.span();
        if span.end == self.source_len() {
            // miette doesn't support placing a highlight at
            // the end of the input, so use the last valid
            // byte in the source
            span.start -= 1;
            span.end = span.start + 1;
        }

        to_source_span(span)
    }

    /// Peeks at the next token.
    pub fn peek(&mut self) -> Option<(LexerResult<T>, SourceSpan)> {
        let mut lexer = self.0.clone();
        lexer.next().map(|r| (r, to_source_span(lexer.span())))
    }

    /// Morph this lexer into a lexer for a new token type.
    ///
    /// The returned lexer continues to point at the same span
    /// as the current lexer.
    pub fn morph<T2>(self) -> Lexer<'a, T2>
    where
        T2: Logos<'a, Source = str, Error = Error>,
        T::Extras: Into<T2::Extras>,
    {
        Lexer(self.0.morph())
    }
}

impl<'a, T> Iterator for Lexer<'a, T>
where
    T: Logos<'a, Error = Error>,
{
    type Item = (LexerResult<T>, SourceSpan);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|r| (r, to_source_span(self.0.span())))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    pub(crate) fn map<T>(
        (t, s): (LexerResult<T>, SourceSpan),
    ) -> (LexerResult<T>, std::ops::Range<usize>) {
        (t, s.offset()..s.offset() + s.len())
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
