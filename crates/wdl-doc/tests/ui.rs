//! `wdl-doc` UI tests
//!
//! The runner will search the `./ui` directory for test files.
//!
//! Each test file is expected to contain a single `UiTest`, which will also
//! need to be added to the list in `all_tests()`.

#[path = "ui/base/mod.rs"]
mod base;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
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
use wdl_analysis::Config as AnalysisConfig;
use wdl_doc::Config;
use wdl_doc::document_workspace;

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
    async fn run(&self, driver: &mut WebDriver) -> anyhow::Result<()>;
}

/// Map of test name -> test implementation
pub type TestMap = HashMap<&'static str, Arc<dyn UiTest>>;

/// Map of test category -> TestMap
static TEST_CATEGORIES: LazyLock<HashMap<&'static str, TestMap>> = LazyLock::new(|| {
    let mut categories = HashMap::new();
    categories.extend([("base", base::all_tests())]);
    categories
});

/// Important directories within a test category.
struct TestCategoryDirs {
    /// The directory containing the generated documentation.
    docs: PathBuf,
    /// The directory containing the assets used by `wdl-doc`.
    assets: PathBuf,
}

impl TestCategoryDirs {
    /// Get the directories for the given test category.
    fn for_category(tmp: &Path, category: &str) -> Self {
        let project_base = Path::new("tests").join("ui").join(category);
        let base = tmp.join(category);
        Self {
            docs: base.join("docs"),
            assets: project_base.join("assets"),
        }
    }
}

/// Finds all UI tests and sets up libtest `Trials` for each.
async fn find_tests(tmp_dir: PathBuf) -> anyhow::Result<Vec<Trial>> {
    let ui_tests_dir = Path::new("tests").join("ui");

    let mut trials = Vec::new();
    for entry in ui_tests_dir.read_dir()? {
        let entry = entry.context("failed to read directory")?;
        let category_path = entry.path();
        if !category_path.is_dir() {
            continue;
        }

        let category_name = category_path
            .file_stem()
            .map(OsStr::to_string_lossy)
            .unwrap()
            .into_owned();

        let Some(category) = TEST_CATEGORIES.get(&*category_name) else {
            bail!(
                "no category found for directory '{}'. Was it added to `all_tests()`?",
                category_path.display()
            );
        };

        for category_entry in category_path.read_dir()? {
            let entry = category_entry.expect("failed to read directory");
            let test_path = entry.path();
            if test_path.is_dir() {
                continue;
            }

            let test_name = test_path
                .file_stem()
                .map(OsStr::to_string_lossy)
                .unwrap()
                .into_owned();

            // Ignore module declarations
            if test_name == "mod" {
                continue;
            }

            let Some(test) = category.get(&*test_name).cloned() else {
                bail!(
                    "no test found for file {}. Was it added to `all_tests()`?",
                    test_path.display()
                );
            };

            let category_name = category_name.clone();
            let tmp_dir = tmp_dir.clone();
            trials.push(Trial::test(test_name, move || {
                let task = async move {
                    let ctx = match global_state(tmp_dir).await {
                        GlobalState::Ready(ctx) => ctx,
                        GlobalState::Failed(e, _) => {
                            return Err(anyhow!("failed to get global state: {e}").into());
                        }
                    };

                    let server_addr = ctx
                        .server_addresses
                        .get(&*category_name)
                        .expect("should exist");
                    let category_url = format!("http://{server_addr}");

                    let mut driver = ctx.driver.lock().await;
                    let handle = driver.new_tab().await?;
                    driver.switch_to_window(handle).await?;
                    driver.goto(&category_url).await?;
                    test.run(&mut driver).await?;

                    // Cleanup
                    driver.close_window().await?;
                    driver.switch_to_window(ctx.primary_window.clone()).await?;

                    Ok(())
                };

                block_on(task)
            }));
        }
    }

    Ok(trials)
}

/// Generates the documentation for the workspace under
/// `<tmp_dir>/<category>/assets`.
async fn generate_docs(tmp_dir: &Path, category: &str) -> anyhow::Result<()> {
    let paths = TestCategoryDirs::for_category(tmp_dir, category);
    let config = Config::new(AnalysisConfig::default(), &paths.assets, &paths.docs);

    tracing::info!("Generating docs for workspace '{}'", paths.assets.display());
    if let Err(e) = document_workspace(config).await {
        let _ = std::fs::remove_dir_all(&paths.docs);
        bail!("failed to generate docs for {category}: {e}");
    }

    Ok(())
}

/// Create a router that serves `docs_path`.
fn router(docs_path: &Path) -> Router {
    Router::new().fallback_service(get_service(
        tower_http::services::ServeDir::new(docs_path).append_index_html_on_directories(true),
    ))
}

