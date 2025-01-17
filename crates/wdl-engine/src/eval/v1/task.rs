//! Implementation of evaluation for V1 tasks.

use std::collections::HashMap;
use std::mem;
use std::path::Path;

use anyhow::Context;
use anyhow::anyhow;
use petgraph::Direction;
use petgraph::Graph;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use tracing::debug;
use tracing::info;
use tracing::warn;
use wdl_analysis::diagnostics::multiple_type_mismatch;
use wdl_analysis::diagnostics::unknown_name;
use wdl_analysis::document::Document;
use wdl_analysis::document::TASK_VAR_NAME;
use wdl_analysis::document::Task;
use wdl_analysis::eval::v1::TaskGraphBuilder;
use wdl_analysis::eval::v1::TaskGraphNode;
use wdl_analysis::types::Optional;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::task_hint_types;
use wdl_analysis::types::v1::task_requirement_types;
use wdl_ast::Ast;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::SupportedVersion;
use wdl_ast::TokenStrHash;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Decl;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::StrippedCommandPart;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::version::V1;

use crate::Coercible;
use crate::EvaluationContext;
use crate::EvaluationResult;
use crate::Outputs;
use crate::Scope;
use crate::ScopeIndex;
use crate::ScopeRef;
use crate::TaskExecution;
use crate::TaskExecutionBackend;
use crate::TaskInputs;
use crate::TaskValue;
use crate::Value;
use crate::diagnostics::output_evaluation_failed;
use crate::diagnostics::runtime_type_mismatch;
use crate::eval::EvaluatedTask;
use crate::v1::ExprEvaluator;

/// The index of a task's root scope.
const ROOT_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(0);
/// The index of a task's output scope.
const OUTPUT_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(1);
/// The index of the evaluation scope where the WDL 1.2 `task` variable is
/// visible.
const TASK_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(2);

/// Used to evaluate expressions in tasks.
struct TaskEvaluationContext<'a, 'b> {
    /// The associated evaluation state.
    state: &'a State<'b>,
    /// The current evaluation scope.
    scope: ScopeIndex,
    /// The standard out value to use.
    stdout: Option<&'a Value>,
    /// The standard error value to use.
    stderr: Option<&'a Value>,
    /// Whether or not the evaluation has associated task information.
    ///
    /// This is `true` when evaluating hints sections.
    task: bool,
}

impl<'a, 'b> TaskEvaluationContext<'a, 'b> {
    /// Constructs a new expression evaluation context.
    pub fn new(state: &'a State<'b>, scope: ScopeIndex) -> Self {
        Self {
            state,
            scope,
            stdout: None,
            stderr: None,
            task: false,
        }
    }

    /// Sets the stdout value to use for the evaluation context.
    pub fn with_stdout(mut self, stdout: &'a Value) -> Self {
        self.stdout = Some(stdout);
        self
    }

    /// Sets the stderr value to use for the evaluation context.
    pub fn with_stderr(mut self, stderr: &'a Value) -> Self {
        self.stderr = Some(stderr);
        self
    }

    /// Marks the evaluation as having associated task information.
    ///
    /// This is used in evaluating hints sections.
    pub fn with_task(mut self) -> Self {
        self.task = true;
        self
    }
}

impl EvaluationContext for TaskEvaluationContext<'_, '_> {
    fn version(&self) -> SupportedVersion {
        self.state
            .document
            .version()
            .expect("document should have a version")
    }

    fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic> {
        ScopeRef::new(&self.state.scopes, self.scope)
            .lookup(name.as_str())
            .cloned()
            .ok_or_else(|| unknown_name(name.as_str(), name.span()))
    }

    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
        crate::resolve_type_name(self.state.document, name)
    }

    fn work_dir(&self) -> &Path {
        self.state.execution.work_dir()
    }

    fn temp_dir(&self) -> &Path {
        self.state.execution.temp_dir()
    }

    fn stdout(&self) -> Option<&Value> {
        self.stdout
    }

    fn stderr(&self) -> Option<&Value> {
        self.stderr
    }

    fn task(&self) -> Option<&Task> {
        if self.task {
            Some(self.state.task)
        } else {
            None
        }
    }
}

