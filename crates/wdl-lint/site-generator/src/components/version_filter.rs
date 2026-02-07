//! Defines the version dropdown component.

use maud::PreEscaped;
use maud::html;

/// A filter for crate versions.
pub fn version_filter() -> PreEscaped<String> {
    let dropdown_menu = html! {
        li class="checkbox" {
            label {
                input
                    type="checkbox"
                {}
                ("TODO")
            }
        }
    };

    super::dropdown("version-filter", "Versions", dropdown_menu)
}
