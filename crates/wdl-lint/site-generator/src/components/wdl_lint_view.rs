use maud::PreEscaped;
use maud::html;

use crate::components::LintRuleSource;

pub fn wdl_lint_view() -> PreEscaped<String> {
    let filters = html! {
        (super::tag_filter())
        (super::version_filter())
        span class="ml-auto" x-text="lintVersion" {}
    };

    html! {
        (super::filters(filters))

        (super::lint_rule_list(LintRuleSource::WdlLint))
    }
}
