//! Basic UI tests.

use std::collections::HashMap;
use std::sync::Arc;

use crate::UiTest;

mod search;
mod search_invalid;
mod toggle_theme;

/// All tests in this category.
pub fn all_tests() -> HashMap<&'static str, Arc<dyn UiTest>> {
    let tests: Vec<Arc<dyn UiTest>> = vec![
        Arc::new(toggle_theme::ToggleTheme),
        Arc::new(search::Search),
        Arc::new(search_invalid::SearchInvalid),
    ];

    tests.into_iter().map(|test| (test.name(), test)).collect()
}
