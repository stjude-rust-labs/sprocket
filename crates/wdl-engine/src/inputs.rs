//! Implementation of workflow and task inputs.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::mem;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use serde_json::Value as JsonValue;
use wdl_analysis::document::Document;
use wdl_analysis::document::Task;
use wdl_analysis::document::Workflow;
use wdl_analysis::types::CallKind;
use wdl_analysis::types::Coercible;
use wdl_analysis::types::Types;
use wdl_analysis::types::display_types;
use wdl_analysis::types::v1::task_hint_types;
use wdl_analysis::types::v1::task_requirement_types;

use crate::Value;

/// A type alias to a JSON map (object).
type JsonMap = serde_json::Map<String, JsonValue>;

/// Represents inputs to a task.
#[derive(Default, Debug, Clone)]
pub struct TaskInputs {
    /// The task input values.
    inputs: HashMap<String, Value>,
    /// The overridden requirements section values.
    requirements: HashMap<String, Value>,
    /// The overridden hints section values.
    hints: HashMap<String, Value>,
}

impl TaskInputs {
    /// Gets the inputs to the task.
    pub fn inputs(&self) -> &HashMap<String, Value> {
        &self.inputs
    }

    /// Gets the overridden requirements.
    pub fn requirements(&self) -> &HashMap<String, Value> {
        &self.requirements
    }

    /// Gets the overridden hints.
    pub fn hints(&self) -> &HashMap<String, Value> {
        &self.hints
    }

    /// Sets a task input.
    pub fn set_input(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.inputs.insert(name.into(), value.into());
    }

    /// Overrides a task requirement.
    pub fn override_requirement(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.requirements.insert(name.into(), value.into());
    }

    /// Overrides a task hint.
    pub fn override_hint(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.hints.insert(name.into(), value.into());
    }

    /// Validates the inputs for the given task.
    pub fn validate(&self, types: &mut Types, document: &Document, task: &Task) -> Result<()> {
        let version = document
            .version()
            .ok_or_else(|| anyhow!("missing document version"))?;

        // Start by validating all the specified inputs and their types
        for (name, value) in &self.inputs {
            let input = task
                .inputs()
                .get(name)
                .ok_or_else(|| anyhow!("unknown input `{name}`"))?;
            let expected_ty = types.import(document.types(), input.ty());
            let ty = value.ty();
            if !ty.is_coercible_to(types, &expected_ty) {
                bail!(
                    "expected type `{expected_ty}` for input `{name}`, but found `{ty}`",
                    expected_ty = expected_ty.display(types),
                    ty = ty.display(types)
                );
            }
        }

        // Next check for missing required inputs
        for (name, input) in task.inputs() {
            if input.required() && !self.inputs.contains_key(name) {
                bail!("missing required input `{name}`");
            }
        }

        // Check the types of the specified requirements
        for (name, value) in &self.requirements {
            let ty = value.ty();
            if let Some(expected) = task_requirement_types(version, name.as_str()) {
                if !expected
                    .iter()
                    .any(|target| ty.is_coercible_to(types, target))
                {
                    bail!(
                        "expected {expected} for requirement `{name}`, but found type `{ty}`",
                        expected = display_types(types, expected),
                        ty = ty.display(types)
                    );
                }

                continue;
            }

            bail!("unsupported requirement `{name}`");
        }

        // Check the types of the specified hints
        for (name, value) in &self.hints {
            let ty = value.ty();
            if let Some(expected) = task_hint_types(version, name.as_str(), false) {
                if !expected
                    .iter()
                    .any(|target| ty.is_coercible_to(types, target))
                {
                    bail!(
                        "expected {expected} for hint `{name}`, but found type `{ty}`",
                        expected = display_types(types, expected),
                        ty = ty.display(types)
                    );
                }
            }
        }

        Ok(())
    }

