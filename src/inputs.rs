//! Invocations (inputs and entrypoints) parsed in from the command line.

use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::bail;
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value as JsonValue;
use thiserror::Error;
use wdl::analysis::Document;
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::LocatedJsonValue;
use wdl::engine::path::EvaluationPath;

pub mod file;
pub mod origin_paths;

pub use origin_paths::OriginPaths;

/// A regex that matches a valid identifier.
///
/// This is useful when recognizing whether a key provided on the command line
/// is a valid identifier.
static IDENTIFIER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // SAFETY: this is checked statically with tests to always unwrap.
    Regex::new(r"^([a-zA-Z][a-zA-Z0-9_.]*)$").unwrap()
});

/// If a value in a key-value pair passed in on the command line cannot be
/// resolved to a WDL type, this regex is compared to the value.
///
/// If the regex matches, we assume the value is a string.
static ASSUME_STRING_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // SAFETY: this is checked statically with tests to always unwrap.
    Regex::new(r"^[^\[\]{}]*$").unwrap()
});

/// An error related to inputs.
#[derive(Error, Debug)]
pub enum Error {
    /// Failed to determine the current working directory.
    #[error("failed to determine the current working directory")]
    NoCurrentWorkingDirectory,

    /// A file error.
    #[error(transparent)]
    File(#[from] file::Error),

    /// Encountered an invalid key-value pair.
    #[error("invalid key-value pair `{pair}`: {reason}")]
    InvalidPair {
        /// The string-value of the pair.
        pair: String,

        /// The reason the pair was not valid.
        reason: String,
    },

    /// An invalid entrypoint was specified.
    #[error("invalid entrypoint `{0}`")]
    InvalidEntrypoint(String),

    /// A deserialization error.
    #[error("unable to deserialize `{0}` as a valid WDL value")]
    Deserialize(String),
}

/// A [`Result`](std::result::Result) with an [`Error`](enum@self::Error).
pub type Result<T> = std::result::Result<T, Error>;

/// An input parsed from the command line.
#[derive(Clone, Debug)]
pub enum Input {
    /// A file.
    File(
        /// The path to the file.
        ///
        /// If this input is successfully created, the input is guaranteed to
        /// exist at the time the inputs were processed.
        EvaluationPath,
    ),
    /// A key-value pair representing an input.
    Pair {
        /// The key.
        key: String,

        /// The value.
        value: JsonValue,
    },
}

impl FromStr for Input {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Error> {
        match s.split_once("=") {
            Some((key, value)) => {
                if !IDENTIFIER_REGEX.is_match(key) {
                    return Err(Error::InvalidPair {
                        pair: s.to_string(),
                        reason: format!(
                            "key `{}` did not match the identifier regex (`{}`)",
                            key,
                            IDENTIFIER_REGEX.as_str()
                        ),
                    });
                }

                let value = serde_json::from_str(value).or_else(|_| {
                    if ASSUME_STRING_REGEX.is_match(value) {
                        Ok(JsonValue::String(value.to_owned()))
                    } else {
                        Err(Error::Deserialize(value.to_owned()))
                    }
                })?;

                Ok(Input::Pair {
                    key: key.to_owned(),
                    value,
                })
            }
            None => {
                let path: EvaluationPath = s.parse().map_err(|e| file::Error::Path {
                    path: s.to_string(),
                    error: e,
                })?;
                if let Some(path) = path.as_local()
                    && !path.exists()
                {
                    return Err(file::Error::NotFound(path.to_path_buf()).into());
                }

                Ok(Input::File(path))
            }
        }
    }
}

/// The map structure used for parsed inputs that have not yet had their paths
/// normalized and converted to engine values.
// TODO ACF 2025-11-11: why is this an `IndexMap` and not a built-in map type?
type JsonInputMap = IndexMap<String, LocatedJsonValue>;

/// A command-line invocation of a WDL workflow or task.
///
/// An invocation is set of inputs parsed from the command line and/or read from
/// files, along with an optional explicit specification of a named entrypoint.
#[derive(Clone, Debug, Default)]
pub struct Invocation {
    /// The actual inputs map.
    inputs: JsonInputMap,
    /// The name of the task or workflow these inputs are provided for.
    entrypoint: Option<String>,
}

impl Invocation {
    /// Adds an input read from the command line.
    async fn add_input(&mut self, input: &str) -> Result<()> {
        match input.parse::<Input>()? {
            Input::File(path) => {
                let inputs = file::read_input_file(&path).await.map_err(Error::File)?;
                self.inputs_mut().extend(inputs);
            }
            Input::Pair { key, value } => {
                let cwd = std::env::current_dir().map_err(|_| Error::NoCurrentWorkingDirectory)?;

                let key = if let Some(prefix) = &self.entrypoint {
                    format!("{prefix}.{key}")
                } else {
                    key
                };
                self.inputs_mut().insert(
                    key,
                    LocatedJsonValue {
                        origin: EvaluationPath::Local(cwd),
                        value,
                    },
                );
            }
        };

        Ok(())
    }