/// Represents task evaluation state.
struct State<'a> {
    /// The document containing the workflow being evaluated.
    document: &'a Document,
    /// The task being evaluated.
    task: &'a Task,
    /// The scopes of the task being evaluated.
    ///
    /// The first scope is the root scope, the second is the output scope, and
    /// the third is the scope where the "task" variable is visible in 1.2+
    /// evaluations.
    scopes: [Scope; 3],
    /// The task execution.
    execution: Box<dyn TaskExecution>,
    /// The evaluated command for the task.
    command: String,
    /// The evaluated requirements for the task.
    requirements: HashMap<String, Value>,
    /// The evaluated hints for the task.
    hints: HashMap<String, Value>,
}

impl<'a> State<'a> {
    /// Constructs a new task evaluation state.
    fn new(document: &'a Document, task: &'a Task, execution: Box<dyn TaskExecution>) -> Self {
        // Tasks have a root scope (index 0), an output scope (index 1), and a `task`
        // variable scope (index 2). The output scope inherits from the root scope and
        // the task scope inherits from the output scope. Inputs and private
        // declarations are evaluated into the root scope. Outputs are evaluated into
        // the output scope. The task scope is used for evaluating expressions in both
        // the command and output sections. Only the `task` variable in WDL 1.2 is
        // introduced into the task scope; in previous WDL versions, the task scope will
        // not have any local names.
        let scopes = [
            Scope::default(),
            Scope::new(ROOT_SCOPE_INDEX),
            Scope::new(OUTPUT_SCOPE_INDEX),
        ];

        Self {
            document,
            task,
            scopes,
            execution,
            command: Default::default(),
            requirements: Default::default(),
            hints: Default::default(),
        }
    }
}

/// Represents a WDL V1 task evaluator.
pub struct TaskEvaluator<'a> {
    /// The associated task execution backend.
    backend: &'a dyn TaskExecutionBackend,
}

impl<'a> TaskEvaluator<'a> {
    /// Constructs a new task evaluator with the given backend.
    pub fn new(backend: &'a dyn TaskExecutionBackend) -> Self {
        Self { backend }
    }

