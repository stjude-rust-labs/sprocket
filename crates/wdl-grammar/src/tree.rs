//! Module for the concrete syntax tree (CST) representation.

pub mod dive;

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt;
use std::iter;

use rowan::GreenNodeBuilder;
use rowan::GreenNodeData;
use strum::VariantArray;

use super::Diagnostic;
use super::grammar;
use super::lexer::Lexer;
use super::parser::Event;
use crate::parser::Parser;

/// Represents the kind of syntax element (node or token) in a WDL concrete
/// syntax tree (CST).
///
/// Nodes have at least one token child and represent a syntactic construct.
///
/// Tokens are terminal and represent any span of the source.
///
/// This enumeration is a union of all supported WDL tokens and nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
#[repr(u16)]
pub enum SyntaxKind {
    /// The token is unknown to WDL.
    Unknown,
    /// The token represents unparsed source.
    ///
    /// Unparsed source occurs in WDL source files with unsupported versions.
    Unparsed,
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
    /// The `Object` type keyword token.
    ObjectTypeKeyword,
    /// The `Pair` type keyword token.
    PairTypeKeyword,
    /// The `String` type keyword token.
    StringTypeKeyword,
    /// The `after` keyword token.
    AfterKeyword,
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
    /// The `env` keyword token.
    EnvKeyword,
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
    /// The `None` keyword.
    NoneKeyword,
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
    /// The 1.2 `Directory` type keyword token.
    DirectoryTypeKeyword,
    /// The 1.2 `hints` keyword token.
    HintsKeyword,
    /// The 1.2 `requirements` keyword token.
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
    /// The `**` symbol token.
    Exponentiation,
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
    /// A literal text part of a string.
    LiteralStringText,
    /// A literal text part of a command.
    LiteralCommandText,
    /// A placeholder open token.
    PlaceholderOpen,

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
    /// Represents an import statement node.
    ImportStatementNode,
    /// Represents an import alias node.
    ImportAliasNode,
    /// Represents a struct definition node.
    StructDefinitionNode,
    /// Represents a task definition node.
    TaskDefinitionNode,
    /// Represents a workflow definition node.
    WorkflowDefinitionNode,
    /// Represents an unbound declaration node.
    UnboundDeclNode,
    /// Represents a bound declaration node.
    BoundDeclNode,
    /// Represents an input section node.
    InputSectionNode,
    /// Represents an output section node.
    OutputSectionNode,
    /// Represents a command section node.
    CommandSectionNode,
    /// Represents a requirements section node.
    RequirementsSectionNode,
    /// Represents a requirements item node.
    RequirementsItemNode,
    /// Represents a hints section node in a task.
    TaskHintsSectionNode,
    /// Represents a hints section node in a workflow.
    WorkflowHintsSectionNode,
    /// Represents a hints item node in a task.
    TaskHintsItemNode,
    /// Represents a hints item node in a workflow.
    WorkflowHintsItemNode,
    /// Represents a literal object in a workflow hints item value.
    WorkflowHintsObjectNode,
    /// Represents an item in a workflow hints object.
    WorkflowHintsObjectItemNode,
    /// Represents a literal array in a workflow hints item value.
    WorkflowHintsArrayNode,
    /// Represents a runtime section node.
    RuntimeSectionNode,
    /// Represents a runtime item node.
    RuntimeItemNode,
    /// Represents a primitive type node.
    PrimitiveTypeNode,
    /// Represents a map type node.
    MapTypeNode,
    /// Represents an array type node.
    ArrayTypeNode,
    /// Represents a pair type node.
    PairTypeNode,
    /// Represents an object type node.
    ObjectTypeNode,
    /// Represents a type reference node.
    TypeRefNode,
    /// Represents a metadata section node.
    MetadataSectionNode,
    /// Represents a parameter metadata section node.
    ParameterMetadataSectionNode,
    /// Represents a metadata object item node.
    MetadataObjectItemNode,
    /// Represents a metadata object node.
    MetadataObjectNode,
    /// Represents a metadata array node.
    MetadataArrayNode,
    /// Represents a literal integer node.
    LiteralIntegerNode,
    /// Represents a literal float node.
    LiteralFloatNode,
    /// Represents a literal boolean node.
    LiteralBooleanNode,
    /// Represents a literal `None` node.
    LiteralNoneNode,
    /// Represents a literal null node.
    LiteralNullNode,
    /// Represents a literal string node.
    LiteralStringNode,
    /// Represents a literal pair node.
    LiteralPairNode,
    /// Represents a literal array node.
    LiteralArrayNode,
    /// Represents a literal map node.
    LiteralMapNode,
    /// Represents a literal map item node.
    LiteralMapItemNode,
    /// Represents a literal object node.
    LiteralObjectNode,
    /// Represents a literal object item node.
    LiteralObjectItemNode,
    /// Represents a literal struct node.
    LiteralStructNode,
    /// Represents a literal struct item node.
    LiteralStructItemNode,
    /// Represents a literal hints node.
    LiteralHintsNode,
    /// Represents a literal hints item node.
    LiteralHintsItemNode,
    /// Represents a literal input node.
    LiteralInputNode,
    /// Represents a literal input item node.
    LiteralInputItemNode,
    /// Represents a literal output node.
    LiteralOutputNode,
    /// Represents a literal output item node.
    LiteralOutputItemNode,
    /// Represents a parenthesized expression node.
    ParenthesizedExprNode,
    /// Represents a name reference expression node.
    NameRefExprNode,
    /// Represents an `if` expression node.
    IfExprNode,
    /// Represents a logical not expression node.
    LogicalNotExprNode,
    /// Represents a negation expression node.
    NegationExprNode,
    /// Represents a logical `OR` expression node.
    LogicalOrExprNode,
    /// Represents a logical `AND` expression node.
    LogicalAndExprNode,
    /// Represents an equality expression node.
    EqualityExprNode,
    /// Represents an inequality expression node.
    InequalityExprNode,
    /// Represents a "less than" expression node.
    LessExprNode,
    /// Represents a "less than or equal to" expression node.
    LessEqualExprNode,
    /// Represents a "greater than" expression node.
    GreaterExprNode,
    /// Represents a "greater than or equal to" expression node.
    GreaterEqualExprNode,
    /// Represents an addition expression node.
    AdditionExprNode,
    /// Represents a subtraction expression node.
    SubtractionExprNode,
    /// Represents a multiplication expression node.
    MultiplicationExprNode,
    /// Represents a division expression node.
    DivisionExprNode,
    /// Represents a modulo expression node.
    ModuloExprNode,
    /// Represents a exponentiation expr node.
    ExponentiationExprNode,
    /// Represents a call expression node.'
    CallExprNode,
    /// Represents an index expression node.
    IndexExprNode,
    /// Represents an an access expression node.
    AccessExprNode,
    /// Represents a placeholder node in a string literal.
    PlaceholderNode,
    /// Placeholder `sep` option node.
    PlaceholderSepOptionNode,
    /// Placeholder `default` option node.
    PlaceholderDefaultOptionNode,
    /// Placeholder `true`/`false` option node.
    PlaceholderTrueFalseOptionNode,
    /// Represents a conditional statement node.
    ConditionalStatementNode,
    /// Represents a scatter statement node.
    ScatterStatementNode,
    /// Represents a call statement node.
    CallStatementNode,
    /// Represents a call target node in a call statement.
    CallTargetNode,
    /// Represents a call alias node in a call statement.
    CallAliasNode,
    /// Represents an `after` clause node in a call statement.
    CallAfterNode,
    /// Represents a call input item node.
    CallInputItemNode,

