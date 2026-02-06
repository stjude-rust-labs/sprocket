//! A lint rule that disallows redundant input names.

use wdl_analysis::Diagnostics;
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

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed input name rule.
const ID: &str = "InputName";

/// Declaration identifier too short
fn decl_identifier_too_short(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier must be at least 3 characters")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to be at least 3 characters long")
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
#[derive(Default, Debug, Clone, Copy)]
pub struct InputNameRule {
    /// Track if we're in the input section.
    input_section: bool,
}

impl Rule for InputNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures input names are meaningful (e.g. not generic like 'input', 'in', or too short)."
    }

    fn explanation(&self) -> &'static str {
        "Any input name matching these regular expressions will be flagged: /^[iI]n[A-Z_]/, \
         /^input/i or /^..?$/. It is redundant and needlessly verbose to use an input's name to \
         specify that it is an input. Input names should be short yet descriptive. Prefixing a \
         name with in or input adds length to the name without adding clarity or context. \
         Additionally, names with only 2 characters can lead to confusion and obfuscates the \
         content of an input. Input names should be at least 3 characters long."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

task say_hello {
    input {
        String input_name
    }

    command <<<
        echo "Hello, ~{input_name}!"
    >>>
}
```"#,
            r#"Use instead:

```wdl
version 1.2

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
```"#,
        ]
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

    fn related_rules(&self) -> &[&'static str] {
        &["OutputName", "DeclarationName"]
    }
}

impl Visitor for InputNameRule {
    fn reset(&mut self) {
        *self = Self::default();
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
            );
        }
    }
}

/// Check declaration name
fn check_decl_name(
    diagnostics: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let name = decl.name();
    let name = name.text();

    let length = name.len();
    if length < 3 {
        // name is too short
        diagnostics.exceptable_add(
            decl_identifier_too_short(decl.name().span()),
            SyntaxElement::from(decl.inner().clone()),
            exceptable_nodes,
        );
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
