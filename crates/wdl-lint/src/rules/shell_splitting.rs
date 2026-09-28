//! A lint rule for placeholders that are subject to shell word splitting.

pub(crate) mod scanner;

use scanner::ShellState;
use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::ExprTypeEvaluator;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::LiteralString;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOption;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::StrippedCommandPart;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::CommandContext;

/// The identifier for the shell splitting rule.
const ID: &str = "ShellSplitting";

/// The maximum number of shell states tracked at once.
const MAX_STATES: usize = 32;

/// The maximum number of array elements to simulate when joining an array.
const MAX_ELEMENTS: usize = 4;

/// The possible shell states at a position in a command.
type States = Vec<ShellState>;

/// Combines two sets of shell states.
fn union(mut states: States, other: States) -> States {
    for state in other {
        if states.len() >= MAX_STATES {
            break;
        }

        if !states.contains(&state) {
            states.push(state);
        }
    }

    states
}

/// Determines whether values of a type may contain whitespace or glob
/// characters.
fn is_splittable(ty: &Type) -> bool {
    matches!(
        ty.as_primitive(),
        Some(PrimitiveType::String | PrimitiveType::File | PrimitiveType::Directory)
    )
}

/// Gets the name of an expression if it is a name reference.
fn name_of(expr: &Expr) -> Option<String> {
    match expr {
        Expr::NameRef(name) => Some(name.name().text().to_string()),
        _ => None,
    }
}

/// The kind of unquoted value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProblemKind {
    /// An entire placeholder is unquoted.
    Placeholder,
    /// A value within a placeholder expression is unquoted.
    Value,
    /// The elements of an array are unquoted.
    Array,
}

/// An unquoted value that is subject to word splitting.
#[derive(Clone, Debug)]
struct Problem {
    /// The span to highlight.
    span: Span,
    /// The kind of unquoted value.
    kind: ProblemKind,
    /// The type of the value.
    ty: Type,
    /// The name of the value, if it is a name reference.
    name: Option<String>,
}

/// Creates a diagnostic for an unquoted value.
fn unquoted_value(problem: &Problem) -> Diagnostic {
    let ty = &problem.ty;
    let (message, fix) = match (problem.kind, &problem.name) {
        (ProblemKind::Placeholder, _) => (
            format!("unquoted `{ty}` placeholder is subject to word splitting"),
            String::from("surround the placeholder with double quotes"),
        ),
        (ProblemKind::Value, Some(name)) => (
            format!("unquoted `{ty}` value is subject to word splitting"),
            format!(
                "surround the value with quotes inside the placeholder expression (e.g., \
                 `\"'~{{{name}}}'\"`)"
            ),
        ),
        (ProblemKind::Value, None) => (
            format!("unquoted `{ty}` value is subject to word splitting"),
            String::from("surround the value with quotes inside the placeholder expression"),
        ),
        (ProblemKind::Array, Some(name)) => (
            format!("unquoted `{ty}` elements are subject to word splitting"),
            format!("quote each element with `squote({name})` or `quote({name})`"),
        ),
        (ProblemKind::Array, None) => (
            format!("unquoted `{ty}` elements are subject to word splitting"),
            String::from("quote each element with `squote()` or `quote()`"),
        ),
    };

    Diagnostic::warning(message)
        .with_rule(ID)
        .with_highlight(problem.span)
        .with_fix(fix)
}

/// The separator used to join array elements.
enum Separator {
    /// A separator given by a `sep` function argument.
    Expr(Expr),
    /// A separator given by a `sep` placeholder option.
    String(LiteralString),
}

/// Analyzes a command section for unquoted values.
struct Analyzer<'a, 'c> {
    /// The evaluator used to determine the types of expressions.
    evaluator: ExprTypeEvaluator<'a, CommandContext<'c>>,
    /// The unquoted values found so far.
    problems: Vec<Problem>,
}

