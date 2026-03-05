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
mod meta;
mod parameter;
mod runnable;
mod r#struct;

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
pub use crate::error::DocError;
use crate::error::DocErrorKind;
use crate::error::DocResult;
use crate::error::NpmError;
use crate::error::ResultContextExt;

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
    page_title: &str,
    root: P,
    addl_html: &AdditionalHtml,
) -> Markup {
    let root = root.as_ref();
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
    page_title: &str,
    body: Markup,
    root: P,
    addl_html: &AdditionalHtml,
    init_light_mode: bool,
) -> Markup {
    html! {
        (DOCTYPE)
        html
            lang="en"
            x-data=(if init_light_mode { "{ theme: $persist('light') }" } else { "{ theme: $persist('dark') }" })
            x-bind:class="theme === 'light' ? 'light' : 'dark'"
            x-cloak
        {
            (header(page_title, root, addl_html))
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
        // Sanitize it with ammonia
        let safe_html = ammonia::clean(&unsafe_html);

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
            div class="main__badge" {
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
        let cur_dir = docs_dir.join(root_to_wdl.with_extension(""));
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

                    let r#enum =
                        r#enum::Enum::new(e, version, external_wdl, config.enable_doc_comments);

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
        let (document, _) = AstDocument::parse(source);

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
}
