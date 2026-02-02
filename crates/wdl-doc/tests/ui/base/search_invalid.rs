//! Test for an invalid search query.

use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;
use crate::WebDriverExt;

/// Test for an invalid search query.
pub struct SearchInvalid;

#[async_trait::async_trait]
impl UiTest for SearchInvalid {
    fn name(&self) -> &'static str {
        "search_invalid"
    }

    async fn run(&self, driver: &mut WebDriver) -> anyhow::Result<()> {
        driver.search("does_not_exist").await?;

        let no_results = driver
            .query(By::ClassName("left-sidebar__search-result-item"))
            .wait(Duration::from_millis(500), Duration::from_millis(100))
            .not_exists()
            .await?;
        if !no_results {
            bail!("Expected no search results");
        }

        let no_results_text = driver
            .query(By::XPath(
                "//li/span[contains(@x-text, \"No results found\")]",
            ))
            .exists()
            .await?;
        if !no_results_text {
            bail!("expected \"No results found\" text");
        }

        Ok(())
    }
}
