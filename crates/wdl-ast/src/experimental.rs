//! Module for the experimental AST implementation.
//!
//! The new implementation is based on the experimental parser which
//! implements an infallible parse and uses `rowan` for CST representation.
//!
//! The experimental AST implementation is effectively a facade over the
//! CST to represent a WDL document semantically.
//!
//! See [Document::parse][Document::parse] for parsing WDL source into
//! an AST.
//!
//! When it is ready, the `experimental` module will be removed and this
//! implementation will replace the existing AST; all existing rules will
//! be updated to use the new AST representation at that time.

use std::sync::Arc;

use rowan::ast::support::child;
use rowan::ast::AstNode;
use rowan::NodeOrToken;
use wdl_grammar::experimental::parser::Error;
use wdl_grammar::experimental::tree::SyntaxKind;
use wdl_grammar::experimental::tree::SyntaxNode;
use wdl_grammar::experimental::tree::SyntaxToken;
use wdl_grammar::experimental::tree::SyntaxTree;
use wdl_grammar::experimental::tree::WorkflowDescriptionLanguage;

pub mod v1;

/// Gets a token of a given parent that can cast to the given type.
fn token<T: AstToken>(parent: &SyntaxNode) -> Option<T> {
    parent
        .children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find_map(T::cast)
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

/// Represents the result of a parse: a [Document] and a list of errors.
///
/// A parse always produces a [Document], even for documents that contain
/// syntax errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parse {
    /// The document that was parsed.
    document: Document,
    /// The parse errors that were encountered.
    errors: Option<Arc<[Error]>>,
}

impl Parse {
    /// Constructs a new parse result from the given document and list of
    /// parser errors.
    fn new(document: Document, errors: Vec<Error>) -> Parse {
        Self {
            document,
            errors: if errors.is_empty() {
                None
            } else {
                Some(errors.into())
            },
        }
    }

    /// Gets the root syntax node from the parse.
    pub fn root(&self) -> &SyntaxNode {
        &self.document.0
    }

    /// Gets the errors from the parse.
    pub fn errors(&self) -> &[Error] {
        self.errors.as_deref().unwrap_or_default()
    }

    /// Gets the document resulting from the parse.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Converts the parse into a result.
    pub fn into_result(self) -> Result<Document, Arc<[Error]>> {
        match self.errors {
            Some(e) => Err(e),
            None => Ok(self.document),
        }
    }
}

/// Represents a single WDL document.
///
/// See [Document::ast] for getting a version-specific Abstract
/// Syntax Tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Document(SyntaxNode);

impl Document {
    /// Parses a document from the given source.
    ///
    /// A document and its AST elements are trivially cloned.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use wdl_ast::experimental::{Document, AstToken, Ast};
    /// let parse = Document::parse("version 1.1");
    /// assert!(parse.errors().is_empty());
    ///
    /// let document = parse.document();
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
    pub fn parse(source: &str) -> Parse {
        let (tree, errors) = SyntaxTree::parse(source);
        Parse::new(
            Document::cast(tree.into_syntax()).expect("document should cast"),
            errors,
        )
    }

    /// Gets the version statement of the document.
    ///
    /// This can be used to determine the version of the document that was
    /// parsed.
    ///
    /// A return value of `None` signifies a missing version statement.
    pub fn version_statement(&self) -> Option<VersionStatement> {
        child(&self.0)
    }

    /// Gets the AST representation of the document.
    pub fn ast(&self) -> Ast {
        self.version_statement()
            .as_ref()
            .map(|s| {
                let v = s.version();
                match v.as_str() {
                    "1.0" | "1.1" => {
                        Ast::V1(v1::Ast::cast(self.0.clone()).expect("root should cast"))
                    }
                    _ => Ast::Unsupported,
                }
            })
            .unwrap_or(Ast::Unsupported)
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
