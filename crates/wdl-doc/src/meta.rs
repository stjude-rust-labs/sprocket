//! Create HTML documentation for WDL meta sections.

use std::collections::BTreeMap;
use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::MetadataValue;

use crate::Markdown;
use crate::Render;

/// The key used to specify the description in a meta entry.
pub(crate) const DESCRIPTION_KEY: &str = "description";
/// Help key for custom rendering.
const HELP_KEY: &str = "help";
/// External help key for custom rendering.
const EXTERNAL_HELP_KEY: &str = "external_help";
/// Warning key for custom rendering.
const WARNING_KEY: &str = "warning";

/// The maximum length of a description before it is summarized.
const DESCRIPTION_MAX_LENGTH: usize = 140;
/// The length of a description when summarized.
const DESCRIPTION_CLIP_LENGTH: usize = 80;

/// A map of metadata key-value pairs, sorted by key.
pub(crate) type MetaMap = BTreeMap<String, MetadataValue>;

/// An extension trait for [`MetaMap`] to provide additional functionality
/// commonly used in WDL documentation generation.
pub(crate) trait MetaMapExt {
    /// Returns the rendered [`Markup`] of the `description` key, optionally
    /// summarizing it.
    ///
    /// This will always return some text; in the absence of a `description`
    /// key, it will return a default message ("No description provided").
    fn render_description(&self, summarize: bool) -> Markup;
    /// Returns the rendered [`Markup`] of the remaining metadata keys,
    /// excluding the keys specified in `filter_keys`.
    fn render_remaining(&self, filter_keys: &[&str], assets: &Path) -> Option<Markup>;
}

impl MetaMapExt for MetaMap {
    fn render_description(&self, summarize: bool) -> Markup {
        let desc = self
            .get(DESCRIPTION_KEY)
            .map(|v| match v {
                MetadataValue::String(s) => {
                    let t = s.text().expect("meta string should not be interpolated");
                    t.text().to_string()
                }
                _ => "ERROR: description not of type String".to_string(),
            })
            .unwrap_or_else(|| "No description provided".to_string());

        if !summarize {
            return Markdown(desc).render();
        }

        match summarize_if_needed(desc, DESCRIPTION_MAX_LENGTH, DESCRIPTION_CLIP_LENGTH) {
            MaybeSummarized::No(desc) => Markdown(desc).render(),
            MaybeSummarized::Yes(summary) => {
                html! {
                    div class="main__summary-container" {
                        (summary)
                        "..."
                        button type="button" class="main__button" x-on:click="description_expanded = !description_expanded" {
                            b x-text="description_expanded ? 'Hide full description' : 'Show full description'" {}
                        }
                    }
                }
            }
        }
    }

    fn render_remaining(&self, filter_keys: &[&str], assets: &Path) -> Option<Markup> {
        let custom_keys = &[HELP_KEY, EXTERNAL_HELP_KEY, WARNING_KEY];
        let filtered_items = self
            .iter()
            .filter(|(k, _v)| {
                !filter_keys.contains(&k.as_str()) && !custom_keys.contains(&k.as_str())
            })
            .collect::<Vec<_>>();

        let help_item = self.get(HELP_KEY);
        let external_help_item = self.get(EXTERNAL_HELP_KEY);
        let warning_item = self.get(WARNING_KEY);

        let any_additional_items = !filtered_items.is_empty();
        let custom_key_present =
            help_item.is_some() || external_help_item.is_some() || warning_item.is_some();

        if !(any_additional_items || custom_key_present) {
            return None;
        }

        let external_link_on_click = if let Some(MetadataValue::String(s)) = external_help_item {
            Some(format!(
                "window.open('{}', '_blank')",
                s.text()
                    .expect("meta string should not be interpolated")
                    .text()
            ))
        } else {
            None
        };

        Some(html! {
            @if let Some(help) = help_item {
                div class="markdown-body" {
                    (render_value(help))
                }
            }
            @if let Some(on_click) = external_link_on_click {
                button type="button" class="main__button" x-on:click=(on_click) {
                    b { "Go to External Documentation" }
                    img src=(assets.join("link.svg").to_string_lossy()) alt="External Documentation Icon" class="size-5";
                }
            }
            @if let Some(warning) = warning_item {
                div class="metadata__warning" {
                    img src=(assets.join("information-circle.svg").to_string_lossy()) alt="Warning Icon" class="size-5";
                    p { (render_value(warning)) }
                }
            }
            @if any_additional_items {
                div class="main__grid-nested-container" {
                    // No header row, just the items
                    @for (key, value) in filtered_items {
                        (render_key_value(key, value))
                    }
                }
            }
        })
    }
}

