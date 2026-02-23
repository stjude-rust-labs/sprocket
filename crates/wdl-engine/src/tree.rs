//! Implementation of syntax tree elements that are `Send + Sync`.
//!
//! This is used by the engine instead of the corresponding types from `rowan`
//! because that implementation is inherently not `Send`.
//!
//! As evaluation is required to be asynchronous, AST elements must be `Send`.

use std::fmt;
use std::hash::Hash;
use std::sync::Arc;

use rowan::GreenNode;
use rowan::GreenNodeData;
use rowan::GreenToken;
use rowan::GreenTokenData;
use rowan::Language;
use rowan::NodeOrToken;
use rowan::WalkEvent;
use wdl_analysis::Exceptable;
use wdl_ast::NewRoot;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;
use wdl_ast::WorkflowDescriptionLanguage;

/// Internal data for an element in the tree.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ElementData {
    /// The parent data.
    ///
    /// This is `None` for the root node.
    parent: Option<Arc<ElementData>>,
    /// The associated green element.
    green: NodeOrToken<GreenNode, GreenToken>,
    /// The index of this element in the parent's list of children.
    index: usize,
    /// The offset to the start of this element.
    offset: usize,
}

impl ElementData {
    /// Constructs a new element data.
    fn new(
        parent: Arc<ElementData>,
        green: NodeOrToken<GreenNode, GreenToken>,
        index: usize,
        offset: usize,
    ) -> Self {
        Self {
            parent: Some(parent),
            green,
            index,
            offset,
        }
    }

    /// Constructs element data for a new root node.
    fn new_root(green: GreenNode) -> Self {
        Self {
            parent: None,
            green: green.into(),
            index: 0,
            offset: 0,
        }
    }
}

/// Represents an element in a syntax tree.
pub type SyntaxElement = NodeOrToken<SyntaxNode, SyntaxToken>;

/// Represents a syntax node that is `Send + Sync`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SyntaxNode(Arc<ElementData>);

impl Exceptable for SyntaxNode {}

impl SyntaxNode {
    /// Constructs a new child node for this node.
    fn new_child_node(&self, green: GreenNode, index: usize, offset: usize) -> Self {
        Self(Arc::new(ElementData::new(
            self.0.clone(),
            green.into(),
            index,
            offset,
        )))
    }

    /// Constructs a new child token for this node.
    fn new_child_token(&self, green: GreenToken, index: usize, offset: usize) -> SyntaxToken {
        SyntaxToken(Arc::new(ElementData::new(
            self.0.clone(),
            green.into(),
            index,
            offset,
        )))
    }

    /// Gets the first child node.
    ///
    /// Returns `None` if there are no children or if all the children are
    /// tokens.
    pub fn first_child(&self) -> Option<SyntaxNode> {
        self.children().next()
    }

    /// Gets the first child node or token.
    ///
    /// Returns `None` if there are no children.
    pub fn first_child_or_token(&self) -> Option<SyntaxElement> {
        self.children_with_tokens().next()
    }

    /// Gets the next sibling node.
    ///
    /// Returns `None` if there is no sibling node.
    pub fn next_sibling(&self) -> Option<SyntaxNode> {
        // This should also be a constant-time access rather than having to iterate.
        // We need the offset relative to the start of the parent from the green node to
        // do that; currently that information is private in `rowan`.

        let parent = self.parent()?;
        let mut children = parent.children();

        while let Some(child) = children.next() {
            if child.eq(self) {
                return children.next();
            }
        }

        None
    }

    /// Gets the next sibling node or token.
    ///
    /// Returns `None` if there is no sibling node or token.
    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        // This should also be a constant-time access rather than having to iterate.
        // We need the offset relative to the start of the parent from the green node to
        // do that; currently that information is private in `rowan`.

        let parent = self.parent()?;
        let mut children = parent.children_with_tokens();

        while let Some(child) = children.next() {
            if let Some(node) = child.as_node()
                && node.eq(self)
            {
                return children.next();
            }
        }

        None
    }

    /// Gets a preorder traversal iterator starting at this node.
    #[inline]
    pub fn preorder(&self) -> Preorder {
        Preorder::new(self.clone())
    }

    /// Gets a preorder-with-tokens traversal iterator starting at this node.
    #[inline]
    pub fn preorder_with_tokens(&self) -> PreorderWithTokens {
        PreorderWithTokens::new(self.clone())
    }

    /// Gets an iterator over the descendants with tokens for this node.
    fn descendants_with_tokens(&self) -> impl Iterator<Item = SyntaxElement> {
        self.preorder_with_tokens().filter_map(|event| match event {
            WalkEvent::Enter(it) => Some(it),
            WalkEvent::Leave(_) => None,
        })
    }
}

impl TreeNode for SyntaxNode {
    type Token = SyntaxToken;

    fn parent(&self) -> Option<Self> {
        self.0.parent.clone().map(Self)
    }

    fn kind(&self) -> SyntaxKind {
        WorkflowDescriptionLanguage::kind_from_raw(self.0.green.kind())
    }

    fn text(&self) -> impl fmt::Display {
        SyntaxText(self.clone())
    }

