//! Module for the concrete syntax tree (CST) representation.

use std::fmt;

use rowan::GreenNodeBuilder;

use super::grammar;
use super::lexer::Lexer;
use super::parser::Error;
use super::parser::Event;
use crate::experimental::parser::Parser;

/// Represents the kind of a node or token in a WDL concrete
/// syntax tree (CST).
///
/// Nodes have at least one token child and represent a syntactic construct.
///
/// Tokens are terminal and represent any span of the source.
///
/// This enumeration is a union of all supported WDL tokens and nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    /// A whitespace token.
    Whitespace,
    /// A comment token.
    Comment,
    /// A WDL version token.
    Version,
    /// A literal float token.
    Float,
    /// A literal integer token.
    Integer,
    /// An identifier token.
    Ident,
    /// A qualified name token.
    QualifiedName,
    /// A single quote token.
    SingleQuote,
    /// A double quote token.
    DoubleQuote,
    /// An open heredoc token.
    OpenHeredoc,
    /// A close heredoc token.
    CloseHeredoc,
    /// The `Array` type keyword token.
    ArrayTypeKeyword,
    /// The `Boolean` type keyword token.
    BooleanTypeKeyword,
    /// The `File` type keyword token.
    FileTypeKeyword,
    /// The `Float` type keyword token.
    FloatTypeKeyword,
    /// The `Int` type keyword token.
    IntTypeKeyword,
    /// The `Map` type keyword token.
    MapTypeKeyword,
    /// The `None` type keyword token.
    NoneTypeKeyword,
    /// The `Object` type keyword token.
    ObjectTypeKeyword,
    /// The `Pair` type keyword token.
    PairTypeKeyword,
    /// The `String` type keyword token.
    StringTypeKeyword,
    /// The `alias` keyword token.
    AliasKeyword,
    /// The `as` keyword token.
    AsKeyword,
    /// The `call` keyword token.
    CallKeyword,
    /// The `command` keyword token.
    CommandKeyword,
    /// The `else` keyword token.
    ElseKeyword,
    /// The `false` keyword token.
    FalseKeyword,
    /// The `if` keyword token.
    IfKeyword,
    /// The `in` keyword token.
    InKeyword,
    /// The `import` keyword token.
    ImportKeyword,
    /// The `input` keyword token.
    InputKeyword,
    /// The `meta` keyword token.
    MetaKeyword,
    /// The `null` keyword token.
    NullKeyword,
    /// The `object` keyword token.
    ObjectKeyword,
    /// The `output` keyword token.
    OutputKeyword,
    /// The `parameter_meta` keyword token.
    ParameterMetaKeyword,
    /// The `runtime` keyword token.
    RuntimeKeyword,
    /// The `scatter` keyword token.
    ScatterKeyword,
    /// The `struct` keyword token.
    StructKeyword,
    /// The `task` keyword token.
    TaskKeyword,
    /// The `then` keyword token.
    ThenKeyword,
    /// The `true` keyword token.
    TrueKeyword,
    /// The `version` keyword token.
    VersionKeyword,
    /// The `workflow` keyword token.
    WorkflowKeyword,
    /// The reserved `Directory` type keyword token.
    DirectoryTypeKeyword,
    /// The reserved `hints` keyword token.
    HintsKeyword,
    /// The reserved `requirements` keyword token.
    RequirementsKeyword,
    /// The `{` symbol token.
    OpenBrace,
    /// The `}` symbol token.
    CloseBrace,
    /// The `[` symbol token.
    OpenBracket,
    /// The `]` symbol token.
    CloseBracket,
    /// The `=` symbol token.
    Assignment,
    /// The `:` symbol token.
    Colon,
    /// The `,` symbol token.
    Comma,
    /// The `(` symbol token.
    OpenParen,
    /// The `)` symbol token.
    CloseParen,
    /// The `?` symbol token.
    QuestionMark,
    /// The `!` symbol token.
    Exclamation,
    /// The `+` symbol token.
    Plus,
    /// The `-` symbol token.
    Minus,
    /// The `||` symbol token.
    LogicalOr,
    /// The `&&` symbol token.
    LogicalAnd,
    /// The `*` symbol token.
    Asterisk,
    /// The `/` symbol token.
    Slash,
    /// The `%` symbol token.
    Percent,
    /// The `==` symbol token.
    Equal,
    /// The `!=` symbol token.
    NotEqual,
    /// The `<=` symbol token.
    LessEqual,
    /// The `>=` symbol token.
    GreaterEqual,
    /// The `<` symbol token.
    Less,
    /// The `>` symbol token.
    Greater,
    /// The `.` symbol token.
    Dot,

    /// Abandoned nodes are nodes that encountered errors.
    ///
    /// Children of abandoned nodes are re-parented to the parent of
    /// the abandoned node.
    ///
    /// As this is an internal implementation of error recovery,
    /// hide this variant from the documentation.
    #[doc(hidden)]
    Abandoned,
    /// Represents the WDL document root node.
    RootNode,
    /// Represents a version statement node.
    VersionStatementNode,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum syntax kind value.
    MAX,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Represents the Workflow Definition Language (WDL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkflowDescriptionLanguage;

impl rowan::Language for WorkflowDescriptionLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::MAX as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Represents a node in the concrete syntax tree.
pub type SyntaxNode = rowan::SyntaxNode<WorkflowDescriptionLanguage>;
/// Represents a token in the concrete syntax tree.
pub type SyntaxToken = rowan::SyntaxToken<WorkflowDescriptionLanguage>;
/// Represents an element (node or token) in the concrete syntax tree.
pub type SyntaxElement = rowan::SyntaxElement<WorkflowDescriptionLanguage>;
/// Represents node children in the concrete syntax tree.
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<WorkflowDescriptionLanguage>;

/// Represents an untyped concrete syntax tree.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SyntaxTree(SyntaxNode);

impl SyntaxTree {
    /// Parses WDL source to produce a syntax tree.
    ///
    /// A syntax tree is always returned, even for invalid WDL documents.
    ///
    /// Additionally, the list of errors encountered during the parse is
    /// returned; if the list is empty, the tree is semantically correct.
    ///
    /// However, additional validation is required to ensure the source is
    /// a valid WDL document.
    pub fn parse(source: &str) -> (Self, Vec<Error>) {
        let parser = Parser::new(Lexer::new(source));
        let events = grammar::document(source, parser);
        Self::build(source, events)
    }

    /// Builds the concrete syntax tree from a list of parser events.
    fn build(source: &str, events: Vec<Event>) -> (Self, Vec<Error>) {
        let mut builder = GreenNodeBuilder::default();
        let mut errors = Vec::new();

        for event in events {
            match event {
                Event::NodeStarted(SyntaxKind::Abandoned) => {
                    // The node was abandoned, so all the descendants of the
                    // node will attach to the current node
                }
                Event::NodeStarted(kind) => builder.start_node(kind.into()),
                Event::NodeFinished => builder.finish_node(),
                Event::Token { kind, span } => builder.token(
                    kind.into(),
                    &source[span.offset()..span.offset() + span.len()],
                ),
                Event::Error(error) => errors.push(error),
            }
        }

        (Self(SyntaxNode::new_root(builder.finish())), errors)
    }

    /// Gets the root syntax node of the tree.
    pub fn root(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Display for SyntaxTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for SyntaxTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
