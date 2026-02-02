//! Test for a valid search query.

use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;
use crate::WebDriverExt;

/// Test for a valid search query.
pub struct Search;

#[async_trait::async_trait]
impl UiTest for Search {
    fn name(&self) -> &'static str {
        "search"
    }

    async fn run(&self, driver: &mut WebDriver) -> anyhow::Result<()> {
        driver.search("flag_filter").await?;

        let has_results = driver
            .query(By::ClassName("left-sidebar__search-result-item"))
            .wait(Duration::from_millis(500), Duration::from_millis(100))
            .exists()
            .await?;
        if !has_results {
            bail!("expected search results");
        }

        let search_result = driver
            .find(By::XPath("//sprocket-tooltip[@content=\"flag_filter\"]"))
            .await?;
        let search_result_container = search_result.parent().await?;
        let search_result_icon = search_result_container
            .find(By::ClassName("left-sidebar__icon"))
            .await?;

        let icon_path = search_result_icon.attr("src").await?;
        if icon_path.as_deref() != Some("assets/wdl-dir-unselected.svg") {
            bail!(
                "wrong icon shown—expected 'assets/wdl-dir-unselected.svg', found: {icon_path:?}"
            );
        }

        Ok(())
    }
}
