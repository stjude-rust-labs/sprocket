//! Create HTML documentation for WDL meta sections.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::DOC_COMMENT_PREFIX;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::MetadataObjectItem;
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

/// The default description used on undocumented items.
pub(crate) const DEFAULT_DESCRIPTION: &str = "No description provided";

/// Parse [`MetadataObjectItem`]s into a [`MetaMap`].
pub(crate) fn parse_metadata_items(meta: impl Iterator<Item = MetadataObjectItem>) -> MetaMap {
    meta.map(|m| {
        let name = m.name().text().to_owned();
        let item = m.value();
        (name, MetaMapValueSource::MetaValue(item))
    })
    .collect()
}

/// The source of a [`MetaMap`] entry.
#[derive(Debug, Clone)]
pub(crate) enum MetaMapValueSource {
    /// The value comes from a `meta`/`parameter_meta` section in the document.
    MetaValue(MetadataValue),
    /// The value comes from a doc comment.
    Comment(String),
}

impl MetaMapValueSource {
    /// Get the text representation of this value, if possible
    ///
    /// For `Comment` values, this will always return a value.
    /// For `MetaValue` values, this will only return if the value is
    /// [`MetadataValue::String`].
    pub fn text(&self) -> Option<String> {
        match self {
            MetaMapValueSource::Comment(text) => Some(text.clone()),
            MetaMapValueSource::MetaValue(MetadataValue::String(s)) => Some(
                s.text()
                    .expect("meta string should not be interpolated")
                    .text()
                    .to_string(),
            ),
            _ => None,
        }
    }

    /// Consumes the value, returning a [`MetadataValue`] if the variant is
    /// `MetaValue`
    #[cfg(test)]
    pub fn into_meta(self) -> Option<MetadataValue> {
        match self {
            MetaMapValueSource::MetaValue(meta) => Some(meta),
            _ => None,
        }
    }
}

/// A map of metadata key-value pairs, sorted by key.
pub(crate) type MetaMap = BTreeMap<String, MetaMapValueSource>;

/// An extension trait for [`MetaMap`] to provide additional functionality
/// commonly used in WDL documentation generation.
pub(crate) trait MetaMapExt {
    /// Returns the "full" description for an item.
    ///
    /// This is a concatenation of `description` and `help`. If neither is
    /// present, this will return `None`.
    fn full_description(&self) -> Option<String>;
    /// Get the `description` key as text.
    fn description(&self) -> Option<String>;
    /// Returns the rendered [`Markup`] of the `description` key, optionally
    /// summarizing it.
    ///
    /// This will always return some text; in the absence of a `description`
    /// key, it will return a default message ([`DEFAULT_DESCRIPTION`]).
    fn render_description(&self, summarize: bool) -> Markup;
    /// Returns the rendered [`Markup`] of the [`self.full_description()`].
    ///
    /// See [`Self::render_description()`] for defaults.
    fn render_full_description(&self, summarize: bool) -> Markup;
    /// Returns the rendered [`Markup`] of the remaining metadata keys,
    /// excluding the keys specified in `filter_keys`.
    fn render_remaining(&self, filter_keys: &[&str], assets: &Path) -> Option<Markup>;
}

/// Render `text` as Markdown, optionally summarizing it.
fn maybe_summarize_text(text: String, summarize: bool) -> Markup {
    if !summarize {
        return Markdown(text).render();
    }

    match summarize_if_needed(text, DESCRIPTION_MAX_LENGTH, DESCRIPTION_CLIP_LENGTH) {
        MaybeSummarized::No(desc) => Markdown(desc).render(),
        MaybeSummarized::Yes(summary) => {
            html! {
                div class="main__summary-container" {
                    (Markdown(summary))
                    "..."
                    button type="button" class="main__button" x-on:click="description_expanded = !description_expanded" {
                        b x-text="description_expanded ? 'Hide full description' : 'Show full description'" {}
                    }
                }
            }
        }
    }
}

impl MetaMapExt for MetaMap {
    fn full_description(&self) -> Option<String> {
        let help = self.get(HELP_KEY).and_then(MetaMapValueSource::text);

        if let Some(mut description) = self.description() {
            if let Some(help) = help {
                description.push_str("\n\n");
                description.push_str(&help);
            }

            return Some(description);
        }

        help
    }

    fn description(&self) -> Option<String> {
        self.get(DESCRIPTION_KEY).and_then(MetaMapValueSource::text)
    }

    fn render_description(&self, summarize: bool) -> Markup {
        let desc = self
            .description()
            .unwrap_or_else(|| DEFAULT_DESCRIPTION.to_string());

        maybe_summarize_text(desc, summarize)
    }