    /// Evaluates the given task.
    ///
    /// Upon success, returns the evaluated task.
    pub async fn evaluate(
        &mut self,
        document: &Document,
        task: &Task,
        inputs: &TaskInputs,
        root: &Path,
        id: &str,
    ) -> EvaluationResult<EvaluatedTask> {
        // Return the first error analysis diagnostic if there was one
        // With this check, we can assume certain correctness properties of the document
        if let Some(diagnostic) = document
            .diagnostics()
            .iter()
            .find(|d| d.severity() == Severity::Error)
        {
            return Err(diagnostic.clone().into());
        }

        inputs.validate(document, task, None).with_context(|| {
            format!(
                "failed to validate the inputs to task `{task}`",
                task = task.name()
            )
        })?;

        let ast = match document.node().ast() {
            Ast::V1(ast) => ast,
            _ => {
                return Err(
                    anyhow!("task evaluation is only supported for WDL 1.x documents").into(),
                );
            }
        };

        // Find the task in the AST
        let definition = ast
            .tasks()
            .find(|t| t.name().as_str() == task.name())
            .expect("task should exist in the AST");

        let version = document.version().expect("document should have version");

        // Build an evaluation graph for the task
        let mut diagnostics = Vec::new();
        let graph = TaskGraphBuilder::default().build(version, &definition, &mut diagnostics);
        if let Some(diagnostic) = diagnostics.pop() {
            return Err(diagnostic.into());
        }

        info!(
            "evaluating task `{task}` in `{uri}`",
            task = task.name(),
            uri = document.uri()
        );

        let mut state = State::new(document, task, self.backend.create_execution(root)?);
        let mut envs = Vec::new();
        let nodes = toposort(&graph, None).expect("graph should be acyclic");
        let mut current = 0;
        while current < nodes.len() {
            match &graph[nodes[current]] {
                TaskGraphNode::Input(decl) => {
                    let value = self.evaluate_input(&mut state, decl, inputs)?;
                    if decl.env().is_some() {
                        envs.push((
                            decl.name().as_str().to_string(),
                            value
                                .as_primitive()
                                .expect("value should be primitive")
                                .raw()
                                .to_string(),
                        ));
                    }
                }
                TaskGraphNode::Decl(decl) => {
                    let value = self.evaluate_decl(&mut state, decl)?;
                    if decl.env().is_some() {
                        envs.push((
                            decl.name().as_str().to_string(),
                            value
                                .as_primitive()
                                .expect("value should be primitive")
                                .raw()
                                .to_string(),
                        ));
                    }
                }
                TaskGraphNode::Output(_) => {
                    // Stop at the first output; at this point the task can be executed
                    break;
                }
                TaskGraphNode::Command(section) => {
                    assert!(state.command.is_empty());

                    // Get the execution constraints
                    let constraints = state
                        .execution
                        .constraints(&state.requirements, &state.hints)
                        .with_context(|| {
                            format!("failed to execute task `{task}`", task = task.name())
                        })?;

                    // Introduce the task variable at this point; valid for both the command
                    // section and the outputs section
                    if version >= SupportedVersion::V1(V1::Two) {
                        let task = TaskValue::new_v1(task.name(), id, &definition, constraints);
                        state.scopes[TASK_SCOPE_INDEX.0].insert(TASK_VAR_NAME, Value::Task(task));
                    }

                    // Map any paths needed for command evaluation
                    let mapped_paths = Self::map_command_paths(
                        &graph,
                        nodes[current],
                        ScopeRef::new(&state.scopes, TASK_SCOPE_INDEX),
                        &mut state.execution,
                    );

                    self.evaluate_command(&mut state, section, &mapped_paths)?;
                }
                TaskGraphNode::Runtime(section) => {
                    assert!(
                        state.requirements.is_empty(),
                        "requirements should not have been evaluated"
                    );
                    assert!(
                        state.hints.is_empty(),
                        "hints should not have been evaluated"
                    );

                    self.evaluate_runtime_section(&mut state, section, inputs)?;
                }
                TaskGraphNode::Requirements(section) => {
                    assert!(
                        state.requirements.is_empty(),
                        "requirements should not have been evaluated"
                    );
                    self.evaluate_requirements_section(&mut state, section, inputs)?;
                }
                TaskGraphNode::Hints(section) => {
                    assert!(
                        state.hints.is_empty(),
                        "hints should not have been evaluated"
                    );
                    self.evaluate_hints_section(&mut state, section, inputs)?;
                }
            }

            current += 1;
        }

        // TODO: check call cache for a hit. if so, skip task execution and use cache
        // paths for output evaluation

        let status_code = state
            .execution
            .spawn(&state.command, &state.requirements, &state.hints, &envs)?
            .await?;

        // TODO: support retrying the task if it fails

        let mut evaluated = EvaluatedTask::new(state.execution.as_ref(), status_code)?;

        // Update the task variable's return code
        if version >= SupportedVersion::V1(V1::Two) {
            let task = state.scopes[TASK_SCOPE_INDEX.0]
                .get_mut(TASK_VAR_NAME)
                .unwrap()
                .as_task_mut()
                .unwrap();
            task.set_return_code(evaluated.status_code);
        }

        evaluated.outputs = {
            evaluated.handle_exit(&state.requirements)?;

            for index in &nodes[current..] {
                match &graph[*index] {
                    TaskGraphNode::Output(decl) => {
                        self.evaluate_output(&mut state, decl, &evaluated)?;
                    }
                    TaskGraphNode::Input(decl) => {
                        self.evaluate_input(&mut state, decl, inputs)?;
                    }
                    TaskGraphNode::Decl(decl) => {
                        self.evaluate_decl(&mut state, decl)?;
                    }
                    _ => {
                        unreachable!("only declarations should be evaluated after the command")
                    }
                }
            }

            let mut outputs: Outputs = mem::take(&mut state.scopes[OUTPUT_SCOPE_INDEX.0]).into();
            if let Some(section) = definition.output() {
                let indexes: HashMap<_, _> = section
                    .declarations()
                    .enumerate()
                    .map(|(i, d)| (TokenStrHash::new(d.name()), i))
                    .collect();
                outputs.sort_by(move |a, b| indexes[a].cmp(&indexes[b]))
            }

            Ok(outputs)
        };

        Ok(evaluated)
    }

