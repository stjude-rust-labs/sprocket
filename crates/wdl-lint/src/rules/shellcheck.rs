//! A lint rule for running shellcheck against command sections.
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::process;
use std::process::Stdio;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ftree::FenwickTree;
use rand::distr::Alphanumeric;
use rand::distr::SampleString;
use rowan::ast::support;
use serde::Deserialize;
use serde_json;
use tracing::debug;
use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_analysis::document::ScopeRef;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::EvaluationContext;
use wdl_analysis::types::v1::ExprTypeEvaluator;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::StrippedCommandPart;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::fix::Fixer;
use crate::fix::InsertionPoint;
use crate::fix::Replacement;
use crate::util::is_quote_balanced;
use crate::util::lines_with_offset;
use crate::util::program_exists;

/// The shellcheck executable
const SHELLCHECK_BIN: &str = "shellcheck";

// TODO 2043, 2050, 2157 should be enabled and only suppressed
// when it's a placeholder substitution.
/// Shellcheck lints that we want to suppress.
const SHELLCHECK_SUPPRESS: &[&str] = &[
    "1009", // the mentioned parser error was in... (unhelpful commentary)
    "1072", // Unexpected eof (unhelpful commentary)
    "2043", // This loop will only ever run once for a constant value (caused by substitution)
    "2050", // This expression is constant (caused by substitution)
    "2157", // Argument to -n is always true due to literal strings (caused by substitution)
];

/// Shellcheck lints that we want to keep,
/// but ignore the fix suggestion.
const SHELLCHECK_IGNORE_FIX: &[&str] = &[
    "2086", /* Double quote to prevent globbing and word splitting (fix message includes our
            * substitution) */
];

/// ShellCheck: var is referenced but not assigned.
const SHELLCHECK_REFERENCED_UNASSIGNED: usize = 2154;

/// ShellCheck wiki base url.
const SHELLCHECK_WIKI: &str = "https://www.shellcheck.net/wiki";

/// Whether or not shellcheck exists on the system
static SHELLCHECK_EXISTS: OnceLock<bool> = OnceLock::new();

/// The identifier for the command section ShellCheck rule.
const ID: &str = "ShellCheck";

/// Suggested fix for a ShellCheck diagnostic.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckFix {
    /// The replacements to perform.
    pub replacements: Vec<ShellCheckReplacement>,
}

/// A ShellCheck replacement.
///
/// This differs from a [`Replacement`] in that
/// 1) columns are 1-indexed
/// 2) it may span multiple lines and thus cannot be directly passed to a
///    [`Fixer`].
///
/// It must be normalized with `normalize_replacements` before use.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckReplacement {
    /// Line number replacement occurs on.
    pub line: usize,
    /// Line number replacement ends on.
    #[serde(rename = "endLine")]
    pub end_line: usize,
    /// Order in which replacements should happen. Highest precedence first.
    pub precedence: usize,
    /// An `InsertionPoint`.
    #[serde(rename = "insertionPoint")]
    pub insertion_point: InsertionPoint,
    /// Column replacement occurs on.
    pub column: usize,
    /// Column replacements ends on.
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// Replacement text.
    #[serde(rename = "replacement")]
    pub value: String,
}

/// A ShellCheck diagnostic.
///
/// The `file` field is omitted as we have no use for it.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckDiagnostic {
    /// Line number comment starts on.
    pub line: usize,
    /// Line number comment ends on.
    #[serde(rename = "endLine")]
    pub end_line: usize,
    /// Column comment starts on.
    pub column: usize,
    /// Column comment ends on.
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// Severity of the comment.
    pub level: String,
    /// ShellCheck error code.
    pub code: usize,
    /// Message associated with the comment.
    pub message: String,
    /// Optional fixes to apply.
    pub fix: Option<ShellCheckFix>,
}

