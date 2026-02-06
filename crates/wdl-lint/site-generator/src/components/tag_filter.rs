use maud::PreEscaped;
use maud::html;
use strum::VariantArray;
use wdl_lint::Tag;

use crate::default_tags;

pub fn tag_filter() -> PreEscaped<String> {
    let dropdown_menu = html! {
        li class="checkbox" {
            button class="reset-all" "@click"=(format!("activeTags = {}", default_tags())) { ("All") }
        }
        li class="checkbox" {
            button class="reset-none" "@click"="activeTags = []" { ("None") }
        }
        li role="separator" class="divider" {}
        @for tag in Tag::VARIANTS {
            li class="checkbox" {
                label class="flex items-center gap-3 px-2 py-1 rounded hover:bg-slate-800 cursor-pointer text-sm text-slate-300" "@mousedown.prevent" {
                    input
                        type="checkbox"
                        ":checked"=(format!("activeTags.includes('{tag}')"))
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
