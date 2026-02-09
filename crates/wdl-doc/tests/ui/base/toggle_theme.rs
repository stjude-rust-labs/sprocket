//! Test for the light/dark theme toggle.

use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::WebElement;
use thirtyfour::prelude::ElementWaitable;

use crate::UiTest;
use crate::WebDriverExt;

// Colors from wdl-doc/theme/src/colors.css
/// The expected background color of the dark theme.
const BACKGROUND_DARK: &str = "rgba(7, 10, 26, 1)";
/// The expected background color of the light theme.
const BACKGROUND_LIGHT: &str = "rgba(248, 249, 252, 1)";

/// Test for the light/dark theme toggle.
pub struct ToggleTheme;

#[async_trait::async_trait]
impl UiTest for ToggleTheme {
    fn name(&self) -> &'static str {
        "toggle_theme"
    }

    async fn run(&self, driver: &mut WebDriver) -> anyhow::Result<()> {
        if let Some(theme) = driver.localstorage("theme").await? {
            bail!("expected `localStorage.theme` to be empty, found: {theme}");
        }

        // Theme should be dark by default
        let bg = driver.find(By::ClassName("layout__container")).await?;
        let current_color = bg.css_value("background-color").await?;
        if current_color != BACKGROUND_DARK {
            bail!(
                "expected dark theme background color to be {BACKGROUND_DARK}, found \
                 {current_color}"
            );
        }

        let toggle_button = driver.find(By::Id("theme-toggle")).await?;
        toggle_button.click().await?;

        bg.wait_until()
            .wait(Duration::from_millis(10), Duration::from_millis(5))
            .condition(move |elem: WebElement| {
                let current_color = current_color.clone();
                async move {
                    let bg_color = elem.css_value("background-color").await?;
                    Ok(bg_color != current_color)
                }
            })
            .await?;

        if driver.localstorage("theme").await?.as_deref() != Some("light") {
            bail!("expected light theme to be stored in `localStorage`");
        }

        let current_color = bg.css_value("background-color").await?;
        if current_color != BACKGROUND_LIGHT {
            bail!(
                "expected light theme background color to be {BACKGROUND_LIGHT}, found \
                 {current_color}"
            );
        }

        Ok(())
    }
}