impl Analyzer<'_, '_> {
    /// Feeds literal text to every state.
    fn feed(&self, states: States, text: &str) -> States {
        states
            .into_iter()
            .map(|mut state| {
                state.feed(text);
                state
            })
            .fold(Vec::new(), |states, state| union(states, vec![state]))
    }

    /// Inserts a value into every state.
    ///
    /// If the value is split in any state, the problem is recorded.
    fn insert(&mut self, states: States, problem: Option<Problem>) -> States {
        let mut splits = false;
        let states = states
            .into_iter()
            .map(|mut state| {
                splits |= state.insert();
                state
            })
            .fold(Vec::new(), |states, state| union(states, vec![state]));

        if splits
            && let Some(problem) = problem
            && !self.problems.iter().any(|p| p.span == problem.span)
        {
            self.problems.push(problem);
        }

        states
    }

    /// Analyzes a command section.
    fn command(&mut self, section: &CommandSection) {
        // The script is analyzed with its common indentation removed so that
        // heredoc delimiters are matched as the shell sees them.
        let parts = section.strip_whitespace().unwrap_or_else(|| {
            section
                .parts()
                .map(|part| match part {
                    CommandPart::Text(text) => {
                        let mut buffer = String::new();
                        text.unescape_to(section.is_heredoc(), &mut buffer);
                        StrippedCommandPart::Text(buffer)
                    }
                    CommandPart::Placeholder(placeholder) => {
                        StrippedCommandPart::Placeholder(placeholder)
                    }
                })
                .collect()
        });

        let mut states = vec![ShellState::default()];
        for part in parts {
            states = match part {
                StrippedCommandPart::Text(text) => self.feed(states, &text),
                StrippedCommandPart::Placeholder(placeholder) => {
                    self.placeholder(&placeholder, states, true)
                }
            };
        }
    }

    /// Analyzes a placeholder.
    ///
    /// A top-level placeholder is one that appears directly in the command
    /// text rather than inside a string.
    fn placeholder(
        &mut self,
        placeholder: &Placeholder,
        states: States,
        top_level: bool,
    ) -> States {
        let expr = placeholder.expr();
        let root = top_level.then(|| placeholder.span());
        match placeholder.option() {
            Some(PlaceholderOption::Sep(sep)) => {
                self.sequence(&expr, states, &Separator::String(sep.separator()))
            }
            Some(PlaceholderOption::TrueFalse(options)) => {
                let (true_value, false_value) = options.values();
                let true_states = self.string(&true_value, states.clone());
                let false_states = self.string(&false_value, states);
                union(true_states, false_states)
            }
            Some(PlaceholderOption::Default(default)) => {
                let value_states = self.expr(&expr, states.clone(), root);
                let default_states = self.string(&default.value(), states);
                union(value_states, default_states)
            }
            None => self.expr(&expr, states, root),
        }
    }

    /// Analyzes a string literal.
    fn string(&mut self, string: &LiteralString, mut states: States) -> States {
        for part in string.parts() {
            states = match part {
                StringPart::Text(text) => {
                    let mut buffer = String::new();
                    text.unescape_to(&mut buffer);
                    self.feed(states, &buffer)
                }
                StringPart::Placeholder(placeholder) => {
                    self.placeholder(&placeholder, states, false)
                }
            };
        }

        states
    }

    /// Analyzes an expression that is rendered as a single value.
    ///
    /// `root` is the span of the enclosing top-level placeholder if the
    /// expression is the entire placeholder expression.
    fn expr(&mut self, expr: &Expr, states: States, root: Option<Span>) -> States {
        match expr {
            Expr::Literal(LiteralExpr::String(string)) => self.string(string, states),
            Expr::Literal(_) => self.insert(states, None),
            Expr::Parenthesized(expr) => self.expr(&expr.expr(), states, root),
            Expr::If(expr) => {
                // The condition is never rendered, so only the branches are
                // analyzed.
                let (_, true_expr, false_expr) = expr.exprs();
                let true_states = self.expr(&true_expr, states.clone(), None);
                let false_states = self.expr(&false_expr, states, None);
                union(true_states, false_states)
            }
            Expr::Addition(addition) if !self.is_numeric(expr) => {
                let (left, right) = addition.operands();
                let states = self.expr(&left, states, None);
                self.expr(&right, states, None)
            }
            Expr::Call(call) if call.target().text() == "sep" => {
                let mut arguments = call.arguments();
                match (arguments.next(), arguments.next()) {
                    (Some(separator), Some(array)) => {
                        self.sequence(&array, states, &Separator::Expr(separator))
                    }
                    _ => self.insert(states, None),
                }
            }
            _ => self.value(expr, states, root),
        }
    }

