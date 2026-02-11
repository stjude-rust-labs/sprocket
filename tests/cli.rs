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
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use regex::Regex;
use tempfile::NamedTempFile;
use tempfile::TempDir;
use tracing::debug;
use walkdir::WalkDir;

/// Regex pattern for timestamp directories (e.g., `2026-02-05_123456789012`).
static TIMESTAMP_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}_\d{12,}").unwrap());

/// Regex pattern for UUIDs.
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .unwrap()
});

/// Binary file extensions that should only be checked for existence.
const BINARY_EXTENSIONS: &[&str] = &["db", "sqlite", "sqlite3"];

/// Transient file suffixes that should be removed during `BLESS`.
const TRANSIENT_SUFFIXES: &[&str] = &["-shm", "-wal"];

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
            // The following tests require Docker, so skip if the Docker tests are disabled
            #[cfg(docker_tests_disabled)]
            match path.file_name().and_then(|n| n.to_str()) {
                Some("run") | Some("test") => continue,
                _ => {}
            }

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
fn run_test(test_path: &Path) -> Result<()> {
    let working_test_directory = setup_working_test_directory(test_path)
        .context("failed to setup working test directory")?;
    let command_output = run_sprocket(test_path, working_test_directory.path())
        .context("failed to run sprocket command")?;
    compare_test_results(test_path, working_test_directory.path(), &command_output)
}

/// Sets up the working test directory by copying initial files.
fn setup_working_test_directory(test_path: &Path) -> Result<TempDir> {
    let inputs_directory = test_path.join("inputs");
    let working_test_directory = TempDir::new().context("failed to create temp directory")?;
    if inputs_directory.exists() {
        recursive_copy(&inputs_directory, working_test_directory.path())
            .context("failed to copy input files to temp directory")?;
    }
    Ok(working_test_directory)
}

/// Recursively copies the source path to the target path.
///
/// Symlinks are recreated as symlinks pointing to the same relative target.
fn recursive_copy(source: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        fs::create_dir_all(target)
            .with_context(|| format!("failed to create target directory {target:?}"))?;
    }
    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        let entry = entry?;
        let from = entry.path();
        let file_type = entry.file_type();
        let normalized_relative_path = from
            .strip_prefix(source)
            .context("failed to strip path prefix from source")?;
        let to = target.join(normalized_relative_path);

        if file_type.is_dir() {
            fs::create_dir_all(&to)
                .with_context(|| format!("failed to create directory at {:?}", &to))?;
        } else if file_type.is_symlink() {
            // Recreate symlink with same target
            let link_target = fs::read_link(from)
                .with_context(|| format!("failed to read symlink at {:?}", from))?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &to)
                .with_context(|| format!("failed to create symlink at {:?}", &to))?;
            #[cfg(windows)]
            {
                if link_target.is_dir() {
                    std::os::windows::fs::symlink_dir(&link_target, &to)
                        .with_context(|| format!("failed to create symlink at {:?}", &to))?;
                } else {
                    std::os::windows::fs::symlink_file(&link_target, &to)
                        .with_context(|| format!("failed to create symlink at {:?}", &to))?;
                }
            }
        } else {
            fs::copy(from, &to).with_context(|| format!("failed to copy file to {:?}", &to))?;
        }
    }
    Ok(())
}

/// Runs sprocket for a test.
fn run_sprocket(test_path: &Path, working_test_directory: &Path) -> Result<CommandOutput> {
    debug!(test_path = %test_path.display(), "running Sprocket for test");
    let sprocket_exe = PathBuf::from(env!("CARGO_BIN_EXE_sprocket"));
    let args_path = test_path.join("args");
    let args_string = fs::read_to_string(&args_path)
        .with_context(|| format!("failed to read command at path {:?}", &args_path))?;
    let args_string = args_string.replace("\r\n", "\n");
    let args = shlex::split(&format!("--skip-config-search {args_string}"))
        .ok_or_else(|| anyhow!("failed to split command args"))?;
    let mut command = Command::new(sprocket_exe);

    let env_config = resolve_env_config(test_path)?;
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
fn resolve_env_config(test_path: &Path) -> Result<Option<NamedTempFile>> {
    let mut config_overridden = false;
    let mut sprocket_config = sprocket::Config::default();
    // For `run` tests, allow overriding the engine config. We restrict the override
    // to this subset of tests in order to avoid messing with the expected output
    // for commands that format the config and therefore expect the config to be
    // exactly the default.
    if test_path.starts_with("tests/cli/run")
        && let Some(env_config) = env::var_os("SPROCKET_TEST_ENGINE_CONFIG")
    {
        sprocket_config.run.engine = toml::from_str(&fs::read_to_string(env_config)?)?;
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

/// Normalizes a string for OS platform differences and dynamic content.
fn normalize_string(input: &str) -> String {
    // NOTE: the drive prefix removal (e.g., `C:`) must occur after backslash
    // normalization so that paths like `C:\foo` are first converted to `C:/foo`
    // before the prefix is stripped.
    let s = input
        .replace("\r\n", "\n")
        .replace("\\r\\n", "\\n")
        .replace("sprocket.exe", "sprocket")
        .replace("\\", "/")
        .replace("//", "/");

    // Strip Windows drive prefixes (e.g., `C:`) from absolute paths.
    static DRIVE_PREFIX: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"[A-Za-z]:(/[^\s])").unwrap());
    let s = DRIVE_PREFIX.replace_all(&s, "$1");

    let s = UUID_PATTERN.replace_all(&s, "_UUID_");
    let s = TIMESTAMP_PATTERN.replace_all(&s, "_TIMESTAMP_");
    s.to_string()
}

/// Normalizes a path by replacing dynamic components (timestamps) with
/// placeholders.
fn normalize_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    let normalized = TIMESTAMP_PATTERN.replace_all(&path_str, "_TIMESTAMP_");
    PathBuf::from(normalized.as_ref())
}

