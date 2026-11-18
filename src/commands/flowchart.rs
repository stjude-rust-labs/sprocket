//! Implementation of the `dev flowchart` subcommand.

mod mermaid;

use std::fmt;

use anyhow::Context;
use anyhow::anyhow;
use clap::ValueEnum;
use wdl::ast::Ast;
use wdl::ast::AstNode as _;
use wdl::ast::AstToken as _;
use wdl::ast::Severity;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;

use crate::Config;
use crate::analysis::Source;
use crate::analysis::analyze_singular_source;
use crate::commands::CommandError;
use crate::commands::CommandResult;

/// The type of diagram to render.
#[derive(ValueEnum, Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum FlowchartType {
    /// A `Mermaid` flowchart diagram.
    #[default]
    Mermaid,
}

impl fmt::Display for FlowchartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mermaid => write!(f, "mermaid"),
        }
    }
}

/// Arguments for the `dev flowchart` subcommand.
#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The path or URL to a `WDL` document containing the workflow.
    #[arg(value_name = "SOURCE")]
    pub source: Source,

    /// The type of diagram to render.
    #[arg(short = 't', long = "type", value_name = "TYPE", default_value_t)]
    pub kind: FlowchartType,

    /// The name of the workflow to render.
    #[arg(short, long, value_name = "WORKFLOW")]
    pub workflow: Option<String>,

    /// The maximum recursion depth when expanding called workflows.
    ///
    /// Defaults to unlimited. Pass `0` to disable expansion entirely (all
    /// called workflows render as plain nodes). Pass `1` to expand only direct
    /// callees, `2` for their callees, and so on.
    #[arg(long, value_name = "DEPTH")]
    pub depth: Option<usize>,

    /// The report mode for diagnostics.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// Runs the `dev flowchart` subcommand.
pub async fn flowchart(args: Args, config: &Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or_default();
    let document = analyze_singular_source(
        &args.source,
        config.common.wdl.fallback_version.into(),
        config.modules.clone(),
        config.common.wdl.feature_flags,
        config.common.ignore_filename(),
        report_mode,
        colorize,
    )
    .await
    .map_err(CommandError::from)?;

    let errors: Vec<_> = document
        .diagnostics()
        .filter(|d| d.severity() == Severity::Error)
        .collect();

    if !errors.is_empty() {
        let path = document.path().to_string();
        let source = document.root().text().to_string();
        emit_diagnostics(&path, &source, errors, report_mode, colorize)
            .context("failed to emit diagnostics")?;
        return Err(anyhow!("source contains analysis errors").into());
    }

    let local_workflows = document.local_workflows().count();
    if local_workflows == 0 {
        return Err(anyhow!("document does not contain any workflows").into());
    } else if local_workflows > 1 && args.workflow.is_none() {
        return Err(anyhow!(
            "document contains multiple workflows, specify one by name with `--workflow`"
        )
        .into());
    }

    let ast_doc = document.root();
    let Ast::V1(v1) = ast_doc.ast() else {
        return Err(anyhow!("document does not use a supported WDL v1 version").into());
    };

    let workflow = match args.workflow {
        Some(wf) => v1
            .workflows()
            .find(|w| w.name().text() == wf)
            .ok_or_else(|| anyhow!("document does not contain a workflow named `{wf}`"))?,
        None => v1.workflows().next().expect("length verified before"),
    };

    let diagram = match args.kind {
        FlowchartType::Mermaid => mermaid::render(&workflow, &document, args.depth),
    };
    println!("{diagram}");
    Ok(())
}
