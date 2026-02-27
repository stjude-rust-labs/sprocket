//! Defines the `wdl-lint` lint list component.

use maud::PreEscaped;
use maud::html;
use strum::VariantArray;
use wdl_lint::Tag;

use crate::components::LintRuleSource;
use crate::default_tags;

/// The content of the `wdl-lint` tab.
pub fn wdl_lint_view() -> PreEscaped<String> {
    let filters = html! {
        (tag_filter())
        span class="ml-auto" x-text="wdlLint.currentVersion" {}
    };

    html! {
        (super::filters(filters))

        (super::lint_rule_list(LintRuleSource::WdlLint))
    }
}

/// A filter for `wdl-lint` tags.
pub fn tag_filter() -> PreEscaped<String> {
    let dropdown_menu = html! {
        li class="checkbox" {
            button class="w-full" "@click"=(format!("wdlLint.activeTags = {}", default_tags())) { ("All") }
        }
        li class="checkbox" {
            button class="w-full" "@click"="wdlLint.activeTags = []" { ("None") }
        }
        li role="separator" class="divider" {}
        @for tag in Tag::VARIANTS {
            li class="checkbox" {
                label class="flex items-center gap-3 px-2 py-1 rounded hover:bg-slate-800 cursor-pointer text-sm text-slate-300" "@mousedown.prevent" {
                    input
                        type="checkbox"
                        ":checked"=(format!("wdlLint.activeTags.includes('{tag}')"))
                        "@click"=(format!("toggleTag('{tag}')"))
                        class="rounded bg-slate-800 border-slate-600"
                    {}
                    (tag)
                }
            }
        }
    };

    super::dropdown("tag-filter", "Tags", dropdown_menu)
}