/// Convert [`ShellCheckReplacement`]s into [`Replacement`]s.
///
/// Column indices are shifted to 0-based.
/// Multi-line replacements are normalized so that column indices are
/// as though the string is on a single line.
fn normalize_replacements(
    replacements: &[ShellCheckReplacement],
    shift_tree: &FenwickTree<usize>,
) -> Vec<Replacement> {
    replacements
        .iter()
        .map(|r| {
            Replacement::new(
                r.column + shift_tree.prefix_sum(r.line - 1, 0) - 1,
                r.end_column + shift_tree.prefix_sum(r.end_line - 1, 0) - 1,
                r.insertion_point,
                r.value.clone(),
                r.precedence,
            )
        })
        .collect()
}

/// Run shellcheck on a command.
///
/// writes command text to stdin of shellcheck process
/// and returns parsed `ShellCheckDiagnostic`s
fn run_shellcheck(command: &str) -> Result<Vec<ShellCheckDiagnostic>> {
    let mut sc_proc = process::Command::new(SHELLCHECK_BIN)
        .args([
            "-s", // bash shell
            "bash",
            "-f", // output JSON
            "json",
            "-e", // errors to suppress
            &SHELLCHECK_SUPPRESS.join(","),
            "-S", // set minimum lint level to style
            "style",
            "-", // input is piped to STDIN
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("spawning the `shellcheck` process")?;
    debug!("`shellcheck` process id: {}", sc_proc.id());
    {
        let mut proc_stdin = sc_proc
            .stdin
            .take()
            .context("obtaining the STDIN handle of the `shellcheck` process")?;
        proc_stdin.write_all(command.as_bytes())?;
    }

    let output = sc_proc
        .wait_with_output()
        .context("waiting for the `shellcheck` process to complete")?;

    // shellcheck returns exit code 1 if
    // any checked files result in comments
    // so cannot check with status.success()
    match output.status.code() {
        Some(0) | Some(1) => serde_json::from_slice::<Vec<ShellCheckDiagnostic>>(&output.stdout)
            .context("deserializing STDOUT from `shellcheck` process"),
        Some(code) => bail!("unexpected `shellcheck` exit code: {}", code),
        None => bail!("the `shellcheck` process appears to have been interrupted"),
    }
}

/// Runs ShellCheck on a command section and reports diagnostics.
#[derive(Default, Debug, Clone)]
pub struct ShellCheckRule {
    /// The document being linted.
    document: Option<Document>,
}

impl Rule for ShellCheckRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that command blocks are free of ShellCheck violations."
    }

    fn explanation(&self) -> &'static str {
        "ShellCheck (https://shellcheck.net) is a static analysis tool and linter for sh / bash. \
         The lints provided by ShellCheck help prevent common errors and pitfalls in your scripts. \
         Following its recommendations will increase the robustness of your command sections."
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

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

/// Create an appropriate 'fix' message.
///
/// Returns the following range of text:
/// start = min(diagnostic highlight start, left-most replacement start)
/// end = max(diagnostic highlight end, right-most replacement end)
/// start..end
fn create_fix_message(
    replacements: Vec<Replacement>,
    command_text: &str,
    diagnostic_span: Span,
) -> String {
    let mut fixer = Fixer::new(command_text.to_owned());
    // Get the original left-most and right-most replacement indices.
    let rep_start = replacements
        .iter()
        .map(|r| r.start())
        .min()
        .expect("replacements is non-empty");
    let rep_end = replacements
        .iter()
        .map(|r| r.end())
        .max()
        .expect("replacements is non-empty");
    let start = rep_start.min(diagnostic_span.start());
    let end = rep_end.max(diagnostic_span.end());
    fixer.apply_replacements(replacements);
    // Adjust start and end based on final tree.
    let adj_range = {
        let range = fixer.adjust_range(start..end);
        // the prefix sum does not include the value at
        // the actual index. But, we want this value because
        // we may have inserted text at the very end.
        // ftree provides no method to get this value, so
        // we must calculate it.
        let max_pos = (end + 1).min(fixer.value().len());
        let extend_by = (fixer.transform(max_pos) - fixer.transform(max_pos - 1)).saturating_sub(1);
        range.start..(range.end + extend_by)
    };
    format!("did you mean `{}`?", &fixer.value()[adj_range])
}

/// Creates a "ShellCheck lint" diagnostic from a [ShellCheckDiagnostic]
fn shellcheck_lint(
    diagnostic: &ShellCheckDiagnostic,
    command_text: &str,
    line_map: &HashMap<usize, Span>,
    shift_tree: &FenwickTree<usize>,
) -> Diagnostic {
    let label = format!(
        "SC{}[{}]: {}",
        diagnostic.code, diagnostic.level, diagnostic.message
    );
    // This span is relative to the entire document.
    let span = calculate_span(diagnostic, line_map);
    let fix_msg = match diagnostic.fix {
        Some(ref fix)
            if !SHELLCHECK_IGNORE_FIX
                .iter()
                .any(|code| code == &diagnostic.code.to_string()) =>
        {
            let reps = normalize_replacements(&fix.replacements, shift_tree);
            // This span is relative to the command text.
            let diagnostic_span = {
                let start = diagnostic.column + shift_tree.prefix_sum(diagnostic.line - 1, 0) - 1;
                let end =
                    diagnostic.end_column + shift_tree.prefix_sum(diagnostic.end_line - 1, 0) - 1;
                Span::new(start, end - start)
            };
            create_fix_message(reps, command_text, diagnostic_span)
        }
        Some(_) | None => String::from("address the diagnostic as recommended in the message"),
    };
    Diagnostic::note(&diagnostic.message)
        .with_rule(ID)
        .with_label(label, span)
        .with_label(
            format!("more info: {}/SC{}", &SHELLCHECK_WIKI, diagnostic.code),
            span,
        )
        .with_fix(fix_msg)
}

/// A context for evaluating expressions in a command section.
struct CommandContext<'a> {
    /// The document being linted.
    document: Document,
    /// The scope of the command section.
    scope: ScopeRef<'a>,
}

