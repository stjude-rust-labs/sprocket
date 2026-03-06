//! `wdl-doc` UI tests
//!
//! The runner will search the `./ui` directory for test files.
//!
//! Each test file is expected to contain a single `UiTest`, which will also
//! need to be added to the list in `all_tests()`.

#[path = "ui/base/mod.rs"]
mod base;
#[path = "ui/custom_logo/mod.rs"]
mod custom_logo;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Context;
use anyhow::anyhow;
use anyhow::bail;
use axum::Router;
use axum::routing::get_service;
use colored::Colorize;
use libtest_mimic::Trial;
use tempfile::TempDir;
use thirtyfour::By;
use thirtyfour::ChromeCapabilities;
use thirtyfour::ChromiumLikeCapabilities;
use thirtyfour::DesiredCapabilities;
use thirtyfour::WebDriver;
use thirtyfour::WebDriverProcessBrowser;
use thirtyfour::WebDriverProcessPort;
use thirtyfour::WindowHandle;
use thirtyfour::prelude::ElementQueryable;
use thirtyfour::prelude::WebDriverResult;
use thirtyfour::start_webdriver_process_full;
use thirtyfour::support::block_on;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;
use tokio::task::JoinSet;

/// Extension trait for [`WebDriver`]s.
pub trait WebDriverExt {
    /// Get a string value from `localStorage`.
    fn localstorage(
        &self,
        key: impl AsRef<str> + Send,
    ) -> impl Future<Output = WebDriverResult<Option<String>>> + Send;

    /// Type `input` into the search box.
    fn search(
        &self,
        input: impl AsRef<str> + Send,
    ) -> impl Future<Output = WebDriverResult<()>> + Send;
}

impl WebDriverExt for WebDriver {
    async fn localstorage(&self, key: impl AsRef<str>) -> WebDriverResult<Option<String>> {
        let script = format!("return window.localStorage.getItem('{}');", key.as_ref());
        let ret = self.execute(script, []).await?;
        if ret.json().is_null() {
            return Ok(None);
        }

        ret.convert()
    }

    async fn search(&self, input: impl AsRef<str> + Send) -> WebDriverResult<()> {
        let searchbox = self
            .query(By::Id("searchbox"))
            .wait(Duration::from_secs(10), Duration::from_millis(100))
            .first()
            .await?;
        searchbox.send_keys(input.as_ref()).await?;
        Ok(())
    }
}

/// A `wdl-doc` UI test.
#[async_trait::async_trait]
pub trait UiTest: Send + Sync {
    /// The name of the test (the file name without the extension).
    fn name(&self) -> &'static str;
    /// Execute the UI test.
    ///
    /// By default, the `driver` will be navigated to the workspace index page.
    async fn run(&self, driver: &mut WebDriver, docs_path: &Path) -> anyhow::Result<()>;
}

/// Map of test name -> test implementation
pub type TestMap = HashMap<&'static str, Arc<dyn UiTest>>;

/// Map of test category -> TestMap
static TEST_CATEGORIES: LazyLock<HashMap<&'static str, TestMap>> = LazyLock::new(|| {
    let mut categories = HashMap::new();
    categories.extend([
        ("base", base::all_tests()),
        ("custom_logo", custom_logo::all_tests()),
    ]);
    categories
});

/// Metadata derived from the test file.
#[derive(Clone, Default)]
struct TestMetadata {
    /// Arguments to pass to `wdl-doc` for this file.
    wdl_doc_args: Vec<String>,
}

impl TestMetadata {
    /// Parse the metadata comments at the top of the file.
    fn load(path: &Path) -> anyhow::Result<Self> {
        const METADATA_MARKER: &str = "//@";

        let mut metadata = Self::default();
        for line in BufReader::new(File::open(path)?).lines() {
            let line = line?;
            let Some(meta) = line.strip_prefix(METADATA_MARKER) else {
                break;
            };

            let Some((field, value)) = meta.split_once(':') else {
                bail!("Malformed meta line in '{}': {line}", path.display());
            };

            match field {
                "args" => {
                    metadata.wdl_doc_args = value.split(' ').map(ToString::to_string).collect()
                }
                _ => bail!("Unexpected meta line in '{}': {line}", path.display()),
            }
        }

        Ok(metadata)
    }
}

/// A UI test instance.
#[derive(Clone)]
struct Test {
    /// The category the test lives in.
    category: String,
    /// The name of the test.
    name: String,
    /// The metadata of the test.
    metadata: TestMetadata,
    /// The actual test implementation.
    test_impl: Arc<dyn UiTest>,
}

impl Test {
    /// A unique identifier for the test.
    fn id(&self) -> String {
        format!("{}_{}", self.category, self.name)
    }
}

