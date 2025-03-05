//! Implementations for a [`DocsTree`] which represents the docs directory.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::rc::Rc;

use maud::Markup;
use maud::html;
use pathdiff::diff_paths;

use crate::Document;
use crate::full_page;
use crate::r#struct::Struct;
use crate::task::Task;
use crate::workflow::Workflow;

/// The type of a page.
#[derive(Debug)]
pub enum PageType {
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
pub struct HTMLPage {
    /// The display name of the page.
    name: String,
    /// The type of the page.
    page_type: PageType,
}

impl HTMLPage {
    /// Create a new HTML page.
    pub fn new(name: String, page_type: PageType) -> Self {
        Self { name, page_type }
    }

    /// Get the name of the page.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the type of the page.
    pub fn page_type(&self) -> &PageType {
        &self.page_type
    }
}

/// A node in the docs directory tree.
#[derive(Debug)]
struct Node {
    /// The name of the node.
    name: String,
    /// The absolute path to the node.
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

    /// Get the path of the node.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the page associated with the node.
    pub fn page(&self) -> Option<Rc<HTMLPage>> {
        self.page.clone()
    }

    /// Gather the node and its children in a Depth First Traversal order.
    pub fn depth_first_traversal(&self) -> Vec<&Node> {
        fn recurse_depth_first<'a>(node: &'a Node, nodes: &mut Vec<&'a Node>) {
            nodes.push(node);

            for child in node.children.values() {
                recurse_depth_first(child, nodes);
            }
        }

        let mut nodes = Vec::new();
        recurse_depth_first(self, &mut nodes);

        nodes
    }
}

/// A tree representing the docs directory.
#[derive(Debug)]
pub struct DocsTree {
    /// The root of the tree.
    ///
    /// `root.path` is the path to the docs directory and is absolute.
    root: Node,
    /// The absolute path to the stylesheet, if it exists.
    stylesheet: Option<PathBuf>,
}

