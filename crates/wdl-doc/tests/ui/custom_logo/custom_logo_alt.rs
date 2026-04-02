//@args:--logo assets/test-logo.svg --alt-light-logo assets/test-logo.light.svg
//! Test for a custom alt light mode logo.

use std::path::Path;
use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;

/// Test for a custom alt light mode logo.
pub struct CustomLogoAlt;

#[async_trait::async_trait]
impl UiTest for CustomLogoAlt {
    fn name(&self) -> &'static str {
        "custom_logo_alt"
    }

    async fn run(&self, driver: &mut WebDriver, docs_path: &Path) -> anyhow::Result<()> {
        const EXPECTED_IMAGE_DARK: &str = "assets/logo.svg";
        const EXPECTED_IMAGE_LIGHT: &str = "assets/logo.light.svg";

        let logo = driver
            .query(By::Id("logo"))
            .wait(Duration::from_secs(5), Duration::from_millis(100))
            .first()
            .await?;
        let images = logo
            .query(By::Tag("img"))
            .all_from_selector_required()
            .await?;
        if images.len() != 2 {
            bail!("Expected two logo variants");
        }

        let mut found_dark = false;
        let mut found_light = false;
        for image in images {
            let src = image.attr("src").await?;

            let category_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("ui")
                .join("custom_logo");

            let original_svg_path;
            let generated_svg_path;
            if src.as_deref() == Some(EXPECTED_IMAGE_DARK) {
                generated_svg_path = docs_path.join(EXPECTED_IMAGE_DARK);
                original_svg_path = category_dir.join("assets").join("test-logo.svg");
                found_dark = true;
            } else if src.as_deref() == Some(EXPECTED_IMAGE_LIGHT) {
                generated_svg_path = docs_path.join(EXPECTED_IMAGE_LIGHT);
                original_svg_path = category_dir.join("assets").join("test-logo.light.svg");
                found_light = true;
            } else {
                continue;
            }

            let generated_svg = std::fs::read_to_string(&generated_svg_path)?;
            let original = std::fs::read_to_string(&original_svg_path)?;
            if original != generated_svg {
                bail!(
                    "Expected generated logo at '{}' to match original at '{}'",
                    generated_svg_path.display(),
                    original_svg_path.display()
                );
            }
        }

        if !found_dark {
            bail!("Expected a dark theme logo at '{EXPECTED_IMAGE_DARK}'");
        }

        if !found_light {
            bail!("Expected a light theme logo at '{EXPECTED_IMAGE_LIGHT}'");
        }

        Ok(())
    }
}
