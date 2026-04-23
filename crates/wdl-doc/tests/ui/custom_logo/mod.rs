//! Custom logo tests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;

use crate::UiTest;

#[allow(clippy::module_inception)]
mod custom_logo;
mod custom_logo_alt;

/// All tests in this category.
pub fn all_tests() -> HashMap<&'static str, Arc<dyn UiTest>> {
    let tests: Vec<Arc<dyn UiTest>> = vec![
        Arc::new(custom_logo::CustomLogo),
        Arc::new(custom_logo_alt::CustomLogoAlt),
    ];

    tests.into_iter().map(|test| (test.name(), test)).collect()
}

/// Used by [`custom_logo::CustomLogo`] and [`custom_logo_alt::CustomLogoAlt`]
/// to verify logo generation.
async fn assert_logo_variants(
    driver: &mut WebDriver,
    docs_path: &Path,
    has_custom_light_logo: bool,
) -> anyhow::Result<()> {
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

    let mut found_dark = false;
    let mut found_light = false;
    for image in images {
        let src = image.attr("src").await?;

        let original_svg_path;
        let generated_svg_path;
        if src.as_deref() == Some(EXPECTED_IMAGE_DARK) {
            generated_svg_path = docs_path.join(EXPECTED_IMAGE_DARK);
            original_svg_path = category_dir.join("assets").join("test-logo.svg");
            found_dark = true;
        } else if src.as_deref() == Some(EXPECTED_IMAGE_LIGHT) {
            generated_svg_path = docs_path.join(EXPECTED_IMAGE_LIGHT);
            if has_custom_light_logo {
                original_svg_path = category_dir.join("assets").join("test-logo.light.svg");
            } else {
                // Otherwise the light mode logo is the same as dark mode
                original_svg_path = category_dir.join("assets").join("test-logo.svg");
            }
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