    fn span(&self) -> Span {
        Span::new(self.0.offset, usize::from(self.0.green.text_len()))
    }

    fn children(&self) -> impl Iterator<Item = Self> {
        let mut offset = self.0.offset;
        self.0
            .green
            .as_node()
            .expect("should be node")
            .children()
            .enumerate()
            .filter_map(move |(index, child)| {
                let start = offset;
                offset += usize::from(child.text_len());
                Some(self.new_child_node(child.into_node()?.to_owned(), index, start))
            })
    }

    fn children_with_tokens(&self) -> impl Iterator<Item = SyntaxElement> {
        let mut offset = self.0.offset;
        self.0
            .green
            .as_node()
            .expect("should be node")
            .children()
            .enumerate()
            .map(move |(index, child)| {
                let start = offset;
                offset += usize::from(child.text_len());
                match child {
                    NodeOrToken::Node(n) => self.new_child_node(n.to_owned(), index, start).into(),
                    NodeOrToken::Token(t) => {
                        self.new_child_token(t.to_owned(), index, start).into()
                    }
                }
            })
    }

    fn last_token(&self) -> Option<Self::Token> {
        // Unfortunately `rowan` does not expose the relative offset of each green
        // child. If it did, we could easily just look at the last child here
        // instead of iterating to find the last child's start.
        let mut last: Option<(usize, NodeOrToken<&GreenNodeData, &GreenTokenData>)> = None;
        let mut start = self.0.offset;

        for (index, child) in self
            .0
            .green
            .as_node()
            .expect("should be node")
            .children()
            .enumerate()
        {
            if let Some((_, prev)) = last {
                start += usize::from(prev.text_len());
            }

            last = Some((index, child));
        }

        match last? {
            (index, NodeOrToken::Node(n)) => {
                self.new_child_node(n.to_owned(), index, start).last_token()
            }
            (index, NodeOrToken::Token(t)) => {
                Some(self.new_child_token(t.to_owned(), index, start))
            }
        }
    }

    fn descendants(&self) -> impl Iterator<Item = Self> {
        self.preorder().filter_map(|event| match event {
            WalkEvent::Enter(node) => Some(node),
            WalkEvent::Leave(_) => None,
        })
    }

    fn ancestors(&self) -> impl Iterator<Item = Self> {
        std::iter::successors(Some(self.clone()), SyntaxNode::parent)
    }
}

impl fmt::Debug for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let mut level = 0;
            for event in self.preorder_with_tokens() {
                match event {
                    WalkEvent::Enter(element) => {
                        for _ in 0..level {
                            write!(f, "  ")?;
                        }
                        match element {
                            NodeOrToken::Node(node) => writeln!(f, "{node:?}")?,
                            NodeOrToken::Token(token) => writeln!(f, "{token:?}")?,
                        }
                        level += 1;
                    }
                    WalkEvent::Leave(_) => level -= 1,
                }
            }
            assert_eq!(level, 0);
            Ok(())
        } else {
            write!(f, "{:?}@{}", self.kind(), self.span())
        }
    }
}

impl NewRoot<wdl_ast::SyntaxNode> for SyntaxNode {
    fn new_root(root: wdl_ast::SyntaxNode) -> Self {
        Self(Arc::new(ElementData::new_root(root.green().into())))
    }
}

impl From<SyntaxNode> for SyntaxElement {
    fn from(value: SyntaxNode) -> Self {
        Self::Node(value)
    }
}

/// Represents a syntax token that is `Send + Sync`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SyntaxToken(Arc<ElementData>);

impl SyntaxToken {
    /// Gets the next sibling node or token.
    ///
    /// Returns `None` if there is no next sibling node or token.
    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        // This should also be a constant-time access rather than having to iterate.
        // We need the offset relative to the start of the parent from the green node to
        // do that.

        let parent = self.parent();
        let mut children = parent.children_with_tokens();

        while let Some(child) = children.next() {
            if let Some(token) = child.as_token()
                && token.eq(self)
            {
                return children.next();
            }
        }

        None
    }
}

impl TreeToken for SyntaxToken {
    type Node = SyntaxNode;

    fn parent(&self) -> Self::Node {
        self.0
            .parent
            .clone()
            .map(SyntaxNode)
            .expect("tokens should always have parents")
    }

    fn kind(&self) -> SyntaxKind {
        WorkflowDescriptionLanguage::kind_from_raw(self.0.green.kind())
    }

    fn text(&self) -> &str {
        self.0.green.as_token().expect("should be token").text()
    }

    fn span(&self) -> Span {
        Span::new(self.0.offset, usize::from(self.0.green.text_len()))
    }
}

impl fmt::Debug for SyntaxToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}@{:?}", self.kind(), self.span())?;
        if self.text().len() < 25 {
            return write!(f, " {:?}", self.text());
        }
        let text = self.text();
        for idx in 21..25 {
            if text.is_char_boundary(idx) {
                let text = format!("{} ...", &text[..idx]);
                return write!(f, " {text:?}");
            }
        }
        unreachable!()
    }
}

