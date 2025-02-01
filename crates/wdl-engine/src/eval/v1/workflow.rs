//! Implementation of evaluation for V1 workflows.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::future::Future;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use futures::FutureExt;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use petgraph::Direction;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Bfs;
use petgraph::visit::EdgeRef;
use rowan::ast::AstPtr;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::trace;
use wdl_analysis::diagnostics::only_one_namespace;
use wdl_analysis::diagnostics::recursive_workflow_call;
use wdl_analysis::diagnostics::type_is_not_array;
use wdl_analysis::diagnostics::unknown_name;
use wdl_analysis::diagnostics::unknown_namespace;
use wdl_analysis::diagnostics::unknown_task_or_workflow;
use wdl_analysis::document::Document;
use wdl_analysis::document::Task;
use wdl_analysis::eval::v1::WorkflowGraphBuilder;
use wdl_analysis::eval::v1::WorkflowGraphNode;
use wdl_analysis::types::ArrayType;
use wdl_analysis::types::CallType;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::PromotionKind;
use wdl_analysis::types::Type;
use wdl_ast::Ast;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::SupportedVersion;
use wdl_ast::TokenStrHash;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ScatterStatement;

use super::DeclPtr;
use super::ProgressKind;
use crate::Array;
use crate::CallValue;
use crate::Coercible;
use crate::EvaluationContext;
use crate::EvaluationResult;
use crate::Inputs;
use crate::Outputs;
use crate::Scope;
use crate::ScopeIndex;
use crate::ScopeRef;
use crate::TaskExecutionBackend;
use crate::Value;
use crate::WorkflowInputs;
use crate::config::Config;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::output_evaluation_failed;
use crate::diagnostics::runtime_type_mismatch;
use crate::v1::ExprEvaluator;
use crate::v1::TaskEvaluator;

/// Helper for formatting a workflow or task identifier for a call statement.
fn id(namespace: Option<&str>, target: &str, alias: &str, scatter_index: &str) -> String {
    if alias != target {
        match namespace {
            Some(ns) => {
                format!(
                    "{ns}-{target}-{alias}{sep}{scatter_index}",
                    sep = if scatter_index.is_empty() { "" } else { "-" },
                )
            }
            None => {
                format!(
                    "{target}-{alias}{sep}{scatter_index}",
                    sep = if scatter_index.is_empty() { "" } else { "-" },
                )
            }
        }
    } else {
        match namespace {
            Some(ns) => {
                format!(
                    "{ns}-{alias}{sep}{scatter_index}",
                    sep = if scatter_index.is_empty() { "" } else { "-" },
                )
            }
            None => {
                format!(
                    "{alias}{sep}{scatter_index}",
                    sep = if scatter_index.is_empty() { "" } else { "-" },
                )
            }
        }
    }
}

/// A "hidden" scope variable for representing the scope's scatter index.
///
/// This is only present in the scope created for a scatter statement.
///
/// The name is intentionally not a valid WDL identifier so that it cannot
/// conflict with any other variables in scope.
const SCATTER_INDEX_VAR: &str = "$idx";

/// Represents a "pointer" to a workflow evaluation graph node.
///
/// Unlike `WorkflowGraphNode`, this type is `Send`+`Sync`.
///
/// This type is cheaply cloned.
#[derive(Debug, Clone)]
enum WorkflowGraphNodePtr {
    /// The node is an input.
    Input(DeclPtr),
    /// The node is a private decl.
    Decl(DeclPtr),
    /// The node is an output decl.
    Output(DeclPtr),
    /// The node is a conditional statement.
    ///
    /// Stores the AST node along with the exit node index.
    Conditional(AstPtr<ConditionalStatement>, NodeIndex),
    /// The node is a scatter statement.
    ///
    /// Stores the AST node along with the exit node index.
    Scatter(AstPtr<ScatterStatement>, NodeIndex),
    /// The node is a call statement.
    Call(AstPtr<CallStatement>),
    /// The node is an exit of a conditional statement.
    ///
    /// This is a special node that is paired with each conditional statement
    /// node.
    ///
    /// It is the point by which the conditional is being exited and the outputs
    /// of the statement are introduced into the parent scope.
    ExitConditional(AstPtr<ConditionalStatement>),
    /// The node is an exit of a scatter statement.
    ///
    /// This is a special node that is paired with each scatter statement node.
    ///
    /// It is the point by which the scatter is being exited and the outputs of
    /// the statement are introduced into the parent scope.
    ExitScatter(AstPtr<ScatterStatement>),
}

impl WorkflowGraphNodePtr {
    /// Constructs a new indirect workflow graph node from a workflow graph
    /// node.
    fn new(node: &WorkflowGraphNode) -> Self {
        match node {
            WorkflowGraphNode::Input(decl) => Self::Input(DeclPtr::new(decl)),
            WorkflowGraphNode::Decl(decl) => Self::Decl(DeclPtr::new(decl)),
            WorkflowGraphNode::Output(decl) => Self::Output(DeclPtr::new(decl)),
            WorkflowGraphNode::Conditional(stmt, exit) => {
                Self::Conditional(AstPtr::new(stmt), *exit)
            }
            WorkflowGraphNode::Scatter(stmt, exit) => Self::Scatter(AstPtr::new(stmt), *exit),
            WorkflowGraphNode::Call(stmt) => Self::Call(AstPtr::new(stmt)),
            WorkflowGraphNode::ExitConditional(stmt) => Self::ExitConditional(AstPtr::new(stmt)),
            WorkflowGraphNode::ExitScatter(stmt) => Self::ExitScatter(AstPtr::new(stmt)),
        }
    }

    /// Converts the pointer back to the workflow graph node.
    fn to_node(&self, document: &Document) -> WorkflowGraphNode {
        match self {
            Self::Input(decl) => WorkflowGraphNode::Input(decl.to_node(document)),
            Self::Decl(decl) => WorkflowGraphNode::Decl(decl.to_node(document)),
            Self::Output(decl) => WorkflowGraphNode::Output(decl.to_node(document)),
            Self::Conditional(stmt, exit) => {
                WorkflowGraphNode::Conditional(stmt.to_node(document.node().syntax()), *exit)
            }
            Self::Scatter(stmt, exit) => {
                WorkflowGraphNode::Scatter(stmt.to_node(document.node().syntax()), *exit)
            }
            Self::Call(stmt) => WorkflowGraphNode::Call(stmt.to_node(document.node().syntax())),
            Self::ExitConditional(stmt) => {
                WorkflowGraphNode::ExitConditional(stmt.to_node(document.node().syntax()))
            }
            Self::ExitScatter(stmt) => {
                WorkflowGraphNode::ExitScatter(stmt.to_node(document.node().syntax()))
            }
        }
    }
}

