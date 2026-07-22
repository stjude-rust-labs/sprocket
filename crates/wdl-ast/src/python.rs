//! Python-specific APIs.

use pyo3::prelude::pyclass;
use rowan::GreenNode;
use rowan::SyntaxNode;
use rowan::ast::SyntaxNodePtr;
use wdl_grammar::WorkflowDescriptionLanguage;

/// A [`SyntaxNode<WorkflowDescriptionLanguage>`] that is [`Send`] and [`Sync`].
///
/// Use the [`From`] impls to convert to and from
/// [`SyntaxNode<WorkflowDescriptionLanguage>`].
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ThreadSafeSyntaxNode {
    root: GreenNode,
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

/// A trait implemented by AST nodes.
#[pyclass(module = "sprocket_bio.ast", name = "AstNode", subclass, frozen)]
#[expect(missing_debug_implementations)]
pub struct PyAstNode;

#[cfg(test)]
mod tests {
    use super::*;

    // Assert `ThreadSafeSyntaxNode` is actually thread safe.
    const _: () = {
        const fn assert_send<T: Send>() {}
        const fn assert_sync<T: Sync>() {}

        assert_send::<ThreadSafeSyntaxNode>();
        assert_sync::<ThreadSafeSyntaxNode>();
    };
}
