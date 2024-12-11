//! Module for evaluation.

use std::collections::HashMap;
use std::path::MAIN_SEPARATOR;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use indexmap::IndexMap;
use wdl_analysis::document::Task;
use wdl_analysis::types::Type;
use wdl_analysis::types::Types;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;

use crate::CompoundValue;
use crate::Outputs;
use crate::PrimitiveValue;
use crate::TaskExecution;
use crate::Value;

pub mod v1;

/// Represents an error that may occur when evaluating a workflow or task.
#[derive(Debug)]
pub enum EvaluationError {
    /// The error came from WDL source evaluation.
    Source(Diagnostic),
    /// The error came from another source.
    Other(anyhow::Error),
}

impl From<Diagnostic> for EvaluationError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::Source(diagnostic)
    }
}

impl From<anyhow::Error> for EvaluationError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

/// Represents a result from evaluating a workflow or task.
pub type EvaluationResult<T> = Result<T, EvaluationError>;

/// Represents context to an expression evaluator.
pub trait EvaluationContext {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the types collection associated with the evaluation.
    fn types(&self) -> &Types;

    /// Gets the mutable types collection associated with the evaluation.
    fn types_mut(&mut self) -> &mut Types;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic>;

    /// Gets the working directory for the evaluation.
    fn work_dir(&self) -> &Path;

    /// Gets the temp directory for the evaluation.
    fn temp_dir(&self) -> &Path;

    /// Gets the value to return for a call to the `stdout` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stdout(&self) -> Option<&Value>;

    /// Gets the value to return for a call to the `stderr` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stderr(&self) -> Option<&Value>;

    /// Gets the task associated with the evaluation context.
    ///
    /// This is only `Some` when evaluating task hints sections.
    fn task(&self) -> Option<&Task>;

    /// Gets the types collection associated with the document being evaluated.
    fn document_types(&self) -> &Types;
}

/// Represents an index of a scope in a collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeIndex(usize);

impl From<usize> for ScopeIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl From<ScopeIndex> for usize {
    fn from(index: ScopeIndex) -> Self {
        index.0
    }
}

/// Represents an evaluation scope in a WDL document.
#[derive(Default, Debug)]
pub struct Scope {
    /// The index of the parent scope.
    ///
    /// This is `None` for task and workflow scopes.
    parent: Option<ScopeIndex>,
    /// The map of names in scope to their values.
    names: IndexMap<String, Value>,
}

impl Scope {
    /// Creates a new scope given the parent scope.
    pub fn new(parent: Option<ScopeIndex>) -> Self {
        Self {
            parent,
            names: Default::default(),
        }
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.names.insert(name.into(), value.into());
    }

    /// Gets a mutable reference to an existing name in scope.
    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.names.get_mut(name)
    }
}

impl From<Scope> for IndexMap<String, Value> {
    fn from(scope: Scope) -> Self {
        scope.names
    }
}

/// Represents a reference to a scope.
#[derive(Debug, Clone, Copy)]
pub struct ScopeRef<'a> {
    /// The reference to the scopes collection.
    scopes: &'a [Scope],
    /// The index of the scope in the collection.
    index: ScopeIndex,
}

impl<'a> ScopeRef<'a> {
    /// Creates a new scope reference given the scope index.
    pub fn new(scopes: &'a [Scope], index: impl Into<ScopeIndex>) -> Self {
        Self {
            scopes,
            index: index.into(),
        }
    }

    /// Gets the parent scope.
    ///
    /// Returns `None` if there is no parent scope.
    pub fn parent(&self) -> Option<Self> {
        self.scopes[self.index.0].parent.map(|p| Self {
            scopes: self.scopes,
            index: p,
        })
    }

    /// Gets all of the name and values available at this scope.
    pub fn names(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.scopes[self.index.0]
            .names
            .iter()
            .map(|(n, name)| (n.as_str(), name))
    }

    /// Gets the value of a name local to this scope.
    ///
    /// Returns `None` if a name local to this scope was not found.
    pub fn local(&self, name: &str) -> Option<&Value> {
        self.scopes[self.index.0].names.get(name)
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<&Value> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name) {
                return Some(name);
            }

            current = self.scopes[index.0].parent;
        }

        None
    }
}

