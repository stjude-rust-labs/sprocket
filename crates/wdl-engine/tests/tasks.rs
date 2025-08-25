//! The WDL task file tests.
//!
//! This test looks for directories in `tests/tasks`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to evaluate; the file is expected to
//!   contain no static analysis errors, but may fail at evaluation time.
//! * `error.txt` - the expected evaluation error, if any.
//! * `inputs.json` - the inputs to the task.
//! * `outputs.json` - the expected outputs from the task, if the task runs
//!   successfully.
//! * `stdout` - the expected stdout from the task.
//! * `stderr` - the expected stderr from the task.
//! * `files` - a directory containing any expected files written by the task.
//!
//! The expected files may be automatically generated or updated by setting the
//! `BLESS` environment variable when running this test.

use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::absolute;
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use regex::Regex;
use serde_json::to_string_pretty;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;
use wdl_analysis::Analyzer;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::config::BackendConfig;
use wdl_engine::config::{self};
use wdl_engine::v1::TaskEvaluator;

/// Regex used to remove both host and guest path prefixes.
static PATH_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(attempts[\/\\]\d+[\/\\]|\/mnt\/task\/inputs\/\d+\/)"#).expect("invalid regex")
});

/// Regex used to replace temporary file names in task command files with
/// consistent names for test baselines.
static TEMP_FILENAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("tmp[[:alnum:]]{6}").expect("invalid regex"));

/// Find tests to run.
fn find_tests(runtime: &tokio::runtime::Handle) -> Result<Vec<Trial>, anyhow::Error> {
    let mut tests = vec![];
    for entry in Path::new("tests").join("tasks").read_dir().unwrap() {
        let entry = entry.expect("failed to read directory");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let test_name_base = path
            .file_stem()
            .map(OsStr::to_string_lossy)
            .unwrap()
            .into_owned();
        for (config_name, config) in
            configs(&path).with_context(|| format!("getting configs for {test_name_base}"))?
        {
            let test_runtime = runtime.clone();
            let test_path = path.clone();
            tests.push(Trial::test(
                format!("{test_name_base}_{config_name}"),
                move || Ok(test_runtime.block_on(run_test(&test_path, config))?),
            ));
        }
    }
    Ok(tests)
}

