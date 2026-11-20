//! Contextual help for the `run` subcommand.

use std::fmt;

use clap::CommandFactory as _;
use wdl::analysis::Document;
use wdl::ast::AstNode as _;
use wdl::ast::AstToken as _;
use wdl::ast::DOC_COMMENT_PREFIX;
use wdl::ast::Documented as _;
use wdl::ast::v1::Ast;
use wdl::ast::v1::Decl;

use super::Args;
use crate::Config;
use crate::analysis::Source;
use crate::analysis::analyze_singular_source;
use crate::commands::CommandError;
use crate::commands::CommandOutput;
use crate::commands::CommandResult;
use crate::commands::inputs::InputProcessor;
use crate::commands::inputs::Key;
use crate::system::v1::exec::SelectTargetError;
use crate::system::v1::exec::Target;
use crate::system::v1::exec::select_target;

/// Displays help for the `run` subcommand.
pub async fn display_help(
    args: &Args,
    config: &Config,
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

    let help =
        format_contextual_help(args.source.as_ref(), args.target.as_deref(), config, &base).await;
    output.payload(help);
    Ok(())
}

/// Renders the standard Clap help for the `run` subcommand.
fn base_help(long: bool) -> String {
    let mut cmd = crate::Cli::command();
    cmd.build();

    let mut run = cmd
        .find_subcommand("run")
        .expect("the `run` subcommand should exist")
        .clone();

    // The source is only rendered as required; the actual requirement is
    // enforced during parsing via `required_unless_present = "help"`, which
    // Clap renders as optional in the help output.
    run = run.mut_arg("source", |arg| arg.required(true));

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
pub async fn format_contextual_help(
    source: Option<&Source>,
    target: Option<&str>,
    config: &Config,
    base_help: &str,
) -> String {
    let Some(source) = source else {
        return base_help.to_string();
    };

    let source = match source {
        Source::Url(_) => {
            return append_note(
                base_help,
                "contextual input help is not available for remote sources",
            );
        }
        Source::Directory(dir) => match crate::analysis::resolve_module_entrypoint(dir) {
            Ok(source) => source,
            Err(error) => return append_note(base_help, error),
        },
        source => source.clone(),
    };

    let document = match analyze_singular_source(
        &source,
        config.common.wdl.fallback_version.into(),
        config.modules.clone(),
        config.common.wdl.feature_flags,
        config.common.ignore_filename(),
        config.common.report_mode,
        false,
    )
    .await
    {
        Ok(document) => document,
        Err(errors) => return append_note(base_help, CommandError::from(errors)),
    };

    let Some(ast) = document
        .root()
        .ast_with_version_fallback(document.config().fallback_version())
        .into_v1()
    else {
        return append_note(
            base_help,
            "contextual input help is not available for this WDL document",
        );
    };

    match select_target(&document, target) {
        Ok(selected) => {
            if document
                .diagnostics()
                .any(|d| d.severity() == wdl::ast::Severity::Error)
            {
                return append_note(
                    base_help,
                    "contextual input help is not available for this WDL document",
                );
            }
            let mut help = base_help.to_string();
            if let Err(error) =
                format_input_section(&document, &ast, &selected, target.is_some(), &mut help)
            {
                return append_note(base_help, error);
            }
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

/// A single input row in the contextual inputs listing.
#[derive(Debug)]
struct InputRow {
    /// The key to pass on the command line (e.g., `wf.sample_id=`).
    key: String,
    /// The type, default, required marker, and description of the input.
    details: String,
}

/// Formats the inputs of the selected target.
fn format_input_section(
    document: &Document,
    ast: &Ast,
    target: &Target,
    target_specified: bool,
    help: &mut String,
) -> anyhow::Result<()> {
    let mut processor = InputProcessor::new(true, true, false, false);
    let namespace = Key::new(target.name().to_owned());
    let kind = match target {
        Target::Workflow(name) => {
            let workflow = ast
                .workflows()
                .find(|workflow| workflow.name().text() == name)
                .expect("selected workflow should exist");
            processor.workflow(
                namespace,
                document,
                document
                    .local_workflow_by_name(name)
                    .expect("selected workflow should exist"),
                &workflow,
            )?;
            "Workflow Inputs"
        }
        Target::Task(name) => {
            let task = ast
                .tasks()
                .find(|task| task.name().text() == name)
                .expect("selected task should exist");
            processor.task(namespace, &task, &Default::default());
            "Task Inputs"
        }
    };

    help.push_str(&format!("\n\n{kind} ({}):", target.name()));

    let prefix = format!("{}.", target.name());
    let rows = processor
        .results
        .into_iter()
        .map(|(key, decl)| {
            let key = if target_specified {
                key.strip_prefix(&prefix)
                    .expect("input key should be qualified")
                    .to_owned()
            } else {
                key
            };
            format_row(key, decl)
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        help.push_str("\n  (none)");
        return Ok(());
    }

    let width = rows
        .iter()
        .map(|row| row.key.len())
        .max()
        .unwrap_or_default();
    for row in rows {
        help.push_str(&format!("\n  {:<width$} {}", row.key, row.details));
    }
    Ok(())
}

/// Formats a declaration into an input row.
fn format_row(key: String, decl: Decl) -> InputRow {
    let key = format!("{key}=");
    match decl {
        Decl::Bound(decl) => {
            let details = format!("{} = {}", decl.ty(), decl.expr().text());
            InputRow {
                key,
                details: with_description(details, decl.doc_comments()),
            }
        }
        Decl::Unbound(decl) => {
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
    const MULTI_WORKFLOWS: &str = r#"version 1.4

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

    #[tokio::test]
    async fn selects_workflow_when_present() {
        let (_dir, source) = source_file(
            "version 1.2\n\nworkflow wf {}\n\ntask first { command <<<>>> }\n\ntask second { \
             command <<<>>> }\n",
        );
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(help.contains("Workflow Inputs (wf):"), "{help}");
    }

    #[tokio::test]
    async fn selects_single_task() {
        let (_dir, source) = source_file("version 1.2\n\ntask only { command <<<>>> }\n");
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(help.contains("Task Inputs (only):"), "{help}");
    }

    #[tokio::test]
    async fn lists_available_workflows() {
        let (_dir, source) = source_file(MULTI_WORKFLOWS);
        let mut config = Config::default();
        config.common.wdl.feature_flags = config.common.wdl.feature_flags.with_wdl_1_4();
        let help = format_contextual_help(Some(&source), None, &config, "base").await;
        assert!(help.contains("Available Targets:"), "{help}");
        assert!(help.contains("first"), "{help}");
        assert!(help.contains("second"), "{help}");
        assert!(
            help.contains("the document contains multiple workflows"),
            "{help}"
        );
        assert!(help.contains("`--target <NAME>`"), "{help}");
    }

    #[tokio::test]
    async fn notes_empty_document() {
        let (_dir, source) = source_file("version 1.2\n");
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(
            help.contains("the document contains no tasks and no workflow"),
            "{help}"
        );
    }

    #[tokio::test]
    async fn includes_required_nested_call_input() {
        let (_dir, source) = source_file(
            r#"version 1.2

task nested {
  input {
    String value
  }
  command <<<>>>
}

workflow wf {
  call nested
  meta {
    allowNestedInputs: true
  }
}
"#,
        );
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(
            help.contains("wf.nested.value= String  (required)"),
            "{help}"
        );
        assert!(!help.contains("(none)"), "{help}");
    }

    #[tokio::test]
    async fn includes_imported_nested_inputs_with_bare_keys() {
        let (dir, source) = source_file(include_str!(
            "../../../tests/cli/run/help-nested-inputs/inputs/source.wdl"
        ));
        fs::write(
            dir.path().join("child.wdl"),
            include_str!("../../../tests/cli/run/help-nested-inputs/inputs/child.wdl"),
        )
        .unwrap();
        let help =
            format_contextual_help(Some(&source), Some("wf"), &Config::default(), "base").await;
        assert!(help.contains("outer.inner.value="), "{help}");
        assert!(
            help.contains("String  (required)  ## Sample identifier"),
            "{help}"
        );
        assert!(help.contains("Int = 4"), "{help}");
        assert!(help.contains("File?"), "{help}");
        assert!(help.contains("String = value + \"!\""), "{help}");
        assert!(!help.contains("wf.outer"), "{help}");
        assert!(!help.contains("fixed="), "{help}");
        assert!(!help.contains("supplied="), "{help}");
    }

    #[tokio::test]
    async fn includes_imported_task_inputs_with_alias() {
        let (dir, source) = source_file(
            r#"version 1.2
import "child.wdl" as imported
workflow wf {
  call imported.nested as renamed { input: supplied = "provided" }
  meta { allowNestedInputs: true }
}
"#,
        );
        fs::write(
            dir.path().join("child.wdl"),
            include_str!("../../../tests/cli/run/help-nested-inputs/inputs/child.wdl"),
        )
        .unwrap();
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(help.contains("wf.renamed.value="), "{help}");
        assert!(!help.contains("supplied="), "{help}");
        assert!(!help.contains("wf.nested"), "{help}");
    }

    #[tokio::test]
    async fn omits_disallowed_nested_inputs() {
        let (_dir, source) = source_file(
            r#"version 1.2
task nested {
  input { String value = "default" }
  command <<<>>>
}
workflow wf {
  call nested
  meta { allowNestedInputs: false }
}
"#,
        );
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(help.contains("Workflow Inputs (wf):\n  (none)"), "{help}");
        assert!(!help.contains("value="), "{help}");
    }

    #[tokio::test]
    async fn notes_analysis_errors() {
        let (_dir, source) =
            source_file("version 1.2\nworkflow wf { input { Int value = \"invalid\" } }\n");
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert_eq!(
            help,
            "base\n\nNote: contextual input help is not available for this WDL document"
        );
    }

    #[tokio::test]
    async fn uses_configured_fallback_version() {
        let (_dir, source) = source_file(&WORKFLOW.replace("version 1.2", "version development"));
        let mut config = Config::default();
        config.common.wdl.fallback_version =
            "1.2".parse::<wdl::ast::SupportedVersion>().unwrap().into();
        let help = format_contextual_help(Some(&source), None, &config, "base").await;
        assert_workflow_section(&help);
    }

    #[tokio::test]
    async fn formats_workflow_inputs() {
        let (_dir, source) = source_file(WORKFLOW);
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert_workflow_section(&help);
    }

    #[tokio::test]
    async fn formats_bare_keys_when_target_specified() {
        let (_dir, source) = source_file(WORKFLOW);
        let help =
            format_contextual_help(Some(&source), Some("wf"), &Config::default(), "base").await;
        assert!(help.contains("sample_id="), "{help}");
        assert!(!help.contains("wf.sample_id="), "{help}");
    }

    #[tokio::test]
    async fn formats_task_inputs() {
        let (_dir, source) = source_file(MULTI_TASKS);
        let help =
            format_contextual_help(Some(&source), Some("first"), &Config::default(), "base").await;
        assert!(help.contains("Task Inputs (first):"), "{help}");
        assert!(help.contains("value="), "{help}");
        assert!(!help.contains("first.value="), "{help}");
    }

    #[tokio::test]
    async fn lists_available_targets() {
        let (_dir, source) = source_file(MULTI_TASKS);
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(help.contains("Available Targets:"), "{help}");
        assert!(help.contains("first"), "{help}");
        assert!(help.contains("second"), "{help}");
        assert!(help.contains("`--target <NAME>`"), "{help}");
    }

    #[tokio::test]
    async fn notes_unknown_target() {
        let (_dir, source) = source_file(WORKFLOW);
        let help =
            format_contextual_help(Some(&source), Some("missing"), &Config::default(), "base")
                .await;
        assert!(
            help.contains("no task or workflow named `missing`"),
            "{help}"
        );
    }

    #[tokio::test]
    async fn notes_remote_source() {
        let source: Source = "https://example.com/source.wdl".parse().unwrap();
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert_eq!(
            help,
            "base\n\nNote: contextual input help is not available for remote sources"
        );
    }

    #[tokio::test]
    async fn notes_non_v1_document() {
        let (_dir, source) = source_file("version 2.0\n\nworkflow wf {}\n");
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
        assert!(
            help.contains("contextual input help is not available for this WDL document"),
            "{help}"
        );
    }

    #[tokio::test]
    async fn resolves_module_entrypoint_for_directory_sources() {
        let dir = tempfile::tempdir().expect("should create a temporary directory");
        fs::write(
            dir.path().join("module.json"),
            r#"{"name":"demo","license":"MIT","entrypoint":"index.wdl"}"#,
        )
        .expect("should write the module manifest");
        fs::write(dir.path().join("index.wdl"), WORKFLOW).expect("should write the entrypoint");

        let source = Source::Directory(dir.path().to_path_buf());
        let help = format_contextual_help(Some(&source), None, &Config::default(), "base").await;
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
