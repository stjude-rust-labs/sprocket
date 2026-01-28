//! CLI Tests
//!
//! This test looks for command files in the `tests/cli` directory to run with
//! Sprocket.
//!
//! These directories can be arbitrarily nested to group similar tests together.
//!
//! Each test can contain the following files (but all are optional):
//!   * `args` - entrypoint of each test; contains the arguments to pass to
//!     `sprocket` (without "sprocket").
//!   * `inputs` - a directory containing the starting files that the test will
//!     run with. The contents of this directory are copied to a temp directory
//!     and the temporary directory used as the command's working directory.
//!   * `outputs` - a directory containing the expected ending files that the
//!     temp directory will contain. If a test does not need to verify the
//!     resulting directory contents, it may omit an `outputs` directory.
//!   * `stdout` - the expected stdout from the task.
//!   * `stderr` - the expected stderr from the task.
//!   * `exit_code` - the expected exit code from the task.
//!
//! The expected files may be automatically generated or updated by setting the
//! `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::process::exit;
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use colored::Colorize;
use futures::StreamExt;
use futures::stream;
use pretty_assertions::StrComparison;
use tempfile::NamedTempFile;
use tempfile::TempDir;
use tokio::fs;
use tracing::debug;
use walkdir::WalkDir;

/// Finds tests at the given root directory.
fn find_tests(starting_dir: &Path) -> Vec<PathBuf> {
    let mut tests: Vec<PathBuf> = Vec::new();
    for entry in starting_dir.read_dir().unwrap() {
        let entry = entry.expect("failed to read directory");
        let path = entry.path();
        if path.extension() == Some(OsStr::new("disabled")) {
            continue;
        }
        if path.is_dir() {
            tests.append(&mut find_tests(path.as_path()));
        } else if path.file_name().unwrap() == "args" {
            tests.push(path.parent().unwrap().to_path_buf());
        }
    }
    tests.sort();
    tests
}

/// Gets the name of a test given the directory.
fn get_test_name(path: &Path, test_root: &Path) -> String {
    let root_path = test_root.as_os_str().to_str().unwrap();
    path.as_os_str().to_str().unwrap()[root_path.len() + 1..].to_string()
}

/// Represents output of a test command.
struct CommandOutput {
    /// The stdout of the command.
    stdout: String,
    /// The stderr of the command.
    stderr: String,
    /// The command's exit code.
    exit_code: i32,
}

/// Runs a test given the test root directory.
async fn run_test(test_path: &Path) -> Result<()> {
    let working_test_directory = setup_working_test_directory(test_path)
        .await
        .context("failed to setup working test directory")?;
    let command_output = run_sprocket(test_path, working_test_directory.path())
        .await
        .context("failed to run sprocket command")?;
    compare_test_results(test_path, working_test_directory.path(), &command_output).await
}

/// Sets up the working test directory by copying initial files.
async fn setup_working_test_directory(test_path: &Path) -> Result<TempDir> {
    let inputs_directory = test_path.join("inputs");
    let working_test_directory = TempDir::new().context("failed to create temp directory")?;
    if inputs_directory.exists() {
        recursive_copy(&inputs_directory, working_test_directory.path())
            .await
            .context("failed to copy input files to temp directory")?;
    }
    Ok(working_test_directory)
}

/// Recursively copies the source path to the target path.
async fn recursive_copy(source: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        fs::create_dir_all(target)
            .await
            .with_context(|| format!("failed to create target directory {target:?}"))
            .with_context(|| format!("failed to create base directory at {target:?}"))?;
    }
    for entry in WalkDir::new(source).into_iter() {
        let entry = entry?;
        let from = entry.path();
        let normalized_relative_path = from
            .strip_prefix(source)
            .context("failed to strip path prefix from source")?;
        let to = target.join(normalized_relative_path);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&to)
                .await
                .with_context(|| format!("failed to create directory at {:?}", &to))?;
        } else {
            fs::copy(&from, &to)
                .await
                .with_context(|| format!("failed to copy file to {:?}", &to))?;
        }
    }
    Ok(())
}

