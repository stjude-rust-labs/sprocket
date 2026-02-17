//! A lint rule for misplaced doc comments that will not generate documentation.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxToken;
use wdl_ast::SyntaxTokenExt;
use wdl_ast::v1;

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
pub struct UnusedDocCommentsRule {}

impl UnusedDocCommentsRule {
    /// Walk up the preceding trivia from closest to the visited syntax node and
    /// check to see if there are doc comments present. If there are, lint
    /// on them.
    fn lint_doc_comments(
        &self,
        diagnostics: &mut Diagnostics,
        preceding_trivia: &mut impl Iterator<Item = SyntaxToken>,
    ) {
        let mut span: Option<Span> = None;
        let mut last_comment = None;

        let reversed_trivia = preceding_trivia.collect::<Vec<_>>().into_iter().rev();

        for token in reversed_trivia {
            match token.kind() {
                SyntaxKind::Comment => {
                    let comment = Comment::cast(token).expect(
                        "Token with SyntaxKind::Comment must be able to be casted to comment",
                    );

                    // Ignore directives when linting for unusued doc comments,
                    // and if we aren't already processing a doc comment don't include them in the
                    // highlighted span.
                    if last_comment.is_none() && comment.is_directive() {
                        continue;
                    }

                    if !comment.is_doc_comment() {
                        break;
                    }

                    span = span.map_or(Some(comment.span()), |prev_span| {
                        Some(Span::new(
                            comment.span().start(),
                            prev_span.end() - comment.span().start(),
                        ))
                    });

                    last_comment = Some(comment.inner().clone());
                }
                _ => break,
            }
        }

        if let (Some(span), Some(last_comment)) = (span, last_comment) {
            diagnostics.exceptable_add(
                unused_doc_comment_diagnostic(span),
                SyntaxElement::from(last_comment),
                &self.exceptable_nodes(),
            );
        }
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
    fn reset(&mut self) {}

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            &mut section.keyword().inner().preceding_trivia(),
        );
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            &mut section.keyword().inner().preceding_trivia(),
        );
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            &mut section.keyword().inner().preceding_trivia(),
        );
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if comment.is_directive() || comment.is_doc_comment() {
            return;
        }

        self.lint_doc_comments(diagnostics, &mut comment.inner().preceding_trivia());
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &v1::Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = expr
            .inner()
            .first_token()
            .expect("Expression must have one token");

        self.lint_doc_comments(diagnostics, &mut first.preceding_trivia());
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }
        self.lint_doc_comments(diagnostics, &mut stmt.keyword().inner().preceding_trivia());
    }

    fn conditional_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        for clause in stmt.clauses() {
            let token = clause
                .inner()
                .first_token()
                .expect("ConditionalStatementClause must have some token");

            self.lint_doc_comments(diagnostics, &mut token.preceding_trivia());
        }
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(parent) = decl.inner().parent()
            && (parent.kind() == SyntaxKind::OutputSectionNode
                || parent.kind() == SyntaxKind::InputSectionNode)
        {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            &mut decl
                .inner()
                .first_token()
                .expect("BoundDecl must have at least one token")
                .preceding_trivia(),
        );
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(diagnostics, &mut stmt.keyword().inner().preceding_trivia());
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            &mut section.keyword().inner().preceding_trivia(),
        );
    }

    fn scatter_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.lint_doc_comments(diagnostics, &mut stmt.keyword().inner().preceding_trivia());
    }
}
