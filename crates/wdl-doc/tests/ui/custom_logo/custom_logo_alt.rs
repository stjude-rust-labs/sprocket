//! Test for a custom alt light mode logo.

use std::path::Path;

use thirtyfour::WebDriver;
use wdl_doc::Config as DocConfig;

use crate::UiTest;

/// Test for a custom alt light mode logo.
pub struct CustomLogoAlt;

#[async_trait::async_trait]
impl UiTest for CustomLogoAlt {
    fn name(&self) -> &'static str {
        "custom_logo_alt"
    }

    async fn run(&self, driver: &mut WebDriver, docs_path: &Path) -> anyhow::Result<()> {
        super::assert_logo_variants(driver, docs_path, true).await
    }

    fn setup_config(&self, config: DocConfig, workspace_dir: &Path) -> DocConfig {
        config
            .custom_logo(Some(workspace_dir.join("test-logo.svg")))
            .alt_logo(Some(workspace_dir.join("test-logo.light.svg")))
    }
}