    /// Sets a value with dotted path notation.
    fn set_path_value(
        &mut self,
        types: &mut Types,
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
                        if value.ty().is_coercible_to(types, ty) {
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
                        expected = display_types(types, expected),
                        ty = value.ty().display(types)
                    );
                } else if must_match {
                    bail!("unsupported {key} key `{remainder}`");
                } else {
                    Ok(())
                }
            }
            // The path is to an input
            None => {
                let input = task.inputs().get(path).ok_or_else(|| {
                    anyhow!(
                        "task `{name}` does not have an input named `{path}`",
                        name = task.name()
                    )
                })?;

                let ty = types.import(document.types(), input.ty());
                if !value.ty().is_coercible_to(types, &ty) {
                    bail!(
                        "expected type `{expected}` for input `{path}`, but found type `{actual}`",
                        expected = ty.display(types),
                        actual = value.ty().display(types)
                    );
                }
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

/// Represents inputs to a workflow.
#[derive(Default, Debug, Clone)]
pub struct WorkflowInputs {
    /// The workflow input values.
    inputs: HashMap<String, Value>,
    /// The nested call inputs.
    calls: HashMap<String, Inputs>,
}

impl WorkflowInputs {
    /// Gets the inputs to the workflow.
    pub fn inputs(&self) -> &HashMap<String, Value> {
        &self.inputs
    }

    /// Gets the nested call inputs.
    pub fn calls(&self) -> &HashMap<String, Inputs> {
        &self.calls
    }

    /// Sets a workflow input.
    pub fn set_input(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.inputs.insert(name.into(), value.into());
    }

    /// Sets a nested call inputs.
    pub fn set_call_inputs(&mut self, name: impl Into<String>, inputs: impl Into<Inputs>) {
        self.calls.insert(name.into(), inputs.into());
    }

    /// Validates the inputs for the given workflow.
    pub fn validate(
        &self,
        types: &mut Types,
        document: &Document,
        workflow: &Workflow,
    ) -> Result<()> {
        // Start by validating all the specified inputs and their types
        for (name, value) in &self.inputs {
            let input = workflow
                .inputs()
                .get(name)
                .ok_or_else(|| anyhow!("unknown input `{name}`"))?;
            let expected_ty = types.import(document.types(), input.ty());
            let ty = value.ty();
            if !ty.is_coercible_to(types, &expected_ty) {
                bail!(
                    "expected type `{expected_ty}` for input `{name}`, but found type `{ty}`",
                    expected_ty = expected_ty.display(types),
                    ty = ty.display(types)
                );
            }
        }

        // Next check for missing required inputs
        for (name, input) in workflow.inputs() {
            if input.required() && !self.inputs.contains_key(name) {
                bail!("missing required input `{name}`");
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
            let call = workflow
                .calls()
                .get(name)
                .ok_or_else(|| anyhow!("unknown call `{name}`"))?;

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

                    let task_inputs = inputs.as_task_inputs().ok_or_else(|| {
                        anyhow!("`{name}` is a call to a task, but workflow inputs were supplied")
                    })?;

                    task_inputs.validate(types, document, task)?;
                    task_inputs.inputs()
                }
                CallKind::Workflow => {
                    let workflow = document.workflow().expect("should have a workflow");
                    assert_eq!(
                        workflow.name(),
                        call.name(),
                        "call name does not match workflow name"
                    );
                    let workflow_inputs = inputs.as_workflow_inputs().ok_or_else(|| {
                        anyhow!("`{name}` is a call to a workflow, but task inputs were supplied")
                    })?;

                    workflow_inputs.validate(types, document, workflow)?;
                    workflow_inputs.inputs()
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
                    if !inputs
                        .map(|i| i.inputs().contains_key(input))
                        .unwrap_or(false)
                    {
                        bail!("missing required input `{input}` for call `{call}`");
                    }
                }
            }
        }

        Ok(())
    }

    /// Sets a value with dotted path notation.
    fn set_path_value(
        &mut self,
        types: &mut Types,
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
                let call = workflow.calls().get(name).ok_or_else(|| {
                    anyhow!(
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
                            .set_path_value(types, document, task, remainder, value)
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
                            .set_path_value(types, document, workflow, remainder, value)
                    }
                }
            }
            None => {
                let input = workflow.inputs().get(path).ok_or_else(|| {
                    anyhow!(
                        "workflow `{workflow}` does not have an input named `{path}`",
                        workflow = workflow.name()
                    )
                })?;

                let ty = types.import(document.types(), input.ty());
                if !value.ty().is_coercible_to(types, &ty) {
                    bail!(
                        "expected type `{expected}` for input `{path}`, but found type `{actual}`",
                        expected = ty.display(types),
                        actual = value.ty().display(types)
                    );
                }
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

/// Represents inputs to a WDL workflow or task.
#[derive(Debug, Clone)]
pub enum Inputs {
    /// The inputs are to a task.
    Task(TaskInputs),
    /// The inputs are to a workflow.
    Workflow(WorkflowInputs),
}

impl Inputs {
    /// Gets the input values.
    pub fn inputs(&self) -> &HashMap<String, Value> {
        match self {
            Self::Task(t) => &t.inputs,
            Self::Workflow(w) => &w.inputs,
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

/// Represents a WDL JSON inputs file.
///
/// The expected file format is described in the [WDL specification][1].
///
/// [1]: https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#json-input-format
pub struct InputsFile {
    /// The name of the task to evaluate.
    ///
    /// This is `None` for workflows.
    task: Option<String>,
    /// The inputs to the workflow or task.
    inputs: Inputs,
}

impl InputsFile {
    /// Parses a JSON inputs file from the given file path.
    ///
    /// The parse uses the provided document to validate the input keys within
    /// the file.
    pub fn parse(types: &mut Types, document: &Document, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).with_context(|| {
            format!("failed to open input file `{path}`", path = path.display())
        })?;

        // Parse the JSON (should be an object)
        let reader = BufReader::new(file);
        let object = mem::take(
            serde_json::from_reader::<_, JsonValue>(reader)
                .with_context(|| {
                    format!("failed to parse input file `{path}`", path = path.display())
                })?
                .as_object_mut()
                .ok_or_else(|| {
                    anyhow!(
                        "expected input file `{path}` to contain a JSON object",
                        path = path.display()
                    )
                })?,
        );

        Self::parse_object(types, document, object)
            .with_context(|| format!("failed to parse input file `{path}`", path = path.display()))
    }

    /// Gets the file as inputs to a task.
    ///
    /// Returns `None` if the inputs are to a workflow.
    pub fn as_task_inputs(&self) -> Option<(&str, &TaskInputs)> {
        match &self.inputs {
            Inputs::Task(inputs) => {
                Some((self.task.as_deref().expect("should have task name"), inputs))
            }
            Inputs::Workflow(_) => None,
        }
    }

    /// Gets the file as inputs to a workflow.
    ///
    /// Returns `None` if the inputs are to a task.
    pub fn as_workflow_inputs(&self) -> Option<&WorkflowInputs> {
        match &self.inputs {
            Inputs::Task(_) => None,
            Inputs::Workflow(inputs) => Some(inputs),
        }
    }

    /// Parses the root object in an input file.
    fn parse_object(types: &mut Types, document: &Document, object: JsonMap) -> Result<Self> {
        // Determine the root workflow or task name
        let (key, name) = match object.iter().next() {
            Some((key, _)) => match key.split_once('.') {
                Some((name, _)) => (key, name),
                None => {
                    bail!(
                        "invalid input key `{key}`: expected the value to be prefixed with the \
                         workflow or task name",
                    )
                }
            },
            // If the object is empty, treat it as a workflow evaluation without any inputs
            None => {
                return Ok(Self {
                    task: None,
                    inputs: Inputs::Workflow(Default::default()),
                });
            }
        };

        match (document.task_by_name(name), document.workflow()) {
            (Some(task), _) => Self::parse_task_inputs(types, document, task, object),
            (None, Some(workflow)) if workflow.name() == name => {
                Self::parse_workflow_inputs(types, document, workflow, object)
            }
            _ => bail!(
                "invalid input key `{key}`: a task or workflow named `{name}` does not exist in \
                 the document"
            ),
        }
    }

    /// Parses the inputs for a task.
    fn parse_task_inputs(
        types: &mut Types,
        document: &Document,
        task: &Task,
        object: JsonMap,
    ) -> Result<Self> {
        let mut inputs = TaskInputs::default();
        for (key, value) in object {
            let value = Value::from_json(types, value)
                .with_context(|| format!("invalid input key `{key}`"))?;

            match key.split_once(".") {
                Some((prefix, remainder)) if prefix == task.name() => {
                    inputs
                        .set_path_value(types, document, task, remainder, value)
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

        Ok(Self {
            task: Some(task.name().to_string()),
            inputs: Inputs::Task(inputs),
        })
    }

    /// Parses the inputs for a workflow.
    fn parse_workflow_inputs(
        types: &mut Types,
        document: &Document,
        workflow: &Workflow,
        object: JsonMap,
    ) -> Result<Self> {
        let mut inputs = WorkflowInputs::default();
        for (key, value) in object {
            let value = Value::from_json(types, value)
                .with_context(|| format!("invalid input key `{key}`"))?;

            match key.split_once(".") {
                Some((prefix, remainder)) if prefix == workflow.name() => {
                    inputs
                        .set_path_value(types, document, workflow, remainder, value)
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

        Ok(Self {
            task: None,
            inputs: Inputs::Workflow(inputs),
        })
    }
}
