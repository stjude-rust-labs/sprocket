//! Implementations for a [`DocsTree`] which represents the docs directory.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::rc::Rc;

use anyhow::Context;
use anyhow::Result;
use maud::Markup;
use maud::html;
use path_clean::PathClean;
use pathdiff::diff_paths;
use serde::Serialize;

use crate::Markdown;
use crate::Render;
use crate::document::Document;
use crate::full_page;
use crate::r#struct::Struct;
use crate::task::Task;
use crate::workflow::Workflow;
use crate::write_assets;

/// The type of a page.
#[derive(Debug)]
pub(crate) enum PageType {
    /// An index page.
    Index(Document),
    /// A struct page.
    Struct(Struct),
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
    homepage: Option<PathBuf>,
    /// An optional path to a custom theme to use for the docs.
    custom_theme: Option<PathBuf>,
}

impl DocsTreeBuilder {
    /// Create a new docs tree builder.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = absolute(root.as_ref())
            .expect("should get absolute path")
            .clean();
        Self {
            root,
            homepage: None,
            custom_theme: None,
        }
    }

    /// Set the homepage for the docs with an option.
    pub fn maybe_homepage(mut self, homepage: Option<impl Into<PathBuf>>) -> Self {
        self.homepage = homepage.map(|hp| hp.into());
        self
    }

    /// Set the homepage for the docs.
    pub fn homepage(self, homepage: impl Into<PathBuf>) -> Self {
        self.maybe_homepage(Some(homepage))
    }

    /// Set the custom theme for the docs with an option.
    pub fn maybe_custom_theme(mut self, theme: Option<impl Into<PathBuf>>) -> Self {
        self.custom_theme = theme.map(|s| s.into());
        self
    }

    /// Set the custom theme for the docs.
    pub fn custom_theme(self, theme: impl Into<PathBuf>) -> Self {
        self.maybe_custom_theme(Some(theme))
    }

    /// Build the docs tree.
    pub fn build(self) -> Result<DocsTree> {
        write_assets(&self.root, self.custom_theme.as_ref()).with_context(|| {
            format!(
                "failed to write assets to output directory: `{}`",
                self.root.display()
            )
        })?;
        let node = Node::new(
            self.root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or("docs".to_string()),
            PathBuf::from(""),
        );
        Ok(DocsTree {
            root: node,
            path: self.root,
            homepage: self.homepage,
        })
    }
}

