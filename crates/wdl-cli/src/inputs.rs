//! Inputs parsed in from the command line.

use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::bail;
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;
use thiserror::Error;
use wdl_analysis::Document;
use wdl_engine::Inputs as EngineInputs;

pub mod file;
pub mod origin_paths;

pub use file::InputFile;
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
    /// A file error.
    #[error(transparent)]
    File(#[from] file::Error),

    /// A file was specified on the command line but not found.
    #[error("file `{0}` was not found")]
    FileNotFound(PathBuf),

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
        PathBuf,
    ),

    /// A key-value pair representing an input.
    Pair {
        /// The key.
        key: String,

        /// The value.
        value: Value,
    },
}

impl Input {
    /// Attempts to return a reference to the inner [`Path`].
    ///
    /// * If the input is a [`Input::File`], a reference to the inner path is
    ///   returned wrapped in [`Some`].
    /// * Otherwise, [`None`] is returned.
    pub fn as_file(&self) -> Option<&Path> {
        match self {
            Input::File(p) => Some(p.as_path()),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`PathBuf`].
    ///
    /// * If the input is a [`Input::File`], the inner path buffer is returned
    ///   wrapped in [`Some`].
    /// * Otherwise, [`None`] is returned.
    pub fn into_file(self) -> Option<PathBuf> {
        match self {
            Input::File(p) => Some(p),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`PathBuf`].
    ///
    /// # Panics
    ///
    /// If the input is not a [`Input::File`].
    pub fn unwrap_file(self) -> PathBuf {
        match self {
            Input::File(p) => p,
            v => panic!("{v:?} is not an `Input::File`"),
        }
    }

    /// Attempts to return a reference to the inner key-value pair.
    ///
    /// * If the input is a [`Input::Pair`], a reference to the inner key and
    ///   value is returned wrapped in [`Some`].
    /// * Otherwise, [`None`] is returned.
    pub fn as_pair(&self) -> Option<(&str, &Value)> {
        match self {
            Input::Pair { key, value } => Some((key.as_str(), value)),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner key-value pair.
    ///
    /// * If the input is a [`Input::Pair`], the inner key-value pair is
    ///   returned wrapped in [`Some`].
    /// * Otherwise, [`None`] is returned.
    pub fn into_pair(self) -> Option<(String, Value)> {
        match self {
            Input::Pair { key, value } => Some((key, value)),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner key-value pair.
    ///
    /// # Panics
    ///
    /// If the input is not a [`Input::Pair`].
    pub fn unwrap_pair(self) -> (String, Value) {
        match self {
            Input::Pair { key, value } => (key, value),
            v => panic!("{v:?} is not an `Input::Pair`"),
        }
    }
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
                        Ok(Value::String(value.to_owned()))
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
                let path = PathBuf::from(s);

                if !path.exists() {
                    return Err(Error::FileNotFound(path));
                }

                Ok(Input::File(path))
            }
        }
    }
}

/// The inner type for inputs (for convenience).
type InputsInner = IndexMap<String, (PathBuf, Value)>;

/// A set of inputs parsed from the command line and compiled on top of one
/// another.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    /// The actual inputs map.
    inputs: InputsInner,
    /// The name of the task or workflow these inputs are provided for.
    entrypoint: Option<String>,
}

impl Inputs {
    /// Adds an input read from the command line.
    fn add_input(&mut self, input: &str) -> Result<()> {
        match input.parse::<Input>()? {
            Input::File(path) => {
                let inputs = InputFile::read(&path).map_err(Error::File)?;
                self.extend(inputs.into_inner());
            }
            Input::Pair { key, value } => {
                // SAFETY: we expect that the current working directory is
                // always available for the platforms that `wdl` will run
                // within.
                let cwd = std::env::current_dir().unwrap();

                let key = if let Some(prefix) = &self.entrypoint {
                    format!("{prefix}.{key}")
                } else {
                    key
                };
                self.insert(key, (cwd, value));
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
    pub fn coalesce<T, V>(iter: T, entrypoint: Option<String>) -> Result<Self>
    where
        T: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        if let Some(ep) = &entrypoint
            && ep.contains('.')
        {
            return Err(Error::InvalidEntrypoint(ep.into()));
        }

        let mut inputs = Inputs {
            entrypoint,
            ..Default::default()
        };

        for input in iter {
            inputs.add_input(input.as_ref())?;
        }

        Ok(inputs)
    }

    /// Consumes `self` and returns the inner index map.
    pub fn into_inner(self) -> InputsInner {
        self.inputs
    }

    /// Converts a set of inputs to a set of engine inputs.
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
    pub fn into_engine_inputs(
        self,
        document: &Document,
    ) -> anyhow::Result<Option<(String, EngineInputs, OriginPaths)>> {
        let (origins, values) = self.inputs.into_iter().fold(
            (IndexMap::new(), serde_json::Map::new()),
            |(mut origins, mut values), (key, (origin, value))| {
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
                .collect::<IndexMap<String, PathBuf>>();

            (callee_name, inputs, OriginPaths::from(origins))
        }))
    }
}

impl Deref for Inputs {
    type Target = InputsInner;

    fn deref(&self) -> &Self::Target {
        &self.inputs
    }
}

impl DerefMut for Inputs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Input::File(path) if path.to_str().unwrap() == "./tests/fixtures/inputs_one.json"
        ));