impl EvaluationContext for CommandContext<'_> {
    fn version(&self) -> SupportedVersion {
        self.document.version().expect("document has a version")
    }

    fn resolve_name(&self, name: &str, _span: Span) -> Option<wdl_analysis::types::Type> {
        self.scope.lookup(name).map(|n| n.ty().clone())
    }

    fn resolve_type_name(
        &mut self,
        name: &str,
        _span: Span,
    ) -> std::result::Result<wdl_analysis::types::Type, Diagnostic> {
        dbg!(&self.scope);
        match self.scope.lookup(name).map(|n| n.ty().clone()) {
            Some(ty) => Ok(ty),
            None => unreachable!("should have found type for name `{name}`")
        }
    }

    fn task(&self) -> Option<&wdl_analysis::document::Task> {
        None
    }

    fn diagnostics_config(&self) -> wdl_analysis::DiagnosticsConfig {
        wdl_analysis::DiagnosticsConfig::except_all()
    }

    fn add_diagnostic(&mut self, _diagnostic: Diagnostic) {
        // do nothing
    }
}

impl<'a> CommandContext<'a> {
    /// Create a new `CommandContext`.
    fn new(document: Document, scope: ScopeRef<'a>) -> Self {
        Self { document, scope }
    }
}

/// Detect embedded quotes surrounding an expression in a string.
///
/// This is a utility function called by `evaluates_to_bash_literal`. Only
/// `expr` that are addition or strings with potentially embedded placeholders
/// are valid input. For a given expression, it checks through all descendants
/// to see if there are any name references (variables) that are surrounded by
/// escaped quotes. In WDL, the parent expression is either an addition
/// (concatenation, e.g. `~{"foo " + bar + " baz"}`) operation or a string with
/// an embedded placeholder (e.g. `~{"foo ~{bar} baz"`). So the escaped quotes
/// are not in a single string literal. The descendant expressions must be
/// traversed to check for quoting.
fn is_quoted(expr: &Expr) -> bool {
    let mut opened = false;
    let mut name = false;

    let mut placeholders = Vec::new();
    for c in expr.descendants::<Expr>() {
        match c {
            Expr::Literal(LiteralExpr::String(ref s)) => {
                for p in s.parts() {
                    match p {
                        StringPart::Text(t) => {
                            let mut buffer = String::new();
                            t.unescape_to(&mut buffer);
                            buffer.match_indices(&['\'', '"']).for_each(|(..)| {
                                if opened && name {
                                    name = false;
                                }
                                opened = !opened;
                            });
                        }
                        StringPart::Placeholder(placeholder) => {
                            placeholders.push(placeholder.expr());
                            if !opened {
                                return false;
                            }
                            name = true;
                        }
                    }
                }
            }
            Expr::NameRef(_) => {
                if !placeholders.contains(&c) {
                    if !opened {
                        return false;
                    }
                    name = true;
                }
            }
            _ => {}
        }
    }
    !name
}

