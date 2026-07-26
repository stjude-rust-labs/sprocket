//! Test for a valid search query and the platform-aware search shortcut.

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

/// Dispatches a `keydown` for the `k` key with the given modifier from a
/// non-editable target and returns the `id` of the resulting active element.
///
/// The shortcut listener must ignore editable targets, so the event is
/// dispatched after blurring any focused field.
async fn shortcut_active_element(
    driver: &WebDriver,
    modifier: &str,
) -> anyhow::Result<Option<String>> {
    let script = format!(
        r#"
        const active = document.activeElement;
        if (active && active.blur) active.blur();
        const event = new KeyboardEvent('keydown', {{
            key: 'k',
            code: 'KeyK',
            {modifier}: true,
            bubbles: true,
            cancelable: true,
        }});
        document.dispatchEvent(event);
        "#
    );
    driver.execute(script, Vec::new()).await?;

    // The focus is applied by the Alpine handler, so poll briefly for it.
    for _ in 0..20 {
        let ret = driver
            .execute(
                "return document.activeElement ? document.activeElement.id : null;",
                Vec::new(),
            )
            .await?;
        if ret.json().as_str() == Some("searchbox") {
            return Ok(Some("searchbox".to_string()));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let ret = driver
        .execute(
            "return document.activeElement ? document.activeElement.id : null;",
            Vec::new(),
        )
        .await?;
    Ok(ret.json().as_str().map(str::to_string))
}

#[async_trait::async_trait]
impl UiTest for Search {
    fn name(&self) -> &'static str {
        "search"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        // The search box should be focused by the platform-aware keyboard
        // shortcut on both `Meta+K` (macOS) and `Control+K` (other platforms).
        let meta = shortcut_active_element(driver, "metaKey").await?;
        if meta.as_deref() != Some("searchbox") {
            bail!("expected `Meta+K` to focus the search box, active element was {meta:?}");
        }

        // Blur the search box before exercising the second shortcut.
        driver
            .execute("document.getElementById('searchbox').blur();", Vec::new())
            .await?;

        let ctrl = shortcut_active_element(driver, "ctrlKey").await?;
        if ctrl.as_deref() != Some("searchbox") {
            bail!("expected `Control+K` to focus the search box, active element was {ctrl:?}");
        }

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
