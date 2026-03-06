//@args:--logo assets/test-logo.svg
//! Test for a custom logo.

use std::path::Path;
use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;

/// Test for a custom logo.
pub struct CustomLogo;

#[async_trait::async_trait]
impl UiTest for CustomLogo {
    fn name(&self) -> &'static str {
        "custom_logo"
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

        let category_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("ui")
            .join("custom_logo");
        let original_image_path = category_dir.join("assets").join("test-logo.svg");

        let mut found_dark = false;
        let mut found_light = false;
        for image in images {
            let generated_image_path;

            let src = image.attr("src").await?;
            match src.as_deref() {
                Some(EXPECTED_IMAGE_DARK) => {
                    generated_image_path = docs_path.join(EXPECTED_IMAGE_DARK);
                    found_dark = true;
                }
                Some(EXPECTED_IMAGE_LIGHT) => {
                    generated_image_path = docs_path.join(EXPECTED_IMAGE_DARK);
                    found_light = true;
                }
                _ => bail!("Unexpected logo image source '{src:?}'"),
            }

            let generated_image_content = std::fs::read_to_string(&generated_image_path)?;
            let original_image_content = std::fs::read_to_string(&original_image_path)?;
            if original_image_content != generated_image_content {
                bail!(
                    "Expected generated logo at '{}' to match original at '{}'",
                    generated_image_path.display(),
                    original_image_path.display()
                )
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