/// Finds all UI tests.
fn find_tests() -> anyhow::Result<Vec<Test>> {
    let ui_tests_dir = Path::new("tests").join("ui");
    let mut found = Vec::new();

    for entry in ui_tests_dir.read_dir()? {
        let entry = entry.context("failed to read directory")?;
        let category_path = entry.path();
        if !category_path.is_dir() {
            continue;
        }

        let category_name = category_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        if !TEST_CATEGORIES.contains_key(&*category_name) {
            continue;
        }
        let category_map = TEST_CATEGORIES.get(&*category_name).unwrap();

        for test_entry in category_path.read_dir()? {
            let test_entry = test_entry?;
            let test_path = test_entry.path();

            if test_path.is_dir() || test_path.extension().and_then(OsStr::to_str) != Some("rs") {
                continue;
            }

            let test_name = test_path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned();

            if test_name == "mod" {
                continue;
            }

            let Some(test_impl) = category_map.get(&*test_name).cloned() else {
                bail!(
                    "no test found for file {}. Was it added to `all_tests()`?",
                    test_path.display()
                );
            };

            let metadata = match TestMetadata::load(&test_path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("Failed to parse metadata for {}: {e}", test_path.display());
                    continue;
                }
            };

            found.push(Test {
                category: category_name.clone(),
                name: test_name,
                metadata,
                test_impl,
            });
        }
    }
    Ok(found)
}

/// Generates the documentation for all test instances.
async fn generate_docs(tests: &[Test], tmp_dir: &Path, bin_path: &Path) -> anyhow::Result<()> {
    let mut set = JoinSet::new();

    for test in tests {
        let id = test.id();
        let category = test.category.clone();
        let args = test.metadata.wdl_doc_args.clone();
        let tmp = tmp_dir.to_path_buf();
        let bin = bin_path.to_path_buf();

        set.spawn(async move {
            let category_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("ui")
                .join(&category);

            let output_dir = tmp.join(&id).join("docs");
            let assets_dir = category_root.join("assets");

            tracing::info!("Generating docs for test '{id}'");

            let mut cmd = Command::new(&bin);
            cmd.args(&args)
                .arg("--output")
                .arg(&output_dir)
                .arg(&assets_dir)
                .current_dir(&category_root);

            let status = cmd
                .stderr(Stdio::inherit())
                .stdout(Stdio::inherit())
                .status()
                .context("failed to generate docs")?;

            if !status.success() {
                bail!("failed to generate docs");
            }
            Ok(())
        });
    }

    while let Some(res) = set.join_next().await {
        res??;
    }

    Ok(())
}

/// Setup a web server to serve the test docs.
async fn start_web_server(tmp_dir: &Path) -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind("localhost:0").await?;
    let addr = listener.local_addr()?;

    let router = Router::new().fallback_service(get_service(
        tower_http::services::ServeDir::new(tmp_dir).append_index_html_on_directories(true),
    ));

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!("Web server failed: {e}");
        }
    });

    tracing::info!("Web server listening at http://{addr}");
    Ok(addr)
}

/// Get a random unused port.
async fn random_port() -> anyhow::Result<u16> {
    let addr = tokio::net::TcpListener::bind("localhost:0").await?;
    Ok(addr.local_addr()?.port())
}

// TODO: Figure out why tests fail on platforms other than Linux
/// Whether the browser should be run in headless mode.
fn should_run_headless() -> bool {
    std::env::var("IN_CI").is_ok() || std::env::var("HEADLESS").is_ok()
}

