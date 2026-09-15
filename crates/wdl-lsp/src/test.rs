//! Utilities for working with `sprocket dev test` test definitions.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use line_index::LineIndex;
use sprocket_test_types::DocumentTests;
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;
use wdl_analysis::Diagnostics;
use wdl_analysis::IncrementalChange;

use crate::handlers::associated_wdl_file_path;

/// The result of a [`SprocketTestYaml`] analysis.
#[derive(Debug)]
pub struct AnalysisResult {
    /// The unique ID of this analysis.
    pub id: Arc<str>,
    /// The analyzed document.
    pub document: Arc<SprocketTestYaml>,
    /// The diagnostics from parsing and analysis.
    pub diagnostics: Diagnostics,
}

/// A cached validation result.
#[derive(Clone, Debug)]
struct ValidationResult {
    /// The ID of this current revision of the document.
    id: Arc<str>,
    /// The ID of the associated WDL document.
    wdl_id: Arc<String>,
    /// The validation diagnostics.
    diagnostics: Diagnostics,
}

/// The parse status of a `sprocket dev test` YAML file.
#[derive(Clone, Debug)]
enum DocumentState {
    /// The document was successfully parsed.
    Parsed {
        /// The parsed test definitions.
        tests: DocumentTests,
        /// Any diagnostics encountered during parsing.
        parse_diagnostics: Diagnostics,
        /// Cached validation result.
        validated: Option<ValidationResult>,
    },
    /// The document failed to parse.
    Failed(Diagnostics),
}

/// A `sprocket dev test` YAML file.
#[derive(Clone, Debug)]
pub struct SprocketTestYaml {
    /// The ID of this current revision of the document.
    pub id: Arc<str>,
    /// The line index of the document.
    pub lines: LineIndex,
    /// The current source of the file.
    pub source: String,
    /// The path to the file on disk.
    pub path: PathBuf,
    /// The parsed document, if any.
    document: Option<DocumentState>,
}

impl SprocketTestYaml {
    /// Gets the associated WDL file for this test YAML, if one exists.
    pub fn associated_wdl(&self) -> Option<Url> {
        associated_wdl_file_path(&self.path).and_then(|wdl_path| Url::from_file_path(wdl_path).ok())
    }

    /// Get the tests from the document, if it was parsed.
    pub fn tests(&self) -> Option<&DocumentTests> {
        match self.document.as_ref() {
            Some(DocumentState::Parsed { tests, .. }) => Some(tests),
            _ => None,
        }
    }

    /// Ensures the document has been parsed.
    fn ensure_parsed(&mut self) {
        if self.document.is_some() {
            return;
        }

        let new_state = match DocumentTests::parse(&self.source) {
            Ok((tests, parse_diagnostics)) => DocumentState::Parsed {
                tests,
                parse_diagnostics,
                validated: None,
            },
            Err(err) => DocumentState::Failed(err),
        };
        self.document = Some(new_state);
    }
}

/// A cache of all known `sprocket dev test` YAML files.
#[derive(Debug, Default)]
pub struct SprocketTestCache {
    /// The documents in the cache.
    documents: Mutex<HashMap<Url, Arc<SprocketTestYaml>>>,
}

impl SprocketTestCache {
    /// Add a Sprocket test YAML file to the cache.
    ///
    /// Returns the document.
    pub async fn open(&self, uri: Url, content: String) -> Result<Arc<SprocketTestYaml>> {
        let Ok(path) = uri.to_file_path() else {
            // `Analyzer` only supports `file://` URIs anyway.
            bail!("unsupported uri: {uri}");
        };

        let entry = self
            .documents
            .lock()
            .await
            .entry(uri)
            .or_insert_with(|| {
                Arc::new(SprocketTestYaml {
                    id: Uuid::new_v4().to_string().into(),
                    lines: LineIndex::new(&content),
                    source: content,
                    path,
                    document: None,
                })
            })
            .clone();

        Ok(entry)
    }

    /// Checks if any cached test YAML document depends on the given WDL
    /// document.
    pub async fn has_dependent_test(&self, uri: &Url) -> bool {
        let docs = self.documents.lock().await;
        docs.values()
            .any(|entry| entry.associated_wdl().as_ref() == Some(uri))
    }

    /// Drop a [`SprocketTestYaml`] from the cache.
    pub async fn close(&self, uri: &Url) -> Option<Arc<SprocketTestYaml>> {
        self.documents.lock().await.remove(uri)
    }

