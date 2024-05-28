//! WDL (v1) tokens.

use logos::Logos;

use super::Error;
use crate::experimental::parser::ParserToken;
use crate::experimental::tree::SyntaxKind;

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
    #[regex(r"\\(\n|\r|.)")]
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
    #[regex(r"\\(\n|\r|.)")]
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
    #[regex(r"\\(\n|\r|.)")]
    Escape,

    /// A span of literal text.
    #[regex(r"[^\\~>]+")]
    Text,

    /// A tilde that is part of the literal text.
    #[token("~")]
    Tilde,

    /// A single close angle bracket (not the end).
    ///
    /// This token is part of the literal text.
    #[token(">")]
    SingleCloseAngle,

    /// A double close angle bracket (not the end).
    ///
    /// This token is part of the literal text.
    #[token(">>")]
    DoubleCloseAngle,

    /// An ending triple close angle bracket.
    ///
    /// When encountered, [morph][super::Lexer::morph] the lexer to use [Token].
    #[token(">>>")]
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
    #[regex(r"\\(\n|\r|.)")]
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
    #[regex(r"[1-9][0-9]*")]
    #[regex(r"0[0-7]+")]
    #[regex(r"0[xX][0-9a-fA-F]+")]
    Integer,

    /// An identifier.
    #[regex(r"(?&id)")]
    Ident,

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
    /// An end of a heredoc command.
    #[token(">>>")]
    HeredocCommandEnd,

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
    /// The `after` keyword.
    #[token("after")]
    AfterKeyword,
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

