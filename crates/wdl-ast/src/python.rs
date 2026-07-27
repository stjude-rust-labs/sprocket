//! Python-specific APIs.

use pyo3::prelude::pyclass;
use rowan::GreenNode;
use rowan::SyntaxNode;
use rowan::SyntaxToken;
use rowan::TextSize;
use rowan::TokenAtOffset;
use rowan::ast::SyntaxNodePtr;
use wdl_grammar::WorkflowDescriptionLanguage;

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
