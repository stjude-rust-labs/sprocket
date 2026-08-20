//! Python-specific APIs.

use pyo3::prelude::pyclass;
use pyo3::prelude::pymethods;
use rowan::GreenNode;
use rowan::SyntaxNode;
use rowan::SyntaxToken;
use rowan::TextSize;
use rowan::TokenAtOffset;
use rowan::ast::SyntaxNodePtr;
use wdl_grammar::Diagnostic;
use wdl_grammar::SupportedVersion;
use wdl_grammar::WorkflowDescriptionLanguage;

use crate::Ast;
use crate::Document;
use crate::PyDocument;
use crate::VersionStatement;

/// A [`SyntaxNode<WorkflowDescriptionLanguage>`] that is [`Send`] and [`Sync`].
///
/// Use the [`From`] impls to convert to and from
/// [`SyntaxNode<WorkflowDescriptionLanguage>`].
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ThreadSafeSyntaxNode {
    /// The root node of the syntax tree.
    root: GreenNode,
    /// A pointer to where the syntax node is in the tree.
    node_ptr: SyntaxNodePtr<WorkflowDescriptionLanguage>,
}

impl From<SyntaxNode<WorkflowDescriptionLanguage>> for ThreadSafeSyntaxNode {
    fn from(node: SyntaxNode<WorkflowDescriptionLanguage>) -> Self {
        Self {
            root: node.ancestors().last().unwrap().green().into_owned(),
            node_ptr: SyntaxNodePtr::new(&node),
        }
    }
}

impl From<ThreadSafeSyntaxNode> for SyntaxNode<WorkflowDescriptionLanguage> {
    fn from(node: ThreadSafeSyntaxNode) -> Self {
        node.node_ptr.to_node(&SyntaxNode::new_root(node.root))
    }
}

/// A [`SyntaxToken<WorkflowDescriptionLanguage>`] that is [`Send`] and
/// [`Sync`].
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ThreadSafeSyntaxToken {
    /// The parent node of this token.
    parent: ThreadSafeSyntaxNode,
    /// The index that this token starts at, relative to the root node.
    offset: TextSize,
}

impl From<SyntaxToken<WorkflowDescriptionLanguage>> for ThreadSafeSyntaxToken {
    fn from(token: SyntaxToken<WorkflowDescriptionLanguage>) -> Self {
        let offset = token.text_range().start();

        Self {
            parent: token
                .parent()
                .expect("token must have a parent node")
                .into(),
            offset,
        }
    }
}

impl From<ThreadSafeSyntaxToken> for SyntaxToken<WorkflowDescriptionLanguage> {
    fn from(token: ThreadSafeSyntaxToken) -> Self {
        let node: SyntaxNode<_> = token.parent.into();

        match node.token_at_offset(token.offset) {
            // The offset represents where the token begins. If the offset is 7, we might encounter
            // `Between(VersionKeyword@0..7, Whitespace@7..8)`. The `Whitespace` begins at 7, so we
            // select right token.
            TokenAtOffset::Between(_, token) | TokenAtOffset::Single(token) => token,
            TokenAtOffset::None => unreachable!(),
        }
    }
}

/// A trait implemented by AST nodes.
#[pyclass(module = "sprocket_bio.ast", name = "AstNode", subclass, frozen)]
#[expect(missing_debug_implementations)]
pub struct PyAstNode;

/// A trait implemented by AST tokens.
#[pyclass(module = "sprocket_bio.ast", name = "AstToken", subclass, frozen)]
#[expect(missing_debug_implementations)]
pub struct PyAstToken;

