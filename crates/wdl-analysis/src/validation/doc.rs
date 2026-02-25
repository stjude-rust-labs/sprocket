//! validation of doc comments in an AST
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use crate::Diagnostics;
use crate::Visitor;

/// Creates a "tab in doc comment" diagnostic
fn tab_in_doc_comment(span: Span, count: usize) -> Diagnostic {
     let tabs= if count == 1 {
          "a tab character".to_string()
     } else {          format!("{count} tab characters")
     };
     Diagnostic::warning(format!("doc comments cannot contain {tabs}"))
        .with_label("remove this tab character", span) 
}

/// A visitor that checks doc comments for tab characters
/// 
#[derive(Default, Debug)]
pub struct DocCommentVisitor;
impl Visitor for DocCommentVisitor {
    fn reset(&mut self) {
        *self = Default::default();
    }
     fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if !comment.is_doc_comment() {
            return;
        }
        let text = comment.text();
        let start = comment.span().start();
        // Walk the text, grouping consecutive tab characters into single diagnostics.
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\t' {
                let run_start = i;
                while i < bytes.len() && bytes[i] == b'\t' {
                    i += 1;
                }
                let count = i - run_start;
                diagnostics.add(tab_in_doc_comment(
                    Span::new(start + run_start, count),
                    count,
                ));
            } else {
                i += 1;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wdl_ast::Document;

    #[test]
    fn test_no_tabs() {
        // A normal doc comment with no tabs should produce zero diagnostics
        let (document, diagnostics) = Document::parse("version 1.1\n## hello world\ntask foo {command<<<>>>}");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_single_tab() {
        // A doc comment with one tab should produce one diagnostic
        let (document, diagnostics) = Document::parse("version 1.1\n##\thello\ntask foo {command<<<>>>}");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_consecutive_tabs() {
        // Two consecutive tabs should produce ONE diagnostic, not two
        let (document, diagnostics) = Document::parse("version 1.1\n##\t\thello\ntask foo {command<<<>>>}");
        assert_eq!(diagnostics.len(), 1); // grouped into one!
    }
}