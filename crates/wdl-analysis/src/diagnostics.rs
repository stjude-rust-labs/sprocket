//! Module for all diagnostic creation functions.

use std::fmt;

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::Version;

use crate::UNNECESSARY_FUNCTION_CALL;
use crate::UNUSED_CALL_RULE_ID;
use crate::UNUSED_DECL_RULE_ID;
use crate::UNUSED_IMPORT_RULE_ID;
use crate::UNUSED_INPUT_RULE_ID;
use crate::types::CallKind;
use crate::types::CallType;
use crate::types::Type;
use crate::types::display_types;
use crate::types::v1::ComparisonOperator;
use crate::types::v1::NumericOperator;

/// Utility type to represent an input or an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Io {
    /// The I/O is an input.
    Input,
    /// The I/O is an output.
    Output,
}

impl fmt::Display for Io {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
        }
    }
}

/// Represents the context for diagnostic reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// The name is a workflow name.
    Workflow(Span),
    /// The name is a task name.
    Task(Span),
    /// The name is a struct name.
    Struct(Span),
    /// The name is a struct member name.
    StructMember(Span),
    /// A name from a scope.
    Name(NameContext),
}

impl Context {
    /// Gets the span of the name.
    fn span(&self) -> Span {
        match self {
            Self::Workflow(s) => *s,
            Self::Task(s) => *s,
            Self::Struct(s) => *s,
            Self::StructMember(s) => *s,
            Self::Name(n) => n.span(),
        }
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workflow(_) => write!(f, "workflow"),
            Self::Task(_) => write!(f, "task"),
            Self::Struct(_) => write!(f, "struct"),
            Self::StructMember(_) => write!(f, "struct member"),
            Self::Name(n) => n.fmt(f),
        }
    }
}

/// Represents the context of a name in a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameContext {
    /// The name was introduced by an task or workflow input.
    Input(Span),
    /// The name was introduced by an task or workflow output.
    Output(Span),
    /// The name was introduced by a private declaration.
    Decl(Span),
    /// The name was introduced by a workflow call statement.
    Call(Span),
    /// The name was introduced by a variable in workflow scatter statement.
    ScatterVariable(Span),
}

impl NameContext {
    /// Gets the span of the name.
    pub fn span(&self) -> Span {
        match self {
            Self::Input(s) => *s,
            Self::Output(s) => *s,
            Self::Decl(s) => *s,
            Self::Call(s) => *s,
            Self::ScatterVariable(s) => *s,
        }
    }
}

impl fmt::Display for NameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(_) => write!(f, "input"),
            Self::Output(_) => write!(f, "output"),
            Self::Decl(_) => write!(f, "declaration"),
            Self::Call(_) => write!(f, "call"),
            Self::ScatterVariable(_) => write!(f, "scatter variable"),
        }
    }
}

impl From<NameContext> for Context {
    fn from(context: NameContext) -> Self {
        Self::Name(context)
    }
}

/// Creates a "name conflict" diagnostic.
pub fn name_conflict(name: &str, conflicting: Context, first: Context) -> Diagnostic {
    Diagnostic::error(format!("conflicting {conflicting} name `{name}`"))
        .with_label(
            format!("this {conflicting} conflicts with a previously used name"),
            conflicting.span(),
        )
        .with_label(
            format!("the {first} with the conflicting name is here"),
            first.span(),
        )
}

/// Constructs a "cannot index" diagnostic.
pub fn cannot_index(actual: &Type, span: Span) -> Diagnostic {
    Diagnostic::error("indexing is only allowed on `Array` and `Map` types")
        .with_label(format!("this is type `{actual}`"), span)
}

/// Creates an "unknown name" diagnostic.
pub fn unknown_name(name: &str, span: Span) -> Diagnostic {
    // Handle special case names here
    let message = match name {
        "task" => "the `task` variable may only be used within a task command section or task \
                   output section using WDL 1.2 or later"
            .to_string(),
        _ => format!("unknown name `{name}`"),
    };

    Diagnostic::error(message).with_highlight(span)
}

