//! Create HTML documentation for WDL meta sections.

use maud::Markup;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::v1::MetadataValue;

use crate::Markdown;
use crate::Render;

/// Render a [`MetadataValue`] as HTML.
pub(crate) fn render_value(value: &MetadataValue) -> Markup {
    match value {
        MetadataValue::String(s) => {
            html! { (Markdown(s.text().map(|t| t.text().to_string()).unwrap_or_default()).render()) }
        }
        MetadataValue::Boolean(b) => html! { code { (b.text().to_string()) } },
        MetadataValue::Integer(i) => html! { code { (i.text().to_string()) } },
        MetadataValue::Float(f) => html! { code { (f.text().to_string()) } },
        MetadataValue::Null(n) => html! { code { (n.text().to_string()) } },
        MetadataValue::Array(a) => {
            html! {
                div {
                    code { "[" }
                    ul {
                        @for item in a.elements() {
                            li {
                                @match item {
                                    MetadataValue::Array(_) | MetadataValue::Object(_) => {
                                        (render_value(&item)) ","
                                    }
                                    _ => {
                                        code { (item.text().to_string()) } ","
                                    }
                                }
                            }
                        }
                    }
                    code { "]" }
                }
            }
        }
        MetadataValue::Object(o) => {
            html! {
                div {
                    code { "{" }
                    ul {
                        @for item in o.items() {
                            li {
                                b { (item.name().text()) ":" } " " (render_value(&item.value())) ","
                            }
                        }
                    }
                    code { "}" }
                }
            }
        }
    }
}
