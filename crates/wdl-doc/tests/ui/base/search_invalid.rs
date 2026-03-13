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
        driver.search("this does not exist").await?;

        let no_results = driver
            .query(By::ClassName("search-result"))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .not_exists()
            .await?;
        if !no_results {
            bail!("Expected no search results");
        }

        let no_results_text = driver
            .query(By::XPath(
                "//div[contains(@class,'layout__main-center-content')]//span[contains(@x-text, \
                 \"No results found for\")]",
            ))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .exists()
            .await?;
        if !no_results_text {
            bail!("expected \"No results found\" text");
        }

        Ok(())
    }
}
