//! Invocations (inputs and targets) parsed in from the command line.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use regex::Regex;
use serde_json::Value as JsonValue;
use url::Url;
use wdl::analysis::Document;
use wdl::engine::EvaluationPath;
use wdl::engine::Inputs as EngineInputs;

pub mod file;

use crate::analysis::is_supported_source_url;

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

/// An input value that has not yet had its paths normalized and been converted
/// to an engine value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedJsonValue {
    /// The location where this input was initially read, used for normalizing
    /// any paths the value may contain.
    pub origin: EvaluationPath,
    /// The raw JSON representation of the input value.
    pub value: JsonValue,
}

/// An input parsed from the command line.
#[derive(Clone, Debug)]
pub enum Input {
    /// The input is a file.
    ///
    /// If this input is successfully created, the input is guaranteed to
    /// exist at the time the inputs were processed.
    File(EvaluationPath),
    /// The input is a key-value pair.
    Pair {
        /// The key.
        key: String,

        /// The value.
        value: JsonValue,
    },
}

impl FromStr for Input {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some(path_str) = s.strip_prefix('@') {
            let path: EvaluationPath = if is_supported_source_url(path_str) {
                path_str
                    .parse::<Url>()
                    .map_err(|_| anyhow!("invalid inputs file URL `{path_str}`"))?
                    .try_into()?
            } else {
                let path: EvaluationPath = path_str.parse()?;

                if path.is_remote() {
                    bail!("unsupported inputs file URL `{path_str}`");
                }

                if let Some(path) = path.as_local()
                    && !path.exists()
                {
                    bail!("input file `{path_str}` was not found");
                }

                path
            };

            return Ok(Self::File(path));
        }

        match s.split_once("=") {
            Some((key, value)) => {
                if !IDENTIFIER_REGEX.is_match(key) {
                    bail!(
                        "invalid key-value pair `{s}`: key `{key}` did not match the identifier \
                         regex (`{regex}`)",
                        regex = IDENTIFIER_REGEX.as_str()
                    );
                }

                let value = serde_json::from_str(value).or_else(|_| {
                    if ASSUME_STRING_REGEX.is_match(value) {
                        Ok(JsonValue::String(value.to_owned()))
                    } else {
                        bail!("unable to deserialize `{value}` as a valid WDL value");
                    }
                })?;

                Ok(Self::Pair {
                    key: key.to_owned(),
                    value,
                })
            }
            None => {
                bail!(
                    "unrecognized input `{s}`: use `@` prefix for input files or `key=value` for \
                     inputs"
                );
            }
        }
    }
}

/// The map structure used for parsed inputs that have not yet had their paths
/// normalized and converted to engine values.
type JsonInputMap = BTreeMap<String, Vec<LocatedJsonValue>>;

/// A command-line invocation of a WDL workflow or task.
///
/// An invocation is set of inputs parsed from the command line and/or read from
/// files, along with an optional explicit specification of a named target.
#[derive(Clone, Debug, Default)]
pub struct Invocation {
    /// The actual inputs map.
    inputs: JsonInputMap,
    /// The most recently added CLI key-value pair key.
    ///
    /// When a bare argument (no `=`) is encountered that is not a valid input
    /// file, it is appended to this key's values. This supports shell glob
    /// expansion where `files=*.txt` expands to `files=a.txt b.txt c.txt`.
    last_pair_key: Option<String>,
    /// The name of the task or workflow these inputs are provided for.
    target: Option<String>,
}

impl Invocation {
    /// Prefixes a key with the target name if needed.
    ///
    /// When `self.target` is set:
    ///
    /// * If the key already starts with `"{target}."`, it is returned as-is.
    /// * Otherwise the key is prefixed with `"{target}."`.
    fn prefix_key(&self, key: String) -> String {
        match &self.target {
            Some(target) => {
                if key
                    .strip_prefix(target)
                    .and_then(|r| r.strip_prefix('.'))
                    .is_some()
                {
                    key
                } else {
                    format!("{target}.{key}")
                }
            }
            None => key,
        }
    }

