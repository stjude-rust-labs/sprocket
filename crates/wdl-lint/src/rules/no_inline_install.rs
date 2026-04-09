//! A lint rule for disallowing the use of inline installations in command sections.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use regex::Regex;
use wdl_ast::v1::CommandSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the NoInlineInstall rule.
const ID: &str = "NoInlineInstall";

// option 1: hardcoded
const INSTALL_REGEX: &str = r"(?im)(apt-get|apt|pip|pip3|yum|dnf|brew|npm|gem|cargo|go|conda|mamba)(?:\s+--[\w-]+=?\S*)*\s+(install)|(conda|mamba)\s+(?:create)|(apk)\s(add)\b";
const PIPED_INSTALL_REGEX: &str = r"(?im)(curl|wget)\b.*\|\s*(bash|sh|python3?)\b";

// Creates a diagnostic for an inline installation in a command section.
fn inline_install_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::warning("inline installation of packages is discouraged")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("consider using a more robust dependency management approach, such as a Docker image or a Conda environment")
}

/// A lint rule for disallowing the use of inline installations in command sections.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoInlineInstall;

impl Rule for NoInlineInstall {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures inline installation of packages is not used in command sections."
    }

    fn explanation(&self) -> &'static str {
        ""
    }

    fn examples(&self) -> &'static [&'static str] {
        &[]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::OutputSectionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for NoInlineInstall {
    fn reset(&mut self) {
        *self = Self;
    }

    fn command_section(
            &mut self,
            diagnostics: &mut Diagnostics,
            reason: VisitReason,
            section: &CommandSection,
        ) {
        if reason != VisitReason::Enter {
            return;
        }

        let inline_regex = Regex::new(INSTALL_REGEX).unwrap();
        let piped_regex = Regex::new(PIPED_INSTALL_REGEX).unwrap();
        let text = section.text().to_string();
        for mat in inline_regex.find_iter(&text) {
            diagnostics.exceptable_add(
            inline_install_diagnostic(Span::new(section.span().start() + mat.start(), mat.end() - mat.start() )), 
            SyntaxElement::from(section.inner().clone()),
            &self.exceptable_nodes());
            tracing::info!("Found potential inline installation: {}", mat.as_str());
        }
        for mat in piped_regex.find_iter(&text) {
            diagnostics.exceptable_add(
            inline_install_diagnostic(Span::new(section.span().start() + mat.start(), mat.end() - mat.start() )), 
            SyntaxElement::from(section.inner().clone()),
            &self.exceptable_nodes());
            tracing::info!("Found potential piped installation: {}", mat.as_str());
        }

    }

}
