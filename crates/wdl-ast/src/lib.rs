//! An abstract syntax tree for Workflow Description Language (WDL) documents.
//!
//! The AST implementation is effectively a facade over the concrete syntax tree
//! (CST) implemented by [SyntaxTree] from `wdl-grammar`.
//!
//! An AST is cheap to construct and may be cheaply cloned at any level.
//!
//! However, an AST (and the underlying CST) are immutable; updating the tree
//! requires replacing a node in the tree to produce a new tree. The unaffected
//! nodes of the replacement are reused from the old tree to the new tree.
//!
//! # Examples
//!
//! An example of parsing a WDL document into an AST and validating it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl_ast::Document;
//! use wdl_ast::Validator;
//!
//! let (document, diagnostics) = Document::parse(source);
//! if !diagnostics.is_empty() {
//!     // Handle the failure to parse
//! }
//!
//! let mut validator = Validator::default();
//! if let Err(diagnostics) = validator.validate(&document) {
//!     // Handle the failure to validate
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::fmt;

pub use rowan::ast::support;
pub use rowan::ast::AstChildren;
pub use rowan::ast::AstNode;
pub use rowan::Direction;
pub use wdl_grammar::version;
pub use wdl_grammar::Diagnostic;
pub use wdl_grammar::Label;
pub use wdl_grammar::Severity;
pub use wdl_grammar::Span;
pub use wdl_grammar::SupportedVersion;
pub use wdl_grammar::SyntaxElement;
pub use wdl_grammar::SyntaxKind;
pub use wdl_grammar::SyntaxNode;
pub use wdl_grammar::SyntaxToken;
pub use wdl_grammar::SyntaxTree;
pub use wdl_grammar::ToSpan;
pub use wdl_grammar::WorkflowDescriptionLanguage;

pub mod v1;

mod validation;
mod visitor;

pub use validation::*;
pub use visitor::*;

/// Gets a token of a given parent that can cast to the given type.
fn token<T: AstToken>(parent: &SyntaxNode) -> Option<T> {
    parent
        .children_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find_map(T::cast)
}

/// Gets the source span of the given node.
///
/// This differs from `SyntaxNode::text_range` in that it will exclude
/// leading trivia child tokens of the node.
pub fn span_of<N: AstNode<Language = WorkflowDescriptionLanguage>>(node: &N) -> Span {
    let start = node
        .syntax()
        .children_with_tokens()
        .find(|c| !matches!(c.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment))
        .expect("should have a non-trivia first child");
    let end = node
        .syntax()
        .last_child_or_token()
        .expect("should have last child");

    let start = start.text_range().start().into();
    Span::new(start, usize::from(end.text_range().end()) - start)
}

/// Represents the reason an AST node has been visited.
///
/// Each node is visited exactly once, but the visitor will receive
/// a call for entering the node and a call for exiting the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VisitReason {
    /// The visit has entered the node.
    Enter,
    /// The visit has exited the node.
    Exit,
}

/// The trait implemented on AST tokens to go from untyped `SyntaxToken`
/// to a typed representation.
///
/// The design of `AstToken` is directly inspired by `rust-analyzer`.
pub trait AstToken {
    /// Determines if the kind can be cast to this type representation.
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    /// Casts the untyped `SyntaxToken` to the typed representation.
    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized;

    /// Gets the untyped `SyntaxToken` of this AST token.
    fn syntax(&self) -> &SyntaxToken;

    /// Gets the text of the token.
    fn as_str(&self) -> &str {
        self.syntax().text()
    }

    /// Gets the source span of the token.
    fn span(&self) -> Span {
        self.syntax().text_range().to_span()
    }
}

/// Represents the AST of a [Document].
///
/// See [Document::ast].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ast {
    /// The WDL document specifies an unsupported version.
    Unsupported,
    /// The WDL document is V1.
    V1(v1::Ast),
}