    // WARNING: this must always be the last variant.
    /// The exclusive maximum syntax kind value.
    MAX,
}

impl SyntaxKind {
    /// Returns whether the token is a symbolic [`SyntaxKind`].
    ///
    /// Generally speaking, symbolic [`SyntaxKind`]s have special meanings
    /// during parsing—they are not real elements of the grammar but rather an
    /// implementation detail.
    pub fn is_symbolic(&self) -> bool {
        matches!(
            self,
            Self::Abandoned | Self::Unknown | Self::Unparsed | Self::MAX
        )
    }

    /// Describes the syntax kind.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Unknown => unreachable!(),
            Self::Unparsed => unreachable!(),
            Self::Whitespace => "whitespace",
            Self::Comment => "comment",
            Self::Version => "version",
            Self::Float => "float",
            Self::Integer => "integer",
            Self::Ident => "identifier",
            Self::SingleQuote => "single quote",
            Self::DoubleQuote => "double quote",
            Self::OpenHeredoc => "open heredoc",
            Self::CloseHeredoc => "close heredoc",
            Self::ArrayTypeKeyword => "`Array` type keyword",
            Self::BooleanTypeKeyword => "`Boolean` type keyword",
            Self::FileTypeKeyword => "`File` type keyword",
            Self::FloatTypeKeyword => "`Float` type keyword",
            Self::IntTypeKeyword => "`Int` type keyword",
            Self::MapTypeKeyword => "`Map` type keyword",
            Self::ObjectTypeKeyword => "`Object` type keyword",
            Self::PairTypeKeyword => "`Pair` type keyword",
            Self::StringTypeKeyword => "`String` type keyword",
            Self::AfterKeyword => "`after` keyword",
            Self::AliasKeyword => "`alias` keyword",
            Self::AsKeyword => "`as` keyword",
            Self::CallKeyword => "`call` keyword",
            Self::CommandKeyword => "`command` keyword",
            Self::ElseKeyword => "`else` keyword",
            Self::EnvKeyword => "`env` keyword",
            Self::FalseKeyword => "`false` keyword",
            Self::IfKeyword => "`if` keyword",
            Self::InKeyword => "`in` keyword",
            Self::ImportKeyword => "`import` keyword",
            Self::InputKeyword => "`input` keyword",
            Self::MetaKeyword => "`meta` keyword",
            Self::NoneKeyword => "`None` keyword",
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
            Self::DirectoryTypeKeyword => "`Directory` type keyword",
            Self::HintsKeyword => "`hints` keyword",
            Self::RequirementsKeyword => "`requirements` keyword",
            Self::OpenBrace => "`{` symbol",
            Self::CloseBrace => "`}` symbol",
            Self::OpenBracket => "`[` symbol",
            Self::CloseBracket => "`]` symbol",
            Self::Assignment => "`=` symbol",
            Self::Colon => "`:` symbol",
            Self::Comma => "`,` symbol",
            Self::OpenParen => "`(` symbol",
            Self::CloseParen => "`)` symbol",
            Self::QuestionMark => "`?` symbol",
            Self::Exclamation => "`!` symbol",
            Self::Plus => "`+` symbol",
            Self::Minus => "`-` symbol",
            Self::LogicalOr => "`||` symbol",
            Self::LogicalAnd => "`&&` symbol",
            Self::Asterisk => "`*` symbol",
            Self::Exponentiation => "`**` symbol",
            Self::Slash => "`/` symbol",
            Self::Percent => "`%` symbol",
            Self::Equal => "`==` symbol",
            Self::NotEqual => "`!=` symbol",
            Self::LessEqual => "`<=` symbol",
            Self::GreaterEqual => "`>=` symbol",
            Self::Less => "`<` symbol",
            Self::Greater => "`>` symbol",
            Self::Dot => "`.` symbol",
            Self::LiteralStringText => "literal string text",
            Self::LiteralCommandText => "literal command text",
            Self::PlaceholderOpen => "placeholder open",
            Self::Abandoned => unreachable!(),
            Self::RootNode => "root node",
            Self::VersionStatementNode => "version statement",
            Self::ImportStatementNode => "import statement",
            Self::ImportAliasNode => "import alias",
            Self::StructDefinitionNode => "struct definition",
            Self::TaskDefinitionNode => "task definition",
            Self::WorkflowDefinitionNode => "workflow definition",
            Self::UnboundDeclNode => "declaration without assignment",
            Self::BoundDeclNode => "declaration with assignment",
            Self::InputSectionNode => "input section",
            Self::OutputSectionNode => "output section",
            Self::CommandSectionNode => "command section",
            Self::RequirementsSectionNode => "requirements section",
            Self::RequirementsItemNode => "requirements item",
            Self::TaskHintsSectionNode | Self::WorkflowHintsSectionNode => "hints section",
            Self::TaskHintsItemNode | Self::WorkflowHintsItemNode => "hints item",
            Self::WorkflowHintsObjectNode => "literal object",
            Self::WorkflowHintsObjectItemNode => "literal object item",
            Self::WorkflowHintsArrayNode => "literal array",
            Self::RuntimeSectionNode => "runtime section",
            Self::RuntimeItemNode => "runtime item",
            Self::PrimitiveTypeNode => "primitive type",
            Self::MapTypeNode => "map type",
            Self::ArrayTypeNode => "array type",
            Self::PairTypeNode => "pair type",
            Self::ObjectTypeNode => "object type",
            Self::TypeRefNode => "type reference",
            Self::MetadataSectionNode => "metadata section",
            Self::ParameterMetadataSectionNode => "parameter metadata section",
            Self::MetadataObjectItemNode => "metadata object item",
            Self::MetadataObjectNode => "metadata object",
            Self::MetadataArrayNode => "metadata array",
            Self::LiteralIntegerNode => "literal integer",
            Self::LiteralFloatNode => "literal float",
            Self::LiteralBooleanNode => "literal boolean",
            Self::LiteralNoneNode => "literal `None`",
            Self::LiteralNullNode => "literal null",
            Self::LiteralStringNode => "literal string",
            Self::LiteralPairNode => "literal pair",
            Self::LiteralArrayNode => "literal array",
            Self::LiteralMapNode => "literal map",
            Self::LiteralMapItemNode => "literal map item",
            Self::LiteralObjectNode => "literal object",
            Self::LiteralObjectItemNode => "literal object item",
            Self::LiteralStructNode => "literal struct",
            Self::LiteralStructItemNode => "literal struct item",
            Self::LiteralHintsNode => "literal hints",
            Self::LiteralHintsItemNode => "literal hints item",
            Self::LiteralInputNode => "literal input",
            Self::LiteralInputItemNode => "literal input item",
            Self::LiteralOutputNode => "literal output",
            Self::LiteralOutputItemNode => "literal output item",
            Self::ParenthesizedExprNode => "parenthesized expression",
            Self::NameRefExprNode => "name reference expression",
            Self::IfExprNode => "`if` expression",
            Self::LogicalNotExprNode => "logical not expression",
            Self::NegationExprNode => "negation expression",
            Self::LogicalOrExprNode => "logical OR expression",
            Self::LogicalAndExprNode => "logical AND expression",
            Self::EqualityExprNode => "equality expression",
            Self::InequalityExprNode => "inequality expression",
            Self::LessExprNode => "less than expression",
            Self::LessEqualExprNode => "less than or equal to expression",
            Self::GreaterExprNode => "greater than expression",
            Self::GreaterEqualExprNode => "greater than or equal to expression",
            Self::AdditionExprNode => "addition expression",
            Self::SubtractionExprNode => "subtraction expression",
            Self::MultiplicationExprNode => "multiplication expression",
            Self::DivisionExprNode => "division expression",
            Self::ModuloExprNode => "modulo expression",
            Self::ExponentiationExprNode => "exponentiation expression",
            Self::CallExprNode => "call expression",
            Self::IndexExprNode => "index expression",
            Self::AccessExprNode => "access expression",
            Self::PlaceholderNode => "placeholder",
            Self::PlaceholderSepOptionNode => "placeholder `sep` option",
            Self::PlaceholderDefaultOptionNode => "placeholder `default` option",
            Self::PlaceholderTrueFalseOptionNode => "placeholder `true`/`false` option",
            Self::ConditionalStatementNode => "conditional statement",
            Self::ScatterStatementNode => "scatter statement",
            Self::CallStatementNode => "call statement",
            Self::CallTargetNode => "call target",
            Self::CallAliasNode => "call alias",
            Self::CallAfterNode => "call `after` clause",
            Self::CallInputItemNode => "call input item",
            Self::MAX => unreachable!(),
        }
    }

    /// Returns whether the [`SyntaxKind`] is trivia.
    pub fn is_trivia(&self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
    }
}