/// A tree representing the docs directory.
#[derive(Debug)]
pub struct DocsTree {
    /// The root of the tree.
    root: Node,
    /// The absolute path to the root directory.
    path: PathBuf,
    /// An optional path to a Markdown file which will be embedded in the
    /// `<root>/index.html` page.
    homepage: Option<PathBuf>,
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
    pub fn root_relative_to<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        diff_paths(self.root_abs_path(), path).expect("should diff paths")
    }

    /// Get the absolute path to the stylesheet.
    pub fn stylesheet(&self) -> PathBuf {
        self.root_abs_path().join("style.css")
    }

    /// Get the absolute path to the assets directory.
    pub fn assets(&self) -> PathBuf {
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
                let new_path = current_node.path().join(component);
                let new_node = Node::new(cur_name.to_string(), new_path);
                current_node.children.insert(cur_name.to_string(), new_node);
                current_node = current_node
                    .children
                    .get_mut(cur_name.as_ref())
                    .expect("node should exist");
            }
            if let Some(next_component) = components.peek() {
                if next_component.as_os_str().to_string_lossy() == "index.html" {
                    current_node.path = current_node.path().join("index.html");
                    break;
                }
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
    /// Can be an abolute path or a path relative to the root.
    fn get_page<P: AsRef<Path>>(&self, path: P) -> Option<&Rc<HTMLPage>> {
        self.get_node(path).and_then(|node| node.page())
    }

    /// Get workflows by category.
    fn get_workflows_by_category(&self) -> Vec<(String, Vec<&Node>)> {
        let mut workflows_by_category = Vec::new();
        let mut categories = HashSet::new();
        let mut nodes = Vec::new();

        for node in self.root().depth_first_traversal() {
            if let Some(page) = node.page() {
                if let PageType::Workflow(workflow) = page.page_type() {
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
                    div class="left-sidebar__row" {
                        img src=(self.get_asset(base, "category-selected.svg")) class="left-sidebar__icon" alt="Category icon";
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
                                self.get_asset(base, if self.root_abs_path().join(node.path()) == destination {
                                        "workflow-selected.svg"
                                    } else {
                                        "workflow-unselected.svg"
                                    },
                            ))) class="left-sidebar__row" x-bind:class="node.current ? 'bg-slate-700/50 is-scrolled-to' : 'hover:bg-slate-800'" {
                                @if let Some(page) = node.page() {
                                    @match page.page_type() {
                                        PageType::Workflow(wf) => {
                                            div class="left-sidebar__indent -1" {}
                                            div class="left-sidebar__content-item-container crop-ellipsis"{
                                                img x-bind:src="node.icon" class="left-sidebar__icon" alt="Workflow icon";
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
                let display_name = match node.page() {
                    Some(page) => page.name().to_string(),
                    None => node.name().to_string(),
                };
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
                let icon = match node.page() {
                    Some(page) => match page.page_type() {
                        PageType::Task(_) => Some(self.get_asset(
                            base,
                            if ancestor {
                                "task-selected.svg"
                            } else {
                                "task-unselected.svg"
                            },
                        )),
                        PageType::Struct(_) => Some(self.get_asset(
                            base,
                            if ancestor {
                                "struct-selected.svg"
                            } else {
                                "struct-unselected.svg"
                            },
                        )),
                        PageType::Workflow(_) => Some(self.get_asset(
                            base,
                            if ancestor {
                                "workflow-selected.svg"
                            } else {
                                "workflow-unselected.svg"
                            },
                        )),
                        PageType::Index(_) => Some(self.get_asset(
                            base,
                            if ancestor {
                                "wdl-dir-selected.svg"
                            } else {
                                "wdl-dir-unselected.svg"
                            },
                        )),
                    },
                    None => None,
                };
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

        let data = format!(
            r#"{{
                showWorkflows: $persist(true).using(sessionStorage),
                search: $persist('').using(sessionStorage),
                dirOpen: '{}',
                dirClosed: '{}',
                nodes: [{}],
                get searchedNodes() {{
                    if (this.search === '') {{
                        return [];
                    }}
                    this.showWorkflows = false;
                    return this.nodes.filter(node => node.search_name.toLowerCase().includes(this.search.toLowerCase()));
                }},
                get shownNodes() {{
                    if (this.search !== '') {{
                        return [];
                    }}
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

        html! {
            div x-data=(data) x-init="$nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'smooth' }); })" class="left-sidebar__container" {
                // top navbar
                div class="sticky px-4" {
                    a href=(self.root_index_relative_to(base).to_string_lossy()) {
                        img src=(self.get_asset(base, "sprocket-logo.svg")) class="w-[120px] flex-none mb-8" alt="Sprocket logo";
                    }
                    div class="relative w-full h-10" {
                        input id="searchbox" "x-model.debounce"="search" type="text" placeholder="Search..." class="left-sidebar__searchbox";
                        img src=(self.get_asset(base, "search.svg")) class="absolute left-2 top-1/2 -translate-y-1/2 size-6 pointer-events-none" alt="Search icon";
                        img src=(self.get_asset(base, "x-mark.svg")) class="absolute right-2 top-1/2 -translate-y-1/2 size-6 hover:cursor-pointer" alt="Clear icon" x-show="search !== ''" x-on:click="search = ''";
                    }
                    div class="left-sidebar__tabs-container mt-4" {
                        button x-on:click="showWorkflows = true; search = ''; $nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'smooth' }); })" class="left-sidebar__tabs text-slate-50 border-b-slate-50" x-bind:class="! showWorkflows ? 'opacity-40 hover:opacity-80' : ''" {
                            img src=(self.get_asset(base, "list-bullet-selected.svg")) class="left-sidebar__icon" alt="List icon";
                            p { "Workflows" }
                        }
                        button x-on:click="showWorkflows = false; $nextTick(() => { document.querySelector('.is-scrolled-to')?.scrollIntoView({ block: 'center', behavior: 'smooth' }); })" class="left-sidebar__tabs text-slate-50 border-b-slate-50" x-bind:class="showWorkflows ? 'opacity-40 hover:opacity-80' : ''" {
                            img src=(self.get_asset(base, "folder-selected.svg")) class="left-sidebar__icon" alt="List icon";
                            p { "Full Directory" }
                        }
                    }
                }
                // Main content
                div x-cloak class="left-sidebar__content-container pt-4" {
                    // Full directory view
                    ul x-show="! showWorkflows || search != ''" class="left-sidebar__content" {
                        // Root node for the directory tree
                        sprocket-tooltip content=(root.name()) class="block" {
                            a href=(self.root_index_relative_to(base).to_string_lossy()) x-show="search === ''" aria-label=(root.name()) class="left-sidebar__row hover:bg-slate-700" {
                                div class="left-sidebar__content-item-container crop-ellipsis" {
                                    div class="relative shrink-0" {
                                        img src=(self.get_asset(base, "dir-open.svg")) class="left-sidebar__icon" alt="Directory icon";
                                    }
                                    div class="text-slate-50" { (root.name()) }
                                }
                            }
                        }
                        // Nodes in the directory tree
                        template x-for="node in shownNodes" {
                            sprocket-tooltip x-bind:content="node.display_name" class="block isolate" {
                                a x-bind:href="node.href" x-show="showSelfCache[node.key]" x-on:click="if (node.href === null) toggleChildren(node.key)" x-bind:aria-label="node.display_name" class="left-sidebar__row" x-bind:class="`${node.current ? 'is-scrolled-to left-sidebar__row--active' : (node.href === null) ? showChildrenCache[node.key] ? 'left-sidebar__row-folder left-sidebar__row-folder--open' : 'left-sidebar__row-folder left-sidebar__row-folder--closed' : 'left-sidebar__row-page'} ${node.ancestor ? 'left-sidebar__content-item-container--ancestor' : ''}`" {
                                    template x-for="i in Array.from({ length: node.nest_level })" {
                                        div class="left-sidebar__indent -z-1" {}
                                    }
                                    div class="left-sidebar__content-item-container crop-ellipsis" {
                                        div class="relative left-sidebar__icon shrink-0" {
                                            img x-bind:src="node.icon || dirOpen" class="left-sidebar__icon" alt="Node icon" x-bind:class="`${(node.icon === null) && !showChildrenCache[node.key] ? 'rotate-180' : ''}`";
                                        }
                                        div class="crop-ellipsis" x-text="node.display_name" {
                                        }
                                    }
                                }
                            }
                        }
                        // Search results
                        template x-for="node in searchedNodes" {
                            li class="left-sidebar__search-result-item" {
                                p class="text-xs text-slate-500 crop-ellipsis" x-text="node.parent" {}
                                div class="left-sidebar__search-result-item-container" {
                                    img x-bind:src="node.icon" class="left-sidebar__icon" alt="Node icon";
                                    sprocket-tooltip class="crop-ellipsis" x-bind:content="node.display_name" {
                                        a x-bind:href="node.href" x-text="node.display_name" {}
                                    }
                                }
                            }
                        }
                        // No results found icon
                        li x-show="search !== '' && searchedNodes.length === 0" class="flex place-content-center" {
                            img src=(self.get_asset(base, "search.svg")) class="size-8" alt="Search icon";
                        }
                        // No results found message
                        li x-show="search !== '' && searchedNodes.length === 0" class="flex gap-1 place-content-center text-center break-words whitespace-normal text-sm text-slate-500" {
                            span x-text="'No results found for'" {}
                            span x-text="`\"${search}\"`" class="text-slate-50" {}
                        }
                    }
                    // Workflows view
                    ul x-show="showWorkflows && search === ''" class="left-sidebar__content" {
                        (self.sidebar_workflows_view(path))
                    }
                }
            }
        }
    }

    /// Render a right sidebar component.
    fn render_right_sidebar(&self, headers: PageSections) -> Markup {
        html! {
            div class="right-sidebar__container" {
                div class="right-sidebar__header" {
                    "ON THIS PAGE"
                }
                (headers.render())
                div class="right-sidebar__back-to-top-container" {
                    // TODO: this should be a link to the top of the page, not just a link to the title
                    a href="#title" class="right-sidebar__back-to-top" {
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
            div class="layout__breadcrumb-container" {
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
    pub fn render_all(&self) -> Result<()> {
        let root = self.root();

        for node in root.depth_first_traversal() {
            if let Some(page) = node.page() {
                self.write_page(page.as_ref(), self.root_abs_path().join(node.path()))
                    .with_context(|| {
                        format!("failed to write page at `{}`", node.path().display())
                    })?;
            }
        }

        self.write_homepage()
            .with_context(|| "failed to write homepage".to_string())?;
        Ok(())
    }

    /// Write the homepage to disk.
    fn write_homepage(&self) -> Result<()> {
        let index_path = self.root_abs_path().join("index.html");

        let left_sidebar = self.render_left_sidebar(&index_path);
        let content = html! {
            @if let Some(homepage) = &self.homepage {
                div class="main__section" {
                    div class="markdown-body" {
                        (Markdown(std::fs::read_to_string(homepage).with_context(|| {
                            format!("failed to read provided homepage file: `{}`", homepage.display())
                        })?).render())
                    }
                }
            } @else {
                div class="main__section--empty" {
                    img src=(self.get_asset(self.root_abs_path(), "missing-home.svg")) class="size-12" alt="Missing home icon";
                    h2 class="main__section-header" { "There's nothing to see on this page" }
                    p { "The markdown file for this page wasn't supplied." }
                }
            }
        };

        let homepage_content = html! {
            h5 class="main__homepage-header" {
                "Home"
            }
            (content)
        };

        let html = full_page(
            "Home",
            self.render_layout(
                left_sidebar,
                homepage_content,
                self.render_right_sidebar(PageSections::default()),
                None,
                &self.assets_relative_to(self.root_abs_path()),
            ),
            self.root().path(),
        );
        std::fs::write(&index_path, html.into_string())
            .with_context(|| format!("failed to write homepage to `{}`", index_path.display()))?;
        Ok(())
    }

    /// Render reusable sidebar control buttons
    fn render_sidebar_control_buttons(&self, assets: &Path) -> Markup {
        html! {
            button
                x-on:click="collapseSidebar()"
                x-bind:disabled="sidebarState === 'hidden'"
                x-bind:class="getSidebarButtonClass('hidden')" {
                img src=(assets.join("sidebar-icon-hide.svg").to_string_lossy()) alt="" {}
            }
            button
                x-on:click="restoreSidebar()"
                x-bind:disabled="sidebarState === 'normal'"
                x-bind:class="getSidebarButtonClass('normal')" {
                img src=(assets.join("sidebar-icon-default.svg").to_string_lossy()) alt="" {}
            }
            button
                x-on:click="expandSidebar()"
                x-bind:disabled="sidebarState === 'xl'"
                x-bind:class="getSidebarButtonClass('xl')" {
                    img src=(assets.join("sidebar-icon-expand.svg").to_string_lossy()) alt="" {}
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
    ) -> Markup {
        html! {
            div class="layout__container layout__container--alt-layout" x-transition x-data="{
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
            }" x-bind:class="containerClasses" {
                div class="layout__sidebar-left" x-transition {
                    div class="absolute top-5 right-2 flex gap-1 z-10" x-cloak x-show="showSidebarButtons" {
                        (self.render_sidebar_control_buttons(assets))
                    }
                    (left_sidebar)
                }
                div class="layout__main-center" {
                    div class="layout__main-center-content" {
                        div {
                            div class="flex gap-1 mb-3" x-show="showCenterButtons" {
                                (self.render_sidebar_control_buttons(assets))
                            }
                            @if let Some(breadcrumbs) = breadcrumbs {
                                div class="layout__breadcrumbs" {
                                    (breadcrumbs)
                                }
                            }
                        }
                        (content)
                    }
                }
                div class="layout__sidebar-right" {
                    (right_sidebar)
                }
            }
        }
    }

    /// Write a page to disk at the designated path.
    ///
    /// Path is expected to be an absolute path.
    fn write_page<P: Into<PathBuf>>(&self, page: &HTMLPage, path: P) -> Result<()> {
        let path = path.into();
        let base = path.parent().expect("path should have a parent");

        let (content, headers) = match page.page_type() {
            PageType::Index(doc) => doc.render(),
            PageType::Struct(s) => s.render(),
            PageType::Task(t) => t.render(&self.assets_relative_to(base)),
            PageType::Workflow(w) => w.render(&self.assets_relative_to(base)),
        };

        let breadcrumbs = self.render_breadcrumbs(&path);

        let left_sidebar = self.render_left_sidebar(&path);

        let html = full_page(
            page.name(),
            self.render_layout(
                left_sidebar,
                content,
                self.render_right_sidebar(headers),
                Some(breadcrumbs),
                &self.assets_relative_to(base),
            ),
            self.root_relative_to(base),
        );
        std::fs::write(&path, html.into_string())
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
