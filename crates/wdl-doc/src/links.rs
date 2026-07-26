//! Page-local type links for generated struct and enum documentation.
//!
//! A [`PageLinkIndex`] maps uniquely named generated struct and enum pages to
//! their output paths so that WDL type text can be rendered with anchors that
//! point at the corresponding documentation page. Names that resolve to more
//! than one page are left unlinked so that ambiguous references never point at
//! the wrong declaration.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use maud::Markup;
use maud::html;
use pathdiff::diff_paths;

/// A single run of a tokenized WDL type string.
enum Token<'a> {
    /// A run of identifier characters (letters, digits, and underscores).
    Ident(&'a str),
    /// A run of any other characters (brackets, commas, whitespace, `?`, ...).
    Other(&'a str),
}

/// Whether a character can appear inside a WDL type identifier.
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenize WDL type text into maximal identifier and non-identifier runs.
///
/// The concatenation of every token's text reproduces the input exactly, so
/// rendering each token in order preserves the original type string.
fn tokenize(ty: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut chars = ty.char_indices().peekable();

    while let Some(&(start, first)) = chars.peek() {
        let ident = is_ident_char(first);
        let mut end = start;
        while let Some(&(idx, c)) = chars.peek() {
            if is_ident_char(c) == ident {
                end = idx + c.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let run = &ty[start..end];
        tokens.push(if ident {
            Token::Ident(run)
        } else {
            Token::Other(run)
        });
    }

    tokens
}

/// An index of generated struct and enum page names to their output paths.
///
/// The index only retains names that resolve to exactly one page; names with
/// multiple generated pages are dropped so ambiguous type references remain
/// plain text.
#[derive(Debug, Default)]
pub(crate) struct PageLinkIndex {
    /// Map from a uniquely named struct or enum to its page path relative to
    /// the docs root.
    targets: HashMap<String, PathBuf>,
}

impl PageLinkIndex {
    /// Build an index from an iterator of generated page names and their paths.
    ///
    /// Paths are expected to be relative to the docs root. A name that appears
    /// more than once is treated as ambiguous and excluded from the index.
    pub(crate) fn from_pages<I, S>(pages: I) -> Self
    where
        I: IntoIterator<Item = (S, PathBuf)>,
        S: Into<String>,
    {
        let mut seen: HashMap<String, Option<PathBuf>> = HashMap::new();
        for (name, path) in pages {
            seen.entry(name.into())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(path));
        }

        let targets = seen
            .into_iter()
            .filter_map(|(name, path)| path.map(|path| (name, path)))
            .collect();

        Self { targets }
    }

    /// Resolve the relative href for a type name from the given page directory.
    ///
    /// Returns `None` when the name is not a uniquely generated struct or enum.
    fn href(&self, name: &str, page_dir: &Path) -> Option<String> {
        let target = self.targets.get(name)?;
        let relative = diff_paths(target, page_dir).unwrap_or_else(|| target.clone());
        Some(relative.to_string_lossy().replace('\\', "/"))
    }

    /// Render WDL type text as HTML, linking uniquely named struct and enum
    /// identifiers to their pages relative to `page_dir`.
    ///
    /// The whole type is wrapped in a single `<code>` element. Identifiers that
    /// match a uniquely generated page become relative anchors; every other run
    /// of text is rendered verbatim.
    pub(crate) fn render_type(&self, ty: &str, page_dir: &Path) -> Markup {
        let tokens = tokenize(ty);
        html! {
            code {
                @for token in &tokens {
                    @match token {
                        Token::Ident(name) => {
                            @if let Some(href) = self.href(name, page_dir) {
                                a href=(href) { (name) }
                            } @else {
                                (name)
                            }
                        }
                        Token::Other(text) => {
                            (text)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::path::PathBuf;

    use super::PageLinkIndex;

    #[test]
    fn links_unique_struct_and_enum_types() {
        let links = PageLinkIndex::from_pages([
            ("Sample", PathBuf::from("main/Sample-struct.html")),
            (
                "ReferenceBuild",
                PathBuf::from("main/ReferenceBuild-enum.html"),
            ),
        ]);
        let html = links
            .render_type("Array[Sample]?", Path::new("main"))
            .into_string();
        assert!(html.contains("href=\"Sample-struct.html\""));
        assert!(html.contains("Array["));
    }

    #[test]
    fn links_enum_types_relative_to_page() {
        let links = PageLinkIndex::from_pages([(
            "ReferenceBuild",
            PathBuf::from("main/ReferenceBuild-enum.html"),
        )]);
        let html = links
            .render_type("ReferenceBuild", Path::new("main/nested"))
            .into_string();
        assert!(html.contains("href=\"../ReferenceBuild-enum.html\""));
    }

    #[test]
    fn unknown_identifiers_stay_plain() {
        let links =
            PageLinkIndex::from_pages([("Sample", PathBuf::from("main/Sample-struct.html"))]);
        let html = links
            .render_type("Map[String, Int]", Path::new("main"))
            .into_string();
        assert!(!html.contains("href"));
        assert!(html.contains("Map[String, Int]"));
    }

    #[test]
    fn ambiguous_names_stay_plain() {
        let links = PageLinkIndex::from_pages([
            ("Sample", PathBuf::from("main/Sample-struct.html")),
            ("Sample", PathBuf::from("other/Sample-struct.html")),
        ]);
        let html = links.render_type("Sample", Path::new("main")).into_string();
        assert!(!html.contains("href"));
        assert!(html.contains("<code>"));
        assert!(html.contains("Sample"));
    }
}
