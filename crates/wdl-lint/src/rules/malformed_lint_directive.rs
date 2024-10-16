//! A lint rule for flagging malformed lint directives.
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::is_inline_comment;

/// The identifier for the Malformed Lint Directive rule.
const ID: &str = "MalformedLintDirective";
/// The accepted lint directives.
const ACCEPTED_LINT_DIRECTIVES: [&str; 1] = ["except"];

/// Creates an "Excessive Whitespace" diagnostic.
fn excessive_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("expected exactly one space before lint directive")
        .with_rule(ID)
        .with_label("replace this whitespace with a single space", span)
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
            "use any of the recognized lint directives: [{:#?}]",
            accepted_directives
        ))
}

/// Creates a "Missing Lint Directive" diagnostic.
fn missing_lint_directive(span: Span) -> Diagnostic {
    Diagnostic::warning("lint directive not found")
        .with_rule(ID)
        .with_label("missing lint directive", span)
}

/// Creates a "Missing Whitespace" diagnostic.
fn missing_whitespace(span: Span) -> Diagnostic {
    Diagnostic::warning("expected exactly one space before lint directive")
        .with_rule(ID)
        .with_label("expected a single space before this", span)
}

/// Creates a "No Colon Detected" diagnostic.
fn no_colon_detected(span: Span) -> Diagnostic {
    Diagnostic::warning("expected a colon to follow a lint directive")
        .with_rule(ID)
        .with_label("expected a colon here", span)
}

/// Detects a malformed lint directive.
#[derive(Default, Debug, Clone, Copy)]
pub struct MalformedLintDirectiveRule;

impl Rule for MalformedLintDirectiveRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags malformed lint directives."
    }

    fn explanation(&self) -> &'static str {
        "Comments which begin with `#@` must only contain valid lint directives. Lint directives \
         must be on their own line, only preceded by whitespace. Lint directives should follow the \
         pattern `#@ <directive>: <value>` _exactly_. Currently the only accepted lint directive \
         is `except`. For example, `#@ except: MalformedLintDirective`."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Correctness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }
}

impl Visitor for MalformedLintDirectiveRule {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, _: VisitReason, _: &Document, _: SupportedVersion) {
        // This is intentionally empty, as this rule has no state.
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        if let Some(lint_directive) = comment.as_str().strip_prefix("#@") {
            let base_offset = comment.span().start();

            if is_inline_comment(comment) {
                state.add(inline_lint_directive(comment.span()));
            }

            if lint_directive.trim().is_empty() {
                state.add(missing_lint_directive(Span::new(
                    base_offset + 2,
                    lint_directive.len(),
                )));
                return;
            }

            if !lint_directive.starts_with(" ") {
                state.add(missing_whitespace(Span::new(base_offset + 2, 1)));
            }

            if lint_directive.starts_with("  ") {
                let leading_whitespace_len =
                    lint_directive.len() - lint_directive.trim_start().len();
                state.add(excessive_whitespace(Span::new(
                    base_offset + 2,
                    leading_whitespace_len,
                )));
            }

            if let Some(mut directive) = lint_directive.trim().split(" ").next() {
                if !directive.ends_with(":") {
                    state.add(no_colon_detected(Span::new(
                        base_offset + 3 + directive.chars().count(),
                        1,
                    )));
                } else if let Some(stripped_directive) = directive.strip_suffix(":") {
                    directive = stripped_directive;
                }

                if !ACCEPTED_LINT_DIRECTIVES.contains(&directive) {
                    state.add(invalid_lint_directive(
                        directive,
                        Span::new(base_offset + 3, directive.chars().count()),
                    ));
                }
            }
        }
    }
}
