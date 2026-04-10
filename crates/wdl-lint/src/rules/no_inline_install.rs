//! A lint rule for disallowing the use of inline installations in command
//! sections.

use regex::Regex;
use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CommandSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the NoInlineInstall rule.
const ID: &str = "NoInlineInstall";

/// Regex pattern to match inline installations.
const INSTALL_REGEX: &str = r"(?im)(apt-get|apt|pip|pip3|yum|dnf|brew|npm|gem|cargo|go)(?:\s+(-[\w-])+)*\s+install|(conda|mamba)(?:\s+(-[\w-])+)*\s+(?:install|create)|(apk)(?:\s+(-[\w-])+)*\s+\s*(add)\b";

/// Regex pattern to match piped installations.
const PIPED_INSTALL_REGEX: &str = r"(?im)(curl|wget)\b.*\|\s*(bash|sh|python3?)\b";

/// Creates a diagnostic for an inline installation in a command section.
fn inline_install_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::warning("inline installation of packages is discouraged")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "consider using a more robust dependency management approach, such as a container \
             image",
        )
}

/// A lint rule for disallowing the use of inline installations in command
/// sections.
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
        "All required software should be installed in the execution environment before the \
         workflow is run. Inline installations can lead to lack of reproducibility,portability, \
         and incur a performance cost, as software must be downloaded and installed every \
         invocation."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

task say_hello {
    meta {}

    command <<<
        pip install --user pandas
        curl -sL https://example.com/install.sh | bash
    >>>
}
```"#,
            r#"Avoid inline installations in command sections"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness, Tag::Portability, Tag::Performance])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::CommandSectionNode,
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
                inline_install_diagnostic(Span::new(
                    section.span().start() + mat.start(),
                    mat.end() - mat.start(),
                )),
                SyntaxElement::from(section.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
        for mat in piped_regex.find_iter(&text) {
            diagnostics.exceptable_add(
                inline_install_diagnostic(Span::new(
                    section.span().start() + mat.start(),
                    mat.end() - mat.start(),
                )),
                SyntaxElement::from(section.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
