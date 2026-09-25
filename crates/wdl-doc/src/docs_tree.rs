//! Implementations for a [`DocsTree`] which represents the docs directory.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::rc::Rc;

use ammonia::Url;
use maud::Markup;
use maud::html;
use path_clean::PathClean;
use pathdiff::diff_paths;
use serde::Serialize;

use crate::AdditionalHtml;
use crate::DocError;
use crate::Markdown;
use crate::Render;
use crate::config::ExternalUrls;
use crate::config::Seo;
use crate::document::Document;
use crate::r#enum::Enum;
use crate::error::DocResult;
use crate::error::ResultContextExt;
use crate::full_page;
use crate::get_assets;
use crate::links::PageLinkIndex;
use crate::r#struct::Struct;
use crate::task::Task;
use crate::workflow::Workflow;
use crate::workspace::ModuleMetadata;
use crate::workspace::WorkspaceMetadata;

/// Filename for the dark theme logo SVG expected to be in the "assets"
/// directory.
const LOGO_FILE_NAME: &str = "logo.svg";
/// Filename for the light theme logo SVG expected to be in the "assets"
/// directory.
const LIGHT_LOGO_FILE_NAME: &str = "logo.light.svg";

/// The type of a page.
#[derive(Debug)]
pub(crate) enum PageType {
    /// An index page.
    Index(Document),
    /// A struct page.
    Struct(Struct),
    /// An enum page.
    Enum(Enum),
    /// A task page.
    Task(Task),
    /// A workflow page.
    Workflow(Workflow),
}

/// An HTML page in the docs directory.
#[derive(Debug)]
pub(crate) struct HTMLPage {
    /// The display name of the page.
    name: String,
    /// The type of the page.
    page_type: PageType,
}

impl HTMLPage {
    /// Create a new HTML page.
    pub(crate) fn new(name: String, page_type: PageType) -> Self {
        Self { name, page_type }
    }

    /// Get the name of the page.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Get the type of the page.
    pub(crate) fn page_type(&self) -> &PageType {
        &self.page_type
    }
}

/// A page header or page sub header.
///
/// This is used to represent the headers in the right sidebar of the
/// documentation pages. Each header has a name (first `String`) and an ID
/// (second `String`), which is used to link to the header in the page.
#[derive(Debug)]
pub(crate) enum Header {
    /// A header in the page.
    Header(String, String),
    /// A sub header in the page.
    SubHeader(String, String),
}

/// A collection of page headers representing the sections of a page.
///
/// This is used to render the right sidebar of documentation pages.
/// Each section added to this collection will be rendered in the
/// order it was added.
#[derive(Debug, Default)]
pub(crate) struct PageSections {
    /// The headers in the page.
    pub headers: Vec<Header>,
}

impl PageSections {
    /// Push a header to the page sections.
    pub fn push(&mut self, header: Header) {
        self.headers.push(header);
    }

    /// Extend the page headers with another collection of headers.
    pub fn extend(&mut self, headers: Self) {
        self.headers.extend(headers.headers);
    }

    /// Render the page sections as HTML for the right sidebar.
    pub fn render(&self) -> Markup {
        html!(
            @for header in &self.headers {
                @match header {
                    Header::Header(name, id) => {
                        a href=(format!("#{}", id)) class="right-sidebar__section-header" { (name) }
                    }
                    Header::SubHeader(name, id) => {
                        div class="right-sidebar__section-items" {
                            a href=(format!("#{}", id)) class="right-sidebar__section-item" { (name) }
                        }
                    }
                }
            }
        )
    }
}

/// A node in the docs directory tree.
#[derive(Debug)]
struct Node {
    /// The name of the node.
    name: String,
    /// The path from the root to the node.
    path: PathBuf,
    /// The page associated with the node.
    page: Option<Rc<HTMLPage>>,
    /// The children of the node.
    children: BTreeMap<String, Node>,
}

impl Node {
    /// Create a new node.
    pub fn new<P: Into<PathBuf>>(name: String, path: P) -> Self {
        Self {
            name,
            path: path.into(),
            page: None,
            children: BTreeMap::new(),
        }
    }

    /// Get the name of the node.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the path from the root to the node.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Determine if the node is part of a path.
    ///
    /// Path should be relative to the root or false positives may occur.
    pub fn part_of_path<P: AsRef<Path>>(&self, path: P) -> bool {
        let other_path = path.as_ref();
        let self_path = if self.path().ends_with("index.html") {
            self.path().parent().expect("index should have parent")
        } else {
            self.path()
        };
        self_path
            .components()
            .all(|c| other_path.components().any(|p| p == c))
    }

    /// Get the page associated with the node.
    pub fn page(&self) -> Option<&Rc<HTMLPage>> {
        self.page.as_ref()
    }

    /// Get the children of the node.
    pub fn children(&self) -> &BTreeMap<String, Node> {
        &self.children
    }

    /// Gather the node and its children in a Depth First Traversal order.
    ///
    /// Traversal order among children is alphabetical by node name, with the
    /// exception of any "external" node, which is always last.
    pub fn depth_first_traversal(&self) -> Vec<&Node> {
        fn recurse_depth_first<'a>(node: &'a Node, nodes: &mut Vec<&'a Node>) {
            nodes.push(node);

            for child in node.children().values() {
                recurse_depth_first(child, nodes);
            }
        }

        let mut nodes = Vec::new();
        nodes.push(self);
        for child in self.children().values().filter(|c| c.name() != "external") {
            recurse_depth_first(child, &mut nodes);
        }
        if let Some(external) = self.children().get("external") {
            recurse_depth_first(external, &mut nodes);
        }

        nodes
    }
}

/// A builder for a [`DocsTree`] which represents the docs directory.
#[derive(Debug)]
pub struct DocsTreeBuilder {
    /// The root directory for the docs.
    root: PathBuf,
    /// The path to a Markdown file to embed in the `<root>/index.html` page.
    index_page: Option<PathBuf>,
    /// An optional path to a custom theme to use for the docs.
    custom_theme: Option<PathBuf>,
    /// The path to a custom dark theme logo to embed at the top of the left
    /// sidebar.
    ///
    /// If this is `Some(_)` and no `alt_logo` is supplied, this will be used
    /// for both dark and light themes.
    logo: Option<PathBuf>,
    /// External URLs related to the project, rendered in the right rail.
    external_urls: ExternalUrls,
    /// Site-level SEO metadata embedded into each page's `<head>`.
    seo: Seo,
    /// The path to an alternate light theme custom logo to embed at the top of
    /// the left sidebar.
    alt_logo: Option<PathBuf>,
    /// Optional extra HTML to embed in each page.
    additional_html: AdditionalHtml,
    /// Start in light mode instead of the default dark mode.
    init_light_mode: bool,
    /// Optional workspace module metadata, present when the documented
    /// workspace is a WDL module.
    workspace_metadata: Option<WorkspaceMetadata>,
}

