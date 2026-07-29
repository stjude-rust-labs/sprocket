//! Test that the "on this page" navigation is generated from the rendered
//! headings.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;

/// Test that the right-rail page navigation mirrors the page's rendered `h2`
/// and `h3` headings in order.
pub struct PageNavigation;

#[async_trait::async_trait]
impl UiTest for PageNavigation {
    fn name(&self) -> &'static str {
        "page_navigation"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        // The driver starts on the workspace index page; navigate to the
        // generated `Employee` struct page.
        let base = driver.current_url().await?;
        let target = base
            .join("employee/Employee-struct.html")
            .context("failed to build struct page url")?;
        driver.goto(target.as_str()).await?;

        // Wait for the client-side navigation to be populated.
        driver
            .query(By::Css("#page-sections a"))
            .wait(Duration::from_secs(10), Duration::from_millis(100))
            .any()
            .await?;

        // Collect the visible content headings, in document order.
        let mut headings = Vec::new();
        for heading in driver
            .find_all(By::Css(
                ".layout__main-center-content h2, .layout__main-center-content h3",
            ))
            .await?
        {
            if !heading.is_displayed().await? {
                continue;
            }
            let tag = heading.tag_name().await?;
            let text = heading.text().await?.trim().to_string();
            headings.push((tag, text));
        }

        if headings.is_empty() {
            bail!("expected the struct page to render at least one heading");
        }

        // The navigation must contain one link per visible heading, in order.
        let link_info = driver
            .execute(
                r#"
                return Array.from(document.querySelectorAll('#page-sections a'), link => ({
                    text: link.textContent.trim(),
                    href: link.getAttribute('href') || '',
                }));
                "#,
                Vec::new(),
            )
            .await?;
        let link_info: Vec<(String, String)> = link_info
            .json()
            .as_array()
            .context("expected page navigation links")?
            .iter()
            .map(|link| {
                (
                    link["text"].as_str().unwrap_or_default().to_string(),
                    link["href"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();

        let link_texts: Vec<&str> = link_info.iter().map(|(text, _)| text.as_str()).collect();
        let heading_texts: Vec<&str> = headings.iter().map(|(_, text)| text.as_str()).collect();
        if link_texts != heading_texts {
            bail!(
                "page navigation links {link_texts:?} do not match rendered headings \
                 {heading_texts:?}"
            );
        }

        // The struct members section and the authored Markdown heading must both
        // be present.
        if !link_texts.contains(&"Members") {
            bail!("expected a `Members` navigation link");
        }
        if !link_texts.contains(&"Modeling notes") {
            bail!("expected the authored Markdown heading in the navigation");
        }

        // Each navigation link must resolve to its heading's anchor.
        for (text, href) in &link_info {
            let Some(fragment) = href.strip_prefix('#') else {
                bail!("navigation link `{text}` href `{href}` is not a fragment");
            };
            let target = driver.find(By::Id(fragment)).await?;
            let tag = target.tag_name().await?;
            if tag != "h2" && tag != "h3" {
                bail!("navigation target `{fragment}` is a `{tag}`, expected a heading");
            }
        }

        Ok(())
    }
}
