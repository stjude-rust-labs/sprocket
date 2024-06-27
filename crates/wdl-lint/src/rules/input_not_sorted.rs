//! A lint rule for sorting of inputs.

use wdl_ast::span_of;
use wdl_ast::v1::InputSection;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the input not sorted rule.
const ID: &str = "InputSorting";

/// Creates a "input not sorted" diagnostic.
fn input_not_sorted(span: Span, sorted_inputs: String) -> Diagnostic {
    Diagnostic::warning("input not sorted")
        .with_rule(ID)
        .with_label("input section must be sorted".to_string(), span)
        .with_fix(format!("sort input statements as: \n{}", sorted_inputs))
}

/// Detects unsorted input declarations.
#[derive(Debug, Clone, Copy)]
pub struct InputNotSortedRule;

impl Rule for InputNotSortedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that input declarations are sorted."
    }

    fn explanation(&self) -> &'static str {
        "Each input declaration section should be sorted. This rule enforces an opinionated \
         sorting. First sorts by 1. required inputs, 2. optional inputs without defaults, 3. \
         optional inputs with defaults, and 4. inputs with a default value. Then by the type: 1. \
         File, 2. Array[*]+, 3. Array[*], 4. struct, 5. Object, 6. Map[*, *], 7. Pair[*, *], 8. \
         String, 9. Boolean, 10. Float, 11. Int. For ordering of the same compound type (Array[*], \
         Map[*, *], Pair[*, *]), drop the outermost type (Array, Map, etc.) and recursively apply \
         above sorting on the first inner type *, with ties broken by the second inner type. \
         Continue this pattern as far as possible. Once this ordering is satisfied, it is up to \
         the developer for final order of inputs of the same type."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Sorting])
    }
}

impl Visitor for InputNotSortedRule {
    type State = Diagnostics;

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        input: &InputSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Get input section declarations
        let decls: Vec<_> = input.declarations().collect();
        let mut sorted_decls = decls.clone();
        sorted_decls.sort();

        let input_string: String = sorted_decls
            .clone()
            .into_iter()
            .map(|decl| decl.syntax().text().to_string() + "\n")
            .collect::<String>();
        let mut errors = 0;

        decls
            .into_iter()
            .zip(sorted_decls)
            .for_each(|(decl, sorted_decl)| {
                if decl != sorted_decl {
                    errors += 1;
                }
            });
        if errors > 0 {
            state.add(input_not_sorted(span_of(input), input_string));
        }
    }
}