impl Ast {
    /// Gets the AST as a V1 AST.
    ///
    /// Returns `None` if the AST is not a V1 AST.
    pub fn as_v1(&self) -> Option<&v1::Ast> {
        match self {
            Self::V1(ast) => Some(ast),
            _ => None,
        }
    }
}

/// Represents a single WDL document.
///
/// See [Document::ast] for getting a version-specific Abstract
/// Syntax Tree.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Document(SyntaxNode);

impl Document {
    /// Parses a document from the given source.
    ///
    /// A document and its AST elements are trivially cloned.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use wdl_ast::{Document, AstToken, Ast};
    /// let (document, diagnostics) = Document::parse("version 1.1");
    /// assert!(diagnostics.is_empty());
    ///
    /// assert_eq!(
    ///     document
    ///         .version_statement()
    ///         .expect("should have version statement")
    ///         .version()
    ///         .as_str(),
    ///     "1.1"
    /// );
    ///
    /// match document.ast() {
    ///     Ast::V1(ast) => {
    ///         assert_eq!(ast.items().count(), 0);
    ///     }
    ///     Ast::Unsupported => panic!("should be a V1 AST"),
    /// }
    /// ```
    pub fn parse(source: &str) -> (Self, Vec<Diagnostic>) {
        let (tree, diagnostics) = SyntaxTree::parse(source);
        (
            Document::cast(tree.into_syntax()).expect("document should cast"),
            diagnostics,
        )
    }

    /// Gets the version statement of the document.
    ///
    /// This can be used to determine the version of the document that was
    /// parsed.
    ///
    /// A return value of `None` signifies a missing version statement.
    pub fn version_statement(&self) -> Option<VersionStatement> {
        support::child(&self.0)
    }

    /// Gets the AST representation of the document.
    pub fn ast(&self) -> Ast {
        self.version_statement()
            .as_ref()
            .and_then(|s| s.version().as_str().parse::<SupportedVersion>().ok())
            .map(|_| Ast::V1(v1::Ast::cast(self.0.clone()).expect("root should cast")))
            .unwrap_or(Ast::Unsupported)
    }

    /// Visits the document with a pre-order traversal using the provided
    /// visitor to visit each element in the document.
    pub fn visit<V: Visitor>(&self, state: &mut V::State, visitor: &mut V) {
        visit(&self.0, state, visitor)
    }
}

impl AstNode for Document {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RootNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Represents a whitespace token in the AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Whitespace(SyntaxToken);

impl AstToken for Whitespace {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::Whitespace
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::Whitespace => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// Represents a comment token in the AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Comment(SyntaxToken);

impl AstToken for Comment {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::Comment
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::Comment => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// Represents a version statement in a WDL AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionStatement(SyntaxNode);

impl VersionStatement {
    /// Gets the version of the version statement.
    pub fn version(&self) -> Version {
        token(&self.0).expect("version statement must have a version token")
    }
}

impl AstNode for VersionStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::VersionStatementNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::VersionStatementNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a version in the AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(SyntaxToken);

impl AstToken for Version {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::Version
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::Version => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// Represents an identifier token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(SyntaxToken);

impl AstToken for Ident {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::Ident
    }

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::Ident => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// Helper for hashing any AST token on string representation alone.
///
/// Normally an AST token's equality and hash implementation work by comparing
/// the token's element in the AST; thus, two `Ident` tokens with the same name
/// but different positions in the tree will compare and hash differently.
#[derive(Debug, Clone)]
pub struct TokenStrHash<T>(T);

impl<T: AstToken> TokenStrHash<T> {
    /// Constructs a new token hash for the given token.
    pub fn new(token: T) -> Self {
        Self(token)
    }
}

impl<T: AstToken> PartialEq for TokenStrHash<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl<T: AstToken> Eq for TokenStrHash<T> {}

impl<T: AstToken> std::hash::Hash for TokenStrHash<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl<T: AstToken> std::borrow::Borrow<str> for TokenStrHash<T> {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl<T: AstToken> AsRef<T> for TokenStrHash<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}
