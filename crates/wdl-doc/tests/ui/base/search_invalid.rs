//! Test for an invalid search query.

use std::path::Path;
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

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        driver.search("zyxwvutsrqponmlkjihgfedcba").await?;

        let no_results_text = driver
            .query(By::XPath(
                "//div[contains(@class,'layout__main-center-content')]//span[contains(@x-text, \
                 \"No results found for\")]",
            ))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .and_displayed()
            .exists()
            .await?;
        if !no_results_text {
            let state = driver
                .execute(
                    "const s = window.Alpine.store('search'); return {query:s.query, \
                     input:document.getElementById('searchbox').value, loading:s.loading, \
                     results:s.results.map(r => r.meta?.title)};",
                    Vec::new(),
                )
                .await?;
            bail!(
                "expected \"No results found\" text; state: {}",
                state.json()
            );
        }

        let no_results = driver
            .query(By::ClassName("search-result"))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .not_exists()
            .await?;
        if !no_results {
            bail!("Expected no search results");
        }

        let latest_results = driver
            .execute_async(
                r#"
                const done = arguments[arguments.length - 1];
                (async () => {
                    const search = { ...window.Alpine.store('search') };
                    let release;
                    const pending = new Promise(resolve => { release = resolve; });
                    search.pagefind = {
                        async search(query) {
                            return query === 'flag_filter'
                                ? { results: [{ data: async () => {
                                    await pending;
                                    return { url: 'stale-result' };
                                } }] }
                                : { results: [] };
                        }
                    };
                    search.query = 'flag_filter';
                    const older = search.performSearch(search.query);
                    search.query = 'does_not_exist';
                    await search.performSearch(search.query);
                    release();
                    await older;
                    return search.results.length === 0;
                })().then(done, error => done(String(error)));
                "#,
                Vec::new(),
            )
            .await?;
        if latest_results.json().as_bool() != Some(true) {
            bail!("an older search overwrote the latest query's results");
        }

        Ok(())
    }
}
