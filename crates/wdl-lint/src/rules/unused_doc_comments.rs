//! A lint rule for misplaced doc comments that will not generate documentation.

use rowan::NodeOrToken;
use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The ID for the UnusedDocComments lint.
const ID: &str = "UnusedDocComments";

/// Creates a diagnostic for a misplaced doc comment.
fn unused_doc_comment_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::note("Unused doc comment")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("Documentation will not be generated for this item")
        .with_fix("If you're intending to use a regular comment here, replace the `##` with `#`")
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
fn valid_target_for_doc_comment(doc_comment_target: &NodeOrToken<SyntaxNode, SyntaxToken>) -> bool {
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

/// While searching for the sibling that a doc comment is targeting, we want to
/// also maintain the span of the doc comment itself. We use this struct to
/// return the two items to the caller.
struct DocCommentSearchResult {
    /// The NodeOrToken we think that the doc comment is attempting to document.
    target: NodeOrToken<SyntaxNode, SyntaxToken>,
    /// The span of the doc comment.
    span: Span,
}

impl UnusedDocCommentsRule {
    /// Find the first [`NodeOrToken`] in the comment's siblings that is not
    /// a ...
    /// - doc comment
    /// - comment directive
    /// - single newline
    ///
    /// to determine what this doc comment is targeting.
    ///
    /// Increment the skip_count while walking comments to avoid linting on a
    /// single doc comment block twice, and build up the span of the doc
    /// comment throughout the search.
    fn search_siblings_for_doc_comment_target(
        &mut self,
        comment: &Comment,
    ) -> Option<DocCommentSearchResult> {
        let mut span = comment.span();
        let mut next = comment.inner().next_sibling_or_token();
        while let Some(sibling) = next {
            next = sibling.next_sibling_or_token();

            if sibling.kind() == SyntaxKind::Whitespace {
                continue;
            }

            // If we are still looking at a doc comment, then continue, increase the
            // skip_count and update the span.
            if let Some(continued_comment) =
                sibling.as_token().and_then(|t| Comment::cast(t.clone()))
                && (continued_comment.is_doc_comment() || continued_comment.is_directive())
            {
                self.skip_count += 1;

                // Don't include a trailing directive in the doc comment's span, but ignore a
                // directive in the middle of a doc comment block for the purposes of this lint.
                if !continued_comment.is_directive() {
                    span = Span::new(
                        comment.span().start(),
                        continued_comment.span().end() - comment.span().start(),
                    );
                }

                continue;
            }

            return Some(DocCommentSearchResult {
                target: sibling,
                span,
            });
        }
        None
    }
}

impl Rule for UnusedDocCommentsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Reports doc comments that are attached to WDL sections that don't support them."
    }

    fn explanation(&self) -> &'static str {
        "Some Workflow Definition Language items and sections do not support doc comments (`##`). \
         This lint reports if a doc comment is attached to some section or item that isn't \
         supported.

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
        None
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

        // If the visited comment isn't a doc comment, or we've already seen it, then
        // there's no need to process it!
        //
        // Also, ignore comment directives.
        if !comment.is_doc_comment() || comment.is_directive() {
            return;
        }

        let target = self.search_siblings_for_doc_comment_target(comment);
        if let Some(result) = target
            && !valid_target_for_doc_comment(&result.target)
        {
            diagnostics.exceptable_add(
                unused_doc_comment_diagnostic(result.span),
                SyntaxElement::from(comment.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
