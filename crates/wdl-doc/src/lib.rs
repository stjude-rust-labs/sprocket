//! Library for generating HTML documentation from WDL files.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

mod command_section;
pub mod config;
mod docs_tree;
mod document;
mod r#enum;
pub mod error;
mod links;
mod meta;
mod page;
mod parameter;
mod runnable;
mod r#struct;
mod workspace;

use std::io::Error as IoError;
use std::io::ErrorKind;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::rc::Rc;

pub use command_section::CommandSectionExt;
pub use docs_tree::DocsTree;
pub use docs_tree::DocsTreeBuilder;
use docs_tree::HTMLPage;
use docs_tree::PageType;
use document::Document;
use maud::DOCTYPE;
use maud::Markup;
use maud::PreEscaped;
use maud::Render;
use maud::html;
use path_clean::PathClean;
use pathdiff::diff_paths;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use runnable::task;
use runnable::workflow;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::DocumentItem;
use wdl_ast::version::V1;

use crate::config::AdditionalHtml;
pub use crate::config::Config;
use crate::config::Seo;
pub use crate::error::DocError;
use crate::error::DocErrorKind;
use crate::error::DocResult;
use crate::error::NpmError;
use crate::error::ResultContextExt;
use crate::workspace::WorkspaceMetadata;

/// Install the theme dependencies using npm.
pub fn install_theme(theme_dir: &Path) -> DocResult<()> {
    let theme_dir = absolute(theme_dir)?;
    if !theme_dir.exists() {
        return Err(IoError::new(
            ErrorKind::NotFound,
            format!(
                "theme directory does not exist at `{}`",
                theme_dir.display()
            ),
        )
        .into());
    }
    let output = std::process::Command::new(npm()?)
        .arg("install")
        .current_dir(&theme_dir)
        .output()
        .map_err(NpmError::Install)
        .map_err(Into::<DocError>::into)
        .with_context(|| {
            format!(
                "failed to run `npm install` in the theme directory: `{}`",
                theme_dir.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NpmError::Install(IoError::other(stderr)).into());
    }
    Ok(())
}

/// Build the web components for the theme.
pub fn build_web_components(theme_dir: &Path) -> DocResult<()> {
    let theme_dir = absolute(theme_dir)?;
    let output = std::process::Command::new(npm()?)
        .arg("run")
        .arg("build")
        .current_dir(&theme_dir)
        .output()
        .map_err(NpmError::Build)
        .map_err(Into::<DocError>::into)
        .with_context(|| {
            format!(
                "failed to execute `npm run build` in the theme directory: `{}`",
                theme_dir.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NpmError::Build(IoError::other(stderr)).into());
    }
    Ok(())
}

/// Get the path to the `npx` executable.
fn npx() -> std::io::Result<PathBuf> {
    which::which("npx").map_err(|_| IoError::other("npx not found (is Node.js installed?)"))
}

/// Get the path to the `npm` executable.
fn npm() -> std::io::Result<PathBuf> {
    which::which("npm").map_err(|_| IoError::other("npm not found (is Node.js installed?)"))
}

/// Build a stylesheet for the documentation, using Tailwind CSS.
pub fn build_stylesheet(theme_dir: &Path) -> DocResult<()> {
    let theme_dir = absolute(theme_dir)?;
    let output = std::process::Command::new(npx()?)
        .arg("@tailwindcss/cli")
        .arg("-i")
        .arg("src/main.css")
        .arg("-o")
        .arg("dist/style.css")
        .current_dir(&theme_dir)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NpmError::Tailwind(IoError::other(stderr)).into());
    }
    let css_path = theme_dir.join("dist/style.css");
    if !css_path.exists() {
        return Err(NpmError::Tailwind(IoError::new(
            ErrorKind::NotFound,
            format!("no output file found at `{}`", css_path.display()),
        ))
        .into());
    }

    Ok(())
}

/// Build the search index using [Pagefind](https://pagefind.app).
pub fn build_search_index(dist_dir: &Path) -> DocResult<()> {
    let dist_dir = absolute(dist_dir)?;
    let output = std::process::Command::new(npx()?)
        .arg("pagefind@1.5.0")
        .arg("--site")
        .arg(dist_dir)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NpmError::SearchIndex(IoError::other(stderr)).into());
    }

    Ok(())
}

/// HTML link to a CSS stylesheet at the given path.
struct Css<'a>(&'a str);