/// Creates a "self-referential" diagnostic.
pub fn self_referential(name: &str, span: Span, reference: Span) -> Diagnostic {
    Diagnostic::error(format!("declaration of `{name}` is self-referential"))
        .with_label("self-reference is here", reference)
        .with_highlight(span)
}

/// Creates a "task reference cycle" diagnostic.
pub fn task_reference_cycle(
    from: &impl fmt::Display,
    from_span: Span,
    to: &str,
    to_span: Span,
) -> Diagnostic {
    Diagnostic::error("a name reference cycle was detected")
        .with_label(
            format!("ensure this expression does not directly or indirectly refer to {from}"),
            to_span,
        )
        .with_label(format!("a reference back to `{to}` is here"), from_span)
}

/// Creates a "workflow reference cycle" diagnostic.
pub fn workflow_reference_cycle(
    from: &impl fmt::Display,
    from_span: Span,
    to: &str,
    to_span: Span,
) -> Diagnostic {
    Diagnostic::error("a name reference cycle was detected")
        .with_label(format!("this name depends on {from}"), to_span)
        .with_label(format!("a reference back to `{to}` is here"), from_span)
}

/// Creates a "call conflict" diagnostic.
pub fn call_conflict(name: &Ident, first: NameContext, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!(
        "conflicting call name `{name}`",
        name = name.as_str()
    ))
    .with_label(
        "this call name conflicts with a previously used name",
        name.span(),
    )
    .with_label(
        format!("the {first} with the conflicting name is here"),
        first.span(),
    );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the call to specify a different name")
    } else {
        diagnostic
    }
}

/// Creates a "namespace conflict" diagnostic.
pub fn namespace_conflict(
    name: &str,
    conflicting: Span,
    first: Span,
    suggest_fix: bool,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting import namespace `{name}`"))
        .with_label("this conflicts with another import namespace", conflicting)
        .with_label(
            "the conflicting import namespace was introduced here",
            first,
        );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the import to specify a namespace")
    } else {
        diagnostic
    }
}

/// Creates an "unknown namespace" diagnostic.
pub fn unknown_namespace(ns: &Ident) -> Diagnostic {
    Diagnostic::error(format!("unknown namespace `{ns}`", ns = ns.as_str()))
        .with_highlight(ns.span())
}

/// Creates an "only one namespace" diagnostic.
pub fn only_one_namespace(span: Span) -> Diagnostic {
    Diagnostic::error("only one namespace may be specified in a call statement")
        .with_highlight(span)
}

/// Creates an "import cycle" diagnostic.
pub fn import_cycle(span: Span) -> Diagnostic {
    Diagnostic::error("import introduces a dependency cycle")
        .with_label("this import has been skipped to break the cycle", span)
}

/// Creates an "import failure" diagnostic.
pub fn import_failure(uri: &str, error: &anyhow::Error, span: Span) -> Diagnostic {
    Diagnostic::error(format!("failed to import `{uri}`: {error:?}")).with_highlight(span)
}

/// Creates an "incompatible import" diagnostic.
pub fn incompatible_import(
    import_version: &str,
    import_span: Span,
    importer_version: &Version,
) -> Diagnostic {
    Diagnostic::error("imported document has incompatible version")
        .with_label(
            format!("the imported document is version `{import_version}`"),
            import_span,
        )
        .with_label(
            format!(
                "the importing document is version `{version}`",
                version = importer_version.as_str()
            ),
            importer_version.span(),
        )
}

/// Creates an "import missing version" diagnostic.
pub fn import_missing_version(span: Span) -> Diagnostic {
    Diagnostic::error("imported document is missing a version statement").with_highlight(span)
}

/// Creates an "invalid relative import" diagnostic.
pub fn invalid_relative_import(error: &url::ParseError, span: Span) -> Diagnostic {
    Diagnostic::error(format!("{error:?}")).with_highlight(span)
}

/// Creates a "struct not in document" diagnostic.
pub fn struct_not_in_document(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "a struct named `{name}` does not exist in the imported document",
        name = name.as_str()
    ))
    .with_label("this struct does not exist", name.span())
}