/// Configure and run the webdriver process.
async fn setup_webdriver() -> anyhow::Result<WebDriver> {
    let port = random_port().await?;

    let mut caps = DesiredCapabilities::chrome();
    if should_run_headless() {
        caps.add_arg("--headless=new")?;
        caps.add_arg("--no-sandbox")?;
        caps.add_arg("--disable-dev-shm-usage")?;
        caps.add_arg("--disable-gpu")?;
    }

    start_webdriver_process_full(
        WebDriverProcessPort::Port(port),
        WebDriverProcessBrowser::<ChromeCapabilities>::Caps(&caps),
        true,
    )
    .context("failed to start web driver process")?;

    tracing::info!("Waiting for webdriver process to start...");

    let mut driver = None;
    for _ in 0..20 {
        if let Ok(d) = WebDriver::new(format!("http://localhost:{port}"), caps.clone()).await {
            driver = Some(d);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let driver = driver.context("Webdriver failed to start within the timeout period")?;
    driver.fullscreen_window().await?;

    Ok(driver)
}

/// Global state shared between all UI tests.
#[derive(Clone)]
enum GlobalState {
    /// The state was initialized successfully.
    Ready(Arc<DriverContext>),
    /// The state failed initialization.
    Failed(Arc<anyhow::Error>),
}

/// The webdriver handle and its surrounding context.
#[derive(Debug)]
struct DriverContext {
    /// A handle to the webdriver.
    driver: Arc<Mutex<WebDriver>>,
    /// Address for the web server.
    server_addr: SocketAddr,
    /// The initial blank window of the webdriver.
    primary_window: WindowHandle,
}

/// Global state shared between all UI tests.
static GLOBAL_STATE: OnceCell<Mutex<Option<GlobalState>>> = OnceCell::const_new();

/// Initialize and get the [`GLOBAL_STATE`].
async fn global_state(tests: Arc<Vec<Test>>, tmp_dir: PathBuf) -> GlobalState {
    let mutex = GLOBAL_STATE
        .get_or_init(|| async {
            let state = async {
                let build_status = Command::new("cargo")
                    .args(["build", "-p", "wdl-doc-bin", "--bin", "wdl-doc"])
                    .status()?;
                if !build_status.success() {
                    bail!("Failed to build the `wdl-doc` binary");
                }

                let target_dir = std::env::var("CARGO_TARGET_DIR").map_or_else(
                    |_| {
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("target")
                    },
                    PathBuf::from,
                );
                let bin_path = target_dir.join("debug").join(if cfg!(windows) {
                    "wdl-doc.exe"
                } else {
                    "wdl-doc"
                });

                generate_docs(&tests, &tmp_dir, &bin_path).await?;

                let server_addr = start_web_server(&tmp_dir).await?;
                let driver = setup_webdriver().await?;

                let driver_arc = Arc::new(Mutex::new(driver));
                let primary_window = driver_arc.lock().await.window().await.unwrap();

                Ok::<_, anyhow::Error>(Arc::new(DriverContext {
                    driver: driver_arc.clone(),
                    server_addr,
                    primary_window,
                }))
            }
            .await;

            let result = match state {
                Ok(ctx) => GlobalState::Ready(ctx),
                Err(e) => GlobalState::Failed(Arc::new(e)),
            };

            Mutex::new(Some(result))
        })
        .await;

    let guard = mutex.lock().await;
    guard.as_ref().expect("taken").clone()
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if !should_run_headless() {
        let warning = r#"+------------------------------------+
|                                    |
|                                    |
|  !!! PHOTOSENSITIVITY WARNING !!!  |
|                                    |
|  The UI test suite contains rapid  |
|  window transitions and flashing   |
|  content.                          |
|                                    |
|                                    |
+------------------------------------+"#;
        println!("{}\n", warning.red());
        for i in (1..6).rev() {
            println!("The tests will start in {i} seconds.");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    let build_status = Command::new("cargo")
        .args(["build", "-p", "wdl-doc-bin", "--bin", "wdl-doc"])
        .status()?;
    if !build_status.success() {
        bail!("Failed to build wdl-doc binary");
    }

    let tmp = TempDir::with_prefix("wdl-doc-ui")?;

    let args = libtest_mimic::Arguments::from_args();
    let tests = Arc::new(find_tests()?);

    let trials = create_trials(tmp.path().to_path_buf(), tests);
    let conclusion = libtest_mimic::run(&args, trials);

    cleanup_global_state().await;
    conclusion.exit()
}

/// Create [`Trial`] instances for all tests.
fn create_trials(tmp: PathBuf, tests: Arc<Vec<Test>>) -> Vec<Trial> {
    let mut trials = Vec::new();

    for test in &*tests {
        let test = test.clone();
        let tests = tests.clone();
        let tmp = tmp.clone();

        trials.push(Trial::test(
            format!("{}::{}", test.category, test.name),
            move || {
                let task = async move {
                    let ctx = match global_state(tests, tmp.clone()).await {
                        GlobalState::Ready(ctx) => ctx,
                        GlobalState::Failed(e) => {
                            return Err(anyhow!("failed to get global state: {e}").into());
                        }
                    };

                    let test_id = test.id();
                    let test_url = format!("http://{}/{test_id}/docs/", ctx.server_addr);
                    let docs_dir = tmp.join(&test_id).join("docs");

                    let mut driver = ctx.driver.lock().await;
                    let tab = driver.new_tab().await?;

                    let test_result = async {
                        driver.switch_to_window(tab.clone()).await?;
                        driver.goto(&test_url).await?;
                        test.test_impl.run(&mut driver, &docs_dir).await?;
                        Ok(())
                    }
                    .await;

                    if let Err(e) = driver.close_window().await {
                        tracing::warn!("Failed to close tab: {e}");
                    }
                    if let Err(e) = driver.switch_to_window(ctx.primary_window.clone()).await {
                        tracing::warn!("Failed to switch to primary window: {e}");
                    }

                    test_result.map_err(|e: anyhow::Error| e.into())
                };

                block_on(task)
            },
        ));
    }

    trials
}

/// Cleanup the processes related to [`GLOBAL_STATE`].
async fn cleanup_global_state() {
    let Some(global_state) = GLOBAL_STATE.get() else {
        return;
    };

    let mut guard = global_state.lock().await;
    if let Some(state) = guard.take()
        && let GlobalState::Ready(ctx) = state
    {
        let ctx = Arc::try_unwrap(ctx).expect("should be exclusive");
        let driver_mutex = Arc::try_unwrap(ctx.driver).expect("should be exclusive");
        let driver = driver_mutex.into_inner();

        if let Err(e) = driver.quit().await {
            tracing::error!("Failed to quit webdriver: {e}");
        }
    }
}