impl Render for Css<'_> {
    fn render(&self) -> Markup {
        html! {
            link rel="stylesheet" type="text/css" href=(self.0);
        }
    }
}

/// An HTML header with a `page_title` and all the link/script dependencies
/// expected by `wdl-doc`.
///
/// Requires a relative path to the root where `style.css` and `index.js` files
/// are expected.
pub(crate) fn header<P: AsRef<Path>>(
    page_name: &str,
    root: P,
    addl_html: &AdditionalHtml,
    init_light_mode: bool,
    seo: &Seo,
    canonical_url: Option<&str>,
) -> Markup {
    let root = root.as_ref();
    // The browser/tab title is "<page> | <site title>" when a site title is
    // configured, and just the page name otherwise.
    let page_title = match &seo.title {
        Some(site_title) => format!("{page_name} | {site_title}"),
        None => page_name.to_string(),
    };
    // Only emit Open Graph and Twitter Card tags when there is something worth
    // previewing; otherwise the head stays as lean as it was before SEO config.
    let has_social = seo.title.is_some()
        || seo.description.is_some()
        || seo.image_url.is_some()
        || canonical_url.is_some();
    let initial_theme = if init_light_mode { "light" } else { "dark" };
    let theme_bootstrap = format!(
        r#"const storedTheme = localStorage.getItem('_x_theme');
const initialTheme = storedTheme === null ? '{initial_theme}' : JSON.parse(storedTheme);
if (initialTheme === 'light') {{
    document.documentElement.classList.replace('dark', 'light');
}} else {{
    document.documentElement.classList.replace('light', 'dark');
}}
const storedRunWith = localStorage.getItem('run_with');
document.documentElement.dataset.runWith =
    storedRunWith === 'windows' ? 'windows' : 'unix';
const storedSidebar = sessionStorage.getItem('_x_sidebarState');
document.documentElement.dataset.sidebar =
    storedSidebar === null ? (window.innerWidth < 768 ? 'hidden' : 'normal') : JSON.parse(storedSidebar);"#
    );
    let search_import = format!(
        r#"const pagefindPath = new URL('{}', import.meta.url).href;
window.pagefind = import(pagefindPath)"#,
        root.join("pagefind").join("pagefind.js").to_string_lossy()
    );
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1.0";
            title { (page_title) }
            @if let Some(description) = &seo.description {
                meta name="description" content=(description);
            }
            @if let Some(author) = &seo.author {
                meta name="author" content=(author);
            }
            @if !seo.keywords.is_empty() {
                meta name="keywords" content=(seo.keywords.join(", "));
            }
            @if let Some(robots) = &seo.robots {
                meta name="robots" content=(robots);
            }
            @if let Some(theme_color) = &seo.theme_color {
                meta name="theme-color" content=(theme_color);
            }
            @if let Some(canonical) = canonical_url {
                link rel="canonical" href=(canonical);
            }
            @if has_social {
                meta property="og:type" content="website";
                meta property="og:title" content=(page_title);
                @if let Some(site_title) = &seo.title {
                    meta property="og:site_name" content=(site_title);
                }
                @if let Some(description) = &seo.description {
                    meta property="og:description" content=(description);
                }
                @if let Some(canonical) = canonical_url {
                    meta property="og:url" content=(canonical);
                }
                @if let Some(image) = &seo.image_url {
                    meta property="og:image" content=(image.as_str());
                }
                meta property="og:locale" content=(seo.locale.as_deref().unwrap_or("en_US"));
                meta name="twitter:card" content=(if seo.image_url.is_some() { "summary_large_image" } else { "summary" });
                meta name="twitter:title" content=(page_title);
                @if let Some(description) = &seo.description {
                    meta name="twitter:description" content=(description);
                }
                @if let Some(image) = &seo.image_url {
                    meta name="twitter:image" content=(image.as_str());
                }
                @if let Some(handle) = &seo.twitter_handle {
                    meta name="twitter:site" content=(handle);
                    meta name="twitter:creator" content=(handle);
                }
            }
            script { (PreEscaped(theme_bootstrap)) }
            link rel="preconnect" href="https://fonts.googleapis.com";
            link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
            link href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,100..1000;1,9..40,100..1000&display=swap" rel="stylesheet";
            script type="module" {
                (PreEscaped(search_import))
            }

            script defer src=(root.join("index.js").to_string_lossy()) {}
            (Css(&root.join("style.css").to_string_lossy()))
            @if let Some(s) = addl_html.head() {
                (PreEscaped(s))
            }
        }
    }
}