/// Used to evaluate expressions in workflows.
struct WorkflowEvaluationContext<'a, 'b> {
    /// The document being evaluated.
    document: &'a Document,
    /// The scope being evaluated.
    scope: ScopeRef<'b>,
    /// The workflow's work directory.
    work_dir: &'a Path,
    /// The workflow's temporary directory.
    temp_dir: &'a Path,
}

impl<'a, 'b> WorkflowEvaluationContext<'a, 'b> {
    /// Constructs a new expression evaluation context.
    pub fn new(
        document: &'a Document,
        scope: ScopeRef<'b>,
        work_dir: &'a Path,
        temp_dir: &'a Path,
    ) -> Self {
        Self {
            document,
            scope,
            work_dir,
            temp_dir,
        }
    }
}

impl EvaluationContext for WorkflowEvaluationContext<'_, '_> {
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
        crate::resolve_type_name(self.document, name)
    }

    fn work_dir(&self) -> &Path {
        self.work_dir
    }

    fn temp_dir(&self) -> &Path {
        self.temp_dir
    }

    fn stdout(&self) -> Option<&Value> {
        None
    }

    fn stderr(&self) -> Option<&Value> {
        None
    }

    fn task(&self) -> Option<&Task> {
        None
    }
}

/// The scopes collection used for workflow evaluation.
#[derive(Debug)]
struct Scopes {
    /// The scopes available in workflow evaluation.
    ///
    /// The first scope is always the root scope and the second scope is always
    /// the output scope.
    ///
    /// An index in this collection might be either "in use" or "free"; if the
    /// latter, the index will be recorded in the `free` collection.
    all: Vec<Scope>,
    /// Indexes into `scopes` that are currently "free".
    ///
    /// This helps reduce memory usage by reusing scopes from scatter
    /// statements.
    free: Vec<ScopeIndex>,
}

impl Scopes {
    /// The index of a workflow's output scope.
    const OUTPUT_INDEX: ScopeIndex = ScopeIndex::new(1);
    /// The index of a workflow's root scope.
    const ROOT_INDEX: ScopeIndex = ScopeIndex::new(0);

    /// Allocates a new scope and returns the scope index.
    fn alloc(&mut self, parent: ScopeIndex) -> ScopeIndex {
        if let Some(index) = self.free.pop() {
            self.all[index.0].set_parent(parent);
            return index;
        }

        let index = self.all.len();
        self.all.push(Scope::new(parent));
        index.into()
    }

    /// Gets a reference to the given scope.
    fn reference(&self, index: ScopeIndex) -> ScopeRef<'_> {
        ScopeRef::new(&self.all, index)
    }

    /// Takes a scope from the collection, replacing it with a default.
    ///
    /// Note that this does not free the scope.
    fn take(&mut self, index: ScopeIndex) -> Scope {
        mem::take(&mut self.all[index.0])
    }

    /// Gets a mutable reference to the given scope index.
    fn get_mut(&mut self, index: ScopeIndex) -> &mut Scope {
        &mut self.all[index.0]
    }

    /// Gets a mutable reference to the given scope's parent and a reference to
    /// the given scope.
    fn parent_mut(&mut self, index: ScopeIndex) -> (&mut Scope, &Scope) {
        let parent = self.all[index.0].parent.expect("should have parent");
        if index.0 < parent.0 {
            let (left, right) = self.all.split_at_mut(index.0 + 1);
            (&mut right[parent.0 - index.0 - 1], &left[index.0])
        } else {
            let (left, right) = self.all.split_at_mut(parent.0 + 1);
            (&mut left[parent.0], &right[index.0 - parent.0 - 1])
        }
    }

    /// Gets the scatter index for the given scope as a string.
    fn scatter_index(&self, scope: ScopeIndex) -> String {
        let mut scope = ScopeRef::new(&self.all, scope);
        let mut s = String::new();
        loop {
            if let Some(value) = scope.local(SCATTER_INDEX_VAR) {
                if !s.is_empty() {
                    s.push('-');
                }

                write!(
                    &mut s,
                    "{i}",
                    i = value.as_integer().expect("index should be an integer")
                )
                .expect("failed to write to string");
            }

            match scope.parent() {
                Some(parent) => scope = parent,
                None => break,
            }
        }

        s
    }

    /// Frees a scope that is no longer used.
    ///
    /// The scope isn't actually deallocated, just cleared and marked as free to
    /// be reused.
    fn free(&mut self, index: ScopeIndex) {
        let scope = &mut self.all[index.0];
        scope.clear();
        self.free.push(index);
    }
}

impl Default for Scopes {
    fn default() -> Self {
        Self {
            // Create both the root and output scopes
            all: vec![Scope::default(), Scope::new(Self::ROOT_INDEX)],
            free: Default::default(),
        }
    }
}

/// Represents an array being gathered for a scatter statement.
struct GatherArray {
    /// The element type of the gather array.
    element_ty: Type,
    /// The elements of the gather array.
    elements: Vec<Value>,
}

impl GatherArray {
    /// Constructs a new gather array given the first completed element and
    /// capacity of the array.
    fn new(index: usize, value: Value, capacity: usize) -> Self {
        let element_ty = value.ty();
        let mut elements = vec![Value::None; capacity];
        elements[index] = value;
        Self {
            element_ty,
            elements,
        }
    }

    /// Converts the gather array into a WDL array value.
    fn into_array(self) -> Array {
        Array::new_unchecked(ArrayType::new(self.element_ty).into(), self.elements)
    }
}

/// Represents the result of gathering the scatter.
enum Gather {
    /// The values are being gathered into an array value.
    Array(GatherArray),
    /// The values are being gathered into a call value.
    Call {
        /// The type of the call being gathered.
        call_ty: CallType,
        /// The gathered outputs of the call.
        outputs: IndexMap<String, GatherArray>,
    },
}

impl Gather {
    /// Constructs a new gather from the first scatter result with the given
    /// index.
    fn new(capacity: usize, index: usize, value: Value) -> Self {
        if let Value::Call(call) = value {
            return Self::Call {
                call_ty: call.ty().promote(PromotionKind::Scatter),
                outputs: call
                    .outputs()
                    .iter()
                    .map(|(n, v)| (n.to_string(), GatherArray::new(index, v.clone(), capacity)))
                    .collect(),
            };
        }

        Self::Array(GatherArray::new(index, value, capacity))
    }

