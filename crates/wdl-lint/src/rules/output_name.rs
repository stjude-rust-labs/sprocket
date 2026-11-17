//! A lint rule that disallows redundant output names.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::UnboundDecl;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed output name rule.
const ID: &str = "OutputName";

/// Declaration identifier too short
fn decl_identifier_too_short(severity: Severity, span: Span, min_length: u8) -> Diagnostic {
    Diagnostic::new(
        severity,
        format!("declaration identifier must be at least {min_length} characters"),
    )
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "rename the identifier to be at least {min_length} characters long"
    ))
}

/// Diagnostic for input names that start with [oO]ut[A-Z_]
fn decl_identifier_starts_with_out(severity: Severity, span: Span) -> Diagnostic {
    Diagnostic::new(severity, "declaration identifier starts with 'out'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'out'")
}

/// Diagnostic for input names that start with "output"
fn decl_identifier_starts_with_output(severity: Severity, span: Span) -> Diagnostic {
    Diagnostic::new(severity, "declaration identifier starts with 'output'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'output'")
}

/// A lint rule for disallowed output names.
#[derive(Debug, Clone)]
pub struct OutputNameRule {
    /// The severity of the rule's diagnostics.
    severity: Severity,
    /// Track if we're in the output section.
    output_section: bool,
    /// The minimum length below which a name is flagged as too short.
    min_length: u8,
    /// Whether to flag names that start with a disallowed prefix.
    check_prefixes: bool,
}

impl OutputNameRule {
    /// Creates a new instance of the rule.
    pub fn new(config: &Config) -> Self {
        Self {
            severity: config.output_name.diagnostic_severity(),
            output_section: false,
            min_length: config.output_name.min_length,
            check_prefixes: config.output_name.check_prefixes,
        }
    }
}

impl Rule for OutputNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures output names are meaningful (e.g. not generic like 'output', 'out', or too short)."
    }

    fn explanation(&self) -> &'static str {
        "By default, any output name matching these regular expressions will be flagged: [`/^[oO]ut[A-Z_]/`](https://regex101.com/r/r6v2fL/1), \
[`/^output/i`](https://regex101.com/r/vybrEi/1) or [`/^..?$/`](https://regex101.com/r/5yWAfk/1).\n\n\
\
It is redundant and needlessly verbose to use an output's name to \
specify that it is an output. Output names should be short yet descriptive. Prefixing a \
name with \"out\" or \"output\" adds length to the name without adding clarity or context. \
Additionally, short names can lead to confusion and obfuscate the \
content of an output. Output names should be at least `min_length` characters long."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task generate_greeting {
    input {
        String name
    }

    command <<<
    >>>

    output {
        String output_greeting = "Hello, ~{name}!"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task generate_greeting {
    input {
        String name
    }

    command <<<
    >>>

    output {
        String greeting = "Hello, ~{name}!"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming, Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["InputName", "DeclarationName"]
    }
}

impl Visitor for OutputNameRule {
    fn reset(&mut self) {
        self.output_section = false;
    }

    fn output_section(&mut self, _: &mut Diagnostics, reason: VisitReason, _: &OutputSection) {
        self.output_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter && self.output_section {
            check_decl_name(
                self.severity,
                self.min_length,
                self.check_prefixes,
                diagnostics,
                &Decl::Bound(decl.clone()),
                &self.exceptable_nodes(),
            );
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
        if reason == VisitReason::Enter && self.output_section {
            check_decl_name(
                self.severity,
                self.min_length,
                self.check_prefixes,
                diagnostics,
                &Decl::Unbound(decl.clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Check declaration name
fn check_decl_name(
    severity: Severity,
    min_length: u8,
    check_prefixes: bool,
    diagnostics: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let name = decl.name();
    let name = name.text();

    let length = name.len();
    if length < min_length as usize {
        // name is too short
        diagnostics.exceptable_add(
            decl_identifier_too_short(severity, decl.name().span(), min_length),
            decl.inner(),
            exceptable_nodes,
        );
    }

    if !check_prefixes {
        return;
    }

    let mut name = name.chars().peekable();
    if let Some(c) = name.next()
        && (c == 'o' || c == 'O')
        && let Some('u') = name.peek()
    {
        name.next();
        if let Some('t') = name.peek() {
            name.next();
            if let Some(c) = name.peek() {
                if c.is_ascii_uppercase() || c == &'_' {
                    // name starts with "out"
                    diagnostics.exceptable_add(
                        decl_identifier_starts_with_out(severity, decl.name().span()),
                        decl.inner(),
                        exceptable_nodes,
                    );
                } else {
                    let s: String = name.take(3).collect();
                    if s == "put" {
                        // name starts with "output"
                        diagnostics.exceptable_add(
                            decl_identifier_starts_with_output(severity, decl.name().span()),
                            decl.inner(),
                            exceptable_nodes,
                        );
                    }
                }
            }
        }
    }
}