impl From<SyntaxToken> for SyntaxElement {
    fn from(value: SyntaxToken) -> Self {
        Self::Token(value)
    }
}

/// Constant that asserts types are `Send + Sync`; if not, it fails to compile.
const _: () = {
    /// Helper that will fail to compile if T is not `Send + Sync`.
    const fn _assert<T: Send + Sync>() {}
    _assert::<SyntaxNode>();
    _assert::<SyntaxToken>();
};

/// Implements a preorder iterator.
#[derive(Debug)]
pub struct Preorder {
    /// The starting node.
    start: SyntaxNode,
    /// The next event for the iterator.
    next: Option<WalkEvent<SyntaxNode>>,
}

impl Preorder {
    /// Constructs a new preorder iterator for the given start node.
    fn new(start: SyntaxNode) -> Preorder {
        let next = Some(WalkEvent::Enter(start.clone()));
        Preorder { start, next }
    }
}

impl Iterator for Preorder {
    type Item = WalkEvent<SyntaxNode>;

    fn next(&mut self) -> Option<WalkEvent<SyntaxNode>> {
        let next = self.next.take();
        self.next = next.as_ref().and_then(|next| {
            Some(match next {
                WalkEvent::Enter(node) => match node.first_child() {
                    Some(child) => WalkEvent::Enter(child),
                    None => WalkEvent::Leave(node.clone()),
                },
                WalkEvent::Leave(node) => {
                    if node == &self.start {
                        return None;
                    }
                    match node.next_sibling() {
                        Some(sibling) => WalkEvent::Enter(sibling),
                        None => WalkEvent::Leave(node.parent()?),
                    }
                }
            })
        });
        next
    }
}

/// Implements a preorder-with-tokens iterator.
pub struct PreorderWithTokens {
    /// The starting element.
    start: SyntaxElement,
    /// The next event for the iterator.
    next: Option<WalkEvent<SyntaxElement>>,
}

impl PreorderWithTokens {
    /// Constructs a new preorder-with-tokens iterator for the given start node.
    fn new(start: SyntaxNode) -> PreorderWithTokens {
        let next = Some(WalkEvent::Enter(start.clone().into()));
        PreorderWithTokens {
            start: start.into(),
            next,
        }
    }
}

impl Iterator for PreorderWithTokens {
    type Item = WalkEvent<SyntaxElement>;

    fn next(&mut self) -> Option<WalkEvent<SyntaxElement>> {
        let next = self.next.take();
        self.next = next.as_ref().and_then(|next| {
            Some(match next {
                WalkEvent::Enter(el) => match el {
                    NodeOrToken::Node(node) => match node.first_child_or_token() {
                        Some(child) => WalkEvent::Enter(child),
                        None => WalkEvent::Leave(node.clone().into()),
                    },
                    NodeOrToken::Token(token) => WalkEvent::Leave(token.clone().into()),
                },
                WalkEvent::Leave(el) if el == &self.start => return None,
                WalkEvent::Leave(el) => {
                    let sibling = match el {
                        NodeOrToken::Node(n) => n.next_sibling_or_token(),
                        NodeOrToken::Token(t) => t.next_sibling_or_token(),
                    };

                    match sibling {
                        Some(sibling) => WalkEvent::Enter(sibling),
                        None => match el {
                            NodeOrToken::Node(n) => WalkEvent::Leave(n.parent()?.into()),
                            NodeOrToken::Token(t) => WalkEvent::Leave(t.parent().into()),
                        },
                    }
                }
            })
        });
        next
    }
}

/// Represents the text of a syntax node.
///
/// The text of a syntax node is the cumulation of its descendant token texts.
struct SyntaxText(SyntaxNode);

impl SyntaxText {
    /// Calls a fallible callback for each chunk of text.
    pub fn try_for_each_chunk<F: FnMut(&str) -> Result<(), E>, E>(
        &self,
        mut f: F,
    ) -> Result<(), E> {
        self.try_fold_chunks((), move |(), chunk| f(chunk))
    }

    /// Attempts to fold each chunk of text into the given accumulator
    pub fn try_fold_chunks<T, F, E>(&self, init: T, mut f: F) -> Result<T, E>
    where
        F: FnMut(T, &str) -> Result<T, E>,
    {
        self.tokens_with_spans()
            .try_fold(init, move |acc, (token, span)| {
                f(acc, &token.text()[span.start()..span.end()])
            })
    }

    /// Gets an iterator over all descendant tokens with their spans.
    fn tokens_with_spans(&self) -> impl Iterator<Item = (SyntaxToken, Span)> {
        let span = self.0.span();
        self.0.descendants_with_tokens().filter_map(move |element| {
            let token = element.into_token()?;
            let token_span = token.span();
            let intersection = span.intersect(token_span)?;
            Some((
                token,
                Span::new(
                    intersection.start() - token_span.start(),
                    intersection.len() - (intersection.end() - token_span.end()),
                ),
            ))
        })
    }
}

impl fmt::Display for SyntaxText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.try_for_each_chunk(|chunk| fmt::Display::fmt(chunk, f))
    }
}