/// Represents an evaluated task.
pub struct EvaluatedTask {
    /// The evaluated task's status code.
    status_code: i32,
    /// The working directory of the executed task.
    work_dir: PathBuf,
    /// The temp directory of the executed task.
    temp_dir: PathBuf,
    /// The command file of the executed task.
    command: PathBuf,
    /// The value to return from the `stdout` function.
    stdout: Value,
    /// The value to return from the `stderr` function.
    stderr: Value,
    /// The evaluated outputs of the task.
    ///
    /// This is `Ok` when the task executes successfully and all of the task's
    /// outputs evaluated without error.
    ///
    /// Otherwise, this contains the error that occurred while attempting to
    /// evaluate the task's outputs.
    outputs: EvaluationResult<Outputs>,
}

impl EvaluatedTask {
    /// Constructs a new evaluated task.
    ///
    /// Returns an error if the stdout or stderr paths are not UTF-8.
    fn new(execution: &dyn TaskExecution, status_code: i32) -> anyhow::Result<Self> {
        let stdout = PrimitiveValue::new_file(execution.stdout().to_str().with_context(|| {
            format!(
                "path to stdout file `{path}` is not UTF-8",
                path = execution.stdout().display()
            )
        })?)
        .into();
        let stderr = PrimitiveValue::new_file(execution.stderr().to_str().with_context(|| {
            format!(
                "path to stderr file `{path}` is not UTF-8",
                path = execution.stderr().display()
            )
        })?)
        .into();

        Ok(Self {
            status_code,
            work_dir: execution.work_dir().into(),
            temp_dir: execution.temp_dir().into(),
            command: execution.command().into(),
            stdout,
            stderr,
            outputs: Ok(Default::default()),
        })
    }

    /// Gets the status code of the evaluated task.
    pub fn status_code(&self) -> i32 {
        self.status_code
    }

    /// Gets the working directory of the evaluated task.
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Gets the temp directory of the evaluated task.
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// Gets the command file of the evaluated task.
    pub fn command(&self) -> &Path {
        &self.command
    }

    /// Gets the stdout value of the evaluated task.
    pub fn stdout(&self) -> &Value {
        &self.stdout
    }

    /// Gets the stderr value of the evaluated task.
    pub fn stderr(&self) -> &Value {
        &self.stderr
    }

    /// Gets the outputs of the evaluated task.
    ///
    /// This is `Ok` when the task executes successfully and all of the task's
    /// outputs evaluated without error.
    ///
    /// Otherwise, this contains the error that occurred while attempting to
    /// evaluate the task's outputs.
    pub fn outputs(&self) -> &EvaluationResult<Outputs> {
        &self.outputs
    }

    /// Converts the evaluated task into an evaluation result.
    ///
    /// Returns `Ok(_)` if the task outputs were evaluated.
    ///
    /// Returns `Err(_)` if the task outputs could not be evaluated.
    pub fn into_result(self) -> EvaluationResult<Outputs> {
        self.outputs
    }

    /// Handles the exit of a task execution.
    ///
    /// Returns an error if the task failed.
    fn handle_exit(&self, requirements: &HashMap<String, Value>) -> anyhow::Result<()> {
        let mut error = true;
        if let Some(return_codes) = requirements
            .get(TASK_REQUIREMENT_RETURN_CODES)
            .or_else(|| requirements.get(TASK_REQUIREMENT_RETURN_CODES_ALIAS))
        {
            match return_codes {
                Value::Primitive(PrimitiveValue::String(s)) if s.as_ref() == "*" => {
                    error = false;
                }
                Value::Primitive(PrimitiveValue::String(s)) => {
                    bail!(
                        "invalid return code value `{s}`: only `*` is accepted when the return \
                         code is specified as a string"
                    );
                }
                Value::Primitive(PrimitiveValue::Integer(ok)) => {
                    if self.status_code == i32::try_from(*ok).unwrap_or_default() {
                        error = false;
                    }
                }
                Value::Compound(CompoundValue::Array(codes)) => {
                    error = !codes.as_slice().iter().any(|v| {
                        v.as_integer()
                            .map(|i| i32::try_from(i).unwrap_or_default() == self.status_code)
                            .unwrap_or(false)
                    });
                }
                _ => unreachable!("unexpected return codes value"),
            }
        } else {
            error = self.status_code != 0;
        }

        if error {
            bail!(
                "task process has terminated with status code {code}; see the `stdout` and \
                 `stderr` files in execution directory `{dir}{MAIN_SEPARATOR}` for task command \
                 output",
                code = self.status_code,
                dir = Path::new(self.stderr.as_file().unwrap().as_str())
                    .parent()
                    .expect("parent should exist")
                    .display(),
            );
        }

        Ok(())
    }
}
