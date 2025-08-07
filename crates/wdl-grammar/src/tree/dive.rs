//! Utilities for traversing a syntax tree while collecting elements of interest
//! (i.e., "diving" for elements).

use std::iter::FusedIterator;

use rowan::Language;
use rowan::api::PreorderWithTokens;

use crate::SyntaxElement;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;

/// An iterator that performs a pre-order traversal of a
/// [`SyntaxNode`](rowan::SyntaxNode)'s descendants and yields all elements
/// while ignoring undesirable subtrees.
#[allow(missing_debug_implementations)]
pub struct DiveIterator<L, I>
where
    L: Language,
    I: Fn(&rowan::SyntaxNode<L>) -> bool,
{
    /// The iterator that performs the pre-order traversal of elements.
    it: PreorderWithTokens<L>,
    /// The function that evaluates when checking if the subtree beneath a node
    /// should be ignored.
    ignore_predicate: I,
}

impl<L, I> DiveIterator<L, I>
where
    L: Language,
    I: Fn(&rowan::SyntaxNode<L>) -> bool,
{
    /// Creates a new [`DiveIterator`].
    pub fn new(root: rowan::SyntaxNode<L>, ignore_predicate: I) -> Self {
        Self {
            it: root.preorder_with_tokens(),
            ignore_predicate,
        }
    }
}

impl<L, I> Iterator for DiveIterator<L, I>
where
    L: Language,
    I: Fn(&rowan::SyntaxNode<L>) -> bool,
{
    type Item = rowan::SyntaxElement<L>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.it.next() {
            let element = match event {
                rowan::WalkEvent::Enter(element) => element,
                rowan::WalkEvent::Leave(_) => continue,
            };

            if let rowan::SyntaxElement::Node(node) = &element
                && (self.ignore_predicate)(node)
            {
                self.it.skip_subtree();
                continue;
            }

            return Some(element);
        }

        None
    }
}

impl<L, I> FusedIterator for DiveIterator<L, I>
where
    L: Language,
    I: Fn(&rowan::SyntaxNode<L>) -> bool,
{
}

/// Elements of a syntax tree upon which a dive can be performed.
pub trait Divable<L>
where
    L: Language,
{
    /// Iterates over every element in the tree at the current root and yields
    /// the elements for which the given `match_predicate` evaluates to
    /// `true`.
    fn dive<M>(&self, match_predicate: M) -> impl Iterator<Item = rowan::SyntaxElement<L>>
    where
        M: Fn(&rowan::SyntaxElement<L>) -> bool,
    {
        self.dive_with_ignore(match_predicate, |_| false)
    }

    /// Iterates over every element in the tree at the current root and yields
    /// the elements for which the given `match_predicate` evaluates to
    /// `true`.
    ///
    /// If the `ignore_predicate` evaluates to `true`, the subtree at the given
    /// node will not be traversed.
    fn dive_with_ignore<M, I>(
        &self,
        match_predicate: M,
        ignore_predicate: I,
    ) -> impl Iterator<Item = rowan::SyntaxElement<L>>
    where
        M: Fn(&rowan::SyntaxElement<L>) -> bool,
        I: Fn(&rowan::SyntaxNode<L>) -> bool;
}

impl<D, L> Divable<L> for &D
where
    D: Divable<L>,
    L: Language,
{
    fn dive_with_ignore<M, I>(
        &self,
        match_predicate: M,
        ignore_predicate: I,
    ) -> impl Iterator<Item = rowan::SyntaxElement<L>>
    where
        M: Fn(&rowan::SyntaxElement<L>) -> bool,
        I: Fn(&rowan::SyntaxNode<L>) -> bool,
    {
        D::dive_with_ignore(self, match_predicate, ignore_predicate)
    }
}

impl Divable<WorkflowDescriptionLanguage> for SyntaxNode {
    fn dive_with_ignore<M, I>(
        &self,
        match_predicate: M,
        ignore_predicate: I,
    ) -> impl Iterator<Item = SyntaxElement>
    where
        M: Fn(&SyntaxElement) -> bool,
        I: Fn(&SyntaxNode) -> bool,
    {
        DiveIterator::new(
            // NOTE: this is an inexpensive clone of a red node.
            self.clone(),
            ignore_predicate,
        )
        .filter(match_predicate)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use rowan::GreenNode;

    use crate::SyntaxKind;
    use crate::SyntaxNode;
    use crate::SyntaxTree;
    use crate::dive::Divable;

    fn get_syntax_node() -> SyntaxNode {
        static GREEN_NODE: OnceLock<GreenNode> = OnceLock::new();

        let green_node = GREEN_NODE
            .get_or_init(|| {
                let (tree, diagnostics) = SyntaxTree::parse(
                    r#"version 1.2

task hello {
    String a_private_declaration = false
}

workflow world {
    String another_private_declaration = true
}"#,
                );

                assert!(diagnostics.is_empty());
                tree.green().into()
            })
            .clone();

        SyntaxNode::new_root(green_node)
    }

    #[test]
    fn it_dives_correctly() {
        let tree = get_syntax_node();

        let mut idents = tree.dive(|element| element.kind() == SyntaxKind::Ident);

        assert_eq!(idents.next().unwrap().as_token().unwrap().text(), "hello");

        assert_eq!(
            idents.next().unwrap().as_token().unwrap().text(),
            "a_private_declaration"
        );

        assert_eq!(idents.next().unwrap().as_token().unwrap().text(), "world");

        assert_eq!(
            idents.next().unwrap().as_token().unwrap().text(),
            "another_private_declaration"
        );

        assert!(idents.next().is_none());
    }

    #[test]
    fn it_dives_with_ignores_correctly() {
        let tree = get_syntax_node();

        let mut ignored_idents = tree.dive_with_ignore(
            |element| element.kind() == SyntaxKind::Ident,
            |node| node.kind() == SyntaxKind::WorkflowDefinitionNode,
        );

        assert_eq!(
            ignored_idents.next().unwrap().as_token().unwrap().text(),
            "hello"
        );
        assert_eq!(
            ignored_idents.next().unwrap().as_token().unwrap().text(),
            "a_private_declaration"
        );

        // The idents contained in the workflow are not included in the results,
        // as we explicitly ignored any workflow definition nodes.
        assert!(ignored_idents.next().is_none());
    }
}
