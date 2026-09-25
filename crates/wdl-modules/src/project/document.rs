use thiserror::Error;

use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::manifest::ManifestError;

/// An error parsing or editing a `module.json` document.
#[derive(Debug, Error)]
pub enum ManifestDocumentError {
    /// The `module.json` bytes failed strict manifest validation.
    #[error("invalid module manifest")]
    Manifest(#[from] ManifestError),

    /// The `module.json` bytes failed JSON parsing or serialization.
    #[error("invalid manifest JSON")]
    Json(#[from] serde_json::Error),

    /// The root JSON value was not an object.
    #[error("`module.json` root must be an object")]
    RootNotObject,

    /// The `dependencies` value was present but not an object.
    #[error("`dependencies` in `module.json` must be an object")]
    DependenciesNotObject,
}

/// A lossless `module.json` document paired with its validated manifest view.
///
/// This value stays in memory until a caller writes [`Self::to_bytes`] back to
/// the exact `module.json` path chosen by [`super::ModuleProject`]. Each edit
/// preserves unrelated extension fields and keeps [`Self::manifest`] in step
/// with the document; an edit that cannot be represented fails before either
/// one changes.
///
/// ```rust
/// use std::path::PathBuf;
///
/// use wdl_modules::dependency::DependencySource;
/// use wdl_modules::project::ManifestDocument;
///
/// let mut document = ManifestDocument::parse(
///     br#"{
///   "name": "spellbook",
///   "license": "MIT",
///   "x-extra": { "kept": true }
/// }"#,
/// )?;
///
/// document.insert_dependency(
///     "helpers",
///     &DependencySource::LocalPath {
///         path: PathBuf::from("../helpers"),
///         extra: serde_json::Map::new(),
///     },
/// )?;
///
/// let serialized = String::from_utf8(document.to_bytes()?)?;
/// assert!(serialized.contains(r#""helpers""#));
/// let dependency_name = "helpers".parse()?;
/// assert_eq!(
///     document.manifest().dependencies.get(&dependency_name),
///     Some(&DependencySource::LocalPath {
///         path: PathBuf::from("../helpers"),
///         extra: serde_json::Map::new(),
///     })
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct ManifestDocument {
    /// Raw JSON value preserved for lossless editing of `module.json`.
    value: serde_json::Value,
    /// Strictly validated manifest view derived from `value`.
    manifest: Manifest,
}

impl ManifestDocument {
    /// Parses raw `module.json` bytes without discarding unknown extension
    /// fields.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestDocumentError> {
        let manifest = Manifest::parse(bytes)?;
        let value = serde_json::from_slice(bytes)?;
        Ok(Self { value, manifest })
    }

    /// Returns the validated manifest view for the latest accepted
    /// `module.json` bytes.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Inserts or replaces one dependency entry in `module.json`.
    ///
    /// Unrelated manifest fields and dependency extension fields stay intact.
    /// The name is parsed and the source serialized before the document
    /// changes, so a rejected edit leaves both the document and the validated
    /// view untouched.
    pub fn insert_dependency(
        &mut self,
        name: &str,
        source: &DependencySource,
    ) -> Result<(), ManifestDocumentError> {
        let name = Self::parse_dependency_name(name)?;
        let serialized = serde_json::to_value(source)?;
        // Dependency names treat hyphens and underscores as the same
        // character, so `spell-book` and `spell_book` are one dependency while
        // the JSON object is keyed by whichever spelling the author wrote.
        // Inserting the normalized key alone would leave the author's spelling
        // behind as a second entry for the same dependency.
        let superseded = self
            .matching_dependency_key(&name)
            .filter(|existing| *existing != name.manifest())
            .map(str::to_string);
        let root = self
            .value
            .as_object_mut()
            .ok_or(ManifestDocumentError::RootNotObject)?;
        let dependencies = root
            .entry("dependencies")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or(ManifestDocumentError::DependenciesNotObject)?;
        if let Some(superseded) = superseded {
            dependencies.remove(&superseded);
        }
        dependencies.insert(name.manifest().to_string(), serialized);
        dependencies.sort_keys();
        // `DependencyName` compares by normalized identifier, so inserting
        // over an equivalent key would keep the old spelling in the map while
        // the document carries the new one.
        self.manifest.dependencies.remove(&name);
        self.manifest.dependencies.insert(name, source.clone());
        Ok(())
    }

    /// Removes a dependency from `module.json` when present.
    ///
    /// Returns `true` when a dependency was removed, and leaves the document
    /// untouched when the dependency is absent.
    pub fn remove_dependency(&mut self, name: &str) -> Result<bool, ManifestDocumentError> {
        let name = Self::parse_dependency_name(name)?;
        let Some(existing) = self.matching_dependency_key(&name).map(str::to_string) else {
            return Ok(false);
        };
        let root = self
            .value
            .as_object_mut()
            .ok_or(ManifestDocumentError::RootNotObject)?;
        let Some(dependencies) = root.get_mut("dependencies") else {
            return Ok(false);
        };
        let removed = dependencies
            .as_object_mut()
            .ok_or(ManifestDocumentError::DependenciesNotObject)?
            .remove(&existing)
            .is_some();
        if removed {
            self.manifest.dependencies.remove(&name);
        }
        Ok(removed)
    }

    /// Serializes the current document as pretty-printed `module.json` bytes
    /// with a trailing newline.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ManifestDocumentError> {
        let mut bytes = serde_json::to_vec_pretty(&self.value)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Parses one dependency key with manifest validation rules.
    fn parse_dependency_name(name: &str) -> Result<DependencyName, ManifestDocumentError> {
        name.parse().map_err(|source| {
            ManifestDocumentError::Manifest(ManifestError::InvalidDependencyName {
                name: name.to_string(),
                source,
            })
        })
    }

    /// Finds the stored dependency key that matches `name`.
    fn matching_dependency_key(&self, name: &DependencyName) -> Option<&str> {
        self.manifest
            .dependencies
            .keys()
            .find(|existing| *existing == name)
            .map(DependencyName::manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::DependencySource;

    const MANIFEST: &str = r#"{
      "name": "example",
      "license": "MIT",
      "x-extra": { "keep": true },
      "dependencies": {
        "zeta": { "path": "./zeta" },
        "alpha": { "path": "./alpha", "x-source-extra": 7 }
      }
    }"#;

    #[test]
    fn edits_preserve_extensions_and_sort_dependencies() {
        let mut document = ManifestDocument::parse(MANIFEST.as_bytes()).unwrap();
        let source: DependencySource =
            serde_json::from_str(r#"{"path":"./beta","x-source-extra":"kept"}"#).unwrap();

        document.insert_dependency("beta", &source).unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&document.to_bytes().unwrap()).unwrap();
        assert_eq!(value["x-extra"]["keep"], true);
        assert_eq!(value["dependencies"]["alpha"]["x-source-extra"], 7);
        assert_eq!(value["dependencies"]["beta"]["x-source-extra"], "kept");
        assert_eq!(
            value["dependencies"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["alpha", "beta", "zeta"]
        );
    }

    #[test]
    fn failed_edit_leaves_document_unchanged() {
        let mut document = ManifestDocument::parse(MANIFEST.as_bytes()).unwrap();
        let before = document.to_bytes().unwrap();
        let source: DependencySource =
            serde_json::from_str(r#"{"path":"./bad","x-source-extra":null}"#).unwrap();

        let error = document
            .insert_dependency("not valid", &source)
            .expect_err("an invalid dependency name must fail strict manifest parsing");

        assert!(error.to_string().contains("manifest"));
        assert_eq!(document.to_bytes().unwrap(), before);
    }

    #[test]
    fn insert_dependency_replaces_equivalent_hyphenated_name() {
        let manifest = r#"{
          "name": "example",
          "license": "MIT",
          "dependencies": {
            "spell-book": { "path": "./alpha" }
          }
        }"#;
        let mut document = ManifestDocument::parse(manifest.as_bytes()).unwrap();
        let replacement: DependencySource =
            serde_json::from_str(r#"{"path":"./updated"}"#).unwrap();

        document
            .insert_dependency("spell_book", &replacement)
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&document.to_bytes().unwrap()).unwrap();
        assert_eq!(value["dependencies"]["spell_book"]["path"], "./updated");
        assert!(value["dependencies"]["spell-book"].is_null());
    }

    #[test]
    fn insert_dependency_respells_the_key_in_the_manifest_view() {
        let manifest = r#"{
          "name": "example",
          "license": "MIT",
          "dependencies": {
            "spell-book": { "path": "./alpha" }
          }
        }"#;
        let mut document = ManifestDocument::parse(manifest.as_bytes()).unwrap();
        let replacement: DependencySource =
            serde_json::from_str(r#"{"path":"./updated"}"#).unwrap();

        document
            .insert_dependency("spell_book", &replacement)
            .unwrap();

        let spellings: Vec<_> = document
            .manifest()
            .dependencies
            .keys()
            .map(|name| name.manifest().to_string())
            .collect();
        assert_eq!(
            spellings,
            vec!["spell_book".to_string()],
            "the manifest view must carry the spelling written to the document"
        );
    }

    #[test]
    fn remove_dependency_matches_equivalent_hyphenated_name() {
        let manifest = r#"{
          "name": "example",
          "license": "MIT",
          "dependencies": {
            "spell-book": { "path": "./alpha" }
          }
        }"#;
        let mut document = ManifestDocument::parse(manifest.as_bytes()).unwrap();

        assert!(document.remove_dependency("spell_book").unwrap());

        let value: serde_json::Value =
            serde_json::from_slice(&document.to_bytes().unwrap()).unwrap();
        assert!(value["dependencies"]["spell-book"].is_null());
    }

    #[test]
    fn remove_reports_presence_and_preserves_other_fields() {
        let mut document = ManifestDocument::parse(MANIFEST.as_bytes()).unwrap();
        assert!(document.remove_dependency("alpha").unwrap());
        assert!(!document.remove_dependency("missing").unwrap());

        let value: serde_json::Value =
            serde_json::from_slice(&document.to_bytes().unwrap()).unwrap();
        assert_eq!(value["x-extra"]["keep"], true);
        assert_eq!(value["dependencies"]["zeta"]["path"], "./zeta");
    }

    #[test]
    fn serialization_is_pretty_and_newline_terminated() {
        let document = ManifestDocument::parse(MANIFEST.as_bytes()).unwrap();
        let bytes = document.to_bytes().unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert!(bytes.windows(2).any(|window| window == b"\n "));
    }
}