    /// Sets the value with the given gather array index.
    fn set(&mut self, index: usize, value: Value) -> EvaluationResult<()> {
        match self {
            Self::Array(array) => {
                assert!(value.as_call().is_none(), "value should not be a call");
                if let Some(ty) = array.element_ty.common_type(&value.ty()) {
                    array.element_ty = ty;
                }

                array.elements[index] = value;
            }
            Self::Call { outputs, .. } => {
                for (k, v) in value.unwrap_call().outputs().iter() {
                    let output = outputs
                        .get_mut(k)
                        .expect("expected call output to be present");
                    if let Some(ty) = output.element_ty.common_type(&v.ty()) {
                        output.element_ty = ty;
                    }

                    output.elements[index] = v.clone();
                }
            }
        }

        Ok(())
    }

    /// Converts the gather into a WDL value.
    fn into_value(self) -> Value {
        match self {
            Self::Array(array) => array.into_array().into(),
            Self::Call { call_ty, outputs } => CallValue::new_unchecked(
                call_ty,
                Outputs::from_iter(outputs.into_iter().map(|(n, v)| (n, v.into_array().into())))
                    .into(),
            )
            .into(),
        }
    }
}

/// Represents a subgraph of a workflow evaluation graph.
///
/// The subgraph stores relevant node indexes mapped to their current indegrees.
///
/// Scatter and conditional statements introduce new subgraphs for evaluation.
///
/// Subgraphs are entirely disjoint; no two subgraphs will share the same node
/// index from the original evaluation graph.
#[derive(Debug, Clone, Default)]
struct Subgraph(HashMap<NodeIndex, usize>);

impl Subgraph {
    /// Constructs a new subgraph from the given evaluation graph.
    ///
    /// Initially, the subgraph will contain every node in the evaluation graph
    /// until it is split.
    fn new(graph: &DiGraph<WorkflowGraphNodePtr, ()>) -> Self {
        let mut nodes = HashMap::with_capacity(graph.node_count());
        for index in graph.node_indices() {
            nodes.insert(
                index,
                graph.edges_directed(index, Direction::Incoming).count(),
            );
        }

        Self(nodes)
    }

    /// Splits this subgraph and returns a map of entry nodes to the
    /// corresponding subgraph.
    ///
    /// This subgraph is modified to replace any direct subgraphs with only the
    /// entry and exit nodes.
    fn split(&mut self, graph: &DiGraph<WorkflowGraphNodePtr, ()>) -> HashMap<NodeIndex, Subgraph> {
        /// Splits a parent subgraph for a scatter or conditional statement.
        ///
        /// This works by "stealing" the parent's nodes between the entry and
        /// exit nodes into a new subgraph.
        ///
        /// The exit node of the parent graph is reduced to an indegree of 1;
        /// only the connection between the entry and exit node will
        /// remain.
        ///
        /// Returns the nodes that comprise the new subgraph.
        fn split(
            graph: &DiGraph<WorkflowGraphNodePtr, ()>,
            parent: &mut HashMap<NodeIndex, usize>,
            entry: NodeIndex,
            exit: NodeIndex,
        ) -> HashMap<NodeIndex, usize> {
            let mut nodes = HashMap::new();
            let mut bfs = Bfs::new(graph, entry);
            while let Some(node) = {
                // Don't visit the exit node
                if bfs.stack.front() == Some(&exit) {
                    bfs.stack.pop_front();
                }
                bfs.next(graph)
            } {
                // Don't include the entry or exit nodes in the subgraph
                if node == entry || node == exit {
                    continue;
                }

                // Steal the node from the parent
                let prev = nodes.insert(
                    node,
                    parent.remove(&node).expect("node should exist in parent"),
                );
                assert!(prev.is_none());
            }

            // Decrement the indegree the nodes connected to the entry as we're not
            // including it in the subgraph
            for edge in graph.edges_directed(entry, Direction::Outgoing) {
                if edge.target() != exit {
                    *nodes
                        .get_mut(&edge.target())
                        .expect("should be in subgraph") -= 1;
                }
            }

            // Set the exit node to an indegree of 1 (incoming from the entry node)
            *parent.get_mut(&exit).expect("should have exit node") = 1;
            nodes
        }

        /// Used to recursively split the subgraph.
        fn split_recurse(
            graph: &DiGraph<WorkflowGraphNodePtr, ()>,
            nodes: &mut HashMap<NodeIndex, usize>,
            subgraphs: &mut HashMap<NodeIndex, Subgraph>,
        ) {
            for index in graph.node_indices() {
                if !nodes.contains_key(&index) {
                    continue;
                }

                match &graph[index] {
                    WorkflowGraphNodePtr::Conditional(_, exit)
                    | WorkflowGraphNodePtr::Scatter(_, exit) => {
                        let mut nodes = split(graph, nodes, index, *exit);
                        split_recurse(graph, &mut nodes, subgraphs);
                        subgraphs.insert(index, Subgraph(nodes));
                    }
                    _ => {}
                }
            }
        }

        let mut subgraphs = HashMap::new();
        split_recurse(graph, &mut self.0, &mut subgraphs);
        subgraphs
    }

    /// Removes the given node from the subgraph.
    ///
    /// # Panics
    ///
    /// Panics if the node's indegree is not 0.
    fn remove_node(&mut self, graph: &DiGraph<WorkflowGraphNodePtr, ()>, node: NodeIndex) {
        let indegree = self.0.remove(&node);
        assert_eq!(
            indegree,
            Some(0),
            "removed a node with an indegree greater than 0"
        );

        // Decrement the indegrees of connected nodes
        for edge in graph.edges_directed(node, Direction::Outgoing) {
            if let Some(indegree) = self.0.get_mut(&edge.target()) {
                *indegree -= 1;
            }
        }
    }
}

/// Represents workflow evaluation state.
struct State {
    /// The evaluation configuration to use.
    config: Arc<Config>,
    /// The task execution backend to use.
    backend: Arc<dyn TaskExecutionBackend>,
    /// The document containing the workflow being evaluated.
    document: Document,
    /// The workflow's inputs.
    inputs: WorkflowInputs,
    /// The scopes used in workflow evaluation.
    scopes: RwLock<Scopes>,
    /// The workflow evaluation graph.
    graph: DiGraph<WorkflowGraphNodePtr, ()>,
    /// The map from graph node index to subgraph.
    subgraphs: HashMap<NodeIndex, Subgraph>,
    /// The workflow evaluation working directory path.
    work_dir: PathBuf,
    /// The workflow evaluation temp directory path.
    temp_dir: PathBuf,
    /// The calls directory path.
    calls_dir: PathBuf,
}