/// Creates an "imported struct conflict" diagnostic.
pub fn imported_struct_conflict(
    name: &str,
    conflicting: Span,
    first: Span,
    suggest_fix: bool,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label(
            "this import introduces a conflicting definition",
            conflicting,
        )
        .with_label("the first definition was introduced by this import", first);

    if suggest_fix {
        diagnostic.with_fix("add an `alias` clause to the import to specify a different name")
    } else {
        diagnostic
    }
}

/// Creates a "struct conflicts with import" diagnostic.
pub fn struct_conflicts_with_import(name: &str, conflicting: Span, import: Span) -> Diagnostic {
    Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label("this name conflicts with an imported struct", conflicting)
        .with_label("the import that introduced the struct is here", import)
        .with_fix(
            "either rename the struct or use an `alias` clause on the import with a different name",
        )
}

/// Creates a "duplicate workflow" diagnostic.
pub fn duplicate_workflow(name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "cannot define workflow `{name}` as only one workflow is allowed per source file",
        name = name.as_str(),
    ))
    .with_label("consider moving this workflow to a new file", name.span())
    .with_label("first workflow is defined here", first)
}

/// Creates a "recursive struct" diagnostic.
pub fn recursive_struct(name: &str, span: Span, member: Span) -> Diagnostic {
    Diagnostic::error(format!("struct `{name}` has a recursive definition"))
        .with_highlight(span)
        .with_label("this struct member participates in the recursion", member)
}

/// Creates an "unknown type" diagnostic.
pub fn unknown_type(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unknown type name `{name}`")).with_highlight(span)
}

/// Creates a "type mismatch" diagnostic.
pub fn type_mismatch(
    expected: &Type,
    expected_span: Span,
    actual: &Type,
    actual_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected type `{expected}`, but found type `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
    .with_label(format!("this expects type `{expected}`"), expected_span)
}

/// Creates a "non-empty array assignment" diagnostic.
pub fn non_empty_array_assignment(expected_span: Span, actual_span: Span) -> Diagnostic {
    Diagnostic::error("cannot assign an empty array to a non-empty array type")
        .with_label("this is an empty array", actual_span)
        .with_label("this expects a non-empty array", expected_span)
}

/// Creates a "call input type mismatch" diagnostic.
pub fn call_input_type_mismatch(name: &Ident, expected: &Type, actual: &Type) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected type `{expected}`, but found type `{actual}`",
    ))
    .with_label(
        format!(
            "input `{name}` is type `{expected}`, but name `{name}` is type `{actual}`",
            name = name.as_str(),
        ),
        name.span(),
    )
}

/// Creates a "no common type" diagnostic.
pub fn no_common_type(
    expected: &Type,
    expected_span: Span,
    actual: &Type,
    actual_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: a type common to both type `{expected}` and type `{actual}` does not exist"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
    .with_label(format!("this is type `{expected}`"), expected_span)
}

/// Creates a "multiple type mismatch" diagnostic.
pub fn multiple_type_mismatch(
    expected: &[Type],
    expected_span: Span,
    actual: &Type,
    actual_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected {expected}, but found type `{actual}`",
        expected = display_types(expected),
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
    .with_label(
        format!(
            "this expects {expected}",
            expected = display_types(expected)
        ),
        expected_span,
    )
}

/// Creates a "not a task member" diagnostic.
pub fn not_a_task_member(member: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "the `task` variable does not have a member named `{member}`",
        member = member.as_str()
    ))
    .with_highlight(member.span())
}

/// Creates a "not a struct" diagnostic.
pub fn not_a_struct(member: &Ident, input: bool) -> Diagnostic {
    Diagnostic::error(format!(
        "{kind} `{member}` is not a struct",
        kind = if input { "input" } else { "struct member" },
        member = member.as_str()
    ))
    .with_highlight(member.span())
}

/// Creates a "not a struct member" diagnostic.
pub fn not_a_struct_member(name: &str, member: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "struct `{name}` does not have a member named `{member}`",
        member = member.as_str()
    ))
    .with_highlight(member.span())
}

/// Creates a "not a pair accessor" diagnostic.
pub fn not_a_pair_accessor(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "cannot access a pair with name `{name}`",
        name = name.as_str()
    ))
    .with_highlight(name.span())
    .with_fix("use `left` or `right` to access a pair")
}

/// Creates a "missing struct members" diagnostic.
pub fn missing_struct_members(name: &Ident, count: usize, members: &str) -> Diagnostic {
    Diagnostic::error(format!(
        "struct `{name}` requires a value for member{s} {members}",
        name = name.as_str(),
        s = if count > 1 { "s" } else { "" },
    ))
    .with_highlight(name.span())
}

/// Creates a "map key not primitive" diagnostic.
pub fn map_key_not_primitive(span: Span, actual: &Type) -> Diagnostic {
    Diagnostic::error("expected map literal to use primitive type keys")
        .with_highlight(span)
        .with_label(format!("this is type `{actual}`"), span)
}

/// Creates a "if conditional mismatch" diagnostic.
pub fn if_conditional_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected `if` conditional expression to be type `Boolean`, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates a "logical not mismatch" diagnostic.
pub fn logical_not_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected `logical not` operand to be type `Boolean`, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates a "negation mismatch" diagnostic.
pub fn negation_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected negation operand to be type `Int` or `Float`, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates a "logical or mismatch" diagnostic.
pub fn logical_or_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected `logical or` operand to be type `Boolean`, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates a "logical and mismatch" diagnostic.
pub fn logical_and_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected `logical and` operand to be type `Boolean`, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates a "comparison mismatch" diagnostic.
pub fn comparison_mismatch(
    op: ComparisonOperator,
    span: Span,
    lhs: &Type,
    lhs_span: Span,
    rhs: &Type,
    rhs_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: operator `{op}` cannot compare type `{lhs}` to type `{rhs}`"
    ))
    .with_highlight(span)
    .with_label(format!("this is type `{lhs}`"), lhs_span)
    .with_label(format!("this is type `{rhs}`"), rhs_span)
}

/// Creates a "numeric mismatch" diagnostic.
pub fn numeric_mismatch(
    op: NumericOperator,
    span: Span,
    lhs: &Type,
    lhs_span: Span,
    rhs: &Type,
    rhs_span: Span,
) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: {op} operator is not supported for type `{lhs}` and type `{rhs}`"
    ))
    .with_highlight(span)
    .with_label(format!("this is type `{lhs}`"), lhs_span)
    .with_label(format!("this is type `{rhs}`"), rhs_span)
}

/// Creates a "string concat mismatch" diagnostic.
pub fn string_concat_mismatch(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: string concatenation is not supported for type `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Creates an "unknown function" diagnostic.
pub fn unknown_function(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unknown function `{name}`")).with_label(
        "the WDL standard library does not have a function with this name",
        span,
    )
}