/// Returns a full HTML page, including the `DOCTYPE`, `html`, `head`, and
/// `body` tags,
pub(crate) fn full_page<P: AsRef<Path>>(
    page_name: &str,
    body: Markup,
    root: P,
    addl_html: &AdditionalHtml,
    init_light_mode: bool,
    seo: &Seo,
    canonical_url: Option<&str>,
) -> Markup {
    html! {
        (DOCTYPE)
        html
            lang="en"
            class=(if init_light_mode { "light" } else { "dark" })
            data-run-with="unix"
            x-data=(if init_light_mode { "{ theme: $persist('light') }" } else { "{ theme: $persist('dark') }" })
            x-bind:class="theme === 'light' ? 'light' : 'dark'"
            x-cloak
        {
            (header(page_name, root, addl_html, init_light_mode, seo, canonical_url))
            body class="body--base" {
                @if let Some(s) = addl_html.body_open() {
                    (PreEscaped(s))
                }
                (body)
                @if let Some(s) = addl_html.body_close() {
                    (PreEscaped(s))
                }
            }
        }
    }
}

/// Renders a block of Markdown using `pulldown-cmark`.
pub(crate) struct Markdown<T>(T);

impl<T: AsRef<str>> Render for Markdown<T> {
    fn render(&self) -> Markup {
        // Generate raw HTML
        let mut unsafe_html = String::new();
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_GFM);
        options.insert(Options::ENABLE_DEFINITION_LIST);
        let parser = Parser::new_ext(self.0.as_ref(), options);
        pulldown_cmark::html::push_html(&mut unsafe_html, parser);
        // Sanitize it with ammonia, preserving the `class` attribute on fenced
        // code blocks so the theme's manual highlighter can detect the
        // `language-*` hint that `pulldown-cmark` emits.
        let safe_html = ammonia::Builder::default()
            .add_tag_attributes("code", ["class"])
            .clean(&unsafe_html)
            .to_string();

        // Remove the outer `<p>` tag that `pulldown_cmark` wraps single lines in
        let safe_html = if safe_html.starts_with("<p>") && safe_html.ends_with("</p>\n") {
            let trimmed = &safe_html[3..safe_html.len() - 5];
            if trimmed.contains("<p>") {
                // If the trimmed string contains another `<p>` tag, it means
                // that the original string was more complicated than a single-line paragraph,
                // so we should keep the outer `<p>` tag.
                safe_html
            } else {
                trimmed.to_string()
            }
        } else {
            safe_html
        };
        PreEscaped(safe_html)
    }
}

/// A version badge for a WDL document. This is used to display the WDL
/// version at the top of each documentation page.
#[derive(Debug, Clone)]
pub(crate) struct VersionBadge {
    /// The WDL version of the document.
    version: SupportedVersion,
}

impl VersionBadge {
    /// Create a new version badge.
    fn new(version: SupportedVersion) -> Self {
        Self { version }
    }

    /// Render the version badge as HTML.
    fn render(&self) -> Markup {
        let latest = match &self.version {
            SupportedVersion::V1(v) => matches!(v, V1::Two),
            _ => unreachable!("only V1 is supported"),
        };
        let text = self.version.to_string();
        html! {
            div class="main__badge main__badge--wdl" {
                span class="main__badge-wdl-icon" aria-hidden="true" {
                    (PreEscaped(include_str!("../theme/assets/wdl.svg")))
                }
                span class="main__badge-text" {
                    "WDL Version"
                }
                div class="main__badge-inner" {
                    span class="main__badge-inner-text" {
                        (text)
                    }
                }
                @if latest {
                    div class="main__badge-inner main__badge-inner-latest" {
                        span class="main__badge-inner-text" {
                            "Latest"
                        }
                    }
                }
            }
        }
    }
}

