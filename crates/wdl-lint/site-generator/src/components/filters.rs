//! Defines the filter list component.

use maud::PreEscaped;
use maud::html;

/// A list of filters that can be used on the current tab.
pub fn filters(children: PreEscaped<String>) -> PreEscaped<String> {
    html! {
        div class="flex items-center gap-3 mb-6 pb-4 border-b border-slate-800" {
            span class="text-xs font-bold uppercase text-slate-500" { "Filters:" }
            (children)
        }
    }
}
