//! A lint rule that disallows redundant input names.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::UnboundDecl;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed input name rule.
const ID: &str = "InputName";

/// Declaration identifier too short
fn decl_identifier_too_short(span: Span, min_length: usize) -> Diagnostic {
    Diagnostic::note(format!(
        "declaration identifier must be at least {min_length} characters"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!(
        "rename the identifier to be at least {min_length} characters long"
    ))
}

/// Diagnostic for input names that start with [iI]n[A-Z_]
fn decl_identifier_starts_with_in(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'in'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'in'")
}

/// Diagnostic for input names that start with "input"
fn decl_identifier_starts_with_input(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'input'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'input'")
}

/// A lint rule for disallowed input names.
#[derive(Debug, Clone, Copy)]
pub struct InputNameRule {
    /// Track if we're in the input section.
    input_section: bool,
    /// The minimum length below which a name is flagged as too short.
    min_length: usize,
    /// Whether to flag disallowed name prefixes.
    check_prefixes: bool,
}

impl InputNameRule {
    /// Creates a new instance of the rule from the given configuration.
    pub fn new(config: &Config) -> Self {
        let config = config.resolved(ID);
        Self {
            input_section: false,
            min_length: config.min_length as usize,
            check_prefixes: config.check_prefixes,
        }
    }
}

impl Rule for InputNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures input names are meaningful (e.g. not generic like 'input', 'in', or too short)."
    }

    fn explanation(&self) -> &'static str {
        "Any input name matching these regular expressions will be flagged: [`/^[iI]n[A-Z_]/`](https://regex101.com/r/V0AFIG/2), \
[`/^input/i`](https://regex101.com/r/Ox8oYb/1) or [`/^..?$/`](https://regex101.com/r/IS1d49/1).\n\n\
It is redundant and needlessly verbose to use an input's name to \
specify that it is an input. Input names should be short yet descriptive. Prefixing a \
name with in or input adds length to the name without adding clarity or context. \
Additionally, names with only 2 characters can lead to confusion and obfuscates the \
content of an input. Input names should be at least 3 characters long."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    input {
        String input_name
    }

    command <<<
        echo "Hello, ~{input_name}!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    meta {
        description: "Says hello for the given name"
    }

    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
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
            SyntaxKind::InputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["OutputName", "DeclarationName"]
    }
}

impl Visitor for InputNameRule {
    fn reset(&mut self) {
        *self = Self {
            input_section: false,
            min_length: self.min_length,
            check_prefixes: self.check_prefixes,
        };
    }

    fn input_section(&mut self, _: &mut Diagnostics, reason: VisitReason, _: &InputSection) {
        self.input_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter && self.input_section {
            check_decl_name(
                diagnostics,
                &Decl::Bound(decl.clone()),
                &self.exceptable_nodes(),
                self.min_length,
                self.check_prefixes,
            );
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
        if reason == VisitReason::Enter && self.input_section {
            check_decl_name(
                diagnostics,
                &Decl::Unbound(decl.clone()),
                &self.exceptable_nodes(),
                self.min_length,
                self.check_prefixes,
            );
        }
    }
}

/// Check declaration name
fn check_decl_name(
    diagnostics: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
    min_length: usize,
    check_prefixes: bool,
) {
    let name = decl.name();
    let name = name.text();

    let length = name.len();
    if length < min_length {
        // name is too short
        diagnostics.exceptable_add(
            decl_identifier_too_short(decl.name().span(), min_length),
            SyntaxElement::from(decl.inner().clone()),
            exceptable_nodes,
        );
    }

    if !check_prefixes {
        return;
    }

    let mut name = name.chars().peekable();
    if let Some(c) = name.next()
        && (c == 'i' || c == 'I')
        && let Some('n') = name.peek()
    {
        name.next();
        if let Some(c) = name.peek() {
            if c.is_ascii_uppercase() || c == &'_' {
                // name starts with "in"
                diagnostics.exceptable_add(
                    decl_identifier_starts_with_in(decl.name().span()),
                    SyntaxElement::from(decl.inner().clone()),
                    exceptable_nodes,
                );
            } else {
                let s: String = name.take(3).collect();
                if s == "put" {
                    // name starts with "input"
                    diagnostics.exceptable_add(
                        decl_identifier_starts_with_input(decl.name().span()),
                        SyntaxElement::from(decl.inner().clone()),
                        exceptable_nodes,
                    );
                }
            }
        }
    }
}
