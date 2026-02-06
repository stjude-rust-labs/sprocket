//! A lint rule for flagging placeholder options as deprecated.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOption;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the deprecated placeholder option rule.
const ID: &str = "DeprecatedPlaceholder";

/// Creates a diagnostic for the use of the deprecated `default` placeholder
/// option.
fn deprecated_default_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `default` placeholder option",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(
        "replace the `default` placeholder option with a call to the `select_first()` standard \
         library function",
    )
}

/// Creates a diagnostic for the use of the deprecated `sep` placeholder option.
fn deprecated_sep_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `sep` placeholder option",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(
        "replace the `sep` placeholder option with a call to the `sep()` standard library function",
    )
}

/// Creates a diagnostic for the use of the deprecated `true`/`false`
/// placeholder option.
fn deprecated_true_false_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `true`/`false` placeholder option",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("replace the `true`/`false` placeholder option with an `if`/`else` expression")
}

/// Detects the use of a deprecated placeholder option.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeprecatedPlaceholderRule {
    /// Stores the supported version of the WDL document we're visiting.
    version: Option<SupportedVersion>,
}

impl Rule for DeprecatedPlaceholderRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that deprecated expression placeholder options are not used."
    }

    fn explanation(&self) -> &'static str {
        "Expression placeholder options were deprecated in WDL v1.1 and will be removed in the \
         next major WDL version.

         - `sep` placeholder options should be replaced by the `sep()` standard library function.
         - `true/false` placeholder options should be replaced with `if`/`else` statements.
         - `default` placeholder options should be replaced by the `select_first()` standard \
         library function.

         This rule only evaluates for WDL V1 documents with a version of v1.1 or later, as this \
         was the version where the deprecation was introduced."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow example {
    meta {}

    Array[String] names = ["James", "Jimmy", "John"]
    String names_separated = "~{sep="," names}"

    output {}
}
```"#,
            r#"Use instead:

```wdl
version 1.2

workflow example {
    meta {}

    Array[String] names = ["James", "Jimmy", "John"]
    String names_separated = "~{sep(",", names)}"

    output {}
}
```"#,
        ]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::PlaceholderNode,
        ])
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Deprecated])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["DeprecatedObject", "ExpectedRuntimeKeys"]
    }
}

impl Visitor for DeprecatedPlaceholderRule {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn placeholder(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        placeholder: &Placeholder,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // This rule only executes for WDL documents that have v1.1 or greater.
        //
        // SAFETY: the version must always be set before we get to this point,
        // as document is the root node of the tree.
        match self.version.unwrap() {
            SupportedVersion::V1(v) if v >= V1::One => {}
            _ => return,
        };

        if let Some(option) = placeholder.option() {
            let diagnostic = match option {
                PlaceholderOption::Sep(option) => deprecated_sep_placeholder_option(option.span()),
                PlaceholderOption::Default(option) => {
                    deprecated_default_placeholder_option(option.span())
                }
                PlaceholderOption::TrueFalse(option) => {
                    deprecated_true_false_placeholder_option(option.span())
                }
            };
            diagnostics.exceptable_add(
                diagnostic,
                SyntaxElement::from(placeholder.inner().clone()),
                &self.exceptable_nodes(),
            )
        }
    }
}