/// Map of `test category` -> server address.
type ServerAddressMap = HashMap<&'static str, SocketAddr>;

/// Setup a web server for each test category.
async fn setup_web_servers(tmp_dir: PathBuf) -> anyhow::Result<ServerAddressMap> {
    let addresses = Arc::new(Mutex::new(HashMap::new()));

    let mut set = JoinSet::new();

    for category in TEST_CATEGORIES.keys().copied() {
        let paths = TestCategoryDirs::for_category(&tmp_dir, category);
        let addresses = addresses.clone();

        let tmp = tmp_dir.clone();
        set.spawn(async move {
            generate_docs(&tmp, category).await?;
            let listener = tokio::net::TcpListener::bind("localhost:0").await?;
            let addr = listener.local_addr()?;

            tracing::info!("Listening on '{addr}' for category '{category}'");

            addresses.lock().await.insert(category, addr);

            tokio::spawn(async move {
                let _ = axum::serve(listener, router(&paths.docs)).await;
            });

            Ok(())
        });
    }

    let wait_for_setup = async {
        let results = set.join_all().await;
        results.into_iter().collect::<anyhow::Result<_>>()
    };

    match tokio::time::timeout(Duration::from_secs(10), wait_for_setup).await? {
        Ok(()) => Ok(Arc::try_unwrap(addresses)
            .expect("should be exclusive")
            .into_inner()),
        Err(e) => Err(e),
    }
}

/// Get a random unused port.
async fn random_port() -> anyhow::Result<u16> {
    let addr = tokio::net::TcpListener::bind("localhost:0").await?;
    Ok(addr.local_addr()?.port())
}

// TODO: Figure out why tests fail on platforms other than Linux
/// Whether the browser should be run in headless mode.
fn should_run_headless() -> bool {
    std::env::var("IN_CI").is_ok() || cfg!(target_os = "linux")
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

    // `start_webdriver_process_full()` only waits 1 second. It may take longer for
    // the process to spawn in CI.
    tracing::info!("Waiting for webdriver process to start...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    let driver = WebDriver::new(format!("http://localhost:{port}"), caps).await?;
    driver.fullscreen_window().await?;
    Ok(driver)
}

/// Global state shared between all UI tests.
#[derive(Clone)]
enum GlobalState {
    /// The state was initialized successfully.
    Ready(Arc<DriverContext>),
    /// The state failed initialization.
    Failed(Arc<anyhow::Error>, Option<Arc<Mutex<WebDriver>>>),
}

/// The webdriver handle and its surrounding context.
struct DriverContext {
    /// A handle to the webdriver.
    driver: Arc<Mutex<WebDriver>>,
    /// A map of test categories to their server addresses.
    server_addresses: ServerAddressMap,
    /// The initial blank window of the webdriver.
    primary_window: WindowHandle,
}

/// Global state shared between all UI tests.
static GLOBAL_STATE: OnceCell<Mutex<Option<GlobalState>>> = OnceCell::const_new();

/// Initialize and get the [`GLOBAL_STATE`].
async fn global_state(tmp_dir: PathBuf) -> GlobalState {
    let mutex = GLOBAL_STATE
        .get_or_init(|| async {
            let state = match setup_webdriver().await {
                Ok(driver) => {
                    let driver_arc = Arc::new(Mutex::new(driver));
                    match setup_web_servers(tmp_dir).await {
                        Ok(addresses) => GlobalState::Ready(Arc::new(DriverContext {
                            driver: driver_arc.clone(),
                            server_addresses: addresses,
                            primary_window: driver_arc.lock().await.window().await.unwrap(),
                        })),
                        Err(e) => GlobalState::Failed(Arc::new(e), Some(driver_arc)),
                    }
                }
                Err(e) => GlobalState::Failed(Arc::new(e), None),
            };
            Mutex::new(Some(state))
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

    let tmp = TempDir::with_prefix("wdl-doc-ui")?;

    let args = libtest_mimic::Arguments::from_args();
    let tests = find_tests(tmp.path().to_path_buf()).await?;

    let conclusion = libtest_mimic::run(&args, tests);

    let Some(global_state) = GLOBAL_STATE.get() else {
        return Ok(());
    };

    let mut guard = global_state.lock().await;
    if let Some(state) = guard.take() {
        let driver_arc = match state {
            GlobalState::Ready(ctx) => Some(ctx.driver.clone()),
            GlobalState::Failed(e, d) => {
                tracing::error!("Failed to initialize webdriver: {e}");
                d
            }
        };

        if let Some(driver) = driver_arc {
            let driver_mutex = Arc::try_unwrap(driver).expect("should be exclusive");
            let driver = driver_mutex.into_inner();

            if let Err(e) = driver.quit().await {
                tracing::error!("Failed to quit webdriver: {e}");
            }
        }
    }

    conclusion.exit()
}