    /// Attempts to coalesce a set of inputs into an [`Inputs`].
    ///
    /// `entrypoint` is the task or workflow the inputs are for.
    /// If `entrypoint` is `Some(_)` then it will be prefixed to each
    /// [`Input::Pair`]. Keys inside a [`Input::File`] must always have this
    /// common prefix specified. If `entrypoint` is `None` then all of the
    /// inputs in `iter` must be prefixed with the task or workflow name.
    pub async fn coalesce<T, V>(iter: T, entrypoint: Option<String>) -> Result<Self>
    where
        T: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        if let Some(ep) = &entrypoint
            && ep.contains('.')
        {
            return Err(Error::InvalidEntrypoint(ep.into()));
        }

        let mut inputs = Invocation {
            entrypoint,
            ..Default::default()
        };

        for input in iter {
            inputs.add_input(input.as_ref()).await?;
        }

        Ok(inputs)
    }

    /// Get a reference to the input map.
    #[cfg_attr(not(test), expect(unused))]
    pub fn inputs(&self) -> &JsonInputMap {
        &self.inputs
    }

    /// Get a mutable reference to the input map.
    pub fn inputs_mut(&mut self) -> &mut JsonInputMap {
        &mut self.inputs
    }

    /// Converts an [`EngineInvocation`] for the given [`Invocation`]
    ///
    /// Returns `Ok(Some(_))` if the inputs are not empty.
    ///
    /// Returns `Ok(None)` if the inputs are empty.
    ///
    /// When the inputs are not empty, the return type contained in `Some(_)` is
    /// a tuple of,
    ///
    /// - the name of the callee (the name of the task or workflow being run),
    /// - the transformed engine inputs, and
    /// - a map containing the origin path for each provided input key.
    pub fn into_engine_invocation(
        self,
        document: &Document,
    ) -> anyhow::Result<Option<(String, EngineInputs, OriginPaths)>> {
        let (origins, values) = self.inputs.into_iter().fold(
            (IndexMap::new(), serde_json::Map::new()),
            |(mut origins, mut values), (key, LocatedJsonValue { origin, value })| {
                origins.insert(key.clone(), origin);
                values.insert(key, value);
                (origins, values)
            },
        );

        let result = EngineInputs::parse_object(document, values)?;

        if let Some((derived, _)) = &result
            && let Some(ep) = &self.entrypoint
            && derived != ep
        {
            bail!(format!(
                "supplied entrypoint `{ep}` does not match derived entrypoint `{derived}`"
            ))
        }

        Ok(result.map(|(callee_name, inputs)| {
            let callee_prefix = format!("{callee_name}.");

            let origins = origins
                .into_iter()
                .map(|(key, path)| {
                    if let Some(key) = key.strip_prefix(&callee_prefix) {
                        (key.to_owned(), path)
                    } else {
                        (key, path)
                    }
                })
                .collect::<IndexMap<_, _>>();

            (callee_name, inputs, OriginPaths::Map(origins))
        }))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    impl Input {
        /// Consumes `self` and returns the inner key-value pair.
        ///
        /// # Panics
        ///
        /// If the input is not a [`Input::Pair`].
        pub fn unwrap_pair(self) -> (String, JsonValue) {
            match self {
                Input::Pair { key, value } => (key, value),
                v => panic!("{v:?} is not an `Input::Pair`"),
            }
        }
    }

    #[test]
    fn identifier_regex() {
        assert!(IDENTIFIER_REGEX.is_match("here_is_an.identifier"));
        assert!(!IDENTIFIER_REGEX.is_match("here is not an identifier"));
    }

    #[test]
    fn assume_string_regex() {
        // Matches.
        assert!(ASSUME_STRING_REGEX.is_match(""));
        assert!(ASSUME_STRING_REGEX.is_match("fooBAR082"));
        assert!(ASSUME_STRING_REGEX.is_match("foo bar baz"));

        // Non-matches.
        assert!(!ASSUME_STRING_REGEX.is_match("[1, a]"));
    }

    #[test]
    fn file_parsing() {
        // A valid JSON file path.
        let input = "./tests/fixtures/inputs_one.json".parse::<Input>().unwrap();
        assert!(matches!(
            input,
            Input::File(path) if path.to_str().unwrap().replace("\\", "/") == "tests/fixtures/inputs_one.json"
        ));

        // A valid YAML file path.
        let input = "tests/fixtures/inputs_three.yml".parse::<Input>().unwrap();
        assert!(matches!(
            input,
            Input::File(path) if path.to_str().unwrap().replace("\\", "/") == "tests/fixtures/inputs_three.yml"
        ));

        // A missing file path.
        let err = "./tests/fixtures/missing.json"
            .parse::<Input>()
            .unwrap_err();
        assert_eq!(
            err.to_string().replace("\\", "/"),
            "input file `tests/fixtures/missing.json` was not found"
        );
    }

    #[test]
    fn key_value_pair_parsing() {
        // A standard key-value pair.
        let input = r#"foo="bar""#.parse::<Input>().unwrap();
        let (key, value) = input.unwrap_pair();
        assert_eq!(key, "foo");
        assert_eq!(value.as_str().unwrap(), "bar");

        // A standard key-value pair.
        let input = r#"foo.bar_baz_quux="qil""#.parse::<Input>().unwrap();
        let (key, value) = input.unwrap_pair();
        assert_eq!(key, "foo.bar_baz_quux");
        assert_eq!(value.as_str().unwrap(), "qil");

        // An invalid identifier for the key.
        let err = r#"foo$="bar""#.parse::<Input>().unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid key-value pair `foo$="bar"`: key `foo$` did not match the identifier regex (`^([a-zA-Z][a-zA-Z0-9_.]*)$`)"#
        );

        // A value that is valid despite that value not being valid as a key.
        let input = r#"foo="bar$""#.parse::<Input>().unwrap();
        let (key, value) = input.unwrap_pair();
        assert_eq!(key, "foo");
        assert_eq!(value.as_str().unwrap(), "bar$");
    }

