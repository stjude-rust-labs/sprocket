//! V1 AST tokens.

use crate::AstToken;
use crate::SyntaxKind;
use crate::SyntaxToken;

/// A token representing the `after` keyword.
#[derive(Clone, Debug)]
pub struct AfterKeyword(SyntaxToken);

impl AstToken for AfterKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::AfterKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for AfterKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "after")
    }
}

/// A token representing the `alias` keyword.
#[derive(Clone, Debug)]
pub struct AliasKeyword(SyntaxToken);

impl AstToken for AliasKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::AliasKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for AliasKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "alias")
    }
}

/// A token representing the `Array` type keyword.
#[derive(Clone, Debug)]
pub struct ArrayTypeKeyword(SyntaxToken);

impl AstToken for ArrayTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ArrayTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ArrayTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Array")
    }
}

/// A token representing the `as` keyword.
#[derive(Clone, Debug)]
pub struct AsKeyword(SyntaxToken);

impl AstToken for AsKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::AsKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for AsKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "as")
    }
}

/// A token representing the `=` symbol.
#[derive(Clone, Debug)]
pub struct Assignment(SyntaxToken);

impl AstToken for Assignment {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Assignment)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Assignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "=")
    }
}

/// A token representing the `*` symbol.
#[derive(Clone, Debug)]
pub struct Asterisk(SyntaxToken);

impl AstToken for Asterisk {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Asterisk)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Asterisk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "*")
    }
}

/// A token representing the `Boolean` keyword.
#[derive(Clone, Debug)]
pub struct BooleanTypeKeyword(SyntaxToken);

impl AstToken for BooleanTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::BooleanTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for BooleanTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Boolean")
    }
}

/// A token representing the `call` keyword.
#[derive(Clone, Debug)]
pub struct CallKeyword(SyntaxToken);

impl AstToken for CallKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CallKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CallKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "call")
    }
}

/// A token representing the `}` symbol.
#[derive(Clone, Debug)]
pub struct CloseBrace(SyntaxToken);

impl AstToken for CloseBrace {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CloseBrace)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CloseBrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "}}")
    }
}

/// A token representing the `]` symbol.
#[derive(Clone, Debug)]
pub struct CloseBracket(SyntaxToken);

impl AstToken for CloseBracket {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CloseBracket)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CloseBracket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "]")
    }
}

/// A token representing the `>>>` token.
#[derive(Clone, Debug)]
pub struct CloseHeredoc(SyntaxToken);

impl AstToken for CloseHeredoc {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CloseHeredoc)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CloseHeredoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ">>>")
    }
}

/// A token representing the `)` symbol.
#[derive(Clone, Debug)]
pub struct CloseParen(SyntaxToken);

impl AstToken for CloseParen {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CloseParen)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CloseParen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ")")
    }
}

/// A token representing the `:` symbol.
#[derive(Clone, Debug)]
pub struct Colon(SyntaxToken);

impl AstToken for Colon {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Colon)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Colon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":")
    }
}

/// A token representing the `,` symbol.
#[derive(Clone, Debug)]
pub struct Comma(SyntaxToken);

impl AstToken for Comma {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Comma)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Comma {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ",")
    }
}

/// A token representing the `command` keyword.
#[derive(Clone, Debug)]
pub struct CommandKeyword(SyntaxToken);

impl AstToken for CommandKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::CommandKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for CommandKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "command")
    }
}

/// A token representing the `Directory` type keyword.
#[derive(Clone, Debug)]
pub struct DirectoryTypeKeyword(SyntaxToken);

impl AstToken for DirectoryTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::DirectoryTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for DirectoryTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Directory")
    }
}

/// A token representing the `.` symbol.
#[derive(Clone, Debug)]
pub struct Dot(SyntaxToken);

impl AstToken for Dot {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Dot)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Dot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".")
    }
}

/// A token representing the `"` symbol.
#[derive(Clone, Debug)]
pub struct DoubleQuote(SyntaxToken);

