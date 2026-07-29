//! Basic UI tests.

use std::collections::HashMap;
use std::sync::Arc;

use crate::UiTest;

mod code_block;
mod markdown_fence;
mod mobile_layout;
mod page_navigation;
mod scrollbar_autohide;
mod search;
mod search_invalid;
mod status_badge;
mod toggle_theme;

/// All tests in this category.
pub fn all_tests() -> HashMap<&'static str, Arc<dyn UiTest>> {
    let tests: Vec<Arc<dyn UiTest>> = vec![
        Arc::new(toggle_theme::ToggleTheme),
        Arc::new(search::Search),
        Arc::new(search_invalid::SearchInvalid),
        Arc::new(page_navigation::PageNavigation),
        Arc::new(code_block::CodeBlock),
        Arc::new(markdown_fence::MarkdownFence),
        Arc::new(mobile_layout::MobileLayout),
        Arc::new(scrollbar_autohide::ScrollbarAutohide),
        Arc::new(status_badge::StatusBadge),
    ];

    tests.into_iter().map(|test| (test.name(), test)).collect()
}