/// Every [`SyntaxKind`] variant.
pub static ALL_SYNTAX_KIND: &[SyntaxKind] = SyntaxKind::VARIANTS;

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

/// Constructs a concrete syntax tree from a list of parser events.
pub fn construct_tree(source: &str, mut events: Vec<Event>) -> SyntaxNode {
    let mut builder = GreenNodeBuilder::default();
    let mut ancestors = Vec::new();

    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::abandoned()) {
            Event::NodeStarted {
                kind,
                forward_parent,
            } => {
                // Walk the forward parent chain, if there is one, and push
                // each forward parent to the ancestors list
                ancestors.push(kind);
                let mut idx = i;
                let mut fp: Option<usize> = forward_parent;
                while let Some(distance) = fp {
                    idx += distance;
                    fp = match std::mem::replace(&mut events[idx], Event::abandoned()) {
                        Event::NodeStarted {
                            kind,
                            forward_parent,
                        } => {
                            ancestors.push(kind);
                            forward_parent
                        }
                        _ => unreachable!(),
                    };
                }

                // As the current node was pushed first and then its ancestors, walk
                // the list in reverse to start the "oldest" ancestor first
                for kind in ancestors.drain(..).rev() {
                    if kind != SyntaxKind::Abandoned {
                        builder.start_node(kind.into());
                    }
                }
            }
            Event::NodeFinished => builder.finish_node(),
            Event::Token { kind, span } => {
                builder.token(kind.into(), &source[span.start()..span.end()])
            }
        }
    }

    SyntaxNode::new_root(builder.finish())
}

