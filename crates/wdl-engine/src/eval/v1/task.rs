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
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_analysis::types::v1::task_hint_types;
use wdl_analysis::types::v1::task_requirement_types;
use wdl_ast::Ast;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::SupportedVersion;
use wdl_ast::ToSpan;
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
use crate::Engine;
use crate::EvaluationContext;
use crate::EvaluationResult;
use crate::Outputs;
use crate::Scope;
use crate::ScopeRef;
use crate::TaskExecution;
use crate::TaskInputs;
use crate::TaskValue;
use crate::Value;
use crate::diagnostics::missing_task_output;
use crate::diagnostics::runtime_type_mismatch;
use crate::eval::EvaluatedTask;
use crate::v1::ExprEvaluator;

/// The index of a task's root scope.
const ROOT_SCOPE_INDEX: usize = 0;
/// The index of a task's output scope.
const OUTPUT_SCOPE_INDEX: usize = 1;
/// The index of the evaluation scope where the WDL 1.2 `task` variable is
/// visible.
const TASK_SCOPE_INDEX: usize = 2;

/// Used to evaluate expressions in tasks.
struct TaskEvaluationContext<'a> {
    /// The associated evaluation engine.
    engine: &'a mut Engine,
    /// The document being evaluated.
    document: &'a Document,
    /// The working directory for the evaluation.
    work_dir: &'a Path,
    /// The temp directory for the evaluation.
    temp_dir: &'a Path,
    /// The current evaluation scope.
    scope: ScopeRef<'a>,
    /// The standard out value to use.
    stdout: Option<&'a Value>,
    /// The standard error value to use.
    stderr: Option<&'a Value>,
    /// The task associated with the evaluation.
    ///
    /// This is only `Some` when evaluating task hints sections.
    task: Option<&'a Task>,
}

impl EvaluationContext for TaskEvaluationContext<'_> {
    fn version(&self) -> SupportedVersion {
        self.document
            .version()
            .expect("document should have a version")
    }

    fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic> {
        self.scope
            .lookup(name.as_str())
            .cloned()
            .ok_or_else(|| unknown_name(name.as_str(), name.span()))
    }

    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
        self.engine.resolve_type_name(self.document, name)
    }

    fn work_dir(&self) -> &Path {
        self.work_dir
    }

    fn temp_dir(&self) -> &Path {
        self.temp_dir
    }

    fn stdout(&self) -> Option<&Value> {
        self.stdout
    }

    fn stderr(&self) -> Option<&Value> {
        self.stderr
    }

    fn task(&self) -> Option<&Task> {
        self.task
    }
}

impl<'a> TaskEvaluationContext<'a> {
    /// Constructs a new expression evaluation context.
    pub fn new(
        engine: &'a mut Engine,
        document: &'a Document,
        work_dir: &'a Path,
        temp_dir: &'a Path,
        scope: ScopeRef<'a>,
    ) -> Self {
        Self {
            engine,
            document,
            work_dir,
            temp_dir,
            scope,
            stdout: None,
            stderr: None,
            task: None,
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

    /// Sets the associated task for evaluation.
    ///
    /// This is used in evaluating hints sections.
    pub fn with_task(mut self, task: &'a Task) -> Self {
        self.task = Some(task);
        self
    }
}

/// Represents a WDL V1 task evaluator.
pub struct TaskEvaluator<'a> {
    /// The associated evaluation engine.
    engine: &'a mut Engine,
}

impl<'a> TaskEvaluator<'a> {
    /// Constructs a new task evaluator.
    pub fn new(engine: &'a mut Engine) -> Self {
        Self { engine }
    }