impl AstToken for DoubleQuote {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::DoubleQuote)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for DoubleQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, r#"""#)
    }
}

/// A token representing the `else` keyword.
#[derive(Clone, Debug)]
pub struct ElseKeyword(SyntaxToken);

impl AstToken for ElseKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ElseKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ElseKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "else")
    }
}

/// A token representing the `==` symbol.
#[derive(Clone, Debug)]
pub struct Equal(SyntaxToken);

impl AstToken for Equal {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Equal)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Equal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "=")
    }
}

/// A token representing the `!` symbol.
#[derive(Clone, Debug)]
pub struct Exclamation(SyntaxToken);

impl AstToken for Exclamation {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Exclamation)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Exclamation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "!")
    }
}

/// A token representing the `**` keyword.
#[derive(Clone, Debug)]
pub struct Exponentiation(SyntaxToken);

impl AstToken for Exponentiation {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Exponentiation)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Exponentiation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "**")
    }
}

/// A token representing the `false` keyword.
#[derive(Clone, Debug)]
pub struct FalseKeyword(SyntaxToken);

impl AstToken for FalseKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::FalseKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for FalseKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "false")
    }
}

/// A token representing the `File` type keyword.
#[derive(Clone, Debug)]
pub struct FileTypeKeyword(SyntaxToken);

impl AstToken for FileTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::FileTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for FileTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "File")
    }
}

/// A token representing the `Float` type keyword.
#[derive(Clone, Debug)]
pub struct FloatTypeKeyword(SyntaxToken);

impl AstToken for FloatTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::FloatTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for FloatTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Float")
    }
}

/// A token representing the `>` symbol.
#[derive(Clone, Debug)]
pub struct Greater(SyntaxToken);

impl AstToken for Greater {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Greater)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Greater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ">")
    }
}

/// A token representing the `>=` symbol.
#[derive(Clone, Debug)]
pub struct GreaterEqual(SyntaxToken);

impl AstToken for GreaterEqual {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::GreaterEqual)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for GreaterEqual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ">=")
    }
}

/// A token representing the `hints` keyword.
#[derive(Clone, Debug)]
pub struct HintsKeyword(SyntaxToken);

impl AstToken for HintsKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::HintsKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for HintsKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hints")
    }
}

/// A token representing the `if` keyword.
#[derive(Clone, Debug)]
pub struct IfKeyword(SyntaxToken);

impl AstToken for IfKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::IfKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for IfKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "if")
    }
}

/// A token representing the `import` keyword.
#[derive(Clone, Debug)]
pub struct ImportKeyword(SyntaxToken);

impl AstToken for ImportKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ImportKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ImportKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "import")
    }
}

/// A token representing the `in` keyword.
#[derive(Clone, Debug)]
pub struct InKeyword(SyntaxToken);

impl AstToken for InKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::InKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for InKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "in")
    }
}

/// A token representing the `input` keyword.
#[derive(Clone, Debug)]
pub struct InputKeyword(SyntaxToken);

impl AstToken for InputKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::InputKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for InputKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "input")
    }
}

/// A token representing the `Int` type keyword.
#[derive(Clone, Debug)]
pub struct IntTypeKeyword(SyntaxToken);

impl AstToken for IntTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::IntTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for IntTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Int")
    }
}

/// A token representing the `<` symbol.
#[derive(Clone, Debug)]
pub struct Less(SyntaxToken);

impl AstToken for Less {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Less)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Less {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<")
    }
}

/// A token representing the `<=` symbol.
#[derive(Clone, Debug)]
pub struct LessEqual(SyntaxToken);

impl AstToken for LessEqual {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::LessEqual)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for LessEqual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<=")
    }
}

/// A token representing the `&&` symbol.
#[derive(Clone, Debug)]
pub struct LogicalAnd(SyntaxToken);