/// Represents an untyped concrete syntax tree.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SyntaxTree(SyntaxNode);

impl SyntaxTree {
    /// Parses WDL source to produce a syntax tree.
    ///
    /// A syntax tree is always returned, even for invalid WDL documents.
    ///
    /// Additionally, the list of diagnostics encountered during the parse is
    /// returned; if the list is empty, the tree is syntactically correct.
    ///
    /// However, additional validation is required to ensure the source is
    /// a valid WDL document.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use wdl_grammar::SyntaxTree;
    /// let (tree, diagnostics) = SyntaxTree::parse("version 1.1");
    /// assert!(diagnostics.is_empty());
    /// println!("{tree:#?}");
    /// ```
    pub fn parse(source: &str) -> (Self, Vec<Diagnostic>) {
        let parser = Parser::new(Lexer::new(source));
        let (events, mut diagnostics) = grammar::document(parser);
        diagnostics.sort();
        (Self(construct_tree(source, events)), diagnostics)
    }

    /// Gets the root syntax node of the tree.
    pub fn root(&self) -> &SyntaxNode {
        &self.0
    }

    /// Gets a copy of the underlying root green node for the tree.
    pub fn green(&self) -> Cow<'_, GreenNodeData> {
        self.0.green()
    }

    /// Converts the tree into a syntax node.
    pub fn into_syntax(self) -> SyntaxNode {
        self.0
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

/// An extension trait for [`SyntaxToken`]s.
pub trait SyntaxTokenExt {
    /// Gets all of the substantial preceding trivia for an element.
    fn preceding_trivia(&self) -> impl Iterator<Item = SyntaxToken>;

    /// Gets all of the substantial succeeding trivia for an element.
    #[deprecated(since = "0.14.0")]
    fn succeeding_trivia(&self) -> impl Iterator<Item = SyntaxToken>;

    /// Get any inline comment directly following an element on the
    /// same line.
    fn inline_comment(&self) -> Option<SyntaxToken>;
}

impl SyntaxTokenExt for SyntaxToken {
    fn preceding_trivia(&self) -> impl Iterator<Item = SyntaxToken> {
        let mut tokens = VecDeque::new();
        let mut cur = self.prev_token();
        while let Some(token) = cur {
            cur = token.prev_token();
            // Stop at first non-trivia
            if !token.kind().is_trivia() {
                break;
            }
            // Stop if a comment is not on its own line
            if token.kind() == SyntaxKind::Comment
                && let Some(prev) = token.prev_token()
            {
                if prev.kind() == SyntaxKind::Whitespace {
                    let has_newlines = prev.text().chars().any(|c| c == '\n');
                    // If there are newlines in 'prev' then we know
                    // that the comment is on its own line.
                    // The comment may still be on its own line if
                    // 'prev' does not have newlines and nothing comes
                    // before 'prev'.
                    if !has_newlines && prev.prev_token().is_some() {
                        break;
                    }
                } else {
                    // There is something else on this line before the comment.
                    break;
                }
            }
            // Filter out whitespace that is not substantial
            match token.kind() {
                SyntaxKind::Whitespace
                    if token.text().chars().filter(|c| *c == '\n').count() > 1 =>
                {
                    tokens.push_front(token);
                }
                SyntaxKind::Comment => {
                    tokens.push_front(token);
                }
                _ => {}
            }
        }
        tokens.into_iter()
    }

    fn succeeding_trivia(&self) -> impl Iterator<Item = SyntaxToken> {
        let mut next = self.next_token();
        iter::from_fn(move || {
            let cur = next.clone()?;
            next = cur.next_token();
            Some(cur)
        })
        .take_while(|t| {
            // Stop at first non-trivia
            t.kind().is_trivia()
        })
        .filter(|t| {
            // Filter out whitespace that is not substantial
            if t.kind() == SyntaxKind::Whitespace {
                return t.text().chars().filter(|c| *c == '\n').count() > 1;
            }
            true
        })
    }

    fn inline_comment(&self) -> Option<SyntaxToken> {
        let mut next = self.next_token();
        iter::from_fn(move || {
            let cur = next.clone()?;
            next = cur.next_token();
            Some(cur)
        })
        .take_while(|t| {
            // Stop at non-trivia
            if !t.kind().is_trivia() {
                return false;
            }
            // Stop on first whitespace containing a newline
            if t.kind() == SyntaxKind::Whitespace {
                return !t.text().chars().any(|c| c == '\n');
            }
            true
        })
        .find(|t| t.kind() == SyntaxKind::Comment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntaxTree;

    #[test]
    fn preceding_comments() {
        let (tree, diagnostics) = SyntaxTree::parse(
            "version 1.2

# This comment should not be included
task foo {} # This comment should not be included

# Some
# comments
# are
# long
    
# Others are short

#     and, yet    another
workflow foo {} # This should not be collected.

# This comment should not be included either.",
        );

        assert!(diagnostics.is_empty());

        let workflow = tree.root().last_child().unwrap();
        assert_eq!(workflow.kind(), SyntaxKind::WorkflowDefinitionNode);
        let token = workflow.first_token().unwrap();
        let mut trivia = token.preceding_trivia();
        assert_eq!(trivia.next().unwrap().text(), "\n\n");
        assert_eq!(trivia.next().unwrap().text(), "# Some");
        assert_eq!(trivia.next().unwrap().text(), "# comments");
        assert_eq!(trivia.next().unwrap().text(), "# are");
        assert_eq!(trivia.next().unwrap().text(), "# long");
        assert_eq!(trivia.next().unwrap().text(), "\n    \n");
        assert_eq!(trivia.next().unwrap().text(), "# Others are short");
        assert_eq!(trivia.next().unwrap().text(), "\n\n");
        assert_eq!(trivia.next().unwrap().text(), "#     and, yet    another");
        assert!(trivia.next().is_none());
    }

    #[test]
    fn succeeding_comments() {
        let (tree, diagnostics) = SyntaxTree::parse(
            "version 1.2

# This comment should not be included
task foo {}

# This should not be collected.
workflow foo {} # Here is a comment that should be collected.

# This comment should be included too.",
        );

        assert!(diagnostics.is_empty());

        let workflow = tree.root().last_child().unwrap();
        assert_eq!(workflow.kind(), SyntaxKind::WorkflowDefinitionNode);
        let token = workflow.last_token().unwrap();
        #[allow(deprecated)]
        let mut trivia = token.succeeding_trivia();
        assert_eq!(
            trivia.next().unwrap().text(),
            "# Here is a comment that should be collected."
        );
        assert_eq!(trivia.next().unwrap().text(), "\n\n");
        assert_eq!(
            trivia.next().unwrap().text(),
            "# This comment should be included too."
        );
        assert!(trivia.next().is_none());
    }

    #[test]
    fn inline_comment() {
        let (tree, diagnostics) = SyntaxTree::parse(
            "version 1.2

# This comment should not be included
task foo {}

# This should not be collected.
workflow foo {} # Here is a comment that should be collected.

# This comment should not be included either.",
        );

        assert!(diagnostics.is_empty());

        let workflow = tree.root().last_child().unwrap();
        assert_eq!(workflow.kind(), SyntaxKind::WorkflowDefinitionNode);
        let comment = workflow.last_token().unwrap().inline_comment().unwrap();
        assert_eq!(
            comment.text(),
            "# Here is a comment that should be collected."
        );
    }
}
