//! A lint rule for spacing in comments.

use std::cmp::Ordering;

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::is_inline_comment;

/// Set indentation string
const INDENT: &str = "    ";

/// The identifier for the comment spacing rule.
const ID: &str = "CommentWhitespace";

/// Creates a diagnostic when an in-line comment is not preceded by two spaces.
fn inline_preceding_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("in-line comments should be preceded by two spaces")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("this comment must be preceded with two spaces")
}

/// Creates a diagnostic when the comment token is not followed by a single
/// space.
fn following_whitespace(span: Span) -> Diagnostic {
    Diagnostic::note("comment delimiter should be followed by a single space")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("follow this comment delimiter with a single space")
}

/// Creates a diagnostic when non-inline comment has insufficient indentation.
fn insufficient_indentation(span: Span, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::note("comment not sufficiently indented")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(format!(
            "this comment has {actual} levels of indentation. It should have {expected} levels of \
             indentation."
        ))
}

/// Creates a diagnostic when non-inline comment has excess indentation.
fn excess_indentation(span: Span, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::note("comment has too much indentation")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(format!(
            "this comment has {actual} levels of indentation. It should have {expected} levels of \
             indentation."
        ))
}

/// Detects improperly spaced comments.
#[derive(Default, Debug, Clone, Copy)]
pub struct CommentWhitespaceRule {
    /// Whether or not the visitor has exited the preamble of the document.
    exited_preamble: bool,
}

impl Rule for CommentWhitespaceRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that WDL comments have the proper spacing."
    }

    fn explanation(&self) -> &'static str {
        "Comments on the same line as code should have 2 spaces before the # and one space before \
         the comment text. Comments on their own line should match the indentation level around \
         them and have one space between the # and the comment text. Keep in mind that even \
         comments must be kept below the 90 character width limit."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }
}

