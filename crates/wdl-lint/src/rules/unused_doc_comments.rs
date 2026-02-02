use std::collections::HashSet;

use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

const DOC_COMMENT_PREFIX: &str = "## ";
const CONTINUED_DOC_COMMENT_PREFIX: &str = "##";

const ID: &str = "UnusedDocComments";

/// Creates a diagnostic for a comment outside the preamble.
fn unused_doc_comment_diagnostic(span: Span, syntax_kind: SyntaxKind) -> Diagnostic {
    Diagnostic::note(format!(
        "wdl-doc does not generate documentation for {}",
        syntax_kind.describe()
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_help("Doc comments must be attached to syntax nodes that support doc comments")
    .with_fix("If you're intending to use a regular comment here, replace the `##` with `#`")
}

/// Detects whether a doc comment has been placed atop a Node that we do not
/// generate documentation for.
#[derive(Default, Debug, Clone)]
pub struct UnusedDocCommentsRule {
    // TODO: This seems wasteful? Is there a better unique identifier to use
    // associated with a comment token in the AST so I can ensure that I don't
    // revisit?
    /// Because each individual doc comment is visited as a single token,
    /// we need to track multiline comments so we don't visit them again.
    comments_seen: HashSet<Comment>,
    version_statement_processed: bool,
}

impl UnusedDocCommentsRule {
    /// Valid syntax kinds for doc comments. Used to decide if a doc comment
    /// is in a valid position.
    ///
    /// Note that UnboundDeclNode is not included. We handle struct
    /// items inline since the UnboundDeclNode used to represent
    /// them isn't specific enough for us to know if the context is
    /// within a struct or not.
    const VALID_SYNTAX_KINDS_FOR_DOC_COMMENTS: &[SyntaxKind] = &[
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::StructDefinitionNode,
        SyntaxKind::InputSectionNode,
        SyntaxKind::OutputSectionNode,
        SyntaxKind::EnumDefinitionNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::EnumVariantNode,
    ];

    fn valid_node_for_doc_comment(kind: SyntaxKind) -> bool {
        Self::VALID_SYNTAX_KINDS_FOR_DOC_COMMENTS.contains(&kind)
    }
}

impl Rule for UnusedDocCommentsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Reports doc comments that are attached to syntax elements that don't support them."
    }

    fn explanation(&self) -> &'static str {
        "Some syntax nodes are not supported by doc comments (`##`). This lint reports if a doc \
         comment is attached to syntax elements that aren't supported.

        Doc comments (`##`) are supported on:

        - Workflow Definitions
        - Task Definitions
        - Struct Definitions
        - Fields in Struct Definitions
        - Input Sections
        - Output Sections
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
        self.comments_seen.clear();
        self.version_statement_processed = false;
    }

    fn version_statement(
        &mut self,
        _diagnostics: &mut Diagnostics,
        _reason: wdl_analysis::VisitReason,
        _stmt: &wdl_ast::VersionStatement,
    ) {
        self.version_statement_processed = true;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        // Don't lint on the preamble
        if !self.version_statement_processed {
            return;
        }

        // If the visited comment isn't a doc comment, or we've already seen it, then
        // there's no need to process it!
        if !comment.text().starts_with(DOC_COMMENT_PREFIX) || self.comments_seen.contains(&comment)
        {
            return;
        }

        let mut span = comment.span();
        let mut current = comment.inner().next_sibling_or_token();

        while let Some(sibling) = current {
            current = sibling.next_sibling_or_token();
            // Given PreambleCommentPlacementRule currently handles "floating comments", we
            // check for them here as well and don't lint on them (as they are
            // assumed to be misplaced preambles).
            if sibling.kind() == SyntaxKind::Whitespace {
                if let Some(token) = sibling.as_token() {
                    let comment_is_floating =
                        token.text().chars().filter(|c| *c == '\n').count() > 1;
                    if comment_is_floating {
                        return;
                    }
                };
                continue;
            }

            // If we are still looking at a doc comment, then continue, but add the
            // sibling to the list of comments_seen so we skip it when we come to visit the
            // comment itself.
            if let Some(comment) = sibling
                .as_token()
                .map(|t| Comment::cast(t.clone()))
                .flatten()
                && comment.text().starts_with(CONTINUED_DOC_COMMENT_PREFIX)
            {
                self.comments_seen.insert(comment.clone());
                continue;
            }

            // TODO: This feels pretty jank. Is there a better way to target a field of a
            // struct?
            let struct_definition_element = sibling.kind() == SyntaxKind::UnboundDeclNode
                && sibling
                    .parent()
                    .map(|t| t.kind() == SyntaxKind::StructDefinitionNode)
                    .unwrap_or_default();

            if !Self::valid_node_for_doc_comment(sibling.kind()) && !struct_definition_element {
                let sibling_span = match &sibling {
                    rowan::NodeOrToken::Node(node) => node.span(),
                    rowan::NodeOrToken::Token(token) => token.span(),
                };

                span = Span::new(span.start(), sibling_span.start() - span.start());

                diagnostics.exceptable_add(
                    unused_doc_comment_diagnostic(span, sibling.kind()),
                    SyntaxElement::from(comment.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
            break;
        }
    }
}