/// Runs sprocket for a test.
async fn run_sprocket(test_path: &Path, working_test_directory: &Path) -> Result<CommandOutput> {
    debug!(test_path = %test_path.display(), "running Sprocket for test");
    let sprocket_exe = PathBuf::from(env!("CARGO_BIN_EXE_sprocket"));
    let args_path = test_path.join("args");
    let args_string = fs::read_to_string(&args_path)
        .await
        .with_context(|| format!("failed to read command at path {:?}", &args_path))?;
    let args = shlex::split(&format!("--skip-config-search {args_string}"))
        .ok_or_else(|| anyhow!("failed to split command args"))?;
    let mut command = Command::new(sprocket_exe);

    let env_config = resolve_env_config(test_path).await?;
    if let Some(env_config) = env_config.as_ref() {
        // If an overridden config has been specified via environment variables,
        // synthesize a Sprocket config with that config.
        command.arg("--config");
        command.arg(env_config.path());
    }

    command.current_dir(working_test_directory).args(args);
    let result = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("RUST_LOG", "none")
        .env_remove("RUST_BACKTRACE")
        .spawn()
        .context("failed to spawn command")?
        .wait_with_output()
        .context("failed while waiting for command to finish")?;

    // Make sure the temporary config isn't dropped prematurely
    drop(env_config);

    Ok(CommandOutput {
        stdout: String::from_utf8(result.stdout).context("failed to convert stdout to string")?,
        stderr: String::from_utf8(result.stderr).context("failed to convert stderr to string")?,
        exit_code: result
            .status
            .code()
            .ok_or_else(|| anyhow!("failed to get status code"))?,
    })
}

/// Resolve a config file that incorporates optional overrides specified in
/// environment variables.
///
/// The current environment variables supported are:
///
/// - `SPROCKET_TEST_ENGINE_CONFIG`: a TOML-serialized [`wdl::engine::Config`]
///   that will be substituted in place of the default engine config for all of
///   the `run/` tests.
async fn resolve_env_config(test_path: &Path) -> Result<Option<NamedTempFile>> {
    let mut config_overridden = false;
    let mut sprocket_config = sprocket::Config::default();
    // For `run` tests, allow overriding the engine config. We restrict the override
    // to this subset of tests in order to avoid messing with the expected output
    // for commands that format the config and therefore expect the config to be
    // exactly the default.
    if test_path.starts_with("tests/cli/run")
        && let Some(env_config) = env::var_os("SPROCKET_TEST_ENGINE_CONFIG")
    {
        sprocket_config.run.engine = toml::from_str(&fs::read_to_string(env_config).await?)?;
        config_overridden = true;
    }

    if !config_overridden {
        Ok(None)
    } else {
        let temp_config = tempfile::NamedTempFile::new()?;
        sprocket_config.write_config(&temp_config.path().display().to_string())?;
        Ok(Some(temp_config))
    }
}

/// Normalizes a string for OS platform differences.
fn normalize_string(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .replace("\\r\\n", "\\n")
        .replace("sprocket.exe", "sprocket")
        .replace("\\", "/")
        .replace("//", "/")
        .to_string()
}

/// Compares the contents in the expected file with the actual test results.
async fn compare_results(expected_path: &Path, actual: &str) -> Result<()> {
    let expected = fs::read_to_string(expected_path)
        .await
        .with_context(|| format!("failed to read result file {expected_path:?}"))?;

    let expected = normalize_string(&expected);
    let actual = normalize_string(actual);
    if expected != actual {
        eprintln!("expected:{expected:?}");
        eprintln!("actual:{actual:?}");
        bail!(
            "result from `{}` is not as expected:\nafter normalization:\n{}",
            expected_path.display(),
            StrComparison::new(&expected, &actual)
        )
    }

    Ok(())
}

/// Compares the contents of two text files.
async fn compare_files(expected_path: &Path, actual_path: &Path) -> Result<()> {
    let actual = fs::read_to_string(actual_path)
        .await
        .with_context(|| format!("failed to read actual file {actual_path:?}"))?;
    compare_results(expected_path, &actual).await
}

/// Builds a list of entry paths in a directory relative to the directory's
/// path.
fn build_relative_path_list(path: &Path) -> Result<Vec<PathBuf>> {
    let mut path_list = Vec::new();
    if path.exists() {
        for entry in WalkDir::new(path).into_iter() {
            let entry = entry?;
            let normalized_relative_path = entry
                .path()
                .strip_prefix(path)
                .context("failed to strip path prefix from source")?;
            if !entry.file_type().is_dir() {
                path_list.push(normalized_relative_path.to_path_buf());
            }
        }
        path_list.sort();
    }
    Ok(path_list)
}

/// Gets the paths only in the left side.
fn get_paths_only_owned_by_left_side(left: &[PathBuf], right: &[PathBuf]) -> Vec<PathBuf> {
    left.iter()
        .filter(|entry| !right.contains(entry))
        .cloned()
        .collect()
}

