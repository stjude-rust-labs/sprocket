//! Implementation of the inputs prefix trie.
//!
//! The inputs prefix trie is used to map input host paths to guest paths for
//! task evaluation.

use std::collections::HashMap;
use std::path::Component;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use url::Url;

use crate::ContentKind;
use crate::EvaluationPath;
use crate::GuestPath;
use crate::backend::Input;
use crate::eval::ROOT_NAME;

/// Represents a node in an input trie.
#[derive(Debug)]
struct InputTrieNode {
    /// The children of this node.
    children: HashMap<String, Self>,
    /// The identifier of the node in the trie.
    ///
    /// A node's identifier is used when formatting guest paths of children.
    id: usize,
    /// The index into the trie's `inputs` collection.
    ///
    /// This is `Some` only for terminal nodes in the trie.
    index: Option<usize>,
}

impl InputTrieNode {
    /// Constructs a new input trie node with the given id.
    fn new(id: usize) -> Self {
        Self {
            children: Default::default(),
            id,
            index: None,
        }
    }
}

/// Represents a prefix trie based on input paths.
///
/// This is used to determine guest paths for inputs.
///
/// From the root to a terminal node represents a unique input.
#[derive(Debug)]
pub struct InputTrie {
    /// The guest inputs root directory.
    ///
    /// This is `None` for backends that don't use containers.
    guest_inputs_dir: Option<&'static str>,
    /// The URL path children of the tree.
    ///
    /// The key in the map is the scheme of each URL.
    urls: HashMap<String, InputTrieNode>,
    /// The local path children of the tree.
    ///
    /// The key in the map is the first component of each path.
    paths: HashMap<String, InputTrieNode>,
    /// The inputs in the trie.
    inputs: Vec<Input>,
    /// The next node identifier.
    next_id: usize,
}

impl InputTrie {
    /// Constructs a new inputs trie without a guest inputs directory.
    ///
    /// Terminal nodes in the trie will not be mapped to guest paths.
    pub fn new() -> Self {
        Self {
            guest_inputs_dir: None,
            urls: Default::default(),
            paths: Default::default(),
            inputs: Default::default(),
            // The first id starts at 1 as 0 is considered the "virtual root" of the trie
            next_id: 1,
        }
    }

    /// Constructs a new inputs trie with a guest inputs directory.
    ///
    /// Inputs with a host path will be mapped to a guest path relative to the
    /// guest inputs directory.
    ///
    /// Note: a guest inputs directory is always a Unix-style path.
    ///
    /// # Panics
    ///
    /// Panics if the guest inputs directory does not end with a slash.
    pub fn new_with_guest_dir(guest_inputs_dir: &'static str) -> Self {
        assert!(guest_inputs_dir.ends_with('/'));

        let mut trie = Self::new();
        trie.guest_inputs_dir = Some(guest_inputs_dir);
        trie
    }

    /// Inserts a new input into the trie.
    ///
    /// The path is either a local or remote input path.
    ///
    /// Relative paths are made absolute via the provided base path.
    ///
    /// If an input was added, returns `Ok(Some(index))` where `index` is the
    /// index of the input in the trie.
    ///
    /// Returns `Ok(None)` if the provided path was already a guest input path.
    ///
    /// Returns an error for an invalid input path.
    pub fn insert(
        &mut self,
        kind: ContentKind,
        path: &str,
        base_dir: &EvaluationPath,
    ) -> Result<Option<usize>> {
        let path = base_dir.join(path)?;
        if let Some(p) = path.as_local() {
            // Check to see if the path being inserted is already a guest path
            if let Some(dir) = self.guest_inputs_dir
                && p.starts_with(dir)
            {
                return Ok(None);
            }

            self.insert_path(kind, path.unwrap_local()).map(Some)
        } else {
            self.insert_url(kind, path.unwrap_remote()).map(Some)
        }
    }

    /// Gets the inputs of the trie as a slice.
    pub fn as_slice(&self) -> &[Input] {
        &self.inputs
    }

    /// Gets the inputs of the trie as a mutable slice.
    pub fn as_slice_mut(&mut self) -> &mut [Input] {
        &mut self.inputs
    }

