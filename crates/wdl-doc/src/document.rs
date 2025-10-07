//! Create HTML documentation for WDL documents.
//!
//! This module defines the [`Document`] struct, which represents an entire WDL
//! document's HTML representation (i.e., an index page that links to other
//! pages).
//!
//! See [`crate::task::Task`], [`crate::workflow::Workflow`], and
//! [`crate::struct::Struct`] for how to render individual tasks, workflows, and
//! structs.

use std::path::PathBuf;
use std::rc::Rc;

use maud::Markup;
use maud::PreEscaped;
use maud::Render;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxTokenExt;
use wdl_ast::VersionStatement;

use crate::HTMLPage;
use crate::Markdown;
use crate::VersionBadge;
use crate::docs_tree::Header;
use crate::docs_tree::PageSections;
use crate::docs_tree::PageType;
use crate::runnable::Runnable;

/// Parse the preamble comments of a document using the version statement.
pub fn parse_preamble_comments(version: &VersionStatement) -> String {
    let comments = version
        .keyword()
        .inner()
        .preceding_trivia()
        .map(|t| match t.kind() {
            wdl_ast::SyntaxKind::Comment => match t.to_string().strip_prefix("## ") {
                Some(comment) => comment.to_string(),
                None => "".to_string(),
            },
            wdl_ast::SyntaxKind::Whitespace => "".to_string(),
            _ => {
                panic!("Unexpected token kind: {:?}", t.kind())
            }
        })
        .collect::<Vec<_>>();
    comments.join("\n")
}

/// A WDL document. This is an index page that links to other HTML pages.
#[derive(Debug)]
pub(crate) struct Document {
    /// The name of the document.
    name: String,
    /// The [`VersionBadge`] which displays the WDL version of the document.
    version: VersionBadge,
    /// The AST node for the version statement.
    ///
    /// This is used to fetch to any preamble comments.
    version_statement: VersionStatement,
    /// The pages that this document should link to.
    local_pages: Vec<(PathBuf, Rc<HTMLPage>)>,
}

impl Document {
    /// Create a new document.
    pub(crate) fn new(
        name: String,
        version: SupportedVersion,
        version_statement: VersionStatement,
        local_pages: Vec<(PathBuf, Rc<HTMLPage>)>,
    ) -> Self {
        Self {
            name,
            version: VersionBadge::new(version),
            version_statement,
            local_pages,
        }
    }

    /// Get the name of the document.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Render the version of the document as a badge.
    pub fn render_version(&self) -> Markup {
        self.version.render()
    }

    /// Get the preamble comments of the document as HTML if there are any.
    pub fn render_preamble(&self) -> Option<Markup> {
        let preamble = parse_preamble_comments(&self.version_statement);
        if preamble.is_empty() {
            return None;
        }
        Some(html! {
            div class="markdown-body" {
                (Markdown(&preamble).render())
            }
        })
    }

    /// Render the document as HTML.
    pub fn render(&self) -> (Markup, PageSections) {
        let rows = self.local_pages.iter().map(|page| {
            html! {
                div class="main__grid-row" x-data="{ description_expanded: false }" {
                    @match page.1.page_type() {
                        PageType::Struct(_) => {
                            div class="main__grid-cell" {
                                a class="text-brand-pink-400 hover:text-pink-200" href=(page.0.to_string_lossy()) {
                                    (page.1.name())
                                }
                            }
                            div class="main__grid-cell" { code { "struct" } }
                            div class="main__grid-cell" { "N/A" }
                        }
                        PageType::Task(t) => {
                            div class="main__grid-cell" {
                                a class="text-brand-violet-400 hover:text-violet-200" href=(page.0.to_string_lossy()) {
                                    (page.1.name())
                                }
                            }
                            div class="main__grid-cell" { code { "task" } }
                            div class="main__grid-cell" {
                                (t.render_description(true))
                            }
                        }
                        PageType::Workflow(w) => {
                            div class="main__grid-cell" {
                                a class="text-brand-emerald-400 hover:text-brand-emerald-200" href=(page.0.to_string_lossy()) {
                                    (page.1.name())
                                }
                            }
                            div class="main__grid-cell" { code { "workflow" } }
                            div class="main__grid-cell" {
                                (w.render_description(true))
                            }
                        }
                        // Index pages should not link to other index pages.
                        PageType::Index(_) => {
                            // This should be unreachable
                            div class="main__grid-cell" { "ERROR" }
                            div class="main__grid-cell" { "ERROR" }
                            div class="main__grid-cell" { "ERROR" }
                        }
                    }
                    div x-show="description_expanded" class="main__grid-full-width-cell" {
                        @match page.1.page_type() {
                            PageType::Struct(_) => "ERROR"
                            PageType::Task(t) => {
                                (t.render_description(false))
                            }
                            PageType::Workflow(w) => {
                                (w.render_description(false))
                            }
                            PageType::Index(_) => "ERROR"
                        }
                    }
                }
            }
        }.into_string()).collect::<Vec<_>>().join(&html! { div class="main__grid-row-separator" {} }.into_string());

        let markup = html! {
            div class="main__container" {
                h1 id="title" class="main__title" { (self.name()) }
                div class="main__badge-container" {
                    (self.render_version())
                }
                @if let Some(preamble) = self.render_preamble() {
                    div id="preamble" class="main__section" {
                        (preamble)
                    }
                }
                div class="main__section" {
                    h2 id="toc" class="main__section-header" { "Table of Contents" }
                    div class="main__grid-container" {
                        div class="main__grid-toc-container" {
                            div class="main__grid-header-cell" { "Page" }
                            div class="main__grid-header-cell" { "Type" }
                            div class="main__grid-header-cell" { "Description" }
                            div class="main__grid-header-separator" {}
                            (PreEscaped(rows))
                        }
                    }
                }
            }
        };

        let mut headers = PageSections::default();
        headers.push(Header::Header(
            "Preamble".to_string(),
            "preamble".to_string(),
        ));
        headers.push(Header::Header(
            "Table of Contents".to_string(),
            "toc".to_string(),
        ));

        (markup, headers)
    }
}
