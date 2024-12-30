//! Implementation of the `validate-inputs` command.

use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::Buffer;
use url::Url;
use wdl::analysis::path_to_uri;
use wdl::ast::Diagnostic;
use wdl::ast::Severity;
use wdl::engine::Inputs;

use crate::analyze;

/// Arguments for the `validate-inputs` command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct ValidateInputsArgs {
    /// The path or URL to the WDL document.
    #[arg(required = true)]
    #[clap(value_name = "PATH or URL")]
    pub document: String,

    /// The path to the input JSON file.
    #[arg(short, long, value_name = "JSON")]
    pub inputs: PathBuf,
}

/// Validates the inputs for a task or workflow.
pub async fn validate_inputs(args: ValidateInputsArgs) -> anyhow::Result<()> {
    let ValidateInputsArgs { document, inputs } = args;

    if Path::new(&document).is_dir() {
        bail!("expected a WDL document, found a directory");
    }

    let results = analyze(&document, vec![], false, false).await?;

    let uri = if let Ok(uri) = Url::parse(&document) {
        uri
    } else {
        path_to_uri(&document).expect("file should be a local path")
    };

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context("failed to find document in analysis results")?;
    let analyzed_document = result.document();

    let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
        Some(e) => vec![Diagnostic::error(format!(
            "failed to read `{document}`: {e:#}"
        ))]
        .into(),
        None => analyzed_document.diagnostics().into(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        let source = result.document().node().syntax().text().to_string();
        let file = SimpleFile::new(&document, &source);

        let mut buffer = Buffer::no_color();
        emit(
            &mut buffer,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(),
        )
        .expect("should emit");

        let diagnostic: String = String::from_utf8(buffer.into_inner()).expect("should be UTF-8");
        bail!("document `{document}` contains at least one diagnostic error:\n{diagnostic}");
    }

    let result = match Inputs::parse(analyzed_document, inputs) {
        Ok(Some((name, inputs))) => match inputs {
            Inputs::Task(inputs) => {
                match inputs
                    .validate(
                        analyzed_document,
                        analyzed_document
                            .task_by_name(&name)
                            .expect("task should exist"),
                    )
                    .with_context(|| {
                        format!("failed to validate inputs for task `{name}`", name = name)
                    }) {
                    Ok(()) => String::new(),
                    Err(e) => format!("{e:?}"),
                }
            }
            Inputs::Workflow(inputs) => {
                let workflow = analyzed_document.workflow().expect("workflow should exist");
                match inputs
                    .validate(analyzed_document, workflow)
                    .with_context(|| {
                        format!(
                            "failed to validate inputs for workflow `{name}`",
                            name = name
                        )
                    }) {
                    Ok(()) => String::new(),
                    Err(e) => format!("{e:?}"),
                }
            }
        },
        Ok(None) => String::new(),
        Err(e) => format!("{e:?}"),
    };

    if !result.is_empty() {
        bail!("failed to validate inputs:\n{result}");
    } else {
        println!("inputs are valid");
    }

    anyhow::Ok(())
}