/// Returns true if the file is a binary file that should only be checked for
/// existence.
fn is_binary_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| BINARY_EXTENSIONS.contains(&ext))
}

/// Returns true if the path is a symlink.
fn is_symlink(base_path: &Path, relative_path: &Path) -> bool {
    let full_path = base_path.join(relative_path);
    full_path.symlink_metadata().is_ok_and(|m| m.is_symlink())
}

/// Normalizes the expected outputs directory for stable comparison.
///
/// This function:
///
/// 1. Removes transient files (e.g., SQLite `-shm`, `-wal` files)
/// 2. Zeroes out binary files so they exist but have no content to compare
/// 3. Updates symlink targets to replace timestamps with `_TIMESTAMP_`
/// 4. Renames timestamp directories to `_TIMESTAMP_`
fn normalize_expected_outputs(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // Remove transient files and zero out binary files
    for entry in WalkDir::new(path).follow_links(false).into_iter() {
        let entry = entry?;
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();

            if TRANSIENT_SUFFIXES
                .iter()
                .any(|suffix| name.ends_with(suffix))
            {
                fs::remove_file(entry.path())?;
            } else if is_binary_file(entry.path()) {
                fs::write(entry.path(), b"")?;
            }
        }
    }

    // Update symlink targets to normalize timestamps
    for entry in WalkDir::new(path).follow_links(false).into_iter() {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            let link_path = entry.path();
            let target = fs::read_link(link_path)?;
            let target_str = target.to_string_lossy();
            if TIMESTAMP_PATTERN.is_match(&target_str) {
                let normalized_target = TIMESTAMP_PATTERN.replace_all(&target_str, "_TIMESTAMP_");
                fs::remove_file(link_path)?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(normalized_target.as_ref(), link_path)?;
                #[cfg(windows)]
                {
                    let normalized_path = PathBuf::from(normalized_target.as_ref());
                    if normalized_path.is_dir() {
                        std::os::windows::fs::symlink_dir(normalized_target.as_ref(), link_path)?;
                    } else {
                        std::os::windows::fs::symlink_file(normalized_target.as_ref(), link_path)?;
                    }
                }
            }
        }
    }

    // Collect directories to rename (depth-first to handle nested timestamps)
    let dirs_to_rename = WalkDir::new(path)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_dir() && TIMESTAMP_PATTERN.is_match(&e.file_name().to_string_lossy())
        })
        .map(|e| e.path().to_path_buf())
        .collect::<Vec<_>>();

    // Rename directories
    for dir in dirs_to_rename {
        let parent = dir.parent().context("directory should have parent")?;
        let new_path = parent.join("_TIMESTAMP_");
        if new_path.exists() {
            fs::remove_dir_all(&new_path)?;
        }
        fs::rename(&dir, &new_path)?;
    }

    Ok(())
}

/// Compares the contents in the expected file with the actual test results.
fn compare_results(expected_path: &Path, actual: &str) -> Result<()> {
    let expected = fs::read_to_string(expected_path)
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
fn compare_files(expected_path: &Path, actual_path: &Path) -> Result<()> {
    let actual = fs::read_to_string(actual_path)
        .with_context(|| format!("failed to read actual file {actual_path:?}"))?;
    compare_results(expected_path, &actual)
}

/// Builds a list of entry paths in a directory relative to the directory's
/// path. Paths are normalized to replace dynamic components (e.g., timestamps)
/// with placeholders like `_TIMESTAMP_` for stable comparison.
///
/// Symlinks are included for existence checking but their content is not
/// compared. Binary files are similarly included for existence but skipped for
/// content comparison. Transient files (e.g., SQLite `-shm`, `-wal`) are
/// excluded entirely.
///
/// Returns a list of tuples: `(normalized_path, original_path)` so we can use
/// the original path to read file contents while comparing normalized paths.
fn build_relative_path_list(path: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let is_transient = |name: &str| {
        TRANSIENT_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
    };

    let mut path_list = WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_type().is_dir())
        .filter(|e| !is_transient(&e.file_name().to_string_lossy()))
        .map(|e| {
            let relative = e.path().strip_prefix(path).unwrap().to_path_buf();
            let normalized = normalize_path(&relative);
            (normalized, relative)
        })
        .collect::<Vec<_>>();

    path_list.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(path_list)
}

