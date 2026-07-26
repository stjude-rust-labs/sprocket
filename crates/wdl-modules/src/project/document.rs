use thiserror::Error;

use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::manifest::ManifestError;

/// An error parsing or editing a manifest document.
#[derive(Debug, Error)]
pub enum ManifestDocumentError {
    /// The manifest bytes failed strict manifest validation.
    #[error("invalid module manifest")]
    Manifest(#[source] ManifestError),

    /// The bytes failed JSON parsing or serialization.
    #[error("invalid manifest JSON")]
    Json(#[source] serde_json::Error),

    /// The root JSON value was not an object.
    #[error("`module.json` root must be an object")]
    RootNotObject,

    /// The `dependencies` value was present but not an object.
    #[error("`dependencies` in `module.json` must be an object")]
    DependenciesNotObject,
}

/// A lossless `module.json` document paired with its validated manifest view.
#[derive(Clone, Debug)]
pub struct ManifestDocument {
    value: serde_json::Value,
    manifest: Manifest,
}

impl ManifestDocument {
    /// Parses a manifest document from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestDocumentError> {
        let manifest = Manifest::parse(bytes).map_err(ManifestDocumentError::Manifest)?;
        let value = serde_json::from_slice(bytes).map_err(ManifestDocumentError::Json)?;
        Ok(Self { value, manifest })
    }

    /// Returns the validated manifest view.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Sets or replaces a dependency while preserving unrelated fields.
    ///
    /// The in-memory document changes only after the edited result re-parses
    /// as a valid manifest.
    pub fn set_dependency(
        &mut self,
        name: &str,
        source: &DependencySource,
    ) -> Result<(), ManifestDocumentError> {
        let name = Self::parse_dependency_name(name)?;
        let mut value = self.value.clone();
        let root = value
            .as_object_mut()
            .ok_or(ManifestDocumentError::RootNotObject)?;
        let dependencies = root
            .entry("dependencies")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or(ManifestDocumentError::DependenciesNotObject)?;
        if let Some(existing) = self.matching_dependency_key(&name)
            && existing != name.manifest()
        {
            dependencies.remove(existing);
        }
        dependencies.insert(
            name.into_manifest(),
            serde_json::to_value(source).map_err(ManifestDocumentError::Json)?,
        );
        dependencies.sort_keys();
        self.replace_validated(value)
    }

    /// Removes a dependency when present.
    ///
    /// Returns `true` when a dependency was removed.
    pub fn remove_dependency(&mut self, name: &str) -> Result<bool, ManifestDocumentError> {
        let name = Self::parse_dependency_name(name)?;
        let mut value = self.value.clone();
        let root = value
            .as_object_mut()
            .ok_or(ManifestDocumentError::RootNotObject)?;
        let Some(dependencies) = root.get_mut("dependencies") else {
            return Ok(false);
        };
        let Some(existing) = self.matching_dependency_key(&name) else {
            return Ok(false);
        };
        let removed = dependencies
            .as_object_mut()
            .ok_or(ManifestDocumentError::DependenciesNotObject)?
            .remove(existing)
            .is_some();
        if removed {
            self.replace_validated(value)?;
        }
        Ok(removed)
    }

    /// Serializes the document as pretty-printed JSON with a trailing newline.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ManifestDocumentError> {
        let mut bytes =
            serde_json::to_vec_pretty(&self.value).map_err(ManifestDocumentError::Json)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn replace_validated(&mut self, value: serde_json::Value) -> Result<(), ManifestDocumentError> {
        let mut bytes = serde_json::to_vec_pretty(&value).map_err(ManifestDocumentError::Json)?;
        bytes.push(b'\n');
        let manifest = Manifest::parse(&bytes).map_err(ManifestDocumentError::Manifest)?;
        self.value = value;
        self.manifest = manifest;
        Ok(())
    }

    fn parse_dependency_name(name: &str) -> Result<DependencyName, ManifestDocumentError> {
        name.parse().map_err(|_| {
            ManifestDocumentError::Manifest(ManifestError::InvalidDependencyName(name.to_string()))
        })
    }

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

        document.set_dependency("beta", &source).unwrap();

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
            .set_dependency("not valid", &source)
            .expect_err("an invalid dependency name must fail strict manifest parsing");

        assert!(error.to_string().contains("manifest"));
        assert_eq!(document.to_bytes().unwrap(), before);
    }

    #[test]
    fn set_dependency_replaces_equivalent_hyphenated_name() {
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

        document.set_dependency("spell_book", &replacement).unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&document.to_bytes().unwrap()).unwrap();
        assert_eq!(value["dependencies"]["spell_book"]["path"], "./updated");
        assert!(value["dependencies"]["spell-book"].is_null());
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
