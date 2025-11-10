//! Implementation of workflow and task inputs.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
use serde::Serialize;
use serde::ser::SerializeMap;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use wdl_analysis::Document;
use wdl_analysis::document::Input;
use wdl_analysis::document::Task;
use wdl_analysis::document::Workflow;
use wdl_analysis::types::CallKind;
use wdl_analysis::types::Coercible as _;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::display_types;
use wdl_analysis::types::v1::task_hint_types;
use wdl_analysis::types::v1::task_requirement_types;
use wdl_ast::SupportedVersion;
use wdl_ast::version::V1;

use crate::Coercible;
use crate::Value;
use crate::path::EvaluationPath;

/// A type alias to a JSON map (object).
pub type JsonMap = serde_json::Map<String, JsonValue>;

/// Checks that an input value matches the type of the input.
fn check_input_type(document: &Document, name: &str, input: &Input, value: &Value) -> Result<()> {
    // For WDL 1.2, we accept optional values for the input even if the input's type
    // is non-optional; if the runtime value is `None` for a non-optional input, the
    // default expression will be evaluated instead
    let expected_ty = if !input.required()
        && document
            .version()
            .map(|v| v >= SupportedVersion::V1(V1::Two))
            .unwrap_or(false)
    {
        input.ty().optional()
    } else {
        input.ty().clone()
    };

    let ty = value.ty();
    if !ty.is_coercible_to(&expected_ty) {
        bail!("expected type `{expected_ty}` for input `{name}`, but found `{ty}`");
    }

    Ok(())
}

/// Represents inputs to a task.
#[derive(Default, Debug, Clone)]
pub struct TaskInputs {
    /// The task input values.
    inputs: IndexMap<String, Value>,
    /// The overridden requirements section values.
    requirements: HashMap<String, Value>,
    /// The overridden hints section values.
    hints: HashMap<String, Value>,
}