    /// Apply a change to a [`SprocketTestYaml`].
    pub async fn change(&self, uri: Url, change: IncrementalChange) -> Result<(), anyhow::Error> {
        let mut docs = self.documents.lock().await;
        let Entry::Occupied(mut entry) = docs.entry(uri) else {
            return Ok(());
        };

        let test_yaml = Arc::make_mut(entry.get_mut());
        let (new_source, new_lines) = if change.start.is_some() {
            change.apply()?
        } else {
            let mut source = test_yaml.source.clone();
            let mut lines = test_yaml.lines.clone();
            change.apply_to(&mut source, &mut lines)?;
            (source, lines)
        };

        test_yaml.id = Uuid::new_v4().to_string().into();
        test_yaml.source = new_source;
        test_yaml.lines = new_lines;
        test_yaml.document = None;
        Ok(())
    }

    /// Get a [`SprocketTestYaml`] by its URI.
    pub async fn get(&self, uri: &Url) -> Option<Arc<SprocketTestYaml>> {
        let docs = self.documents.lock().await;
        docs.get(uri).cloned()
    }

    /// Returns true if the URI exists in the server's test YAML cache.
    pub async fn contains(&self, uri: &Url) -> bool {
        self.documents.lock().await.contains_key(uri)
    }

    /// Get a [`SprocketTestYaml`] by its URI, ensuring it is parsed beforehand.
    pub async fn ensure_parsed(
        &self,
        uri: Url,
    ) -> Result<Option<Arc<SprocketTestYaml>>, Diagnostics> {
        let mut docs = self.documents.lock().await;
        let Entry::Occupied(mut entry) = docs.entry(uri) else {
            return Ok(None);
        };

        let test_yaml = Arc::make_mut(entry.get_mut());
        test_yaml.ensure_parsed();

        match &test_yaml.document.as_ref().unwrap() {
            DocumentState::Parsed { .. } => Ok(Some(Arc::clone(entry.get()))),
            DocumentState::Failed(diagnostics) => Err(diagnostics.clone()),
        }
    }

    /// Evaluates the document's validation state and returns all associated
    /// diagnostics.
    pub async fn analyze_document(
        &self,
        uri: Url,
        associated_wdl: &wdl_analysis::Document,
    ) -> Result<Option<AnalysisResult>> {
        let mut docs = self.documents.lock().await;
        let Entry::Occupied(mut entry) = docs.entry(uri) else {
            return Ok(None);
        };

        let test_yaml = Arc::make_mut(entry.get_mut());
        test_yaml.ensure_parsed();

        let doc = test_yaml.document.as_mut().unwrap();

        let (diagnostics, id) = match doc {
            DocumentState::Failed(diagnostics) => (diagnostics.clone(), test_yaml.id.clone()),
            DocumentState::Parsed {
                validated:
                    Some(ValidationResult {
                        id,
                        wdl_id,
                        diagnostics,
                    }),
                ..
            } if wdl_id == associated_wdl.id() => (diagnostics.clone(), id.clone()),
            DocumentState::Parsed {
                tests,
                parse_diagnostics,
                validated,
            } => {
                let mut diagnostics = parse_diagnostics.clone();
                if let Err(e) = tests.validate(associated_wdl) {
                    diagnostics.extend(e);
                }
                let id: Arc<str> = format!("{}-{}", test_yaml.id, associated_wdl.id()).into();
                *validated = Some(ValidationResult {
                    id: id.clone(),
                    diagnostics: diagnostics.clone(),
                    wdl_id: associated_wdl.id().clone(),
                });
                (diagnostics, id)
            }
        };

        Ok(Some(AnalysisResult {
            id,
            document: entry.get().clone(),
            diagnostics,
        }))
    }
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

    associated_wdl_file_path(&path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sprocket_test_cache_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wdl_path = temp_dir.path().join("foo.wdl");
        std::fs::write(&wdl_path, "version 1.1\nworkflow test {}\n").unwrap();
        let yaml_path = temp_dir.path().join("foo.yaml");
        std::fs::write(&yaml_path, "test: []\n").unwrap();

        let yaml_uri = Url::from_file_path(&yaml_path).unwrap();
        let wdl_uri = Url::from_file_path(&wdl_path).unwrap();

        let cache = SprocketTestCache::default();
        let entry = cache
            .open(yaml_uri.clone(), "test: []\n".to_string())
            .await
            .unwrap();
        assert_eq!(entry.associated_wdl(), Some(wdl_uri.clone()));

        assert!(cache.has_dependent_test(&wdl_uri).await);

        let other_uri = Url::from_file_path(temp_dir.path().join("other.wdl")).unwrap();
        assert!(!cache.has_dependent_test(&other_uri).await);

        // Closing the test YAML removes the dependency
        assert!(cache.close(&yaml_uri).await.is_some());
        assert!(!cache.has_dependent_test(&wdl_uri).await);
    }
}