    /// Evaluates the given task.
    ///
    /// Upon success, returns the evaluated task.
    #[allow(clippy::redundant_closure_call)]
    pub async fn evaluate(
        &mut self,
        document: &'a Document,
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

        let mut execution = self.engine.backend().create_execution(root)?;
        match document.node().ast() {
            Ast::V1(ast) => {
                // Find the task in the AST
                let definition = ast
                    .tasks()
                    .find(|t| t.name().as_str() == task.name())
                    .expect("task should exist in the AST");

                let version = document.version().expect("document should have version");

                // Build an evaluation graph for the task
                let mut diagnostics = Vec::new();
                let graph =
                    TaskGraphBuilder::default().build(version, &definition, &mut diagnostics);

                if let Some(diagnostic) = diagnostics.pop() {
                    return Err(diagnostic.into());
                }

                info!(
                    "evaluating task `{task}` in `{uri}`",
                    task = task.name(),
                    uri = document.uri()
                );

                // Tasks have a root scope (index 0), an output scope (index 1), and a `task`
                // variable scope (index 2). The output scope inherits from the root scope and
                // the task scope inherits from the output scope. Inputs and private
                // declarations are evaluated into the root scope. Outputs are evaluated into
                // the output scope. The task scope is used for evaluating expressions in both
                // the command and output sections. Only the `task` variable in WDL 1.2 is
                // introduced into the task scope; in previous WDL versions, the task scope will
                // not have any local names.
                let mut scopes = [
                    Scope::new(None),
                    Scope::new(Some(ROOT_SCOPE_INDEX.into())),
                    Scope::new(Some(OUTPUT_SCOPE_INDEX.into())),
                ];
                let mut command = String::new();
                let mut requirements = None;
                let mut hints = None;

                let nodes = toposort(&graph, None).expect("graph should be acyclic");
                let mut current = 0;
                while current < nodes.len() {
                    match &graph[nodes[current]] {
                        TaskGraphNode::Input(decl) => {
                            self.evaluate_input(
                                document,
                                execution.as_ref(),
                                &mut scopes,
                                task,
                                decl,
                                inputs,
                            )?;
                        }
                        TaskGraphNode::Decl(decl) => {
                            self.evaluate_decl(
                                document,
                                execution.as_ref(),
                                &mut scopes,
                                task,
                                decl,
                            )?;
                        }
                        TaskGraphNode::Output(_) => {
                            // Stop at the first output; at this point the task can be executed
                            break;
                        }
                        TaskGraphNode::Command(section) => {
                            assert!(command.is_empty());

                            // Get the execution constraints
                            let empty = Default::default();
                            let constraints = execution
                                .constraints(
                                    self.engine,
                                    requirements.as_ref().unwrap_or(&empty),
                                    hints.as_ref().unwrap_or(&empty),
                                )
                                .with_context(|| {
                                    format!("failed to execute task `{task}`", task = task.name())
                                })?;

                            // Introduce the task variable at this point; valid for both the command
                            // section and the outputs section
                            if version >= SupportedVersion::V1(V1::Two) {
                                let task =
                                    TaskValue::new_v1(task.name(), id, &definition, constraints);
                                scopes[TASK_SCOPE_INDEX].insert(TASK_VAR_NAME, Value::Task(task));
                            }

                            // Map any paths needed for command evaluation
                            let mapped_paths = Self::map_command_paths(
                                &graph,
                                nodes[current],
                                ScopeRef::new(&scopes, TASK_SCOPE_INDEX),
                                &mut execution,
                            );

                            command = self.evaluate_command(
                                document,
                                execution.as_mut(),
                                &scopes,
                                task,
                                section,
                                &mapped_paths,
                            )?;
                        }
                        TaskGraphNode::Runtime(section) => {
                            assert!(
                                requirements.is_none(),
                                "requirements should not have been evaluated"
                            );
                            assert!(hints.is_none(), "hints should not have been evaluated");

                            let (r, h) = self.evaluate_runtime_section(
                                document,
                                execution.as_ref(),
                                &scopes,
                                task,
                                section,
                                inputs,
                            )?;

                            requirements = Some(r);
                            hints = Some(h);
                        }
                        TaskGraphNode::Requirements(section) => {
                            assert!(
                                requirements.is_none(),
                                "requirements should not have been evaluated"
                            );
                            requirements = Some(self.evaluate_requirements_section(
                                document,
                                execution.as_ref(),
                                &scopes,
                                task,
                                section,
                                inputs,
                            )?);
                        }
                        TaskGraphNode::Hints(section) => {
                            assert!(hints.is_none(), "hints should not have been evaluated");
                            hints = Some(self.evaluate_hints_section(
                                document,
                                execution.as_ref(),
                                &scopes,
                                task,
                                section,
                                inputs,
                            )?);
                        }
                    }

                    current += 1;
                }

                let requirements = requirements.unwrap_or_default();
                let hints = hints.unwrap_or_default();

                // TODO: check call cache for a hit. if so, skip task execution and use cache
                // paths for output evaluation

                let status_code = execution.spawn(command, &requirements, &hints)?.await?;

                // TODO: support retrying the task if it fails

                let mut evaluated = EvaluatedTask::new(execution.as_ref(), status_code)?;

                // Update the task variable's return code
                if version >= SupportedVersion::V1(V1::Two) {
                    let task = scopes[TASK_SCOPE_INDEX]
                        .get_mut(TASK_VAR_NAME)
                        .unwrap()
                        .as_task_mut()
                        .unwrap();
                    task.set_return_code(evaluated.status_code);
                }

                // Use a closure that returns an evaluation result for evaluating the outputs
                let mut outputs = || -> EvaluationResult<Outputs> {
                    evaluated.handle_exit(&requirements)?;

                    for index in &nodes[current..] {
                        match &graph[*index] {
                            TaskGraphNode::Output(decl) => {
                                self.evaluate_output(
                                    document,
                                    &mut scopes,
                                    task,
                                    decl,
                                    &evaluated,
                                )?;
                            }
                            TaskGraphNode::Input(decl) => {
                                self.evaluate_input(
                                    document,
                                    execution.as_ref(),
                                    &mut scopes,
                                    task,
                                    decl,
                                    inputs,
                                )?;
                            }
                            TaskGraphNode::Decl(decl) => {
                                self.evaluate_decl(
                                    document,
                                    execution.as_ref(),
                                    &mut scopes,
                                    task,
                                    decl,
                                )?;
                            }
                            _ => {
                                unreachable!(
                                    "only declarations should be evaluated after the command"
                                )
                            }
                        }
                    }

                    let mut outputs: Outputs = mem::take(&mut scopes[OUTPUT_SCOPE_INDEX]).into();
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

                evaluated.outputs = outputs();
                Ok(evaluated)
            }
            _ => Err(anyhow!("document is not a 1.x document").into()),
        }
    }

    /// Evaluates a task input.
    fn evaluate_input(
        &mut self,
        document: &Document,
        execution: &dyn TaskExecution,
        scopes: &mut [Scope],
        task: &Task,
        decl: &Decl,
        inputs: &TaskInputs,
    ) -> EvaluationResult<()> {
        let name = decl.name();
        let decl_ty = decl.ty();
        let ty = self.convert_ast_type(document, &decl_ty)?;

        let (value, span) = match inputs.get(name.as_str()) {
            Some(input) => (input.clone(), name.span()),
            None => {
                if let Some(expr) = decl.expr() {
                    debug!(
                        "evaluating input `{name}` for task `{task}` in `{uri}`",
                        name = name.as_str(),
                        task = task.name(),
                        uri = document.uri(),
                    );

                    let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(
                        self.engine,
                        document,
                        execution.work_dir(),
                        execution.temp_dir(),
                        ScopeRef::new(scopes, ROOT_SCOPE_INDEX),
                    ));
                    let value = evaluator.evaluate_expr(&expr)?;
                    (value, expr.span())
                } else {
                    assert!(decl.ty().is_optional(), "type should be optional");
                    (Value::None, name.span())
                }
            }
        };

        let value = value.coerce(&ty).map_err(|e| {
            runtime_type_mismatch(
                e,
                &ty,
                decl_ty.syntax().text_range().to_span(),
                &value.ty(),
                span,
            )
        })?;
        scopes[ROOT_SCOPE_INDEX].insert(name.as_str(), value);
        Ok(())
    }

