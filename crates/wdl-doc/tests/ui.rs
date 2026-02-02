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
use libtest_mimic::Trial;
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
            .wait(Duration::from_secs(5), Duration::from_millis(100))
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
    fn for_category(category: &str) -> Self {
        let base = Path::new("tests").join("ui").join(category);
        Self {
            docs: base.join("docs"),
            assets: base.join("assets"),
        }
    }
}

/// Finds all UI tests and sets up libtest `Trials` for each.
fn find_tests(
    addresses: &ServerAddressMap,
    driver: Arc<Mutex<WebDriver>>,
    primary_window: WindowHandle,
) -> anyhow::Result<Vec<Trial>> {
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

        let server_addr = addresses.get(&*category_name).expect("should exist");

        let category_url = format!("http://{server_addr}");
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

            let driver = driver.clone();
            let category_url = category_url.clone();
            let primary_window = primary_window.clone();

            trials.push(Trial::test(test_name, move || {
                let task = async move {
                    let mut driver = driver.lock().await;
                    let handle = driver.new_tab().await?;
                    driver.switch_to_window(handle).await?;
                    driver.goto(&category_url).await?;
                    test.run(&mut driver).await?;

                    // Cleanup
                    driver.close_window().await?;
                    driver.switch_to_window(primary_window).await?;

                    Ok(())
                };

                block_on(task)
            }));
        }
    }

    Ok(trials)
}

/// Generates the documentation for the workspace under
/// `./ui/<category>/assets`, if it doesn't already exist, or it needs to be
/// blessed.
async fn generate_docs_if_needed(category: &str) -> anyhow::Result<()> {
    let paths = TestCategoryDirs::for_category(category);
    if paths.docs.exists() {
        if std::env::var_os("BLESS").is_some() {
            std::fs::remove_dir_all(&paths.docs)?;
        } else {
            return Ok(());
        }
    }

    let config = Config::new(AnalysisConfig::default(), &paths.assets, &paths.docs);

    tracing::info!("Generating docs for workspace '{}'", paths.assets.display());
    if let Err(e) = document_workspace(config).await {
        let _ = std::fs::remove_dir_all(&paths.docs);
        bail!("failed to generate docs for {category}: {e}");
    }

    Ok(())
}

/// Create a router for the given `category`.
fn router(category: &str) -> Router {
    let paths = TestCategoryDirs::for_category(category);
    Router::new().fallback_service(get_service(
        tower_http::services::ServeDir::new(paths.docs).append_index_html_on_directories(true),
    ))
}

/// Map of `test category` -> server address.
type ServerAddressMap = HashMap<&'static str, SocketAddr>;

/// Setup a web server for each test category.
async fn setup_web_servers() -> anyhow::Result<Arc<Mutex<ServerAddressMap>>> {
    let addresses = Arc::new(Mutex::new(HashMap::new()));

    let mut set = JoinSet::new();

    for category in TEST_CATEGORIES.keys().copied() {
        let addresses = addresses.clone();
        set.spawn(async move {
            generate_docs_if_needed(category).await?;
            let listener = tokio::net::TcpListener::bind("localhost:0").await?;
            let addr = listener.local_addr()?;

            tracing::info!("Listening on '{addr}' for category '{category}'");

            addresses.lock().await.insert(category, addr);

            tokio::spawn(async move {
                let _ = axum::serve(listener, router(category)).await;
            });

            Ok(())
        });
    }

    let wait_for_setup = async {
        let results = set.join_all().await;
        results.into_iter().collect::<anyhow::Result<_>>()
    };

    match tokio::time::timeout(Duration::from_secs(10), wait_for_setup).await? {
        Ok(()) => Ok(addresses),
        Err(e) => Err(e),
    }
}

/// Configure and run the webdriver process.
async fn setup_webdriver() -> anyhow::Result<WebDriver> {
    const WEB_DRIVER_PORT: u16 = 9515;

    let mut caps = DesiredCapabilities::chrome();
    caps.add_arg("--headless=new")?;
    caps.add_arg("--no-sandbox")?;
    caps.add_arg("--disable-dev-shm-usage")?;
    caps.add_arg("--disable-gpu")?;
    start_webdriver_process_full(
        WebDriverProcessPort::Port(WEB_DRIVER_PORT),
        WebDriverProcessBrowser::<ChromeCapabilities>::Caps(&caps),
        true,
    )
    .context("failed to start web driver process")?;

    // `start_webdriver_process_full()` only waits 1 second. It may take longer for
    // the process to spawn in CI.
    tracing::info!("Waiting for webdriver process to start...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    let driver = WebDriver::new(format!("http://localhost:{WEB_DRIVER_PORT}"), caps).await?;
    Ok(driver)
}

/// The fallible main logic.
async fn real_main() -> Result<WebDriver, (anyhow::Error, Option<WebDriver>)> {
    let unwrap_driver = |driver| {
        Arc::try_unwrap(driver)
            .map(Mutex::into_inner)
            .expect("should be able to unwrap driver")
    };

    let args = libtest_mimic::Arguments::from_args();

    let addresses = setup_web_servers().await.map_err(|e| (e, None))?;
    let driver = setup_webdriver().await.map_err(|e| (e, None))?;

    // The initial blank page
    let primary_window = match driver.window().await {
        Ok(window) => window,
        Err(e) => return Err((e.into(), Some(driver))),
    };

    let driver = Arc::new(Mutex::new(driver));

    let tests = match find_tests(&*addresses.lock().await, driver.clone(), primary_window) {
        Ok(tests) => tests,
        Err(e) => {
            return Err((e, Some(unwrap_driver(driver))));
        }
    };

    if libtest_mimic::run(&args, tests).has_failed() {
        return Err((
            anyhow!("one or more tests failed"),
            Some(unwrap_driver(driver)),
        ));
    }

    Ok(unwrap_driver(driver))
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let result = real_main().await;
    let exit;
    let driver = match result {
        Ok(driver) => {
            exit = Ok(());
            Some(driver)
        }
        Err((e, driver)) => {
            exit = Err(e);
            driver
        }
    };

    if let Some(driver) = driver
        && let Err(e) = driver.quit().await
    {
        tracing::error!("Failed to quit web driver: {e}");
    }

    exit
}