impl TaskInputs {
    /// Iterates the inputs to the task.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.inputs.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Gets an input by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.inputs.get(name)
    }

    /// Sets a task input.
    ///
    /// Returns the previous value, if any.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        self.inputs.insert(name.into(), value.into())
    }

    /// Gets an overridden requirement by name.
    pub fn requirement(&self, name: &str) -> Option<&Value> {
        self.requirements.get(name)
    }

    /// Overrides a requirement by name.
    pub fn override_requirement(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.requirements.insert(name.into(), value.into());
    }

    /// Gets an overridden hint by name.
    pub fn hint(&self, name: &str) -> Option<&Value> {
        self.hints.get(name)
    }

    /// Overrides a hint by name.
    pub fn override_hint(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.hints.insert(name.into(), value.into());
    }

    /// Replaces any `File` or `Directory` input values with joining the
    /// specified path with the value.
    ///
    /// This method will attempt to coerce matching input values to their
    /// expected types.
    pub async fn join_paths<'a>(
        &mut self,
        task: &Task,
        path: impl Fn(&str) -> Result<&'a EvaluationPath>,
    ) -> Result<()> {
        for (name, value) in self.inputs.iter_mut() {
            let Some(ty) = task.inputs().get(name).map(|input| input.ty().clone()) else {
                bail!("could not find an expected type for input {name}");
            };

            let base_dir = path(name)?;

            if let Ok(v) = value.coerce(None, &ty) {
                *value = v
                    .resolve_paths(ty.is_optional(), None, None, &|path| path.expand(base_dir))
                    .await?;
            }
        }
        Ok(())
    }

    /// Validates the inputs for the given task.
    ///
    /// The `specified` set of inputs are those that are present, but may not
    /// have values available at validation.
    pub fn validate(
        &self,
        document: &Document,
        task: &Task,
        specified: Option<&HashSet<String>>,
    ) -> Result<()> {
        let version = document.version().context("missing document version")?;

        // Start by validating all the specified inputs and their types
        for (name, value) in &self.inputs {
            let input = task
                .inputs()
                .get(name)
                .with_context(|| format!("unknown input `{name}`"))?;

            check_input_type(document, name, input, value)?;
        }

        // Next check for missing required inputs
        for (name, input) in task.inputs() {
            if input.required()
                && !self.inputs.contains_key(name)
                && specified.map(|s| !s.contains(name)).unwrap_or(true)
            {
                bail!(
                    "missing required input `{name}` to task `{task}`",
                    task = task.name()
                );
            }
        }

        // Check the types of the specified requirements
        for (name, value) in &self.requirements {
            let ty = value.ty();
            if let Some(expected) = task_requirement_types(version, name.as_str()) {
                if !expected.iter().any(|target| ty.is_coercible_to(target)) {
                    bail!(
                        "expected {expected} for requirement `{name}`, but found type `{ty}`",
                        expected = display_types(expected),
                    );
                }

                continue;
            }

            bail!("unsupported requirement `{name}`");
        }

        // Check the types of the specified hints
        for (name, value) in &self.hints {
            let ty = value.ty();
            if let Some(expected) = task_hint_types(version, name.as_str(), false)
                && !expected.iter().any(|target| ty.is_coercible_to(target))
            {
                bail!(
                    "expected {expected} for hint `{name}`, but found type `{ty}`",
                    expected = display_types(expected),
                );
            }
        }

        Ok(())
    }

    /// Sets a value with dotted path notation.
    ///
    /// If the provided `value` is a [`PrimitiveType`] other than
    /// [`PrimitiveType::String`] and the `path` is to an input which is of
    /// type [`PrimitiveType::String`], `value` will be converted to a string
    /// and accepted as valid.
    fn set_path_value(
        &mut self,
        document: &Document,
        task: &Task,
        path: &str,
        value: Value,
    ) -> Result<()> {
        let version = document.version().expect("document should have a version");

        match path.split_once('.') {
            // The path might contain a requirement or hint
            Some((key, remainder)) => {
                let (must_match, matched) = match key {
                    "runtime" => (
                        false,
                        task_requirement_types(version, remainder)
                            .map(|types| (true, types))
                            .or_else(|| {
                                task_hint_types(version, remainder, false)
                                    .map(|types| (false, types))
                            }),
                    ),
                    "requirements" => (
                        true,
                        task_requirement_types(version, remainder).map(|types| (true, types)),
                    ),
                    "hints" => (
                        false,
                        task_hint_types(version, remainder, false).map(|types| (false, types)),
                    ),
                    _ => {
                        bail!(
                            "task `{task}` does not have an input named `{path}`",
                            task = task.name()
                        );
                    }
                };

                if let Some((requirement, expected)) = matched {
                    for ty in expected {
                        if value.ty().is_coercible_to(ty) {
                            if requirement {
                                self.requirements.insert(remainder.to_string(), value);
                            } else {
                                self.hints.insert(remainder.to_string(), value);
                            }
                            return Ok(());
                        }
                    }

                    bail!(
                        "expected {expected} for {key} key `{remainder}`, but found type `{ty}`",
                        expected = display_types(expected),
                        ty = value.ty()
                    );
                } else if must_match {
                    bail!("unsupported {key} key `{remainder}`");
                } else {
                    Ok(())
                }
            }
            // The path is to an input
            None => {
                let input = task.inputs().get(path).with_context(|| {
                    format!(
                        "task `{name}` does not have an input named `{path}`",
                        name = task.name()
                    )
                })?;

                // Allow primitive values to implicitly convert to string
                let actual = value.ty();
                let expected = input.ty();
                if let Some(PrimitiveType::String) = expected.as_primitive()
                    && let Some(actual) = actual.as_primitive()
                    && actual != PrimitiveType::String
                {
                    self.inputs
                        .insert(path.to_string(), value.to_string().into());
                    return Ok(());
                }

                check_input_type(document, path, input, &value)?;
                self.inputs.insert(path.to_string(), value);
                Ok(())
            }
        }
    }
}

impl<S, V> FromIterator<(S, V)> for TaskInputs
where
    S: Into<String>,
    V: Into<Value>,
{
    fn from_iter<T: IntoIterator<Item = (S, V)>>(iter: T) -> Self {
        Self {
            inputs: iter
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            requirements: Default::default(),
            hints: Default::default(),
        }
    }
}

impl Serialize for TaskInputs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Only serialize the input values
        let mut map = serializer.serialize_map(Some(self.inputs.len()))?;
        for (k, v) in &self.inputs {
            let serialized_value = crate::ValueSerializer::new(v, true);
            map.serialize_entry(k, &serialized_value)?;
        }
        map.end()
    }
}

/// Represents inputs to a workflow.
#[derive(Default, Debug, Clone)]
pub struct WorkflowInputs {
    /// The workflow input values.
    inputs: IndexMap<String, Value>,
    /// The nested call inputs.
    calls: HashMap<String, Inputs>,
}

