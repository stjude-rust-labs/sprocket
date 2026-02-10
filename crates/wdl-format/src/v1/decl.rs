//! Formatting functions for declarations.

use wdl_ast::SyntaxKind;

use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats a [`PrimitiveType`](wdl_ast::v1::PrimitiveType).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_primitive_type(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("primitive type children") {
        (&child).write(stream, None);
    }
}

/// Formats an [`ArrayType`](wdl_ast::v1::ArrayType).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_array_type(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("array type children") {
        (&child).write(stream, None);
    }
}

/// Formats a [`MapType`](wdl_ast::v1::MapType).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_map_type(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("map type children") {
        (&child).write(stream, None);
        if child.element().kind() == SyntaxKind::Comma {
            stream.end_word();
        }
    }
}

/// Formats an [`ObjectType`](wdl_ast::v1::ObjectType).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_object_type(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("object type children") {
        (&child).write(stream, None);
    }
}

/// Formats a [`PairType`](wdl_ast::v1::PairType).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_pair_type(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("pair type children") {
        (&child).write(stream, None);
        if child.element().kind() == SyntaxKind::Comma {
            stream.end_word();
        }
    }
}

/// Formats a [`TypeRef`](wdl_ast::v1::TypeRef).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_type_ref(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("type ref children") {
        (&child).write(stream, None);
    }
}

/// Formats an [`UnboundDecl`](wdl_ast::v1::UnboundDecl).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_unbound_decl(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("unbound decl children") {
        (&child).write(stream, None);
        stream.end_word();
    }
    stream.end_line();
}

/// Formats a [`BoundDecl`](wdl_ast::v1::BoundDecl).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_bound_decl(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("bound decl children") {
        (&child).write(stream, None);
        stream.end_word();
    }
    stream.end_line();
}