/// Recursively render a [`MetadataValue`] as HTML.
fn render_value(value: &MetadataValue) -> Markup {
    match value {
        MetadataValue::String(s) => {
            let inner_text = s
                .text()
                .map(|t| t.text().to_string())
                .expect("meta string should not be interpolated");
            Markdown(inner_text).render()
        }
        MetadataValue::Boolean(b) => html! { code { (b.text()) } },
        MetadataValue::Integer(i) => html! { code { (i.text()) } },
        MetadataValue::Float(f) => html! { code { (f.text()) } },
        MetadataValue::Null(n) => html! { code { (n.text()) } },
        MetadataValue::Array(a) => {
            html! {
                div class="main__grid-meta-array-container" {
                    @for item in a.elements() {
                        @match item {
                            MetadataValue::Array(_) | MetadataValue::Object(_) => {
                                // This is going to render weirdly. I (a-frantz)
                                // don't have a real example case for this,
                                // so I'm leaving it as is for now. This would be a very
                                // odd structure in WDL metadata, but it is valid.
                                (render_value(&item))
                            }
                            _ => {
                                div class="main__grid-meta-array-item" {
                                    code { (item.text()) }
                                }
                            }
                        }
                    }
                }
            }
        }
        MetadataValue::Object(o) => {
            html! {
                div class="main__grid-nested-container" {
                    @for item in o.items() {
                        (render_key_value(item.name().text(), &item.value()))
                    }
                }
            }
        }
    }
}

/// Render a key-value pair from metadata as HTML.
///
/// This function assumes that it is called for rendering within a grid layout,
/// where the key is displayed in the left cell and the value in the right cell.
///
/// A notable difference from [`render_value`] is that this function will _not_
/// render WDL Strings as Markdown, but rather as code snippets. The reason
/// for this is that the key-value pairs are typically used to display metadata
/// in a grid format, where the value is expected to be a simple code snippet
/// rather than full Markdown-rendered text.
fn render_key_value(key: &str, value: &MetadataValue) -> Markup {
    let (ty, rhs_markup) = match value {
        MetadataValue::String(s) => (
            s.inner().kind(),
            html! { code { (s.text().expect("meta string should not be interpolated").text()) } },
        ),
        MetadataValue::Boolean(b) => (b.inner().kind(), html! { code { (b.text()) } }),
        MetadataValue::Integer(i) => (i.inner().kind(), html! { code { (i.text()) } }),
        MetadataValue::Float(f) => (f.inner().kind(), html! { code { (f.text()) } }),
        MetadataValue::Null(n) => (n.inner().kind(), html! { code { (n.text()) } }),
        MetadataValue::Array(a) => {
            let markup = html! {
                div class="main__grid-meta-array-container" {
                    @for item in a.elements() {
                        @match item {
                            MetadataValue::Array(_) | MetadataValue::Object(_) => {
                                // TODO: revisit this
                                (render_value(&item))
                            }
                            _ => {
                                div class="main__grid-meta-array-item" {
                                    code { (item.text()) }
                                }
                            }
                        }
                    }
                }
            };
            (a.inner().kind(), markup)
        }
        MetadataValue::Object(o) => {
            let markup = html! {
                div class="main__grid-nested-container" {
                    @for item in o.items() {
                        (render_key_value(item.name().text(), &item.value()))
                    }
                }
            };
            (o.inner().kind(), markup)
        }
    };

    let lhs_markup = match ty {
        SyntaxKind::MetadataArrayNode | SyntaxKind::MetadataObjectNode => {
            // TODO: special icon for arrays and objects
            html! { code { (key) } }
        }
        _ => {
            // For other types, just render the key as code
            html! { code { (key) } }
        }
    };

    html! {
        div class="main__grid-nested-row" {
            div class="main__grid-nested-cell" {
                (lhs_markup)
            }
            div class="main__grid-nested-cell" {
                (rhs_markup)
            }
        }
    }
}

/// A string that may be summarized.
#[derive(Debug)]
pub(crate) enum MaybeSummarized {
    /// The string was truncated, providing a summary.
    Yes(String),
    /// The string was not truncated, providing the full thing.
    No(String),
}

/// Summarize a string if it exceeds a maximum length.
pub(crate) fn summarize_if_needed(
    in_string: String,
    max_length: usize,
    clip_length: usize,
) -> MaybeSummarized {
    if in_string.len() > max_length {
        MaybeSummarized::Yes(in_string[..clip_length].trim_end().to_string())
    } else {
        MaybeSummarized::No(in_string)
    }
}
