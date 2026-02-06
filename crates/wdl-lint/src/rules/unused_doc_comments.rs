//! A lint rule for misplaced doc comments that will not generate documentation.

use wdl_analysis::Diagnostics;
use wdl_analysis::EXCEPT_COMMENT_PREFIX;
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
use wdl_ast::TreeToken;
use wdl_ast::v1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// Prefix for defining the start of a doc comment.
const DOC_COMMENT_PREFIX: &str = "## ";

/// An "empty doc comment" should have this text.
///
/// Used to ensure that if we are parsing a doc comment and we don't see
/// `DOC_COMMENT_PREFIX`, but we see this text we continue parsing.
const EMPTY_DOC_COMMENT_TEXT: &str = "##";

/// The ID for the UnusedDocComments lint.
const ID: &str = "UnusedDocComments";

/// Creates a diagnostic for a misplaced doc comment
fn unused_doc_comment_diagnostic(span: Span, syntax_kind: SyntaxKind) -> Diagnostic {
    Diagnostic::note(format!(
        "Doc comments aren't supported on {}",
        syntax_kind.describe()
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_help("Doc comments must be attached to WDL items that support doc comments")
    .with_fix("If you're intending to use a regular comment here, replace the `##` with `#`")
}

/// Detects whether a doc comment has been placed atop a Node that we do not
/// generate documentation for.
#[derive(Default, Debug, Clone)]
pub struct UnusedDocCommentsRule {
    /// Tracks whether the `version_statement` has been processed or not. At the
    /// moment, any comment with a doc comment prefix before the
    /// `version_statement` is assumed to be a part of the WDL preamble.
    version_statement_processed: bool,
}

impl UnusedDocCommentsRule {
    /// Walk up the preceding trivia from closest to the visited syntax node and
    /// check to see if there are doc comments present. If there are, lint
    /// on them.
    fn lint_doc_comments(
        &self,
        diagnostics: &mut Diagnostics,
        kind: SyntaxKind,
        preceding_trivia: &mut impl Iterator<Item = SyntaxToken>,
    ) {
        let mut span: Option<Span> = None;
        let mut last_comment = None;

        let reversed_trivia = preceding_trivia.collect::<Vec<_>>().into_iter().rev();

        for token in reversed_trivia {
            match token.kind() {
                SyntaxKind::Comment => {
                    if !(token.text().starts_with(DOC_COMMENT_PREFIX)
                        || (token.text() == EMPTY_DOC_COMMENT_TEXT))
                    {
                        break;
                    }

                    if last_comment.is_none() && token.text().starts_with(EXCEPT_COMMENT_PREFIX) {
                        continue;
                    }

                    span = span.map_or(Some(token.span()), |prev_span| {
                        Some(Span::new(
                            token.span().start(),
                            prev_span.end() - token.span().start(),
                        ))
                    });

                    if token.text().starts_with(EXCEPT_COMMENT_PREFIX) {
                        continue;
                    }

                    last_comment = Some(token);
                }
                _ => break,
            }
        }

        if let (Some(span), Some(last_comment)) = (span, last_comment) {
            diagnostics.exceptable_add(
                unused_doc_comment_diagnostic(span, kind),
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
        "Some Workflow Definition Language items and sections do not supported doc comments \
         (`##`). This lint reports if a doc comment is attached to some section or item that isn't \
         supported.

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
        self.version_statement_processed = false;
    }

    // Doc Comments before the version statement are assumed to be a "preamble" for
    // now.
    fn version_statement(
        &mut self,
        _diagnostics: &mut Diagnostics,
        _reason: wdl_analysis::VisitReason,
        _stmt: &wdl_ast::VersionStatement,
    ) {
        self.version_statement_processed = true;
    }

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
            section.kind(),
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
            section.kind(),
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
            section.kind(),
            &mut section.keyword().inner().preceding_trivia(),
        );
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if comment.text().starts_with(EXCEPT_COMMENT_PREFIX)
            || comment.text() == EMPTY_DOC_COMMENT_TEXT
            || comment.text().starts_with(DOC_COMMENT_PREFIX)
            || !self.version_statement_processed
        {
            return;
        }

        self.lint_doc_comments(
            diagnostics,
            comment.kind(),
            &mut comment.inner().preceding_trivia(),
        );
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &v1::Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = expr
            .inner()
            .first_token()
            .expect("Expression must have one token");

        self.lint_doc_comments(diagnostics, expr.kind(), &mut first.preceding_trivia());
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
        self.lint_doc_comments(
            diagnostics,
            stmt.kind(),
            &mut stmt.keyword().inner().preceding_trivia(),
        );
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

            self.lint_doc_comments(
                diagnostics,
                clause.inner().kind(),
                &mut token.preceding_trivia(),
            );
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
            decl.kind(),
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

        self.lint_doc_comments(
            diagnostics,
            stmt.kind(),
            &mut stmt.keyword().inner().preceding_trivia(),
        );
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
            section.kind(),
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

        self.lint_doc_comments(
            diagnostics,
            stmt.kind(),
            &mut stmt.keyword().inner().preceding_trivia(),
        );
    }
}