/// Evaluate an expression to determine if it can be simplified to a literal.
///
/// Many WDL expressions can be simplified to a bash literal. For example
/// concatenation of strings (e.g. `"foo" + "bar"`) is a WDL expression, but can
/// be represented as a string for shellcheck. This function checks for various
/// WDL functions and their arguments to evaluate if the WDL expression
/// ultimately evaluates to a literal in the bash script.
fn evaluates_to_bash_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralExpr::String(s)) => {
            if s.text().is_some() {
                return true;
            }
            is_quoted(expr)
        }
        Expr::Literal(_) => true,
        Expr::Call(c) => match c.target().text() {
            // `sep` concatenates its arguments with a separator.
            // `prefix` and `suffix` add a prefix or suffix to the argument.
            // So we check the array argument to see if it evaluates to a
            // bash literal.
            "sep" | "prefix" | "suffix" => evaluates_to_bash_literal(
                &c.arguments()
                    .nth(1)
                    .expect("`sep`/`prefix`/`suffix` call should have two arguments"),
            ),
            // `quote` and `squote` both return quoted strings, so they can be treated as bash
            // literals.
            "quote" | "squote" => true,
            _ => false,
        },
        Expr::Parenthesized(p) => evaluates_to_bash_literal(&p.expr()),
        Expr::If(i) => {
            let (_, if_expr, else_expr) = i.exprs();
            evaluates_to_bash_literal(&if_expr) && evaluates_to_bash_literal(&else_expr)
        }
        Expr::Addition(a) => {
            let balanced = is_quoted(expr);
            let (left, right) = a.operands();
            (evaluates_to_bash_literal(&left) && evaluates_to_bash_literal(&right)) || balanced
        }
        _ => false,
    }
}

/// Convert a WDL placeholder to a bash variable or literal.
///
/// The boolean returned indicates whether the placeholder was replaced with a
/// literal (true) or a bash variable (false).
/// If the placeholder is an integer, float, or boolean,
/// it is replaced with a literal value.
/// If it is a string, then the string is checked to see if it evaluates to a
/// literal. Otherwise, it is replaced with a bash variable.
fn to_bash_var(placeholder: &Placeholder, ty: Option<Type>) -> (String, bool) {
    let placeholder_len: usize = placeholder.inner().text_range().len().into();

    if let Some(Type::Primitive(pty, _)) = ty {
        match pty {
            PrimitiveType::Integer | PrimitiveType::Float => {
                return ("4".repeat(placeholder_len), true);
            }
            PrimitiveType::Boolean => {
                return (
                    format!("true{}", " ".repeat(placeholder_len.saturating_sub(4))),
                    true,
                );
            }
            PrimitiveType::String => {
                if evaluates_to_bash_literal(&placeholder.expr()) {
                    return ("a".repeat(placeholder_len), true);
                }
            }
            _ => {}
        }
    };

    // Don't start variable with numbers. This is lowercase to avoid triggering
    // Shellcheck's misspelling rule: https://www.shellcheck.net/wiki/SC2153
    let mut bash_var = String::from("wdl");
    bash_var
        .push_str(&Alphanumeric.sample_string(&mut rand::rng(), placeholder_len.saturating_sub(3)));
    (bash_var, false)
}

