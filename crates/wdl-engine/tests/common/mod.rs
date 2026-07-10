//! Common logic for the `wdl-engine` integration tests.
//!
//! This is located in `common/mod.rs` rather than `common.rs` in order to avoid
//! `cargo test` treating this as an integration test target and needlessly
//! compiling a test executable from its source.

use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Context as _;
use anyhow::bail;
use futures::future::BoxFuture;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use regex::Regex;
use toml_spanner::Toml;
use wdl_analysis::Config as AnalysisConfig;
use wdl_engine::config::Config as EngineConfig;

/// The set of tests that should only use the Docker backend
const DOCKER_ONLY_TESTS: &[&str] = &[
    // Exercises container image fallback, which requires a real pull
    "container-fallback",
    // Disabled for local backend due to paths coming from the download cache
    "url-symlink",
    // Error message contains a guest path
    "subdir-output-escape",
];

/// Matches volatile guest input mount prefixes in container diagnostics.
static GUEST_INPUT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // SAFETY: this regex is a fixed literal pattern and is valid.
    Regex::new(r"/mnt/task/inputs/\d+/").unwrap()
});

/// The set of configs that determine how a test is run.
#[derive(Debug, Clone, Default, Toml)]
pub struct TestConfig {
    /// The analysis configuration for the tests.
    pub analysis: AnalysisConfig,
    /// The engine configuration for the tests.
    pub engine: EngineConfig,
}

/// Find tests to run in the given directory.
pub fn find_tests(
    run_test: fn(&Path, TestConfig) -> BoxFuture<'_, Result<(), anyhow::Error>>,
    base_dir: &Path,
    runtime: &tokio::runtime::Handle,
) -> Result<Vec<Trial>, anyhow::Error> {
    let mut tests = vec![];
    for entry in base_dir.read_dir().unwrap() {
        let entry = entry.expect("failed to read directory");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let test_name_base = path
            .file_stem()
            .map(std::ffi::OsStr::to_string_lossy)
            .unwrap()
            .into_owned();
        for (config_name, config) in resolve_configs(&path)
            .with_context(|| format!("getting configs for {test_name_base}"))?
        {
            let test_runtime = runtime.clone();
            let test_path = path.clone();
            tests.push(Trial::test(
                format!("{test_name_base}_{config_name}"),
                move || {
                    Ok(test_runtime
                        .block_on(run_test(test_path.as_path(), config))
                        .map_err(|e| format!("{e:?}"))?)
                },
            ));
        }
    }
    Ok(tests)
}

/// Gets the configurations to use for the test, merging in any
/// `config-override.toml` files that may be present in the test directory.
pub fn resolve_configs(path: &Path) -> Result<HashMap<String, TestConfig>, anyhow::Error> {
    let config_override_path = path.join("config-override.toml");
    let exists = config_override_path.exists();

    let mut configs: HashMap<String, TestConfig> = base_configs()?
        .into_iter()
        .map(|(name, config)| {
            let mut builder = wdl_engine::Config::builder().with_string_source(config);

            if exists {
                builder = builder.with_file_source(&config_override_path);
            }

            Ok((
                name,
                TestConfig {
                    engine: builder.try_build()?,
                    ..Default::default()
                },
            ))
        })
        .collect::<anyhow::Result<_>>()?;

    // Remove the local configuration if the test is marked as Docker-only
    if let Some(test) = path.file_name().and_then(OsStr::to_str)
        && DOCKER_ONLY_TESTS.contains(&test)
    {
        configs.remove("local");
    }

    Ok(configs)
}

/// Get the baseline configs for executing the tests.
///
/// These configs may be modified by merging with `config-override.toml` files
/// in individual test directories before execution.
///
/// If the `SPROCKET_TEST_ENGINE_CONFIG` environment variable is set, the file
/// it points to will be used as the sole base engine config. This is primarily
/// meant for testing in environments with idiosyncratic requirements, such as
/// an HPC.
///
/// Otherwise, a default set containing at least the default analysis config and
/// a local backend config will be used.
pub fn base_configs() -> Result<HashMap<String, String>, anyhow::Error> {
    if let Some(env_config) = env::var_os("SPROCKET_TEST_ENGINE_CONFIG") {
        return Ok(HashMap::from([(
            "env_config".to_string(),
            std::fs::read_to_string(env_config)?,
        )]));
    }

    #[allow(unused_mut)]
    let mut configs = HashMap::from([(
        "local".to_string(),
        r#"
[backends.default]
type = "local"
"#
        .into(),
    )]);

    // Currently we limit running the Docker backend to Linux as GitHub does not
    // have Docker installed on macOS hosted runners and the Windows hosted
    // runners are configured to use Windows containers
    #[cfg(not(docker_tests_disabled))]
    configs.insert(
        "docker".to_string(),
        r#"
[backends.default]
type = "docker"
"#
        .into(),
    );

    Ok(configs)
}

/// Strips paths from the given string.
pub fn strip_paths(root: &Path, s: &str) -> String {
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
pub fn normalize(s: &str) -> String {
    // Normalize paths separation characters first
    s.replace("\\\\", "/")
        .replace("\\", "/")
        .replace("\r\n", "\n")
}

/// Normalizes dynamic error output for comparison.
fn normalize_error_result(s: &str) -> String {
    let mut s = GUEST_INPUT_PATTERN
        .replace_all(s, "/mnt/task/inputs/_INPUT_/")
        .to_string();
    s.truncate(s.trim_end_matches('\n').len());
    s.push('\n');
    s
}

/// Compares a single result.
pub fn compare_result(path: &Path, result: &str) -> Result<(), anyhow::Error> {
    let result = normalize(result);
    let result = if path.file_name() == Some(OsStr::new("error.txt")) {
        normalize_error_result(&result)
    } else {
        result
    };
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
    let expected = if path.file_name() == Some(OsStr::new("error.txt")) {
        normalize_error_result(&expected)
    } else {
        expected
    };

    if expected != result {
        bail!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}