impl WorkflowInputs {
    /// Iterates the inputs to the workflow.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.inputs.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Gets an input by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.inputs.get(name)
    }

    /// Gets the nested call inputs.
    pub fn calls(&self) -> &HashMap<String, Inputs> {
        &self.calls
    }

    /// Gets the nested call inputs.
    pub fn calls_mut(&mut self) -> &mut HashMap<String, Inputs> {
        &mut self.calls
    }

    /// Sets a workflow input.
    ///
    /// Returns the previous value, if any.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        self.inputs.insert(name.into(), value.into())
    }

    /// Checks if the inputs contain a value with the specified name.
    ///
    /// This does not check nested call inputs.
    pub fn contains(&self, name: &str) -> bool {
        self.inputs.contains_key(name)
    }

    /// Replaces any `File` or `Directory` input values with joining the
    /// specified path with the value.
    ///
    /// This method will attempt to coerce matching input values to their
    /// expected types.
    pub async fn join_paths<'a>(
        &mut self,
        workflow: &Workflow,
        path: impl Fn(&str) -> Result<&'a EvaluationPath>,
    ) -> Result<()> {
        for (name, value) in self.inputs.iter_mut() {
            let Some(ty) = workflow.inputs().get(name).map(|input| input.ty().clone()) else {
                bail!("could not find an expected type for input {name}");
            };

            let base_dir = path(name)?;

            if let Ok(v) = value.coerce(None, &ty) {
                *value = v
                    .resolve_paths(ty.is_optional(), None, None, &|path| path.expand(base_dir))
                    .await?;
            }
        }
        Ok(())
    }

    /// Validates the inputs for the given workflow.
    ///
    /// The `specified` set of inputs are those that are present, but may not
    /// have values available at validation.
    pub fn validate(
        &self,
        document: &Document,
        workflow: &Workflow,
        specified: Option<&HashSet<String>>,
    ) -> Result<()> {
        // Start by validating all the specified inputs and their types
        for (name, value) in &self.inputs {
            let input = workflow
                .inputs()
                .get(name)
                .with_context(|| format!("unknown input `{name}`"))?;
            check_input_type(document, name, input, value)?;
        }

        // Next check for missing required inputs
        for (name, input) in workflow.inputs() {
            if input.required()
                && !self.inputs.contains_key(name)
                && specified.map(|s| !s.contains(name)).unwrap_or(true)
            {
                bail!(
                    "missing required input `{name}` to workflow `{workflow}`",
                    workflow = workflow.name()
                );
            }
        }

        // Check that the workflow allows nested inputs
        if !self.calls.is_empty() && !workflow.allows_nested_inputs() {
            bail!(
                "cannot specify a nested call input for workflow `{name}` as it does not allow \
                 nested inputs",
                name = workflow.name()
            );
        }

        // Check the inputs to the specified calls
        for (name, inputs) in &self.calls {
            let call = workflow.calls().get(name).with_context(|| {
                format!(
                    "workflow `{workflow}` does not have a call named `{name}`",
                    workflow = workflow.name()
                )
            })?;

            // Resolve the target document; the namespace is guaranteed to be present in the
            // document.
            let document = call
                .namespace()
                .map(|ns| {
                    document
                        .namespace(ns)
                        .expect("namespace should be present")
                        .document()
                })
                .unwrap_or(document);

            // Validate the call's inputs
            let inputs = match call.kind() {
                CallKind::Task => {
                    let task = document
                        .task_by_name(call.name())
                        .expect("task should be present");

                    let task_inputs = inputs.as_task_inputs().with_context(|| {
                        format!("`{name}` is a call to a task, but workflow inputs were supplied")
                    })?;

                    task_inputs.validate(document, task, Some(call.specified()))?;
                    &task_inputs.inputs
                }
                CallKind::Workflow => {
                    let workflow = document.workflow().expect("should have a workflow");
                    assert_eq!(
                        workflow.name(),
                        call.name(),
                        "call name does not match workflow name"
                    );
                    let workflow_inputs = inputs.as_workflow_inputs().with_context(|| {
                        format!("`{name}` is a call to a workflow, but task inputs were supplied")
                    })?;

                    workflow_inputs.validate(document, workflow, Some(call.specified()))?;
                    &workflow_inputs.inputs
                }
            };

            for input in inputs.keys() {
                if call.specified().contains(input) {
                    bail!(
                        "cannot specify nested input `{input}` for call `{call}` as it was \
                         explicitly specified in the call itself",
                        call = call.name(),
                    );
                }
            }
        }

        // Finally, check for missing call arguments
        if workflow.allows_nested_inputs() {
            for (call, ty) in workflow.calls() {
                let inputs = self.calls.get(call);

                for (input, _) in ty
                    .inputs()
                    .iter()
                    .filter(|(n, i)| i.required() && !ty.specified().contains(*n))
                {
                    if !inputs.map(|i| i.get(input).is_some()).unwrap_or(false) {
                        bail!("missing required input `{input}` for call `{call}`");
                    }
                }
            }
        }

        Ok(())
    }

    /// Sets a value with dotted path notation.
    ///
    /// If the provided `value` is a [`PrimitiveType`] other than
    /// [`PrimitiveType::String`] and the `path` is to an input which is of
    /// type [`PrimitiveType::String`], `value` will be converted to a string
    /// and accepted as valid.
    fn set_path_value(
        &mut self,
        document: &Document,
        workflow: &Workflow,
        path: &str,
        value: Value,
    ) -> Result<()> {
        match path.split_once('.') {
            Some((name, remainder)) => {
                // Check that the workflow allows nested inputs
                if !workflow.allows_nested_inputs() {
                    bail!(
                        "cannot specify a nested call input for workflow `{workflow}` as it does \
                         not allow nested inputs",
                        workflow = workflow.name()
                    );
                }

                // Resolve the call by name
                let call = workflow.calls().get(name).with_context(|| {
                    format!(
                        "workflow `{workflow}` does not have a call named `{name}`",
                        workflow = workflow.name()
                    )
                })?;

                // Insert the inputs for the call
                let inputs =
                    self.calls
                        .entry(name.to_string())
                        .or_insert_with(|| match call.kind() {
                            CallKind::Task => Inputs::Task(Default::default()),
                            CallKind::Workflow => Inputs::Workflow(Default::default()),
                        });

                // Resolve the target document; the namespace is guaranteed to be present in the
                // document.
                let document = call
                    .namespace()
                    .map(|ns| {
                        document
                            .namespace(ns)
                            .expect("namespace should be present")
                            .document()
                    })
                    .unwrap_or(document);

                let next = remainder
                    .split_once('.')
                    .map(|(n, _)| n)
                    .unwrap_or(remainder);
                if call.specified().contains(next) {
                    bail!(
                        "cannot specify nested input `{next}` for call `{name}` as it was \
                         explicitly specified in the call itself",
                    );
                }

                // Recurse on the call's inputs to set the value
                match call.kind() {
                    CallKind::Task => {
                        let task = document
                            .task_by_name(call.name())
                            .expect("task should be present");
                        inputs
                            .as_task_inputs_mut()
                            .expect("should be a task input")
                            .set_path_value(document, task, remainder, value)
                    }
                    CallKind::Workflow => {
                        let workflow = document.workflow().expect("should have a workflow");
                        assert_eq!(
                            workflow.name(),
                            call.name(),
                            "call name does not match workflow name"
                        );
                        inputs
                            .as_workflow_inputs_mut()
                            .expect("should be a task input")
                            .set_path_value(document, workflow, remainder, value)
                    }
                }
            }
            None => {
                let input = workflow.inputs().get(path).with_context(|| {
                    format!(
                        "workflow `{workflow}` does not have an input named `{path}`",
                        workflow = workflow.name()
                    )
                })?;

                // Allow primitive values to implicitly convert to string
                let actual = value.ty();
                let expected = input.ty();
                if let Some(PrimitiveType::String) = expected.as_primitive()
                    && let Some(actual) = actual.as_primitive()
                    && actual != PrimitiveType::String
                {
                    self.inputs
                        .insert(path.to_string(), value.to_string().into());
                    return Ok(());
                }

                check_input_type(document, path, input, &value)?;
                self.inputs.insert(path.to_string(), value);
                Ok(())
            }
        }
    }
}

