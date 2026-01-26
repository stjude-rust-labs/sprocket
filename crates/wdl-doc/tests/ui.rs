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
use thirtyfour::prelude::WebDriverResult;
use thirtyfour::start_webdriver_process_full;
use thirtyfour::support::block_on;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
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
        let searchbox = self.find(By::Id("searchbox")).await?;
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
    addresses: &HashMap<String, SocketAddr>,
    driver: Arc<Mutex<WebDriver>>,
    primary_window: WindowHandle,
) -> anyhow::Result<Vec<Trial>> {
    let ui_tests_dir = Path::new("tests").join("ui");

    let mut trials = Vec::new();
    for entry in ui_tests_dir.read_dir()? {
        let entry = entry.expect("failed to read directory");
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
                "No category found for directory '{}'. Was it added to `all_tests()`?",
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
                    "No test found for file {}. Was it added to `all_tests()`?",
                    test_path.display()
                );
            };

            let driver = driver.clone();
            let category_url = category_url.clone();
            let primary_window = primary_window.clone();
            let task: JoinHandle<anyhow::Result<()>> = tokio::task::spawn(async move {
                let mut driver = driver.lock().await;
                let handle = driver.new_tab().await?;
                driver.switch_to_window(handle).await?;
                driver.goto(&category_url).await?;
                test.run(&mut driver).await?;

                // Cleanup
                driver.close_window().await?;
                driver.switch_to_window(primary_window).await?;

                Ok(())
            });

            trials.push(Trial::test(test_name, move || match block_on(task) {
                Ok(result) => result.map_err(Into::into),
                Err(e) => Err(e.into()),
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
        panic!("failed to generate docs for {category}: {e}")
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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    const WEB_DRIVER_PORT: u16 = 9515;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = libtest_mimic::Arguments::from_args();

    let mut caps = DesiredCapabilities::chrome();
    caps.set_headless()?;
    start_webdriver_process_full(
        WebDriverProcessPort::Port(WEB_DRIVER_PORT),
        WebDriverProcessBrowser::<ChromeCapabilities>::Caps(&caps),
        true,
    )
    .expect("failed to start web driver process");

    let addresses = Arc::new(Mutex::new(HashMap::new()));
    for category in TEST_CATEGORIES.keys() {
        let addresses = addresses.clone();
        tokio::spawn(async move {
            if let Err(e) = generate_docs_if_needed(category).await {
                tracing::error!("failed to generate docs for category '{category}'");
                return Err(e);
            }

            let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
            let addr = listener.local_addr()?;

            tracing::info!("Listening on '{addr}' for category '{category}'");

            addresses.lock().await.insert(category.to_string(), addr);

            let router = router(category);
            axum::serve(listener, router).await.unwrap();

            Ok(())
        });
    }

    let driver = WebDriver::new(format!("http://127.0.0.1:{WEB_DRIVER_PORT}"), caps).await?;

    // The initial blank page
    let primary_window = driver.window().await?;

    let driver = Arc::new(Mutex::new(driver));

    let tests = find_tests(&*addresses.lock().await, driver.clone(), primary_window)?;
    let result = libtest_mimic::run(&args, tests);

    if let Ok(driver) = Arc::try_unwrap(driver)
        && let Err(e) = driver.into_inner().quit().await
    {
        tracing::error!("Failed to quit web driver: {e}");
    }

    result.exit()
}