impl<'a> ParserToken<'a> for Token {
    fn into_syntax(self) -> SyntaxKind {
        match self {
            Self::Whitespace => SyntaxKind::Whitespace,
            Self::Comment => SyntaxKind::Comment,
            Self::Float => SyntaxKind::Float,
            Self::Integer => SyntaxKind::Integer,
            Self::Ident => SyntaxKind::Ident,
            Self::SQStringStart => SyntaxKind::SingleQuote,
            Self::DQStringStart => SyntaxKind::DoubleQuote,
            Self::HeredocCommandStart => SyntaxKind::OpenHeredoc,
            Self::HeredocCommandEnd => SyntaxKind::CloseHeredoc,
            Self::ArrayTypeKeyword => SyntaxKind::ArrayTypeKeyword,
            Self::BooleanTypeKeyword => SyntaxKind::BooleanTypeKeyword,
            Self::FileTypeKeyword => SyntaxKind::FileTypeKeyword,
            Self::FloatTypeKeyword => SyntaxKind::FloatTypeKeyword,
            Self::IntTypeKeyword => SyntaxKind::IntTypeKeyword,
            Self::MapTypeKeyword => SyntaxKind::MapTypeKeyword,
            Self::NoneTypeKeyword => SyntaxKind::NoneTypeKeyword,
            Self::ObjectTypeKeyword => SyntaxKind::ObjectTypeKeyword,
            Self::PairTypeKeyword => SyntaxKind::PairTypeKeyword,
            Self::StringTypeKeyword => SyntaxKind::StringTypeKeyword,
            Self::AfterKeyword => SyntaxKind::AfterKeyword,
            Self::AliasKeyword => SyntaxKind::AliasKeyword,
            Self::AsKeyword => SyntaxKind::AsKeyword,
            Self::CallKeyword => SyntaxKind::CallKeyword,
            Self::CommandKeyword => SyntaxKind::CommandKeyword,
            Self::ElseKeyword => SyntaxKind::ElseKeyword,
            Self::FalseKeyword => SyntaxKind::FalseKeyword,
            Self::IfKeyword => SyntaxKind::IfKeyword,
            Self::InKeyword => SyntaxKind::InKeyword,
            Self::ImportKeyword => SyntaxKind::ImportKeyword,
            Self::InputKeyword => SyntaxKind::InputKeyword,
            Self::MetaKeyword => SyntaxKind::MetaKeyword,
            Self::NullKeyword => SyntaxKind::NullKeyword,
            Self::ObjectKeyword => SyntaxKind::ObjectKeyword,
            Self::OutputKeyword => SyntaxKind::OutputKeyword,
            Self::ParameterMetaKeyword => SyntaxKind::ParameterMetaKeyword,
            Self::RuntimeKeyword => SyntaxKind::RuntimeKeyword,
            Self::ScatterKeyword => SyntaxKind::ScatterKeyword,
            Self::StructKeyword => SyntaxKind::StructKeyword,
            Self::TaskKeyword => SyntaxKind::TaskKeyword,
            Self::ThenKeyword => SyntaxKind::ThenKeyword,
            Self::TrueKeyword => SyntaxKind::TrueKeyword,
            Self::VersionKeyword => SyntaxKind::VersionKeyword,
            Self::WorkflowKeyword => SyntaxKind::WorkflowKeyword,
            Self::ReservedDirectoryTypeKeyword => SyntaxKind::DirectoryTypeKeyword,
            Self::ReservedHintsKeyword => SyntaxKind::HintsKeyword,
            Self::ReservedRequirementsKeyword => SyntaxKind::RequirementsKeyword,
            Self::OpenBrace => SyntaxKind::OpenBrace,
            Self::CloseBrace => SyntaxKind::CloseBrace,
            Self::OpenBracket => SyntaxKind::OpenBracket,
            Self::CloseBracket => SyntaxKind::CloseBracket,
            Self::Assignment => SyntaxKind::Assignment,
            Self::Colon => SyntaxKind::Colon,
            Self::Comma => SyntaxKind::Comma,
            Self::OpenParen => SyntaxKind::OpenParen,
            Self::CloseParen => SyntaxKind::CloseParen,
            Self::QuestionMark => SyntaxKind::QuestionMark,
            Self::Exclamation => SyntaxKind::Exclamation,
            Self::Plus => SyntaxKind::Plus,
            Self::Minus => SyntaxKind::Minus,
            Self::LogicalOr => SyntaxKind::LogicalOr,
            Self::LogicalAnd => SyntaxKind::LogicalAnd,
            Self::Asterisk => SyntaxKind::Asterisk,
            Self::Slash => SyntaxKind::Slash,
            Self::Percent => SyntaxKind::Percent,
            Self::Equal => SyntaxKind::Equal,
            Self::NotEqual => SyntaxKind::NotEqual,
            Self::LessEqual => SyntaxKind::LessEqual,
            Self::GreaterEqual => SyntaxKind::GreaterEqual,
            Self::Less => SyntaxKind::Less,
            Self::Greater => SyntaxKind::Greater,
            Self::Dot => SyntaxKind::Dot,
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
            Self::Float => "float",
            Self::Integer => "integer",
            Self::Ident => "identifier",
            Self::SQStringStart => "`'`",
            Self::DQStringStart => "`\"`",
            Self::HeredocCommandStart => "`<<<`",
            Self::HeredocCommandEnd => "`>>>`",
            Self::ArrayTypeKeyword => "`Array` keyword",
            Self::BooleanTypeKeyword => "`Boolean` keyword",
            Self::FileTypeKeyword => "`File` keyword",
            Self::FloatTypeKeyword => "`Float` keyword",
            Self::IntTypeKeyword => "`Int` keyword",
            Self::MapTypeKeyword => "`Map` keyword",
            Self::NoneTypeKeyword => "`None` keyword",
            Self::ObjectTypeKeyword => "`Object` keyword",
            Self::PairTypeKeyword => "`Pair` keyword",
            Self::StringTypeKeyword => "`String` keyword",
            Self::AfterKeyword => "`after` keyword",
            Self::AliasKeyword => "`alias` keyword",
            Self::AsKeyword => "`as` keyword",
            Self::CallKeyword => "`call` keyword",
            Self::CommandKeyword => "`command` keyword",
            Self::ElseKeyword => "`else` keyword",
            Self::FalseKeyword => "`false` keyword",
            Self::IfKeyword => "`if` keyword",
            Self::InKeyword => "`int` keyword",
            Self::ImportKeyword => "`import` keyword",
            Self::InputKeyword => "`input` keyword",
            Self::MetaKeyword => "`meta` keyword",
            Self::NullKeyword => "`null` keyword",
            Self::ObjectKeyword => "`object` keyword",
            Self::OutputKeyword => "`output` keyword",
            Self::ParameterMetaKeyword => "`parameter_meta` keyword",
            Self::RuntimeKeyword => "`runtime` keyword",
            Self::ScatterKeyword => "`scatter` keyword",
            Self::StructKeyword => "`struct` keyword",
            Self::TaskKeyword => "`task` keyword",
            Self::ThenKeyword => "`then` keyword",
            Self::TrueKeyword => "`true` keyword",
            Self::VersionKeyword => "`version` keyword",
            Self::WorkflowKeyword => "`workflow` keyword",
            Self::ReservedDirectoryTypeKeyword => "reserved `Directory` keyword",
            Self::ReservedHintsKeyword => "reserved `hints` keyword",
            Self::ReservedRequirementsKeyword => "reserved `requirements` keyword",
            Self::OpenBrace => "`{`",
            Self::CloseBrace => "`}`",
            Self::OpenBracket => "`[`",
            Self::CloseBracket => "`]`",
            Self::Assignment => "`=`",
            Self::Colon => "`:`",
            Self::Comma => "`,`",
            Self::OpenParen => "`(`",
            Self::CloseParen => "`)`",
            Self::QuestionMark => "`?`",
            Self::Exclamation => "`!`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::LogicalOr => "`||`",
            Self::LogicalAnd => "`&&`",
            Self::Asterisk => "`*`",
            Self::Slash => "`/`",
            Self::Percent => "`%`",
            Self::Equal => "`==`",
            Self::NotEqual => "`!=`",
            Self::LessEqual => "`<=`",
            Self::GreaterEqual => "`>=`",
            Self::Less => "`<`",
            Self::Greater => "`>`",
            Self::Dot => "`.`",
            Self::MAX => unreachable!(),
        }
    }

    fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
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
5
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
                (Ok(Integer), 3..4),
                (Ok(Whitespace), 4..5),
                (Ok(Integer), 5..14),
                (Ok(Whitespace), 14..15),
                (Ok(Integer), 15..23),
                (Ok(Whitespace), 23..24),
                (Ok(Integer), 24..28),
                (Ok(Whitespace), 28..29),
                (Ok(Integer), 29..33),
                (Ok(Whitespace), 33..34),
                (Ok(Integer), 34..37),
                (Ok(Whitespace), 37..38),
                (Ok(Integer), 38..41),
                (Ok(Whitespace), 41..42),
                (Ok(Integer), 42..59),
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
            Some((Ok(HeredocCommandToken::DoubleCloseAngle), 116..118))
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
            Some((Ok(HeredocCommandToken::End), 140..143))
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
        assert_eq!(lexer.next().map(map), Some((Ok(Token::Whitespace), 7..8)));
        assert_eq!(lexer.next().map(map), Some((Ok(Token::OpenBrace), 8..9)));

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
after
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
                (Ok(AfterKeyword), 58..63),
                (Ok(Whitespace), 63..64),
                (Ok(AliasKeyword), 64..69),
                (Ok(Whitespace), 69..70),
                (Ok(AsKeyword), 70..72),
                (Ok(Whitespace), 72..73),
                (Ok(CallKeyword), 73..77),
                (Ok(Whitespace), 77..78),
                (Ok(CommandKeyword), 78..85),
                (Ok(Whitespace), 85..86),
                (Ok(ElseKeyword), 86..90),
                (Ok(Whitespace), 90..91),
                (Ok(FalseKeyword), 91..96),
                (Ok(Whitespace), 96..97),
                (Ok(IfKeyword), 97..99),
                (Ok(Whitespace), 99..100),
                (Ok(InKeyword), 100..102),
                (Ok(Whitespace), 102..103),
                (Ok(ImportKeyword), 103..109),
                (Ok(Whitespace), 109..110),
                (Ok(InputKeyword), 110..115),
                (Ok(Whitespace), 115..116),
                (Ok(MetaKeyword), 116..120),
                (Ok(Whitespace), 120..121),
                (Ok(NullKeyword), 121..125),
                (Ok(Whitespace), 125..126),
                (Ok(ObjectKeyword), 126..132),
                (Ok(Whitespace), 132..133),
                (Ok(OutputKeyword), 133..139),
                (Ok(Whitespace), 139..140),
                (Ok(ParameterMetaKeyword), 140..154),
                (Ok(Whitespace), 154..155),
                (Ok(RuntimeKeyword), 155..162),
                (Ok(Whitespace), 162..163),
                (Ok(ScatterKeyword), 163..170),
                (Ok(Whitespace), 170..171),
                (Ok(StructKeyword), 171..177),
                (Ok(Whitespace), 177..178),
                (Ok(TaskKeyword), 178..182),
                (Ok(Whitespace), 182..183),
                (Ok(ThenKeyword), 183..187),
                (Ok(Whitespace), 187..188),
                (Ok(TrueKeyword), 188..192),
                (Ok(Whitespace), 192..193),
                (Ok(VersionKeyword), 193..200),
                (Ok(Whitespace), 200..201),
                (Ok(WorkflowKeyword), 201..209),
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
