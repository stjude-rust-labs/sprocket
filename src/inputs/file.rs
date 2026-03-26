//! Input files parsed in from the command line.

use std::ffi::OsStr;
use std::fmt;
use std::path::absolute;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use wdl::engine::EvaluationPath;
use wdl::engine::JsonMap;

use super::JsonInputMap;
use crate::inputs::LocatedJsonValue;

/// Helper for formatting an unsupported file format error.
fn unsupported_file_extension(path: impl fmt::Display) -> anyhow::Error {
    anyhow!(
        "unsupported input file `{path}`: the supported formats are JSON (`.json`) or YAML \
         (`.yaml` and `.yml`)"
    )
}

/// Supported inputs file formats
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Format {
    /// The inputs file is a JSON file
    Json,
    /// The inputs file is a YAML file
    Yaml,
}

/// Read an input file into an input map.
///
/// The file is attempted to be parsed based on its extension.
///
/// The supported file formats are JSON and YAML.
pub async fn read_input_file(path: &EvaluationPath) -> Result<JsonInputMap> {
    fn json_map_to_inputs(map: JsonMap, origin: &EvaluationPath) -> JsonInputMap {
        let mut inputs = JsonInputMap::new();

        for (key, value) in map {
            inputs.insert(
                key,
                LocatedJsonValue {
                    origin: origin.clone(),
                    value,
                },
            );
        }

        inputs
    }

    fn parse(
        format: Format,
        contents: &str,
        path: impl fmt::Display,
        origin: EvaluationPath,
    ) -> Result<JsonInputMap> {
        match format {
            Format::Json => match serde_json::from_str::<JsonValue>(contents)
                .with_context(|| format!("failed to deserialize JSON inputs file `{path}`"))?
            {
                JsonValue::Object(object) => Ok(json_map_to_inputs(object, &origin)),
                _ => bail!(
                    "input file `{path}` did not contain a map from strings to values at the root"
                ),
            },
            Format::Yaml => {
                if let YamlValue::Mapping(mapping) = serde_yaml_ng::from_str::<YamlValue>(contents)
                    .with_context(|| format!("failed to deserialize YAML inputs file `{path}`"))?
                {
                    let value = serde_json::to_value(mapping)
                        .with_context(|| format!("invalid YAML input file `{path}`"))?;

                    if let JsonValue::Object(map) = value {
                        return Ok(json_map_to_inputs(map, &origin));
                    }
                }

                bail!(
                    "input file `{path}` did not contain a map from strings to values at the root"
                );
            }
        }
    }

    if path.is_local() {
        let path = path.as_local().expect("path should be local");
        if path.is_dir() {
            bail!(
                "an inputs file cannot be read from directory `{path}`",
                path = path.display()
            );
        }

        let format = match path.extension().and_then(OsStr::to_str) {
            Some("json") => Format::Json,
            Some("yml") | Some("yaml") => Format::Yaml,
            _ => return Err(unsupported_file_extension(path.display())),
        };

        // Get the absolute path for the origin
        let absolute_path = absolute(path).with_context(|| {
            format!(
                "cannot make origin path `{path}` absolute",
                path = path.display()
            )
        })?;

        let origin = absolute_path.parent().unwrap_or(&absolute_path);

        // Read the contents from the local file
        let contents = tokio::fs::read_to_string(&path).await.with_context(|| {
            format!("failed to read inputs file `{path}`", path = path.display())
        })?;

        parse(format, &contents, path.display(), origin.into())
    } else {
        let url = path.as_remote().expect("path should be remote");
        let format = if url.path().ends_with(".json") {
            Format::Json
        } else if url.path().ends_with(".yml") || url.path().ends_with(".yaml") {
            Format::Yaml
        } else {
            return Err(unsupported_file_extension(url));
        };

        let mut origin = url.clone();
        origin
            .path_segments_mut()
            // the URL always has a base, so we can unwrap `path_segments_mut`
            .unwrap()
            // pop off a trailing `/`, if it exists
            .pop_if_empty()
            // pop the "file name" to get to the parent
            .pop()
            // push an empty segment on the end so any subsequent `join()` will treat the
            // URL as a "directory"
            .push("");

        // Read the contents from the URL
        let contents = reqwest::get(url.clone())
            .await
            .context("failed to read inputs file")?
            .error_for_status()
            .context("failed to read inputs file")?
            .text()
            .await
            .with_context(|| format!("failed to read inputs file `{url}`"))?;

        parse(format, &contents, url, origin.try_into()?)
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
        let err = read_input_file(&"./tests/fixtures/nonmap_inputs.json".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string().replace("\\", "/"),
            "input file `tests/fixtures/nonmap_inputs.json` did not contain a map from strings to \
             values at the root"
        );

        // A YML file that does not have a map at the root.
        let err = read_input_file(&"./tests/fixtures/nonmap_inputs.yml".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string().replace("\\", "/"),
            "input file `tests/fixtures/nonmap_inputs.yml` did not contain a map from strings to \
             values at the root"
        );
    }

    #[tokio::test]
    async fn missing_ext() {
        let err = read_input_file(&"./tests/fixtures/missing_ext".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string().replace("\\", "/"),
            "unsupported input file `tests/fixtures/missing_ext`: the supported formats are JSON \
             (`.json`) or YAML (`.yaml` and `.yml`)"
        );

        let err = read_input_file(&"http://example.com".parse().unwrap())
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
        let inputs = read_input_file(&"./tests/fixtures/inputs_one.json".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(inputs.len(), 3);

        let expected_origin = absolute(Path::new("tests/fixtures")).unwrap();
        let expected_origin = expected_origin.to_str().unwrap();

        let LocatedJsonValue { origin, value } = &inputs["foo"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "bar");

        let LocatedJsonValue { origin, value } = &inputs["baz"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_number().unwrap().as_f64().unwrap() as u64, 42);

        let LocatedJsonValue { origin, value } = &inputs["quux"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "qil");
    }

    #[tokio::test]
    async fn read_remote() {
        // The URL is a gist of `fixtures/inputs_one.json`
        // Create a new gist and substitute it here if the file contents need to change
        let inputs = read_input_file(&"https://gist.githubusercontent.com/peterhuene/9990b86bf0c419e144326b0276bf6f14/raw/d4116ef8888ccd78e2967d7ad32e1aeb3e4ab734/inputs.json".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(inputs.len(), 3);

        let expected_origin = "https://gist.githubusercontent.com/peterhuene/9990b86bf0c419e144326b0276bf6f14/raw/d4116ef8888ccd78e2967d7ad32e1aeb3e4ab734/";

        let LocatedJsonValue { origin, value } = &inputs["foo"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "bar");

        let LocatedJsonValue { origin, value } = &inputs["baz"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_number().unwrap().as_f64().unwrap() as u64, 42);

        let LocatedJsonValue { origin, value } = &inputs["quux"];
        assert_eq!(origin.to_string(), expected_origin);
        assert_eq!(value.as_str().unwrap(), "qil");
    }

    #[tokio::test]
    async fn read_remote_missing() {
        let err = read_input_file(&"https://google.com/404.json".parse().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "failed to read inputs file: HTTP status client error (404 Not Found) for url (https://google.com/404.json)"
        );
    }
}
