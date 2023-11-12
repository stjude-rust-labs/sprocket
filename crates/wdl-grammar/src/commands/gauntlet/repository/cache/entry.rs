//! A single file entry within a [`Cache`](super::Cache).

use octocrab::etag::EntityTag;

/// A single file entry within a [`Cache`](super::Cache).
#[derive(Debug)]
pub struct Entry {
    /// The cached contents of the file.
    contents: String,

    /// The cached `etag` HTTP response header for the remote file.
    etag: EntityTag,
}

impl Entry {
    /// Creates a new [`Entry`].
    pub fn new(etag: EntityTag, contents: String) -> Self {
        Self { contents, etag }
    }

    /// Gets the cached file contents of the [`Entry`] by reference.
    pub fn contents(&self) -> &str {
        self.contents.as_str()
    }

    /// Gets the cached [`etag` HTTP response header](EntityTag) of the
    /// [`Entry`] by reference.
    pub fn etag(&self) -> &EntityTag {
        &self.etag
    }
}