impl State {
    /// Constructs a new workflow evaluation state.
    fn new(
        config: Arc<Config>,
        backend: Arc<dyn TaskExecutionBackend>,
        document: Document,
        inputs: WorkflowInputs,
        graph: DiGraph<WorkflowGraphNodePtr, ()>,
        subgraphs: HashMap<NodeIndex, Subgraph>,
        root: &Path,
    ) -> anyhow::Result<Self> {
        let work_dir = root.join("work");

        // Create the temp directory now as it may be needed for workflow evaluation
        let temp_dir = root.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        let calls_dir = root.join("calls");
        fs::create_dir_all(&calls_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        Ok(Self {
            config,
            backend,
            document,
            inputs,
            scopes: Default::default(),
            graph,
            subgraphs,
            work_dir,
            temp_dir,
            calls_dir,
        })
    }
}

/// Represents a WDL V1 workflow evaluator.
///
/// This type is cheaply cloned.
#[derive(Clone)]
pub struct WorkflowEvaluator {
    /// The configuration for evaluation.
    config: Arc<Config>,
    /// The associated task execution backend.
    backend: Arc<dyn TaskExecutionBackend>,
}

impl WorkflowEvaluator {
    /// Constructs a new workflow evaluator with the given evaluation
    /// configuration.
    ///
    /// This method creates a default task execution backend.
    ///
    /// Returns an error if the configuration isn't valid.
    pub fn new(config: Config) -> Result<Self> {
        let backend = config.create_backend()?;
        Self::new_with_backend(config, backend)
    }

    /// Constructs a new workflow evaluator with the given evaluation
    /// configuration and task execution backend.
    ///
    /// Returns an error if the configuration isn't valid.
    pub fn new_with_backend(
        config: Config,
        backend: Arc<dyn TaskExecutionBackend>,
    ) -> Result<Self> {
        config.validate()?;

        Ok(Self {
            config: Arc::new(config),
            backend,
        })
    }

    /// Evaluates the workflow of the given document.
    ///
    /// Upon success, returns the outputs of the workflow.
    pub async fn evaluate<P, R>(
        &mut self,
        document: &Document,
        inputs: WorkflowInputs,
        root_dir: impl AsRef<Path>,
        progress: P,
    ) -> EvaluationResult<Outputs>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        let workflow = document
            .workflow()
            .context("document does not contain a workflow")?;

        self.evaluate_with_progress(
            document,
            inputs,
            root_dir.as_ref(),
            workflow.name(),
            Arc::new(progress),
        )
        .await
    }

    /// Evaluates the workflow of the given document with the given shared
    /// progress callback.
    async fn evaluate_with_progress<P, R>(
        &mut self,
        document: &Document,
        inputs: WorkflowInputs,
        root_dir: &Path,
        id: &str,
        progress: Arc<P>,
    ) -> EvaluationResult<Outputs>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        progress(ProgressKind::WorkflowStarted { id }).await;

        let result = self
            .perform_evaluation(document, inputs, root_dir, progress.clone())
            .await;

        progress(ProgressKind::WorkflowCompleted {
            id,
            result: &result,
        })
        .await;

