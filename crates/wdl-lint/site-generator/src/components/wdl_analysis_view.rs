//! Defines the `wdl-analysis` lint list component.

use maud::PreEscaped;
use maud::html;

use crate::components::LintRuleSource;

/// The content of the `wdl-analysis` tab.
pub fn wdl_analysis_view() -> PreEscaped<String> {
    let filters = html! {
        span class="ml-auto" x-text="wdlAnalysis.currentVersion" {}
    };

    html! {
        (super::filters(filters))

        (super::lint_rule_list(LintRuleSource::WdlAnalysis))
    }
}
