//! CLI Tests
//!
//! This tests looks for "sprocket_command" files in the `/tests/cli` directory
//! These directories can be arbitrarily nested to group similar tests together
//!
//! Each test can contain the following files (but all are optional)
//! * `sprocket_command` - entrypoint of each test, contains a sprocket command
//!   that will be
//! run (without the sprocket keyword)
//! * `inputs` - a directory containing the starting files that the test will
//!   run with.
//! These are copied to a temp folder, and the command above will be run inside
//! this temp folder.
//! * `outputs` - a directory containing the expected ending files that the temp
//!   folder will
//! end up with. These often will be a copy of the inputs directory if the
//! command does not change the input files.
//! * `stdout` - the expected stdout from the task
//! * `stderr` - the expected stderr from the task
//!
//! The expected files may be automatically generated or updated by setting the
//! `BLESS` environment variable when running this test.

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use assert_cmd::Command;
use colored::Colorize;
use futures::StreamExt;
use futures::stream;
use pretty_assertions::StrComparison;
use tempfile::TempDir;
use tokio::fs;
use walkdir::WalkDir;

fn find_tests(starting_dir: &Path) -> Vec<PathBuf> {
    let mut tests: Vec<PathBuf> = Vec::new();
    for entry in starting_dir.read_dir().unwrap() {
        let entry = entry.expect("failed to read directory");
        let path = entry.path();
        if path.is_dir() {
            tests.append(&mut find_tests(path.as_path()));
        } else if path.file_name().unwrap() == "sprocket_command" {
            tests.push(path.parent().unwrap().to_path_buf());
        }
    }
    tests.sort();
    tests
}

fn get_test_name(path: &Path, test_root: &Path) -> String {
    let root_path = test_root.as_os_str().to_str().unwrap();
    path.as_os_str().to_str().unwrap()[root_path.len() + 1..].to_string()
}

struct CommandOutput {
    stdout: String,
    stderr: String,
}

async fn run_test(test_path: &Path) -> Result<()> {
    let working_test_directory = setup_working_test_directory(test_path)
        .await
        .context("failed to setup working test directory")?;
    let command_output = run_sprocket(test_path, &working_test_directory.path())
        .await
        .context("failed to run sproket command")?;
    compare_test_results(test_path, &working_test_directory.path(), &command_output).await
}

async fn setup_working_test_directory(test_path: &Path) -> Result<TempDir> {
    let inputs_directory = test_path.join("inputs");
    let working_test_directory = TempDir::new().context("failed to create temp directory")?;
    if inputs_directory.exists() {
        recursive_copy(&inputs_directory, &working_test_directory.path())
            .await
            .context("failed to copy input files to temp directory")?;
    }
    Ok(working_test_directory)
}

async fn recursive_copy(source: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        fs::create_dir_all(target)
            .await
            .context(format!("failed to create target directory {:?}", target))
            .context(format!("failed to create base directory at {:?}", target))?;
    }
    for entry in WalkDir::new(&source).into_iter() {
        let entry = entry?;
        let from = entry.path();
        let normalized_relative_path = from
            .strip_prefix(&source)
            .context("failed to strip path prefix from source")?;
        let to = target.join(normalized_relative_path);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&to)
                .await
                .context(format!("failed to create directory at {:?}", &to))?;
        } else {
            fs::copy(&from, &to)
                .await
                .context(format!("failed to copy file to {:?}", &to))?;
        }
    }
    Ok(())
}

async fn run_sprocket(test_path: &Path, working_test_directory: &Path) -> Result<CommandOutput> {
    let command_path = test_path.join("sprocket_command");
    let command_string = fs::read_to_string(&command_path).await.context(format!(
        "failed to read command at path {:?}",
        &command_path
    ))?;
    let command_input = shlex::split(&command_string).unwrap();
    let mut command = Command::cargo_bin("sprocket")?;
    command
        .current_dir(working_test_directory)
        .args(command_input);

    let command_assert = command.assert();
    let command_output = CommandOutput {
        stdout: String::from_utf8(command_assert.get_output().stdout.clone())
            .context("failed to get stdout from sprocket command")?,
        stderr: String::from_utf8(command_assert.get_output().stderr.clone())
            .context("failed to get stderr from sprocket command")?,
    };

    Ok(command_output)
}

async fn compare_results(expected_path: &Path, actual: &str) -> Result<()> {
    let result = fs::read_to_string(expected_path).await;
    let expected = result.context(format!("failed to read result file {:?}", expected_path))?;
    if expected != actual.to_string() {
        Err(anyhow!(
            "result from `{}` is not as expected: \n{}",
            expected_path.display(),
            StrComparison::new(&expected, &actual)
        ))
    } else {
        Ok(())
    }
}

async fn compare_files(expected_path: &Path, actual_path: &Path) -> Result<()> {
    let actual = fs::read_to_string(actual_path)
        .await
        .context(format!("failed to read actual file {:?}", actual_path))?;
    compare_results(expected_path, &actual).await
}

fn build_relative_path_list(path: &Path) -> Result<Vec<PathBuf>> {
    let mut path_list = Vec::new();
    if path.exists() {
        for entry in WalkDir::new(&path).into_iter() {
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

fn get_paths_only_owned_by_left_side(left: &Vec<PathBuf>, right: &Vec<PathBuf>) -> Vec<PathBuf> {
    left.iter()
        .filter(|entry| !right.contains(entry).clone())
        .map(|entry| entry.clone())
        .collect()
}

fn get_paths_shared_by_left_and_right_sides(
    left: &Vec<PathBuf>,
    right: &Vec<PathBuf>,
) -> Vec<PathBuf> {
    left.iter()
        .filter(|entry| right.contains(entry).clone())
        .map(|entry| entry.clone())
        .collect()
}

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

async fn compare_test_results(
    test_path: &Path,
    working_test_directory: &Path,
    command_output: &CommandOutput,
) -> Result<()> {
    let expected_output_folder = test_path.join("outputs");
    let expected_stderr_file = test_path.join("stderr");
    let expected_stdout_file = test_path.join("stdout");
    if env::var_os("BLESS").is_some() {
        fs::write(&expected_stderr_file, command_output.stderr.as_bytes())
            .await
            .context("failed to write stderr output")?;
        fs::write(&expected_stdout_file, command_output.stdout.as_bytes())
            .await
            .context("failed to write stdout output")?;
        fs::remove_dir_all(&expected_output_folder)
            .await
            .unwrap_or_default();
        recursive_copy(working_test_directory, &expected_output_folder)
            .await
            .context(
                "failed to copy output files from test results to setup new expected outputs",
            )?;
    }
    compare_results(&expected_stderr_file, &command_output.stderr).await?;
    compare_results(&expected_stdout_file, &command_output.stdout).await?;
    recursive_compare(&expected_output_folder, working_test_directory).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
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
                errors.push((test_name, format!("{e:?}")));
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