impl DocsTree {
    /// Create a new docs tree.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let abs_path = absolute(root.as_ref()).unwrap();
        let node = Node::new(
            abs_path.file_name().unwrap().to_str().unwrap().to_string(),
            abs_path.clone(),
        );
        Self {
            root: node,
            stylesheet: None,
        }
    }

    /// Create a new docs tree with a stylesheet.
    pub fn new_with_stylesheet(
        root: impl AsRef<Path>,
        stylesheet: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let abs_path = absolute(root.as_ref()).unwrap();
        let in_stylesheet = absolute(stylesheet.as_ref())?;
        let new_stylesheet = abs_path.join("style.css");
        std::fs::copy(in_stylesheet, &new_stylesheet)?;

        let node = Node::new(
            abs_path.file_name().unwrap().to_str().unwrap().to_string(),
            abs_path.clone(),
        );

        Ok(Self {
            root: node,
            stylesheet: Some(new_stylesheet),
        })
    }

    /// Get the root of the tree.
    fn root(&self) -> &Node {
        &self.root
    }

    /// Get the root of the tree as mutable.
    fn root_mut(&mut self) -> &mut Node {
        &mut self.root
    }

    /// Get the absolute path to the stylesheet.
    pub fn stylesheet(&self) -> Option<&PathBuf> {
        self.stylesheet.as_ref()
    }

    /// Get a relative path to the stylesheet.
    pub fn stylesheet_relative_to<P: AsRef<Path>>(&self, path: P) -> Option<PathBuf> {
        if let Some(stylesheet) = self.stylesheet() {
            let path = path.as_ref();
            let stylesheet = diff_paths(stylesheet, path).unwrap();
            Some(stylesheet)
        } else {
            None
        }
    }

    /// Add a page to the tree.
    pub fn add_page<P: Into<PathBuf>>(&mut self, abs_path: P, page: Rc<HTMLPage>) {
        let root = self.root_mut();
        let path = abs_path.into();
        let rel_path = path
            .strip_prefix(&root.path)
            .expect("path should be in the docs directory");

        let mut current_node = root;

        let mut components = rel_path.components().peekable();
        while let Some(component) = components.next() {
            let cur_name = component.as_os_str().to_str().unwrap();
            if current_node.children.contains_key(cur_name) {
                current_node = current_node.children.get_mut(cur_name).unwrap();
            } else {
                let new_node = Node::new(cur_name.to_string(), current_node.path().join(component));
                current_node.children.insert(cur_name.to_string(), new_node);
                current_node = current_node.children.get_mut(cur_name).unwrap();
            }
            if let Some(next_component) = components.peek() {
                if next_component.as_os_str().to_str().unwrap() == "index.html" {
                    break;
                }
            }
        }

        current_node.page = Some(page);
    }

    /// Get the Node associated with a path.
    fn get_node<P: AsRef<Path>>(&self, abs_path: P) -> Option<&Node> {
        let root = self.root();
        let path = abs_path.as_ref();
        let rel_path = path.strip_prefix(&root.path).unwrap();

        let mut current_node = root;

        for component in rel_path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap())
        {
            if current_node.children.contains_key(component) {
                current_node = current_node.children.get(component).unwrap();
            } else {
                return None;
            }
        }

        Some(current_node)
    }

    /// Get the page associated with a path.
    pub fn get_page<P: AsRef<Path>>(&self, abs_path: P) -> Option<Rc<HTMLPage>> {
        self.get_node(abs_path).and_then(|node| node.page())
    }

    /// Render a sidebar component given a path.
    ///
    /// The sidebar will contain a table of contents for the docs directory.
    /// Every node in the tree will be visited in a Depth First Traversal order.
    /// If the node has a page associated with it, a link to the page will be
    /// rendered. If the node does not have a page associated with it, the
    /// name of the node will be rendered. All links will be relative to the
    /// given path.
    pub fn render_sidebar_component<P: AsRef<Path>>(&self, path: P) -> Markup {
        let root = self.root();
        let base = path.as_ref().parent().unwrap();
        let nodes = root.depth_first_traversal();

        html! {
            div class="top-0 left-0 h-full w-1/6 dark:bg-slate-950 dark:text-white" {
                h1 class="text-2xl text-center" { "Sidebar" }
                @for node in nodes {
                    @if let Some(page) = node.page() {
                        @match page.page_type() {
                            PageType::Index(_) => {
                                p { a href=(diff_paths(node.path().join("index.html"), base).unwrap().to_string_lossy()) { (page.name()) } }
                            }
                            _ => {
                                p { a href=(diff_paths(node.path(), base).unwrap().to_string_lossy()) { (page.name()) } }
                            }
                        }
                    } @else {
                        p class="" { (node.name()) }
                    }
                }
            }
        }
    }

    /// Render every page in the tree.
    pub fn render_all(&self) -> anyhow::Result<()> {
        let root = self.root();

        for node in root.depth_first_traversal() {
            if let Some(page) = node.page() {
                self.write_page(page.as_ref(), node.path())?;
            }
        }

        self.write_homepage()?;
        Ok(())
    }

    /// Write the homepage to disk.
    fn write_homepage(&self) -> anyhow::Result<()> {
        let root = self.root();
        let index_path = root.path().join("index.html");

        let sidebar = self.render_sidebar_component(&index_path);
        let content = html! {
            div class="" {
                h3 class="" { "Home" }
                table class="border" {
                    thead class="border" { tr {
                        th class="" { "Page" }
                    }}
                    tbody class="border" {
                        @for node in root.depth_first_traversal() {
                            @if node.page().is_some() {
                                tr class="border" {
                                    td class="border" {
                                        @match node.page().unwrap().page_type() {
                                            PageType::Index(_) => {
                                                a href=(diff_paths(node.path().join("index.html"), root.path()).unwrap().to_str().unwrap()) {(node.name()) }
                                            }
                                            _ => {
                                                a href=(diff_paths(node.path(), root.path()).unwrap().to_str().unwrap()) {(node.name()) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        let html = full_page(
            "Home",
            html! {
                (sidebar)
                (content)
            },
            self.stylesheet_relative_to(root.path()).as_deref(),
        );
        std::fs::write(index_path, html.into_string())?;
        Ok(())
    }

    /// Write a page to disk at the designated path.
    pub fn write_page<P: Into<PathBuf>>(&self, page: &HTMLPage, path: P) -> anyhow::Result<()> {
        let mut path = path.into();

        let content = match page.page_type() {
            PageType::Index(doc) => {
                path = path.join("index.html");
                doc.render()
            }
            PageType::Struct(s) => s.render(),
            PageType::Task(t) => t.render(),
            PageType::Workflow(w) => w.render(),
        };

        let stylesheet =
            self.stylesheet_relative_to(path.parent().expect("path should have a parent"));
        let sidebar = self.render_sidebar_component(&path);

        let html = full_page(
            page.name(),
            html! {
                (sidebar)
                (content)
            },
            stylesheet.as_deref(),
        );
        std::fs::write(path, html.into_string())?;
        Ok(())
    }
}
