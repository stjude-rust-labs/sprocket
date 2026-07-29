//! Shared building blocks for WDL declaration pages.

use std::path::Path;

use maud::Markup;
use maud::Render;
use maud::html;

/// How a [`DeclarationHero`] title should be rendered.
///
/// WDL identifiers are shown as code literals, while friendly human-facing
/// display names (such as a workflow's `meta.name`) are shown as plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TitleKind {
    /// A raw WDL identifier, wrapped in `<code class="heading-code-literal">`.
    #[default]
    Identifier,
    /// A friendly human display name, rendered as plain text.
    Plain,
}

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
    /// How the title is rendered (code literal vs. plain text).
    title_kind: TitleKind,
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
    ///
    /// The title defaults to [`TitleKind::Identifier`], rendering `name` as a
    /// code literal; call [`DeclarationHero::title_kind`] to override this for
    /// friendly display names.
    pub(crate) fn new(kind: &'a str, name: &'a str, summary: impl Render) -> Self {
        Self {
            kind,
            kind_class: None,
            pagefind_type: None,
            name,
            title_kind: TitleKind::default(),
            summary: summary.render(),
            source_path: None,
            badges: Vec::new(),
        }
    }

    /// Set how the title is rendered.
    ///
    /// Use [`TitleKind::Plain`] when `name` is a friendly display name rather
    /// than a WDL identifier.
    pub(crate) fn title_kind(mut self, kind: TitleKind) -> Self {
        self.title_kind = kind;
        self
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
    pub(crate) fn render(&self, assets: &Path) -> Markup {
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
                    @match self.title_kind {
                        TitleKind::Identifier => code class="heading-code-literal" { (self.name) },
                        TitleKind::Plain => (self.name),
                    }
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
                        span class="source-card__icon" aria-hidden="true" {
                            img
                                src=(assets.join("folder.dark.svg").to_string_lossy())
                                class="block light:hidden"
                                alt="";
                            img
                                src=(assets.join("folder.light.svg").to_string_lossy())
                                class="hidden light:block"
                                alt="";
                        }
                        code class="source-card__path" title=(path.to_string_lossy()) { (path.to_string_lossy()) }
                        button
                            type="button"
                            class="source-card__copy"
                            aria-label="Copy source path"
                            x-data="{ copied: false }"
                            x-on:click=(format!("navigator.clipboard.writeText({path:?}); copied = true; clearTimeout($data._copyTimer); $data._copyTimer = setTimeout(() => copied = false, 3000)"))
                            x-text="copied ? 'Copied!' : 'Copy'"
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
            .render(Path::new("assets"))
            .into_string();
        assert!(html.contains("class=\"declaration-hero__title\""));
        // The identifier used as the page title is styled as a code literal
        // while remaining inside the h1 (preserving its heading size via CSS).
        assert!(html.contains("<code class=\"heading-code-literal\">Sample</code>"));
        assert!(html.contains("source-card__path\" title=\"main.wdl\">main.wdl"));
        assert!(html.contains("src=\"assets/folder.dark.svg\""));
        assert!(html.contains("src=\"assets/folder.light.svg\""));
        assert!(!html.contains(">▱<"));
        assert!(html.contains("navigator.clipboard.writeText"));
        assert!(!html.contains("<code>Sample</code>"));

        let assets = crate::get_assets();
        let icon = std::str::from_utf8(assets.get("folder.dark.svg").expect("bundled folder icon"))
            .expect("folder icon is valid UTF-8");
        assert!(icon.contains("M3.12508 8.14668"));
    }
}
