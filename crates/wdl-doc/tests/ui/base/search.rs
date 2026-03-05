//! Test for a valid search query.

use std::path::Path;
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

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        driver.search("flag_filter").await?;

        let search_results = driver
            .query(By::ClassName("search-result"))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .any()
            .await?;
        if search_results.len() != 2 {
            bail!("expected 2 search results");
        }

        let mut found_struct = false;
        let mut found_task = false;
        for element in search_results {
            let anchor = element.query(By::Tag("a")).first().await?;
            match &*anchor.text().await? {
                "FlagFilter" => found_struct = true,
                "validate_flag_filter" => found_task = true,
                text => bail!("unexpected search result: {text}"),
            }
        }

        if !found_struct {
            bail!("expected to find `FlagFilter` struct");
        }

        if !found_task {
            bail!("expected to find `validate_flag_filter` task");
        }

        Ok(())
    }
}