impl<S, V> FromIterator<(S, V)> for WorkflowInputs
where
    S: Into<String>,
    V: Into<Value>,
{
    fn from_iter<T: IntoIterator<Item = (S, V)>>(iter: T) -> Self {
        Self {
            inputs: iter
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            calls: Default::default(),
        }
    }
}

impl Serialize for WorkflowInputs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Note: for serializing, only serialize the direct inputs, not the nested
        // inputs
        let mut map = serializer.serialize_map(Some(self.inputs.len()))?;
        for (k, v) in &self.inputs {
            let serialized_value = crate::ValueSerializer::new(v, true);
            map.serialize_entry(k, &serialized_value)?;
        }
        map.end()
    }
}

/// Represents inputs to a WDL workflow or task.
#[derive(Debug, Clone)]
pub enum Inputs {
    /// The inputs are to a task.
    Task(TaskInputs),
    /// The inputs are to a workflow.
    Workflow(WorkflowInputs),
}

impl Inputs {
    /// Parses an inputs file from the given file path.
    ///
    /// The format (JSON or YAML) is determined by the file extension:
    ///
    /// - `.json` for JSON format
    /// - `.yml` or `.yaml` for YAML format
    ///
    /// The parse uses the provided document to validate the input keys within
    /// the file.
    ///
    /// Returns `Ok(Some(_))` if the inputs are not empty.
    ///
    /// Returns `Ok(None)` if the inputs are empty.
    pub fn parse(document: &Document, path: impl AsRef<Path>) -> Result<Option<(String, Self)>> {
        let path = path.as_ref();

        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => Self::parse_json(document, path),
            Some("yml") | Some("yaml") => Self::parse_yaml(document, path),
            ext => bail!(
                "unsupported file extension: `{ext}`; the supported formats are JSON (`.json`) \
                 and YAML (`.yaml` and `.yml`)",
                ext = ext.unwrap_or("")
            ),
        }
        .with_context(|| format!("failed to parse input file `{path}`", path = path.display()))
    }

    /// Parses a JSON inputs file from the given file path.
    ///
    /// The parse uses the provided document to validate the input keys within
    /// the file.
    ///
    /// Returns `Ok(Some(_))` if the inputs are not empty.
    ///
    /// Returns `Ok(None)` if the inputs are empty.
    pub fn parse_json(
        document: &Document,
        path: impl AsRef<Path>,
    ) -> Result<Option<(String, Self)>> {
        let path = path.as_ref();

        let file = File::open(path).with_context(|| {
            format!("failed to open input file `{path}`", path = path.display())
        })?;

        // Parse the JSON (should be an object)
        let reader = BufReader::new(file);

        let map = std::mem::take(
            serde_json::from_reader::<_, JsonValue>(reader)?
                .as_object_mut()
                .with_context(|| {
                    format!(
                        "expected input file `{path}` to contain a JSON object",
                        path = path.display()
                    )
                })?,
        );

        Self::parse_object(document, map)
    }

    /// Parses a YAML inputs file from the given file path.
    ///
    /// The parse uses the provided document to validate the input keys within
    /// the file.
    ///
    /// Returns `Ok(Some(_))` if the inputs are not empty.
    ///
    /// Returns `Ok(None)` if the inputs are empty.
    pub fn parse_yaml(
        document: &Document,
        path: impl AsRef<Path>,
    ) -> Result<Option<(String, Self)>> {
        let path = path.as_ref();

        let file = File::open(path).with_context(|| {
            format!("failed to open input file `{path}`", path = path.display())
        })?;

        // Parse the YAML
        let reader = BufReader::new(file);
        let yaml = serde_yaml_ng::from_reader::<_, YamlValue>(reader)?;

        // Convert YAML to JSON format
        let mut json = serde_json::to_value(yaml).with_context(|| {
            format!(
                "failed to convert YAML to JSON for processing `{path}`",
                path = path.display()
            )
        })?;

        let object = std::mem::take(json.as_object_mut().with_context(|| {
            format!(
                "expected input file `{path}` to contain a YAML mapping",
                path = path.display()
            )
        })?);

        Self::parse_object(document, object)
    }

    /// Gets an input value.
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Task(t) => t.inputs.get(name),
            Self::Workflow(w) => w.inputs.get(name),
        }
    }

    /// Sets an input value.
    ///
    /// Returns the previous value, if any.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        match self {
            Self::Task(inputs) => inputs.set(name, value),
            Self::Workflow(inputs) => inputs.set(name, value),
        }
    }

    /// Gets the task inputs.
    ///
    /// Returns `None` if the inputs are for a workflow.
    pub fn as_task_inputs(&self) -> Option<&TaskInputs> {
        match self {
            Self::Task(inputs) => Some(inputs),
            Self::Workflow(_) => None,
        }
    }

    /// Gets a mutable reference to task inputs.
    ///
    /// Returns `None` if the inputs are for a workflow.
    pub fn as_task_inputs_mut(&mut self) -> Option<&mut TaskInputs> {
        match self {
            Self::Task(inputs) => Some(inputs),
            Self::Workflow(_) => None,
        }
    }

    /// Unwraps the inputs as task inputs.
    ///
    /// # Panics
    ///
    /// Panics if the inputs are for a workflow.
    pub fn unwrap_task_inputs(self) -> TaskInputs {
        match self {
            Self::Task(inputs) => inputs,
            Self::Workflow(_) => panic!("inputs are for a workflow"),
        }
    }

    /// Gets the workflow inputs.
    ///
    /// Returns `None` if the inputs are for a task.
    pub fn as_workflow_inputs(&self) -> Option<&WorkflowInputs> {
        match self {
            Self::Task(_) => None,
            Self::Workflow(inputs) => Some(inputs),
        }
    }

    /// Gets a mutable reference to workflow inputs.
    ///
    /// Returns `None` if the inputs are for a task.
    pub fn as_workflow_inputs_mut(&mut self) -> Option<&mut WorkflowInputs> {
        match self {
            Self::Task(_) => None,
            Self::Workflow(inputs) => Some(inputs),
        }
    }

    /// Unwraps the inputs as workflow inputs.
    ///
    /// # Panics
    ///
    /// Panics if the inputs are for a task.
    pub fn unwrap_workflow_inputs(self) -> WorkflowInputs {
        match self {
            Self::Task(_) => panic!("inputs are for a task"),
            Self::Workflow(inputs) => inputs,
        }
    }

    /// Parses the root object in a [`JsonMap`].
    ///
    /// Returns `Ok(Some(_))` if the inputs are not empty.
    ///
    /// Returns `Ok(None)` if the inputs are empty.
    pub fn parse_object(document: &Document, object: JsonMap) -> Result<Option<(String, Self)>> {
        // Determine the root workflow or task name
        let (key, name) = match object.iter().next() {
            Some((key, _)) => match key.split_once('.') {
                Some((name, _remainder)) => (key, name),
                None => {
                    bail!(
                        "invalid input key `{key}`: expected the value to be prefixed with the \
                         workflow or task name",
                    )
                }
            },
            // If the object is empty, treat it as a workflow evaluation without any inputs
            None => {
                return Ok(None);
            }
        };

        match (document.task_by_name(name), document.workflow()) {
            (Some(task), _) => Ok(Some(Self::parse_task_inputs(document, task, object)?)),
            (None, Some(workflow)) if workflow.name() == name => Ok(Some(
                Self::parse_workflow_inputs(document, workflow, object)?,
            )),
            _ => bail!(
                "invalid input key `{key}`: a task or workflow named `{name}` does not exist in \
                 the document"
            ),
        }
    }

    /// Parses the inputs for a task.
    fn parse_task_inputs(
        document: &Document,
        task: &Task,
        object: JsonMap,
    ) -> Result<(String, Self)> {
        let mut inputs = TaskInputs::default();
        for (key, value) in object {
            // Convert from serde_json::Value to crate::Value
            let value = serde_json::from_value(value)
                .with_context(|| format!("invalid input key `{key}`"))?;

            match key.split_once(".") {
                Some((prefix, remainder)) if prefix == task.name() => {
                    inputs
                        .set_path_value(document, task, remainder, value)
                        .with_context(|| format!("invalid input key `{key}`"))?;
                }
                _ => {
                    bail!(
                        "invalid input key `{key}`: expected key to be prefixed with `{task}`",
                        task = task.name()
                    );
                }
            }
        }

        Ok((task.name().to_string(), Inputs::Task(inputs)))
    }

    /// Parses the inputs for a workflow.
    fn parse_workflow_inputs(
        document: &Document,
        workflow: &Workflow,
        object: JsonMap,
    ) -> Result<(String, Self)> {
        let mut inputs = WorkflowInputs::default();
        for (key, value) in object {
            // Convert from serde_json::Value to crate::Value
            let value = serde_json::from_value(value)
                .with_context(|| format!("invalid input key `{key}`"))?;

            match key.split_once(".") {
                Some((prefix, remainder)) if prefix == workflow.name() => {
                    inputs
                        .set_path_value(document, workflow, remainder, value)
                        .with_context(|| format!("invalid input key `{key}`"))?;
                }
                _ => {
                    bail!(
                        "invalid input key `{key}`: expected key to be prefixed with `{workflow}`",
                        workflow = workflow.name()
                    );
                }
            }
        }

        Ok((workflow.name().to_string(), Inputs::Workflow(inputs)))
    }
}

impl From<TaskInputs> for Inputs {
    fn from(inputs: TaskInputs) -> Self {
        Self::Task(inputs)
    }
}

impl From<WorkflowInputs> for Inputs {
    fn from(inputs: WorkflowInputs) -> Self {
        Self::Workflow(inputs)
    }
}