        result
    }

    /// Evaluates the workflow of the given document with the given shared
    /// progress callback.
    async fn perform_evaluation<P, R>(
        &mut self,
        document: &Document,
        inputs: WorkflowInputs,
        root_dir: &Path,
        progress: Arc<P>,
    ) -> EvaluationResult<Outputs>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        // Return the first error analysis diagnostic if there was one
        // With this check, we can assume certain correctness properties of the document
        if let Some(diagnostic) = document
            .diagnostics()
            .iter()
            .find(|d| d.severity() == Severity::Error)
        {
            return Err(diagnostic.clone().into());
        }

        // Validate the inputs for the workflow
        let workflow = document
            .workflow()
            .context("document does not contain a workflow")?;
        inputs.validate(document, workflow, None).with_context(|| {
            format!(
                "failed to validate the inputs to workflow `{workflow}`",
                workflow = workflow.name()
            )
        })?;

        let (graph, workflow_outputs) = {
            let ast = match document.node().ast() {
                Ast::V1(ast) => ast,
                _ => {
                    return Err(anyhow!(
                        "workflow evaluation is only supported for WDL 1.x documents"
                    )
                    .into());
                }
            };

            info!(
                "evaluating workflow `{workflow}` in `{uri}`",
                workflow = workflow.name(),
                uri = document.uri()
            );

            // Find the workflow in the AST
            let definition = ast
                .workflows()
                .next()
                .expect("workflow should exist in the AST");

            // Build an evaluation graph for the workflow
            let mut diagnostics = Vec::new();
            let graph = WorkflowGraphBuilder::default().build(&definition, &mut diagnostics);
            if let Some(diagnostic) = diagnostics.pop() {
                return Err(diagnostic.into());
            }

            // Map the graph to using indirect graph nodes so that we can share the graph
            // between threads
            (
                graph.map(|_, n| WorkflowGraphNodePtr::new(n), |_, e| *e),
                definition.output().map(|s| AstPtr::new(&s)),
            )
        };

        // Split the root subgraph for every conditional and scatter statement
        let mut root = Subgraph::new(&graph);
        let subgraphs = root.split(&graph);

        let max_concurrency = self
            .config
            .workflow
            .scatter
            .concurrency
            .unwrap_or_else(|| self.backend.max_concurrency());

        // Evaluate the root graph to completion
        let state = Arc::new(State::new(
            self.config.clone(),
            self.backend.clone(),
            document.clone(),
            inputs,
            graph,
            subgraphs,
            root_dir,
        )?);
        Self::evaluate_subgraph(
            state.clone(),
            Scopes::ROOT_INDEX,
            root,
            max_concurrency,
            progress,
        )
        .await?;

        // Take the output scope and return it
        let mut outputs: Outputs = state.scopes.write().await.take(Scopes::OUTPUT_INDEX).into();
        if let Some(section) = workflow_outputs {
            let section = section.to_node(document.node().syntax());
            let indexes: HashMap<_, _> = section
                .declarations()
                .enumerate()
                .map(|(i, d)| (TokenStrHash::new(d.name()), i))
                .collect();
            outputs.sort_by(move |a, b| indexes[a].cmp(&indexes[b]))
        }

        Ok(outputs)
    }

    /// Evaluates a subgraph to completion.
    ///
    /// Note that this method is not `async` because it is indirectly recursive.
    ///
    /// The boxed future breaks the cycle that would otherwise exist when trying
    /// to have the Rust compiler create an opaque type for the future returned
    /// by an `async` method.
    fn evaluate_subgraph<P, R>(
        state: Arc<State>,
        scope: ScopeIndex,
        mut subgraph: Subgraph,
        max_concurrency: u64,
        progress: Arc<P>,
    ) -> BoxFuture<'static, EvaluationResult<()>>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        async move {
            let mut futures = JoinSet::new();
            // The set of nodes being processed
            let mut processing: Vec<NodeIndex> = Vec::new();
            // The set of graph nodes being awaited on
            let mut awaiting: HashSet<NodeIndex> = HashSet::new();

            while !subgraph.0.is_empty() {
                // Add nodes with indegree 0 that we aren't already waiting on
                processing.extend(subgraph.0.iter().filter_map(|(node, indegree)| {
                    if *indegree == 0 && !awaiting.contains(node) {
                        Some(*node)
                    } else {
                        None
                    }
                }));

                // If no graph nodes can be processed, await on any futures
                if processing.is_empty() {
                    let node: EvaluationResult<NodeIndex> = futures
                        .join_next()
                        .await
                        .expect("should have a future to wait on")
                        .expect("failed to join future");

                    let node = node?;
                    match state.graph[node].to_node(&state.document) {
                        WorkflowGraphNode::Call(stmt) => debug!(
                            "evaluation of call statement `{name}` has completed; removing from \
                             evaluation graph",
                            name = stmt
                                .alias()
                                .map(|a| a.name())
                                .unwrap_or_else(|| stmt.target().names().last().unwrap())
                                .as_str()
                        ),
                        WorkflowGraphNode::Conditional(stmt, _) => debug!(
                            "evaluation of conditional statement `{expr}` has completed; removing \
                             from evaluation graph",
                            expr = stmt.expr().syntax().text()
                        ),
                        WorkflowGraphNode::Scatter(stmt, _) => debug!(
                            "evaluation of scatter statement `{name}` has completed; removing \
                             from evaluation graph",
                            name = stmt.variable().as_str(),
                        ),
                        _ => unreachable!(),
                    }

                    awaiting.remove(&node);
                    subgraph.remove_node(&state.graph, node);

                    // Continue to see if we can progress further in the subgraph; if not we'll
                    // await more futures
                    continue;
                }

                // Process the node now or spawn a future
                for node in processing.iter().copied() {
                    trace!("evaluating node `{:?}` ({node:?})", state.graph[node]);
                    match state.graph[node].clone() {
                        WorkflowGraphNodePtr::Input(decl) => {
                            Self::evaluate_input(&state, decl).await?
                        }
                        WorkflowGraphNodePtr::Decl(decl) => {
                            Self::evaluate_decl(&state, scope, decl).await?
                        }
                        WorkflowGraphNodePtr::Output(decl) => {
                            Self::evaluate_output(&state, decl).await?
                        }
                        WorkflowGraphNodePtr::Conditional(stmt, _) => {
                            let state = state.clone();
                            let progress = progress.clone();
                            futures.spawn(async move {
                                Self::evaluate_conditional(
                                    state,
                                    scope,
                                    node,
                                    stmt,
                                    max_concurrency,
                                    progress,
                                )
                                .await?;
                                Ok(node)
                            });
                            awaiting.insert(node);
                        }
                        WorkflowGraphNodePtr::Scatter(stmt, _) => {
                            let state = state.clone();
                            let progress = progress.clone();
                            futures.spawn(async move {
                                Self::evaluate_scatter(
                                    state,
                                    scope,
                                    node,
                                    stmt,
                                    max_concurrency,
                                    progress,
                                )
                                .await?;
                                Ok(node)
                            });
                            awaiting.insert(node);
                        }
                        WorkflowGraphNodePtr::Call(stmt) => {
                            let state = state.clone();
                            let progress = progress.clone();
                            futures.spawn(async move {
                                Self::evaluate_call(state, scope, stmt, progress).await?;
                                Ok(node)
                            });
                            awaiting.insert(node);
                        }
                        WorkflowGraphNodePtr::ExitConditional(_)
                        | WorkflowGraphNodePtr::ExitScatter(_) => {
                            // Handled directly in `evaluate_conditional` and `evaluate_scatter`
                            continue;
                        }
                    }
                }

                // Remove nodes that have completed
                for node in processing.drain(..) {
                    if awaiting.contains(&node) {
                        continue;
                    }

                    subgraph.remove_node(&state.graph, node);
                }
            }

            Ok(())
        }
        .boxed()
    }

    /// Evaluates a workflow input.
    async fn evaluate_input(state: &State, decl: DeclPtr) -> EvaluationResult<()> {
        // Create a scope for using `decl` as AST nodes aren't `Send` and cannot cross
        // await points
        let (expected_ty, name, name_span, expr) = {
            let decl = decl.to_node(&state.document);
            let name = decl.name();
            (
                crate::convert_ast_type_v1(&state.document, &decl.ty())?,
                name.syntax().green().to_owned(),
                name.span(),
                decl.expr().map(|e| (AstPtr::new(&e), e.span())),
            )
        };

        // Either use the specified input or evaluate the input's expression
        let (value, span) = match state.inputs.get(name.text()) {
            Some(input) => (input.clone(), name_span),
            None => {
                if let Some((expr, span)) = expr {
                    debug!(
                        "evaluating input `{name}` for workflow `{workflow}` in `{uri}`",
                        name = name.text(),
                        workflow = state
                            .document
                            .workflow()
                            .expect("should have workflow")
                            .name(),
                        uri = state.document.uri(),
                    );

                    (
                        Self::evaluate_expr(state, Scopes::ROOT_INDEX, expr).await?,
                        span,
                    )
                } else {
                    assert!(expected_ty.is_optional(), "type should be optional");
                    (Value::None, name_span)
                }
            }
        };

        // Coerce the value to the expected type
        let value = value
            .coerce(&expected_ty)
            .map_err(|e| runtime_type_mismatch(e, &expected_ty, name_span, &value.ty(), span))?;

        // Write the value into the root scope
        state
            .scopes
            .write()
            .await
            .get_mut(Scopes::ROOT_INDEX)
            .insert(name.text(), value);
        Ok(())
    }

    /// Evaluates a workflow private declaration.
    async fn evaluate_decl(
        state: &State,
        scope: ScopeIndex,
        decl: DeclPtr,
    ) -> EvaluationResult<()> {
        // Create a scope for using `decl` as AST nodes aren't `Send` and cannot cross
        // await points
        let (expected_ty, name, name_span, expr, span) = {
            let decl = decl.to_node(&state.document);
            let name = decl.name();
            let expr = decl.expr().expect("declaration should have expression");
            (
                crate::convert_ast_type_v1(&state.document, &decl.ty())?,
                name.syntax().green().to_owned(),
                name.span(),
                AstPtr::new(&expr),
                expr.span(),
            )
        };

        debug!(
            "evaluating private declaration `{name}` for workflow `{workflow}` in `{uri}`",
            name = name.text(),
            workflow = state
                .document
                .workflow()
                .expect("should have workflow")
                .name(),
            uri = state.document.uri(),
        );

        // Evaluate the decl's expression
        let value = Self::evaluate_expr(state, scope, expr).await?;

        // Coerce the value to the expected type
        let value = value
            .coerce(&expected_ty)
            .map_err(|e| runtime_type_mismatch(e, &expected_ty, name_span, &value.ty(), span))?;

        state
            .scopes
            .write()
            .await
            .get_mut(scope)
            .insert(name.text(), value);
        Ok(())
    }

    /// Evaluates a workflow output.
    async fn evaluate_output(state: &State, decl: DeclPtr) -> EvaluationResult<()> {
        // Create a scope for using `decl` as AST nodes aren't `Send` and cannot cross
        // await points
        let (expected_ty, name, name_span, expr, span) = {
            let decl = decl.to_node(&state.document);
            let name = decl.name();
            let expr = decl.expr().expect("declaration should have expression");
            (
                crate::convert_ast_type_v1(&state.document, &decl.ty())?,
                name.syntax().green().to_owned(),
                name.span(),
                AstPtr::new(&expr),
                expr.span(),
            )
        };

        debug!(
            "evaluating output `{name}` for workflow `{workflow}` in `{uri}`",
            name = name.text(),
            workflow = state
                .document
                .workflow()
                .expect("should have workflow")
                .name(),
            uri = state.document.uri()
        );

        // Evaluate the decl's expression
        let value = Self::evaluate_expr(state, Scopes::OUTPUT_INDEX, expr).await?;

        // Coerce the value to the expected type
        let mut value = value
            .coerce(&expected_ty)
            .map_err(|e| runtime_type_mismatch(e, &expected_ty, name_span, &value.ty(), span))?;

        // Finally, join any paths with the working directory, checking for existence
        value
            .join_paths(&state.work_dir, true, expected_ty.is_optional())
            .map_err(|e| {
                output_evaluation_failed(
                    e,
                    state
                        .document
                        .workflow()
                        .expect("should have workflow")
                        .name(),
                    false,
                    name.text(),
                    name_span,
                )
            })?;

        // Write the value into the output scope
        state
            .scopes
            .write()
            .await
            .get_mut(Scopes::OUTPUT_INDEX)
            .insert(name.text(), value);
        Ok(())
    }

    /// Evaluates a workflow conditional statement.
    async fn evaluate_conditional<P, R>(
        state: Arc<State>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: AstPtr<ConditionalStatement>,
        max_concurrency: u64,
        progress: Arc<P>,
    ) -> EvaluationResult<()>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        // Create a scope for using `stmt` as AST nodes aren't `Send` and cannot cross
        // await points
        let (expr, span, start) = {
            let stmt = stmt.to_node(state.document.node().syntax());
            let expr = stmt.expr();

            debug!(
                "evaluating conditional statement `{expr}` for workflow `{workflow}` in `{uri}`",
                expr = expr.syntax().text(),
                workflow = state
                    .document
                    .workflow()
                    .expect("should have workflow")
                    .name(),
                uri = state.document.uri()
            );

            (
                AstPtr::new(&expr),
                expr.span(),
                stmt.braced_scope_span()
                    .expect("should have braced scope span")
                    .start(),
            )
        };

        // Evaluate the conditional expression
        let value = Self::evaluate_expr(&state, parent, expr).await?;

        if value
            .coerce(&PrimitiveType::Boolean.into())
            .map_err(|e| if_conditional_mismatch(e, &value.ty(), span))?
            .unwrap_boolean()
        {
            debug!("conditional statement branch was taken; evaluating subgraph");

            // Intentionally drop the write lock before evaluating the subgraph
            let scope = { state.scopes.write().await.alloc(parent) };

            // Evaluate the subgraph
            Self::evaluate_subgraph(
                state.clone(),
                scope,
                state.subgraphs[&entry].clone(),
                max_concurrency,
                progress.clone(),
            )
            .await?;

            // Promote all values in the scope to the parent scope as optional
            let mut scopes = state.scopes.write().await;
            let (parent, child) = scopes.parent_mut(scope);
            for (name, value) in child.iter() {
                parent.insert(name.to_string(), value.clone_as_optional());
            }

            scopes.free(scope);
        } else {
            debug!("conditional statement branch was not taken; subgraph was skipped");

            // Conditional evaluated to false; set the expected names to `None` in the
            // parent scope
            let mut scopes = state.scopes.write().await;
            let parent = scopes.get_mut(parent);
            let scope = state
                .document
                .find_scope_by_position(start)
                .expect("should have scope");

            for (name, n) in scope.names() {
                if let Type::Call(ty) = n.ty() {
                    parent.insert(
                        name.to_string(),
                        CallValue::new_unchecked(
                            ty.promote(PromotionKind::Conditional),
                            Outputs::from_iter(
                                ty.outputs().iter().map(|(n, _)| (n.clone(), Value::None)),
                            )
                            .into(),
                        ),
                    );
                } else {
                    parent.insert(name.to_string(), Value::None);
                }
            }
        }

        Ok(())
    }

    /// Evaluates a workflow scatter statement.
    async fn evaluate_scatter<P, R>(
        state: Arc<State>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: AstPtr<ScatterStatement>,
        max_concurrency: u64,
        progress: Arc<P>,
    ) -> EvaluationResult<()>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        /// Awaits the next future in the set of futures.
        async fn await_next(
            futures: &mut JoinSet<EvaluationResult<(usize, ScopeIndex)>>,
            scopes: &RwLock<Scopes>,
            gathers: &mut HashMap<String, Gather>,
            capacity: usize,
        ) -> EvaluationResult<()> {
            let (index, scope) = futures
                .join_next()
                .await
                .expect("should have a future to wait on")
                .expect("failed to join future")?;

            // Append the result to the gather (the first two variables in scope are always
            // the scatter index and variable)
            let mut scopes = scopes.write().await;
            for (name, value) in scopes.get_mut(scope).iter().skip(2) {
                match gathers.get_mut(name) {
                    Some(gather) => gather.set(index, value.clone())?,
                    None => {
                        let prev = gathers.insert(
                            name.to_string(),
                            Gather::new(capacity, index, value.clone()),
                        );
                        assert!(prev.is_none());
                    }
                }
            }

            scopes.free(scope);
            Ok(())
        }

        // Create a scope for using `stmt` as AST nodes aren't `Send` and cannot cross
        // await points
        let (expr, variable, span) = {
            let stmt = stmt.to_node(state.document.node().syntax());
            let variable = stmt.variable();
            let expr = stmt.expr();

            debug!(
                "evaluating scatter statement `{variable}` for workflow `{workflow}` in `{uri}`",
                variable = variable.as_str(),
                workflow = state
                    .document
                    .workflow()
                    .expect("should have workflow")
                    .name(),
                uri = state.document.uri()
            );

            (
                AstPtr::new(&expr),
                variable.syntax().green().to_owned(),
                expr.span(),
            )
        };

        // Evaluate the scatter array expression
        let value = Self::evaluate_expr(&state, parent, expr).await?;

        let array = value
            .as_array()
            .ok_or_else(|| type_is_not_array(&value.ty(), span))?
            .as_slice();

        let mut futures = JoinSet::new();
        let mut gathers: HashMap<_, Gather> = HashMap::new();
        for (i, value) in array.iter().enumerate() {
            // Allocate a scope
            let scope = {
                let mut scopes = state.scopes.write().await;
                let index = scopes.alloc(parent);
                let scope = scopes.get_mut(index);
                scope.insert(
                    SCATTER_INDEX_VAR,
                    i64::try_from(i).map_err(|_| anyhow!("array index out of bounds"))?,
                );
                scope.insert(variable.text(), value.clone());
                index
            };

            // Evaluate the subgraph
            {
                let state = state.clone();
                let subgraph = state.subgraphs[&entry].clone();
                let progress = progress.clone();
                futures.spawn(async move {
                    Self::evaluate_subgraph(
                        state.clone(),
                        scope,
                        subgraph,
                        max_concurrency,
                        progress,
                    )
                    .await?;

                    Ok((i, scope))
                });
            }

            // If we've reached the concurrency limit, await one of the futures to complete
            if futures.len() as u64 >= max_concurrency {
                await_next(&mut futures, &state.scopes, &mut gathers, array.len()).await?;
            }
        }

        // Complete any outstanding futures
        while !futures.is_empty() {
            await_next(&mut futures, &state.scopes, &mut gathers, array.len()).await?;
        }

        let mut scopes = state.scopes.write().await;
        let scope = scopes.get_mut(parent);
        for (name, gather) in gathers {
            scope.insert(name, gather.into_value());
        }

        Ok(())
    }

    /// Evaluates a workflow call statement.
    async fn evaluate_call<P, R>(
        state: Arc<State>,
        scope: ScopeIndex,
        stmt: AstPtr<CallStatement>,
        progress: Arc<P>,
    ) -> EvaluationResult<()>
    where
        P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        /// Abstracts evaluation for both task and workflow calls.
        enum Evaluator<'a> {
            /// Used to evaluate a task call.
            Task(&'a Task, TaskEvaluator),
            /// Used to evaluate a workflow call.
            Workflow(WorkflowEvaluator),
        }

        impl Evaluator<'_> {
            /// Runs evaluation with the given inputs.
            ///
            /// Returns the passed in context and the result of the evaluation.
            async fn evaluate<P, R>(
                self,
                document: &Document,
                inputs: Inputs,
                root_dir: &Path,
                id: &str,
                progress: &Arc<P>,
            ) -> EvaluationResult<Outputs>
            where
                P: Fn(ProgressKind<'_>) -> R + Send + Sync + 'static,
                R: Future<Output = ()> + Send,
            {
                match self {
                    Evaluator::Task(task, mut evaluator) => {
                        debug!("evaluating call to task `{id}`");
                        evaluator
                            .evaluate_with_progress(
                                document,
                                task,
                                inputs.as_task_inputs().expect("should be task inputs"),
                                root_dir,
                                id,
                                progress.clone(),
                            )
                            .await?
                            .outputs
                    }
                    Evaluator::Workflow(mut evaluator) => {
                        debug!("evaluating call to workflow `{id}`");
                        evaluator
                            .evaluate_with_progress(
                                document,
                                inputs.unwrap_workflow_inputs(),
                                root_dir,
                                id,
                                progress.clone(),
                            )
                            .await
                    }
                }
            }
        }

        // Create a scope for using `stmt` as AST nodes aren't `Send` and cannot cross
        // await points
        let (namespace, target, target_span, alias) = {
            let stmt = stmt.to_node(state.document.node().syntax());
            let alias = stmt.alias().map(|a| a.name().syntax().green().to_owned());
            let mut names = stmt.target().names().peekable();
            let mut namespace = None;
            let mut target = None;

            // Resolve the target and namespace for the call
            while let Some(n) = names.next() {
                if names.peek().is_none() {
                    target = Some(n);
                    break;
                }

                if namespace.is_some() {
                    return Err(only_one_namespace(n.span()).into());
                }

                namespace = Some((
                    n.syntax().green().to_owned(),
                    state
                        .document
                        .namespace(n.as_str())
                        .ok_or_else(|| unknown_namespace(&n))?,
                ));
            }

            let target = target.expect("expected at least one name");
            (
                namespace,
                target.syntax().green().to_owned(),
                target.span(),
                alias,
            )
        };

        let alias = alias
            .as_ref()
            .map(|t| t.text())
            .unwrap_or_else(|| target.text());

        debug!(
            "evaluating call statement `{alias}` for workflow `{workflow}` in `{uri}`",
            workflow = state
                .document
                .workflow()
                .expect("should have workflow")
                .name(),
            uri = state.document.uri(),
        );

        // Check for a directly recursive workflow call
        if namespace.is_none()
            && target.text()
                == state
                    .document
                    .workflow()
                    .expect("should have workflow")
                    .name()
        {
            return Err(recursive_workflow_call(target.text(), target_span).into());
        }

        // Determine the inputs and evaluator to use for the task or workflow call
        let inputs = state.inputs.calls().get(alias).cloned();
        let document = namespace
            .as_ref()
            .map(|(_, ns)| ns.document())
            .unwrap_or(&state.document);
        let (mut inputs, evaluator) = if let Some(task) = document.task_by_name(target.text()) {
            (
                inputs.unwrap_or_else(|| Inputs::Task(Default::default())),
                Evaluator::Task(
                    task,
                    TaskEvaluator::new_unchecked(state.config.clone(), state.backend.clone()),
                ),
            )
        } else {
            match document.workflow() {
                Some(workflow) if workflow.name() == target.text() => (
                    inputs.unwrap_or_else(|| Inputs::Workflow(Default::default())),
                    Evaluator::Workflow(WorkflowEvaluator {
                        config: state.config.clone(),
                        backend: state.backend.clone(),
                    }),
                ),
                _ => {
                    return Err(unknown_task_or_workflow(
                        namespace.as_ref().map(|(_, ns)| ns.span()),
                        target.text(),
                        target_span,
                    )
                    .into());
                }
            }
        };

        // Evaluate the inputs
        let scatter_index = Self::evaluate_call_inputs(&state, stmt, scope, &mut inputs).await?;

        let dir = format!(
            "{alias}{sep}{scatter_index}",
            sep = if scatter_index.is_empty() { "" } else { "-" },
        );

        let id = id(
            namespace.as_ref().map(|(n, _)| n.text()),
            target.text(),
            alias,
            &scatter_index,
        );

        // Finally, evaluate the task or workflow and return the outputs
        let outputs = evaluator
            .evaluate(
                document,
                inputs,
                &state.calls_dir.join(&dir),
                &id,
                &progress,
            )
            .await?
            .with_name(alias);

        let ty = state
            .document
            .workflow()
            .expect("should have workflow")
            .calls()
            .get(alias)
            .expect("should have call");
        state.scopes.write().await.get_mut(scope).insert(
            alias,
            Value::Call(CallValue::new_unchecked(ty.clone(), Arc::new(outputs))),
        );

        Ok(())
    }

    /// Evaluates an expression.
    ///
    /// This takes a read lock on the scopes.
    async fn evaluate_expr(
        state: &State,
        scope: ScopeIndex,
        expr: AstPtr<Expr>,
    ) -> EvaluationResult<Value> {
        let scopes = state.scopes.read().await;
        let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
            &state.document,
            scopes.reference(scope),
            &state.work_dir,
            &state.temp_dir,
        ));
        let expr = expr.to_node(state.document.node().syntax());
        Ok(evaluator.evaluate_expr(&expr)?)
    }

    /// Evaluates the call inputs of a call statement.
    ///
    /// Returns the scatter index for the provided scope.
    ///
    /// This takes a read lock on the scopes.
    async fn evaluate_call_inputs(
        state: &State,
        stmt: AstPtr<CallStatement>,
        scope: ScopeIndex,
        inputs: &mut Inputs,
    ) -> EvaluationResult<String> {
        let scopes = state.scopes.read().await;
        let stmt = stmt.to_node(state.document.node().syntax());
        for input in stmt.inputs() {
            let name = input.name();
            let value = match input.expr() {
                Some(expr) => {
                    let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                        &state.document,
                        scopes.reference(scope),
                        &state.work_dir,
                        &state.temp_dir,
                    ));

                    evaluator.evaluate_expr(&expr)?
                }
                None => scopes
                    .reference(scope)
                    .lookup(name.as_str())
                    .cloned()
                    .ok_or_else(|| unknown_name(name.as_str(), name.span()))?,
            };

            let prev = inputs.set(input.name().as_str(), value);
            assert!(
                prev.is_none(),
                "attempted to override a specified call input"
            );
        }

        Ok(scopes.scatter_index(scope))
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use tempfile::TempDir;
    use wdl_analysis::Analyzer;
    use wdl_analysis::DiagnosticsConfig;

    use super::*;
    use crate::config::Backend;

    #[tokio::test]
    async fn it_reports_progress() {
        // Create two test WDL files: one with a no-op workflow to be called and another
        // with a no-op task to be called
        let root_dir = TempDir::new().expect("failed to create temporary directory");
        fs::write(
            root_dir.path().join("other.wdl"),
            r#"
version 1.1
workflow w {}
"#,
        )
        .expect("failed to write WDL source file");

        let source_path = root_dir.path().join("source.wdl");
        fs::write(
            &source_path,
            r#"
version 1.1

import "other.wdl"

task t {
  command <<<>>>
}

workflow w {
  scatter (i in range(10)) {
    call t
  }

  scatter (j in range(25)) {
    call other.w
  }
}
"#,
        )
        .expect("failed to write WDL source file");

        // Analyze the source files
        let analyzer = Analyzer::new(DiagnosticsConfig::except_all(), |(), _, _, _| async {});
        analyzer
            .add_directory(root_dir.path().to_path_buf())
            .await
            .expect("failed to add directory");
        let results = analyzer
            .analyze(())
            .await
            .expect("failed to analyze document");
        assert_eq!(results.len(), 2, "expected only two results");

        // Keep track of how many progress events we saw for evaluation
        #[derive(Default)]
        struct State {
            tasks_started: AtomicUsize,
            tasks_executions_started: AtomicUsize,
            tasks_executions_completed: AtomicUsize,
            tasks_completed: AtomicUsize,
            workflows_started: AtomicUsize,
            workflows_completed: AtomicUsize,
        }

        // Use a progress callback that simply increments the appropriate counter
        let mut config = Config::default();
        let state = Arc::<State>::default();
        let state_cloned = state.clone();
        config.backend.default = Backend::Local;
        let mut evaluator = WorkflowEvaluator::new(config).unwrap();

        // Evaluate the `w` workflow in `source.wdl` using the default local
        // backend
        let outputs = evaluator
            .evaluate(
                results
                    .iter()
                    .find(|r| r.document().uri().as_str().ends_with("source.wdl"))
                    .expect("should have result")
                    .document(),
                WorkflowInputs::default(),
                root_dir.path(),
                move |kind| {
                    match kind {
                        ProgressKind::TaskStarted { id, .. } => {
                            assert!(id.starts_with("t-"));
                            state_cloned.tasks_started.fetch_add(1, Ordering::SeqCst);
                        }
                        ProgressKind::TaskExecutionStarted { id, .. } => {
                            assert!(id.starts_with("t-"));
                            state_cloned
                                .tasks_executions_started
                                .fetch_add(1, Ordering::SeqCst);
                        }
                        ProgressKind::TaskExecutionCompleted { id, .. } => {
                            assert!(id.starts_with("t-"));
                            state_cloned
                                .tasks_executions_completed
                                .fetch_add(1, Ordering::SeqCst);
                        }
                        ProgressKind::TaskCompleted { id, .. } => {
                            assert!(id.starts_with("t-"));
                            state_cloned.tasks_completed.fetch_add(1, Ordering::SeqCst);
                        }
                        ProgressKind::WorkflowStarted { id, .. } => {
                            assert!(id == "w" || id.starts_with("other-w-"));
                            state_cloned
                                .workflows_started
                                .fetch_add(1, Ordering::SeqCst);
                        }
                        ProgressKind::WorkflowCompleted { id, .. } => {
                            assert!(id == "w" || id.starts_with("other-w-"));
                            state_cloned
                                .workflows_completed
                                .fetch_add(1, Ordering::SeqCst);
                        }
                    }

                    async {}
                },
            )
            .await
            .expect("failed to evaluate workflow");
        assert_eq!(outputs.iter().count(), 0, "expected no outputs");

        // Ensure the counters are what is expected based on the WDL
        assert_eq!(state.tasks_started.load(Ordering::SeqCst), 10);
        assert_eq!(state.tasks_executions_started.load(Ordering::SeqCst), 10);
        assert_eq!(state.tasks_executions_completed.load(Ordering::SeqCst), 10);
        assert_eq!(state.tasks_completed.load(Ordering::SeqCst), 10);
        assert_eq!(state.workflows_started.load(Ordering::SeqCst), 26);
        assert_eq!(state.workflows_completed.load(Ordering::SeqCst), 26);
    }
}
