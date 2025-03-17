//! Elements used during formatting.

use std::iter::Peekable;

use nonempty::NonEmpty;
use wdl_ast::Element;
use wdl_ast::Node;

pub mod node;

/// An iterator that asserts that all items have been consumed when dropped.
pub struct AssertConsumedIter<I: Iterator>(Peekable<I>);

impl<I> AssertConsumedIter<I>
where
    I: Iterator,
{
    /// Creates a new [`AssertConsumedIter`].
    pub fn new(iter: I) -> Self {
        Self(iter.peekable())
    }
}

impl<I> Iterator for AssertConsumedIter<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<I> Drop for AssertConsumedIter<I>
where
    I: Iterator,
{
    fn drop(&mut self) {
        assert!(
            self.0.peek().is_none(),
            "not all iterator items were consumed!"
        );
    }
}

/// A formattable element.
#[derive(Clone, Debug)]
pub struct FormatElement {
    /// The inner element.
    element: Element,

    /// Children as format elements.
    children: Option<NonEmpty<Box<FormatElement>>>,
}

impl FormatElement {
    /// Creates a new [`FormatElement`].
    pub fn new(element: Element, children: Option<NonEmpty<Box<FormatElement>>>) -> Self {
        Self { element, children }
    }

    /// Gets the inner element.
    pub fn element(&self) -> &Element {
        &self.element
    }

    /// Gets the children for this node.
    pub fn children(&self) -> Option<AssertConsumedIter<impl Iterator<Item = &FormatElement>>> {
        self.children
            .as_ref()
            // NOTE: we wrap the iterator in an [`AssertConsumedIter`] to ensure
            // that no children are ever forgotten to be formatted (they must be
            // explicitly consumed and dropped).
            .map(|children| AssertConsumedIter::new(children.iter().map(|c| c.as_ref())))
    }
}

/// An extension trait for formatting [`Element`]s.
pub trait AstElementFormatExt {
    /// Consumes `self` and returns the [`Element`] as a [`FormatElement`].
    fn into_format_element(self) -> FormatElement;
}

impl AstElementFormatExt for Element {
    fn into_format_element(self) -> FormatElement
    where
        Self: Sized,
    {
        let children = match &self {
            Element::Node(node) => collate(node),
            Element::Token(_) => None,
        };

        FormatElement::new(self, children)
    }
}

/// Collates the children of a particular node.
///
/// This function ignores trivia.
fn collate(node: &Node) -> Option<NonEmpty<Box<FormatElement>>> {
    let mut results = Vec::new();
    let stream = node.inner().children_with_tokens().filter_map(|syntax| {
        if syntax.kind().is_trivia() {
            None
        } else {
            Some(Element::cast(syntax))
        }
    });

    for element in stream {
        let children = match element {
            Element::Node(ref node) => collate(node),
            Element::Token(_) => None,
        };

        results.push(Box::new(FormatElement { element, children }));
    }

    if !results.is_empty() {
        let mut results = results.into_iter();
        // SAFETY: we just checked to ensure that `results` wasn't empty, so
        // this will always unwrap.
        let mut children = NonEmpty::new(results.next().unwrap());
        children.extend(results);
        Some(children)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document;
    use wdl_ast::Node;
    use wdl_ast::SyntaxKind;

    use crate::element::node::AstNodeFormatExt;

    #[test]
    fn smoke() {
        let (document, diagnostics) = Document::parse(
            "## WDL
version 1.2  # This is a comment attached to the version.

# This is a comment attached to the task keyword.
task foo # This is an inline comment on the task ident.
{

} # This is an inline comment on the task close brace.

# This is a comment attached to the workflow keyword.
workflow bar # This is an inline comment on the workflow ident.
{
  # This is attached to the call keyword.
  call foo {}
} # This is an inline comment on the workflow close brace.",
        );

        assert!(diagnostics.is_empty());
        let document = document.ast().into_v1().unwrap();

        let format_element = Node::Ast(document).into_format_element();
        let mut children = format_element.children().unwrap();

        // Version statement.

        let version = children.next().expect("version statement element");
        assert_eq!(
            version.element().inner().kind(),
            SyntaxKind::VersionStatementNode
        );

        let mut version_children = version.children().unwrap();
        assert_eq!(
            version_children.next().unwrap().element().kind(),
            SyntaxKind::VersionKeyword
        );
        assert_eq!(
            version_children.next().unwrap().element().kind(),
            SyntaxKind::Version
        );

        // Task Definition.

        let task = children.next().expect("task element");
        assert_eq!(
            task.element().inner().kind(),
            SyntaxKind::TaskDefinitionNode
        );

        // Children.

        let mut task_children = task.children().unwrap();
        assert_eq!(
            task_children.next().unwrap().element().kind(),
            SyntaxKind::TaskKeyword
        );

        let ident = task_children.next().unwrap();
        assert_eq!(ident.element().kind(), SyntaxKind::Ident);

        assert_eq!(
            task_children.next().unwrap().element().kind(),
            SyntaxKind::OpenBrace
        );
        assert_eq!(
            task_children.next().unwrap().element().kind(),
            SyntaxKind::CloseBrace
        );

        assert!(task_children.next().is_none());

        // Workflow Definition.

        let workflow = children.next().expect("workflow element");
        assert_eq!(
            workflow.element().inner().kind(),
            SyntaxKind::WorkflowDefinitionNode
        );

        // Children.

        let mut workflow_children = workflow.children().unwrap();

        assert_eq!(
            workflow_children.next().unwrap().element().kind(),
            SyntaxKind::WorkflowKeyword
        );

        let ident = workflow_children.next().unwrap();
        assert_eq!(ident.element().kind(), SyntaxKind::Ident);

        assert_eq!(
            workflow_children.next().unwrap().element().kind(),
            SyntaxKind::OpenBrace
        );

        let call = workflow_children.next().unwrap();
        assert_eq!(call.element().kind(), SyntaxKind::CallStatementNode);

        assert_eq!(
            workflow_children.next().unwrap().element().kind(),
            SyntaxKind::CloseBrace
        );

        assert!(workflow_children.next().is_none());
    }

    #[test]
    #[should_panic]
    fn unconsumed_children_nodes_panic() {
        let (document, diagnostics) = Document::parse(
            "## WDL
version 1.2  # This is a comment attached to the version.

# This is a comment attached to the task keyword.
task foo # This is an inline comment on the task ident.
{

} # This is an inline comment on the task close brace.",
        );

        assert!(diagnostics.is_empty());
        let document = document.ast().into_v1().unwrap();

        let format_element = Node::Ast(document).into_format_element();
        fn inner(format_element: &crate::element::FormatElement) {
            let mut _children = format_element.children().unwrap();
        }
        inner(&format_element);
    }
}
