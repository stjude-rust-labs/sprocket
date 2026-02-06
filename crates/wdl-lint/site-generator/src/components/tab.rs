use maud::PreEscaped;
use maud::html;

pub fn tab(name: &'static str) -> PreEscaped<String> {
    html! {
        button
            ":class"=(format!("tab === '{name}' ? 'bg-slate-700 text-white shadow-sm' : 'text-slate-400 hover:text-slate-200'"))
            "@click"=(format!("switchTab('{name}')"))
            class="px-6 py-2 rounded-lg text-sm font-medium transition-all duration-200"
        { (name) }
    }
}
