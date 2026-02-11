//! Defines the version dropdown component.

use maud::PreEscaped;
use maud::html;

use crate::components::LintRuleSource;

/// A filter for crate versions.
pub fn version_filter(source: LintRuleSource) -> PreEscaped<String> {
    let source = match source {
        LintRuleSource::WdlLint => "wdlLint",
        LintRuleSource::WdlAnalysis => "wdlAnalysis",
    };

    let dropdown_menu = html! {
        li {
            label for="versionSelector" x-text=(format!("`Up to: v${{{source}.allVersions[{source}.filteredVersion]}}`")) {}
            input
                    name="versionSelector"
                    type="range"
                    min="0"
                    ":max"=(format!("{source}.allVersions.length - 1"))
                    "x-model.number"=(format!("{source}.filteredVersion"))
                    step="1"
                {}
        }
    };

    super::dropdown("version-filter", "Versions", dropdown_menu)
}
