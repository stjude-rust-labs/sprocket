//! Contextual help for the `run` subcommand.

use std::fmt;
use std::fs;
use std::path::PathBuf;

use clap::CommandFactory as _;
use wdl::ast::AstNode as _;
use wdl::ast::AstToken as _;
use wdl::ast::DOC_COMMENT_PREFIX;
use wdl::ast::Documented as _;
use wdl::ast::SupportedVersion;
use wdl::ast::v1::Ast;
use wdl::ast::v1::Decl;

use super::Args;
use crate::analysis::Source;
use crate::commands::CommandOutput;
use crate::commands::CommandResult;
use crate::system::v1::exec::SelectTargetError;
use crate::system::v1::exec::Target;

/// Displays help for the `run` subcommand.
///
/// Contextual information about the inputs of the selected task or workflow is
/// appended to the standard help when the source is a local WDL document.
pub fn display_help(
    args: &Args,
    fallback_version: Option<SupportedVersion>,
    output: CommandOutput,
) -> CommandResult<()> {
    // Match Clap's behavior of rendering the long help for `--help` and the
    // short help for `-h`. Both flags map to the same `SetTrue` action, so the
    // requested variant is recovered from the raw arguments.
    let base = if std::env::args().any(|arg| arg == "--help") {
        base_help(true)
    } else {
        base_help(false)
    };

    let help = format_contextual_help(
        args.source.as_ref(),
        args.target.as_deref(),
        fallback_version,
        &base,
    );
    output.payload(help);
    Ok(())
}

/// Renders the standard Clap help for the `run` subcommand.
///
/// The help is rendered from the full command tree so that propagated global
/// arguments are included and the usage line is prefixed with the binary name.
///
/// When `long` is true, the long help is rendered (as for `--help`); otherwise
/// the short help is rendered (as for `-h`).
fn base_help(long: bool) -> String {
    let mut cmd = crate::Cli::command();
    cmd.build();

    let run = cmd
        .find_subcommand_mut("run")
        .expect("the `run` subcommand should exist");

    // The source is only rendered as required; the actual requirement is
    // enforced during parsing via `required_unless_present = "help"`, which
    // Clap renders as optional in the help output.
    *run = std::mem::take(run).mut_arg("source", |arg| arg.required(true));

    let rendered = if long {
        run.render_long_help()
    } else {
        run.render_help()
    }
    .to_string();

    // Clap trims trailing whitespace from blank separator lines when printing
    // help directly; match that output exactly.
    rendered
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats the contextual help for the `run` subcommand.
///
/// The returned help consists of the standard Clap help followed by a section
/// describing the inputs of the selected task or workflow (if any).
pub fn format_contextual_help(
    source: Option<&Source>,
    target: Option<&str>,
    fallback_version: Option<SupportedVersion>,
    base_help: &str,
) -> String {
    let Some(source) = source else {
        return base_help.to_string();
    };

    let path = match resolve_local_file(source) {
        Ok(Some(path)) => path,
        Ok(None) => {
            return append_note(
                base_help,
                "contextual input help is not available for remote sources",
            );
        }
        Err(note) => return append_note(base_help, note),
    };

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            return append_note(
                base_help,
                format!(
                    "failed to read `{path}` for contextual input help: {e}",
                    path = path.display()
                ),
            );
        }
    };

    let (document, _) = wdl::ast::Document::parse(&text, fallback_version);
    let Some(ast) = document.ast().into_v1() else {
        return append_note(
            base_help,
            "contextual input help is not available for this WDL document",
        );
    };

    match select_target_from_ast(&ast, target) {
        Ok(selected) => {
            let mut help = base_help.to_string();
            format_input_section(&ast, &selected, target.is_some(), &mut help);
            help
        }
        Err(SelectTargetError::TargetNotFound(name)) => append_note(
            base_help,
            format!("no task or workflow named `{name}` was found in the document"),
        ),
        Err(SelectTargetError::TargetRequired) => {
            let mut help = base_help.to_string();
            help.push_str("\n\nAvailable Targets:");
            for workflow in ast.workflows() {
                help.push_str(&format!("\n  {}", workflow.name().text()));
            }
            for task in ast.tasks() {
                help.push_str(&format!("\n  {}", task.name().text()));
            }

            let reason = if ast.workflows().nth(1).is_some() {
                "the document contains multiple workflows"
            } else {
                "the document contains multiple tasks and no workflow"
            };
            help.push_str(&format!(
                "\n\nNote: {reason}; use `--target <NAME>` to select a target"
            ));
            help
        }
        Err(SelectTargetError::NoExecutableTarget) => {
            append_note(base_help, "the document contains no tasks and no workflow")
        }
    }
}

