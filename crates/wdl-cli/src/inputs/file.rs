//! Input files parsed in from the command line.

use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;
use wdl_engine::CompoundValue;
use wdl_engine::Object;
use wdl_engine::Value;

use crate::Inputs;

/// An error related to a input file.
#[derive(Error, Debug)]
pub enum Error {
    /// An error occurring in [`serde_json`].
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// An input file cannot be read from a directory.
    #[error("an input file cannot be read from directory `{0}`")]
    InvalidDir(PathBuf),

    /// An I/O error.
    #[error(transparent)]
    Io(std::io::Error),

    /// The input file did not contain a map at the root.
    #[error("input file `{0}` did not contain a map from strings to values at the root")]
    NonMapRoot(PathBuf),

    /// Neither JSON nor YAML could be parsed from the provided path.
    #[error(
        "unsupported file extension `{0}`: the supported formats are JSON (`.json`) or YAML \
         (`.yaml` and `.yml`)"
    )]
    UnsupportedFileExt(String),

    /// An error occurring in [`serde_yaml_ng`].
    #[error(transparent)]
    Yaml(#[from] serde_yaml_ng::Error),
}

/// A [`Result`](std::result::Result) with an [`Error`].
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
    pub fn read<P: AsRef<Path>>(path: P) -> Result<Inputs> {
        let path = path.as_ref();

        if path.is_dir() {
            return Err(Error::InvalidDir(path.to_path_buf()));
        }

        // SAFETY: the check above ensures that the path is not a directory,
        // which means that it can't be the root directory, which means that
        // this call to `.parent()` cannot return `None`.
        let parent = path.parent().unwrap();
        let content: String = std::fs::read_to_string(path).map_err(Error::Io)?;

        fn coerce_object_to_inputs(object: Object, parent: &Path) -> Result<Inputs> {
            let mut inputs = Inputs::default();

            for (key, value) in object.iter() {
                inputs.insert(key.to_owned(), (parent.to_path_buf(), value.clone()));
            }

            Ok(inputs)
        }

        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => serde_json::from_str::<Value>(&content)
                .map_err(Error::from)
                .and_then(|value| match value {
                    Value::Compound(CompoundValue::Object(object)) => {
                        coerce_object_to_inputs(object, parent)
                    }
                    _ => Err(Error::NonMapRoot(path.to_path_buf())),
                }),
            Some("yml") | Some("yaml") => serde_yaml_ng::from_str::<Value>(&content)
                .map_err(Error::from)
                .and_then(|value| match value {
                    Value::Compound(CompoundValue::Object(object)) => {
                        coerce_object_to_inputs(object, parent)
                    }
                    _ => Err(Error::NonMapRoot(path.to_path_buf())),
                }),
            ext => Err(Error::UnsupportedFileExt(ext.unwrap_or("").to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonmap_root() {
        // A JSON file that does not have a map at the root.
        let err = InputFile::read(Path::new("./tests/fixtures/nonmap_inputs.json")).unwrap_err();
        assert!(matches!(
            err,
            Error::NonMapRoot(path) if path.to_str().unwrap() == "./tests/fixtures/nonmap_inputs.json"
        ));

        // A YML file that does not have a map at the root.
        let err = InputFile::read(Path::new("./tests/fixtures/nonmap_inputs.yml")).unwrap_err();
        assert!(matches!(
            err,
            Error::NonMapRoot(path) if path.to_str().unwrap() == "./tests/fixtures/nonmap_inputs.yml"
        ));
    }
}
