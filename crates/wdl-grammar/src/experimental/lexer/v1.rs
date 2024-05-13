//! WDL (v1) tokens.

use std::fmt;

use logos::Logos;

use super::Error;

/// Represents a token in a single quoted string (e.g. `'hello'`).
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
pub enum SQStringToken {
    /// A start of a placeholder.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("~{")]
    #[token("${")]
    PlaceholderStart,

    /// The start of an escape sequence.
    ///
    /// This token is considered part of the literal text.
    ///
    /// Note that escape sequences are not validated by the lexer.
    #[regex(r"\\.")]
    Escape,

    /// A span of literal text.
    #[regex(r"[^\\$~']+")]
    Text,

    /// A dollar sign that is part of literal text.
    #[token("$")]
    DollarSign,

    /// A tilde that is part of the literal text.
    #[token("~")]
    Tilde,

    /// An ending single quote.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("'")]
    End,
}

/// Represents a token in a double quoted string (e.g. `"hello"`).
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
pub enum DQStringToken {
    /// A start of a placeholder.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("~{")]
    #[token("${")]
    PlaceholderStart,

    /// The start of an escape sequence.
    ///
    /// This token is considered part of the literal text.
    ///
    /// Note that escape sequences are not validated by the lexer.
    #[regex(r"\\.")]
    Escape,

    /// A span of literal text of the string.
    #[regex(r#"[^\\$~"]+"#)]
    Text,

    /// A dollar sign that is part of literal text.
    #[token("$")]
    DollarSign,

    /// A tilde that is part of the literal text.
    #[token("~")]
    Tilde,

    /// An ending double quote.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("\"")]
    End,
}

/// Represents a token in a heredoc command (e.g. `<<< hello >>>`).
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
pub enum HeredocCommandToken {
    /// A start of a placeholder.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("~{")]
    PlaceholderStart,

    /// The start of an escape sequence.
    ///
    /// This token is considered part of the literal text.
    ///
    /// Note that escape sequences are not validated by the lexer.
    #[regex(r"\\.")]
    Escape,

    /// A span of literal text.
    #[regex(r"[^\\~>]+")]
    Text,

    /// A tilde that is part of the literal text.
    #[token("~")]
    Tilde,

    /// An ending angle bracket.
    ///
    /// When three of these tokens are sequentially encountered,
    /// [morph][super::Lexer::morph] the lexer to use [Token].
    ///
    /// Otherwise, consider the token to be part of the
    /// literal text.
    #[token(">")]
    End,
}

/// Represents a token in an "older-style" brace command.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
pub enum BraceCommandToken {
    /// A start of a placeholder.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("~{")]
    #[token("${")]
    PlaceholderStart,

    /// The start of an escape sequence.
    ///
    /// This token is considered part of the literal text.
    ///
    /// Note that escape sequences are not validated by the lexer.
    #[regex(r"\\.")]
    Escape,

    /// A span of literal text.
    #[regex(r"[^\\$~}]+")]
    Text,

    /// A dollar sign that is part of literal text.
    #[token("$")]
    DollarSign,

    /// A tilde that is part of the literal text.
    #[token("~")]
    Tilde,

    /// An ending close brace.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token("}")]
    End,
}

/// Represents a WDL (v1) token.
///
/// As WDL supports string interpolation, sub-lexers are used when certain
/// tokens are encountered:
///
/// | Token                                                                    | Sub-lexer token       |
/// |--------------------------------------------------------------------------|-----------------------|
/// | [SQStringStart][Token::SQStringStart]                                    | [SQStringToken]       |
/// | [DQStringStart][Token::DQStringStart]                                    | [DQStringToken]       |
/// | [HeredocCommandStart][Token::HeredocCommandStart]                        | [HeredocCommandToken] |
/// | [CommandKeyword][Token::CommandKeyword] ~> [OpenBrace][Token::OpenBrace] | [BraceCommandToken]   |
///
/// After the start token is encountered, the [morph][super::Lexer::morph]
/// method is used to morph the current lexer into a sub-lexer.
///
/// When the sub-lexer token's `End` variant is encountered,
/// [morph][super::Lexer::morph] is called again to morph the sub-lexer back to
/// the WDL lexer using the [Token] type.
///
/// An unterminated string or heredoc can be determined by the lexer iterator
/// terminating before the sub-lexer token's `End` variant is encountered.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[logos(error = Error)]
#[logos(subpattern exp = r"[eE][+-]?[0-9]+")]
#[logos(subpattern id = r"[a-zA-Z][a-zA-Z0-9_]*")]
pub enum Token {
    /// Contiguous whitespace.
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// A comment.
    #[regex(r"#[^\n]*")]
    Comment,