    /// Evaluates a task private declaration.
    fn evaluate_decl(
        &mut self,
        document: &Document,
        execution: &dyn TaskExecution,
        scopes: &mut [Scope],
        task: &Task,
        decl: &Decl,
    ) -> EvaluationResult<()> {
        let name = decl.name();
        debug!(
            "evaluating private declaration `{name}` for task `{task}` in `{uri}`",
            name = name.as_str(),
            task = task.name(),
            uri = document.uri(),
        );

        let decl_ty = decl.ty();
        let ty = self.convert_ast_type(document, &decl_ty)?;

        let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(
            self.engine,
            document,
            execution.work_dir(),
            execution.temp_dir(),
            ScopeRef::new(scopes, ROOT_SCOPE_INDEX),
        ));

        let expr = decl.expr().expect("private decls should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;
        let value = value.coerce(&ty).map_err(|e| {
            runtime_type_mismatch(
                e,
                &ty,
                decl_ty.syntax().text_range().to_span(),
                &value.ty(),
                expr.span(),
            )
        })?;
        scopes[ROOT_SCOPE_INDEX].insert(name.as_str(), value);
        Ok(())
    }

    /// Evaluates the runtime section.
    fn evaluate_runtime_section(
        &mut self,
        document: &Document,
        execution: &dyn TaskExecution,
        scopes: &[Scope],
        task: &Task,
        section: &RuntimeSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<(HashMap<String, Value>, HashMap<String, Value>)> {
        debug!(
            "evaluating runtimes section for task `{task}` in `{uri}`",
            task = task.name(),
            uri = document.uri()
        );

        let mut requirements = HashMap::new();
        let mut hints = HashMap::new();
        let version = document.version().expect("document should have version");
        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.requirement(name.as_str()) {
                requirements.insert(name.as_str().to_string(), value.clone());
                continue;
            } else if let Some(value) = inputs.hint(name.as_str()) {
                hints.insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(
                self.engine,
                document,
                execution.work_dir(),
                execution.temp_dir(),
                ScopeRef::new(scopes, ROOT_SCOPE_INDEX),
            ));

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
                requirements.insert(name.as_str().to_string(), value);
            } else {
                hints.insert(name.as_str().to_string(), value);
            }
        }

        Ok((requirements, hints))
    }

