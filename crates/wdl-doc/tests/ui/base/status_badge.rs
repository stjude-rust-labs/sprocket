//! Test workflow status badge background colors.

use std::path::Path;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::WebDriver;

use crate::UiTest;

/// Test that nested-input status badges use solid green and red backgrounds.
pub struct StatusBadge;

#[async_trait::async_trait]
impl UiTest for StatusBadge {
    fn name(&self) -> &'static str {
        "status_badge"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        let base = driver.current_url().await?;
        let target = base
            .join("employee/employee_is_person-workflow.html")
            .context("failed to build workflow page url")?;
        driver.goto(target.as_str()).await?;

        let styles = driver
            .execute(
                r#"
                const badge = document.querySelector('.main__badge--error');
                const wdlBadge = document.querySelector('.main__badge--wdl');
                document.documentElement.classList.remove('light');
                const errorDark = getComputedStyle(badge).backgroundColor;
                badge.classList.replace('main__badge--error', 'main__badge--success');
                const successDark = getComputedStyle(badge).backgroundColor;
                const wdlBackgroundDark = getComputedStyle(wdlBadge).backgroundColor;
                document.documentElement.classList.add('light');
                const successLight = getComputedStyle(badge).backgroundColor;
                const wdlBackgroundLight = getComputedStyle(wdlBadge).backgroundColor;
                badge.classList.replace('main__badge--success', 'main__badge--error');
                const errorLight = getComputedStyle(badge).backgroundColor;
                document.documentElement.classList.remove('light');
                return {
                  errorDark,
                  successDark,
                  errorLight,
                  successLight,
                  copyTransition:
                    getComputedStyle(document.querySelector('.source-card__copy'))
                      .transitionDuration,
                  runWithTransition:
                    getComputedStyle(
                      document.querySelector('.main__run-with-toggle-label')
                    ).transitionDuration,
                  wdlBackgroundDark,
                  wdlBackgroundLight,
                };
                "#,
                Vec::new(),
            )
            .await?;
        let styles = styles.json();
        let error_dark = styles["errorDark"]
            .as_str()
            .context("expected dark-mode error badge background color")?;
        let success_dark = styles["successDark"]
            .as_str()
            .context("expected dark-mode success badge background color")?;
        let error_light = styles["errorLight"]
            .as_str()
            .context("expected light-mode error badge background color")?;
        let success_light = styles["successLight"]
            .as_str()
            .context("expected light-mode success badge background color")?;

        if error_dark != "oklch(0.704 0.191 22.216)" {
            bail!("expected a solid dark-mode red badge background, found `{error_dark}`");
        }
        if success_dark != "oklch(0.765 0.177 163.223)" {
            bail!("expected a solid dark-mode green badge background, found `{success_dark}`");
        }
        if error_light != "oklch(0.936 0.032 17.717)" {
            bail!("expected a muted light-mode red badge background, found `{error_light}`");
        }
        if success_light != "oklch(0.95 0.052 163.051)" {
            bail!("expected a muted light-mode green badge background, found `{success_light}`");
        }
        if styles["copyTransition"].as_str() != Some("0s") {
            bail!("expected the source copy button to remain static");
        }
        if styles["runWithTransition"].as_str() != Some("0s") {
            bail!("expected the run-with control to remain static");
        }
        if styles["wdlBackgroundDark"].as_str() != Some("rgb(10, 12, 18)") {
            bail!("expected the dark WDL badge background to match its icon");
        }
        if styles["wdlBackgroundLight"].as_str() != Some("rgb(241, 243, 249)") {
            bail!("expected a near-white light-mode WDL badge background");
        }

        Ok(())
    }
}