    /// Inserts an input with a local path into the trie.
    fn insert_path(&mut self, kind: ContentKind, path: PathBuf) -> Result<usize> {
        let mut components = path.components();

        let component = components
            .next()
            .context("input path cannot be empty")?
            .as_os_str()
            .to_str()
            .with_context(|| format!("input path `{path}` is not UTF-8", path = path.display()))?;

        let mut parent_id = 0;
        let mut node = self.paths.entry(component.to_string()).or_insert_with(|| {
            let node = InputTrieNode::new(self.next_id);
            self.next_id += 1;
            node
        });

        let mut last_component = None;
        for component in components {
            match component {
                Component::CurDir | Component::ParentDir => {
                    bail!(
                        "input path `{path}` may not contain `.` or `..`",
                        path = path.display()
                    );
                }
                _ => {}
            }

            let component = component.as_os_str().to_str().with_context(|| {
                format!("input path `{path}` is not UTF-8", path = path.display())
            })?;

            parent_id = node.id;

            node = node
                .children
                .entry(component.to_string())
                .or_insert_with(|| {
                    let node = InputTrieNode::new(self.next_id);
                    self.next_id += 1;
                    node
                });

            last_component = Some(component);
        }

        // Check to see if the input already exists in the trie
        if let Some(index) = node.index {
            return Ok(index);
        }

        let guest_path = self.guest_inputs_dir.map(|d| {
            GuestPath::new(format!(
                "{d}{parent_id}/{last}",
                // On Windows, `last_component` might be `Some` despite being a root due to the
                // prefix (e.g. `C:`); instead check if the path has a parent
                last = if path.parent().is_none() {
                    ROOT_NAME
                } else {
                    last_component.unwrap_or(ROOT_NAME)
                }
            ))
        });

        let index = self.inputs.len();
        self.inputs.push(Input::new(
            kind,
            EvaluationPath::from_local_path(path),
            guest_path,
        ));
        node.index = Some(index);
        Ok(index)
    }

