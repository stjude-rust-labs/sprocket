//! Input files parsed in from the command line.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::path::absolute;

use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use thiserror::Error;
use wdl_engine::JsonMap;
use wdl_engine::path::EvaluationPath;

use crate::Inputs;

/// An error related to a input file.
#[derive(Error, Debug)]
pub enum Error {
    /// An input file specified by local path was not found.
    #[error("input file `{0}` was not found")]
    NotFound(PathBuf),

    /// An error occurred parsing an input file path.
    #[error("input file path `{path}` is invalid: {error:#}")]
    Path {
        /// The path to the inputs file.
        path: String,
        /// The error parsing the path.
        error: anyhow::Error,
    },

    /// An error occurring in [`serde_json`].
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// An input file cannot be read from a directory.
    #[error("an input file cannot be read from directory `{0}`")]
    InvalidDir(PathBuf),

    /// The input file did not contain a map at the root.
    #[error("input file `{path}` did not contain a map from strings to values at the root", path = .0.display())]
    NonMapRoot(EvaluationPath),

    /// Failed to read the contents of an input file due to I/O error.
    #[error("failed to read input file `{path}`: {error:#}", path = .path.display())]
    Io {
        /// The path to the inputs file.
        path: EvaluationPath,
        /// The I/O error that occurred.
        error: std::io::Error,
    },

    /// Failed to read the contents of an input file due to reqwest error.
    #[error("failed to read input file `{path}`: {error:#}", path = .path.display())]
    Reqwest {
        /// The path to the inputs file.
        path: EvaluationPath,
        /// The reqwest error that occurred.
        error: reqwest::Error,
    },

    /// Neither JSON nor YAML could be parsed from the provided path.
    #[error(
        "unsupported input file `{path}`: the supported formats are JSON (`.json`) or YAML (`.yaml` and `.yml`)", path = .0.display()
    )]
    UnsupportedFileExt(EvaluationPath),

    /// An error occurring in [`serde_yaml_ng`].
    #[error(transparent)]
    Yaml(#[from] serde_yaml_ng::Error),
}

/// A [`Result`](std::result::Result) with an [`Error`](enum@self::Error).
pub type Result<T> = std::result::Result<T, Error>;

/// An input file containing WDL values.
pub struct InputFile;

