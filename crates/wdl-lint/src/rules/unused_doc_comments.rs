//! A lint rule for misplaced doc comments that will not generate documentation.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::CommentKind;
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
fn unused_doc_comment_diagnostic(
    comment_span: Span,
    target_span: Option<Span>,
    valid_target: bool,
    floating: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::note("unused doc comment")
        .with_rule(ID)
        .with_highlight(comment_span);

    if valid_target {
        assert!(floating);
        diagnostic =
            diagnostic.with_help("doc comments must be attached to the item to be recognized");
    } else {
        diagnostic = diagnostic.with_fix(
            "if this is a non-doc comment, replace the leading `##` with `#`; otherwise move this \
             comment so it documents the intended item",
        );
    }

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
    SyntaxKind::EnumChoiceNode,
    SyntaxKind::UnboundDeclNode,
    SyntaxKind::BoundDeclNode,
];

/// [`SyntaxKind`]s that are allowed to have floating doc comments.
const VALID_SYNTAX_KINDS_FOR_FLOATING_COMMENTS: &[SyntaxKind] = &[SyntaxKind::VersionStatementNode];

/// Determines whether the SyntaxNodeOrToken is a valid target for a doc
/// comment.
fn valid_target_for_doc_comment(doc_comment_target: &SyntaxElement) -> bool {
    let kind = doc_comment_target.kind();

    // A BoundDeclNode can only have doc comments if it is a part of an
    // InputSection or OutputSection.
    if kind == SyntaxKind::BoundDeclNode {
        let Some(parent) = doc_comment_target.parent() else {
            return false;
        };

        return parent.kind() == SyntaxKind::InputSectionNode
            || parent.kind() == SyntaxKind::OutputSectionNode;
    }

    VALID_SYNTAX_KINDS_FOR_DOC_COMMENTS.contains(&kind)
}

/// The element targeted by a doc comment.
struct DocCommentTarget {
    /// The target element.
    element: SyntaxElement,
    /// Whether the comment is detached from the target.
    floating: bool,
}

/// Finds the first non-trivia [`SyntaxElement`] in the comment's siblings
/// to determine what this doc comment is targeting.
fn search_siblings_for_doc_comment_target(comment: &Comment) -> Option<DocCommentTarget> {
    let mut next = comment.inner().next_sibling_or_token();
    let mut floating = false;
    while let Some(sibling) = next {
        next = sibling.next_sibling_or_token();
        match &sibling {
            SyntaxElement::Node(_) => {
                return Some(DocCommentTarget {
                    element: sibling,
                    floating,
                });
            }
            SyntaxElement::Token(t) => {
                match t.kind() {
                    SyntaxKind::Whitespace => {
                        let lines = t.text().chars().filter(|c| *c == '\n').count();
                        if !floating {
                            floating = lines > 1;
                        }
                    }
                    SyntaxKind::Comment => {
                        let c = Comment::cast(t.clone()).unwrap();
                        if c.kind() != CommentKind::Documentation
                            && !matches!(c.kind(), CommentKind::Directive(_))
                        {
                            // A regular comment means we are not immediately
                            // touching the target
                            floating = true;
                        }
                    }
                    _ => {
                        return Some(DocCommentTarget {
                            element: sibling,
                            floating,
                        });
                    }
                }
            }
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

/// Gets the [`Span`] of the first token of the provided [`SyntaxElement`].
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
    /// Collects all doc comments in the block containing `comment`.
    ///
    /// Returns the span of the entire comment block.
    fn collect_consecutive_doc_comments(&mut self, comment: &Comment) -> Span {
        let mut next = comment.inner().next_sibling_or_token();
        let mut span_end = comment.span().end();
        while let Some(sibling) = next {
            next = sibling.next_sibling_or_token();
            if sibling.kind() == SyntaxKind::Whitespace {
                let lines = sibling
                    .as_token()
                    .unwrap()
                    .text()
                    .chars()
                    .filter(|c| *c == '\n')
                    .count();
                if lines > 1 {
                    break;
                }
                continue;
            }
            if let Some(continued_comment) =
                sibling.as_token().and_then(|t| Comment::cast(t.clone()))
                && continued_comment.kind() == CommentKind::Documentation
            {
                self.skip_count += 1;
                span_end = continued_comment.span().end();
                continue;
            }
            break;
        }

        Span::new(comment.span().start(), span_end - comment.span().start())
    }

    /// Produces an unused doc comment diagnostic for the doc comment block
    /// starting at `comment`. Updates `skip_count` along the way.
    fn lint_next_doc_comment_block(
        &mut self,
        diagnostics: &mut Diagnostics,
        comment: &Comment,
        target: Option<DocCommentTarget>,
    ) {
        let (mut target_span, floating) = target
            .as_ref()
            .map(|target| {
                (
                    Some(get_span_of_first_token_for_syntax_element(&target.element)),
                    target.floating,
                )
            })
            .unwrap_or((None, false));
        let mut valid_target = target
            .as_ref()
            .is_some_and(|t| valid_target_for_doc_comment(&t.element));
        let valid_floater = target
            .as_ref()
            .is_some_and(|t| VALID_SYNTAX_KINDS_FOR_FLOATING_COMMENTS.contains(&t.element.kind()));

        if valid_target && (!floating || valid_floater) {
            self.collect_consecutive_doc_comments(comment);
            return; // Valid doc comment
        }

        // Floaters targeting nodes that don't allow floaters should be linted
        // separately, without targeting the node below them.
        if floating && !valid_floater {
            target_span = None;
            valid_target = false;
        }

        let block_span = self.collect_consecutive_doc_comments(comment);
        diagnostics.add(unused_doc_comment_diagnostic(
            block_span,
            target_span,
            valid_target,
            floating,
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
        - Enum Choices"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    # This isn't documenting anything!
    ## The inputs for the workflow
    input {
        String name
    }

    # Neither is this!
    ## The outputs for the workflow
    output {
        String greeting = "Hello, ~{name}!"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the comments or moving them to applicable items"),
                snippet: r#"version 1.2

workflow example {
    input {
        ## The name to greet
        String name
    }

    output {
        ## The generated greeting
        String greeting = "Hello, ~{name}!"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> crate::TagSet {
        TagSet::new(&[Tag::Documentation])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &'static [&'static str] {
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
        if comment.kind() != CommentKind::Documentation {
            return;
        }

        if comment.is_inline_comment()
            && let Some(target) = find_inline_doc_comment_target(comment)
        {
            diagnostics.add(unused_doc_comment_diagnostic(
                comment.span(),
                Some(get_span_of_first_token_for_syntax_element(&target)),
                false,
                false,
            ));
            return;
        }

        let target = search_siblings_for_doc_comment_target(comment);
        self.lint_next_doc_comment_block(diagnostics, comment, target);
    }
}
