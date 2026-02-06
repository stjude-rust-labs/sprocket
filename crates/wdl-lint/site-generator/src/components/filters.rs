use maud::PreEscaped;
use maud::html;

pub fn filters(children: PreEscaped<String>) -> PreEscaped<String> {
    html! {
        div class="flex items-center gap-3 mb-6 pb-4 border-b border-slate-800" {
            span class="text-xs font-bold uppercase text-slate-500" { "Filters:" }
            (children)
        }
    }
}
