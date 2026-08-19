//! Utilities for working with `sprocket dev test` test definitions.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use async_lsp::lsp_types::TextDocumentContentChangeEvent;
use line_index::LineIndex;
use sprocket_test_types::DocumentTests;
use tokio::sync::Mutex;
use url::Url;
use wdl_analysis::Diagnostics;

/// The parse status of a `sprocket dev test` YAML file.
#[derive(Clone, Debug)]
pub enum Document {
    /// The document was successfully parsed.
    Parsed((DocumentTests, Diagnostics)),
    /// The document failed to parse.
    #[allow(dead_code)]
    Failed(Diagnostics),
}

/// A `sprocket dev test` YAML file.
#[derive(Clone, Debug)]
pub struct SprocketTestYaml {
    /// The line index of the document.
    pub lines: LineIndex,
    /// The current source of the file.
    pub source: String,
    /// The path to the file on disk.
    pub path: PathBuf,
    /// The parsed document, if any.
    pub document: Option<Document>,
}

/// A cache of all known `sprocket dev test` YAML files.
#[derive(Debug, Default)]
pub struct SprocketTestCache {
    /// The documents in the cache.
    documents: Mutex<HashMap<Url, Arc<SprocketTestYaml>>>,
}

impl SprocketTestCache {
    /// Add a Sprocket test YAML file to the cache.
    pub async fn open(&self, uri: Url) -> Result<Arc<SprocketTestYaml>> {
        let Ok(path) = uri.to_file_path() else {
            // `Analyzer` only supports `file://` URIs anyway.
            bail!("unsupported uri: {uri}");
        };

        let content = tokio::fs::read_to_string(&path).await?;
        Ok(self
            .documents
            .lock()
            .await
            .entry(uri)
            .or_insert_with(|| {
                Arc::new(SprocketTestYaml {
                    lines: LineIndex::new(&content),
                    source: content,
                    path,
                    document: None,
                })
            })
            .clone())
    }

    /// Drop a [`SprocketTestYaml`] from the cache.
    pub async fn close(&self, uri: &Url) {
        self.documents.lock().await.remove(uri);
    }

    /// Apply a change to a [`SprocketTestYaml`].
    pub async fn change(
        &self,
        uri: Url,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<(), anyhow::Error> {
        let mut docs = self.documents.lock().await;
        let Entry::Occupied(mut entry) = docs.entry(uri) else {
            return Ok(());
        };

        let test_yaml = Arc::make_mut(entry.get_mut());
        for change in changes {
            match &change.range {
                None => {
                    test_yaml.source = change.text;
                }
                Some(range) => {
                    let start_wide = line_index::WideLineCol {
                        line: range.start.line,
                        col: range.start.character,
                    };
                    let end_wide = line_index::WideLineCol {
                        line: range.end.line,
                        col: range.end.character,
                    };

                    let start_offset = test_yaml
                        .lines
                        .to_utf8(line_index::WideEncoding::Utf16, start_wide)
                        .and_then(|lc| test_yaml.lines.offset(lc))
                        .map(usize::from)
                        .unwrap_or(0);

                    let end_offset = test_yaml
                        .lines
                        .to_utf8(line_index::WideEncoding::Utf16, end_wide)
                        .and_then(|lc| test_yaml.lines.offset(lc))
                        .map(usize::from)
                        .unwrap_or(test_yaml.source.len());

                    if start_offset <= end_offset && end_offset <= test_yaml.source.len() {
                        test_yaml
                            .source
                            .replace_range(start_offset..end_offset, &change.text);
                    }
                }
            }
        }

        test_yaml.lines = LineIndex::new(&test_yaml.source);
        test_yaml.document = None;
        Ok(())
    }

    /// Get a [`SprocketTestYaml`] by its URI, ensuring it is parsed beforehand.
    pub async fn ensure_parsed(&self, uri: Url) -> Result<Option<Arc<SprocketTestYaml>>> {
        let mut docs = self.documents.lock().await;
        if let Entry::Occupied(mut entry) = docs.entry(uri) {
            if entry.get().document.is_none() {
                let test_yaml = Arc::make_mut(entry.get_mut());
                test_yaml.document = match DocumentTests::parse(&test_yaml.source) {
                    Ok(result) => Some(Document::Parsed(result)),
                    Err(err) => Some(Document::Failed(err)),
                };
            }

            return Ok(Some(Arc::clone(entry.get())));
        }

        Ok(None)
    }
}

/// Check if a directory is a valid Sprocket test directory.
///
/// A Sprocket test directory is valid if:
/// 1. Its name is `test`.
/// 2. Its parent contains at least one `.wdl` file.
fn is_sprocket_test_dir(path: &std::path::Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    if path.file_name().and_then(|s| s.to_str()) != Some("test") {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("wdl")
            {
                return true;
            }
        }
    }
    false
}

/// Check if a file is a valid Sprocket test definition file.
///
/// A Sprocket test definition file is valid if:
/// 1. Its extension is `yaml` or `yml`.
/// 2. Either its parent is a valid Sprocket test directory, OR there is an
///    accompanying `.wdl` file of the same name in the same directory.
pub fn is_sprocket_test_file(uri: &Url) -> bool {
    let Some(path) = uri.to_file_path().ok() else {
        return false;
    };

    let Some(ext) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
    if ext != "yaml" && ext != "yml" {
        return false;
    }

    let Some(parent) = path.parent() else {
        return false;
    };

    if is_sprocket_test_dir(parent) {
        return true;
    }

    let Some(base_name) = path.file_stem() else {
        return false;
    };
    let wdl_sibling = parent.join(base_name).with_extension("wdl");
    if wdl_sibling.is_file() {
        return true;
    }

    false
}
