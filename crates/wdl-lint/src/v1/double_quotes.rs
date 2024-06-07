//! A lint rule for using double quoted strings.

use wdl_ast::experimental::span_of;
use wdl_ast::experimental::v1::Expr;
use wdl_ast::experimental::v1::LiteralExpr;
use wdl_ast::experimental::v1::Visitor;
use wdl_ast::experimental::Diagnostic;
use wdl_ast::experimental::Diagnostics;
use wdl_ast::experimental::Span;
use wdl_ast::experimental::VisitReason;

use super::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the double quotes rule.
const ID: &str = "DoubleQuotes";

/// Creates a "use double quotes" diagnostic.
fn use_double_quotes(span: Span) -> Diagnostic {
    Diagnostic::warning("string defined with single quotes")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change the single quotes to double quotes")
}

/// Detects strings that are not defined with double quotes.
#[derive(Debug, Clone, Copy)]
pub struct DoubleQuotesRule;

impl Rule for DoubleQuotesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that strings are defined using double quotes."
    }

    fn explanation(&self) -> &'static str {
        "All strings should be defined using double quotes. There is no semantic difference \
         between single and double quotes in WDL, but double quotes should be used exclusively to \
         ensure consistency and avoid any confusion."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Style])
    }

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(DoubleQuotesVisitor)
    }
}

/// Implements the visitor for the double quotes rule.
struct DoubleQuotesVisitor;

impl Visitor for DoubleQuotesVisitor {
    type State = Diagnostics;

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Expr::Literal(LiteralExpr::String(s)) = expr {
            if s.quote() != '"' {
                state.add(use_double_quotes(span_of(s)));
            }
        }
    }
}
