//! Test for a custom logo.

use std::path::Path;

use thirtyfour::WebDriver;
use wdl_doc::Config as DocConfig;

use crate::UiTest;

/// Test for a custom logo.
pub struct CustomLogo;

#[async_trait::async_trait]
impl UiTest for CustomLogo {
    fn name(&self) -> &'static str {
        "custom_logo"
    }

    async fn run(&self, driver: &mut WebDriver, docs_path: &Path) -> anyhow::Result<()> {
        super::assert_logo_variants(driver, docs_path, false).await
    }

    fn setup_config(&self, config: DocConfig, workspace_dir: &Path) -> DocConfig {
        config.custom_logo(Some(workspace_dir.join("test-logo.svg")))
    }
}