    /// A literal float.
    #[regex(r"[0-9]+(?&exp)")]
    #[regex(r"[0-9]+\.[0-9]*(?&exp)?", priority = 5)]
    #[regex(r"[0-9]*\.[0-9]+(?&exp)?")]
    Float,

    /// A literal integer.
    #[token("0")]
    #[regex(r"[1-9][0-9]+")]
    #[regex(r"0[0-7]+")]
    #[regex(r"0[xX][0-9a-fA-F]+")]
    Integer,

    /// An identifier.
    #[regex(r"(?&id)")]
    Ident,

    /// A qualified name.
    #[regex(r"(?&id)(\.(?&id))+")]
    QualifiedName,

    /// A start of a single-quoted string.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use
    /// [SQStringToken].
    #[token("'")]
    SQStringStart,

    /// A start of a double-quoted string.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use
    /// [DQStringToken].
    #[token("\"")]
    DQStringStart,

    /// A start of a heredoc command.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use
    /// [HeredocCommandToken].
    #[token("<<<")]
    HeredocCommandStart,

    /// The `Array` type keyword.
    #[token("Array")]
    ArrayTypeKeyword,
    /// The `Boolean` type keyword.
    #[token("Boolean")]
    BooleanTypeKeyword,
    /// The `File` type keyword.
    #[token("File")]
    FileTypeKeyword,
    /// The `Float` type keyword.
    #[token("Float")]
    FloatTypeKeyword,
    /// The `Int` type keyword.
    #[token("Int")]
    IntTypeKeyword,
    /// The `Map` type keyword.
    #[token("Map")]
    MapTypeKeyword,
    /// The `None` type keyword.
    #[token("None")]
    NoneTypeKeyword,
    /// The `Object` type keyword.
    #[token("Object")]
    ObjectTypeKeyword,
    /// The `Pair` type keyword.
    #[token("Pair")]
    PairTypeKeyword,
    /// The `String` type keyword.
    #[token("String")]
    StringTypeKeyword,
    /// The `alias` keyword.
    #[token("alias")]
    AliasKeyword,
    /// The `as` keyword.
    #[token("as")]
    AsKeyword,
    /// The `call` keyword.
    #[token("call")]
    CallKeyword,
    /// The `command` keyword.
    #[token("command")]
    CommandKeyword,
    /// The `else` keyword.
    #[token("else")]
    ElseKeyword,
    /// The `false` keyword.
    #[token("false")]
    FalseKeyword,
    /// The `if` keyword.
    #[token("if")]
    IfKeyword,
    /// The `in` keyword.
    #[token("in")]
    InKeyword,
    /// The `import` keyword.
    #[token("import")]
    ImportKeyword,
    /// The `input` keyword.
    #[token("input")]
    InputKeyword,
    /// The `meta` keyword.
    #[token("meta")]
    MetaKeyword,
    /// The `null` keyword.
    #[token("null")]
    NullKeyword,
    /// The `object` keyword.
    #[token("object")]
    ObjectKeyword,
    /// The `output` keyword.
    #[token("output")]
    OutputKeyword,
    /// The `parameter_meta` keyword.
    #[token("parameter_meta")]
    ParameterMetaKeyword,
    /// The `runtime` keyword.
    #[token("runtime")]
    RuntimeKeyword,
    /// The `scatter` keyword.
    #[token("scatter")]
    ScatterKeyword,
    /// The `struct` keyword.
    #[token("struct")]
    StructKeyword,
    /// The `task` keyword.
    #[token("task")]
    TaskKeyword,
    /// The `then` keyword.
    #[token("then")]
    ThenKeyword,
    /// The `true` keyword.
    #[token("true")]
    TrueKeyword,
    /// The `version` keyword.
    #[token("version")]
    VersionKeyword,
    /// The `workflow` keyword.
    #[token("workflow")]
    WorkflowKeyword,

    /// The reserved `Directory` type keyword.
    #[token("Directory")]
    ReservedDirectoryTypeKeyword,
    /// The reserved `hints` keyword.
    #[token("hints")]
    ReservedHintsKeyword,
    /// The reserved `requirements` keyword.
    #[token("requirements")]
    ReservedRequirementsKeyword,

