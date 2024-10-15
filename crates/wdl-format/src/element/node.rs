//! A wrapper for formatting [`AstNode`]s.

use wdl_ast::Element;
use wdl_ast::Node;

use crate::element::FormatElement;
use crate::element::collate;

/// An extension trait for formatting [`Node`]s.
pub trait AstNodeFormatExt {
    /// Consumes `self` and returns the [`Node`] as a [`FormatElement`].
    fn into_format_element(self) -> FormatElement;
}

impl AstNodeFormatExt for Node {
    fn into_format_element(self) -> FormatElement
    where
        Self: Sized,
    {
        let children = collate(&self);
        FormatElement::new(Element::Node(self), children)
    }
}