    /// Evaluates the requirements section.
    fn evaluate_requirements_section(
        &mut self,
        document: &Document,
        execution: &dyn TaskExecution,
        scopes: &[Scope],
        task: &Task,
        section: &RequirementsSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<HashMap<String, Value>> {
        debug!(
            "evaluating requirements section for task `{task}` in `{uri}`",
            task = task.name(),
            uri = document.uri()
        );

        let mut requirements = HashMap::new();
        let version = document.version().expect("document should have version");
        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.requirement(name.as_str()) {
                requirements.insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(
                self.engine,
                document,
                execution.work_dir(),
                execution.temp_dir(),
                ScopeRef::new(scopes, ROOT_SCOPE_INDEX),
            ));

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

            requirements.insert(name.as_str().to_string(), value);
        }

        Ok(requirements)
    }

    /// Evaluates the hints section.
    fn evaluate_hints_section(
        &mut self,
        document: &Document,
        execution: &dyn TaskExecution,
        scopes: &[Scope],
        task: &Task,
        section: &TaskHintsSection,
        inputs: &TaskInputs,
    ) -> EvaluationResult<HashMap<String, Value>> {
        debug!(
            "evaluating hints section for task `{task}` in `{uri}`",
            task = task.name(),
            uri = document.uri()
        );

        let mut hints = HashMap::new();
        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.hint(name.as_str()) {
                hints.insert(name.as_str().to_string(), value.clone());
                continue;
            }

            let mut evaluator = ExprEvaluator::new(
                TaskEvaluationContext::new(
                    self.engine,
                    document,
                    execution.work_dir(),
                    execution.temp_dir(),
                    ScopeRef::new(scopes, ROOT_SCOPE_INDEX),
                )
                .with_task(task),
            );

            let value = evaluator.evaluate_hints_item(&name, &item.expr())?;
            hints.insert(name.as_str().to_string(), value);
        }

        Ok(hints)
    }

    /// Evaluates the command of a task.
    fn evaluate_command(
        &mut self,
        document: &Document,
        execution: &mut dyn TaskExecution,
        scopes: &[Scope],
        task: &Task,
        section: &CommandSection,
        mapped_paths: &HashMap<String, String>,
    ) -> EvaluationResult<String> {
        debug!(
            "evaluating command section for task `{task}` in `{uri}`",
            task = task.name(),
            uri = document.uri()
        );

        let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(
            self.engine,
            document,
            execution.work_dir(),
            execution.temp_dir(),
            ScopeRef::new(scopes, TASK_SCOPE_INDEX),
        ));

        let mut command = String::new();
        if let Some(parts) = section.strip_whitespace() {
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
                task = task.name(),
                uri = document.uri(),
            );

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

        Ok(command)
    }

    /// Evaluates a task output.
    fn evaluate_output(
        &mut self,
        document: &Document,
        scopes: &mut [Scope],
        task: &Task,
        decl: &Decl,
        evaluated: &EvaluatedTask,
    ) -> EvaluationResult<()> {
        let name = decl.name();
        debug!(
            "evaluating output `{name}` for task `{task}`",
            name = name.as_str(),
            task = task.name(),
        );

        let decl_ty = decl.ty();
        let ty = self.convert_ast_type(document, &decl_ty)?;
        let mut evaluator = ExprEvaluator::new(
            TaskEvaluationContext::new(
                self.engine,
                document,
                &evaluated.work_dir,
                &evaluated.temp_dir,
                ScopeRef::new(scopes, TASK_SCOPE_INDEX),
            )
            .with_stdout(&evaluated.stdout)
            .with_stderr(&evaluated.stderr),
        );

        let expr = decl.expr().expect("outputs should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;

        // First coerce the output value to the expected type
        let mut value = value.coerce(&ty).map_err(|e| {
            runtime_type_mismatch(
                e,
                &ty,
                decl_ty.syntax().text_range().to_span(),
                &value.ty(),
                expr.span(),
            )
        })?;

        // Finally, join the path with the working directory, checking for existence
        value
            .join_paths(&evaluated.work_dir, true, ty.is_optional())
            .map_err(|e| missing_task_output(e, task.name(), &name))?;

        scopes[OUTPUT_SCOPE_INDEX].insert(name.as_str(), value);
        Ok(())
    }

    /// Converts an AST type to an analysis type.
    fn convert_ast_type(
        &mut self,
        document: &Document,
        ty: &wdl_ast::v1::Type,
    ) -> Result<Type, Diagnostic> {
        /// Used to resolve a type name from a document.
        struct Resolver<'a> {
            /// The engine that we'll resolve type names with.
            engine: &'a mut Engine,
            /// The document containing the type name to resolve.
            document: &'a Document,
        }

        impl TypeNameResolver for Resolver<'_> {
            fn resolve(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
                self.engine.resolve_type_name(self.document, name)
            }
        }

        AstTypeConverter::new(Resolver {
            engine: self.engine,
            document,
        })
        .convert_type(ty)
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