impl Visitor for CommentWhitespaceRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn version_statement(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &wdl_ast::VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            self.exited_preamble = true;
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        if !self.exited_preamble {
            // Handled by `PreambleFormatting` rule
            return;
        }

        if is_inline_comment(comment) {
            // check preceding whitespace for two spaces
            if let Some(prior) = comment.syntax().prev_sibling_or_token() {
                if prior.kind() != SyntaxKind::Whitespace
                    || prior.as_token().expect("should be a token").text() != "  "
                {
                    // Report a diagnostic if there are not two spaces before the comment delimiter
                    state.exceptable_add(
                        inline_preceding_whitespace(comment.span()),
                        SyntaxElement::from(comment.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        } else {
            // Not an in-line comment, so check indentation level
            let ancestors = comment
                .syntax()
                .parent_ancestors()
                .filter(filter_parent_ancestors)
                .count();
            let expected_indentation = INDENT.repeat(ancestors);

            if let Some(leading_whitespace) = comment
                .syntax()
                .prev_sibling_or_token()
                .and_then(SyntaxElement::into_token)
            {
                let this_whitespace = leading_whitespace.text();
                let this_indentation = this_whitespace
                    .split('\n')
                    .last()
                    .expect("should have prior whitespace");
                if this_indentation != expected_indentation {
                    // Report a diagnostic if the comment is not indented properly
                    match this_indentation.len().cmp(&expected_indentation.len()) {
                        Ordering::Greater => state.exceptable_add(
                            excess_indentation(
                                comment.span(),
                                expected_indentation.len() / INDENT.len(),
                                this_indentation.len() / INDENT.len(),
                            ),
                            SyntaxElement::from(comment.syntax().clone()),
                            &self.exceptable_nodes(),
                        ),
                        Ordering::Less => state.exceptable_add(
                            insufficient_indentation(
                                comment.span(),
                                expected_indentation.len() / INDENT.len(),
                                this_indentation.len() / INDENT.len(),
                            ),
                            SyntaxElement::from(comment.syntax().clone()),
                            &self.exceptable_nodes(),
                        ),
                        Ordering::Equal => {}
                    }
                }
            } else {
                // If there is no prior whitespace, this comment must be at the
                // start of the file.
            }
        }

        // check the comment for one space following the comment delimiter
        let mut comment_chars = comment.as_str().chars().peekable();

        let mut n_delimiter = 0;
        while let Some('#') = comment_chars.peek() {
            n_delimiter += 1;
            comment_chars.next();
        }

        let preamble = n_delimiter == 2;

        if let Some('@') = comment_chars.peek() {
            n_delimiter += 1;
            comment_chars.next();
        }

        let n_whitespace = comment_chars.by_ref().take_while(|c| *c == ' ').count();

        if comment_chars.skip(n_whitespace).count() > 0
            && ((n_whitespace != 1 && !preamble) || (preamble && n_whitespace == 0))
        {
            state.exceptable_add(
                following_whitespace(Span::new(comment.span().start(), n_delimiter)),
                SyntaxElement::from(comment.syntax().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Filter parent nodes, removing any that don't contribute indentation.
fn filter_parent_ancestors(node: &SyntaxNode) -> bool {
    // If the prior token is Whitespace with a newline, then this ancestor
    // contributes to indentation.
    if let Some(prior) = node
        .prev_sibling_or_token()
        .and_then(SyntaxElement::into_token)
    {
        if prior.kind() == SyntaxKind::Whitespace && prior.text().contains('\n') {
            return true;
        }
    }
    // If a parenthesized expression has a prior sibling that contains a newline
    // before we get to a node, then this ancestor contributes to indentation.
    if node.kind() == SyntaxKind::ParenthesizedExprNode {
        let mut prior = node.prev_sibling_or_token();
        while let Some(p) = prior {
            if p.as_node().is_some() {
                break;
            }
            if p.kind() == SyntaxKind::Whitespace
                && p.as_token()
                    .expect("should be a token")
                    .text()
                    .contains('\n')
            {
                return true;
            }
            prior = p.prev_sibling_or_token();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use wdl_ast::AstToken;
    use wdl_ast::Comment;
    use wdl_ast::SyntaxKind;
    use wdl_ast::SyntaxTree;

    use crate::rules::comment_whitespace::is_inline_comment;

    #[test]
    fn it_detects_inline() {
        let (tree, _) = SyntaxTree::parse(
            r#"version 1.2

task foo {  # an in-line comment
    # not an in-line comment
}"#,
        );

        let mut comments = tree
            .root()
            .descendants_with_tokens()
            .filter(|t| t.kind() == SyntaxKind::Comment);

        let inline_comment = comments.next().expect("there should be a first comment");
        let inline_comment = Comment::cast(inline_comment.as_token().unwrap().clone()).unwrap();

        let is_inline = is_inline_comment(&inline_comment);

        assert!(is_inline);

        let non_inline_comment = comments.next().expect("there should be a second comment");
        let non_inline_comment =
            Comment::cast(non_inline_comment.as_token().unwrap().clone()).unwrap();

        let is_inline = is_inline_comment(&non_inline_comment);

        assert!(!is_inline);
    }

    #[test]
    fn filter_parents() {
        let (tree, _) = SyntaxTree::parse(
            r#"version 1.2

task foo {
    meta {
        # a comment
        description: "test string"
        choices: [
            # another comment
            "a",
            "b",
            "c",
        ],
        choice2:
            [
                # another comment
                "a",
                "b",
                "c",
            ]
    }

    input {
        # another comment
        Int a = 10 / (
            # another comment
            5
        )
    }

    command {
        # comment

    }
}"#,
        );

        let mut comments = tree
            .root()
            .descendants_with_tokens()
            .filter(|t| t.kind() == SyntaxKind::Comment);

        let comment = comments.next().expect("there should be a first comment");
        let comment = Comment::cast(comment.as_token().unwrap().clone()).unwrap();

        let ancestors = comment
            .syntax()
            .parent_ancestors()
            .filter(super::filter_parent_ancestors)
            .count();

        assert_eq!(ancestors, 2);

        let comment = comments.next().expect("there should be a second comment");
        let comment = Comment::cast(comment.as_token().unwrap().clone()).unwrap();

        let ancestors = comment
            .syntax()
            .parent_ancestors()
            .filter(super::filter_parent_ancestors)
            .count();

        assert_eq!(ancestors, 3);

        let comment = comments.next().expect("there should be a third comment");
        let comment = Comment::cast(comment.as_token().unwrap().clone()).unwrap();

        let ancestors = comment
            .syntax()
            .parent_ancestors()
            .filter(super::filter_parent_ancestors)
            .count();

        assert_eq!(ancestors, 4);

        let comment = comments.next().expect("there should be a fourth comment");
        let comment = Comment::cast(comment.as_token().unwrap().clone()).unwrap();

        let ancestors = comment
            .syntax()
            .parent_ancestors()
            .filter(super::filter_parent_ancestors)
            .count();

        assert_eq!(ancestors, 2);

        let comment = comments.next().expect("there should be a fifth comment");
        let comment = Comment::cast(comment.as_token().unwrap().clone()).unwrap();

        let ancestors = comment
            .syntax()
            .parent_ancestors()
            .filter(super::filter_parent_ancestors)
            .count();

        assert_eq!(ancestors, 3);
    }
}