/// Creates an "unsupported function" diagnostic.
pub fn unsupported_function(minimum: SupportedVersion, name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "this use of function `{name}` requires a minimum WDL version of {minimum}"
    ))
    .with_highlight(span)
}

/// Creates a "too few arguments" diagnostic.
pub fn too_few_arguments(name: &str, span: Span, minimum: usize, count: usize) -> Diagnostic {
    Diagnostic::error(format!(
        "function `{name}` requires at least {minimum} argument{s} but {count} {v} supplied",
        s = if minimum == 1 { "" } else { "s" },
        v = if count == 1 { "was" } else { "were" },
    ))
    .with_highlight(span)
}

/// Creates a "too many arguments" diagnostic.
pub fn too_many_arguments(
    name: &str,
    span: Span,
    maximum: usize,
    count: usize,
    excessive: impl Iterator<Item = Span>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(format!(
        "function `{name}` requires no more than {maximum} argument{s} but {count} {v} supplied",
        s = if maximum == 1 { "" } else { "s" },
        v = if count == 1 { "was" } else { "were" },
    ))
    .with_highlight(span);

    for span in excessive {
        diagnostic = diagnostic.with_label("this argument is unexpected", span);
    }

    diagnostic
}

/// Constructs an "argument type mismatch" diagnostic.
pub fn argument_type_mismatch(name: &str, expected: &str, actual: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: argument to function `{name}` expects type {expected}, but found type \
         `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), span)
}

/// Constructs an "ambiguous argument" diagnostic.
pub fn ambiguous_argument(name: &str, span: Span, first: &str, second: &str) -> Diagnostic {
    Diagnostic::error(format!(
        "ambiguous call to function `{name}` with conflicting signatures `{first}` and `{second}`",
    ))
    .with_highlight(span)
}

/// Constructs an "index type mismatch" diagnostic.
pub fn index_type_mismatch(expected: &Type, actual: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected index to be type `{expected}`, but found type `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), span)
}

