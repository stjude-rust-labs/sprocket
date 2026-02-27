//! A lint rule for misplaced doc comments that will not generate documentation.

use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::TreeToken;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The ID for the UnusedDocComments lint.
const ID: &str = "UnusedDocComments";

/// Creates a diagnostic for a misplaced doc comment.
fn unused_doc_comment_diagnostic(comment_span: Span, target_span: Option<Span>) -> Diagnostic {
    let diagnostic = Diagnostic::note("unused doc comment")
        .with_rule(ID)
        .with_highlight(comment_span)
        .with_fix(
            "if this is a non-doc comment, replace the leading `##` with `#`; otherwise move this \
             comment so it is bound to its associated element",
        );

    if let Some(target_span) = target_span {
        diagnostic.with_label(
            "documentation will not be generated for this item",
            target_span,
        )
    } else {
        diagnostic
    }
}

/// Detects whether a doc comment has been placed atop a Node that we do not
/// generate documentation for.
#[derive(Default, Debug, Clone)]
pub struct UnusedDocCommentsRule {
    /// The number of comment tokens to skip.
    ///
    /// This is used when consolidating multiple comments into a single
    /// diagnostic.
    skip_count: u32,
}

/// Valid syntax kinds for doc comments.
///
/// Note that [`BoundDeclNode`] is included, although not all bound declarations
/// may have doc comments. It is the consumer's responsibility to verify whether
/// a bound declaration is in a valid context for doc comments (i.e. there is a
/// `InputSectionNode` or `OutputSectionNode` parent). [`UnboundDeclNode`] is
/// always valid for doc comments, although they may appear either as members of
/// a `struct` or as required inputs for a task or workflow.
const VALID_SYNTAX_KINDS_FOR_DOC_COMMENTS: &[SyntaxKind] = &[
    SyntaxKind::VersionStatementNode,
    SyntaxKind::WorkflowDefinitionNode,
    SyntaxKind::StructDefinitionNode,
    SyntaxKind::EnumDefinitionNode,
    SyntaxKind::TaskDefinitionNode,
    SyntaxKind::EnumVariantNode,
    SyntaxKind::UnboundDeclNode,
    SyntaxKind::BoundDeclNode,
];

/// Determine whether the SyntaxNodeOrToken is a valid target for a doc comment.
fn valid_target_for_doc_comment(doc_comment_target: &SyntaxElement) -> bool {
    let kind = doc_comment_target.kind();

    // A BoundDeclNode can only have doc comments if it is a part of an InputSection
    // or OutputSection.
    if kind == SyntaxKind::BoundDeclNode {
        let Some(parent) = doc_comment_target.parent() else {
            return false;
        };

        return parent.kind() == SyntaxKind::InputSectionNode
            || parent.kind() == SyntaxKind::OutputSectionNode;
    }

    VALID_SYNTAX_KINDS_FOR_DOC_COMMENTS.contains(&kind)
}

/// Find the first non-trivia [`SyntaxElement`] in the comment's siblings
/// to determine what this doc comment is targeting.
fn search_siblings_for_doc_comment_target(comment: &Comment) -> Option<SyntaxElement> {
    let mut next = comment.inner().next_sibling_or_token();
    while let Some(sibling) = next {
        next = sibling.next_sibling_or_token();
        if !sibling.kind().is_trivia() {
            return Some(sibling);
        }
    }
    None
}

/// An inline doc comment is invalid, but the author likely is intending to
/// comment the closest non-trivia node behind the comment.
///
/// This function attempts to find the SyntaxElement the author is likely
/// intending to document.
fn find_inline_doc_comment_target(comment: &Comment) -> Option<SyntaxElement> {
    let mut prev = comment.inner().prev_sibling_or_token();
    while let Some(sibling) = prev {
        if sibling.kind().is_trivia() {
            prev = sibling.prev_sibling_or_token();
            continue;
        } else {
            return Some(sibling);
        }
    }
    None
}

/// Get the [`Span`] of the first token of the provided [`SyntaxElement`]
fn get_span_of_first_token_for_syntax_element(element: &SyntaxElement) -> Span {
    if let Some(token) = element.as_token() {
        token.span()
    } else if let Some(node) = element.as_node() {
        node.first_token().unwrap().span()
    } else {
        unreachable!();
    }
}

impl UnusedDocCommentsRule {
    /// Produce an unused doc comment diagnostic for the doc comment block
    /// starting at `comment`. Update `skip_count` along the way.
    fn lint_next_doc_comment_block(
        &mut self,
        diagnostics: &mut Diagnostics,
        comment: &Comment,
        target_span: Option<Span>,
    ) {
        let mut next = comment.inner().next_sibling_or_token();
        let mut span_end = comment.span().end();
        while let Some(sibling) = next {
            next = sibling.next_sibling_or_token();

            if sibling.kind() == SyntaxKind::Whitespace {
                continue;
            }

            if let Some(continued_comment) =
                sibling.as_token().and_then(|t| Comment::cast(t.clone()))
                && continued_comment.is_doc_comment()
            {
                self.skip_count += 1;
                span_end = continued_comment.span().end();
                continue;
            } else {
                diagnostics.add(unused_doc_comment_diagnostic(
                    Span::new(comment.span().start(), span_end - comment.span().start()),
                    target_span,
                ));
                return;
            }
        }

        // If the doc comment block extends to the end of the token stream,
        // we won't add a diagnostic above. Add one here.
        diagnostics.add(unused_doc_comment_diagnostic(
            Span::new(comment.span().start(), span_end - comment.span().start()),
            target_span,
        ));
    }
}

impl Rule for UnusedDocCommentsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Reports doc comments that are attached to WDL items that don't support them."
    }

    fn explanation(&self) -> &'static str {
        "Some Workflow Definition Language items do not support doc comments (`##`). This lint \
         reports if a doc comment is attached to an item that isn't supported.

        Doc comments are supported on:

        - Workflow Definitions
        - Task Definitions
        - Struct Definitions
        - Fields in Struct Definitions
        - Fields in Input Sections
        - Fields in Output Sections
        - Enum Definitions
        - Enum Variants"
    }

    fn tags(&self) -> crate::TagSet {
        TagSet::new(&[Tag::Documentation])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for UnusedDocCommentsRule {
    fn reset(&mut self) {
        self.skip_count = 0;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if self.skip_count > 0 {
            self.skip_count -= 1;
            return;
        }

        // If the visited comment isn't a doc comment, then
        // there's no need to process it!
        if !comment.is_doc_comment() || comment.is_directive() {
            return;
        }

        if comment.is_inline_comment()
            && let Some(target) = find_inline_doc_comment_target(comment)
        {
            diagnostics.add(unused_doc_comment_diagnostic(
                comment.span(),
                Some(get_span_of_first_token_for_syntax_element(&target)),
            ));
            return;
        }

        let target = search_siblings_for_doc_comment_target(comment);
        if target
            .as_ref()
            .is_none_or(|t| !valid_target_for_doc_comment(t))
        {
            self.lint_next_doc_comment_block(
                diagnostics,
                comment,
                target
                    .as_ref()
                    .map(get_span_of_first_token_for_syntax_element),
            );
        }
    }
}