impl AstToken for LogicalAnd {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::LogicalAnd)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for LogicalAnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "&&")
    }
}

/// A token representing the `||` symbol.
#[derive(Clone, Debug)]
pub struct LogicalOr(SyntaxToken);

impl AstToken for LogicalOr {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::LogicalOr)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for LogicalOr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "||")
    }
}

/// A token representing the `Map` type keyword.
#[derive(Clone, Debug)]
pub struct MapTypeKeyword(SyntaxToken);

impl AstToken for MapTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::MapTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for MapTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Map")
    }
}

/// A token representing the `meta` keyword.
#[derive(Clone, Debug)]
pub struct MetaKeyword(SyntaxToken);

impl AstToken for MetaKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::MetaKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for MetaKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "meta")
    }
}

/// A token representing the `-` symbol.
#[derive(Clone, Debug)]
pub struct Minus(SyntaxToken);

impl AstToken for Minus {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Minus)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Minus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "-")
    }
}

/// A token representing the `None` keyword.
#[derive(Clone, Debug)]
pub struct NoneKeyword(SyntaxToken);

impl AstToken for NoneKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::NoneKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for NoneKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "None")
    }
}

/// A token representing the `!=` symbol.
#[derive(Clone, Debug)]
pub struct NotEqual(SyntaxToken);

impl AstToken for NotEqual {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::NotEqual)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for NotEqual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "!=")
    }
}

/// A token representing the `null` keyword.
#[derive(Clone, Debug)]
pub struct NullKeyword(SyntaxToken);

impl AstToken for NullKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::NullKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for NullKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "null")
    }
}

/// A token representing the `object` keyword.
#[derive(Clone, Debug)]
pub struct ObjectKeyword(SyntaxToken);

impl AstToken for ObjectKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ObjectKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ObjectKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "object")
    }
}

/// A token representing the `Object` type keyword.
#[derive(Clone, Debug)]
pub struct ObjectTypeKeyword(SyntaxToken);

impl AstToken for ObjectTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ObjectTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ObjectTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object")
    }
}

/// A token representing the `{` symbol.
#[derive(Clone, Debug)]
pub struct OpenBrace(SyntaxToken);

impl AstToken for OpenBrace {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::OpenBrace)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for OpenBrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")
    }
}

/// A token representing the `[` symbol.
#[derive(Clone, Debug)]
pub struct OpenBracket(SyntaxToken);

impl AstToken for OpenBracket {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::OpenBracket)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for OpenBracket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")
    }
}

/// A token representing the `<<<` symbol.
#[derive(Clone, Debug)]
pub struct OpenHeredoc(SyntaxToken);

impl AstToken for OpenHeredoc {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::OpenHeredoc)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for OpenHeredoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<<<")
    }
}

/// A token representing the `(` keyword.
#[derive(Clone, Debug)]
pub struct OpenParen(SyntaxToken);

impl AstToken for OpenParen {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::OpenParen)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for OpenParen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")
    }
}

/// A token representing the `output` keyword.
#[derive(Clone, Debug)]
pub struct OutputKeyword(SyntaxToken);

impl AstToken for OutputKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::OutputKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for OutputKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output")
    }
}

/// A token representing the `Pair` type keyword.
#[derive(Clone, Debug)]
pub struct PairTypeKeyword(SyntaxToken);

impl AstToken for PairTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::PairTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for PairTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pair")
    }
}

/// A token representing the `parameter_meta` keyword.
#[derive(Clone, Debug)]
pub struct ParameterMetaKeyword(SyntaxToken);

impl AstToken for ParameterMetaKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ParameterMetaKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ParameterMetaKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parameter_meta")
    }
}

/// A token representing the `%` symbol.
#[derive(Clone, Debug)]
pub struct Percent(SyntaxToken);

impl AstToken for Percent {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Percent)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Percent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%")
    }
}

