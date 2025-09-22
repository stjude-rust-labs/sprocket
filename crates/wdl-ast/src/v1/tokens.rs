//! V1 AST tokens.

use crate::AstToken;
use crate::SyntaxKind;
use crate::SyntaxToken;
use crate::TreeToken;

/// Defines an AST token struct.
macro_rules! define_token_struct {
    ($name:ident, $doc:literal) => {
        #[derive(Clone, Debug)]
        #[doc = concat!("A token representing ", $doc, ".")]
        pub struct $name<T: TreeToken = SyntaxToken>(T);

        impl<T: TreeToken> AstToken<T> for $name<T> {
            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, SyntaxKind::$name)
            }

            fn cast(inner: T) -> Option<Self> {
                if Self::can_cast(inner.kind()) {
                    return Some(Self(inner));
                }

                None
            }

            fn inner(&self) -> &T {
                &self.0
            }
        }
    };
}

/// Defines an AST token.
macro_rules! define_token {
    ($name:ident, $doc:literal, $display:literal) => {
        define_token_struct!($name, $doc);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, $display)
            }
        }
    };
    ($name:ident, $doc:literal) => {
        define_token_struct!($name, $doc);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_token!(AfterKeyword, "the `after` keyword", "after");
define_token!(AliasKeyword, "the `alias` keyword", "alias");
define_token!(ArrayTypeKeyword, "the `Array` type keyword", "Array");
define_token!(AsKeyword, "the `as` keyword", "as");
define_token!(Assignment, "the `=` symbol", "=");
define_token!(Asterisk, "the `*` symbol", "*");
define_token!(BooleanTypeKeyword, "the `Boolean` type keyword", "Boolean");
define_token!(CallKeyword, "the `call` keyword", "call");
define_token!(CloseBrace, "the `}` symbol", "}}");
define_token!(CloseBracket, "the `]` symbol", "]");
define_token!(CloseHeredoc, "the `>>>` symbol", ">>>");
define_token!(CloseParen, "the `)` symbol", ")");
define_token!(Colon, "the `:` symbol", ":");
define_token!(Comma, "the `,` symbol", ",");
define_token!(CommandKeyword, "the `command` keyword", "command");
define_token!(
    DirectoryTypeKeyword,
    "the `Directory` type keyword",
    "Directory"
);
define_token!(Dot, "the `.` symbol", ".");
define_token!(DoubleQuote, "the `\"` symbol", "\"");
define_token!(ElseKeyword, "the `else` keyword", "else");
define_token!(EnvKeyword, "the `env` keyword", "env");
define_token!(Equal, "the `=` symbol", "=");
define_token!(Exclamation, "the `!` symbol", "!");
define_token!(Exponentiation, "the `**` symbol", "**");
define_token!(FalseKeyword, "the `false` keyword", "false");
define_token!(FileTypeKeyword, "the `File` type keyword", "File");
define_token!(FloatTypeKeyword, "the `Float` type keyword", "Float");
define_token!(Greater, "the `>` symbol", ">");
define_token!(GreaterEqual, "the `>=` symbol", ">=");
define_token!(HintsKeyword, "the `hints` keyword", "hints");
define_token!(IfKeyword, "the `if` keyword", "if");
define_token!(ImportKeyword, "the `import` keyword", "import");
define_token!(InKeyword, "the `in` keyword", "in");
define_token!(InputKeyword, "the `input` keyword", "input");
define_token!(IntTypeKeyword, "the `Int` type keyword", "Int");
define_token!(Less, "the `<` symbol", "<");
define_token!(LessEqual, "the `<=` symbol", "<=");
define_token!(LogicalAnd, "the `&&` symbol", "&&");
define_token!(LogicalOr, "the `||` symbol", "||");
define_token!(MapTypeKeyword, "the `Map` type keyword", "Map");
define_token!(MetaKeyword, "the `meta` keyword", "meta");
define_token!(Minus, "the `-` symbol", "-");
define_token!(NoneKeyword, "the `None` keyword", "None");
define_token!(NotEqual, "the `!=` symbol", "!=");
define_token!(NullKeyword, "the `null` keyword", "null");
define_token!(ObjectKeyword, "the `object` keyword", "object");
define_token!(ObjectTypeKeyword, "the `Object` type keyword", "Object");
define_token!(OpenBrace, "the `{` symbol", "{{");
define_token!(OpenBracket, "the `[` symbol", "[");
define_token!(OpenHeredoc, "the `<<<` symbol", "<<<");
define_token!(OpenParen, "the `(` symbol", "(");
define_token!(OutputKeyword, "the `output` keyword", "output");
define_token!(PairTypeKeyword, "the `Pair` type keyword", "Pair");
define_token!(
    ParameterMetaKeyword,
    "the `parameter_meta` keyword",
    "parameter_meta"
);
define_token!(Percent, "the `%` symbol", "%");
define_token!(PlaceholderOpen, "a `${` or `~{` symbol");
define_token!(Plus, "the `+` symbol", "+");
define_token!(QuestionMark, "the `?` symbol", "?");
define_token!(
    RequirementsKeyword,
    "the `requirements` keyword",
    "requirements"
);
define_token!(RuntimeKeyword, "the `runtime` keyword", "runtime");
define_token!(ScatterKeyword, "the `scatter` keyword", "scatter");
define_token!(SingleQuote, "the `'` symbol", "'");
define_token!(Slash, "the `/` symbol", "/");
define_token!(StringTypeKeyword, "the `String` type keyword", "String");
define_token!(StructKeyword, "the `struct` keyword", "struct");
define_token!(TaskKeyword, "the `task` keyword", "task");
define_token!(ThenKeyword, "the `then` keyword", "then");
define_token!(TrueKeyword, "the `true` keyword", "true");
define_token!(Unknown, "unknown contents within a WDL document");
define_token!(VersionKeyword, "the `version` keyword", "version");
define_token!(WorkflowKeyword, "the `workflow` keyword", "workflow");
