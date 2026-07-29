//! Test the mobile header and navigation layout.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::WebDriver;

use crate::UiTest;

/// Test that the mobile header fits and navigation links close the sidebar.
pub struct MobileLayout;

#[async_trait::async_trait]
impl UiTest for MobileLayout {
    fn name(&self) -> &'static str {
        "mobile_layout"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        let original_rect = driver.get_window_rect().await?;

        driver.set_window_rect(0, 0, 1400, 900).await?;
        driver
            .execute("sessionStorage.clear(); location.reload();", Vec::new())
            .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        driver
            .execute(
                r#"
                const controls = document.querySelector('.left-sidebar__controls');
                controls?.querySelectorAll('.left-sidebar__size-button')[2]?.click();
                "#,
                Vec::new(),
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let expanded_controls = driver
            .execute(
                r#"
                const controls = document.querySelector('.left-sidebar__controls');
                return {
                  controlsVisible:
                    controls !== null &&
                    getComputedStyle(controls).display !== 'none',
                  expanded:
                    document.querySelector('.layout__container')
                      .classList.contains('layout__container--left-xl'),
                };
                "#,
                Vec::new(),
            )
            .await?;
        let expanded_controls = expanded_controls.json();
        if !expanded_controls["controlsVisible"]
            .as_bool()
            .unwrap_or(false)
            || !expanded_controls["expanded"].as_bool().unwrap_or(false)
        {
            bail!(
                "expected sidebar controls to remain available at the largest width: \
                 {expanded_controls}"
            );
        }

        driver.set_window_rect(0, 0, 390, 844).await?;
        driver
            .execute("sessionStorage.clear(); location.reload();", Vec::new())
            .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;

        driver
            .execute(
                r#"
                const buttons = document.querySelectorAll(
                  '.layout__main-body .left-sidebar__size-button'
                );
                buttons[1]?.click();
                "#,
                Vec::new(),
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;

        let layout = driver
            .execute(
                r#"
                const rect = selector =>
                  document.querySelector(selector).getBoundingClientRect();
                const header = document.querySelector('.layout__header');
                const sidebar = document.querySelector('.left-sidebar__container');
                return {
                  logo: rect('#logo'),
                  search: rect('#search'),
                  theme: rect('#theme-toggle'),
                  sidebar: rect('.layout__sidebar-left'),
                  firstRowTop: rect('.left-sidebar__row').top,
                  headerBorder: getComputedStyle(header).borderBottomColor,
                  sidebarBorder: getComputedStyle(sidebar).borderRightColor,
                };
                "#,
                Vec::new(),
            )
            .await?;

        driver
            .execute(
                r#"
                const link = document.querySelector('.layout__sidebar-left a[href]');
                link.addEventListener('click', event => event.preventDefault(), { once: true });
                link.click();
                "#,
                Vec::new(),
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let sidebar_after_click = driver
            .execute(
                r#"
                const sidebar = document.querySelector('.layout__sidebar-left');
                const rect = sidebar.getBoundingClientRect();
                return { right: rect.right, transform: getComputedStyle(sidebar).transform };
                "#,
                Vec::new(),
            )
            .await?;

        driver
            .set_window_rect(
                original_rect.x,
                original_rect.y,
                original_rect.width as u32,
                original_rect.height as u32,
            )
            .await?;
        driver
            .execute("sessionStorage.clear();", Vec::new())
            .await?;

        let layout = layout.json();
        let logo_right = layout["logo"]["right"]
            .as_f64()
            .context("expected logo right edge")?;
        let search_left = layout["search"]["left"]
            .as_f64()
            .context("expected search left edge")?;
        let search_right = layout["search"]["right"]
            .as_f64()
            .context("expected search right edge")?;
        let theme_left = layout["theme"]["left"]
            .as_f64()
            .context("expected theme left edge")?;
        if search_left < logo_right + 8.0 {
            bail!(
                "mobile search overlaps the logo; search starts at {search_left}px and logo ends \
                 at {logo_right}px"
            );
        }
        if search_right > theme_left - 8.0 {
            bail!(
                "mobile search overlaps the theme control; search ends at {search_right}px and \
                 theme starts at {theme_left}px"
            );
        }

        let first_row_top = layout["firstRowTop"]
            .as_f64()
            .context("expected first sidebar row position")?;
        if !(160.0..=168.0).contains(&first_row_top) {
            bail!(
                "expected the sidebar tree at its restored vertical position, found it at \
                 {first_row_top}px"
            );
        }
        if layout["headerBorder"].as_str() != Some("rgb(34, 39, 59)") {
            bail!(
                "expected a higher-contrast navbar border, found {:?}",
                layout["headerBorder"]
            );
        }
        if layout["sidebarBorder"].as_str() != Some("rgb(34, 39, 59)") {
            bail!(
                "expected a higher-contrast sidebar border, found {:?}",
                layout["sidebarBorder"]
            );
        }
        let open_sidebar_right = layout["sidebar"]["right"]
            .as_f64()
            .context("expected open sidebar right edge")?;
        if open_sidebar_right < 300.0 {
            bail!("expected the mobile sidebar to be open before choosing a link");
        }

        let sidebar_after_click = sidebar_after_click.json();
        let sidebar_right = sidebar_after_click["right"]
            .as_f64()
            .context("expected sidebar right edge after clicking a link")?;
        if sidebar_right > 0.0 {
            bail!(
                "expected a mobile navigation link to close the sidebar; right edge was \
                 {sidebar_right}px"
            );
        }

        Ok(())
    }
}
