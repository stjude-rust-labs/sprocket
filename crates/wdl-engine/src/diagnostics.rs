//! Implementation of evaluation diagnostics.

use std::fmt;

use wdl_analysis::diagnostics::Io;
use wdl_analysis::types::Type;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::TreeToken;

/// Creates an "integer not in range" diagnostic.
pub fn integer_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "literal integer exceeds the range for a 64-bit signed integer ({min}..={max})",
        min = i64::MIN,
        max = i64::MAX,
    ))
    .with_label("this literal integer is not in range", span)
}

/// Creates an "integer negation not in range" diagnostic.
pub fn integer_negation_not_in_range(value: i64, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "negation of integer value {value} exceeds the range for a 64-bit signed integer \
         ({min}..={max})",
        min = i64::MIN,
        max = i64::MAX,
    ))
    .with_highlight(span)
}

/// Creates a "float not in range" diagnostic.
pub fn float_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "literal float exceeds the range for a 64-bit float ({min:+e}..={max:+e})",
        min = f64::MIN,
        max = f64::MAX,
    ))
    .with_label("this literal float is not in range", span)
}

/// Creates a "numeric overflow" diagnostic.
pub fn numeric_overflow(span: Span) -> Diagnostic {
    Diagnostic::error("evaluation of arithmetic expression resulted in overflow")
        .with_highlight(span)
}

/// Creates a "division by zero" diagnostic.
pub fn division_by_zero(span: Span, divisor_span: Span) -> Diagnostic {
    Diagnostic::error("attempt to divide by zero")
        .with_highlight(span)
        .with_label("this expression evaluated to zero", divisor_span)
}

/// Creates a "exponent not in range" diagnostic.
pub fn exponent_not_in_range(span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "exponent exceeds acceptable range ({min}..={max})",
        min = u32::MIN,
        max = u32::MAX,
    ))
    .with_label("this value exceeds the range for an exponent", span)
}

/// Creates a "runtime type mismatch" diagnostic.
pub fn runtime_type_mismatch(
    e: anyhow::Error,
    expected: &Type,
    expected_span: Span,
    actual: &Type,
    actual_span: Span,
) -> Diagnostic {
    let e = e.context(format!(
        "type mismatch: expected type `{expected}`, but found type `{actual}`"
    ));

    Diagnostic::error(format!("{e:#}"))
        .with_label(format!("this is type `{actual}`"), actual_span)
        .with_label(format!("this expects type `{expected}`"), expected_span)
}

/// Creates an "if conditional mismatch" diagnostic.
pub fn if_conditional_mismatch(e: anyhow::Error, actual: &Type, actual_span: Span) -> Diagnostic {
    let e = e.context(format!(
        "type mismatch: expected `if` conditional expression to be type `Boolean`, but found type \
         `{actual}`"
    ));

    Diagnostic::error(format!("{e:#}")).with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates an "array index out of range" diagnostic.
pub fn array_index_out_of_range(
    index: i64,
    count: usize,
    span: Span,
    target_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!("array index {index} is out of range"))
        .with_highlight(span)
        .with_label(
            if count == 0 {
                "this array is empty".to_string()
            } else {
                format!(
                    "this array has only {count} element{s}",
                    s = if count == 1 { "" } else { "s" }
                )
            },
            target_span,
        )
}

/// Creates a "map key not found" diagnostic.
pub fn map_key_not_found(span: Span) -> Diagnostic {
    Diagnostic::error("the map does not contain an entry for the specified key")
        .with_highlight(span)
}

/// Creates a "not an object member" diagnostic.
pub fn not_an_object_member<T: TreeToken>(member: &Ident<T>) -> Diagnostic {
    Diagnostic::error(format!(
        "object does not have a member named `{member}`",
        member = member.text()
    ))
    .with_highlight(member.span())
}

/// Creates an "exponentiation requirement" diagnostic.
pub fn exponentiation_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of the exponentiation operator requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a "multi-line string requirement" diagnostic.
pub fn multiline_string_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of multi-line strings requires WDL version 1.2").with_highlight(span)
}

/// Creates a "function call failed" diagnostic.
pub fn function_call_failed(name: &str, error: impl fmt::Display, span: Span) -> Diagnostic {
    Diagnostic::error(format!("call to function `{name}` failed: {error}")).with_highlight(span)
}

/// Creates a "input/output/declaration evaluation failed" diagnostic.
pub fn decl_evaluation_failed(
    e: anyhow::Error,
    name: &str,
    task: bool,
    decl_name: &str,
    io: Option<Io>,
    span: Span,
) -> Diagnostic {
    let e = e.context(format!(
        "failed to evaluate {decl_kind} `{decl_name}` for {kind} `{name}`",
        kind = if task { "task" } else { "workflow" },
        decl_kind = match io {
            Some(Io::Input) => "input",
            Some(Io::Output) => "output",
            None => "declaration",
        },
    ));

    Diagnostic::error(format!("{e:#}")).with_highlight(span)
}

/// Creates a "task localization failed" diagnostic.
pub fn task_localization_failed(e: anyhow::Error, name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "{e:#}",
        e = e.context(format!("failed to localize inputs for task `{name}`"))
    ))
    .with_highlight(span)
}

/// Creates a "task execution failed" diagnostic.
pub fn task_execution_failed(e: anyhow::Error, name: &str, id: &str, span: Span) -> Diagnostic {
    Diagnostic::error(if name != id {
        format!("task execution failed for task `{name}` (id `{id}`): {e:#}")
    } else {
        format!("task execution failed for task `{name}`: {e:#}")
    })
    .with_label("this task failed to execute", span)
}