    /// Evaluates a task input.
    fn evaluate_input(
        &mut self,
        state: &mut State<'_>,
        decl: &Decl,
        inputs: &TaskInputs,
    ) -> EvaluationResult<Value> {
        let name = decl.name();
        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;

        let (value, span) = match inputs.get(name.as_str()) {
            Some(input) => (input.clone(), name.span()),
            None => {
                if let Some(expr) = decl.expr() {
                    debug!(
                        "evaluating input `{name}` for task `{task}` in `{uri}`",
                        name = name.as_str(),
                        task = state.task.name(),
                        uri = state.document.uri(),
                    );

                    let mut evaluator =
                        ExprEvaluator::new(TaskEvaluationContext::new(state, ROOT_SCOPE_INDEX));
                    let value = evaluator.evaluate_expr(&expr)?;
                    (value, expr.span())
                } else {
                    assert!(decl.ty().is_optional(), "type should be optional");
                    (Value::None, name.span())
                }
            }
        };

        let value = value
            .coerce(&ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), span))?;
        state.scopes[ROOT_SCOPE_INDEX.0].insert(name.as_str(), value.clone());
        Ok(value)
    }

    /// Evaluates a task private declaration.
    fn evaluate_decl(&mut self, state: &mut State<'_>, decl: &Decl) -> EvaluationResult<Value> {
        let name = decl.name();
        debug!(
            "evaluating private declaration `{name}` for task `{task}` in `{uri}`",
            name = name.as_str(),
            task = state.task.name(),
            uri = state.document.uri(),
        );

        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;

        let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(state, ROOT_SCOPE_INDEX));

        let expr = decl.expr().expect("private decls should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;
        let value = value
            .coerce(&ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;
        state.scopes[ROOT_SCOPE_INDEX.0].insert(name.as_str(), value.clone());
        Ok(value)
    }

    /// Evaluates the runtime section.
    fn evaluate_runtime_section(
        &mut self,
        state: &mut State<'_>,
        section: &RuntimeSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<()> {
        debug!(
            "evaluating runtimes section for task `{task}` in `{uri}`",
            task = state.task.name(),
            uri = state.document.uri()
        );

        let version = state
            .document
            .version()
            .expect("document should have version");
        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.requirement(name.as_str()) {
                state
                    .requirements
                    .insert(name.as_str().to_string(), value.clone());
                continue;
            } else if let Some(value) = inputs.hint(name.as_str()) {
                state.hints.insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(state, ROOT_SCOPE_INDEX));

            let (types, requirement) = match task_requirement_types(version, name.as_str()) {
                Some(types) => (Some(types), true),
                None => match task_hint_types(version, name.as_str(), false) {
                    Some(types) => (Some(types), false),
                    None => (None, false),
                },
            };

            // Evaluate and coerce to the expected type
            let expr = item.expr();
            let mut value = evaluator.evaluate_expr(&expr)?;
            if let Some(types) = types {
                value = types
                    .iter()
                    .find_map(|ty| value.coerce(ty).ok())
                    .ok_or_else(|| {
                        multiple_type_mismatch(types, name.span(), &value.ty(), expr.span())
                    })?;
            }

            if requirement {
                state.requirements.insert(name.as_str().to_string(), value);
            } else {
                state.hints.insert(name.as_str().to_string(), value);
            }
        }

        Ok(())
    }

    /// Evaluates the requirements section.
    fn evaluate_requirements_section(
        &mut self,
        state: &mut State<'_>,
        section: &RequirementsSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<()> {
        debug!(
            "evaluating requirements section for task `{task}` in `{uri}`",
            task = state.task.name(),
            uri = state.document.uri()
        );

        let version = state
            .document
            .version()
            .expect("document should have version");
        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.requirement(name.as_str()) {
                state
                    .requirements
                    .insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(state, ROOT_SCOPE_INDEX));

            let types = task_requirement_types(version, name.as_str())
                .expect("requirement should be known");

            // Evaluate and coerce to the expected type
            let expr = item.expr();
            let value = evaluator.evaluate_expr(&expr)?;
            let value = types
                .iter()
                .find_map(|ty| value.coerce(ty).ok())
                .ok_or_else(|| {
                    multiple_type_mismatch(types, name.span(), &value.ty(), expr.span())
                })?;

            state.requirements.insert(name.as_str().to_string(), value);
        }

        Ok(())
    }

    /// Evaluates the hints section.
    fn evaluate_hints_section(
        &mut self,
        state: &mut State<'_>,
        section: &TaskHintsSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<()> {
        debug!(
            "evaluating hints section for task `{task}` in `{uri}`",
            task = state.task.name(),
            uri = state.document.uri()
        );

        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.hint(name.as_str()) {
                state.hints.insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(state, ROOT_SCOPE_INDEX).with_task());

            let value = evaluator.evaluate_hints_item(&name, &item.expr())?;
            state.hints.insert(name.as_str().to_string(), value);
        }

        Ok(())
    }

    /// Evaluates the command of a task.
    fn evaluate_command(
        &mut self,
        state: &mut State<'_>,
        section: &CommandSection,
        mapped_paths: &HashMap<String, String>,
    ) -> EvaluationResult<()> {
        debug!(
            "evaluating command section for task `{task}` in `{uri}`",
            task = state.task.name(),
            uri = state.document.uri()
        );

        let mut command = String::new();
        if let Some(parts) = section.strip_whitespace() {
            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(state, TASK_SCOPE_INDEX));

            for part in parts {
                match part {
                    StrippedCommandPart::Text(t) => {
                        command.push_str(t.as_str());
                    }
                    StrippedCommandPart::Placeholder(placeholder) => {
                        evaluator.evaluate_placeholder(&placeholder, &mut command, mapped_paths)?;
                    }
                }
            }
        } else {
            warn!(
                "command for task `{task}` in `{uri}` has mixed indentation; whitespace stripping \
                 was skipped",
                task = state.task.name(),
                uri = state.document.uri(),
            );

            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(state, TASK_SCOPE_INDEX));

            let heredoc = section.is_heredoc();
            for part in section.parts() {
                match part {
                    CommandPart::Text(t) => {
                        t.unescape_to(heredoc, &mut command);
                    }
                    CommandPart::Placeholder(placeholder) => {
                        evaluator.evaluate_placeholder(&placeholder, &mut command, mapped_paths)?;
                    }
                }
            }
        }

        state.command = command;

        Ok(())
    }

    /// Evaluates a task output.
    fn evaluate_output(
        &mut self,
        state: &mut State<'_>,
        decl: &Decl,
        evaluated: &EvaluatedTask,
    ) -> EvaluationResult<()> {
        let name = decl.name();
        debug!(
            "evaluating output `{name}` for task `{task}` in `{uri}`",
            name = name.as_str(),
            task = state.task.name(),
            uri = state.document.uri()
        );

        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;
        let mut evaluator = ExprEvaluator::new(
            TaskEvaluationContext::new(state, TASK_SCOPE_INDEX)
                .with_stdout(&evaluated.stdout)
                .with_stderr(&evaluated.stderr),
        );

        let expr = decl.expr().expect("outputs should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;

        // First coerce the output value to the expected type
        let mut value = value
            .coerce(&ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;

        // Finally, join the path with the working directory, checking for existence
        value
            .join_paths(&evaluated.work_dir, true, ty.is_optional())
            .map_err(|e| output_evaluation_failed(e, state.task.name(), true, &name))?;

        state.scopes[OUTPUT_SCOPE_INDEX.0].insert(name.as_str(), value);
        Ok(())
    }

    /// Maps any host paths referenced by a command to a corresponding guest
    /// path.
    fn map_command_paths(
        graph: &Graph<TaskGraphNode, ()>,
        index: NodeIndex,
        scope: ScopeRef<'_>,
        execution: &mut Box<dyn TaskExecution>,
    ) -> HashMap<String, String> {
        let mut mapped_paths = HashMap::new();
        for edge in graph.edges_directed(index, Direction::Incoming) {
            match &graph[edge.source()] {
                TaskGraphNode::Input(decl) | TaskGraphNode::Decl(decl) => {
                    scope
                        .lookup(decl.name().as_str())
                        .expect("declaration should be in scope")
                        .visit_paths(&mut |path| {
                            if !mapped_paths.contains_key(path) {
                                if let Some(guest) = execution.map_path(Path::new(path)) {
                                    debug!(
                                        "host path `{path}` mapped to guest path `{guest}`",
                                        guest = guest.display()
                                    );

                                    mapped_paths.insert(
                                        path.to_string(),
                                        guest
                                            .into_os_string()
                                            .into_string()
                                            .expect("mapped path should be UTF-8"),
                                    );
                                }
                            }
                        });
                }
                _ => continue,
            }
        }

        mapped_paths
    }
}
