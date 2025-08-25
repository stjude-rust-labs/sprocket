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

use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::absolute;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use serde_json::to_string_pretty;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use wdl_analysis::Analyzer;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::config;
use wdl_engine::config::BackendConfig;
use wdl_engine::v1::WorkflowEvaluator;

/// Find tests to run.
fn find_tests(runtime: &tokio::runtime::Handle) -> Vec<Trial> {
    Path::new("tests")
        .join("workflows")
        .read_dir()
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.expect("failed to read directory");
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }

            let test_name = path
                .file_stem()
                .map(OsStr::to_string_lossy)
                .unwrap()
                .into_owned();
            let test_runtime = runtime.clone();
            Some(Trial::test(test_name, move || {
                Ok(test_runtime.block_on(run_test(&path))?)
            }))
        })
        .collect()
}

/// Strips paths from the given string.
fn strip_paths(root: &Path, s: &str) -> String {
    #[cfg(windows)]
    {
        // First try it with a single slash
        let mut pattern = root.to_str().expect("path is not UTF-8").to_string();
        if !pattern.ends_with('\\') {
            pattern.push('\\');
        }

        // Next try with double slashes in case there were escaped backslashes
        let s = s.replace(&pattern, "");
        let pattern = pattern.replace('\\', "\\\\");
        s.replace(&pattern, "")
    }

    #[cfg(unix)]
    {
        let mut pattern = root.to_str().expect("path is not UTF-8").to_string();
        if !pattern.ends_with('/') {
            pattern.push('/');
        }

        s.replace(&pattern, "")
    }
}

/// Normalizes a result.
fn normalize(s: &str) -> String {
    // Normalize paths separation characters first
    s.replace("\\\\", "/")
        .replace("\\", "/")
        .replace("\r\n", "\n")
}

/// Compares a single result.
fn compare_result(path: &Path, result: &str) -> Result<()> {
    let result = normalize(result);
    if env::var_os("BLESS").is_some() {
        fs::write(path, &result).with_context(|| {
            format!(
                "failed to write result file `{path}`",
                path = path.display()
            )
        })?;
        return Ok(());
    }

    let expected = fs::read_to_string(path)
        .with_context(|| {
            format!(
                "failed to read result file `{path}`: expected contents to be `{result}`",
                path = path.display()
            )
        })?
        .replace("\r\n", "\n");

    if expected != result {
        bail!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}

/// Gets the engine configurations to use for the test.
fn configs() -> Vec<config::Config> {
    vec![
        {
            config::Config {
                backends: [(
                    "default".to_string(),
                    BackendConfig::Local(Default::default()),
                )]
                .into(),
                ..Default::default()
            }
        },
        // Currently we limit running the Docker backend to Linux as GitHub does not have Docker
        // installed on macOS hosted runners and the Windows hosted runners are configured to use
        // Windows containers
        #[cfg(target_os = "linux")]
        {
            config::Config {
                backends: [(
                    "default".to_string(),
                    BackendConfig::Docker(Default::default()),
                )]
                .into(),
                ..Default::default()
            }
        },
    ]
}

/// Runs a single test.
async fn run_test(test: &Path) -> Result<()> {
    let analyzer = Analyzer::default();
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
    let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
        None => result.document().diagnostics().into(),
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

    // Make any paths specified in the inputs file relative to the test directory
    let workflow = result
        .document()
        .workflow()
        .context("document does not contain a workflow")?;
    inputs.join_paths(workflow, |_| Ok(&test_dir))?;

    for config in configs() {
        let dir = TempDir::new().context("failed to create temporary directory")?;
        let evaluator = WorkflowEvaluator::new(config, CancellationToken::new(), None).await?;
        match evaluator
            .evaluate(result.document(), inputs.clone(), &dir)
            .await
        {
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
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle());
    libtest_mimic::run(&args, tests).exit();
}