    /// The `{` symbol.
    #[token("{")]
    OpenBrace,
    /// The `}` symbol.
    #[token("}")]
    CloseBrace,
    /// The `[` symbol.
    #[token("[")]
    OpenBracket,
    /// The `]` symbol.
    #[token("]")]
    CloseBracket,
    /// The `=` symbol.
    #[token("=")]
    Assignment,
    /// The `:` symbol.
    #[token(":")]
    Colon,
    /// The `,` symbol.
    #[token(",")]
    Comma,
    /// The `(` symbol.
    #[token("(")]
    OpenParen,
    /// The `)` symbol.
    #[token(")")]
    CloseParen,
    /// The `?` symbol.
    #[token("?")]
    QuestionMark,
    /// The `!` symbol.
    #[token("!")]
    Exclamation,
    /// The `+` symbol.
    #[token("+")]
    Plus,
    /// The `-` symbol.
    #[token("-")]
    Minus,
    /// The `||` symbol.
    #[token("||")]
    LogicalOr,
    /// The `&&` symbol.
    #[token("&&")]
    LogicalAnd,
    /// The `*` symbol.
    #[token("*")]
    Asterisk,
    /// The `/` symbol.
    #[token("/")]
    Slash,
    /// The `%` symbol.
    #[token("%")]
    Percent,
    /// The `==` symbol.
    #[token("==")]
    Equal,
    /// The `!=` symbol.
    #[token("!=")]
    NotEqual,
    /// The `<=` symbol.
    #[token("<=")]
    LessEqual,
    /// The `>=` symbol.
    #[token(">=")]
    GreaterEqual,
    /// The `<` symbol.
    #[token("<")]
    Less,
    /// The `>` symbol.
    #[token(">")]
    Greater,
    /// The `.` symbol.
    #[token(".")]
    Dot,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum token value.
    MAX,
}