    /// Adds an input read from the command line.
    async fn add_input(&mut self, input: &str) -> Result<()> {
        match input.parse::<Input>() {
            Ok(Input::File(url)) => {
                let inputs = file::read_input_file(&url).await?;
                for (key, values) in inputs {
                    self.inputs.insert(self.prefix_key(key), values);
                }
                self.last_pair_key = None;
            }
            Ok(Input::Pair { key, value }) => {
                let cwd = std::env::current_dir()
                    .context("failed to determine the current working directory")?;

                let prefixed = self.prefix_key(key);
                self.inputs
                    .entry(prefixed.clone())
                    .or_default()
                    .push(LocatedJsonValue {
                        origin: cwd.as_path().into(),
                        value,
                    });
                self.last_pair_key = Some(prefixed);
            }
            Err(_) if self.last_pair_key.is_some() => {
                let cwd = std::env::current_dir()
                    .context("failed to determine the current working directory")?;

                let key = self.last_pair_key.as_ref().unwrap();
                self.inputs
                    .entry(key.clone())
                    .or_default()
                    .push(LocatedJsonValue {
                        origin: cwd.as_path().into(),
                        value: JsonValue::String(input.to_owned()),
                    });
            }
            Err(e) => return Err(e),
        };

        Ok(())
    }

    /// Attempts to coalesce a set of inputs into an [`Invocation`].
    ///
    /// `target` is the task or workflow the inputs are for. If `target` is
    /// `Some(_)`, then all input keys—both from [`Input::Pair`] and
    /// [`Input::File`]—that do not already start with `"{target}."` will be
    /// automatically prefixed with the target name. If `target` is `None`,
    /// all input keys must already be prefixed with the task or workflow
    /// name.
    pub async fn coalesce<T, V>(iter: T, target: Option<String>) -> Result<Self>
    where
        T: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        if let Some(t) = &target
            && t.contains('.')
        {
            bail!("invalid target `{t}`");
        }

        let mut inputs = Invocation {
            target,
            ..Default::default()
        };

        for input in iter {
            inputs.add_input(input.as_ref()).await?;
        }