/// Sanitize a [CommandSection].
///
/// Removes all trailing whitespace, replaces placeholders
/// with dummy bash variables or literals.
///
/// If the section contains mixed indentation, returns None.
fn sanitize_command(
    section: &CommandSection,
    context: &mut CommandContext<'_>,
) -> Option<(String, HashSet<String>, usize)> {
    let amount_stripped = section.count_whitespace()?;
    let mut sanitized_command = String::new();
    let mut decls = HashSet::new();
    let mut in_single_quotes = false;

    let mut evaluator = ExprTypeEvaluator::new(context);

    match section.strip_whitespace() {
        Some(cmd_parts) => {
            cmd_parts.iter().for_each(|part| match part {
                StrippedCommandPart::Text(text) => {
                    sanitized_command.push_str(text);
                    in_single_quotes ^= !is_quote_balanced(text, '\'');
                }
                StrippedCommandPart::Placeholder(placeholder) => {
                    let ty = evaluator.evaluate_expr(&placeholder.expr());
                    let (substitution, literal_inserted) = to_bash_var(placeholder, ty);

                    if literal_inserted || in_single_quotes {
                        sanitized_command.push_str(&substitution);
                    } else {
                        let substitution = substitution
                            .chars()
                            .take(substitution.len().saturating_sub(3))
                            .collect::<String>();
                        decls.insert(substitution.clone());
                        sanitized_command.push_str(&format!("${{{substitution}}}"));
                    }
                }
            });
            Some((sanitized_command, decls, amount_stripped))
        }
        _ => None,
    }
}

/// Maps each line as shellcheck sees it to its corresponding span in the
/// source.
fn map_shellcheck_lines(
    section: &CommandSection,
    leading_whitespace: usize,
) -> HashMap<usize, Span> {
    let mut line_map = HashMap::new();
    let mut line_num = 1;
    let mut skip_next_line = false;
    for part in section.parts() {
        match part {
            CommandPart::Text(ref text) => {
                for (line, line_start, _) in lines_with_offset(text.text()) {
                    // this occurs after encountering a placeholder
                    if skip_next_line {
                        skip_next_line = false;
                        continue;
                    }
                    // The first line is removed entirely, UNLESS there is content on it.
                    if line_num == 1 && line.is_empty() {
                        continue;
                    }
                    // Add back the leading whitespace that was stripped.
                    let adjusted_start = text.span().start() + line_start + leading_whitespace;
                    line_map.insert(line_num, Span::new(adjusted_start, line.len()));
                    line_num += 1;
                }
            }
            CommandPart::Placeholder(_) => {
                skip_next_line = true;
            }
        }
    }
    line_map
}

/// Calculates the correct [Span] for a [ShellCheckDiagnostic] relative to the
/// source.
fn calculate_span(diagnostic: &ShellCheckDiagnostic, line_map: &HashMap<usize, Span>) -> Span {
    // shellcheck 1-indexes columns, so subtract 1.
    let start = line_map
        .get(&diagnostic.line)
        .expect("shellcheck line corresponds to command line")
        .start()
        + diagnostic.column
        - 1;
    let len = if diagnostic.end_line > diagnostic.line {
        // this is a multiline diagnostic
        let end_line_end = line_map
            .get(&diagnostic.end_line)
            .expect("shellcheck line corresponds to command line")
            .start()
            + diagnostic.end_column
            - 1;
        end_line_end.saturating_sub(start)
    } else {
        // single line diagnostic
        (diagnostic.end_column).saturating_sub(diagnostic.column)
    };
    Span::new(start, len)
}

impl Visitor for ShellCheckRule {
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