/// Gets the engine configurations to use for the test.
///
/// If the test directory contains any files that begin with `config` and end
/// with `.toml`, only those configs deserializable from those files will be
/// used. Otherwise, configs with default local and/or Docker backends will be
/// used, depending on the platform.
///
/// Note that there's nothing preventing this logic from being applied to the
/// other types of integration tests for this crate, but so far the need has
/// only arisen for testing tasks. Similarly, there may be other reasons beyond
/// `target_os` to filter particular configs.
fn configs(path: &Path) -> Result<Vec<(Cow<'static, str>, config::Config)>, anyhow::Error> {
    let mut configs_on_disk = vec![];
    let mut any_config_toml_found = false;
    for file in path.read_dir()? {
        let Ok(file) = file else {
            continue;
        };
        match file
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 filename for {file:?}"))?
        {
            "config.toml" => (),
            "config.linux.toml" if cfg!(target_os = "linux") => (),
            "config.macos.toml" if cfg!(target_os = "macos") => (),
            "config.windows.toml" if cfg!(target_os = "windows") => (),
            other => {
                // If there are any configs on disk, do not use the hardcoded ones even if our
                // particular configuration doesn't understand them
                if other.starts_with("config") && other.ends_with("toml") {
                    any_config_toml_found = true;
                }
                continue;
            }
        }
        let path = file.path();
        let config_name = path
            .file_stem()
            .expect("file should have a stem after the `match` above")
            .to_string_lossy()
            .into_owned()
            .into();
        let config = toml::from_str(&std::fs::read_to_string(file.path())?)?;
        configs_on_disk.push((config_name, config));
    }
    if !configs_on_disk.is_empty() || any_config_toml_found {
        Ok(configs_on_disk)
    } else {
        Ok(vec![
            ("local".into(), {
                config::Config {
                    backends: [(
                        "default".to_string(),
                        BackendConfig::Local(Default::default()),
                    )]
                    .into(),
                    suppress_env_specific_output: true,
                    ..Default::default()
                }
            }),
            // Currently we limit running the Docker backend to Linux as GitHub does not have
            // Docker installed on macOS hosted runners and the Windows hosted runners
            // are configured to use Windows containers
            #[cfg(target_os = "linux")]
            ("docker".into(), {
                config::Config {
                    backends: [(
                        "default".to_string(),
                        BackendConfig::Docker(Default::default()),
                    )]
                    .into(),
                    suppress_env_specific_output: true,
                    ..Default::default()
                }
            }),
        ])
    }
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

/// Runs a single test.
async fn run_test(test: &Path, config: config::Config) -> Result<()> {
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

    let (name, mut inputs) = match Inputs::parse(result.document(), test.join("inputs.json"))? {
        Some((name, Inputs::Task(inputs))) => (name, inputs),
        Some((_, Inputs::Workflow(_))) => {
            bail!("`inputs.json` contains inputs for a workflow, not a task")
        }
        None => {
            let mut iter = result.document().tasks();
            let name = iter
                .next()
                .context("inputs file is empty and the WDL document contains no tasks")?
                .name()
                .to_string();
            if iter.next().is_some() {
                bail!("inputs file is empty and the WDL document contains more than one task");
            }

            (name, Default::default())
        }
    };

    let test_dir = absolute(test).expect("failed to get absolute directory");

    // Make any paths specified in the inputs file relative to the test directory
    let task = result
        .document()
        .task_by_name(&name)
        .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;
    inputs.join_paths(task, |_| Ok(&test_dir))?;

    let evaluator = TaskEvaluator::new(config, CancellationToken::new(), None).await?;
    let dir = TempDir::new().context("failed to create temporary directory")?;
    match evaluator
        .evaluate(result.document(), task, &inputs, dir.path())
        .await
    {
        Ok(evaluated) => {
            compare_evaluation_results(&test_dir, dir.path(), &evaluated)?;

            match evaluated.into_result() {
                Ok(outputs) => {
                    let outputs = outputs.with_name(name.clone());
                    let outputs =
                        to_string_pretty(&outputs).context("failed to serialize outputs")?;
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
        Err(e) => {
            let error = e.to_string();
            let error = strip_paths(dir.path(), &error);
            compare_result(&test.join("error.txt"), &error)?;
        }
    }

    Ok(())
}

/// Compares the evaluation output files against the baselines.
fn compare_evaluation_results(
    test_dir: &Path,
    temp_dir: &Path,
    evaluated: &EvaluatedTask,
) -> Result<()> {
    let command_path = evaluated.attempt_dir().join("command");
    let command = fs::read_to_string(&command_path).with_context(|| {
        format!(
            "failed to read task command file `{path}`",
            path = command_path.display()
        )
    })?;

    let stdout_path = evaluated.stdout().as_file().unwrap();
    let stdout = fs::read_to_string(stdout_path.as_str())
        .with_context(|| format!("failed to read task stdout file `{stdout_path}`"))?;

    let stderr_path = evaluated.stderr().as_file().unwrap();
    let stderr = fs::read_to_string(stderr_path.as_str())
        .with_context(|| format!("failed to read task stderr file `{stderr_path}`"))?;

    // Strip both temp paths and test dir (input file) paths from the outputs
    let command = strip_paths(temp_dir, &command);
    let command = strip_paths(test_dir, &command);
    let mut command = PATH_PREFIX_REGEX.replace_all(&command, "");

    // Replace any temporary file names in the command
    for i in 0..usize::MAX {
        match TEMP_FILENAME_REGEX.replace(&command, format!("tmp{i}")) {
            Cow::Borrowed(_) => break,
            Cow::Owned(s) => command = s.into(),
        }
    }

    compare_result(&test_dir.join("command"), &command)?;

    let stdout = strip_paths(temp_dir, &stdout);
    let stdout = strip_paths(test_dir, &stdout);
    compare_result(&test_dir.join("stdout"), &stdout)?;

    let stderr = strip_paths(temp_dir, &stderr);
    let stderr = strip_paths(test_dir, &stderr);
    compare_result(&test_dir.join("stderr"), &stderr)?;

    // Compare expected output files
    let mut had_files = false;
    let files_dir = test_dir.join("files");
    for entry in WalkDir::new(
        evaluated
            .work_dir()
            .as_local()
            .expect("work dir should be local"),
    ) {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory `{path}`",
                path = evaluated.work_dir().display()
            )
        })?;
        let metadata = entry.metadata().with_context(|| {
            format!(
                "failed to read metadata of `{path}`",
                path = entry.path().display()
            )
        })?;
        if !metadata.is_file() {
            continue;
        }

        had_files = true;

        let contents = fs::read_to_string(entry.path()).with_context(|| {
            format!(
                "failed to read file `{path}`",
                path = entry.path().display()
            )
        })?;
        let expected_path = files_dir.join(
            entry
                .path()
                .strip_prefix(
                    evaluated
                        .work_dir()
                        .as_local()
                        .expect("should be local path"),
                )
                .unwrap_or(entry.path()),
        );
        fs::create_dir_all(
            expected_path
                .parent()
                .expect("should have parent directory"),
        )
        .context("failed to create output file directory")?;
        compare_result(&expected_path, &contents)?;
    }

    // Look for missing output files
    if files_dir.exists() {
        for entry in WalkDir::new(&files_dir) {
            let entry = entry.with_context(|| {
                format!(
                    "failed to read directory `{path}`",
                    path = files_dir.display()
                )
            })?;
            let metadata = entry.metadata().with_context(|| {
                format!(
                    "failed to read metadata of `{path}`",
                    path = entry.path().display()
                )
            })?;
            if !metadata.is_file() {
                continue;
            }

            let relative_path = entry
                .path()
                .strip_prefix(&files_dir)
                .unwrap_or(entry.path());
            let expected_path = evaluated
                .work_dir()
                .join(relative_path.to_str().unwrap())?
                .unwrap_local();
            if !expected_path.is_file() {
                bail!(
                    "task did not produce expected output file `{path}`",
                    path = relative_path.display()
                );
            }
        }
    } else if had_files {
        bail!(
            "task generated files in the working directory that are not present in a `files` \
             subdirectory"
        );
    }

    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle())?;
    libtest_mimic::run(&args, tests).exit();
}
