//! `module.json` manifest parsing and validation.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use crate::DEFAULT_ENTRYPOINT_FILENAME;
use crate::DependencyName;
use crate::DependencySource;
use crate::DependencySourceError;
use crate::LicenseError;
use crate::LicenseExpression;
use crate::RelativePath;
use crate::RelativePathError;

/// An error parsing a [`Manifest`].
///
/// Parsing is strict per the spec; trailing commas, comments, BOM, and
/// duplicate object keys at any nesting depth are all rejected.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The bytes did not parse as JSON.
    #[error("invalid `module.json` JSON")]
    InvalidJson(#[from] serde_json::Error),

    /// The `name` field is empty.
    #[error("`name` cannot be empty")]
    EmptyName,

    /// The `entrypoint` path failed relative-path validation.
    #[error("`entrypoint` is invalid")]
    InvalidEntrypoint(#[source] RelativePathError),

    /// The `readme` path failed relative-path validation.
    #[error("`readme` is invalid")]
    InvalidReadme(#[source] RelativePathError),

    /// The `readme` field was set to the literal `true`. The schema only
    /// accepts a string, the literal `false`, or absence; `true` is
    /// rejected with a dedicated message because it is a common authoring
    /// mistake (mirroring `false` to mean "enable the default readme").
    #[error("`readme` cannot be set to `true`; omit the field to use the default `README.md`")]
    ReadmeTrue,

    /// A dependency key is not a valid WDL identifier.
    #[error("`dependencies` key `{0}` is not a valid WDL identifier")]
    InvalidDependencyName(String),

    /// A dependency declaration is invalid.
    #[error(transparent)]
    DependencySource(#[from] DependencySourceError),

    /// The `license` field is not a valid SPDX expression.
    #[error(transparent)]
    License(#[from] LicenseError),
}

/// The `readme` field of a manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readme {
    /// The `readme` field was omitted; engines look for `README.md`.
    Default,
    /// The `readme` field is a relative path to a markdown file.
    Path(RelativePath),
    /// The `readme` field is the literal `false`; no readme is associated
    /// with the module.
    Disabled,
}

/// A `tools[]` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    /// The tool name.
    pub name: String,
    /// The tool version.
    pub version: String,
    /// The tool's SPDX license identifier.
    pub license: LicenseExpression,
    /// URL for the tool's homepage or repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<Url>,
    /// DOI for the tool's publication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// `bio.tools` registry identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biotools: Option<String>,
    /// Unknown fields, preserved for round-trip and inspection by
    /// downstream linters.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A parsed `module.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The module's display name. Not used for dependency resolution.
    pub name: String,
    /// The module version.
    pub version: Version,
    /// The module's SPDX license expression.
    pub license: LicenseExpression,
    /// The author descriptions.
    pub authors: Vec<String>,
    /// A brief description of the module.
    pub description: Option<String>,
    /// The canonical Git URL for the module's source repository.
    pub repository: Option<Url>,
    /// A URL for the module's documentation or landing page.
    pub homepage: Option<Url>,
    /// The path to the module's entrypoint WDL file, relative to the
    /// module root. Defaults to [`DEFAULT_ENTRYPOINT_FILENAME`] if absent.
    pub entrypoint: Option<RelativePath>,
    /// The module's readme.
    pub readme: Readme,
    /// Gitignore-style glob patterns identifying files within the module
    /// that consumers may not reach via symbolic import. Has no effect on
    /// content hashing, signing, validation, or quoted within-module
    /// imports.
    pub exclude: Vec<String>,
    /// The upstream tools wrapped by the module.
    pub tools: Vec<Tool>,
    /// The module's dependencies, keyed by consumer-chosen name.
    pub dependencies: BTreeMap<DependencyName, DependencySource>,
    /// Unknown top-level fields. The spec requires implementations to
    /// ignore unrecognized fields; capturing them here lets downstream
    /// linters surface typos without a re-parse.
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Manifest {
    /// Parses a `module.json` from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let raw: ManifestFields = crate::strict_json::from_slice(bytes)?;
        raw.try_into()
    }

    /// Returns the entrypoint filename, falling back to
    /// [`DEFAULT_ENTRYPOINT_FILENAME`] when
    /// [`entrypoint`](Self::entrypoint) is unset.
    pub fn entrypoint_filename(&self) -> &Path {
        self.entrypoint
            .as_ref()
            .map(RelativePath::as_path)
            .unwrap_or(Path::new(DEFAULT_ENTRYPOINT_FILENAME))
    }
}

/// Flat field set of a manifest, deserialized straight from JSON before
/// post-deserialization validation projects it onto [`Manifest`].
#[derive(Debug, Deserialize)]
struct ManifestFields {
    /// The module's display name.
    name: String,
    /// The module version.
    version: Version,
    /// The module's SPDX license.
    license: String,
    /// The author descriptions.
    #[serde(default)]
    authors: Vec<String>,
    /// A brief description of the module.
    #[serde(default)]
    description: Option<String>,
    /// The canonical Git URL for the module's source repository.
    #[serde(default)]
    repository: Option<Url>,
    /// A URL for the module's documentation or landing page.
    #[serde(default)]
    homepage: Option<Url>,
    /// The path to the module's entrypoint WDL file.
    #[serde(default)]
    entrypoint: Option<PathBuf>,
    /// The `readme` field, accepting `null`, a string, or `false`.
    #[serde(default)]
    readme: ReadmeFields,
    /// Gitignore-style glob patterns identifying files outside the public
    /// import surface.
    #[serde(default)]
    exclude: Vec<String>,
    /// The upstream tools.
    #[serde(default)]
    tools: Vec<Tool>,
    /// The module's dependencies.
    #[serde(default)]
    dependencies: BTreeMap<String, DependencySource>,
    /// Unknown top-level fields.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// The `readme` field's JSON shape; one of a string, `false`, or absent.
/// The values `null` and `true` are rejected at parse time.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum ReadmeFields {
    /// A relative path to a readme file.
    Path(PathBuf),
    /// The literal `false`, disabling the readme.
    Bool(bool),
    /// The field was absent.
    #[default]
    Default,
}

impl TryFrom<ManifestFields> for Manifest {
    type Error = ManifestError;

    fn try_from(fields: ManifestFields) -> Result<Self, Self::Error> {
        if fields.name.is_empty() {
            return Err(ManifestError::EmptyName);
        }

        let license = LicenseExpression::try_from(fields.license)?;

        let entrypoint = fields
            .entrypoint
            .map(RelativePath::try_from)
            .transpose()
            .map_err(ManifestError::InvalidEntrypoint)?;

        let readme = match fields.readme {
            ReadmeFields::Default => Readme::Default,
            ReadmeFields::Path(p) => {
                Readme::Path(RelativePath::try_from(p).map_err(ManifestError::InvalidReadme)?)
            }
            ReadmeFields::Bool(false) => Readme::Disabled,
            ReadmeFields::Bool(true) => return Err(ManifestError::ReadmeTrue),
        };

        let mut deps = BTreeMap::new();
        for (key, value) in fields.dependencies {
            let name = DependencyName::try_from(key.clone())
                .map_err(|_| ManifestError::InvalidDependencyName(key))?;
            deps.insert(name, value);
        }

        Ok(Self {
            name: fields.name,
            version: fields.version,
            license,
            authors: fields.authors,
            description: fields.description,
            repository: fields.repository,
            homepage: fields.homepage,
            entrypoint,
            readme,
            exclude: fields.exclude,
            tools: fields.tools,
            dependencies: deps,
            extra: fields.extra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Manifest, ManifestError> {
        Manifest::parse(s.as_bytes())
    }

    #[test]
    fn parses_minimal_manifest() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.2.0",
                "license": "MIT"
            }"#,
        )
        .unwrap();
        assert_eq!(m.name, "spellbook");
        assert_eq!(m.version.to_string(), "1.2.0");
        assert_eq!(m.license.as_str(), "MIT");
        assert!(m.authors.is_empty());
        assert!(matches!(m.readme, Readme::Default));
        assert_eq!(m.entrypoint_filename(), Path::new("index.wdl"));
    }

    #[test]
    fn parses_full_example() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.2.0",
                "license": "MIT OR Apache-2.0",
                "authors": ["Jane Doe <jane.doe@example.com>"],
                "description": "spellbook wrapper",
                "repository": "https://github.com/openwdl/spellbook",
                "homepage": "https://example.com",
                "tools": [
                    {
                        "name": "spellcheck",
                        "version": "2.0.1",
                        "license": "MIT",
                        "homepage": "https://example.com/sc"
                    }
                ],
                "dependencies": {
                    "common": {
                        "git": "https://github.com/openwdl/common",
                        "version": "^1.0.0"
                    },
                    "local_utils": { "path": "../utils" }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(m.tools.len(), 1);
        assert_eq!(m.dependencies.len(), 2);
    }

    #[test]
    fn parses_readme_disabled() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "readme": false
            }"#,
        )
        .unwrap();
        assert!(matches!(m.readme, Readme::Disabled));
    }

    #[test]
    fn parses_readme_path() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "readme": "docs/README.md"
            }"#,
        )
        .unwrap();
        assert!(matches!(m.readme, Readme::Path(_)));
    }

    #[test]
    fn captures_unknown_top_level_fields() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "extra_field": 42,
                "metadata": {"key": "value"}
            }"#,
        )
        .unwrap();
        assert!(m.extra.contains_key("extra_field"));
        assert!(m.extra.contains_key("metadata"));
    }

    #[test]
    fn rejects_empty_name() {
        let err = parse(r#"{ "name": "", "version": "1.0.0", "license": "MIT" }"#).unwrap_err();
        assert!(matches!(err, ManifestError::EmptyName));
    }

    #[test]
    fn rejects_invalid_license() {
        let err = parse(r#"{ "name": "spellbook", "version": "1.0.0", "license": "MIT-2.0" }"#)
            .unwrap_err();
        assert!(matches!(err, ManifestError::License(_)));
    }

    #[test]
    fn rejects_absolute_entrypoint() {
        let err = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "entrypoint": "/abs/path.wdl"
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, ManifestError::InvalidEntrypoint(_)));
    }

    #[test]
    fn rejects_readme_true() {
        let err = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "readme": true
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, ManifestError::ReadmeTrue));
    }

    #[test]
    fn parses_exclude_field() {
        let m = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "exclude": ["internal/**", "scratch/*.wdl"]
            }"#,
        )
        .unwrap();
        assert_eq!(m.exclude, vec!["internal/**", "scratch/*.wdl"]);
    }

    #[test]
    fn rejects_parent_dir_in_readme() {
        let err = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "readme": "../escape.md"
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, ManifestError::InvalidReadme(_)));
    }

    fn assert_duplicate_key_error(err: ManifestError) {
        let inner = match err {
            ManifestError::InvalidJson(e) => e.to_string(),
            other => panic!("expected `InvalidJson` variant; got {other:?}"),
        };
        assert!(
            inner.contains("duplicate object key"),
            "wrong inner message: {inner}"
        );
    }

    #[test]
    fn rejects_duplicate_top_level_keys() {
        assert_duplicate_key_error(
            parse(
                r#"{
                    "name": "spellbook",
                    "name": "duplicate",
                    "version": "1.0.0",
                    "license": "MIT"
                }"#,
            )
            .unwrap_err(),
        );
    }

    #[test]
    fn rejects_duplicate_nested_keys() {
        assert_duplicate_key_error(
            parse(
                r#"{
                    "name": "spellbook",
                    "version": "1.0.0",
                    "license": "MIT",
                    "tools": [
                        {"name": "x", "name": "y", "version": "1", "license": "MIT"}
                    ]
                }"#,
            )
            .unwrap_err(),
        );
    }

    #[test]
    fn rejects_non_identifier_dep_key() {
        let err = parse(
            r#"{
                "name": "spellbook",
                "version": "1.0.0",
                "license": "MIT",
                "dependencies": { "bad-name": {"path": "../local"} }
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, ManifestError::InvalidDependencyName(_)));
    }
}
