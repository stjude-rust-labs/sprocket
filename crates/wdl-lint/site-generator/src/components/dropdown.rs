use maud::PreEscaped;
use maud::html;

pub fn dropdown(id: &str, title: &str, menu_children: PreEscaped<String>) -> PreEscaped<String> {
    html! {
        div id=(id) class="dropdown" x-data="{ open: false }" "@focusout"="if (!$el.contains($event.relatedTarget)) open = false" {
            button type="button" "@click"="open = !open" {
                span class="main__badge-text" {
                    (title)
                }
            }
            ul class="dropdown-menu" x-show="open" {
                (menu_children)
            }
        }
    }
}