/// Constructs an "type is not array" diagnostic.
pub fn type_is_not_array(actual: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "type mismatch: expected an array type, but found type `{actual}`"
    ))
    .with_label(format!("this is type `{actual}`"), span)
}

/// Constructs a "cannot access" diagnostic.
pub fn cannot_access(actual: &Type, actual_span: Span) -> Diagnostic {
    Diagnostic::error(format!("cannot access type `{actual}`"))
        .with_label(format!("this is type `{actual}`"), actual_span)
}

/// Constructs a "cannot coerce to string" diagnostic.
pub fn cannot_coerce_to_string(actual: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(format!("cannot coerce type `{actual}` to `String`"))
        .with_label(format!("this is type `{actual}`"), span)
}

/// Creates an "unknown task or workflow" diagnostic.
pub fn unknown_task_or_workflow(namespace: Option<Span>, name: &str, span: Span) -> Diagnostic {
    let mut diagnostic =
        Diagnostic::error(format!("unknown task or workflow `{name}`")).with_highlight(span);

    if let Some(namespace) = namespace {
        diagnostic = diagnostic.with_label(
            format!("this namespace does not have a task or workflow named `{name}`"),
            namespace,
        );
    }

    diagnostic
}

/// Creates an "unknown call input/output" diagnostic.
pub fn unknown_call_io(call: &CallType, name: &Ident, io: Io) -> Diagnostic {
    Diagnostic::error(format!(
        "{kind} `{call}` does not have an {io} named `{name}`",
        kind = call.kind(),
        call = call.name(),
        name = name.as_str(),
    ))
    .with_highlight(name.span())
}

/// Creates an "unknown task input/output name" diagnostic.
pub fn unknown_task_io(task_name: &str, name: &Ident, io: Io) -> Diagnostic {
    Diagnostic::error(format!(
        "task `{task_name}` does not have an {io} named `{name}`",
        name = name.as_str(),
    ))
    .with_highlight(name.span())
}

/// Creates a "recursive workflow call" diagnostic.
pub fn recursive_workflow_call(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("cannot recursively call workflow `{name}`")).with_highlight(span)
}

/// Creates a "missing call input" diagnostic.
pub fn missing_call_input(kind: CallKind, target: &Ident, input: &str) -> Diagnostic {
    Diagnostic::error(format!(
        "missing required call input `{input}` for {kind} `{target}`",
        target = target.as_str(),
    ))
    .with_highlight(target.span())
}

/// Creates an "unused import" diagnostic.
pub fn unused_import(name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("unused import namespace `{name}`"))
        .with_rule(UNUSED_IMPORT_RULE_ID)
        .with_highlight(span)
}

/// Creates an "unused input" diagnostic.
pub fn unused_input(name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("unused input `{name}`"))
        .with_rule(UNUSED_INPUT_RULE_ID)
        .with_highlight(span)
}

/// Creates an "unused declaration" diagnostic.
pub fn unused_declaration(name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("unused declaration `{name}`"))
        .with_rule(UNUSED_DECL_RULE_ID)
        .with_highlight(span)
}

/// Creates an "unused call" diagnostic.
pub fn unused_call(name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("unused call `{name}`"))
        .with_rule(UNUSED_CALL_RULE_ID)
        .with_highlight(span)
}

/// Creates an "unnecessary function call" diagnostic.
pub fn unnecessary_function_call(
    name: &str,
    span: Span,
    label: &str,
    label_span: Span,
) -> Diagnostic {
    Diagnostic::warning(format!("unnecessary call to function `{name}`"))
        .with_rule(UNNECESSARY_FUNCTION_CALL)
        .with_highlight(span)
        .with_label(label.to_string(), label_span)
}