/// Analyze a workspace directory, ensure it is error-free, and return the
/// results.
///
/// `workspace_root` should be an absolute path.
async fn analyze_workspace(
    workspace_root: impl AsRef<Path>,
    config: AnalysisConfig,
) -> DocResult<Vec<AnalysisResult>> {
    let workspace = workspace_root.as_ref();
    let analyzer = Analyzer::new(config, async |_, _, _, _| ());
    analyzer
        .add_directory(workspace)
        .await
        .map_err(|e| DocError::new(DocErrorKind::Analyzer(e)))
        .with_context(|| "failed to add directory to analyzer".to_string())?;
    let results = analyzer
        .analyze(())
        .await
        .map_err(|e| DocError::new(DocErrorKind::Analyzer(e)))
        .with_context(|| "failed to analyze workspace".to_string())?;

    if results.is_empty() {
        return Err(DocError::new(DocErrorKind::NoDocuments));
    }
    let mut workspace_in_results = false;
    let mut failed = Vec::new();
    for r in &results {
        if r.document()
            .diagnostics()
            .any(|d| d.severity() == wdl_ast::Severity::Error)
        {
            failed.push(r.clone());
        }

        if r.document()
            .uri()
            .to_file_path()
            .is_ok_and(|f| f.starts_with(workspace))
        {
            workspace_in_results = true;
        }
    }

    if !workspace_in_results {
        return Err(DocError::new(DocErrorKind::WorkspaceNotFound(
            workspace.to_path_buf(),
        )));
    }

    if !failed.is_empty() {
        return Err(DocError::new(DocErrorKind::AnalysisFailed(failed)));
    }

    Ok(results)
}