        if !SHELLCHECK_EXISTS.get_or_init(|| {
            if !program_exists(SHELLCHECK_BIN) {
                let command_keyword = support::token(section.inner(), SyntaxKind::CommandKeyword)
                    .expect(
                        "should have a
                command keyword token",
                    );
                diagnostics.exceptable_add(
                    Diagnostic::note("running `shellcheck` on command section")
                        .with_label(
                            "could not find `shellcheck` executable.",
                            command_keyword.text_range(),
                        )
                        .with_rule(ID)
                        .with_fix(
                            "install shellcheck (https://www.shellcheck.net) or disable this lint.",
                        ),
                    SyntaxElement::from(section.inner().clone()),
                    &self.exceptable_nodes(),
                );
                return false;
            }
            true
        }) {
            return;
        }

        // Replace all placeholders in the command with dummy bash variables
        let doc = self.document.clone().expect("should have a document");
        let Some(scope) = doc.find_scope_by_position(section.inner().text_range().start().into())
        else {
            // This is the case where the command section has not been analyzed
            // e.g. it is in a task that has not been analyzed because it is a duplicate.
            return;
        };
        let mut context = CommandContext::new(doc.clone(), scope);
        let Some((sanitized_command, cmd_decls, amount_stripped)) =
            sanitize_command(section, &mut context)
        else {
            // This is the case where the command section contains
            // mixed indentation. We silently return and allow
            // the mixed indentation lint to report this.
            return;
        };
        let line_map = map_shellcheck_lines(section, amount_stripped);

