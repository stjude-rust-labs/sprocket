//! A lint rule for ensuring no curly commands are used.

use wdl_ast::support;
use wdl_ast::v1::CommandSection;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the no curly commands rule.
const ID: &str = "NoCurlyCommands";

/// Creates a "curly commands" diagnostic.
fn curly_commands(task: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
        "task `{task}` uses curly braces in command section"
    ))
    .with_rule(ID)
    .with_label("this command section uses curly braces", span)
    .with_fix("instead of curly braces, use heredoc syntax (<<<>>>>) for command sections")
}

/// Detects curly command section for tasks.
#[derive(Default, Debug, Clone, Copy)]
pub struct NoCurlyCommandsRule;

impl Rule for NoCurlyCommandsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks use heredoc syntax in command sections."
    }

    fn explanation(&self) -> &'static str {
        "Curly command blocks are no longer considered idiomatic WDL. Idiomatic WDL code uses \
         heredoc command blocks instead. This is because curly command blocks create ambiguity \
         with Bash syntax."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity])
    }
}

impl Visitor for NoCurlyCommandsRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if !section.is_heredoc() {
            let name = section.parent().name();
            let command_keyword = support::token(section.syntax(), SyntaxKind::CommandKeyword)
                .expect("should have a command keyword token");

            state.add(curly_commands(
                name.as_str(),
                command_keyword.text_range().to_span(),
            ));
        }
    }
}