// There can only be 128 tokens in a TokenSet.
const _: () = assert!(Token::MAX as u8 <= 128);

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Whitespace => write!(f, "whitespace"),
            Self::Comment => write!(f, "comment"),
            Self::Float => write!(f, "float"),
            Self::Integer => write!(f, "integer"),
            Self::Ident => write!(f, "identifier"),
            Self::QualifiedName => write!(f, "qualified name"),
            Self::SQStringStart => write!(f, "`'`"),
            Self::DQStringStart => write!(f, "`\"`"),
            Self::HeredocCommandStart => write!(f, "`<<<`"),
            Self::ArrayTypeKeyword => write!(f, "`Array` keyword"),
            Self::BooleanTypeKeyword => write!(f, "`Boolean` keyword"),
            Self::FileTypeKeyword => write!(f, "`File` keyword"),
            Self::FloatTypeKeyword => write!(f, "`Float` keyword"),
            Self::IntTypeKeyword => write!(f, "`Int` keyword"),
            Self::MapTypeKeyword => write!(f, "`Map` keyword"),
            Self::NoneTypeKeyword => write!(f, "`None` keyword"),
            Self::ObjectTypeKeyword => write!(f, "`Object` keyword"),
            Self::PairTypeKeyword => write!(f, "`Pair` keyword"),
            Self::StringTypeKeyword => write!(f, "`String` keyword"),
            Self::AliasKeyword => write!(f, "`alias` keyword"),
            Self::AsKeyword => write!(f, "`as` keyword"),
            Self::CallKeyword => write!(f, "`call` keyword"),
            Self::CommandKeyword => write!(f, "`command` keyword"),
            Self::ElseKeyword => write!(f, "`else` keyword"),
            Self::FalseKeyword => write!(f, "`false` keyword"),
            Self::IfKeyword => write!(f, "`if` keyword"),
            Self::InKeyword => write!(f, "`int` keyword"),
            Self::ImportKeyword => write!(f, "`import` keyword"),
            Self::InputKeyword => write!(f, "`input` keyword"),
            Self::MetaKeyword => write!(f, "`meta` keyword"),
            Self::NullKeyword => write!(f, "`null` keyword"),
            Self::ObjectKeyword => write!(f, "`object` keyword"),
            Self::OutputKeyword => write!(f, "`output` keyword"),
            Self::ParameterMetaKeyword => write!(f, "`parameter_meta` keyword"),
            Self::RuntimeKeyword => write!(f, "`runtime` keyword"),
            Self::ScatterKeyword => write!(f, "`scatter` keyword"),
            Self::StructKeyword => write!(f, "`struct` keyword"),
            Self::TaskKeyword => write!(f, "`task` keyword"),
            Self::ThenKeyword => write!(f, "`then` keyword"),
            Self::TrueKeyword => write!(f, "`true` keyword"),
            Self::VersionKeyword => write!(f, "`version` keyword"),
            Self::WorkflowKeyword => write!(f, "`workflow` keyword"),
            Self::ReservedDirectoryTypeKeyword => write!(f, "reserved `Directory` keyword"),
            Self::ReservedHintsKeyword => write!(f, "reserved `hints` keyword"),
            Self::ReservedRequirementsKeyword => write!(f, "reserved `requirements` keyword"),
            Self::OpenBrace => write!(f, "`{{`"),
            Self::CloseBrace => write!(f, "`}}`"),
            Self::OpenBracket => write!(f, "`[`"),
            Self::CloseBracket => write!(f, "`]`"),
            Self::Assignment => write!(f, "`=`"),
            Self::Colon => write!(f, "`:`"),
            Self::Comma => write!(f, "`,`"),
            Self::OpenParen => write!(f, "`(`"),
            Self::CloseParen => write!(f, "`)`"),
            Self::QuestionMark => write!(f, "`?`"),
            Self::Exclamation => write!(f, "`!`"),
            Self::Plus => write!(f, "`+`"),
            Self::Minus => write!(f, "`-`"),
            Self::LogicalOr => write!(f, "`||`"),
            Self::LogicalAnd => write!(f, "`&&`"),
            Self::Asterisk => write!(f, "`*`"),
            Self::Slash => write!(f, "`/`"),
            Self::Percent => write!(f, "`%`"),
            Self::Equal => write!(f, "`==`"),
            Self::NotEqual => write!(f, "`!=`"),
            Self::LessEqual => write!(f, "`<=`"),
            Self::GreaterEqual => write!(f, "`>=`"),
            Self::Less => write!(f, "`<`"),
            Self::Greater => write!(f, "`>`"),
            Self::Dot => write!(f, "`.`"),
            Self::MAX => unreachable!(),
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::experimental::lexer::test::map;
    use crate::experimental::lexer::Lexer;

    #[test]
    pub fn whitespace() {
        let lexer = Lexer::<Token>::new(" \t\r\n");
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[(Ok(Token::Whitespace), 0..4)],
            "produced tokens did not match the expected set"
        );
    }

    #[test]
    fn comments() {
        use Token::*;
        let lexer = Lexer::<Token>::new(
            r#"
## first comment
# second comment
#### third comment"#,
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Comment), 1..17),
                (Ok(Whitespace), 17..18),
                (Ok(Comment), 18..34),
                (Ok(Whitespace), 34..35),
                (Ok(Comment), 35..53)
            ],
            "produced tokens did not match the expected set"
        );
    }

    #[test]
    fn float() {
        use Token::*;
        let lexer = Lexer::<Token>::new(
            r#"
0.
0.0
.0
.123
0.123
123.0
123.123
123e123
123E123
123e+123
123E+123
123e-123
123E-123
123.e123
123.E123
123.e+123
123.E+123
123.e-123
123.E-123
.123e+123
.123E+123
.123e-123
.123E-123
0.123e+123
0.123E+123
0.123e-123
0.123E-123
123.123e123
123.123E123
123.123e+123
123.123E+123
123.123e-123
123.123E-123"#,
        );

        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Float), 1..3),
                (Ok(Whitespace), 3..4),
                (Ok(Float), 4..7),
                (Ok(Whitespace), 7..8),
                (Ok(Float), 8..10),
                (Ok(Whitespace), 10..11),
                (Ok(Float), 11..15),
                (Ok(Whitespace), 15..16),
                (Ok(Float), 16..21),
                (Ok(Whitespace), 21..22),
                (Ok(Float), 22..27),
                (Ok(Whitespace), 27..28),
                (Ok(Float), 28..35),
                (Ok(Whitespace), 35..36),
                (Ok(Float), 36..43),
                (Ok(Whitespace), 43..44),
                (Ok(Float), 44..51),
                (Ok(Whitespace), 51..52),
                (Ok(Float), 52..60),
                (Ok(Whitespace), 60..61),
                (Ok(Float), 61..69),
                (Ok(Whitespace), 69..70),
                (Ok(Float), 70..78),
                (Ok(Whitespace), 78..79),
                (Ok(Float), 79..87),
                (Ok(Whitespace), 87..88),
                (Ok(Float), 88..96),
                (Ok(Whitespace), 96..97),
                (Ok(Float), 97..105),
                (Ok(Whitespace), 105..106),
                (Ok(Float), 106..115),
                (Ok(Whitespace), 115..116),
                (Ok(Float), 116..125),
                (Ok(Whitespace), 125..126),
                (Ok(Float), 126..135),
                (Ok(Whitespace), 135..136),
                (Ok(Float), 136..145),
                (Ok(Whitespace), 145..146),
                (Ok(Float), 146..155),
                (Ok(Whitespace), 155..156),
                (Ok(Float), 156..165),
                (Ok(Whitespace), 165..166),
                (Ok(Float), 166..175),
                (Ok(Whitespace), 175..176),
                (Ok(Float), 176..185),
                (Ok(Whitespace), 185..186),
                (Ok(Float), 186..196),
                (Ok(Whitespace), 196..197),
                (Ok(Float), 197..207),
                (Ok(Whitespace), 207..208),
                (Ok(Float), 208..218),
                (Ok(Whitespace), 218..219),
                (Ok(Float), 219..229),
                (Ok(Whitespace), 229..230),
                (Ok(Float), 230..241),
                (Ok(Whitespace), 241..242),
                (Ok(Float), 242..253),
                (Ok(Whitespace), 253..254),
                (Ok(Float), 254..266),
                (Ok(Whitespace), 266..267),
                (Ok(Float), 267..279),
                (Ok(Whitespace), 279..280),
                (Ok(Float), 280..292),
                (Ok(Whitespace), 292..293),
                (Ok(Float), 293..305),
            ],
        );
    }

    #[test]
    fn integer() {
        use Token::*;
        let lexer = Lexer::<Token>::new(
            r#"
0
123456789
01234567
0000
0777
0x0
0X0
0x123456789ABCDEF"#,
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Integer), 1..2),
                (Ok(Whitespace), 2..3),
                (Ok(Integer), 3..12),
                (Ok(Whitespace), 12..13),
                (Ok(Integer), 13..21),
                (Ok(Whitespace), 21..22),
                (Ok(Integer), 22..26),
                (Ok(Whitespace), 26..27),
                (Ok(Integer), 27..31),
                (Ok(Whitespace), 31..32),
                (Ok(Integer), 32..35),
                (Ok(Whitespace), 35..36),
                (Ok(Integer), 36..39),
                (Ok(Whitespace), 39..40),
                (Ok(Integer), 40..57),
            ],
        );
    }

    #[test]
    fn ident() {
        use Token::*;

        let lexer = Lexer::<Token>::new(
            r#"
foo
Foo123
F_B
f_b
foo_Bar123
foo0123_bar0123_baz0123
foo123_BAR"#,
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(Ident), 1..4),
                (Ok(Whitespace), 4..5),
                (Ok(Ident), 5..11),
                (Ok(Whitespace), 11..12),
                (Ok(Ident), 12..15),
                (Ok(Whitespace), 15..16),
                (Ok(Ident), 16..19),
                (Ok(Whitespace), 19..20),
                (Ok(Ident), 20..30),
                (Ok(Whitespace), 30..31),
                (Ok(Ident), 31..54),
                (Ok(Whitespace), 54..55),
                (Ok(Ident), 55..65),
            ],
        );
    }

    #[test]
    fn single_quote_string() {
        let mut lexer = Lexer::<Token>::new(r#"'hello \'~{name}${'!'}\': not \~{a var~$}'"#);
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::SQStringStart), 0..1))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(SQStringToken::Text), 1..7)));
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Escape), 7..9))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::PlaceholderStart), 9..11))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Ident), 11..15)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 15..16)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::PlaceholderStart), 16..18))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::SQStringStart), 18..19))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Text), 19..20))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::End), 20..21))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 21..22)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Escape), 22..24))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Text), 24..30))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Escape), 30..32))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Text), 32..38))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Tilde), 38..39))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::DollarSign), 39..40))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::Text), 40..41))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(SQStringToken::End), 41..42))
        );

        let mut lexer = lexer.morph::<Token>();
        assert_eq!(lexer.next().map(map), None);
    }

    #[test]
    fn double_quote_string() {
        let mut lexer = Lexer::<Token>::new(r#""hello \"~{name}${"!"}\": not \~{a var~$}""#);
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::DQStringStart), 0..1))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(DQStringToken::Text), 1..7)));
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Escape), 7..9))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::PlaceholderStart), 9..11))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Ident), 11..15)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 15..16)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::PlaceholderStart), 16..18))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::DQStringStart), 18..19))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 19..20))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::End), 20..21))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 21..22)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Escape), 22..24))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 24..30))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Escape), 30..32))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 32..38))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Tilde), 38..39))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::DollarSign), 39..40))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 40..41))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::End), 41..42))
        );

        let mut lexer = lexer.morph::<Token>();
        assert_eq!(lexer.next().map(map), None);
    }

    #[test]
    fn heredoc() {
        let mut lexer = Lexer::<Token>::new(
            r#"<<<
   printf "~{message}"
   printf "${var}"
   printf ~{"this should not close >>>"}
   printf "\~{escaped}"
   \>>>
   still in heredoc~
>>>"#,
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::HeredocCommandStart), 0..3))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 3..15))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::PlaceholderStart), 15..17))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Ident), 17..24)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 24..25)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 25..56))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::PlaceholderStart), 56..58))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::DQStringStart), 58..59))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 59..84))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::End), 84..85))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 85..86)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 86..98))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Escape), 98..100))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 100..114))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Escape), 114..116))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::End), 116..117))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::End), 117..118))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 118..138))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Tilde), 138..139))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::Text), 139..140))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::End), 140..141))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::End), 141..142))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(HeredocCommandToken::End), 142..143))
        );

        let mut lexer = lexer.morph::<Token>();
        assert_eq!(lexer.next().map(map), None);
    }

    #[test]
    fn brace_command() {
        let mut lexer = Lexer::<Token>::new(
            r#"command {
   printf "~{message}"
   printf "${var}"
   printf ~{"this should not close }"}
   printf "\~{escaped\}"
   printf "\${also escaped\}"
   printf "still in command$~"
}"#,
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::CommandKeyword), 0..7)),
        );
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Whitespace), 7..8)),);
        assert_eq!(lexer.next().map(map), Some((Ok(Token::OpenBrace), 8..9)),);

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 9..21))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::PlaceholderStart), 21..23))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Ident), 23..30)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 30..31)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 31..44))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::PlaceholderStart), 44..46))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Ident), 46..49)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 49..50)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 50..62))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::PlaceholderStart), 62..64))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(Token::DQStringStart), 64..65))
        );

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::Text), 65..88))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(DQStringToken::End), 88..89))
        );

        let mut lexer = lexer.morph();
        assert_eq!(lexer.next().map(map), Some((Ok(Token::CloseBrace), 89..90)));

        let mut lexer = lexer.morph();
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 90..102))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Escape), 102..104))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 104..112))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Escape), 112..114))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 114..127))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Escape), 127..129))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 129..142))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Escape), 142..144))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 144..173))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::DollarSign), 173..174))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Tilde), 174..175))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::Text), 175..177))
        );
        assert_eq!(
            lexer.next().map(map),
            Some((Ok(BraceCommandToken::End), 177..178))
        );

        let mut lexer = lexer.morph::<Token>();
        assert_eq!(lexer.next().map(map), None);
    }

    #[test]
    fn keywords() {
        use Token::*;

        let lexer = Lexer::<Token>::new(
            r#"
Array
Boolean
File
Float
Int
Map
None
Object
Pair
String
alias
as
call
command
else
false
if
in
import
input
meta
null
object
output
parameter_meta
runtime
scatter
struct
task
then
true
version
workflow"#,
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(ArrayTypeKeyword), 1..6),
                (Ok(Whitespace), 6..7),
                (Ok(BooleanTypeKeyword), 7..14),
                (Ok(Whitespace), 14..15),
                (Ok(FileTypeKeyword), 15..19),
                (Ok(Whitespace), 19..20),
                (Ok(FloatTypeKeyword), 20..25),
                (Ok(Whitespace), 25..26),
                (Ok(IntTypeKeyword), 26..29),
                (Ok(Whitespace), 29..30),
                (Ok(MapTypeKeyword), 30..33),
                (Ok(Whitespace), 33..34),
                (Ok(NoneTypeKeyword), 34..38),
                (Ok(Whitespace), 38..39),
                (Ok(ObjectTypeKeyword), 39..45),
                (Ok(Whitespace), 45..46),
                (Ok(PairTypeKeyword), 46..50),
                (Ok(Whitespace), 50..51),
                (Ok(StringTypeKeyword), 51..57),
                (Ok(Whitespace), 57..58),
                (Ok(AliasKeyword), 58..63),
                (Ok(Whitespace), 63..64),
                (Ok(AsKeyword), 64..66),
                (Ok(Whitespace), 66..67),
                (Ok(CallKeyword), 67..71),
                (Ok(Whitespace), 71..72),
                (Ok(CommandKeyword), 72..79),
                (Ok(Whitespace), 79..80),
                (Ok(ElseKeyword), 80..84),
                (Ok(Whitespace), 84..85),
                (Ok(FalseKeyword), 85..90),
                (Ok(Whitespace), 90..91),
                (Ok(IfKeyword), 91..93),
                (Ok(Whitespace), 93..94),
                (Ok(InKeyword), 94..96),
                (Ok(Whitespace), 96..97),
                (Ok(ImportKeyword), 97..103),
                (Ok(Whitespace), 103..104),
                (Ok(InputKeyword), 104..109),
                (Ok(Whitespace), 109..110),
                (Ok(MetaKeyword), 110..114),
                (Ok(Whitespace), 114..115),
                (Ok(NullKeyword), 115..119),
                (Ok(Whitespace), 119..120),
                (Ok(ObjectKeyword), 120..126),
                (Ok(Whitespace), 126..127),
                (Ok(OutputKeyword), 127..133),
                (Ok(Whitespace), 133..134),
                (Ok(ParameterMetaKeyword), 134..148),
                (Ok(Whitespace), 148..149),
                (Ok(RuntimeKeyword), 149..156),
                (Ok(Whitespace), 156..157),
                (Ok(ScatterKeyword), 157..164),
                (Ok(Whitespace), 164..165),
                (Ok(StructKeyword), 165..171),
                (Ok(Whitespace), 171..172),
                (Ok(TaskKeyword), 172..176),
                (Ok(Whitespace), 176..177),
                (Ok(ThenKeyword), 177..181),
                (Ok(Whitespace), 181..182),
                (Ok(TrueKeyword), 182..186),
                (Ok(Whitespace), 186..187),
                (Ok(VersionKeyword), 187..194),
                (Ok(Whitespace), 194..195),
                (Ok(WorkflowKeyword), 195..203),
            ],
        );
    }

    #[test]
    fn reserved_keywords() {
        use Token::*;

        let lexer = Lexer::<Token>::new(
            r#"
Directory
hints
requirements"#,
        );
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(Whitespace), 0..1),
                (Ok(ReservedDirectoryTypeKeyword), 1..10),
                (Ok(Whitespace), 10..11),
                (Ok(ReservedHintsKeyword), 11..16),
                (Ok(Whitespace), 16..17),
                (Ok(ReservedRequirementsKeyword), 17..29),
            ],
        );
    }

    #[test]
    fn symbols() {
        use Token::*;

        let lexer = Lexer::<Token>::new(r#"{}[]=:,()?!+-||&&*/%==!=<=>=<>."#);
        let tokens: Vec<_> = lexer.map(map).collect();
        assert_eq!(
            tokens,
            &[
                (Ok(OpenBrace), 0..1),
                (Ok(CloseBrace), 1..2),
                (Ok(OpenBracket), 2..3),
                (Ok(CloseBracket), 3..4),
                (Ok(Assignment), 4..5),
                (Ok(Colon), 5..6),
                (Ok(Comma), 6..7),
                (Ok(OpenParen), 7..8),
                (Ok(CloseParen), 8..9),
                (Ok(QuestionMark), 9..10),
                (Ok(Exclamation), 10..11),
                (Ok(Plus), 11..12),
                (Ok(Minus), 12..13),
                (Ok(LogicalOr), 13..15),
                (Ok(LogicalAnd), 15..17),
                (Ok(Asterisk), 17..18),
                (Ok(Slash), 18..19),
                (Ok(Percent), 19..20),
                (Ok(Equal), 20..22),
                (Ok(NotEqual), 22..24),
                (Ok(LessEqual), 24..26),
                (Ok(GreaterEqual), 26..28),
                (Ok(Less), 28..29),
                (Ok(Greater), 29..30),
                (Ok(Dot), 30..31),
            ],
        );
    }
}