        // A valid YAML file path.
        let input = "./tests/fixtures/inputs_three.yml"
            .parse::<Input>()
            .unwrap();
        assert!(matches!(
            input,
            Input::File(path) if path.to_str().unwrap() == "./tests/fixtures/inputs_three.yml"
        ));

        // A missing file path.
        let err = "./tests/fixtures/missing.json"
            .parse::<Input>()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::FileNotFound(path) if path.to_str().unwrap() == "./tests/fixtures/missing.json"
        ));
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
        assert!(matches!(
                err,
                Error::InvalidPair {
                    pair,
                    reason
                } if pair == r#"foo$="bar""# &&
                reason == r"key `foo$` did not match the identifier regex (`^([a-zA-Z][a-zA-Z0-9_.]*)$`)"));

        // A value that is valid despite that value not being valid as a key.
        let input = r#"foo="bar$""#.parse::<Input>().unwrap();
        let (key, value) = input.unwrap_pair();
        assert_eq!(key, "foo");
        assert_eq!(value.as_str().unwrap(), "bar$");
    }

    #[test]
    fn coalesce() {
        // Helper functions.
        fn check_string_value(inputs: &Inputs, key: &str, value: &str) {
            let (_, input) = inputs.get(key).unwrap();
            assert_eq!(input.as_str().unwrap(), value);
        }

        fn check_float_value(inputs: &Inputs, key: &str, value: f64) {
            let (_, input) = inputs.get(key).unwrap();
            assert_eq!(input.as_f64().unwrap(), value);
        }

        fn check_boolean_value(inputs: &Inputs, key: &str, value: bool) {
            let (_, input) = inputs.get(key).unwrap();
            assert_eq!(input.as_bool().unwrap(), value);
        }

        fn check_integer_value(inputs: &Inputs, key: &str, value: i64) {
            let (_, input) = inputs.get(key).unwrap();
            assert_eq!(input.as_i64().unwrap(), value);
        }

        // The standard coalescing order.
        let inputs = Inputs::coalesce(
            [
                "./tests/fixtures/inputs_one.json",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_three.yml",
            ],
            Some("foo".to_string()),
        )
        .unwrap();

        assert_eq!(inputs.len(), 5);
        check_string_value(&inputs, "foo", "bar");
        check_float_value(&inputs, "baz", 128.0);
        check_string_value(&inputs, "quux", "qil");
        check_string_value(&inputs, "new.key", "foobarbaz");
        check_string_value(&inputs, "new_two.key", "bazbarfoo");

        // The opposite coalescing order.
        let inputs = Inputs::coalesce(
            [
                "./tests/fixtures/inputs_three.yml",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_one.json",
            ],
            Some("name_ex".to_string()),
        )
        .unwrap();

        assert_eq!(inputs.len(), 5);
        check_string_value(&inputs, "foo", "bar");
        check_float_value(&inputs, "baz", 42.0);
        check_string_value(&inputs, "quux", "qil");
        check_string_value(&inputs, "new.key", "foobarbaz");
        check_string_value(&inputs, "new_two.key", "bazbarfoo");

        // An example with some random key-value pairs thrown in.
        let inputs = Inputs::coalesce(
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
        .unwrap();

        assert_eq!(inputs.len(), 6);
        check_string_value(&inputs, "foo", "bar");
        check_boolean_value(&inputs, "baz", false);
        check_string_value(&inputs, "quux", "jacks");
        check_string_value(&inputs, "new.key", "foobarbaz");
        check_string_value(&inputs, "new_two.key", "bazbarfoo");
        check_integer_value(&inputs, "sandwich", -100);

        // An invalid key-value pair.
        let error = Inputs::coalesce(["./tests/fixtures/inputs_one.json", "foo=baz[bar"], None)
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Deserialize(value) if value == "baz[bar"
        ));

        // A missing file.
        let error = Inputs::coalesce(
            [
                "./tests/fixtures/inputs_one.json",
                "./tests/fixtures/inputs_two.json",
                "./tests/fixtures/inputs_three.yml",
                "./tests/fixtures/missing.json",
            ],
            None,
        )
        .unwrap_err();
        assert!(matches!(
                error,
                Error::FileNotFound(path) if path.to_str().unwrap() == "./tests/fixtures/missing.json"));
    }

    #[test]
    fn coalesce_special_characters() {
        fn check_can_coalesce_string(value: &str) {
            let inputs = Inputs::coalesce([format!("input={}", value)], None).unwrap();
            let (_, input) = inputs.get("input").unwrap();
            assert_eq!(input.as_str().unwrap(), value);
        }
        fn check_cannot_coalesce_string(value: &str) {
            let error = Inputs::coalesce([format!("input={}", value)], None).unwrap_err();
            assert!(matches!(
                error,
                Error::Deserialize(output) if output == value
            ));
        }

        check_can_coalesce_string("can-coalesce-dashes");
        check_can_coalesce_string("can\"coalesce\"quotes");
        check_can_coalesce_string("can'coalesce'apostrophes");
        check_can_coalesce_string("can;coalesce;semicolons");
        check_can_coalesce_string("can:coalesce:colons");
        check_can_coalesce_string("can*coalesce*stars");
        check_can_coalesce_string("can,coalesce,commas");
        check_can_coalesce_string("can?coalesce?question?mark");
        check_can_coalesce_string("can|coalesce|pipe");
        check_can_coalesce_string("can<coalesce>less<than>or>greater<than");
        check_can_coalesce_string("can^coalesce^carrot");
        check_can_coalesce_string("can#coalesce#pound#sign");
        check_can_coalesce_string("can%coalesce%percent");
        check_can_coalesce_string("can!coalesce!exclamation!marks");
        check_can_coalesce_string("can\\coalesce\\backslashes");
        check_can_coalesce_string("can@coalesce@at@sign");
        check_can_coalesce_string("can(coalesce(parenthesis))");
        check_can_coalesce_string("can coalesce السلام عليكم");
        check_can_coalesce_string("can coalesce 你");
        check_can_coalesce_string("can coalesce Dobrý den");
        check_can_coalesce_string("can coalesce Hello");
        check_can_coalesce_string("can coalesce שלום");
        check_can_coalesce_string("can coalesce नमस्ते");
        check_can_coalesce_string("can coalesce こんにちは");
        check_can_coalesce_string("can coalesce 안녕하세요");
        check_can_coalesce_string("can coalesce 你好");
        check_can_coalesce_string("can coalesce Olá");
        check_can_coalesce_string("can coalesce Здравствуйте");
        check_can_coalesce_string("can coalesce Hola");
        check_cannot_coalesce_string("cannot coalesce string with [");
        check_cannot_coalesce_string("cannot coalesce string with ]");
        check_cannot_coalesce_string("cannot coalesce string with {");
        check_cannot_coalesce_string("cannot coalesce string with }");
    }

    #[test]
    fn multiple_equal_signs() {
        let (key, value) = r#"foo="bar=baz""#.parse::<Input>().unwrap().unwrap_pair();
        assert_eq!(key, "foo");
        assert_eq!(value.as_str().unwrap(), "bar=baz");
    }
}
