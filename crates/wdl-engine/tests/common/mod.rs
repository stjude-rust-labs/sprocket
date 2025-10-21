use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context as _;
use anyhow::bail;
use futures::future::BoxFuture;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use wdl_analysis::Config as AnalysisConfig;
use wdl_engine::config::BackendConfig;
use wdl_engine::config::Config as EngineConfig;

/// The set of configs that determine how a test is run.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct TestConfig {
    pub analysis: AnalysisConfig,
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
                move || Ok(test_runtime.block_on(run_test(test_path.as_path(), config))?),
            ));
        }
    }
    Ok(tests)
}

/// Gets the configurations to use for the test, merging in any
/// `config-override.yaml` files that may be present in the test directory.
///
/// Regardless of whether `config-override.yaml` is present, this function
/// begins with the configs defined in [`base_configs()`]. If an override is
/// present, its contents are merged into each base config to produce a final
/// set of resolved configs. This is useful for tests which require a
/// modification of the standard configs, particularly those that exercise
/// whether certain options work.
///
/// Why YAML and not TOML for the overrides? TOML doesn't have a way to express
/// "null", and therefore is not suitable for setting `Option` values to `None`.
/// JSON also is a possibility, but YAML is more convenient for human use with
/// its support for comments.
pub fn resolve_configs(path: &Path) -> Result<HashMap<String, TestConfig>, anyhow::Error> {
    use figment::Figment;
    use figment::providers::Format as _;
    use figment::providers::Serialized;
    use figment::providers::Yaml;

    let mut base_configs = base_configs()?;
    let config_override_path = path.join("config-override.yaml");
    if config_override_path.exists() {
        for config in base_configs.values_mut() {
            let combined = Figment::from(Serialized::defaults(&config))
                .merge(Yaml::file_exact(&config_override_path))
                .extract()?;
            *config = combined;
        }
    }
    Ok(base_configs)
}

/// Get the baseline configs for executing the tests.
///
/// These configs may be modified by merging with `*-config-override.json` files
/// in individual test directories before execution.
///
/// If the `SPROCKET_TEST_ENGINE_CONFIG` environment variable is set, the file
/// it points to will be used as the sole base engine config. This is primarily
/// meant for testing in environments with idiosyncratic requirements, such as
/// an HPC.
///
/// Otherwise, a default set containing at least the default analysis config and
/// a local backend config will be used.
pub fn base_configs() -> Result<HashMap<String, TestConfig>, anyhow::Error> {
    if let Some(env_config) = env::var_os("SPROCKET_TEST_ENGINE_CONFIG") {
        let engine = toml::from_str(&std::fs::read_to_string(env_config)?)?;
        let config = TestConfig {
            engine,
            ..TestConfig::default()
        };
        return Ok(HashMap::from([("env_config".to_string(), config)]));
    }

    let mut configs = HashMap::from([(
        "local".to_string(),
        TestConfig {
            engine: EngineConfig {
                backends: [(
                    "default".to_string(),
                    BackendConfig::Local(Default::default()),
                )]
                .into(),
                ..Default::default()
            },
            ..TestConfig::default()
        },
    )]);
    // Currently we limit running the Docker backend to Linux as GitHub does not
    // have Docker installed on macOS hosted runners and the Windows hosted
    // runners are configured to use Windows containers
    if cfg!(target_os = "linux") {
        configs.insert(
            "docker".to_string(),
            TestConfig {
                engine: EngineConfig {
                    backends: [(
                        "default".to_string(),
                        BackendConfig::Docker(Default::default()),
                    )]
                    .into(),
                    ..Default::default()
                },
                ..TestConfig::default()
            },
        );
    }
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

/// Compares a single result.
pub fn compare_result(path: &Path, result: &str) -> Result<(), anyhow::Error> {
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