    #[tokio::test]
    async fn coalesce() {
        // Helper functions.
        fn check_string_value(invocation: &Invocation, key: &str, value: &str) {
            let LocatedJsonValue { value: input, .. } = invocation.inputs().get(key).unwrap();
            assert_eq!(input.as_str().unwrap(), value);
        }

        fn check_float_value(invocation: &Invocation, key: &str, value: f64) {
            let LocatedJsonValue { value: input, .. } = invocation.inputs().get(key).unwrap();
            assert_eq!(input.as_f64().unwrap(), value);
        }

        fn check_boolean_value(invocation: &Invocation, key: &str, value: bool) {
            let LocatedJsonValue { value: input, .. } = invocation.inputs().get(key).unwrap();
            assert_eq!(input.as_bool().unwrap(), value);
        }

        fn check_integer_value(invocation: &Invocation, key: &str, value: i64) {
            let LocatedJsonValue { value: input, .. } = invocation.inputs().get(key).unwrap();
            assert_eq!(input.as_i64().unwrap(), value);
        }

        // The standard coalescing order.
        let invocation = Invocation::coalesce(
            [
                "./tests/fixtures/inputs_one.json",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_three.yml",
            ],
            Some("foo".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs().len(), 5);
        check_string_value(&invocation, "foo", "bar");
        check_float_value(&invocation, "baz", 128.0);
        check_string_value(&invocation, "quux", "qil");
        check_string_value(&invocation, "new.key", "foobarbaz");
        check_string_value(&invocation, "new_two.key", "bazbarfoo");

        // The opposite coalescing order.
        let invocation = Invocation::coalesce(
            [
                "./tests/fixtures/inputs_three.yml",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_one.json",
            ],
            Some("name_ex".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs().len(), 5);
        check_string_value(&invocation, "foo", "bar");
        check_float_value(&invocation, "baz", 42.0);
        check_string_value(&invocation, "quux", "qil");
        check_string_value(&invocation, "new.key", "foobarbaz");
        check_string_value(&invocation, "new_two.key", "bazbarfoo");

        // An example with some random key-value pairs thrown in.
        let invocation = Invocation::coalesce(
            [
                r#"sandwich=-100"#,
                "./tests/fixtures/inputs_one.json",
                "./tests/fixtures/inputs_two.json",
                r#"quux="jacks""#,
                "./tests/fixtures/inputs_three.yml",
                r#"baz=false"#,
            ],
            None,
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs().len(), 6);
        check_string_value(&invocation, "foo", "bar");
        check_boolean_value(&invocation, "baz", false);
        check_string_value(&invocation, "quux", "jacks");
        check_string_value(&invocation, "new.key", "foobarbaz");
        check_string_value(&invocation, "new_two.key", "bazbarfoo");
        check_integer_value(&invocation, "sandwich", -100);

        // An invalid key-value pair.
        let error = Invocation::coalesce(["./tests/fixtures/inputs_one.json", "foo=baz[bar"], None)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "unable to deserialize `baz[bar` as a valid WDL value"
        );

        // A missing file.
        let error = Invocation::coalesce(
            [
                "./tests/fixtures/inputs_one.json",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_three.yml",
                "./tests/fixtures/missing.json",
            ],
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.to_string().replace("\\", "/"),
            "input file `tests/fixtures/missing.json` was not found"
        );
    }

    #[tokio::test]
    async fn coalesce_special_characters() {
        async fn check_can_coalesce_string(value: &str) {
            let invocation = Invocation::coalesce([format!("input={}", value)], None)
                .await
                .unwrap();
            let LocatedJsonValue { value: input, .. } = invocation.inputs().get("input").unwrap();
            assert_eq!(input.as_str().unwrap(), value);
        }
        async fn check_cannot_coalesce_string(value: &str) {
            let error = Invocation::coalesce([format!("input={}", value)], None)
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                Error::Deserialize(output) if output == value
            ));
        }

