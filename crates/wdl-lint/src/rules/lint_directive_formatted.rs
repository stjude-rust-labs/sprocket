//! A lint rule for flagging malformed lint directives.
use wdl_analysis::Diagnostics;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::is_inline_comment;

/// The identifier for the Malformed Lint Directive rule.
const ID: &str = "LintDirectiveFormatted";
/// The accepted lint directives.
const ACCEPTED_LINT_DIRECTIVES: [&str; 1] = ["except"];

/// Creates an "Excessive Whitespace" diagnostic.
fn excessive_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("expected exactly one space before lint directive")
        .with_rule(ID)
        .with_label("this whitespace is unexpected", span)
        .with_fix("replace this whitespace with a single space")
}

/// Creates an "Inline Lint Directive" diagnostic.
fn inline_lint_directive(span: Span) -> Diagnostic {
    Diagnostic::warning("lint directive must be on its own line")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("move the lint directive to its own line")
}

/// Creates an "Invalid Lint Directive" diagnostic.
fn invalid_lint_directive(name: &str, span: Span) -> Diagnostic {
    let accepted_directives = ACCEPTED_LINT_DIRECTIVES.join(", ");
    Diagnostic::warning(format!("lint directive `{name}` is not recognized"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(format!(
            "use any of the recognized lint directives: [{accepted_directives:#?}]"
        ))
}

/// Creates a "Missing Lint Directive" diagnostic.
fn missing_lint_directive(span: Span) -> Diagnostic {
    Diagnostic::warning("lint directive not found")
        .with_rule(ID)
        .with_label("missing lint directive", span)
        .with_fix("add a lint directive or change `#@` prefix")
}

/// Creates a "Missing Whitespace" diagnostic.
fn missing_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("expected exactly one space before lint directive")
        .with_rule(ID)
        .with_label("expected a space before this", span)
        .with_fix("add a single space")
}

/// Creates a "No Colon Detected" diagnostic.
fn no_colon_detected(span: Span) -> Diagnostic {
    Diagnostic::warning("expected a colon to follow a lint directive")
        .with_rule(ID)
        .with_label("expected a colon here", span)
        .with_fix("add a colon after the lint directive")
}

/// Detects a malformed lint directive.
#[derive(Default, Debug, Clone, Copy)]
pub struct LintDirectiveFormattedRule;

impl Rule for LintDirectiveFormattedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures lint directives are correctly formatted."
    }

    fn explanation(&self) -> &'static str {
        "This rule checks that lint directives are properly formatted.\nLint directives must be on \
         their own line, only preceded by whitespace. They should follow the pattern `#@ \
         <directive>: <value>` _exactly_. Currently the only accepted lint directive is `except`. \
         For example, `#@ except: LintDirectiveFormatted`."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Correctness, Tag::SprocketCompatibility])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for LintDirectiveFormattedRule {
    fn reset(&mut self) {
        *self = Self;
    }

    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {
        if let Some(lint_directive) = comment.text().strip_prefix("#@") {
            let base_offset = comment.span().start();

            if is_inline_comment(comment) {
                diagnostics.add(inline_lint_directive(comment.span()));
            }

            if lint_directive.trim().is_empty() {
                diagnostics.add(missing_lint_directive(Span::new(
                    base_offset + 2,
                    lint_directive.len(),
                )));
                return;
            }

            if !lint_directive.starts_with(" ") {
                diagnostics.add(missing_whitespace(Span::new(base_offset + 2, 1)));
            }

            if lint_directive.starts_with("  ") {
                let leading_whitespace_len =
                    lint_directive.len() - lint_directive.trim_start().len();
                diagnostics.add(excessive_whitespace(Span::new(
                    base_offset + 2,
                    leading_whitespace_len,
                )));
            }

            if let Some(mut directive) = lint_directive.trim().split(" ").next() {
                if !directive.ends_with(":") {
                    diagnostics.add(no_colon_detected(Span::new(
                        base_offset + 3 + directive.chars().count(),
                        1,
                    )));
                } else if let Some(stripped_directive) = directive.strip_suffix(":") {
                    directive = stripped_directive;
                }

                if !ACCEPTED_LINT_DIRECTIVES.contains(&directive) {
                    diagnostics.add(invalid_lint_directive(
                        directive,
                        Span::new(base_offset + 3, directive.chars().count()),
                    ));
                }
            }
        }
    }
}