        Ok(inputs)
    }

    /// Converts to an [`Inputs`](EngineInputs).
    ///
    /// Relative paths in the inputs are resolved to their absolute paths based
    /// on the input key's origin.
    ///
    /// Returns the target name and the engine inputs if the inputs were not
    /// empty.
    ///
    /// Returns `Ok(None)` if the inputs were empty.
    pub async fn into_engine_inputs(
        self,
        document: &Document,
    ) -> Result<Option<(String, EngineInputs)>> {
        let (origins, values) = self.inputs.into_iter().fold(
            (BTreeMap::new(), serde_json::Map::new()),
            |(mut origins, mut values), (key, located_values)| {
                if located_values.len() == 1 {
                    let lv = located_values.into_iter().next().unwrap();
                    origins.insert(key.clone(), lv.origin);
                    values.insert(key, lv.value);
                } else {
                    let first_origin = located_values[0].origin.clone();
                    let json_array: Vec<JsonValue> =
                        located_values.into_iter().map(|lv| lv.value).collect();
                    origins.insert(key.clone(), first_origin);
                    values.insert(key, JsonValue::Array(json_array));
                }
                (origins, values)
            },
        );

        let Some((target, mut inputs)) = EngineInputs::parse_json_object(document, values)? else {
            return Ok(None);
        };

        if let Some(t) = &self.target
            && target != *t
        {
            bail!(format!(
                "supplied target `{t}` does not match the target `{target}` derived from the \
                 inputs"
            ))
        }

        // Resolve relative paths using per-input origins
        match &mut inputs {
            EngineInputs::Task(task_inputs) => {
                let task = document
                    .task_by_name(&target)
                    .with_context(|| format!("task `{target}` was not found"))?;

                task_inputs
                    .join_paths(task, |key| {
                        let key = format!("{target}.{key}");
                        origins
                            .get(&key)
                            .ok_or_else(|| anyhow!("no origin path for input `{key}`"))
                    })
                    .await
                    .context("failed to resolve input paths")?;
            }
            EngineInputs::Workflow(workflow_inputs) => {
                let workflow = document.workflow().context("workflow not found")?;

                if workflow.name() != target {
                    bail!("workflow `{target}` was not found");
                }

                workflow_inputs
                    .join_paths(workflow, |key| {
                        let key = format!("{target}.{key}");
                        origins
                            .get(&key)
                            .ok_or_else(|| anyhow!("no origin path for input `{key}`"))
                    })
                    .await
                    .context("failed to resolve input paths")?;
            }
        }

        Ok(Some((target, inputs)))
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
                Self::Pair { key, value } => (key, value),
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
        // A valid JSON file path with `@` prefix.
        let input = "@./tests/fixtures/inputs_one.json"
            .parse::<Input>()
            .unwrap();
        assert!(matches!(
            input,
            Input::File(path) if path.to_string().replace("\\", "/") == "tests/fixtures/inputs_one.json"
        ));

        // A valid YAML file path with `@` prefix.
        let input = "@tests/fixtures/inputs_three.yml".parse::<Input>().unwrap();
        assert!(matches!(
            input,
            Input::File(path) if path.to_string().replace("\\", "/") == "tests/fixtures/inputs_three.yml"
        ));

        // A missing file path.
        let err = "@./tests/fixtures/missing.json"
            .parse::<Input>()
            .unwrap_err();
        assert_eq!(
            err.to_string().replace("\\", "/"),
            "input file `./tests/fixtures/missing.json` was not found"
        );

        // Without `@` prefix, a bare argument is not parsed as a file.
        let err = "./tests/fixtures/inputs_one.json"
            .parse::<Input>()
            .unwrap_err();
        assert!(err.to_string().contains("unrecognized input"));
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
        // Helper functions that check the last value for a given key.
        fn check_string_value(invocation: &Invocation, key: &str, value: &str) {
            let values = invocation.inputs.get(key).unwrap();
            assert_eq!(values.last().unwrap().value.as_str().unwrap(), value);
        }

        fn check_float_value(invocation: &Invocation, key: &str, value: f64) {
            let values = invocation.inputs.get(key).unwrap();
            assert_eq!(values.last().unwrap().value.as_f64().unwrap(), value);
        }

        fn check_boolean_value(invocation: &Invocation, key: &str, value: bool) {
            let values = invocation.inputs.get(key).unwrap();
            assert_eq!(values.last().unwrap().value.as_bool().unwrap(), value);
        }

        fn check_integer_value(invocation: &Invocation, key: &str, value: i64) {
            let values = invocation.inputs.get(key).unwrap();
            assert_eq!(values.last().unwrap().value.as_i64().unwrap(), value);
        }

        // The standard coalescing order.
        let invocation = Invocation::coalesce(
            [
                "@./tests/fixtures/inputs_one.json",
                "@./tests/fixtures/inputs_two.json",
                "@./tests/fixtures/inputs_three.yml",
            ],
            Some("foo".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs.len(), 5);
        check_string_value(&invocation, "foo.foo", "bar");
        check_float_value(&invocation, "foo.baz", 128.0);
        check_string_value(&invocation, "foo.quux", "qil");
        check_string_value(&invocation, "foo.new.key", "foobarbaz");
        check_string_value(&invocation, "foo.new_two.key", "bazbarfoo");

        // The opposite coalescing order.
        let invocation = Invocation::coalesce(
            [
                "@./tests/fixtures/inputs_three.yml",
                "@./tests/fixtures/inputs_two.json",
                "@./tests/fixtures/inputs_one.json",
            ],
            Some("name_ex".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs.len(), 5);
        check_string_value(&invocation, "name_ex.foo", "bar");
        check_float_value(&invocation, "name_ex.baz", 42.0);
        check_string_value(&invocation, "name_ex.quux", "qil");
        check_string_value(&invocation, "name_ex.new.key", "foobarbaz");
        check_string_value(&invocation, "name_ex.new_two.key", "bazbarfoo");

        // An example with some random key-value pairs thrown in.
        let invocation = Invocation::coalesce(
            [
                r#"sandwich=-100"#,
                "@./tests/fixtures/inputs_one.json",
                "@./tests/fixtures/inputs_two.json",
                r#"quux="jacks""#,
                "@./tests/fixtures/inputs_three.yml",
                r#"baz=false"#,
            ],
            None,
        )
        .await
        .unwrap();

        assert_eq!(invocation.inputs.len(), 6);
        check_string_value(&invocation, "foo", "bar");
        check_boolean_value(&invocation, "baz", false);
        check_string_value(&invocation, "quux", "jacks");
        check_string_value(&invocation, "new.key", "foobarbaz");
        check_string_value(&invocation, "new_two.key", "bazbarfoo");
        check_integer_value(&invocation, "sandwich", -100);

        // An invalid key-value pair.
        let error =
            Invocation::coalesce(["@./tests/fixtures/inputs_one.json", "foo=baz[bar"], None)
                .await
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "unable to deserialize `baz[bar` as a valid WDL value"
        );

        // A missing file.
        let error = Invocation::coalesce(
            [
                "@./tests/fixtures/inputs_one.json",
                "@./tests/fixtures/inputs_two.json",
                "@./tests/fixtures/inputs_three.yml",
                "@./tests/fixtures/missing.json",
            ],
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.to_string().replace("\\", "/"),
            "input file `./tests/fixtures/missing.json` was not found"
        );
    }

    #[tokio::test]
    async fn coalesce_special_characters() {
        async fn check_can_coalesce_string(value: &str) {
            let invocation = Invocation::coalesce([format!("input={}", value)], None)
                .await
                .unwrap();
            let values = invocation.inputs.get("input").unwrap();
            assert_eq!(values.last().unwrap().value.as_str().unwrap(), value);
        }
        async fn check_cannot_coalesce_string(value: &str) {
            let error = Invocation::coalesce([format!("input={}", value)], None)
                .await
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("unable to deserialize `{value}` as a valid WDL value")
            );
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

    #[tokio::test]
    async fn file_inputs_prefixed_with_target() {
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_one.json"],
            Some("mytask".to_string()),
        )
        .await
        .unwrap();

        for key in invocation.inputs.keys() {
            assert!(
                key.starts_with("mytask."),
                "expected key `{key}` to be prefixed with `mytask.`"
            );
        }

        assert!(invocation.inputs.contains_key("mytask.foo"));
        assert!(invocation.inputs.contains_key("mytask.baz"));
        assert!(invocation.inputs.contains_key("mytask.quux"));
    }

    #[tokio::test]
    async fn file_inputs_already_prefixed_not_doubled() {
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_two.json"],
            Some("new".to_string()),
        )
        .await
        .unwrap();

        assert!(
            invocation.inputs.contains_key("new.key"),
            "key `new.key` should not be double-prefixed"
        );
        assert!(
            !invocation.inputs.contains_key("new.new.key"),
            "key `new.key` was double-prefixed to `new.new.key`"
        );
        assert!(invocation.inputs.contains_key("new.baz"));
        assert!(invocation.inputs.contains_key("new.quux"));
    }

    #[tokio::test]
    async fn mixed_file_and_cli_inputs_with_target() {
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_one.json", r#"extra="value""#],
            Some("tgt".to_string()),
        )
        .await
        .unwrap();

        assert!(invocation.inputs.contains_key("tgt.foo"));
        assert!(invocation.inputs.contains_key("tgt.baz"));
        assert!(invocation.inputs.contains_key("tgt.quux"));
        assert!(invocation.inputs.contains_key("tgt.extra"));
    }

    #[tokio::test]
    async fn cli_inputs_already_prefixed_not_doubled() {
        let invocation = Invocation::coalesce([r#"tgt.name="hello""#], Some("tgt".to_string()))
            .await
            .unwrap();

        assert!(
            invocation.inputs.contains_key("tgt.name"),
            "key `tgt.name` should not be double-prefixed"
        );
        assert!(
            !invocation.inputs.contains_key("tgt.tgt.name"),
            "key `tgt.name` was double-prefixed to `tgt.tgt.name`"
        );
    }

    #[tokio::test]
    async fn dotted_key_with_different_target_prefixes_normally() {
        // `inputs_two.json` contains `new.key`. The dotted key should be
        // prefixed normally to `foo.new.key`.
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_two.json"],
            Some(String::from("foo")),
        )
        .await
        .unwrap();

        assert!(invocation.inputs.contains_key("foo.new.key"));
        assert!(invocation.inputs.contains_key("foo.baz"));
        assert!(invocation.inputs.contains_key("foo.quux"));
    }

    #[tokio::test]
    async fn repeated_key_collects_into_array() {
        let invocation = Invocation::coalesce(
            ["files=a.txt", "files=b.txt", "files=c.txt"],
            Some("task".to_string()),
        )
        .await
        .unwrap();

        let values = invocation.inputs.get("task.files").unwrap();
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].value.as_str().unwrap(), "a.txt");
        assert_eq!(values[1].value.as_str().unwrap(), "b.txt");
        assert_eq!(values[2].value.as_str().unwrap(), "c.txt");
    }

    #[tokio::test]
    async fn single_key_remains_scalar() {
        let invocation = Invocation::coalesce(["name=hello"], Some("task".to_string()))
            .await
            .unwrap();

        let values = invocation.inputs.get("task.name").unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value.as_str().unwrap(), "hello");
    }

    #[tokio::test]
    async fn repeated_dotted_key_collects_into_array() {
        let invocation = Invocation::coalesce(["wf.task.files=a.txt", "wf.task.files=b.txt"], None)
            .await
            .unwrap();

        let values = invocation.inputs.get("wf.task.files").unwrap();
        assert_eq!(values.len(), 2);
    }

    #[tokio::test]
    async fn mixed_repeated_and_single_keys() {
        let invocation = Invocation::coalesce(
            ["files=a.txt", "files=b.txt", "name=hello"],
            Some("task".to_string()),
        )
        .await
        .unwrap();

        let files = invocation.inputs.get("task.files").unwrap();
        assert_eq!(files.len(), 2);

        let name = invocation.inputs.get("task.name").unwrap();
        assert_eq!(name.len(), 1);
        assert_eq!(name[0].value.as_str().unwrap(), "hello");
    }

    #[tokio::test]
    async fn trailing_bare_args_append_to_last_key() {
        let invocation =
            Invocation::coalesce(["files=a.txt", "b.txt", "c.txt"], Some("task".to_string()))
                .await
                .unwrap();

        let values = invocation.inputs.get("task.files").unwrap();
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].value.as_str().unwrap(), "a.txt");
        assert_eq!(values[1].value.as_str().unwrap(), "b.txt");
        assert_eq!(values[2].value.as_str().unwrap(), "c.txt");
    }

    #[tokio::test]
    async fn at_prefix_is_file() {
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_one.json"],
            Some("foo".to_string()),
        )
        .await
        .unwrap();

        assert!(invocation.inputs.contains_key("foo.foo"));
    }

    #[tokio::test]
    async fn bare_arg_with_no_prior_key_errors() {
        let error = Invocation::coalesce(["not_a_file.txt"], Some("task".to_string()))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("unrecognized input"));
    }

    #[tokio::test]
    async fn bare_args_between_different_keys() {
        let invocation = Invocation::coalesce(
            ["files=a.txt", "b.txt", "names=hello", "world"],
            Some("task".to_string()),
        )
        .await
        .unwrap();

        let files = invocation.inputs.get("task.files").unwrap();
        assert_eq!(files.len(), 2);

        let names = invocation.inputs.get("task.names").unwrap();
        assert_eq!(names.len(), 2);
    }

    #[tokio::test]
    async fn json_array_syntax_still_works() {
        let invocation =
            Invocation::coalesce([r#"files=["a.txt","b.txt"]"#], Some("task".to_string()))
                .await
                .unwrap();

        let values = invocation.inputs.get("task.files").unwrap();
        assert_eq!(values.len(), 1);
        let arr = values[0].value.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn repeated_keys_with_file_inputs() {
        let invocation = Invocation::coalesce(
            ["@./tests/fixtures/inputs_one.json", "extra=a", "extra=b"],
            Some("task".to_string()),
        )
        .await
        .unwrap();

        let values = invocation.inputs.get("task.extra").unwrap();
        assert_eq!(values.len(), 2);
    }

    #[tokio::test]
    async fn file_input_resets_last_pair_key() {
        let error = Invocation::coalesce(
            [
                "files=a.txt",
                "@./tests/fixtures/inputs_one.json",
                "not_a_file.txt",
            ],
            Some("task".to_string()),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("unrecognized input"));
    }
}