impl InputFile {
    /// Reads an input file.
    ///
    /// The file is attempted to be parsed based on its extension.
    ///
    /// - If the input file is successfully parsed, it's returned wrapped in
    ///   [`Ok`].
    /// - If a deserialization error is encountered while parsing the JSON/YAML
    ///   file, an [`Error::Json`]/[`Error::Yaml`] is returned respectively.
    /// - If no recognized extension is found, an [`Error::UnsupportedFileExt`]
    ///   is returned.
    pub async fn read(path: &EvaluationPath) -> Result<Inputs> {
        fn map_to_inputs(map: JsonMap, origin: &EvaluationPath) -> Inputs {
            let mut inputs = Inputs::default();

            for (key, value) in map.iter() {
                inputs.insert(key.to_owned(), (origin.clone(), value.clone()));
            }

            inputs
        }

        if let Some(path) = path.as_local()
            && path.is_dir()
        {
            return Err(Error::InvalidDir(path.to_path_buf()));
        }

        /// Supported inputs file formats
        enum Format {
            /// The inputs file is a JSON file
            Json,
            /// The inputs file is a YAML file
            Yaml,
        }

        let (content, origin, format) = match path {
            EvaluationPath::Local(local) => {
                let format = match local.extension().and_then(OsStr::to_str) {
                    Some("json") => Format::Json,
                    Some("yml") | Some("yaml") => Format::Yaml,
                    _ => return Err(Error::UnsupportedFileExt(path.clone())),
                };

                let origin = absolute(local).map_err(|e| Error::Io {
                    path: path.clone(),
                    error: e,
                })?;
                let origin = if let Some(parent) = origin.parent() {
                    parent.to_path_buf()
                } else {
                    origin
                };

                // Read the contents from the local file
                let contents = std::fs::read_to_string(local).map_err(|e| Error::Io {
                    path: path.clone(),
                    error: e,
                })?;

                (contents, EvaluationPath::Local(origin), format)
            }
            EvaluationPath::Remote(url) => {
                let map_err = |e| Error::Reqwest {
                    path: path.clone(),
                    error: e,
                };

                let format = if url.path().ends_with(".json") {
                    Format::Json
                } else if url.path().ends_with(".yml") || url.path().ends_with(".yaml") {
                    Format::Yaml
                } else {
                    return Err(Error::UnsupportedFileExt(path.clone()));
                };

                // SAFETY: a parsed evaluation path always has a base, so should always have
                // segments; always push an empty segment to treat it as a directory
                let mut origin = url.clone();
                origin
                    .path_segments_mut()
                    .unwrap()
                    .pop_if_empty()
                    .pop()
                    .push("");

                // Read the contents from the URL
                let contents = reqwest::get(url.clone())
                    .await
                    .map_err(map_err)?
                    .error_for_status()
                    .map_err(map_err)?
                    .text()
                    .await
                    .map_err(map_err)?;

                (contents, EvaluationPath::Remote(origin), format)
            }
        };

        match format {
            Format::Json => serde_json::from_str::<JsonValue>(&content)
                .map_err(Error::from)
                .and_then(|value| match value {
                    JsonValue::Object(object) => Ok(map_to_inputs(object, &origin)),
                    _ => Err(Error::NonMapRoot(path.clone())),
                }),
            Format::Yaml => serde_yaml_ng::from_str::<YamlValue>(&content)
                .map_err(Error::from)
                .and_then(|value| match &value {
                    YamlValue::Mapping(_) => {
                        // SAFETY: a YAML mapping should always be able to be
                        // transformed to a JSON value.
                        let value = serde_json::to_value(value).unwrap();
                        if let JsonValue::Object(map) = value {
                            return Ok(map_to_inputs(map, &origin));
                        }

                        // SAFETY: a serde map will always be translated to a
                        // [`YamlValue::Mapping`] and a [`JsonValue::Object`],
                        // so the above `if` statement should always evaluate to
                        // `true`.
                        unreachable!(
                            "a YAML mapping must always coerce to a JSON object, found `{value}`"
                        )
                    }
                    _ => Err(Error::NonMapRoot(path.clone())),
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn nonmap_root() {
        // A JSON file that does not have a map at the root.
        let err = InputFile::read(&"./tests/fixtures/nonmap_inputs.json".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "input file `tests/fixtures/nonmap_inputs.json` did not contain a map from strings to \
             values at the root"
        );

        // A YML file that does not have a map at the root.
        let err = InputFile::read(&"./tests/fixtures/nonmap_inputs.yml".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "input file `tests/fixtures/nonmap_inputs.yml` did not contain a map from strings to \
             values at the root"
        );
    }

    #[tokio::test]
    async fn missing_ext() {
        let err = InputFile::read(&"./tests/fixtures/missing_ext".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "unsupported input file `tests/fixtures/missing_ext`: the supported formats are JSON \
             (`.json`) or YAML (`.yaml` and `.yml`)"
        );

        let err = InputFile::read(&"http://example.com".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "unsupported input file `http://example.com/`: the supported formats are JSON \
             (`.json`) or YAML (`.yaml` and `.yml`)"
        );
    }

    #[tokio::test]
    async fn read_local() {
        let inputs = InputFile::read(&"./tests/fixtures/inputs_one.json".parse().unwrap())
            .await
            .unwrap();

        let inner = inputs.into_inner();
        assert_eq!(inner.len(), 3);

        let expected_origin = absolute(Path::new("tests/fixtures")).unwrap();
        let expected_origin = expected_origin.to_str().unwrap();

        let (origin, value) = &inner["foo"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "bar");

        let (origin, value) = &inner["baz"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_number().unwrap().as_f64().unwrap() as u64, 42);

        let (origin, value) = &inner["quux"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "qil");
    }

    #[tokio::test]
    async fn read_remote() {
        let inputs = InputFile::read(&"https://gist.githubusercontent.com/peterhuene/9990b86bf0c419e144326b0276bf6f14/raw/d4116ef8888ccd78e2967d7ad32e1aeb3e4ab734/inputs.json".parse().unwrap())
            .await
            .unwrap();

        let inner = inputs.into_inner();
        assert_eq!(inner.len(), 3);

        let expected_origin = "https://gist.githubusercontent.com/peterhuene/9990b86bf0c419e144326b0276bf6f14/raw/d4116ef8888ccd78e2967d7ad32e1aeb3e4ab734/";

        let (origin, value) = &inner["foo"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "bar");

        let (origin, value) = &inner["baz"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_number().unwrap().as_f64().unwrap() as u64, 42);

        let (origin, value) = &inner["quux"];
        assert_eq!(origin.to_str().unwrap(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "qil");
    }
}