/// Recursively compares the contents of two paths.
///
/// Paths are normalized before comparison so that dynamic components like
/// timestamps match their `_TIMESTAMP_` placeholders in expected outputs.
/// Binary files (e.g., `.db`) are only checked for existence, not content.
fn recursive_compare(expected_path: &Path, actual_path: &Path) -> Result<()> {
    use std::collections::HashMap;
    use std::collections::HashSet;

    let expected_list = build_relative_path_list(expected_path)?;
    let actual_list = build_relative_path_list(actual_path)?;

    let expected_set = expected_list.iter().map(|(n, _)| n).collect::<HashSet<_>>();
    let actual_set = actual_list.iter().map(|(n, _)| n).collect::<HashSet<_>>();

    if expected_set != actual_set {
        let matches = expected_set.intersection(&actual_set).collect::<Vec<_>>();
        let missing = expected_set.difference(&actual_set).collect::<Vec<_>>();
        let unexpected = actual_set.difference(&expected_set).collect::<Vec<_>>();

        bail!(
            r#"expected and actual outputs do not contain the same files
__MATCHES_FOUND__
{:?}

__EXPECTED_BUT_NOT_FOUND__
{:?}

__UNEXPECTED_FILES_FOUND__
{:?}"#,
            matches,
            missing,
            unexpected
        );
    }

    let actual_map = actual_list.into_iter().collect::<HashMap<_, _>>();

    let failed_comparisons = expected_list
        .iter()
        .filter(|(normalized, expected_original)| {
            !is_binary_file(normalized) && !is_symlink(expected_path, expected_original)
        })
        .filter_map(|(normalized, expected_original)| {
            let expected_full_path = expected_path.join(expected_original);
            let actual_original = actual_map.get(normalized).expect("path should exist");
            let actual_full_path = actual_path.join(actual_original);
            compare_files(&expected_full_path, &actual_full_path).err()
        })
        .collect::<Vec<_>>();

    if !failed_comparisons.is_empty() {
        let errors = failed_comparisons
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!("output files did not match:\n{errors}");
    }

    Ok(())
}

/// Returns true if the working directory contains files that were not in the
/// inputs directory, indicating that the test produced output files.
fn has_output_files(test_path: &Path, working_test_directory: &Path) -> Result<bool> {
    let inputs_dir = test_path.join("inputs");
    let input_files = build_relative_path_list(&inputs_dir)?;
    let working_files = build_relative_path_list(working_test_directory)?;

    let input_normalized: std::collections::HashSet<_> =
        input_files.iter().map(|(n, _)| n).collect();

    Ok(working_files
        .iter()
        .any(|(n, _)| !input_normalized.contains(n)))
}

/// Compares the result of the command output with the expected baseline.
fn compare_test_results(
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
            .context("failed to write stderr output")?;
        fs::write(&expected_stdout_file, &command_output.stdout)
            .context("failed to write stdout output")?;
        fs::remove_dir_all(&expected_output_dir).unwrap_or_default();
        fs::write(
            &expected_exit_code_file,
            command_output.exit_code.to_string(),
        )
        .context("failed to write exit code")?;

        // Create outputs directory if the test produced output files
        let produced_outputs = has_output_files(test_path, working_test_directory)?;
        if expects_outputs || produced_outputs {
            recursive_copy(working_test_directory, &expected_output_dir).context(
                "failed to copy output files from test results to setup new expected outputs",
            )?;
            normalize_expected_outputs(&expected_output_dir)
                .context("failed to normalize expected outputs")?;
        }
    }
    compare_results(&expected_stderr_file, &command_output.stderr)?;
    compare_results(&expected_stdout_file, &command_output.stdout)?;

    if expects_outputs {
        recursive_compare(&expected_output_dir, working_test_directory)?;
    }

    compare_results(
        &expected_exit_code_file,
        &command_output.exit_code.to_string(),
    )?;
    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();

    let test_root = Path::new("tests/cli");
    let tests = find_tests(test_root);

    let trials = tests
        .into_iter()
        .map(|test| {
            Trial::test(get_test_name(&test, test_root), move || {
                run_test(&test).map_err(Into::into)
            })
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}
