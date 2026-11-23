//! Contextual help for the `run` subcommand.

use std::fmt;

use clap::CommandFactory as _;
use wdl::analysis::Document;
use wdl::ast::AstToken as _;
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
    let mut help = if std::env::args().any(|arg| arg == "--help") {
        base_help(true)
    } else {
        base_help(false)
    };

    append_contextual_help(
        args.source.as_ref(),
        args.target.as_deref(),
        config,
        &output,
        &mut help,
    )
    .await;
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
pub async fn append_contextual_help(
    source: Option<&Source>,
    target: Option<&str>,
    config: &Config,
    output: &CommandOutput,
    buffer: &mut String,
) {
    let Some(source) = source else {
        return;
    };

    let source = match source {
        Source::Directory(dir) => match crate::analysis::resolve_module_entrypoint(dir) {
            Ok(source) => source,
            Err(error) => {
                append_note(buffer, error);
                return;
            }
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
        output.colorize(),
    )
    .await
    {
        Ok(document) => document,
        Err(errors) => {
            append_note(buffer, CommandError::from(errors));
            return;
        }
    };

    let Some(ast) = document
        .root()
        .ast_with_version_fallback(document.config().fallback_version())
        .into_v1()
    else {
        append_note(
            buffer,
            "contextual input help is not available for this WDL document",
        );
        return;
    };

    match select_target(&document, target) {
        Ok(selected) => {
            if document
                .diagnostics()
                .any(|d| d.severity() == wdl::ast::Severity::Error)
            {
                append_note(
                    buffer,
                    "contextual input help is not available for this WDL document",
                );
                return;
            }
            if let Err(error) =
                process_input_section(&document, &ast, &selected, target.is_some(), buffer)
            {
                append_note(buffer, error);
            }
        }
        Err(SelectTargetError::TargetNotFound(name)) => append_note(
            buffer,
            format!("no task or workflow named `{name}` was found in the document"),
        ),
        Err(SelectTargetError::TargetRequired) => {
            buffer.push_str("\n\nAvailable Targets:");
            for workflow in ast.workflows() {
                buffer.push_str(&format!("\n  `{}` (workflow)", workflow.name().text()));
            }
            for task in ast.tasks() {
                buffer.push_str(&format!("\n  `{}` (task)", task.name().text()));
            }
        }
        Err(SelectTargetError::NoExecutableTarget) => {
            append_note(buffer, "the document contains no tasks and no workflow");
        }
    }
}

/// Appends a note to the given help text.
fn append_note(buffer: &mut String, note: impl fmt::Display) {
    buffer.push_str(&format!("\n\nNote: {note}"));
}

/// Processes the inputs of the selected target.
fn process_input_section(
    document: &Document,
    ast: &Ast,
    target: &Target,
    target_specified: bool,
    buffer: &mut String,
) -> anyhow::Result<()> {
    let mut processor = InputProcessor::new(true, false, true, false);
    let namespace = Key::new(target.name().to_owned());
    match target {
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
        }
        Target::Task(name) => {
            let task = ast
                .tasks()
                .find(|task| task.name().text() == name)
                .expect("selected task should exist");
            processor.task(namespace, &task, &Default::default());
        }
    };

    buffer.push_str(&format!("\n\nRequired Inputs for `{}`:", target.name()));

    let prefix = format!("{}.", target.name());
    let inputs = processor
        .results
        .into_iter()
        .filter_map(|(key, decl)| {
            let key = if target_specified {
                key.strip_prefix(&prefix)
                    .expect("input key should be qualified")
                    .to_owned()
            } else {
                key
            };
            process_decl(key, decl)
        })
        .collect::<Vec<_>>();

    if inputs.is_empty() {
        buffer.push_str("\n  (no required inputs)");
        return Ok(());
    }

    for row in inputs {
        buffer.push_str(&format!("\n  {row}"));
    }
    Ok(())
}

/// Processes a declaration into an input row.
fn process_decl(key: String, decl: Decl) -> Option<String> {
    match decl {
        Decl::Unbound(decl) if !decl.ty().is_optional() => {
            Some(format!("{key}=<{ty}>", ty = decl.ty()))
        }
        _ => None,
    }
}
