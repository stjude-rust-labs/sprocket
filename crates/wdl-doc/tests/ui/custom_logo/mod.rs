//! Custom logo tests.

use std::collections::HashMap;
use std::sync::Arc;

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
