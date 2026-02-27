//! A lint rule for unnecessary input keyword when WDL version is >= 1.2.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CallStatement;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for this rule.
const ID: &str = "CallInputKeyword";

/// Creates a diagnostic for unnecessary input keyword.
fn call_input_unnecessary(span: Span) -> Diagnostic {
    Diagnostic::note("the `input:` keyword is unnecessary for WDL version 1.2 and later")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the `input:` keyword from the call statement")
}

/// Detects unnecessary use of the `input:` keyword in call statements.
#[derive(Default, Debug, Clone, Copy)]
pub struct CallInputKeywordRule {
    /// The WDL version of the file is stored here
    version: Option<SupportedVersion>,
}

impl Rule for CallInputKeywordRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that the `input:` keyword is not used in call statements when WDL version is 1.2 \
         or later."
    }

    fn explanation(&self) -> &'static str {
        "Starting with WDL version 1.2, the `input:` keyword in call statements is optional. This \
         specification change allows call inputs to be specified directly within the braces \
         without the `input:` keyword, resulting in a cleaner and more concise syntax. This rule \
         encourages adoption of the newer syntax when using WDL 1.2 or later."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow example {
    meta {}

    # In versions prior to WDL v1.2, the `input:` keyword
    # was necessary in `call` statements.
    call say_hello { input:
        name = "world",
    }

    output {}
}
```"#,
            r#"Use instead:

```wdl
version 1.2

workflow example {
    meta {}

    # This is correct for WDL v1.2 and later.
    call say_hello {
        name = "world",
    }

    output {}
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Deprecated, Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::CallStatementNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for CallInputKeywordRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn document(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _doc: &wdl_analysis::Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            self.version = Some(version);
        }
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        call: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let version = self.version.expect("document should have a version");

        if version <= SupportedVersion::V1(V1::One) {
            return;
        }

        if let Some(input_keyword) = call
            .inner()
            .children_with_tokens()
            .find(|c| c.kind() == SyntaxKind::InputKeyword)
        {
            diagnostics.exceptable_add(
                call_input_unnecessary(input_keyword.text_range().into()),
                SyntaxElement::from(call.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