/// Gets the paths only in the right side.
fn get_paths_shared_by_left_and_right_sides(left: &[PathBuf], right: &[PathBuf]) -> Vec<PathBuf> {
    left.iter()
        .filter(|entry| right.contains(entry))
        .cloned()
        .collect()
}

/// Recursively compares the contents of two paths.
async fn recursive_compare(expected_path: &Path, actual_path: &Path) -> Result<()> {
    let expected_relative_path_list = build_relative_path_list(expected_path)?;
    let actual_relative_path_list = build_relative_path_list(actual_path)?;
    if expected_relative_path_list != actual_relative_path_list {
        let matches_found = get_paths_shared_by_left_and_right_sides(
            &expected_relative_path_list,
            &actual_relative_path_list,
        );
        let expected_but_not_found = get_paths_only_owned_by_left_side(
            &expected_relative_path_list,
            &actual_relative_path_list,
        );
        let unexpected_files = get_paths_only_owned_by_left_side(
            &actual_relative_path_list,
            &expected_relative_path_list,
        );
        bail!(
            r#"expected and actual outputs do not contain the same files
__MATCHES_FOUND__
{:?}

__EXPECTED_BUT_NOT_FOUND__
{:?}

__UNEXPECTED_FILES_FOUND__
{:?}"#,
            matches_found,
            expected_but_not_found,
            unexpected_files
        );
    } else {
        let mut failed_comparisons = Vec::new();
        for relative_path in expected_relative_path_list {
            let expected_full_path = expected_path.join(&relative_path);
            let actual_full_path = actual_path.join(&relative_path);
            if let Err(result) = compare_files(&expected_full_path, &actual_full_path).await {
                failed_comparisons.push(result);
            }
        }
        if !failed_comparisons.is_empty() {
            let error_strings: Vec<String> = failed_comparisons
                .iter()
                .map(|error| error.to_string())
                .collect();
            bail!(format!(
                "Output files did not match: \n{}",
                error_strings.join("\n")
            ));
        }
    }
    Ok(())
}

/// Compares the result of the command output with the expected baseline.
async fn compare_test_results(
    test_path: &Path,
    working_test_directory: &Path,
    command_output: &CommandOutput,
) -> Result<()> {
    let expected_output_dir = test_path.join("outputs");
    let expected_stderr_file = test_path.join("stderr");
    let expected_stdout_file = test_path.join("stdout");
    let expected_exit_code_file = test_path.join("exit_code");
    let expects_outputs = expected_output_dir.is_dir();

    if env::var_os("BLESS").is_some() {
        fs::write(&expected_stderr_file, &command_output.stderr)
            .await
            .context("failed to write stderr output")?;
        fs::write(&expected_stdout_file, &command_output.stdout)
            .await
            .context("failed to write stdout output")?;
        fs::remove_dir_all(&expected_output_dir)
            .await
            .unwrap_or_default();
        fs::write(
            &expected_exit_code_file,
            &command_output.exit_code.to_string(),
        )
        .await
        .context("failed to write exit code")?;

        if expects_outputs {
            recursive_copy(working_test_directory, &expected_output_dir)
                .await
                .context(
                    "failed to copy output files from test results to setup new expected outputs",
                )?;
        }
    }
    compare_results(&expected_stderr_file, &command_output.stderr).await?;
    compare_results(&expected_stdout_file, &command_output.stdout).await?;

    if expects_outputs {
        recursive_compare(&expected_output_dir, working_test_directory).await?;
    }

    compare_results(
        &expected_exit_code_file,
        &command_output.exit_code.to_string(),
    )
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let test_root = Path::new("tests/cli");
    let tests = find_tests(test_root);
    let mut futures = Vec::new();
    let mut errors = Vec::new();
    for test in &tests {
        let test_name = get_test_name(test, test_root);
        futures.push(async { (test_name, run_test(test).await) })
    }

    let mut stream = stream::iter(futures)
        .buffer_unordered(available_parallelism().map(Into::into).unwrap_or(1));
    while let Some((test_name, result)) = stream.next().await {
        match result {
            Ok(_) => {
                println!("test {test_name} ... {ok}", ok = "ok".green());
            }
            Err(e) => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((test_name, format!("{e:#}")));
            }
        }
    }

    if !errors.is_empty() {
        eprintln!(
            "\n{count} test(s) {failed}:",
            count = errors.len(),
            failed = "failed".red()
        );

        for (name, msg) in errors.iter() {
            eprintln!("{name}: {msg}", msg = msg.red());
        }

        exit(1);
    } else {
        println!("\ntest result: ok. {count} passed\n", count = tests.len());
    }
}