impl DocsTreeBuilder {
    /// Create a new docs tree builder.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = absolute(root.as_ref())
            .expect("should get absolute path")
            .clean();
        Self {
            root,
            index_page: None,
            custom_theme: None,
            logo: None,
            external_urls: ExternalUrls::default(),
            seo: Seo::default(),
            alt_logo: None,
            additional_html: AdditionalHtml::default(),
            init_light_mode: false,
            workspace_metadata: None,
        }
    }

    /// Set the index page for the docs with an option.
    pub fn maybe_index_page(mut self, index_page: Option<impl Into<PathBuf>>) -> Self {
        self.index_page = index_page.map(|hp| hp.into());
        self
    }

    /// Set the index page for the docs.
    pub fn index_page(self, index_page: impl Into<PathBuf>) -> Self {
        self.maybe_index_page(Some(index_page))
    }

    /// Set the custom theme for the docs with an option.
    pub fn maybe_custom_theme(mut self, theme: Option<impl AsRef<Path>>) -> DocResult<Self> {
        self.custom_theme = if let Some(t) = theme {
            Some(
                absolute(t.as_ref())
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to resolve absolute path for custom theme: `{}`",
                            t.as_ref().display()
                        )
                    })?
                    .clean(),
            )
        } else {
            None
        };
        Ok(self)
    }

    /// Set the custom theme for the docs.
    pub fn custom_theme(self, theme: impl AsRef<Path>) -> DocResult<Self> {
        self.maybe_custom_theme(Some(theme))
    }

    /// Set the custom logo for the left sidebar with an option.
    pub fn maybe_logo(mut self, logo: Option<impl Into<PathBuf>>) -> Self {
        self.logo = logo.map(|l| l.into());
        self
    }

    /// Set the custom logo for the left sidebar.
    pub fn logo(self, logo: impl Into<PathBuf>) -> Self {
        self.maybe_logo(Some(logo))
    }

    /// Set the external URLs for the right rail.
    pub fn external_urls(mut self, external_urls: ExternalUrls) -> Self {
        self.external_urls = external_urls;
        self
    }

    /// Set the site-level SEO metadata embedded into each page's `<head>`.
    pub fn seo(mut self, seo: Seo) -> Self {
        self.seo = seo;
        self
    }

    /// Set the alt (i.e. light mode) custom logo for the left sidebar with an
    /// option.
    pub fn maybe_alt_logo(mut self, logo: Option<impl Into<PathBuf>>) -> Self {
        self.alt_logo = logo.map(|l| l.into());
        self
    }

    /// Set the alt (i.e. light mode) custom logo for the left sidebar.
    pub fn alt_logo(self, logo: impl Into<PathBuf>) -> Self {
        self.maybe_alt_logo(Some(logo))
    }

    /// Set the additional HTML for each page.
    pub fn additional_html(mut self, html: AdditionalHtml) -> Self {
        self.additional_html = html;
        self
    }

    /// Set whether light mode should be the initial view instead of dark mode.
    pub fn init_light_mode(mut self, init_light_mode: bool) -> Self {
        self.init_light_mode = init_light_mode;
        self
    }

    /// Set the workspace module metadata with an option.
    ///
    /// When present, the documented workspace is a WDL module, and the
    /// resulting [`DocsTree`] uses the module's manifest name for its root
    /// node, labels module directories with their manifest name and version,
    /// and renders a generated module overview on the root index page in
    /// place of the plain "Home" header.
    pub(crate) fn maybe_workspace_metadata(mut self, metadata: Option<WorkspaceMetadata>) -> Self {
        self.workspace_metadata = metadata;
        self
    }

    /// Build the docs tree.
    pub fn build(self) -> DocResult<DocsTree> {
        self.write_assets()?;

        let root_name = self
            .workspace_metadata
            .as_ref()
            .and_then(|metadata| metadata.root().map(|root| root.name().to_string()))
            .unwrap_or_else(|| {
                self.root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or("docs".to_string())
            });
        let node = Node::new(root_name, PathBuf::from(""));
        Ok(DocsTree {
            root: node,
            path: self.root,
            index_page: self.index_page,
            external_urls: self.external_urls,
            seo: self.seo,
            additional_html: self.additional_html,
            init_light_mode: self.init_light_mode,
            workspace_metadata: self.workspace_metadata,
        })
    }

    /// Write assets to the root docs directory.
    ///
    /// This will create an `assets` directory in the root and write all
    /// necessary assets to it. It will also write the default `style.css` and
    /// `index.js` files to the root unless a custom theme is
    /// provided, in which case it will copy the `style.css` and `index.js`
    /// files from the custom theme's `dist` directory.
    fn write_assets(&self) -> DocResult<()> {
        let dir = &self.root;
        let custom_theme = self.custom_theme.as_ref();
        let assets_dir = dir.join("assets");
        std::fs::create_dir_all(&assets_dir)
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to create assets directory: `{}`",
                    assets_dir.display()
                )
            })?;

        if let Some(custom_theme) = custom_theme {
            if !custom_theme.exists() {
                return Err(IoError::new(
                    ErrorKind::NotFound,
                    format!(
                        "custom theme does not exist at `{}`",
                        custom_theme.display()
                    ),
                )
                .into());
            }
            std::fs::copy(
                custom_theme.join("dist").join("style.css"),
                dir.join("style.css"),
            )
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to copy stylesheet from `{}` to `{}`",
                    custom_theme.join("dist").join("style.css").display(),
                    dir.join("style.css").display()
                )
            })?;
            std::fs::copy(
                custom_theme.join("dist").join("index.js"),
                dir.join("index.js"),
            )
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to copy web components from `{}` to `{}`",
                    custom_theme.join("dist").join("index.js").display(),
                    dir.join("index.js").display()
                )
            })?;
        } else {
            std::fs::write(
                dir.join("style.css"),
                include_str!("../theme/dist/style.css"),
            )
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to write default stylesheet to `{}`",
                    dir.join("style.css").display()
                )
            })?;
            std::fs::write(dir.join("index.js"), include_str!("../theme/dist/index.js"))
                .map_err(Into::<DocError>::into)
                .with_context(|| {
                    format!(
                        "failed to write default web components to `{}`",
                        dir.join("index.js").display()
                    )
                })?;
        }

        for (file_name, bytes) in get_assets() {
            let path = assets_dir.join(file_name);
            std::fs::write(&path, bytes)
                .map_err(Into::<DocError>::into)
                .with_context(|| format!("failed to write asset to `{}`", path.display()))?;
        }
        // The above `get_assets()` call will write the default logos; then the
        // following logic may overwrite those files with user supplied logos.
        match (&self.logo, &self.alt_logo) {
            (Some(dark_logo), Some(light_logo)) => {
                let logo_path = assets_dir.join(LOGO_FILE_NAME);
                std::fs::copy(dark_logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy dark theme custom logo from `{}` to `{}`",
                            dark_logo.display(),
                            logo_path.display()
                        )
                    })?;
                let logo_path = assets_dir.join(LIGHT_LOGO_FILE_NAME);
                std::fs::copy(light_logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy light theme custom logo from `{}` to `{}`",
                            light_logo.display(),
                            logo_path.display()
                        )
                    })?;
            }
            (Some(logo), None) => {
                let logo_path = assets_dir.join(LOGO_FILE_NAME);
                std::fs::copy(logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy custom logo from `{}` to `{}`",
                            logo.display(),
                            logo_path.display()
                        )
                    })?;
                let logo_path = assets_dir.join(LIGHT_LOGO_FILE_NAME);
                std::fs::copy(logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy custom logo from `{}` to `{}`",
                            logo.display(),
                            logo_path.display()
                        )
                    })?;
            }
            (None, Some(logo)) => {
                let logo_path = assets_dir.join(LOGO_FILE_NAME);
                std::fs::copy(logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy custom logo from `{}` to `{}`",
                            logo.display(),
                            logo_path.display()
                        )
                    })?;
                let logo_path = assets_dir.join(LIGHT_LOGO_FILE_NAME);
                std::fs::copy(logo, &logo_path)
                    .map_err(Into::<DocError>::into)
                    .with_context(|| {
                        format!(
                            "failed to copy custom logo from `{}` to `{}`",
                            logo.display(),
                            logo_path.display()
                        )
                    })?;
            }
            (None, None) => {}
        }

        Ok(())
    }
}

/// A tree representing the docs directory.
///
/// For construction, see [`DocsTreeBuilder`].
#[derive(Debug)]
pub struct DocsTree {
    /// The root of the tree.
    root: Node,
    /// The absolute path to the root directory.
    path: PathBuf,
    /// An optional path to a Markdown file which will be embedded in the
    /// `<root>/index.html` page.
    index_page: Option<PathBuf>,
    /// External URLs related to the project, rendered in the right rail.
    external_urls: ExternalUrls,
    /// Site-level SEO metadata embedded into each page's `<head>`.
    seo: Seo,
    /// Optional extra HTML to embed in each page.
    additional_html: AdditionalHtml,
    /// Initialize in light mode instead of the default dark mode.
    init_light_mode: bool,
    /// Optional workspace module metadata, present when the documented
    /// workspace is a WDL module.
    workspace_metadata: Option<WorkspaceMetadata>,
}

impl DocsTree {
    /// Get the root of the tree.
    fn root(&self) -> &Node {
        &self.root
    }

    /// Get the root of the tree as mutable.
    fn root_mut(&mut self) -> &mut Node {
        &mut self.root
    }