    /// Inserts an input with a URL into the trie.
    fn insert_url(&mut self, kind: ContentKind, url: Url) -> Result<usize> {
        // Insert for scheme
        let mut node = self
            .urls
            .entry(url.scheme().to_string())
            .or_insert_with(|| {
                let node = InputTrieNode::new(self.next_id);
                self.next_id += 1;
                node
            });

        // Insert the authority; if the URL's path is empty, we'll
        let mut parent_id = node.id;
        node = node
            .children
            .entry(url.authority().to_string())
            .or_insert_with(|| {
                let node = InputTrieNode::new(self.next_id);
                self.next_id += 1;
                node
            });

        // Insert the path segments
        let mut last_segment = None;
        if let Some(segments) = url.path_segments() {
            for segment in segments {
                parent_id = node.id;
                node = node.children.entry(segment.to_string()).or_insert_with(|| {
                    let node = InputTrieNode::new(self.next_id);
                    self.next_id += 1;
                    node
                });

                if !segment.is_empty() {
                    last_segment = Some(segment);
                }
            }
        }

        // Check to see if the input already exists in the trie
        if let Some(index) = node.index {
            return Ok(index);
        }

        let guest_path = self.guest_inputs_dir.as_ref().map(|d| {
            GuestPath::new(format!(
                "{d}{parent_id}/{last}",
                last = last_segment.unwrap_or(ROOT_NAME)
            ))
        });

        let index = self.inputs.len();
        self.inputs
            .push(Input::new(kind, EvaluationPath::try_from(url)?, guest_path));
        node.index = Some(index);
        Ok(index)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn empty_trie() {
        let empty = InputTrie::new();
        assert!(empty.as_slice().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unmapped_inputs_unix() {
        let mut trie = InputTrie::new();
        let base_dir: EvaluationPath = "/base".parse().unwrap();
        trie.insert(ContentKind::File, "/foo/bar/baz", &base_dir)
            .unwrap();
        assert_eq!(trie.as_slice().len(), 1);
        assert_eq!(trie.as_slice()[0].path().to_string(), "/foo/bar/baz");
        assert!(trie.as_slice()[0].guest_path().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn unmapped_inputs_windows() {
        let mut trie = InputTrie::new();
        let base_dir: EvaluationPath = "C:\\base".parse().unwrap();
        trie.insert(ContentKind::File, "C:\\foo\\bar\\baz", &base_dir)
            .unwrap();
        assert_eq!(trie.as_slice().len(), 1);
        assert_eq!(trie.as_slice()[0].path().to_string(), "C:\\foo\\bar\\baz");
        assert!(trie.as_slice()[0].guest_path().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn non_empty_trie_unix() {
        let mut trie = InputTrie::new_with_guest_dir("/inputs/");
        let base_dir: EvaluationPath = "/base".parse().unwrap();
        trie.insert(ContentKind::Directory, "/", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/foo/bar/foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/foo/bar/bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/foo/baz/foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/foo/baz/bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/bar/foo/foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "/bar/foo/bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::Directory, "/baz", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "https://example.com/", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/bar/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/bar/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/baz/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/baz/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/bar/foo/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/bar/foo/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(ContentKind::File, "https://foo.com/bar", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "foo.txt", &base_dir)
            .unwrap()
            .unwrap();

        // The important part of the guest paths are:
        // 1) The guest file name should be the same (or `.root` if the path is
        //    considered to be root)
        // 2) Paths with the same parent should have the same guest parent
        let paths: Vec<_> = trie
            .as_slice()
            .iter()
            .map(|i| {
                (
                    i.path().to_string(),
                    i.guest_path().expect("should have guest path").as_str(),
                )
            })
            .collect();

        assert_eq!(
            paths,
            [
                ("/".to_string(), "/inputs/0/.root"),
                ("/foo/bar/foo.txt".to_string(), "/inputs/3/foo.txt"),
                ("/foo/bar/bar.txt".to_string(), "/inputs/3/bar.txt"),
                ("/foo/baz/foo.txt".to_string(), "/inputs/6/foo.txt"),
                ("/foo/baz/bar.txt".to_string(), "/inputs/6/bar.txt"),
                ("/bar/foo/foo.txt".to_string(), "/inputs/10/foo.txt"),
                ("/bar/foo/bar.txt".to_string(), "/inputs/10/bar.txt"),
                ("/baz".to_string(), "/inputs/1/baz"),
                ("https://example.com/".to_string(), "/inputs/15/.root"),
                (
                    "https://example.com/foo/bar/foo.txt".to_string(),
                    "/inputs/18/foo.txt"
                ),
                (
                    "https://example.com/foo/bar/bar.txt".to_string(),
                    "/inputs/18/bar.txt"
                ),
                (
                    "https://example.com/foo/baz/foo.txt".to_string(),
                    "/inputs/21/foo.txt"
                ),
                (
                    "https://example.com/foo/baz/bar.txt".to_string(),
                    "/inputs/21/bar.txt"
                ),
                (
                    "https://example.com/bar/foo/foo.txt".to_string(),
                    "/inputs/25/foo.txt"
                ),
                (
                    "https://example.com/bar/foo/bar.txt".to_string(),
                    "/inputs/25/bar.txt"
                ),
                ("https://foo.com/bar".to_string(), "/inputs/28/bar"),
                ("/base/foo.txt".to_string(), "/inputs/30/foo.txt"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn non_empty_trie_windows() {
        let mut trie = InputTrie::new_with_guest_dir("/inputs/");
        let base_dir: EvaluationPath = "C:\\base".parse().unwrap();
        trie.insert(ContentKind::Directory, "C:\\", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\foo\\bar\\foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\foo\\bar\\bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\foo\\baz\\foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\foo\\baz\\bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\bar\\foo\\foo.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "C:\\bar\\foo\\bar.txt", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::Directory, "C:\\baz", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "https://example.com/", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/bar/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/bar/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/baz/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/foo/baz/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/bar/foo/foo.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(
            ContentKind::File,
            "https://example.com/bar/foo/bar.txt",
            &base_dir,
        )
        .unwrap()
        .unwrap();
        trie.insert(ContentKind::File, "https://foo.com/bar", &base_dir)
            .unwrap()
            .unwrap();
        trie.insert(ContentKind::File, "foo.txt", &base_dir)
            .unwrap()
            .unwrap();

        // The important part of the guest paths are:
        // 1) The guest file name should be the same (or `.root` if the path is
        //    considered to be root)
        // 2) Paths with the same parent should have the same guest parent
        let paths: Vec<_> = trie
            .as_slice()
            .iter()
            .map(|i| {
                (
                    i.path().to_string(),
                    i.guest_path().expect("should have guest path").as_str(),
                )
            })
            .collect();

        assert_eq!(
            paths,
            [
                ("C:\\".to_string(), "/inputs/1/.root"),
                ("C:\\foo\\bar\\foo.txt".to_string(), "/inputs/4/foo.txt"),
                ("C:\\foo\\bar\\bar.txt".to_string(), "/inputs/4/bar.txt"),
                ("C:\\foo\\baz\\foo.txt".to_string(), "/inputs/7/foo.txt"),
                ("C:\\foo\\baz\\bar.txt".to_string(), "/inputs/7/bar.txt"),
                ("C:\\bar\\foo\\foo.txt".to_string(), "/inputs/11/foo.txt"),
                ("C:\\bar\\foo\\bar.txt".to_string(), "/inputs/11/bar.txt"),
                ("C:\\baz".to_string(), "/inputs/2/baz"),
                ("https://example.com/".to_string(), "/inputs/16/.root"),
                (
                    "https://example.com/foo/bar/foo.txt".to_string(),
                    "/inputs/19/foo.txt"
                ),
                (
                    "https://example.com/foo/bar/bar.txt".to_string(),
                    "/inputs/19/bar.txt"
                ),
                (
                    "https://example.com/foo/baz/foo.txt".to_string(),
                    "/inputs/22/foo.txt"
                ),
                (
                    "https://example.com/foo/baz/bar.txt".to_string(),
                    "/inputs/22/bar.txt"
                ),
                (
                    "https://example.com/bar/foo/foo.txt".to_string(),
                    "/inputs/26/foo.txt"
                ),
                (
                    "https://example.com/bar/foo/bar.txt".to_string(),
                    "/inputs/26/bar.txt"
                ),
                ("https://foo.com/bar".to_string(), "/inputs/29/bar"),
                ("C:\\base\\foo.txt".to_string(), "/inputs/31/foo.txt"),
            ]
        );
    }
}