        // create a Fenwick tree where each index is a line number
        // and each value is the length of the line.
        // For efficiency, we do this only once.
        let shift_values = lines_with_offset(&sanitized_command)
            .map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);

        match run_shellcheck(&sanitized_command) {
            Ok(sc_diagnostics) => {
                for sc_diagnostic in sc_diagnostics {
                    // Skip declarations that shellcheck is unaware of.
                    // ShellCheck's message always starts with the variable name
                    // that is unassigned.
                    let target_variable = sc_diagnostic
                        .message
                        .split_whitespace()
                        .next()
                        .unwrap_or("");
                    if sc_diagnostic.code == SHELLCHECK_REFERENCED_UNASSIGNED
                        && cmd_decls.contains(target_variable)
                    {
                        continue;
                    }
                    diagnostics.exceptable_add(
                        shellcheck_lint(&sc_diagnostic, &sanitized_command, &line_map, &shift_tree),
                        SyntaxElement::from(section.inner().clone()),
                        &self.exceptable_nodes(),
                    )
                }
            }
            Err(e) => {
                let command_keyword = support::token(section.inner(), SyntaxKind::CommandKeyword)
                    .expect("should have a command keyword token");
                diagnostics.exceptable_add(
                    Diagnostic::error("running `shellcheck` on command section")
                        .with_label(e.to_string(), command_keyword.text_range())
                        .with_rule(ID)
                        .with_fix("address reported error."),
                    SyntaxElement::from(section.inner().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ftree::FenwickTree;
    use pretty_assertions::assert_eq;
    use wdl_ast::Document;
    use wdl_ast::v1::Expr;

    use super::ShellCheckReplacement;
    use super::normalize_replacements;
    use crate::fix::Fixer;
    use crate::fix::{self};
    use crate::util::lines_with_offset;

    #[test]
    fn test_normalize_replacements() {
        // shellcheck would see this as
        // ABBBB
        // BBBA
        let ref_str = String::from("ABBBB\nBBBA");
        let expected = String::from("AAAAA");
        let sc_rep = ShellCheckReplacement {
            line: 1,
            end_line: 2,
            column: 2,
            end_column: 4,
            precedence: 1,
            insertion_point: fix::InsertionPoint::AfterEnd,
            value: String::from("AAA"),
        };
        let shift_values =
            lines_with_offset(&ref_str).map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);
        let normalized = normalize_replacements(&[sc_rep], &shift_tree);
        let rep = &normalized[0];

        assert_eq!(rep.start(), 1);
        assert_eq!(rep.end(), 9);

        let mut fixer = Fixer::new(ref_str);
        fixer.apply_replacement(rep);
        assert_eq!(fixer.value(), expected);
    }

    #[test]
    fn test_normalize_replacements2() {
        let ref_str = String::from("ABBBBBBBA");
        let expected = String::from("AAAAA");
        let sc_rep = ShellCheckReplacement {
            line: 1,
            end_line: 1,
            column: 2,
            end_column: 9,
            precedence: 1,
            insertion_point: fix::InsertionPoint::AfterEnd,
            value: String::from("AAA"),
        };
        let shift_values =
            lines_with_offset(&ref_str).map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);
        let normalized = normalize_replacements(&[sc_rep], &shift_tree);
        let rep = &normalized[0];

        assert_eq!(rep.start(), 1);
        assert_eq!(rep.end(), 8);

        let mut fixer = Fixer::new(ref_str);
        fixer.apply_replacement(rep);
        assert_eq!(fixer.value(), expected);
    }

    /// Parse a string containing a placeholder expression in the context of a
    /// `command` with a handful of inputs in scope.
    fn parse_placeholder_as_expr(command: &str) -> Expr {
        let source = format!(
            r#"
version 1.2

task test {{
    input {{
        String foo = "bar"
        Int baz = 42
        Array[File] arr = ["a", "b", "c"]
    }}
    command {{
        {command}
    }}
}}
"#
        );
        let (document, _diagnostics) = Document::parse(&source);
        document
            .ast()
            .as_v1()
            .expect("should be a v1 AST")
            .tasks()
            .next()
            .expect("has a task")
            .command()
            .expect("has a command")
            .parts()
            // 0th element is the text preceding the start of the spliced command
            .nth(1)
            .expect("has a command part")
            .unwrap_placeholder()
            .expr()
    }

    #[test]
    fn test_is_quoted1() {
        // Both sides of the addition are literals
        assert!(super::is_quoted(&parse_placeholder_as_expr(
            r#"echo ~{"hello" + " world"}"#
        )));
    }
    #[test]
    fn test_is_quoted2() {
        // This contains an unquoted variable.
        assert!(!super::is_quoted(&parse_placeholder_as_expr(
            r#"echo ~{"hello " + foo + " world"}"#
        )));
    }
    #[test]
    fn test_is_quoted3() {
        // This contains a quoted variable.
        assert!(super::is_quoted(&parse_placeholder_as_expr(
            r#"echo ~{"hello '" + foo + "' world"}"#
        )));
    }
    #[test]
    fn test_is_quoted4() {
        // This contains a hanging quote.
        assert!(!super::is_quoted(&parse_placeholder_as_expr(
            r#"echo ~{"hello '" + foo + " world"}"#
        )));
    }

    #[test]
    fn test_evaluates_to_bash_literal1() {
        // Both sides of the addition are literals
        assert!(super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{"hello" + " world"}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal2() {
        // This is not a literal because of the unquoted
        // placeholder substitution.
        assert!(!super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{"hello " + foo + " world"}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal3() {
        // This is a literal because of the quoted
        // placeholder substitution.
        assert!(super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{"hello '" + foo + "' world"}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal4() {
        // This is a literal because all array elements are literals.
        assert!(super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{sep(" ", ["a", "b", "c"])}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal5() {
        // This is not a literal because the array is not
        // guaranteed to be all literals.
        assert!(!super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{sep(" ", arr)}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal6() {
        // Surrounding with quotes makes it a literal.
        assert!(super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{sep(" ", quote(arr))}"#)
        ));
    }
    #[test]
    fn test_evaluates_to_bash_literal7() {
        // This contains a quoted placeholder.
        assert!(!super::evaluates_to_bash_literal(
            &parse_placeholder_as_expr(r#"echo ~{if 1=1 then "hello '~{foo}' world" else ""}"#)
        ));
    }
}