        check_can_coalesce_string("can-coalesce-dashes").await;
        check_can_coalesce_string("can\"coalesce\"quotes").await;
        check_can_coalesce_string("can'coalesce'apostrophes").await;
        check_can_coalesce_string("can;coalesce;semicolons").await;
        check_can_coalesce_string("can:coalesce:colons").await;
        check_can_coalesce_string("can*coalesce*stars").await;
        check_can_coalesce_string("can,coalesce,commas").await;
        check_can_coalesce_string("can?coalesce?question?mark").await;
        check_can_coalesce_string("can|coalesce|pipe").await;
        check_can_coalesce_string("can<coalesce>less<than>or>greater<than").await;
        check_can_coalesce_string("can^coalesce^carrot").await;
        check_can_coalesce_string("can#coalesce#pound#sign").await;
        check_can_coalesce_string("can%coalesce%percent").await;
        check_can_coalesce_string("can!coalesce!exclamation!marks").await;
        check_can_coalesce_string("can\\coalesce\\backslashes").await;
        check_can_coalesce_string("can@coalesce@at@sign").await;
        check_can_coalesce_string("can(coalesce(parenthesis))").await;
        check_can_coalesce_string("can coalesce السلام عليكم").await;
        check_can_coalesce_string("can coalesce 你").await;
        check_can_coalesce_string("can coalesce Dobrý den").await;
        check_can_coalesce_string("can coalesce Hello").await;
        check_can_coalesce_string("can coalesce שלום").await;
        check_can_coalesce_string("can coalesce नमस्ते").await;
        check_can_coalesce_string("can coalesce こんにちは").await;
        check_can_coalesce_string("can coalesce 안녕하세요").await;
        check_can_coalesce_string("can coalesce 你好").await;
        check_can_coalesce_string("can coalesce Olá").await;
        check_can_coalesce_string("can coalesce Здравствуйте").await;
        check_can_coalesce_string("can coalesce Hola").await;
        check_cannot_coalesce_string("cannot coalesce string with [").await;
        check_cannot_coalesce_string("cannot coalesce string with ]").await;
        check_cannot_coalesce_string("cannot coalesce string with {").await;
        check_cannot_coalesce_string("cannot coalesce string with }").await;
    }

    #[test]
    fn multiple_equal_signs() {
        let (key, value) = r#"foo="bar=baz""#.parse::<Input>().unwrap().unwrap_pair();
        assert_eq!(key, "foo");
        assert_eq!(value.as_str().unwrap(), "bar=baz");
    }
}
