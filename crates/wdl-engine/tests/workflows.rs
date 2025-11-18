//! The WDL workflow file tests.
//!
//! This test looks for directories in `tests/workflows`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to evaluate; the file is expected to
//!   contain no static analysis errors, but may fail at evaluation time.
//! * `error.txt` - the expected evaluation error, if any.
//! * `inputs.json` - the inputs to the workflow.
//! * `outputs.json` - the expected outputs from the workflow, if the workflow
//!   runs successfully.
//!
//! The expected files may be automatically generated or updated by setting the
//! `BLESS` environment variable when running this test.

use std::env;
use std::path::Path;
use std::path::absolute;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use common::TestConfig;
use common::compare_result;
use common::find_tests;
use common::strip_paths;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use serde_json::to_string_pretty;
use tempfile::TempDir;
use tracing::info;
use tracing::level_filters::LevelFilter;
use wdl_analysis::Analyzer;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluationError;
use wdl_engine::Events;
use wdl_engine::Inputs;
use wdl_engine::path::EvaluationPath;
use wdl_engine::v1::TopLevelEvaluator;
use wdl_engine::v1::evaluate_workflow;

mod common;

/// Runs a single test.
fn run_test(test: &Path, config: TestConfig) -> BoxFuture<'_, Result<()>> {
    async move {
        let analyzer = Analyzer::new(config.analysis, |(), _, _, _| async {});
        analyzer
            .add_directory(test)
            .await
            .context("adding directory")?;
        let results = analyzer.analyze(()).await.context("running analysis")?;

        // Find the root source.wdl to evaluate
        let source_path = test.join("source.wdl");
        let Some(result) = results
            .iter()
            .find(|r| Some(r.document().path().as_ref()) == source_path.to_str())
        else {
            bail!("`source.wdl` was not found in the analysis results");
        };
        if let Some(e) = result.error() {
            bail!("parsing failed: {e:#}");
        }
        if result.document().has_errors() {
            bail!("test WDL contains errors; run a `check` on `source.wdl`");
        }

        let path = result.document().path();
        let diagnostics = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))],
            None => result.document().diagnostics().cloned().collect(),
        };

        if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
            bail!(EvaluationError::new(result.document().clone(), diagnostic.clone()).to_string());
        }

        let mut inputs = match Inputs::parse(result.document(), test.join("inputs.json"))? {
            Some((_, Inputs::Task(_))) => {
                bail!("`inputs.json` contains inputs for a task, not a workflow")
            }
            Some((_, Inputs::Workflow(inputs))) => inputs,
            None => Default::default(),
        };

        let test_dir = absolute(test).expect("failed to get absolute directory");
        let test_dir_path = EvaluationPath::Local(test_dir.clone());

        // Make any paths specified in the inputs file relative to the test directory
        let workflow = result
            .document()
            .workflow()
            .context("document does not contain a workflow")?;
        inputs.join_paths(workflow, |_| Ok(&test_dir_path)).await?;

        let mut dir = TempDir::new_in(env!("CARGO_TARGET_TMPDIR"))
            .context("failed to create temporary directory")?;
        if env::var_os("SPROCKET_TEST_KEEP_TMPDIRS").is_some() {
            dir.disable_cleanup(true);
            info!(dir = %dir.path().display(), "test temp dir created (will be kept)");
        } else {
            info!(dir = %dir.path().display(), "test temp dir created");
        }
        let evaluator = TopLevelEvaluator::new(
            dir.path(),
            config.engine,
            Default::default(),
            Events::none(),
        )
        .await?;
        match evaluate_workflow(&evaluator, result.document(), inputs.clone(), &dir).await {
            Ok(outputs) => {
                let outputs = outputs.with_name(workflow.name());
                let outputs = to_string_pretty(&outputs).context("failed to serialize outputs")?;
                let outputs = strip_paths(dir.path(), &outputs);
                compare_result(&test.join("outputs.json"), &outputs)?;
            }
            Err(e) => {
                let error = e.to_string();
                let error = strip_paths(dir.path(), &error);
                compare_result(&test.join("error.txt"), &error)?;
            }
        }

        Ok(())
    }
    .boxed()
}

fn main() -> Result<()> {
    // Default log level to off as some tests are designed to fail and we don't want
    // to log errors during the test
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::OFF.into())
                .from_env_lossy(),
        )
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(
        run_test,
        &Path::new("tests").join("workflows"),
        runtime.handle(),
    )?;
    libtest_mimic::run(&args, tests).exit();
}