/// Represents one of the placeholder open symbols.
#[derive(Clone, Debug)]
pub struct PlaceholderOpen(SyntaxToken);

impl AstToken for PlaceholderOpen {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::PlaceholderOpen)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for PlaceholderOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: this is deferred to the entire underlying string simply because
        // we cannot known a priori what the captured text is.
        write!(f, "{}", self.0)
    }
}

/// A token representing the `+` symbol.
#[derive(Clone, Debug)]
pub struct Plus(SyntaxToken);

impl AstToken for Plus {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Plus)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Plus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "+")
    }
}

/// A token representing the `?` symbol.
#[derive(Clone, Debug)]
pub struct QuestionMark(SyntaxToken);

impl AstToken for QuestionMark {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::QuestionMark)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for QuestionMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "?")
    }
}

/// A token representing the `requirements` keyword.
#[derive(Clone, Debug)]
pub struct RequirementsKeyword(SyntaxToken);

impl AstToken for RequirementsKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::RequirementsKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for RequirementsKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "requirements")
    }
}

/// A token representing the `runtime` keyword.
#[derive(Clone, Debug)]
pub struct RuntimeKeyword(SyntaxToken);

impl AstToken for RuntimeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::RuntimeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for RuntimeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime")
    }
}

/// A token representing the `scatter` keyword.
#[derive(Clone, Debug)]
pub struct ScatterKeyword(SyntaxToken);

impl AstToken for ScatterKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ScatterKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ScatterKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scatter")
    }
}

/// A token representing the `'` symbol.
#[derive(Clone, Debug)]
pub struct SingleQuote(SyntaxToken);

impl AstToken for SingleQuote {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::SingleQuote)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for SingleQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'")
    }
}

/// A token representing the `/` symbol.
#[derive(Clone, Debug)]
pub struct Slash(SyntaxToken);

impl AstToken for Slash {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Slash)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Slash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/")
    }
}

/// A token representing the `String` type keyword.
#[derive(Clone, Debug)]
pub struct StringTypeKeyword(SyntaxToken);

impl AstToken for StringTypeKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::StringTypeKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for StringTypeKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "String")
    }
}

/// A token representing the `struct` keyword.
#[derive(Clone, Debug)]
pub struct StructKeyword(SyntaxToken);

impl AstToken for StructKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::StructKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for StructKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "struct")
    }
}

/// A token representing the `task` keyword.
#[derive(Clone, Debug)]
pub struct TaskKeyword(SyntaxToken);

impl AstToken for TaskKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::TaskKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for TaskKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "task")
    }
}

/// A token representing the `then` keyword.
#[derive(Clone, Debug)]
pub struct ThenKeyword(SyntaxToken);

impl AstToken for ThenKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::ThenKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }

        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for ThenKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "then")
    }
}

/// A token representing the `true` keyword.
#[derive(Clone, Debug)]
pub struct TrueKeyword(SyntaxToken);

impl AstToken for TrueKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::TrueKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for TrueKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "true")
    }
}

/// A token representing unknown contents within a WDL document.
#[derive(Debug)]
pub struct Unknown(SyntaxToken);

impl AstToken for Unknown {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::Unknown)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for Unknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: this is deferred to the entire underlying string simply because
        // we cannot known a priori what the captured text is.
        write!(f, "{}", self.0)
    }
}

/// A token representing the `version` keyword.
#[derive(Clone, Debug)]
pub struct VersionKeyword(SyntaxToken);

impl AstToken for VersionKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::VersionKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for VersionKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "version")
    }
}

/// A token representing the `workflow` keyword.
#[derive(Clone, Debug)]
pub struct WorkflowKeyword(SyntaxToken);

impl AstToken for WorkflowKeyword {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(kind, SyntaxKind::WorkflowKeyword)
    }

    fn cast(syntax: SyntaxToken) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            return Some(Self(syntax));
        }
        None
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl std::fmt::Display for WorkflowKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "workflow")
    }
}