    /// Get the absolute path to the root directory.
    fn root_abs_path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the path to the root directory relative to a given path.
    fn root_relative_to<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        diff_paths(self.root_abs_path(), path).expect("should diff paths")
    }

    /// Builds the absolute canonical URL for the page written to
    /// `page_abs_path`, using the configured SEO `base_url`. Returns `None`
    /// when no `base_url` is set or the page lies outside the docs root.
    fn canonical_url(&self, page_abs_path: &Path) -> Option<String> {
        let base = self.seo.base_url.as_ref()?;
        let relative = page_abs_path
            .strip_prefix(self.root_abs_path())
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        base.join(&relative).ok().map(|url| url.to_string())
    }

    /// Get the absolute path to the assets directory.
    fn assets(&self) -> PathBuf {
        self.root_abs_path().join("assets")
    }

    /// Get a relative path to the assets directory.
    fn assets_relative_to<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        diff_paths(self.assets(), path).expect("should diff paths")
    }

    /// Get a relative path to an asset in the assets directory (converted to a
    /// string).
    fn get_asset<P: AsRef<Path>>(&self, path: P, asset: &str) -> String {
        self.assets_relative_to(path)
            .join(asset)
            .to_string_lossy()
            .to_string()
    }

    /// Get a relative path to the root index page.
    fn root_index_relative_to<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        diff_paths(self.root_abs_path().join("index.html"), path).expect("should diff paths")
    }

    /// Add a page to the tree.
    ///
    /// Path can be an absolute path or a path relative to the root.
    pub(crate) fn add_page<P: Into<PathBuf>>(&mut self, path: P, page: Rc<HTMLPage>) {
        let path = path.into();
        let rel_path = path.strip_prefix(self.root_abs_path()).unwrap_or(&path);

        let root = self.root_mut();
        let mut current_node = root;

        let mut components = rel_path.components().peekable();
        while let Some(component) = components.next() {
            let cur_name = component.as_os_str().to_string_lossy();
            if current_node.children.contains_key(cur_name.as_ref()) {
                current_node = current_node
                    .children
                    .get_mut(cur_name.as_ref())
                    .expect("node should exist");
            } else {
                // A directory node whose entrypoint collapsed onto it stores
                // its path as `.../index.html` (see the `index.html` handling
                // below). When descending into a *sibling* document's
                // subdirectory, derive the child path from the enclosing
                // directory, not that collapsed page path, so the file name
                // does not leak in as a path component.
                let parent_dir = if current_node.path().ends_with("index.html") {
                    current_node
                        .path()
                        .parent()
                        .expect("collapsed index path should have a parent")
                } else {
                    current_node.path()
                };
                let new_path = parent_dir.join(component);
                let new_node = Node::new(cur_name.to_string(), new_path);
                current_node.children.insert(cur_name.to_string(), new_node);
                current_node = current_node
                    .children
                    .get_mut(cur_name.as_ref())
                    .expect("node should exist");
            }
            if let Some(next_component) = components.peek()
                && next_component.as_os_str().to_string_lossy() == "index.html"
            {
                current_node.path = current_node.path().join("index.html");
                break;
            }
        }

        current_node.page = Some(page);
    }

    /// Get the [`Node`] associated with a path.
    ///
    /// Path can be an absolute path or a path relative to the root.
    fn get_node<P: AsRef<Path>>(&self, path: P) -> Option<&Node> {
        let root = self.root();
        let path = path.as_ref();
        let rel_path = path.strip_prefix(self.root_abs_path()).unwrap_or(path);

        let mut current_node = root;

        for component in rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
        {
            if component == "index.html" {
                return Some(current_node);
            }
            if current_node.children.contains_key(component.as_ref()) {
                current_node = current_node
                    .children
                    .get(component.as_ref())
                    .expect("node should exist");
            } else {
                return None;
            }
        }

        Some(current_node)
    }

    /// Get the [`HTMLPage`] associated with a path.
    ///
    /// Can be an absolute path or a path relative to the root.
    fn get_page<P: AsRef<Path>>(&self, path: P) -> Option<&Rc<HTMLPage>> {
        self.get_node(path).and_then(|node| node.page())
    }

    /// Get workflows by category.
    fn get_workflows_by_category(&self) -> Vec<(String, Vec<&Node>)> {
        let mut workflows_by_category = Vec::new();
        let mut categories = HashSet::new();
        let mut nodes = Vec::new();

        for node in self.root().depth_first_traversal() {
            if let Some(page) = node.page()
                && let PageType::Workflow(workflow) = page.page_type()
            {
                if node
                    .path()
                    .iter()
                    .next()
                    .expect("path should have a next component")
                    .to_string_lossy()
                    == "external"
                {
                    categories.insert("External".to_string());
                } else if let Some(category) = workflow.category() {
                    categories.insert(category);
                } else {
                    categories.insert("Other".to_string());
                }
                nodes.push(node);
            }
        }
        let sorted_categories = sort_workflow_categories(categories);

        for category in sorted_categories {
            let workflows = nodes
                .iter()
                .filter(|node| {
                    let page = node
                        .page()
                        .map(|p| p.page_type())
                        .expect("node should have a page");
                    if let PageType::Workflow(workflow) = page {
                        if node
                            .path()
                            .iter()
                            .next()
                            .expect("path should have a next component")
                            .to_string_lossy()
                            == "external"
                        {
                            return category == "External";
                        } else if let Some(cat) = workflow.category() {
                            return cat == category;
                        } else {
                            return category == "Other";
                        }
                    }
                    unreachable!("expected a workflow page");
                })
                .cloned()
                .collect::<Vec<_>>();
            workflows_by_category.push((category, workflows));
        }

        workflows_by_category
    }

    /// Render a left sidebar component in the "workflows view" mode given a
    /// path.
    ///
    /// Destination is expected to be an absolute path.
    fn sidebar_workflows_view(&self, destination: &Path) -> Markup {
        let base = destination
            .parent()
            .expect("destination should have a parent");
        let workflows_by_category = self.get_workflows_by_category();
        html! {
            @for (category, workflows) in workflows_by_category {
                li class="" {
                    div class="left-sidebar__row left-sidebar__row--unclickable" {
                        img src=(self.get_asset(base, "category-selected.svg")) class="left-sidebar__icon block light:hidden" alt="Category icon";
                        img src=(self.get_asset(base, "category-selected.light.svg")) class="left-sidebar__icon hidden light:block" alt="Category icon";
                        p class="text-slate-50" { (category) }
                    }
                    ul class="" {
                        @for node in workflows {
                            a href=(diff_paths(self.root_abs_path().join(node.path()), base).expect("should diff paths").to_string_lossy()) x-data=(format!(r#"{{
                                    node: {{
                                        current: {},
                                        icon: '{}',
                                    }}
                                }}"#,
                                self.root_abs_path().join(node.path()) == destination,
                                self.get_asset(base, "workflow.svg"),
                            )) class="left-sidebar__row" x-bind:class="node.current ? 'bg-slate-600/50 is-scrolled-to' : 'hover:bg-slate-700/40'" {
                                @if let Some(page) = node.page() {
                                    @match page.page_type() {
                                        PageType::Workflow(wf) => {
                                            div class="left-sidebar__indent -1" {}
                                            div class="left-sidebar__content-item-container crop-ellipsis"{
                                                img x-bind:src="node.icon" class="left-sidebar__icon light:hidden" alt="Workflow icon";
                                                img x-bind:src="node.icon?.replace('.svg', '.light.svg')" class="left-sidebar__icon hidden light:block" alt="Workflow icon";
                                                sprocket-tooltip content=(wf.render_name()) class="crop-ellipsis" x-bind:class="node.current ? 'text-slate-50' : 'group-hover:text-slate-50'" {
                                                    span {
                                                        (wf.render_name())
                                                    }
                                                }
                                            }
                                        }
                                        _ => {
                                            p { "ERROR: Not a workflow page" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Render a left sidebar component given a path.
    ///
    /// Path is expected to be an absolute path.
    // TODO: lots here can be improved
    // e.g. it could be broken into smaller functions, the JS could be
    // generated in a more structured way, etc.
    fn render_left_sidebar<P: AsRef<Path>>(&self, path: P) -> Markup {
        let root = self.root();
        let path = path.as_ref();
        let rel_path = path
            .strip_prefix(self.root_abs_path())
            .expect("path should be in root");
        let base = path.parent().expect("path should have a parent");

        let make_key = |path: &Path| -> String {
            let path = if path.file_name().expect("path should have a file name") == "index.html" {
                // Remove unnecessary index.html from the path.
                // Not needed for the key.
                path.parent().expect("path should have a parent")
            } else {
                path
            };
            path.to_string_lossy()
                .replace("-", "_")
                .replace(".", "_")
                .replace(std::path::MAIN_SEPARATOR_STR, "_")
        };

        // Local dependency module entrypoint documents are collapsed onto
        // their module's root directory (see
        // `WorkspaceMetadata::documentation_path`), so a node's
        // directory (i.e. its path without a trailing `index.html`) can
        // be looked up directly as a module root.
        let node_module = |path: &Path| -> Option<&ModuleMetadata> {
            let dir = if path.file_name().expect("path should have a file name") == "index.html" {
                path.parent().expect("path should have a parent")
            } else {
                path
            };
            self.workspace_metadata
                .as_ref()
                .and_then(|metadata| metadata.module_at_root(dir))
        };

        #[derive(Serialize)]
        struct JsNode {
            /// The key of the node.
            key: String,
            /// The display name of the node.
            display_name: String,
            /// The parent directory of the node.
            ///
            /// This is used for displaying the path to the node in the sidebar.
            parent: String,
            /// The search name of the node.
            search_name: String,
            /// The icon for the node.
            icon: Option<String>,
            /// The href for the node.
            href: Option<String>,
            /// Whether this node is the root of a WDL module.
            module_root: bool,
            /// Whether the node is ancestor.
            ancestor: bool,
            /// Whether the node is the current page.
            current: bool,
            /// The nest level of the node.
            nest_level: usize,
            /// The children of the node.
            children: Vec<String>,
        }

        let all_nodes = root
            .depth_first_traversal()
            .iter()
            .skip(1) // Skip the root node
            .map(|node| {
                let key = make_key(node.path());
                let module = node_module(node.path());
                let display_name = match (module, node.page()) {
                    (Some(module), _) => module.name().to_string(),
                    (None, Some(page)) => page.name().to_string(),
                    (None, None) => node.name().to_string(),
                };
                let module_root = module.is_some();
                let parent = node
                    .path()
                    .parent()
                    .expect("path should have a parent")
                    .to_string_lossy()
                    .to_string();
                let search_name = if node.page().is_none() {
                    // Page-less nodes should not be searchable
                    "".to_string()
                } else {
                    node.path().to_string_lossy().to_string()
                };
                let href = if node.page().is_some() {
                    Some(
                        diff_paths(self.root_abs_path().join(node.path()), base)
                            .expect("should diff paths")
                            .to_string_lossy()
                            .to_string(),
                    )
                } else {
                    None
                };
                let ancestor = node.part_of_path(rel_path);
                let current = path == self.root_abs_path().join(node.path());
                let icon = node.page().map(|page| {
                    self.get_asset(
                        base,
                        match page.page_type() {
                            PageType::Task(_) => "task.svg",
                            PageType::Struct(_) => "struct.svg",
                            PageType::Enum(_) => "enum.svg",
                            PageType::Workflow(_) => "workflow.svg",
                            // WDL modules render as a folder; a standalone WDL
                            // document renders as a file.
                            PageType::Index(_) => {
                                if module.is_some() {
                                    "wdl-folder.svg"
                                } else {
                                    "wdl-file.svg"
                                }
                            }
                        },
                    )
                });
                let nest_level = node
                    .path()
                    .components()
                    .filter(|c| c.as_os_str().to_string_lossy() != "index.html")
                    .count();
                let children = node
                    .children()
                    .values()
                    .map(|child| make_key(child.path()))
                    .collect::<Vec<String>>();
                JsNode {
                    key,
                    display_name,
                    parent,
                    search_name: search_name.clone(),
                    icon,
                    href,
                    module_root,
                    ancestor,
                    current,
                    nest_level,
                    children,
                }
            })
            .collect::<Vec<JsNode>>();

        let js_dag = all_nodes
            .iter()
            .map(|node| {
                let children = node
                    .children
                    .iter()
                    .map(|child| format!("'{child}'"))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("'{}': [{}]", node.key, children)
            })
            .collect::<Vec<String>>()
            .join(", ");

        let all_nodes_true = all_nodes
            .iter()
            .map(|node| format!("'{}': true", node.key))
            .collect::<Vec<String>>()
            .join(", ");

        let is_module = self.workspace_metadata.is_some();
        let show_workflows_field = if is_module {
            String::new()
        } else {
            "showWorkflows: $persist(false).using(sessionStorage),".to_string()
        };

        let data = format!(
            r#"{{
                {show_workflows_field}
                dirOpen: '{}',
                dirClosed: '{}',
                nodes: [{}],
                get shownNodes() {{
                    return this.nodes.filter(node => this.showSelfCache[node.key]);
                }},
                dag: {{{}}},
                showSelfCache: $persist({{{}}}).using(sessionStorage),
                showChildrenCache: $persist({{{}}}).using(sessionStorage),
                children(key) {{
                    return this.dag[key];
                }},
                toggleChildren(key) {{
                    this.nodes.forEach(n => {{
                        if (n.key === key) {{
                            this.showChildrenCache[key] = !this.showChildrenCache[key];
                            this.children(key).forEach(child => {{
                                this.setShow(child, this.showChildrenCache[key]);
                            }});
                        }}
                    }});
                }},
                setShow(key, value) {{
                    this.nodes.forEach(n => {{
                        if (n.key === key) {{
                            this.showSelfCache[key] = value;
                            this.showChildrenCache[key] = value;
                            this.children(key).forEach(child => {{
                                this.setShow(child, value);
                            }});
                        }}
                    }});
                }},
                reset() {{
                    this.nodes.forEach(n => {{
                        this.showSelfCache[n.key] = true;
                        this.showChildrenCache[n.key] = true;
                    }});
                }}
            }}"#,
            self.get_asset(base, "chevron-up.svg"),
            self.get_asset(base, "chevron-down.svg"),
            all_nodes
                .iter()
                .map(|node| serde_json::to_string(node).expect("should serialize node"))
                .collect::<Vec<String>>()
                .join(", "),
            js_dag,
            all_nodes_true,
            all_nodes_true,
        );

        // The root node link and per-node rows are shared between module and
        // non-module workspaces; only the surrounding chrome (the tabs and
        // the competing "Workflows" view) differs.
        let directory_tree = html! {
            // Root node for the directory tree
            li {
                sprocket-tooltip content=(root.name()) class="block" {
                    a href=(self.root_index_relative_to(base).to_string_lossy()) aria-label=(root.name()) class="left-sidebar__row hover:bg-slate-700/40" {
                        div class="left-sidebar__content-item-container crop-ellipsis" {
                            div class="relative shrink-0" {
                                img src=(self.get_asset(base, "dir-open.svg")) class="left-sidebar__icon block light:hidden" alt="Directory icon";
                                img src=(self.get_asset(base, "dir-open.light.svg")) class="left-sidebar__icon hidden light:block" alt="Directory icon";
                            }
                            div class="text-slate-50" { (root.name()) }
                        }
                    }
                }
            }
            // Nodes in the directory tree
            template x-for="node in shownNodes" {
                li {
                sprocket-tooltip x-bind:content="node.display_name" class="block isolate" {
                    a x-bind:href="node.href" x-show="showSelfCache[node.key]" x-on:click="if (node.href === null) toggleChildren(node.key)" x-bind:tabindex="node.href === null ? '0' : null" x-bind:role="node.href === null ? 'button' : null" "x-on:keydown.enter"="if (node.href === null) toggleChildren(node.key)" "x-on:keydown.space.prevent"="if (node.href === null) toggleChildren(node.key)" x-bind:aria-label="node.display_name" class="left-sidebar__row" x-bind:class="`${node.current ? 'is-scrolled-to left-sidebar__row--active' : (node.href === null) ? showChildrenCache[node.key] ? 'left-sidebar__row-folder left-sidebar__row-folder--open' : 'left-sidebar__row-folder left-sidebar__row-folder--closed' : 'left-sidebar__row-page'} ${node.ancestor ? 'left-sidebar__content-item-container--ancestor' : ''}`" {
                        template x-for="i in Array.from({ length: node.nest_level })" {
                            div class="left-sidebar__indent -z-1" {}
                        }
                        div class="left-sidebar__content-item-container crop-ellipsis" {
                            // Disclosure chevron for expandable page nodes (WDL modules and
                            // documents). Leaf pages get an equal-width spacer so their icons
                            // stay aligned with expandable siblings.
                            img x-show="node.href && node.children.length" x-on:click="$event.preventDefault(); $event.stopPropagation(); toggleChildren(node.key);" x-bind:src="dirOpen" x-bind:class="showChildrenCache[node.key] ? '' : 'rotate-180'" class="left-sidebar__chevron block light:hidden" alt="";
                            img x-show="node.href && node.children.length" x-on:click="$event.preventDefault(); $event.stopPropagation(); toggleChildren(node.key);" x-bind:src="dirOpen.replace('.svg', '.light.svg')" x-bind:class="showChildrenCache[node.key] ? '' : 'rotate-180'" class="left-sidebar__chevron hidden light:block" alt="";
                            div x-show="node.href && !node.children.length" class="left-sidebar__chevron" {}
                            div class="relative left-sidebar__icon shrink-0" {
                                img x-bind:src="(node.module_root && showChildrenCache[node.key]) ? node.icon.replace('wdl-folder.svg', 'wdl-folder-open.svg') : (node.icon || dirOpen)" class="left-sidebar__icon block light:hidden" alt="Node icon" x-bind:class="`${(node.icon === null) && !showChildrenCache[node.key] ? 'rotate-180' : ''}`";
                                img x-bind:src="((node.module_root && showChildrenCache[node.key]) ? node.icon.replace('wdl-folder.svg', 'wdl-folder-open.svg') : (node.icon || dirOpen)).replace('.svg', '.light.svg')" class="left-sidebar__icon hidden light:block" alt="Node icon" x-bind:class="`${(node.icon === null) && !showChildrenCache[node.key] ? 'rotate-180' : ''}`";
                            }
                            div class="crop-ellipsis" x-text="node.display_name" {}
                        }
                    }
                }
                }
            }
        };

        if is_module {
            // Module workspaces render one semantic module tree; there is
            // no separate "Workflows" view competing with it, so the tabs
            // and the "Full Directory" toggle are omitted entirely.
            html! {
                div x-data=(data) x-cloak x-init="$nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'instant' }); })" class="left-sidebar__container" {
                    div x-cloak class="left-sidebar__content-container pt-4" {
                        ul class="left-sidebar__content" {
                            (directory_tree)
                        }
                    }
                }
            }
        } else {
            html! {
                div x-data=(data) x-cloak x-init="$nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'instant' }); })" class="left-sidebar__container" {
                    // top navbar
                    div class="sticky px-4" {
                        div class="left-sidebar__tabs-container mt-4" {
                            button x-on:click="showWorkflows = true; search = ''; $nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'instant' }); })" class="left-sidebar__tabs text-slate-50 border-b-slate-50" x-bind:class="! showWorkflows ? 'opacity-70 hover:opacity-90' : ''" {
                                img src=(self.get_asset(base, "list-bullet-selected.svg")) class="left-sidebar__icon block light:hidden" alt="List icon";
                                img src=(self.get_asset(base, "list-bullet-selected.light.svg")) class="left-sidebar__icon hidden light:block" alt="List icon";
                                p { "Workflows" }
                            }
                            button x-on:click="showWorkflows = false; $nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'instant' }); })" class="left-sidebar__tabs text-slate-50 border-b-slate-50" x-bind:class="showWorkflows ? 'opacity-70 hover:opacity-90' : ''" {
                                img src=(self.get_asset(base, "folder-selected.svg")) class="left-sidebar__icon block light:hidden" alt="List icon";
                                img src=(self.get_asset(base, "folder-selected.light.svg")) class="left-sidebar__icon hidden light:block" alt="List icon";
                                p { "Full Directory" }
                            }
                        }
                    }
                    // Main content
                    div x-cloak class="left-sidebar__content-container pt-4" {
                        // Full directory view
                        ul x-show="! showWorkflows" class="left-sidebar__content" {
                            (directory_tree)
                        }
                        // Workflows view
                        ul x-show="showWorkflows" class="left-sidebar__content" {
                            (self.sidebar_workflows_view(path))
                        }
                    }
                }
            }
        }
    }

    /// Render a right sidebar component.
    fn render_right_sidebar(&self, headers: PageSections, assets: &Path) -> Markup {
        fn project_link(assets: &Path, url: &Url, icon_name: &str, label: &str) -> Markup {
            html! {
                a class="right-sidebar__project-link" target="_blank" rel="noopener noreferrer" href=(url) {
                    img src=(assets.join(format!("{icon_name}.svg")).to_string_lossy()) class="right-sidebar__project-link-icon block light:hidden" alt="";
                    img src=(assets.join(format!("{icon_name}.light.svg")).to_string_lossy()) class="right-sidebar__project-link-icon hidden light:block" alt="";
                    span { (label) }
                }
            }
        }

        let ExternalUrls {
            github,
            homepage,
            slack,
        } = &self.external_urls;
        let has_project_links = github.is_some() || homepage.is_some() || slack.is_some();

        html! {
            div class="right-sidebar__container" {
                div class="right-sidebar__sticky" {
                    div class="right-sidebar__header" {
                        "ON THIS PAGE"
                    }
                    nav id="page-sections" data-page-sections {
                        (headers.render())
                    }
                    @if has_project_links {
                        div class="right-sidebar__project-links" aria-label="Project links" {
                            @if let Some(homepage) = homepage {
                                (project_link(assets, homepage, "link-chain", "Website"))
                            }
                            @if let Some(github) = github {
                                (project_link(assets, github, "github", "GitHub"))
                            }
                            @if let Some(slack) = slack {
                                (project_link(assets, slack, "slack", "Slack"))
                            }
                        }
                    }
                    div class="right-sidebar__back-to-top-container" {
                        a
                            href="#title"
                            "x-on:click.prevent"="(document.querySelector('.layout__main-center') || document.scrollingElement).scrollTo({ top: 0, behavior: 'smooth' })"
                            class="right-sidebar__back-to-top" {
                            span class="right-sidebar__back-to-top-icon" {
                                "↑"
                            }
                            span class="right-sidebar__back-to-top-text" {
                                "Back to top"
                            }
                        }
                    }
                }
            }
        }
    }

    /// Renders a page "breadcrumb" navigation component.
    ///
    /// Path is expected to be an absolute path.
    fn render_breadcrumbs<P: AsRef<Path>>(&self, path: P) -> Markup {
        let path = path.as_ref();
        let base = path.parent().expect("path should have a parent");

        let mut current_path = path
            .strip_prefix(self.root_abs_path())
            .expect("path should be in the docs directory");

        let mut breadcrumbs = vec![];

        let cur_page = self.get_page(path).expect("path should have a page");
        match cur_page.page_type() {
            PageType::Index(_) => {
                // Index pages are handled by the below while loop
            }
            _ => {
                // Last crumb, i.e. the current page, should not be clickable
                breadcrumbs.push((cur_page.name(), None));
            }
        }

        while let Some(parent) = current_path.parent() {
            let cur_node = self.get_node(parent).expect("path should have a node");
            if let Some(page) = cur_node.page() {
                breadcrumbs.push((
                    page.name(),
                    if self.root_abs_path().join(cur_node.path()) == path {
                        // Don't insert a link to the current page.
                        // This happens on index pages.
                        None
                    } else {
                        Some(
                            diff_paths(self.root_abs_path().join(cur_node.path()), base)
                                .expect("should diff paths"),
                        )
                    },
                ));
            } else if cur_node.name() == self.root().name() {
                breadcrumbs.push((cur_node.name(), Some(self.root_index_relative_to(base))))
            } else {
                breadcrumbs.push((cur_node.name(), None));
            }
            current_path = parent;
        }
        breadcrumbs.reverse();
        let mut breadcrumbs = breadcrumbs.into_iter();
        let root_crumb = breadcrumbs
            .next()
            .expect("should have at least one breadcrumb");
        let root_crumb = html! {
            a class="layout__breadcrumb-clickable" href=(root_crumb.1.expect("root crumb should have path").to_string_lossy()) { (root_crumb.0) }
        };

        html! {
            div class="layout__breadcrumb-container" data-pagefind-ignore="all" {
                (root_crumb)
                @for crumb in breadcrumbs {
                    span { " / " }
                    @if let Some(path) = crumb.1 {
                        a href=(path.to_string_lossy()) class="layout__breadcrumb-clickable" { (crumb.0) }
                    } @else {
                        span class="layout__breadcrumb-inactive" { (crumb.0) }
                    }
                }
            }
        }
    }

    /// Render every page in the tree.
    pub fn render_all(&self) -> DocResult<()> {
        let root = self.root();
        let links = self.build_link_index();

        for node in root.depth_first_traversal() {
            if let Some(page) = node.page() {
                self.write_page(
                    page.as_ref(),
                    self.root_abs_path().join(node.path()),
                    &links,
                )
                .with_context(|| format!("failed to write page at `{}`", node.path().display()))?;
            }
        }

        self.write_index_page()?;

        Ok(())
    }

    /// Write the root index page to disk.
    fn write_index_page(&self) -> DocResult<()> {
        let index_path = self.root_abs_path().join("index.html");

        let left_sidebar = self.render_left_sidebar(&index_path);
        let content = html! {
            @if let Some(index_page) = &self.index_page {
                div class="main__section" {
                    div
                        class="markdown-body"
                        data-pagefind-body
                        meta-img-dark="home.svg"
                        meta-img-light="home.light.svg"
                        data-pagefind-meta="image_dark[meta-img-dark], image_light[meta-img-light]"
                    {
                        (Markdown(std::fs::read_to_string(index_page).map_err(Into::<DocError>::into).with_context(|| {
                            format!("failed to read provided index page file: `{}`", index_page.display())
                        })?).render())
                    }
                }
            } @else {
                div class="main__section--empty" {
                    img src=(self.get_asset(self.root_abs_path(), "missing-home.svg")) class="size-12 block light:hidden" alt="Missing home icon";
                    img src=(self.get_asset(self.root_abs_path(), "missing-home.light.svg")) class="size-12 hidden light:block" alt="Missing home icon";
                    h2 class="main__section-header" { "There's nothing to see on this page" }
                    p { "The markdown file for this page wasn't supplied." }
                }
            }
        };

        let index_page_content = html! {
            @if let Some(metadata) = &self.workspace_metadata {
                (self.render_module_overview(metadata))
            }
            (content)
        };

        let html = full_page(
            "Home",
            self.render_layout(
                left_sidebar,
                index_page_content,
                self.render_right_sidebar(
                    PageSections::default(),
                    &self.assets_relative_to(self.root_abs_path()),
                ),
                None,
                &self.assets_relative_to(self.root_abs_path()),
                &index_path,
            ),
            self.root().path(),
            &self.additional_html,
            self.init_light_mode,
            &self.seo,
            self.canonical_url(&index_path).as_deref(),
        );
        std::fs::write(&index_path, html.into_string())
            .map_err(Into::<DocError>::into)
            .with_context(|| {
                format!(
                    "failed to write root index page to `{}`",
                    index_path.display()
                )
            })?;
        Ok(())
    }

    /// Renders the generated module overview shown at the top of the root
    /// index page when the documented workspace is a WDL module.
    fn render_module_overview(&self, metadata: &WorkspaceMetadata) -> Markup {
        let root = metadata.root();
        // Modules to list as cards: the workspace's dependencies when the root
        // is itself a module, or simply every discovered module for a
        // manifest-less monorepo root.
        let modules = metadata
            .modules()
            .filter(|module| !module.root().as_os_str().is_empty())
            .collect::<Vec<_>>();
        // Without a root module, the overview is titled after the workspace
        // (the docs root node name).
        let title = match root {
            Some(root) => humanize_module_name(root.name()),
            None => humanize_module_name(self.root().name()),
        };

        html! {
            section class="module-overview" {
                p class="module-overview__eyebrow" {
                    (if root.is_some() { "WDL Module" } else { "WDL Modules" })
                }
                h1 id="title" class="module-overview__title" data-pagefind-meta="title" { (title) }
                @if let Some(root) = root {
                    @if let Some(description) = root.description() {
                        p class="module-overview__description" { (description) }
                    }
                    div class="module-overview__metadata" {
                        div class="module-overview__metadata-item" {
                            span class="module-overview__metadata-label" { "Entrypoint" }
                            code class="module-overview__metadata-value" { (root.entrypoint().display().to_string()) }
                        }
                    }
                }
                @if !modules.is_empty() {
                    div class="module-overview__dependencies" {
                        h2 class="module-overview__dependencies-header" {
                            (if root.is_some() { "Dependencies" } else { "Modules" })
                        }
                        div class="module-overview__dependencies-list" {
                            @for module in &modules {
                                div class="module-overview__dependency-card" {
                                    p class="module-overview__dependency-name" { (module.name()) }
                                    @if let Some(description) = module.description() {
                                        p class="module-overview__dependency-description" { (description) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Render reusable sidebar control buttons
    fn render_sidebar_control_buttons(&self, assets: &Path) -> Markup {
        html! {
            button
                type="button"
                aria-label="Hide sidebar"
                x-on:click="collapseSidebar()"
                x-bind:aria-pressed="sidebarState === 'hidden'"
                x-bind:class="getSidebarButtonClass('hidden')" {
                img src=(assets.join("sidebar-icon-hide.svg").to_string_lossy()) alt="" class="block light:hidden" {}
                img src=(assets.join("sidebar-icon-hide.light.svg").to_string_lossy()) alt="" class="hidden light:block" {}
            }
            button
                type="button"
                aria-label="Default sidebar width"
                x-on:click="restoreSidebar()"
                x-bind:aria-pressed="sidebarState === 'normal'"
                x-bind:class="getSidebarButtonClass('normal')" {
                img src=(assets.join("sidebar-icon-default.svg").to_string_lossy()) alt="" class="block light:hidden" {}
                img src=(assets.join("sidebar-icon-default.light.svg").to_string_lossy()) alt="" class="hidden light:block" {}
            }
            button
                type="button"
                aria-label="Expand sidebar"
                x-on:click="expandSidebar()"
                x-bind:aria-pressed="sidebarState === 'xl'"
                x-bind:class="getSidebarButtonClass('xl')" {
                    img src=(assets.join("sidebar-icon-expand.svg").to_string_lossy()) alt="" class="block light:hidden" {}
                    img src=(assets.join("sidebar-icon-expand.light.svg").to_string_lossy()) alt="" class="hidden light:block" {}
                }
        }
    }

    /// Render the header nav.
    fn render_header(&self, assets: &Path, path: &Path) -> Markup {
        let base = path.parent().expect("path should have a parent");
        html! {
            div
                class="layout__header"
                "@keydown.window.meta.k"="
                if (!['INPUT', 'TEXTAREA'].includes($event.target.tagName) && !$event.target.isContentEditable) {
                    $event.preventDefault();
                    $refs.searchBox.focus();
                }"
                "@keydown.window.ctrl.k"="
                if (!['INPUT', 'TEXTAREA'].includes($event.target.tagName) && !$event.target.isContentEditable) {
                    $event.preventDefault();
                    $refs.searchBox.focus();
                }"
            {
                div class="header__content" {
                    div class="col-start-1 flex items-center gap-2 min-w-0" {
                        button
                            type="button"
                            class="header__button shrink-0 md:hidden"
                            aria-label="Toggle navigation"
                            x-on:click="sidebarState = sidebarState === 'hidden' ? 'normal' : 'hidden'"
                        {
                            svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="size-6" aria-hidden="true" {
                                path stroke-linecap="round" stroke-linejoin="round" d="M3 6h18M3 12h18M3 18h18" {}
                            }
                        }
                        a
                            href=(self.root_index_relative_to(base).to_string_lossy())
                            id="logo"
                            class="header__logo"
                        {
                            img src=(self.get_asset(base, LOGO_FILE_NAME)) class="max-w-full max-h-full w-auto h-auto object-contain p-1 block light:hidden" alt="Logo";
                            img src=(self.get_asset(base, LIGHT_LOGO_FILE_NAME)) class="max-w-full max-h-full w-auto h-auto object-contain p-1 hidden light:block" alt="Logo";
                        }
                    }
                    div id="search" class="header__search" {
                        input id="searchbox" "x-ref"="searchBox" "x-model.debounce"="$store.search.query" type="text" placeholder="Search...";
                        img src=(assets.join("search.svg").to_string_lossy()) class="absolute left-2 top-1/2 -translate-y-1/2 size-6 pointer-events-none block light:hidden" alt="Search icon";
                        img src=(assets.join("search.light.svg").to_string_lossy()) class="absolute left-2 top-1/2 -translate-y-1/2 size-6 pointer-events-none hidden light:block" alt="Search icon";
                        img src=(assets.join("x-mark.svg").to_string_lossy()) class="absolute right-2 top-1/2 -translate-y-1/2 size-6 hover:cursor-pointer block light:hidden" alt="Clear icon" x-show="$store.search.query !== ''" x-on:click="$store.search.query = ''";
                        img src=(assets.join("x-mark.light.svg").to_string_lossy()) class="absolute right-2 top-1/2 -translate-y-1/2 size-6 hover:cursor-pointer hidden light:block" alt="Clear icon" x-show="$store.search.query !== ''" x-on:click="$store.search.query = ''";
                        (render_search_shortcut_hint())
                    }
                    div class="header__actions" x-data="{ showTooltip: false }" {
                        div class="relative" {
                            button
                            x-on:click="
                            document.documentElement.classList.toggle('light')
                            theme = document.documentElement.classList.contains('light') ? 'light' : 'dark'
                            "
                            "@mouseenter"="showTooltip = true"
                            "@mouseleave"="showTooltip = false"
                            "@focusin"="showTooltip = true"
                            "@focusout"="showTooltip = false"
                            id="theme-toggle"
                            aria-label="Switch theme"
                            class="header__button" {
                                img src=(assets.join("moon.light.svg").to_string_lossy()) alt="" class="size-6 hidden light:block";
                                img src=(assets.join("sun.svg").to_string_lossy()) alt="" class="size-6 block light:hidden";
                            }

                            div class="absolute top-full flex flex-col items-center left-1/2 -translate-x-1/2 mt-2" x-show="showTooltip" {
                                div class="w-3 h-3 -mb-2 rotate-45 bg-slate-800" {}
                                div class="relative z-10 px-3 py-2 text-sm text-slate-200 bg-slate-800 rounded-md shadow-lg whitespace-nowrap" {
                                    "Switch theme"
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Render the main layout template with left sidebar, content, and right
    /// sidebar.
    fn render_layout(
        &self,
        left_sidebar: Markup,
        content: Markup,
        right_sidebar: Markup,
        breadcrumbs: Option<Markup>,
        assets: &Path,
        path: &Path,
    ) -> Markup {
        html! {
            div class="layout__container layout__container--alt-layout" x-data="{
                sidebarState: $persist(window.innerWidth < 768 ? 'hidden' : 'normal').using(sessionStorage),
                get showSidebarButtons() { return this.sidebarState !== 'hidden'; },
                get showCenterButtons() { return this.sidebarState === 'hidden'; },
                get containerClasses() {
                    const base = 'layout__container layout__container--alt-layout';
                    switch(this.sidebarState) {
                        case 'hidden': return base + ' layout__container--left-hidden';
                        case 'xl': return base + ' layout__container--left-xl';
                        default: return base;
                    }
                },
                getSidebarButtonClass(state) {
                    return 'left-sidebar__size-button ' + (this.sidebarState === state ? 'left-sidebar__size-button--active' : '');
                },
                collapseSidebar() { this.sidebarState = 'hidden'; },
                restoreSidebar() { this.sidebarState = 'normal'; },
                expandSidebar() { this.sidebarState = 'xl'; }
            }" x-bind:class="containerClasses" x-effect="document.documentElement.dataset.sidebar = sidebarState" {
                (self.render_header(assets, path))
                div
                    class="layout__sidebar-left"
                    x-bind:inert="sidebarState === 'hidden' || $store.search.query !== ''"
                    x-on:click="if (window.innerWidth < 768 && $event.target.closest('a[href]')) collapseSidebar()" {
                    div class="left-sidebar__controls" x-show="showSidebarButtons" {
                        (self.render_sidebar_control_buttons(assets))
                    }
                    (left_sidebar)
                }
                div
                    class="md:hidden fixed inset-0 z-30 bg-black/50"
                    x-cloak
                    x-show="sidebarState !== 'hidden'"
                    x-on:click="collapseSidebar()"
                    aria-hidden="true" {}
                div class="layout__main-center" {
                    div class="layout__main-shell" {
                        div class="layout__main-center-content layout__main-body" {
                            @if let Some(breadcrumbs) = breadcrumbs {
                                div class="layout__breadcrumbs" x-show="$store.search.query === ''" {
                                    (breadcrumbs)
                                }
                            }
                            div class="flex flex-col gap-5" x-show="$store.search.query !== ''" x-data {
                                h2 class="text-base leading-6 font-medium" x-text="`${$store.search.results.length} results for '${$store.search.query}'`" {}
                                ul class="flex flex-col gap-5" {
                                    template x-for="result in $store.search.results" ":key"="result.url" {
                                        li class="search-result" {
                                            div class="flex flex-row gap-2 items-center" {
                                                @let assets_str = assets.to_string_lossy().replace('\\', "/");
                                                img
                                                    class="size-6"
                                                    x-bind:src=(format!("theme === 'dark' ? `{assets_str}/${{result.meta.image_dark}}` : `{assets_str}/${{result.meta.image_light}}`"))
                                                    x-bind:alt="result.meta.image_alt || result.meta.title";
                                                a
                                                    ":href"="result.url"
                                                    class="text-2xl leading-8 text-slate-50 font-medium"
                                                    x-text="result.meta.title"
                                                {}
                                            }
                                            p class="search-result-excerpt" x-html="result.excerpt" {}
                                        }
                                    }

                                    div x-show="!$store.search.loading && $store.search.results.length === 0" {
                                        // No results found icon
                                        li class="flex place-content-center" {
                                            img src=(assets.join("search.svg").to_string_lossy()) class="size-8 block light:hidden" alt="Search icon";
                                            img src=(assets.join("search.light.svg").to_string_lossy()) class="size-8 hidden light:block" alt="Search icon";
                                        }
                                        // No results found message
                                        li class="flex gap-1 place-content-center text-center break-words whitespace-normal text-sm text-slate-500" {
                                            span x-text="'No results found for'" {}
                                            span x-text="`\"${$store.search.query}\"`" class="text-slate-50" {}
                                        }
                                    }
                                }
                            }
                            div {
                                div class="flex gap-1 mb-3 max-md:hidden" x-show="showCenterButtons" {
                                    (self.render_sidebar_control_buttons(assets))
                                }
                            }
                            div x-show="$store.search.query === ''" {
                                (content)
                            }
                        }
                        div class="layout__main-rail" {
                            (right_sidebar)
                        }
                    }
                }
            }
        }
    }

    /// Build an index of uniquely named generated struct and enum pages.
    ///
    /// Struct and enum page names that resolve to more than one page are
    /// excluded so that ambiguous type references remain plain text. Paths are
    /// stored relative to the docs root.
    fn build_link_index(&self) -> PageLinkIndex {
        let pages = self
            .root()
            .depth_first_traversal()
            .into_iter()
            .filter_map(|node| {
                let page = node.page()?;
                match page.page_type() {
                    PageType::Struct(_) | PageType::Enum(_) => {
                        Some((page.name().to_string(), node.path().clone()))
                    }
                    _ => None,
                }
            });

        PageLinkIndex::from_pages(pages)
    }

    /// Write a page to disk at the designated path.
    ///
    /// Path is expected to be an absolute path.
    fn write_page<P: Into<PathBuf>>(
        &self,
        page: &HTMLPage,
        path: P,
        links: &PageLinkIndex,
    ) -> DocResult<()> {
        let path = path.into();
        let base = path.parent().expect("path should have a parent");

        let page_dir = path
            .strip_prefix(self.root_abs_path())
            .ok()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let (content, headers) = match page.page_type() {
            PageType::Index(doc) => doc.render(),
            PageType::Struct(s) => s.render(&self.assets_relative_to(base), links, &page_dir),
            PageType::Enum(e) => e.render(&self.assets_relative_to(base)),
            PageType::Task(t) => t.render(&self.assets_relative_to(base), links, &page_dir),
            PageType::Workflow(w) => w.render(&self.assets_relative_to(base), links, &page_dir),
        };

        let breadcrumbs = self.render_breadcrumbs(&path);

        let left_sidebar = self.render_left_sidebar(&path);

        let html = full_page(
            page.name(),
            self.render_layout(
                left_sidebar,
                content,
                self.render_right_sidebar(headers, &self.assets_relative_to(base)),
                Some(breadcrumbs),
                &self.assets_relative_to(base),
                &path,
            ),
            self.root_relative_to(base),
            &self.additional_html,
            self.init_light_mode,
            &self.seo,
            self.canonical_url(&path).as_deref(),
        );
        std::fs::write(&path, html.into_string())
            .map_err(Into::<DocError>::into)
            .with_context(|| format!("failed to write page at `{}`", path.display()))?;
        Ok(())
    }
}

/// Sort workflow categories in a specific order.
fn sort_workflow_categories(categories: HashSet<String>) -> Vec<String> {
    let mut sorted_categories: Vec<String> = categories.into_iter().collect();
    sorted_categories.sort_by(|a, b| {
        if a == b {
            std::cmp::Ordering::Equal
        } else if a == "External" {
            std::cmp::Ordering::Greater
        } else if b == "External" {
            std::cmp::Ordering::Less
        } else if a == "Other" {
            std::cmp::Ordering::Greater
        } else if b == "Other" {
            std::cmp::Ordering::Less
        } else {
            a.cmp(b)
        }
    });
    sorted_categories
}

/// Renders the platform-aware search keyboard shortcut hint.
///
/// Both the macOS (`⌘ K`) and other-platform (`Ctrl K`) variants are always
/// rendered; the inapplicable variant is hidden at runtime by the theme
/// JavaScript based on the visitor's platform.
fn render_search_shortcut_hint() -> Markup {
    html! {
        div id="search-shortcut-hint" x-show="$store.search.query === ''" aria-hidden="true" {
            span class="search-shortcut" data-shortcut="mac" {
                kbd class="search-shortcut__key" { "⌘" }
                kbd class="search-shortcut__key" { "K" }
            }
            span class="search-shortcut" data-shortcut="other" hidden {
                kbd class="search-shortcut__key search-shortcut__key--wide" { "Ctrl" }
                kbd class="search-shortcut__key" { "K" }
            }
        }
    }
}

/// Converts a kebab-case or snake_case module manifest name into a
/// human-friendly title by replacing separators with spaces and
/// capitalizing each word (e.g. `spellcraft-showcase` becomes `Spellcraft
/// Showcase`).
fn humanize_module_name(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::workspace::WorkspaceMetadata;

    #[test]
    fn search_shortcut_hint_renders_platform_variants() {
        let html = render_search_shortcut_hint().into_string();
        assert!(
            html.contains("data-shortcut=\"mac\""),
            "expected a macOS shortcut variant, got: {html}"
        );
        assert!(
            html.contains("data-shortcut=\"other\""),
            "expected a non-macOS shortcut variant, got: {html}"
        );
        assert!(
            html.contains('⌘'),
            "expected the macOS `⌘` key in the hint, got: {html}"
        );
        assert!(
            html.contains("Ctrl"),
            "expected the `Ctrl` key in the hint, got: {html}"
        );
        assert!(
            html.contains('K'),
            "expected the `K` key in the hint, got: {html}"
        );
        // The hint must present the `⌘ K` / `Ctrl K` shortcut, not the old `/`.
        assert!(
            !html.contains(">/<"),
            "expected the `/` shortcut hint to be replaced, got: {html}"
        );
    }

    /// The minimal module-workspace fixture checked into the repository,
    /// reused here as a realistic fixture for module-aware navigation
    /// tests. Unlike the local `wdl-doc-showcase/` demo (which is untracked
    /// and not guaranteed to exist in a fresh clone), this fixture is
    /// committed under `tests/fixtures/` specifically so these tests are
    /// self-contained.
    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module-workspace")
    }

    /// Builds a `TempDir` containing copies of the fixture's `module.json`
    /// manifests, mirroring its on-disk module layout (root plus the local
    /// `wards` and `enchantment` dependencies the root manifest declares)
    /// without copying the WDL source files. Suitable for tests that only
    /// need `WorkspaceMetadata`, not a fully documentable workspace.
    fn manifest_only_workspace() -> TempDir {
        let fixture = fixture_root();
        let dir = tempfile::tempdir().unwrap();

        for relative_manifest in [
            "module.json",
            "modules/wards/module.json",
            "modules/enchantment/module.json",
        ] {
            let src = fixture.join(relative_manifest);
            let dst = dir.path().join(relative_manifest);
            // SAFETY: `relative_manifest` always has a parent component
            // (`modules/wards` and friends, or the workspace root itself).
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(&src, &dst).unwrap();
        }

        dir
    }

    /// Builds a `TempDir` containing a full copy of the fixture workspace
    /// (manifests and WDL sources), suitable for actually generating
    /// documentation end-to-end.
    fn full_module_workspace() -> TempDir {
        let fixture = fixture_root();
        let dir = tempfile::tempdir().unwrap();

        for relative_file in [
            "module.json",
            "main.wdl",
            "modules/wards/module.json",
            "modules/wards/wards.wdl",
            "modules/enchantment/module.json",
            "modules/enchantment/enchantment.wdl",
        ] {
            let src = fixture.join(relative_file);
            let dst = dir.path().join(relative_file);
            // SAFETY: `relative_file` always has a parent component
            // (`modules/wards` and friends, or the workspace root itself).
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(&src, &dst).unwrap();
        }

        dir
    }

    #[test]
    fn module_workspace_uses_manifest_root_name_and_labels_module_directories() {
        let workspace_dir = manifest_only_workspace();
        let metadata = WorkspaceMetadata::load(workspace_dir.path()).unwrap();

        let docs_dir = tempfile::tempdir().unwrap();
        let mut tree = DocsTreeBuilder::new(docs_dir.path())
            .maybe_workspace_metadata(metadata)
            .build()
            .unwrap();

        assert_eq!(tree.root().name(), "spellcraft-showcase");

        // Manually mirror the directory node that document generation would
        // create for the `wards` module's collapsed entrypoint document (see
        // `WorkspaceMetadata::documentation_path`): a plain "wards" directory
        // node rooted at `modules/wards`, without needing a full analysis run.
        let modules_node = Node::new("modules".to_string(), PathBuf::from("modules"));
        tree.root_mut()
            .children
            .insert("modules".to_string(), modules_node);
        let wards_node = Node::new("wards".to_string(), PathBuf::from("modules/wards"));
        tree.root_mut()
            .children
            .get_mut("modules")
            .expect("modules node should exist")
            .children
            .insert("wards".to_string(), wards_node);

        let page = docs_dir.path().join("modules/wards/index.html");
        let sidebar = tree.render_left_sidebar(&page).into_string();
        assert!(sidebar.contains("wards"));
        assert!(!sidebar.contains("Workflows"));
    }

    /// Builds a lightweight task-backed page for tree-shape tests. The page's
    /// type is irrelevant to `add_page`'s path bookkeeping; only its presence
    /// on a node matters here.
    fn task_page(name: &str) -> Rc<HTMLPage> {
        let source = format!("version 1.0\ntask {name} {{\n    command <<< >>>\n}}\n");
        let (doc, _) = wdl_ast::Document::parse(&source, None);
        let item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let definition = item.into_task_definition().unwrap();
        let task = Task::new(
            name.to_string(),
            wdl_ast::SupportedVersion::V1(wdl_ast::version::V1::Zero),
            definition,
            None,
            false,
        );
        Rc::new(HTMLPage::new(name.to_string(), PageType::Task(task)))
    }

    /// Regression test: a module's collapsed `index.html` must not leak into
    /// the paths of sibling documents added to the same module directory
    /// afterward.
    ///
    /// The `wards` module's entrypoint (`wards.wdl`) collapses onto the
    /// `modules/wards` directory node, storing its page at
    /// `modules/wards/index.html`. A second WDL file in the same module
    /// (`modules/wards/scrying.wdl`) documents into `modules/wards/scrying/
    /// `. If the directory node's collapsed path is reused verbatim when
    /// creating that subdirectory, the sibling's pages end up under a bogus
    /// `modules/wards/index.html/scrying/...`, whose `index.html` component
    /// is a file, not a directory (`ENOTDIR` at write time).
    #[test]
    fn collapsed_index_does_not_leak_into_sibling_document_paths() {
        let docs_dir = tempfile::tempdir().unwrap();
        let mut tree = DocsTreeBuilder::new(docs_dir.path()).build().unwrap();

        // Order matters: the entrypoint (which collapses onto the directory
        // node) is added before the sibling document, matching the ordering
        // that triggered the original failure.
        tree.add_page(
            docs_dir
                .path()
                .join("modules/wards/inspect_wards-task.html"),
            task_page("inspect_wards"),
        );
        tree.add_page(
            docs_dir.path().join("modules/wards/index.html"),
            task_page("wards"),
        );
        tree.add_page(
            docs_dir
                .path()
                .join("modules/wards/scrying/scry_runes-task.html"),
            task_page("scry_runes"),
        );

        let node = tree
            .get_node("modules/wards/scrying/scry_runes-task.html")
            .expect("sibling document node should exist");
        assert_eq!(
            node.path(),
            &PathBuf::from("modules/wards/scrying/scry_runes-task.html"),
            "sibling document path must not contain the collapsed `index.html`"
        );
    }

    #[test]
    fn non_module_workspace_uses_output_dir_name_and_keeps_custom_index() {
        let docs_dir = tempfile::tempdir().unwrap();
        let index_source = tempfile::tempdir().unwrap();
        let index_path = index_source.path().join("index.md");
        fs::write(
            &index_path,
            r#"<div class="wdl-tests-dark">Custom homepage content marker</div>"#,
        )
        .unwrap();

        let tree = DocsTreeBuilder::new(docs_dir.path())
            .maybe_workspace_metadata(None)
            .index_page(index_path)
            .build()
            .unwrap();

        let expected_name = docs_dir
            .path()
            .file_name()
            .expect("docs dir should have a file name")
            .to_string_lossy()
            .to_string();
        assert_eq!(tree.root().name(), expected_name);

        tree.write_index_page().unwrap();
        let content = fs::read_to_string(docs_dir.path().join("index.html")).unwrap();
        assert!(!content.contains("main__homepage-header"));
        assert!(content.contains("Custom homepage content marker"));
        assert!(content.contains("class=\"wdl-tests-dark\""));
        assert!(!content.contains("module-overview"));
    }

    #[tokio::test]
    async fn module_workspace_renders_module_overview_and_navigation() {
        let workspace_dir = full_module_workspace();
        let docs_dir = tempfile::tempdir().unwrap();

        let config = crate::Config::new(
            wdl_analysis::Config::default()
                .with_feature_flags(wdl_analysis::FeatureFlags::default().with_wdl_1_4()),
            workspace_dir.path(),
            docs_dir.path(),
        );

        crate::document_workspace(config)
            .await
            .expect("documentation generation should succeed");

        let content = fs::read_to_string(docs_dir.path().join("index.html")).unwrap();
        assert!(content.contains("Spellcraft Showcase"));
        assert!(content.contains("Entrypoint"));
        assert!(content.contains("main.wdl"));
        assert!(content.contains("Dependencies"));
        assert!(content.contains("wards"));
        // The module overview title is a humanized display name, not a WDL
        // identifier, so it must stay plain prose without code-literal styling.
        assert!(
            content.contains(
                "module-overview__title\" data-pagefind-meta=\"title\">Spellcraft Showcase"
            ),
            "expected a plain module overview title, got: {content}"
        );
        assert!(
            !content.contains("heading-code-literal"),
            "module overview page must not apply code-literal heading styling"
        );
    }

    #[test]
    fn project_links_render_below_page_navigation() {
        let docs_dir = tempfile::tempdir().expect("temporary docs directory");
        let tree = DocsTreeBuilder::new(docs_dir.path())
            .external_urls(ExternalUrls {
                homepage: Some(Url::parse("https://sprocket.bio").expect("valid test URL")),
                github: Some(
                    Url::parse("https://github.com/stjude-rust-labs/sprocket")
                        .expect("valid test URL"),
                ),
                slack: Some(Url::parse("https://example.slack.com").expect("valid test URL")),
            })
            .build()
            .expect("documentation tree");

        let sidebar = tree
            .render_right_sidebar(PageSections::default(), Path::new("assets"))
            .into_string();
        let navigation_position = sidebar.find("data-page-sections").expect("page navigation");
        let links_position = sidebar
            .find("right-sidebar__project-links")
            .expect("project links");

        assert!(navigation_position < links_position);
        assert!(sidebar.contains(">GitHub</span>"));
        assert!(sidebar.contains(">Website</span>"));
        assert!(sidebar.contains(">Slack</span>"));
        assert!(sidebar.contains("https://github.com/stjude-rust-labs/sprocket"));
        assert!(sidebar.contains("https://sprocket.bio/"));
        assert!(sidebar.contains("https://example.slack.com/"));
        let website_position = sidebar.find(">Website</span>").expect("website link");
        let github_position = sidebar.find(">GitHub</span>").expect("GitHub link");
        let slack_position = sidebar.find(">Slack</span>").expect("Slack link");
        assert!(website_position < github_position);
        assert!(github_position < slack_position);

        let header = tree
            .render_header(Path::new("assets"), Path::new("index.html"))
            .into_string();
        assert!(!header.contains("https://github.com/stjude-rust-labs/sprocket"));
        assert!(!header.contains("https://sprocket.bio/"));
    }

    #[test]
    fn right_rail_is_nested_beside_the_main_body() {
        let docs_dir = tempfile::tempdir().expect("temporary docs directory");
        let tree = DocsTreeBuilder::new(docs_dir.path())
            .build()
            .expect("documentation tree");
        let layout = tree
            .render_layout(
                html! {},
                html! { div id="main-body-marker" {} },
                html! { aside id="right-rail-marker" {} },
                Some(html! { span id="breadcrumb-marker" {} }),
                Path::new("assets"),
                Path::new("index.html"),
            )
            .into_string();

        assert!(layout.contains("layout__main-body"));
        assert!(layout.contains("layout__main-rail"));
        assert!(!layout.contains("layout__sidebar-right"));
        assert!(!layout.contains("x-transition"));
        assert!(!layout.contains("Loading..."));
        assert!(
            layout.contains("class=\"layout__breadcrumbs\" x-show=\"$store.search.query === ''\"")
        );
        assert!(layout.contains("class=\"left-sidebar__controls\""));
        assert!(layout.contains("x-show=\"showSidebarButtons\""));

        let body_position = layout.find("main-body-marker").expect("main body marker");
        let rail_position = layout.find("right-rail-marker").expect("right rail marker");
        assert!(body_position < rail_position);
    }

    #[test]
    fn project_links_omit_unconfigured_destinations() {
        let docs_dir = tempfile::tempdir().expect("temporary docs directory");
        let tree = DocsTreeBuilder::new(docs_dir.path())
            .external_urls(ExternalUrls {
                homepage: Some(Url::parse("https://sprocket.bio").expect("valid test URL")),
                github: None,
                slack: None,
            })
            .build()
            .expect("documentation tree");

        let sidebar = tree
            .render_right_sidebar(PageSections::default(), Path::new("assets"))
            .into_string();

        assert!(sidebar.contains(">Website</span>"));
        assert!(!sidebar.contains(">GitHub</span>"));
        assert!(!sidebar.contains(">Slack</span>"));
    }

    #[test]
    fn project_link_assets_use_bootstrap_icons() {
        let assets = get_assets();
        let slack = std::str::from_utf8(assets.get("slack.svg").expect("bundled Slack icon asset"))
            .expect("Slack icon is valid UTF-8");
        let github =
            std::str::from_utf8(assets.get("github.svg").expect("bundled GitHub icon asset"))
                .expect("GitHub icon is valid UTF-8");
        let website = std::str::from_utf8(
            assets
                .get("link-chain.svg")
                .expect("bundled website icon asset"),
        )
        .expect("website icon is valid UTF-8");

        assert!(slack.contains("<title>Slack</title>"));
        assert!(slack.contains("viewBox=\"0 0 16 16\""));
        assert!(slack.contains("M3.362 10.11"));
        assert!(github.contains("<title>GitHub</title>"));
        assert!(github.contains("viewBox=\"0 0 16 16\""));
        assert!(github.contains("M8 0C3.58 0"));
        assert!(website.contains("<title>Website</title>"));
        assert!(website.contains("viewBox=\"0 0 16 16\""));
        assert!(website.contains("M4.715 6.542"));
    }
}
