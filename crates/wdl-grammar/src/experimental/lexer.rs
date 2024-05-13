//! Module for the lexer implementation.

use logos::Logos;
use miette::SourceSpan;

pub mod v1;

/// Converts a logos `Span` into a miette `SourceSpan`.
fn to_source_span(span: logos::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.end - span.start)
}

/// Represents a token for lexing version directives in WDL documents.
///
/// A WDL parser may initially use this token to lex the `version`
/// directive at the start of a WDL document.
///
/// Once the version directive has been parsed, the parser will then
/// [morph][Lexer::morph] the lexer to the appropriate token for the
/// document's WDL version and pass the lexer to the matching version
/// of the WDL grammar.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
pub enum VersionToken {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\n]*")]
    Comment,

    /// The `version` keyword.
    #[token("version")]
    VersionKeyword,

    /// A supported WDL version.
    #[regex(r"[a-zA-Z0-9][a-zA-Z0-9.\-]*")]
    Version,
}

/// Represents a lexer error.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Error {
    /// An unexpected token was encountered.
    #[default]
    #[error("an unexpected token was encountered")]
    UnexpectedToken,
}

/// The result type for the lexer.
pub type LexerResult<T> = Result<T, Error>;

/// Implements a WDL lexer.
///
/// A lexer produces a stream of tokens from a WDL source string.
#[allow(missing_debug_implementations)]
pub struct Lexer<'a, T>
where
    T: Logos<'a>,
{
    /// The inner lexer.
    lexer: logos::Lexer<'a, T>,
    /// The stored peeked result [see `Peek`][Self::peek].
    peeked: Option<Option<(LexerResult<T>, SourceSpan)>>,
}

impl<'a, T> Lexer<'a, T>
where
    T: Logos<'a, Source = str, Error = Error> + Copy,
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
    pub fn source(&self, span: SourceSpan) -> &'a str {
        &self.lexer.source()[span.offset()..span.offset() + span.len()]
    }

    /// Gets the length of the source.
    pub fn source_len(&self) -> usize {
        self.lexer.source().len()
    }

    /// Gets the current span of the lexer.
    pub fn span(&self) -> SourceSpan {
        let mut span = self.lexer.span();
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
        *self.peeked.get_or_insert_with(|| {
            self.lexer
                .next()
                .map(|r| (r, to_source_span(self.lexer.span())))
        })
    }

    /// Morph this lexer into a lexer for a new token type.
    ///
    /// The returned lexer continues to point at the same span
    /// as the current lexer.
    ///
    /// # Panics
    ///
    /// This method will panic if the current lexer was peeked without
    /// consuming the peeked token.
    pub fn morph<T2>(self) -> Lexer<'a, T2>
    where
        T2: Logos<'a, Source = T::Source>,
        T::Extras: Into<T2::Extras>,
    {
        assert!(
            self.peeked.is_none(),
            "cannot morph a lexer without consuming a peeked token"
        );

        Lexer {
            lexer: self.lexer.morph(),
            peeked: None,
        }
    }
}

impl<'a, T> Iterator for Lexer<'a, T>
where
    T: Logos<'a, Source = str, Error = Error>,
{
    type Item = (LexerResult<T>, SourceSpan);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(peeked) = self.peeked.take() {
            peeked
        } else {
            self.lexer
                .next()
                .map(|r| (r, to_source_span(self.lexer.span())))
        }
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
        use VersionToken::*;
        let lexer = Lexer::<VersionToken>::new(
            "
# Test for 1.0 documents
version 1.0",
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Comment), 1..25),
                (Ok(Whitespace), 25..26),
                (Ok(VersionKeyword), 26..33),
                (Ok(Whitespace), 33..34),
                (Ok(Version), 34..37)
            ],
            "produced tokens did not match the expected set"
        );
    }

    #[test]
    fn test_version_1_1() {
        use VersionToken::*;
        let lexer = Lexer::<VersionToken>::new(
            "
# Test for 1.1 documents
version 1.1",
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Comment), 1..25),
                (Ok(Whitespace), 25..26),
                (Ok(VersionKeyword), 26..33),
                (Ok(Whitespace), 33..34),
                (Ok(Version), 34..37)
            ],
            "produced tokens did not match the expected set"
        );
    }

    #[test]
    fn test_version_draft3() {
        use VersionToken::*;
        // Note: draft-3 documents aren't supported by `wdl`, but
        // the lexer needs to ensure it can lex any valid version
        // token so that the parser may gracefully reject parsing
        // the document.
        let lexer = Lexer::<VersionToken>::new(
            "
# Test for draft-3 documents
version draft-3",
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Comment), 1..29),
                (Ok(Whitespace), 29..30),
                (Ok(VersionKeyword), 30..37),
                (Ok(Whitespace), 37..38),
                (Ok(Version), 38..45),
            ],
            "produced tokens did not match the expected set"
        );
    }
}
