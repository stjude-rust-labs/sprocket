//! A lint rule for disallowing the use of inline installations in command
//! sections.

use std::sync::LazyLock;

use regex::Regex;
use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
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

/// The identifier for the InlineInstall rule.
const ID: &str = "InlineInstall";

/// Regex pattern to match inline installations.
static INSTALL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Basic `{x} install` commands
    const GENERIC_PACKAGE_MANAGERS: &str = r"(?:apt(?:-get)?|pip[23]?|yum|dnf|brew|npm|gem|cargo|go)(?:\s+-[\w-]+)*\s+(?:install|i)";

    const CONDA: &str = r"(conda|mamba)(?:\s+-[\w-]+)*\s+(?:install|create)";
    const ALPINE: &str = r"(apk)(?:\s+-[\w-]+)*\s+add";

    // R is special, as we might be dealing with CLI package installations with `Rscript`, or
    // embedded R scripts. So it gets its own `r_cmd` group, so we *only* match on the installation itself.
    const R: &str = r"(?P<r_cmd>install\.packages|\w+::install(?:_\w+)?)";
    let r = format!(r"(?:(?:Rscript|R)(?:\s+-[\w-]+)*\s+.*?)?{R}");

    Regex::new(&format!(r"(?im)\b(?:sudo\s+)?(?:(?P<cmd>{GENERIC_PACKAGE_MANAGERS}|{CONDA}|{ALPINE})|{r})\b")).unwrap()
});

/// Regex pattern to match piped installations.
static PIPED_INSTALL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)\b(curl|wget)\b.*\|\s*(bash|sh|python[23]?)\b").unwrap());

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
pub struct InlineInstall;

impl Rule for InlineInstall {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures inline installation of packages is not used in command sections."
    }

    fn explanation(&self) -> &'static str {
        "All required software should be installed in the execution environment before the \
         workflow is run. Inline installations can lead to lack of reproducibility, portability, \
         and incur a performance cost, as software must be downloaded and installed every \
         invocation."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_hello {
    command <<<
        sudo apt install python3

        python3 -c "print('Hello, world!')"
    >>>

    requirements {
        container: "debian:trixie"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider using a dedicated container image"),
                snippet: r#"version 1.3

task say_hello {
    command <<<
        python3 -c "print('Hello, world!')"
    >>>

    requirements {
        container: "python:trixie"
    }
}
"#,
            }),
        }]
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

impl Visitor for InlineInstall {
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

        let text = section.text().to_string();
        for mat in INSTALL_REGEX
            .captures_iter(&text)
            .filter_map(|caps| caps.name("cmd").or_else(|| caps.name("r_cmd")))
            .chain(PIPED_INSTALL_REGEX.find_iter(&text))
        {
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