    /// Determines whether an expression evaluates to a number.
    fn is_numeric(&mut self, expr: &Expr) -> bool {
        self.evaluator.evaluate_expr(expr).is_some_and(|ty| {
            matches!(
                ty.as_primitive(),
                Some(PrimitiveType::Integer | PrimitiveType::Float)
            )
        })
    }

    /// Analyzes an expression whose value is not known until runtime.
    fn value(&mut self, expr: &Expr, states: States, root: Option<Span>) -> States {
        let problem = self
            .evaluator
            .evaluate_expr(expr)
            .filter(is_splittable)
            .map(|ty| match root {
                Some(span) => Problem {
                    span,
                    kind: ProblemKind::Placeholder,
                    ty,
                    name: None,
                },
                None => Problem {
                    span: expr.span(),
                    kind: ProblemKind::Value,
                    ty,
                    name: name_of(expr),
                },
            });

        self.insert(states, problem)
    }

    /// Analyzes the elements of an array joined by a separator.
    ///
    /// Because the number of elements is not known, the states after zero or
    /// more elements are all possible.
    fn sequence(&mut self, array: &Expr, states: States, separator: &Separator) -> States {
        if let Expr::Parenthesized(expr) = array {
            return self.sequence(&expr.expr(), states, separator);
        }

        // The elements of an array literal are rendered in order.
        if let Expr::Literal(LiteralExpr::Array(literal)) = array {
            let mut states = states;
            for (index, element) in literal.elements().enumerate() {
                if index > 0 {
                    states = self.separator(separator, states);
                }

                states = self.expr(&element, states, None);
            }

            return states;
        }

        let mut result = states.clone();
        let mut current = self.element(array, states);
        for _ in 0..MAX_ELEMENTS {
            let len = result.len();
            result = union(result, current.clone());
            if result.len() == len {
                break;
            }

            let separated = self.separator(separator, current);
            current = self.element(array, separated);
        }

        result
    }

    /// Analyzes a separator between array elements.
    fn separator(&mut self, separator: &Separator, states: States) -> States {
        match separator {
            Separator::Expr(expr) => self.expr(expr, states, None),
            Separator::String(string) => self.string(string, states),
        }
    }

    /// Analyzes a single element of an array.
    fn element(&mut self, array: &Expr, states: States) -> States {
        match array {
            Expr::Parenthesized(expr) => self.element(&expr.expr(), states),
            Expr::If(expr) => {
                let (_, true_expr, false_expr) = expr.exprs();
                let true_states = self.element(&true_expr, states.clone());
                let false_states = self.element(&false_expr, states);
                union(true_states, false_states)
            }
            Expr::Literal(LiteralExpr::Array(literal)) => {
                let mut elements = literal.elements().peekable();
                if elements.peek().is_none() {
                    return states;
                }

                elements.fold(Vec::new(), |result, element| {
                    let element_states = self.expr(&element, states.clone(), None);
                    union(result, element_states)
                })
            }
            Expr::Call(call) => {
                let target = call.target();
                let mut arguments = call.arguments();
                match target.text() {
                    "quote" | "squote" => {
                        let quote = if target.text() == "quote" { "\"" } else { "'" };
                        let states = self.feed(states, quote);
                        let states = self.insert(states, None);
                        self.feed(states, quote)
                    }
                    "prefix" => match (arguments.next(), arguments.next()) {
                        (Some(prefix), Some(array)) => {
                            let states = self.expr(&prefix, states, None);
                            self.element(&array, states)
                        }
                        _ => self.insert(states, None),
                    },
                    "suffix" => match (arguments.next(), arguments.next()) {
                        (Some(suffix), Some(array)) => {
                            let states = self.element(&array, states);
                            self.expr(&suffix, states, None)
                        }
                        _ => self.insert(states, None),
                    },
                    _ => self.array_value(array, states),
                }
            }
            _ => self.array_value(array, states),
        }
    }

