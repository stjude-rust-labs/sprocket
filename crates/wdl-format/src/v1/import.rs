//! Formatting for imports.

use crate::PreToken;
use crate::TokenStream;
use crate::Writable as _;
use crate::element::FormatElement;

/// Formats an [`ImportAlias`](wdl_ast::v1::ImportAlias).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_import_alias(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("import alias children") {
        (&child).write(stream, None);
        stream.end_word();
    }
}

/// Formats an [`ImportStatement`](wdl_ast::v1::ImportStatement).
///
/// # Panics
///
/// This will panic if the element does not have the expected children.
pub fn format_import_statement(element: &FormatElement, stream: &mut TokenStream<PreToken>) {
    for child in element.children().expect("import statement children") {
        (&child).write(stream, None);
        stream.end_word();
    }

    stream.end_line();
}