    fn render_full_description(&self, summarize: bool) -> Markup {
        let desc = self
            .full_description()
            .unwrap_or_else(|| DEFAULT_DESCRIPTION.to_string());

        maybe_summarize_text(desc, summarize)
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

        let external_link_on_click =
            if let Some(MetaMapValueSource::MetaValue(MetadataValue::String(s))) =
                external_help_item
            {
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
                    img src=(assets.join("link.svg").to_string_lossy()) alt="External Documentation Icon" class="size-5 block light:hidden";
                    img src=(assets.join("link.light.svg").to_string_lossy()) alt="External Documentation Icon" class="size-5 hidden light:block";
                }
            }
            @if let Some(warning) = warning_item {
                div class="metadata__warning" {
                    img src=(assets.join("information-circle.svg").to_string_lossy()) alt="Warning Icon" class="size-5 block light:hidden";
                    img src=(assets.join("information-circle.light.svg").to_string_lossy()) alt="Warning Icon" class="size-5 hidden light:block";
                    p { (render_value(warning)) }
                }
            }
            @if any_additional_items {
                div class="main__grid-nested-container" {
                    // No header row, just the items
                    @for (key, value) in filtered_items {
                        @if let MetaMapValueSource::MetaValue(value) = value {
                            (render_key_value(key, value))
                        }
                    }
                }
            }
        })
    }
}

/// Recursively render a [`MetaMapValueSource`] as HTML.
fn render_value(value: &MetaMapValueSource) -> Markup {
    match value {
        MetaMapValueSource::Comment(comment) => render_string(comment),
        MetaMapValueSource::MetaValue(meta) => render_metadata_value(meta),
    }
}

/// Render a [`MetadataValue`] as HTML.
fn render_metadata_value(value: &MetadataValue) -> Markup {
    match value {
        MetadataValue::String(s) => s
            .text()
            .map(|t| render_string(t.text()))
            .expect("meta string should not be interpolated"),
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
                                (render_metadata_value(&item))
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

/// Prepare a string for HTML rendering.
fn render_string(s: &str) -> Markup {
    Markdown(s).render()
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
                                (render_metadata_value(&item))
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

/// A doc comment paragraph
#[derive(Debug, Clone, Default)]
pub struct Paragraph(Vec<String>);

impl Display for Paragraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("\n"))
    }
}

impl Deref for Paragraph {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Paragraph {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Collect all doc comments preceding `token` into a [`MetaMap`]
///
/// The first paragraph of the doc comment text will be placed under the
/// `description` key of the map. All other paragraphs will be joined with
/// newlines and placed under the `help` key.
pub(crate) fn doc_comments(comments: impl IntoIterator<Item = Comment>) -> MetaMap {
    let mut map = MetaMap::new();

    let mut current_paragraph = Paragraph::default();
    let mut paragraphs = Vec::new();
    for doc_comment in comments {
        let Some(comment) = doc_comment.text().strip_prefix(DOC_COMMENT_PREFIX) else {
            continue;
        };

        if comment.trim().is_empty() {
            paragraphs.push(current_paragraph);
            current_paragraph = Paragraph::default();
            continue;
        }

        current_paragraph.push(comment.to_owned());
    }

    if !current_paragraph.is_empty() {
        paragraphs.push(current_paragraph);
    }

    if paragraphs.is_empty() {
        return map;
    }

    // We need to determine the minimum indentation that we can strip from each
    // paragraph line. Prior to this point, no lines have been trimmed.
    //
    // In the most common case, we'll just be stripping a single space between the
    // `##` and the comment text, as is convention.
    let min_indent = paragraphs
        .iter()
        .map(|paragraph| {
            paragraph
                .iter()
                .filter(|line| line.chars().any(|c| !c.is_whitespace()))
                .map(|line| line.chars().take_while(|c| *c == ' ' || *c == '\t').count())
                .min()
                .unwrap_or(usize::MAX)
        })
        .min()
        .unwrap_or(0);

    for paragraph in &mut paragraphs {
        for line in paragraph
            .iter_mut()
            .filter(|line| !line.chars().all(char::is_whitespace))
        {
            assert!(line.len() > min_indent);
            *line = line.split_off(min_indent);
        }
    }

    let mut paragraphs = paragraphs.into_iter();

    map.insert(
        DESCRIPTION_KEY.to_string(),
        // SAFETY: if paragraphs were empty, we would have returned early
        MetaMapValueSource::Comment(paragraphs.next().unwrap().to_string()),
    );

    let help = paragraphs.fold(String::new(), |mut acc, p| {
        if !acc.is_empty() {
            acc.push_str("\n\n");
        }

        acc.push_str(&p.to_string());
        acc
    });

    if !help.is_empty() {
        map.insert(HELP_KEY.to_string(), MetaMapValueSource::Comment(help));
    }

    map
}

/// An extension trait for working with item definitions with an associated
/// [`MetaMap`].
pub(crate) trait DefinitionMeta {
    /// Get the [`MetaMap`] of the item.
    fn meta(&self) -> &MetaMap;

    /// Render the description of the item as HTML.
    ///
    /// This will always return some text; in the absence of a `description`
    /// key, it will return a default message ("No description provided").
    fn render_description(&self, summarize: bool) -> Markup {
        self.meta().render_description(summarize)
    }
}