/// Appends a note to the given help text.
fn append_note(base_help: &str, note: impl fmt::Display) -> String {
    format!("{base_help}\n\nNote: {note}")
}

/// Resolves the local file to read for contextual help.
///
/// Returns `None` for remote URLs, which are not read to generate contextual
/// help. An error is returned if a directory source cannot be resolved to its
/// module entrypoint.
fn resolve_local_file(source: &Source) -> Result<Option<PathBuf>, String> {
    match source {
        Source::Url(_) => Ok(None),
        Source::File(url) => Ok(url.to_file_path().ok()),
        Source::Directory(dir) => match crate::analysis::resolve_module_entrypoint(dir) {
            Ok(Source::File(url)) => Ok(url.to_file_path().ok()),
            Ok(_) => unreachable!("the module entrypoint is always a file"),
            Err(e) => Err(e.to_string()),
        },
    }
}

/// Selects the target task or workflow from a parsed AST.
///
/// This mirrors the selection policy of
/// [`select_target`](crate::system::v1::exec::select_target) without requiring
/// a fully analyzed document:
///
/// 1. If a target is provided, the workflow or task with that name is selected.
/// 2. Otherwise, the workflow is selected if exactly one is present.
/// 3. Otherwise, the single task is selected if exactly one exists.
/// 4. Otherwise, an error is returned.
pub fn select_target_from_ast(
    ast: &Ast,
    target: Option<&str>,
) -> Result<Target, SelectTargetError> {
    if let Some(target) = target {
        if ast
            .workflows()
            .any(|workflow| workflow.name().text() == target)
        {
            return Ok(Target::Workflow(target.to_owned()));
        }

        if ast.tasks().any(|task| task.name().text() == target) {
            return Ok(Target::Task(target.to_owned()));
        }

        return Err(SelectTargetError::TargetNotFound(target.to_owned()));
    }

    let mut workflows = ast.workflows();
    let workflow = workflows.next();
    if workflows.next().is_some() {
        return Err(SelectTargetError::TargetRequired);
    }
    if let Some(workflow) = workflow {
        return Ok(Target::Workflow(workflow.name().text().to_string()));
    }

    let mut tasks = ast.tasks();
    match (tasks.next(), tasks.next()) {
        (Some(_), Some(_)) => Err(SelectTargetError::TargetRequired),
        (Some(task), None) => Ok(Target::Task(task.name().text().to_string())),
        (None, _) => Err(SelectTargetError::NoExecutableTarget),
    }
}

/// A single input row in the contextual inputs listing.
#[derive(Debug)]
struct InputRow {
    /// The key to pass on the command line (e.g., `wf.sample_id=`).
    key: String,
    /// The type, default, required marker, and description of the input.
    details: String,
}

/// Formats the inputs of the selected target.
///
/// Input keys are qualified with the target name unless the target was
/// explicitly specified, matching the key format the engine accepts.
fn format_input_section(ast: &Ast, target: &Target, target_specified: bool, help: &mut String) {
    let inputs = match target {
        Target::Workflow(name) => ast
            .workflows()
            .find(|workflow| workflow.name().text() == name)
            .and_then(|workflow| workflow.input()),
        Target::Task(name) => ast
            .tasks()
            .find(|task| task.name().text() == name)
            .and_then(|task| task.input()),
    };

    let kind = match target {
        Target::Workflow(_) => "Workflow Inputs",
        Target::Task(_) => "Task Inputs",
    };

    help.push_str(&format!("\n\n{kind} ({}):", target.name()));

    let prefix = if target_specified {
        String::new()
    } else {
        format!("{}.", target.name())
    };

    let rows = match inputs {
        Some(inputs) => inputs
            .declarations()
            .map(|decl| format_row(&prefix, decl))
            .collect(),
        None => Vec::new(),
    };

    if rows.is_empty() {
        help.push_str("\n  (none)");
        return;
    }

    let width = rows
        .iter()
        .map(|row| row.key.len())
        .max()
        .unwrap_or_default();
    for row in rows {
        help.push_str(&format!("\n  {:<width$} {}", row.key, row.details));
    }
}

