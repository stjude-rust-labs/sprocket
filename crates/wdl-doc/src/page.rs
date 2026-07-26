//! Shared building blocks for WDL declaration pages.

use std::path::Path;

use maud::Markup;
use maud::Render;
use maud::html;

/// A rich header for a WDL declaration page.
///
/// Renders the declaration kind, a sans-serif title, an authored summary,
/// badges, and an optional source card showing the workspace-relative path.
pub(crate) struct DeclarationHero<'a> {
    /// The declaration kind label (e.g. `Struct`, `Task`).
    kind: &'a str,
    /// An optional class applied to the kind label for accent coloring.
    kind_class: Option<&'a str>,
    /// The Pagefind type filter value (e.g. `struct`), rendered as
    /// `type:{value}`.
    pagefind_type: Option<&'a str>,
    /// The declaration name shown as the title.
    name: &'a str,
    /// The rendered summary shown beneath the title.
    summary: Markup,
    /// The workspace-relative path to the source WDL document, if any.
    source_path: Option<&'a Path>,
    /// Badges rendered alongside the title.
    badges: Vec<Markup>,
}

impl<'a> DeclarationHero<'a> {
    /// Create a new hero for a declaration of the given `kind` and `name`.
    ///
    /// The `summary` is rendered immediately and may be any [`Render`] value,
    /// such as a plain string or previously rendered [`Markup`].
    pub(crate) fn new(kind: &'a str, name: &'a str, summary: impl Render) -> Self {
        Self {
            kind,
            kind_class: None,
            pagefind_type: None,
            name,
            summary: summary.render(),
            source_path: None,
            badges: Vec::new(),
        }
    }

    /// Set the accent class applied to the kind label.
    pub(crate) fn kind_class(mut self, class: &'a str) -> Self {
        self.kind_class = Some(class);
        self
    }

    /// Set the Pagefind type filter value for the kind label.
    pub(crate) fn pagefind_type(mut self, ty: &'a str) -> Self {
        self.pagefind_type = Some(ty);
        self
    }

    /// Set the workspace-relative source path shown in the source card.
    pub(crate) fn source_path(mut self, path: &'a Path) -> Self {
        self.source_path = Some(path);
        self
    }

    /// Append a badge to be rendered alongside the title.
    pub(crate) fn badge(mut self, badge: Markup) -> Self {
        self.badges.push(badge);
        self
    }

    /// Render the hero as HTML.
    pub(crate) fn render(&self) -> Markup {
        let kind_class = match self.kind_class {
            Some(class) => format!("declaration-hero__kind {class}"),
            None => "declaration-hero__kind".to_string(),
        };
        let pagefind_filter = self.pagefind_type.map(|ty| format!("type:{ty}"));

        html! {
            header class="declaration-hero" {
                p class=(kind_class) data-pagefind-filter=[pagefind_filter] {
                    (self.kind)
                }
                h1 id="title" data-pagefind-meta="title" class="declaration-hero__title" {
                    (self.name)
                }
                div class="declaration-hero__summary markdown-body" {
                    (self.summary)
                }
                @if !self.badges.is_empty() {
                    div class="main__badge-container" {
                        @for badge in &self.badges {
                            (badge)
                        }
                    }
                }
                @if let Some(path) = self.source_path {
                    div class="source-card" {
                        span class="source-card__icon" { "▱" }
                        code class="source-card__path" { (path.to_string_lossy()) }
                        button
                            type="button"
                            class="source-card__copy"
                            aria-label="Copy source path"
                            x-on:click=(format!("navigator.clipboard.writeText({path:?})"))
                        { "Copy" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DeclarationHero;

    #[test]
    fn declaration_hero_renders_title_and_source_card() {
        let html = DeclarationHero::new("Struct", "Sample", "A sequencing sample.")
            .source_path(Path::new("main.wdl"))
            .render()
            .into_string();
        assert!(html.contains("declaration-hero__title\">Sample"));
        assert!(html.contains("source-card__path\">main.wdl"));
        assert!(html.contains("navigator.clipboard.writeText"));
        assert!(!html.contains("<code>Sample</code>"));
    }
}