// `Document::parse()` is a static method and is in a separate `impl` block from
// `Document`'s other methods, which isn't supported by `#[ast_methods]`, so we
// use `#[pymethods]` directly.
#[pymethods]
impl PyDocument {
    /// Parses a document from the given source.
    ///
    /// This optionally takes a `fallback_version`, which will be used if a
    /// [`SupportedVersion`] cannot be determined from the document.
    ///
    /// A document and its AST elements are trivially cloned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use wdl_ast::{Document, AstToken, Ast};
    /// use wdl_grammar::SupportedVersion;
    /// use wdl_grammar::version::V1;
    /// let (document, diagnostics) = Document::parse("version 1.1", None);
    /// assert!(diagnostics.is_empty());
    ///
    /// assert_eq!(
    ///     document
    ///         .version_statement()
    ///         .expect("should have version statement")
    ///         .version()
    ///         .text(),
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
    ///
    /// With a fallback version:
    ///
    /// ```rust
    /// # use wdl_ast::{Document, AstToken, Ast};
    /// # use wdl_grammar::version::{SupportedVersion, V1};
    /// let fallback_version = SupportedVersion::V1(V1::Three);
    ///
    /// let (document, diagnostics) = Document::parse("version foo", Some(fallback_version));
    /// assert!(diagnostics.is_empty());
    ///
    /// assert_eq!(
    ///     document
    ///         .version_statement()
    ///         .expect("should have version statement")
    ///         .version()
    ///         .text(),
    ///     "foo" // Not a valid version!
    /// );
    ///
    /// match document.ast_with_version_fallback(Some(fallback_version)) {
    ///     Ast::V1(ast) => {
    ///         assert_eq!(ast.items().count(), 0);
    ///     }
    ///     Ast::Unsupported => panic!("should be a V1 AST"),
    /// }
    /// ```
    #[staticmethod]
    fn parse(
        source: &str,
        fallback_version: Option<SupportedVersion>,
    ) -> (Document, Vec<Diagnostic>) {
        Document::parse(source, fallback_version)
    }

    /// Gets the version statement of the document.
    ///
    /// This can be used to determine the version of the document that was
    /// parsed.
    ///
    /// A return value of `None` signifies a missing version statement.
    fn version_statement(&self) -> Option<VersionStatement> {
        Document::from(self.clone()).version_statement()
    }

    /// Gets the AST representation of the document.
    fn ast(&self) -> Ast {
        Document::from(self.clone()).ast()
    }

    /// Gets the AST representation of the document, falling back to the
    /// specified WDL version if the document's version statement contains
    /// an unrecognized version.
    ///
    /// A fallback version of `None` does not have any fallback behavior, and is
    /// equivalent to calling [`Document::ast()`].
    ///
    /// <div class="warning">
    ///
    /// It is the caller's responsibility to ensure that falling back to the
    /// given version does not introduce unwanted behavior. For applications
    /// where correctness is essential, the caller should only provide a
    /// version that is known to be compatible with the version declared in
    /// the document.
    ///
    /// </div>
    fn ast_with_version_fallback(&self, fallback_version: Option<SupportedVersion>) -> Ast {
        Document::from(self.clone()).ast_with_version_fallback(fallback_version)
    }
}

#[cfg(test)]
mod tests {
    use wdl_grammar::SyntaxTree;

    use super::*;

    // Assert `ThreadSafeSyntaxNode` and `ThreadSafeSyntaxToken` are actually thread
    // safe.
    const _: () = {
        const fn assert_send<T: Send>() {}
        const fn assert_sync<T: Sync>() {}

        assert_send::<ThreadSafeSyntaxNode>();
        assert_sync::<ThreadSafeSyntaxNode>();

        assert_send::<ThreadSafeSyntaxToken>();
        assert_sync::<ThreadSafeSyntaxToken>();
    };

    #[test]
    fn syntax_round_trips() {
        const SOURCE: &str = r#"version 1.3

task say_hello {
    input {
        String greeting
    }

    command <<<
        echo "~{greeting}, world!"
    >>>

    output {
        String out = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}"#;

        let (tree, diagnostics) = SyntaxTree::parse(SOURCE, None);

        assert!(diagnostics.is_empty(), "{diagnostics:#?}");

        for element in tree.root().descendants_with_tokens() {
            match element {
                rowan::NodeOrToken::Node(node) => {
                    let round_trip = SyntaxNode::from(ThreadSafeSyntaxNode::from(node.clone()));
                    assert_eq!(node, round_trip);
                }
                rowan::NodeOrToken::Token(token) => {
                    let round_trip = SyntaxToken::from(ThreadSafeSyntaxToken::from(token.clone()));
                    assert_eq!(token, round_trip);
                }
            }
        }
    }
}
