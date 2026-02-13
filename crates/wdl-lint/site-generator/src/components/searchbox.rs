//! Defines the search box component.

use maud::PreEscaped;
use maud::html;

/// The global search box.
pub fn searchbox() -> PreEscaped<String> {
    html! {
        div class="relative flex-1 max-w-md" {
            input
                id="searchbox"
                x-ref="searchBox"
                "x-model.debounce"="search"
                type="text"
                placeholder="Search..."
                class="block w-full pl-12 pr-10 py-2 bg-slate-900 border border-slate-800 rounded-lg text-sm placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500 transition-all text-slate-100"
                "@keydown"="this.search = event.target.textContent.toLowerCase()"
            {}
            div "@keydown.window.slash"="focusSearch($event)" {}
            img src="assets/search.svg" class="absolute left-2 top-1/2 -translate-y-1/2 size-6 pointer-events-none block light:hidden" alt="Search icon";
            img src="assets/search.light.svg" class="absolute left-2 top-1/2 -translate-y-1/2 size-6 pointer-events-none hidden light:block" alt="Search icon";
            img src="assets/x-mark.svg" class="absolute right-2 top-1/2 -translate-y-1/2 size-6 hover:cursor-pointer block light:hidden" alt="Clear icon" x-show="search !== ''" x-on:click="search = ''";
            img src="assets/x-mark.light.svg" class="absolute right-2 top-1/2 -translate-y-1/2 size-6 hover:cursor-pointer hidden light:block" alt="Clear icon" x-show="search !== ''" x-on:click="search = ''";
        }
    }
}