/// Formats a declaration into an input row.
fn format_row(prefix: &str, decl: Decl) -> InputRow {
    match decl {
        Decl::Bound(decl) => {
            let key = format!("{prefix}{}=", decl.name().text());
            let details = format!("{} = {}", decl.ty(), decl.expr().text());
            InputRow {
                key,
                details: with_description(details, decl.doc_comments()),
            }
        }
        Decl::Unbound(decl) => {
            let key = format!("{prefix}{}=", decl.name().text());
            let ty = decl.ty();
            let details = if ty.is_optional() {
                format!("{ty}")
            } else {
                format!("{ty}  (required)")
            };
            InputRow {
                key,
                details: with_description(details, decl.doc_comments()),
            }
        }
    }
}

/// Appends the first line of the declaration's doc comments (if any) to the
/// given details.
fn with_description(mut details: String, comments: Option<Vec<wdl::ast::Comment>>) -> String {
    if let Some(description) = first_doc_line(comments) {
        details.push_str("  ## ");
        details.push_str(&description);
    }

    details
}

/// Extracts the first line of the first doc comment.
fn first_doc_line(comments: Option<Vec<wdl::ast::Comment>>) -> Option<String> {
    comments?
        .first()
        .and_then(|comment| comment.text().strip_prefix(DOC_COMMENT_PREFIX))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use url::Url;

    use super::*;

    /// A workflow with bound, unbound, optional, and documented inputs.
    const WORKFLOW: &str = r#"version 1.2

workflow wf {
  input {
    ## Sample identifier
    String sample_id
    Int threads = 4
    File? ref_fasta
    Array[File] reads
  }
}
"#;

    /// A document with multiple workflows and no tasks.
    const MULTI_WORKFLOWS: &str = r#"version 1.2

workflow first {
}

workflow second {
}
"#;

    /// A document with multiple tasks and no workflow.
    const MULTI_TASKS: &str = r#"version 1.2

task first {
  input {
    String value
  }

  command <<<>>>
}

task second {
  input {
    String value
  }

  command <<<>>>
}
"#;

    /// Parses a WDL source into a v1 AST.
    fn ast(source: &str) -> Ast {
        let (document, _) = wdl::ast::Document::parse(source, None);
        document.ast().into_v1().expect("should parse to a v1 AST")
    }

    /// Writes the given WDL source to a temporary file and returns its source.
    fn source_file(text: &str) -> (TempDir, Source) {
        let dir = tempfile::tempdir().expect("should create a temporary directory");
        let path = dir.path().join("source.wdl");
        fs::write(&path, text).expect("should write the source file");
        let url = Url::from_file_path(&path).expect("should convert the path to a URL");
        (dir, Source::File(url))
    }

    /// Asserts that the given help contains the expected inputs section.
    fn assert_workflow_section(help: &str) {
        assert!(help.contains("Workflow Inputs (wf):"), "{help}");
        assert!(help.contains("wf.sample_id="), "{help}");
        assert!(help.contains("String"), "{help}");
        assert!(help.contains("## Sample identifier"), "{help}");
        assert!(help.contains("Int = 4"), "{help}");
        assert!(help.contains("File?"), "{help}");
        assert!(help.contains("(required)"), "{help}");
    }

    #[test]
    fn selects_workflow_when_present() {
        let ast = ast(
            "version 1.2\n\nworkflow wf {}\n\ntask first { command <<<>>> }\n\ntask second { \
             command <<<>>> }\n",
        );
        assert_eq!(
            select_target_from_ast(&ast, None).unwrap().into_name(),
            "wf"
        );
    }

    #[test]
    fn selects_single_task() {
        let ast = ast("version 1.2\n\ntask only { command <<<>>> }\n");
        assert_eq!(
            select_target_from_ast(&ast, None).unwrap().into_name(),
            "only"
        );
    }

    #[test]
    fn errors_on_multiple_workflows() {
        let ast = ast(MULTI_WORKFLOWS);
        assert!(matches!(
            select_target_from_ast(&ast, None),
            Err(SelectTargetError::TargetRequired)
        ));
    }

    #[test]
    fn lists_available_workflows() {
        let (_dir, source) = source_file(MULTI_WORKFLOWS);
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert!(help.contains("Available Targets:"), "{help}");
        assert!(help.contains("first"), "{help}");
        assert!(help.contains("second"), "{help}");
        assert!(
            help.contains("the document contains multiple workflows"),
            "{help}"
        );
        assert!(help.contains("`--target <NAME>`"), "{help}");
    }

    #[test]
    fn errors_on_multiple_tasks_without_workflow() {
        let ast = ast(MULTI_TASKS);
        assert!(matches!(
            select_target_from_ast(&ast, None),
            Err(SelectTargetError::TargetRequired)
        ));
    }

    #[test]
    fn errors_on_empty_document() {
        let ast = ast("version 1.2\n");
        assert!(matches!(
            select_target_from_ast(&ast, None),
            Err(SelectTargetError::NoExecutableTarget)
        ));
    }

    #[test]
    fn selects_explicit_target() {
        let ast = ast(MULTI_TASKS);
        assert_eq!(
            select_target_from_ast(&ast, Some("second"))
                .unwrap()
                .into_name(),
            "second"
        );
    }

    #[test]
    fn prefers_workflow_over_task_with_same_name() {
        let ast = ast("version 1.2\n\nworkflow wf {}\n\ntask wf { command <<<>>> }\n");
        assert!(matches!(
            select_target_from_ast(&ast, Some("wf")),
            Ok(Target::Workflow(name)) if name == "wf"
        ));
    }

    #[test]
    fn errors_on_unknown_target() {
        let ast = ast(MULTI_TASKS);
        assert!(matches!(
            select_target_from_ast(&ast, Some("missing")),
            Err(SelectTargetError::TargetNotFound(name)) if name == "missing"
        ));
    }

    #[test]
    fn formats_workflow_inputs() {
        let (_dir, source) = source_file(WORKFLOW);
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert_workflow_section(&help);
    }

    #[test]
    fn formats_bare_keys_when_target_specified() {
        let (_dir, source) = source_file(WORKFLOW);
        let help = format_contextual_help(Some(&source), Some("wf"), None, "base");
        assert!(help.contains("sample_id="), "{help}");
        assert!(!help.contains("wf.sample_id="), "{help}");
    }

    #[test]
    fn formats_task_inputs() {
        let (_dir, source) = source_file(MULTI_TASKS);
        let help = format_contextual_help(Some(&source), Some("first"), None, "base");
        assert!(help.contains("Task Inputs (first):"), "{help}");
        assert!(help.contains("value="), "{help}");
        assert!(!help.contains("first.value="), "{help}");
    }

    #[test]
    fn lists_available_targets() {
        let (_dir, source) = source_file(MULTI_TASKS);
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert!(help.contains("Available Targets:"), "{help}");
        assert!(help.contains("first"), "{help}");
        assert!(help.contains("second"), "{help}");
        assert!(help.contains("`--target <NAME>`"), "{help}");
    }

    #[test]
    fn notes_unknown_target() {
        let (_dir, source) = source_file(WORKFLOW);
        let help = format_contextual_help(Some(&source), Some("missing"), None, "base");
        assert!(
            help.contains("no task or workflow named `missing`"),
            "{help}"
        );
    }

    #[test]
    fn notes_remote_source() {
        let source: Source = "https://example.com/source.wdl".parse().unwrap();
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert_eq!(
            help,
            "base\n\nNote: contextual input help is not available for remote sources"
        );
    }

    #[test]
    fn notes_non_v1_document() {
        let (_dir, source) = source_file("version 2.0\n\nworkflow wf {}\n");
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert!(
            help.contains("contextual input help is not available for this WDL document"),
            "{help}"
        );
    }

    #[test]
    fn resolves_module_entrypoint_for_directory_sources() {
        let dir = tempfile::tempdir().expect("should create a temporary directory");
        fs::write(
            dir.path().join("module.json"),
            r#"{"name":"demo","license":"MIT","entrypoint":"index.wdl"}"#,
        )
        .expect("should write the module manifest");
        fs::write(dir.path().join("index.wdl"), WORKFLOW).expect("should write the entrypoint");

        let source = Source::Directory(dir.path().to_path_buf());
        let help = format_contextual_help(Some(&source), None, None, "base");
        assert_workflow_section(&help);
    }

    #[test]
    fn base_help_includes_usage_and_globals() {
        let help = base_help(true);
        assert!(help.starts_with("Runs a task or workflow"), "{help}");
        assert!(
            help.contains("Usage: sprocket run [OPTIONS] <SOURCE> [INPUTS]..."),
            "{help}"
        );
        assert!(help.contains("--suffix"), "{help}");
        assert!(help.contains("--color"), "{help}");
        assert!(
            help.contains("Print help (see a summary with '-h')"),
            "{help}"
        );
    }

    #[test]
    fn short_help_renders_compact_options() {
        let help = base_help(false);
        assert!(help.starts_with("Runs a task or workflow"), "{help}");
        assert!(
            help.contains("Usage: sprocket run [OPTIONS] <SOURCE> [INPUTS]..."),
            "{help}"
        );
        assert!(help.contains("-h, --help"), "{help}");
        assert!(help.contains("Print help"), "{help}");
        assert!(
            !help.contains("Print help (see a summary with '-h')"),
            "{help}"
        );
    }
}