    /// Analyzes an array expression whose elements are not known until
    /// runtime.
    fn array_value(&mut self, array: &Expr, states: States) -> States {
        let problem = self
            .evaluator
            .evaluate_expr(array)
            .filter(|ty| {
                ty.as_array()
                    .is_some_and(|a| is_splittable(a.element_type()))
            })
            .map(|ty| Problem {
                span: array.span(),
                kind: ProblemKind::Array,
                ty,
                name: name_of(array),
            });

        self.insert(states, problem)
    }
}

/// Detects placeholders that are subject to shell word splitting.
#[derive(Default, Debug, Clone)]
pub struct ShellSplittingRule {
    /// The document being linted.
    document: Option<Document>,
}

impl Rule for ShellSplittingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that `String`, `File`, and `Directory` values in command sections are quoted."
    }

    fn explanation(&self) -> &'static str {
        "When a placeholder in a command section is not quoted, the shell splits its value into \
         separate words at whitespace and expands any glob characters. A file path or string that \
         contains a space or a `*` then becomes several arguments or matches unintended files. \
         This rule reports each `String`, `File`, or `Directory` value that is inserted into an \
         unquoted part of the script, including values in individual branches of `if` expressions \
         and elements of arrays joined with `sep`. Values inside shell quotes, assignments, `[[ \
         ]]` tests, `case` words, arithmetic, heredoc bodies, and comments are not split and are \
         not reported. Numbers and booleans are never reported. ShellCheck's SC2086, SC2206, and \
         SC2231 diagnostics are not reported for placeholders, because this rule covers them."
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                negative: LabeledSnippet {
                    label: None,
                    snippet: r#"version 1.2

task count_lines {
    input {
        File reads
    }

    command <<<
        wc -l ~{reads}
    >>>
}
"#,
                },
                revised: Some(LabeledSnippet {
                    label: Some("Surround the placeholder with double quotes"),
                    snippet: r#"version 1.2

task count_lines {
    input {
        File reads
    }

    command <<<
        wc -l "~{reads}"
    >>>
}
"#,
                }),
            },
            Example {
                negative: LabeledSnippet {
                    label: None,
                    snippet: r#"version 1.2

task view {
    input {
        File bam
        String? region
    }

    command <<<
        samtools view "~{bam}" ~{if defined(region) then "--region ~{region}" else ""}
    >>>
}
"#,
                },
                revised: Some(LabeledSnippet {
                    label: Some("Quote the value inside the placeholder expression"),
                    snippet: r#"version 1.2

task view {
    input {
        File bam
        String? region
    }

    command <<<
        samtools view "~{bam}" ~{if defined(region) then "--region '~{region}'" else ""}
    >>>
}
"#,
                }),
            },
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::CommandSectionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &["ShellCheck"]
    }
}

impl Visitor for ShellSplittingRule {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        document: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.document = Some(document.clone());
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let Some(document) = self.document.clone() else {
            return;
        };

        let Some(scope) =
            document.find_scope_by_position(section.inner().text_range().start().into())
        else {
            // The command section has not been analyzed, e.g. because it is
            // in a duplicate task.
            return;
        };

        let mut context = CommandContext::new(document.clone(), scope);
        let mut analyzer = Analyzer {
            evaluator: ExprTypeEvaluator::new(&mut context),
            problems: Vec::new(),
        };
        analyzer.command(section);

        let mut problems = analyzer.problems;
        problems.sort_by_key(|p| p.span.start());
        for problem in problems {
            diagnostics.exceptable_add(
                unquoted_value(&problem),
                section.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}