/// Generate HTML documentation for a workspace.
///
/// This function will generate HTML documentation for all WDL files in the
/// workspace directory. This function will overwrite any existing files which
/// conflict with the generated files, but will not delete any files that
/// are already present.
pub async fn document_workspace(config: Config) -> DocResult<()> {
    let workspace_abs_path = absolute(&config.workspace)?.clean();
    let index_page = config.index_page.and_then(|p| absolute(p).ok());

    if !workspace_abs_path.is_dir() {
        return Err(
            DocError::new(DocErrorKind::Io(IoError::from(ErrorKind::NotADirectory))).with_context(
                format!(
                    "workspace path `{}` is not a directory",
                    workspace_abs_path.display()
                ),
            ),
        );
    }

    let workspace_metadata = WorkspaceMetadata::load(&workspace_abs_path)?;

    let results = analyze_workspace(&workspace_abs_path, config.analysis_config).await?;

    if config.check {
        return Ok(());
    }

    let docs_dir = absolute(&config.output_dir)?.clean();
    if !docs_dir.exists() {
        std::fs::create_dir_all(&docs_dir)
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to create output directory: `{}`",
                    docs_dir.display()
                )
            })?;
    }

    let mut docs_tree = DocsTreeBuilder::new(docs_dir.clone())
        .maybe_index_page(index_page)
        .init_light_mode(config.init_light_mode)
        .maybe_custom_theme(config.custom_theme)?
        .maybe_logo(config.custom_logo)
        .maybe_alt_logo(config.alt_logo)
        .additional_html(config.additional_html)
        .external_urls(config.external_urls)
        .seo(config.seo)
        .maybe_workspace_metadata(workspace_metadata.clone())
        .build()?;

    for result in results {
        let uri = result.document().uri();
        let (root_to_wdl, external_wdl) = match uri.to_file_path() {
            Ok(path) => match path.strip_prefix(&workspace_abs_path) {
                Ok(path) => {
                    // The path is relative to the workspace
                    (path.to_path_buf(), false)
                }
                Err(_) => {
                    // URI was successfully converted to a file path, but it is not in the
                    // workspace. This must be an imported WDL file and the
                    // documentation will be generated in the `external/` directory.
                    let external = PathBuf::from("external").join(
                        path.components()
                            .skip_while(|c| !matches!(c, Component::Normal(_)))
                            .collect::<PathBuf>(),
                    );
                    (external, true)
                }
            },
            Err(_) => (
                // The URI could not be converted to a file path, so it must be a remote WDL file.
                // In this case, we will generate documentation in the `external/` directory.
                PathBuf::from("external").join(
                    uri.path()
                        .strip_prefix("/")
                        .expect("URI path should start with /"),
                ),
                true,
            ),
        };
        let cur_dir = docs_dir.join(
            workspace_metadata
                .as_ref()
                .map(|metadata| metadata.documentation_path(&root_to_wdl))
                .unwrap_or_else(|| root_to_wdl.with_extension("")),
        );
        if !cur_dir.exists() {
            std::fs::create_dir_all(&cur_dir)
                .map_err(Into::<DocError>::into)
                .with_context(|| format!("failed to create directory: `{}`", cur_dir.display()))?;
        }
        let version = result
            .document()
            .version()
            .expect("document should have a supported version");
        let ast = result.document().root();
        let version_statement = ast
            .version_statement()
            .expect("document should have a version statement");
        let ast = ast
            .ast_with_version_fallback(result.document().config().fallback_version())
            .unwrap_v1();

        let mut local_pages = Vec::new();

        for item in ast.items() {
            match item {
                DocumentItem::Struct(s) => {
                    let name = s.name().text().to_owned();
                    let path = cur_dir.join(format!("{name}-struct.html"));

                    let r#struct = r#struct::Struct::new(
                        s.clone(),
                        version,
                        external_wdl,
                        if external_wdl {
                            None
                        } else {
                            Some(root_to_wdl.clone())
                        },
                        config.enable_doc_comments,
                    );

                    let page = Rc::new(HTMLPage::new(name.clone(), PageType::Struct(r#struct)));
                    docs_tree.add_page(path.clone(), page.clone());
                    local_pages
                        .push((diff_paths(path, &cur_dir).expect("should diff paths"), page));
                }
                DocumentItem::Task(t) => {
                    let name = t.name().text().to_owned();
                    let path = cur_dir.join(format!("{name}-task.html"));

                    let task = task::Task::new(
                        name.clone(),
                        version,
                        t,
                        if external_wdl {
                            None
                        } else {
                            Some(root_to_wdl.clone())
                        },
                        config.enable_doc_comments,
                    );

                    let page = Rc::new(HTMLPage::new(name, PageType::Task(task)));
                    docs_tree.add_page(path.clone(), page.clone());
                    local_pages
                        .push((diff_paths(path, &cur_dir).expect("should diff paths"), page));
                }
                DocumentItem::Workflow(w) => {
                    let name = w.name().text().to_owned();
                    let path = cur_dir.join(format!("{name}-workflow.html"));

                    let workflow = workflow::Workflow::new(
                        name.clone(),
                        version,
                        w,
                        if external_wdl {
                            None
                        } else {
                            Some(root_to_wdl.clone())
                        },
                        config.enable_doc_comments,
                    );

                    let page = Rc::new(HTMLPage::new(
                        workflow.name_override().unwrap_or(name),
                        PageType::Workflow(workflow),
                    ));
                    docs_tree.add_page(path.clone(), page.clone());
                    local_pages
                        .push((diff_paths(path, &cur_dir).expect("should diff paths"), page));
                }
                DocumentItem::Import(_) => {}
                DocumentItem::Enum(e) => {
                    let name = e.name().text().to_owned();
                    let path = cur_dir.join(format!("{name}-enum.html"));

                    let r#enum = r#enum::Enum::new(
                        e,
                        version,
                        external_wdl,
                        if external_wdl {
                            None
                        } else {
                            Some(root_to_wdl.clone())
                        },
                        config.enable_doc_comments,
                    );

                    let page = Rc::new(HTMLPage::new(name.clone(), PageType::Enum(r#enum)));
                    docs_tree.add_page(path.clone(), page.clone());
                    local_pages
                        .push((diff_paths(path, &cur_dir).expect("should diff paths"), page));
                }
            }
        }
        let document_name = root_to_wdl
            .file_stem()
            .ok_or_else(|| {
                DocError::new(DocErrorKind::Io(IoError::new(
                    ErrorKind::InvalidFilename,
                    root_to_wdl.display().to_string(),
                )))
                .with_context("failed to get file stem for WDL file")
            })?
            .to_string_lossy();
        let document = Document::new(
            document_name.to_string(),
            version,
            version_statement,
            local_pages,
        );

        let index_path = cur_dir.join("index.html");

        docs_tree.add_page(
            index_path,
            Rc::new(HTMLPage::new(
                document_name.to_string(),
                PageType::Index(document),
            )),
        );
    }

    docs_tree.render_all().with_context(|| {
        format!(
            "failed to write documentation to output directory: `{}`",
            docs_dir.display()
        )
    })?;

    build_search_index(&docs_dir)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document as AstDocument;

    use super::*;
    use crate::meta::DefinitionMeta;

    #[test]
    fn test_simple_markdown_render() {
        let source = r#"
        version 1.0
        workflow test {
            meta {
                description: "A simple description should not render with p tags"
            }
        }
        "#;
        let (document, _) = AstDocument::parse(source, None);

        let doc_item = document.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();
        let workflow = workflow::Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Zero),
            ast_workflow,
            None,
            false,
        );

        let description = workflow.render_description(false);
        assert_eq!(
            description.into_string(),
            "A simple description should not render with p tags"
        );
    }

    #[test]
    fn fenced_code_retains_language_class() {
        // The theme's manual highlighter keys off the `language-*` class that
        // `pulldown-cmark` emits, so sanitization must preserve it.
        let rendered = Markdown("```json\n{\"a\": 1}\n```").render().into_string();
        assert!(
            rendered.contains("class=\"language-json\""),
            "expected the fenced code block to keep its language class, got: {rendered}"
        );
    }

    #[test]
    fn persisted_theme_is_applied_before_the_stylesheet_loads() {
        let page = full_page(
            "Theme test",
            html! {},
            Path::new("."),
            &AdditionalHtml::default(),
            false,
            &Seo::default(),
            None,
        )
        .into_string();
        let theme_position = page
            .find("localStorage.getItem('_x_theme')")
            .expect("persisted theme bootstrap");
        let stylesheet_position = page.find("style.css").expect("theme stylesheet");

        assert!(page.contains("<html lang=\"en\" class=\"dark\""));
        assert!(theme_position < stylesheet_position);
        assert!(page.contains("classList.replace('dark', 'light')"));
        assert!(page.contains("document.documentElement.dataset.runWith"));
    }

    #[test]
    fn seo_metadata_populates_the_head() {
        let seo = Seo {
            title: Some("Sprocket Docs".to_string()),
            description: Some("WDL documentation".to_string()),
            author: Some("St. Jude".to_string()),
            keywords: vec!["wdl".to_string(), "workflows".to_string()],
            base_url: None,
            image_url: Some(ammonia::Url::parse("https://example.com/card.png").unwrap()),
            locale: Some("en_GB".to_string()),
            twitter_handle: Some("@sprocket".to_string()),
            robots: Some("index, follow".to_string()),
            theme_color: Some("#0a0c12".to_string()),
        };
        let page = full_page(
            "analyze_sample",
            html! {},
            Path::new("."),
            &AdditionalHtml::default(),
            false,
            &seo,
            Some("https://example.com/main/analyze_sample-workflow.html"),
        )
        .into_string();

        // The site title composes with the page name.
        assert!(page.contains("<title>analyze_sample | Sprocket Docs</title>"));
        assert!(page.contains(r#"<meta name="description" content="WDL documentation">"#));
        assert!(page.contains(r#"<meta name="author" content="St. Jude">"#));
        assert!(page.contains(r#"<meta name="keywords" content="wdl, workflows">"#));
        assert!(page.contains(r#"<meta name="robots" content="index, follow">"#));
        assert!(page.contains(r##"<meta name="theme-color" content="#0a0c12">"##));
        assert!(page.contains(
            r#"<link rel="canonical" href="https://example.com/main/analyze_sample-workflow.html">"#
        ));
        assert!(page.contains(r#"<meta property="og:site_name" content="Sprocket Docs">"#));
        assert!(
            page.contains(r#"<meta property="og:image" content="https://example.com/card.png">"#)
        );
        assert!(page.contains(r#"<meta property="og:locale" content="en_GB">"#));
        assert!(page.contains(r#"<meta name="twitter:card" content="summary_large_image">"#));
        assert!(page.contains(r#"<meta name="twitter:site" content="@sprocket">"#));
    }

    #[test]
    fn head_stays_lean_without_seo_config() {
        let page = full_page(
            "Home",
            html! {},
            Path::new("."),
            &AdditionalHtml::default(),
            false,
            &Seo::default(),
            None,
        )
        .into_string();

        assert!(page.contains("<title>Home</title>"));
        assert!(!page.contains("og:"));
        assert!(!page.contains("twitter:"));
        assert!(!page.contains(r#"name="description""#));
    }

    #[test]
    fn version_badge_uses_bundled_openwdl_icon() {
        let badge = VersionBadge::new(SupportedVersion::V1(V1::Two))
            .render()
            .into_string();
        let assets = get_assets();

        assert!(badge.contains("main__badge--wdl"));
        assert!(badge.contains("main__badge-wdl-icon"));
        assert!(badge.contains("<svg width=\"512\" height=\"512\""));
        assert!(assets.contains_key("wdl.svg"));
        assert!(assets.contains_key("wdl.svg.license.txt"));
    }
}
