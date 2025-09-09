//! Implementation of evaluation for V1 workflows.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use crankshaft::events::Event;
use futures::FutureExt;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use petgraph::Direction;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Bfs;
use petgraph::visit::EdgeRef;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::trace;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::Io;
use wdl_analysis::diagnostics::only_one_namespace;
use wdl_analysis::diagnostics::recursive_workflow_call;
use wdl_analysis::diagnostics::type_is_not_array;
use wdl_analysis::diagnostics::unknown_name;
use wdl_analysis::diagnostics::unknown_namespace;
use wdl_analysis::diagnostics::unknown_task_or_workflow;
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
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::CallKeyword;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::Decl;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::version::V1;

use crate::Array;
use crate::CallLocation;
use crate::CallValue;
use crate::Coercible;
use crate::EvaluationContext;
use crate::EvaluationError;
use crate::EvaluationResult;
use crate::Inputs;
use crate::Outputs;
use crate::PrimitiveValue;
use crate::Scope;
use crate::ScopeIndex;
use crate::ScopeRef;
use crate::TaskExecutionBackend;
use crate::Value;
use crate::WorkflowInputs;
use crate::config::Config;
use crate::diagnostics::decl_evaluation_failed;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::runtime_type_mismatch;
use crate::http::Downloader;
use crate::http::HttpDownloader;
use crate::path;
use crate::path::EvaluationPath;
use crate::tree::SyntaxNode;
use crate::tree::SyntaxToken;
use crate::v1::ExprEvaluator;
use crate::v1::INPUTS_FILE;
use crate::v1::OUTPUTS_FILE;
use crate::v1::TaskEvaluator;
use crate::v1::write_json_file;

/// Helper for formatting a workflow or task identifier for a call statement.
fn format_id(namespace: Option<&str>, target: &str, alias: &str, scatter_index: &str) -> String {
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

/// Used to evaluate expressions in workflows.
struct WorkflowEvaluationContext<'a, 'b> {
    /// The evaluation state.
    state: &'a State,
    /// The scope being evaluated.
    scope: ScopeRef<'b>,
}

impl<'a, 'b> WorkflowEvaluationContext<'a, 'b> {
    /// Constructs a new expression evaluation context.
    pub fn new(state: &'a State, scope: ScopeRef<'b>) -> Self {
        Self { state, scope }
    }
}

impl EvaluationContext for WorkflowEvaluationContext<'_, '_> {
    fn version(&self) -> SupportedVersion {
        self.state
            .document
            .version()
            .expect("document should have a version")
    }

    fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic> {
        self.scope
            .lookup(name)
            .cloned()
            .ok_or_else(|| unknown_name(name, span))
    }

    fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic> {
        crate::resolve_type_name(&self.state.document, name, span)
    }

    fn base_dir(&self) -> &EvaluationPath {
        &self.state.base_dir
    }

    fn temp_dir(&self) -> &Path {
        &self.state.temp_dir
    }

    fn downloader(&self) -> &dyn Downloader {
        &self.state.downloader
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
        let mut elements = vec![Value::new_none(element_ty.optional()); capacity];
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
    fn new(graph: &DiGraph<WorkflowGraphNode<SyntaxNode>, ()>) -> Self {
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
    fn split(
        &mut self,
        graph: &DiGraph<WorkflowGraphNode<SyntaxNode>, ()>,
    ) -> HashMap<NodeIndex, Subgraph> {
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
            graph: &DiGraph<WorkflowGraphNode<SyntaxNode>, ()>,
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
            graph: &DiGraph<WorkflowGraphNode<SyntaxNode>, ()>,
            nodes: &mut HashMap<NodeIndex, usize>,
            subgraphs: &mut HashMap<NodeIndex, Subgraph>,
        ) {
            for index in graph.node_indices() {
                if !nodes.contains_key(&index) {
                    continue;
                }

                match &graph[index] {
                    WorkflowGraphNode::Conditional(_, exit)
                    | WorkflowGraphNode::Scatter(_, exit) => {
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
    fn remove_node(&mut self, graph: &DiGraph<WorkflowGraphNode<SyntaxNode>, ()>, node: NodeIndex) {
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
    /// The cancellation token for cancelling workflow evaluation.
    token: CancellationToken,
    /// The document containing the workflow being evaluated.
    document: Document,
    /// The workflow's inputs.
    inputs: WorkflowInputs,
    /// The scopes used in workflow evaluation.
    scopes: RwLock<Scopes>,
    /// The workflow evaluation graph.
    graph: DiGraph<WorkflowGraphNode<SyntaxNode>, ()>,
    /// The map from graph node index to subgraph.
    subgraphs: HashMap<NodeIndex, Subgraph>,
    /// The base directory for evaluation.
    ///
    /// This is the document's directory.
    base_dir: EvaluationPath,
    /// The workflow evaluation temp directory.
    temp_dir: PathBuf,
    /// The calls directory path.
    calls_dir: PathBuf,
    /// The downloader for expression evaluation.
    downloader: HttpDownloader,
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
    /// The cancellation token for cancelling workflow evaluation.
    token: CancellationToken,
    /// The downloader for expression evaluation.
    downloader: HttpDownloader,
}

impl WorkflowEvaluator {
    /// Constructs a new workflow evaluator with the given evaluation
    /// configuration and cancellation token.
    ///
    /// This method creates a default task execution backend.
    ///
    /// Returns an error if the configuration isn't valid.
    pub async fn new(
        config: Config,
        token: CancellationToken,
        events: Option<broadcast::Sender<Event>>,
    ) -> Result<Self> {
        config.validate()?;

        let config = Arc::new(config);
        let backend = config.create_backend(events).await?;
        let downloader = HttpDownloader::new(config.clone())?;

        Ok(Self {
            config,
            backend,
            token,
            downloader,
        })
    }

    /// Evaluates the workflow of the given document.
    ///
    /// Upon success, returns the outputs of the workflow.
    pub async fn evaluate(
        &self,
        document: &Document,
        inputs: WorkflowInputs,
        root_dir: impl AsRef<Path>,
    ) -> EvaluationResult<Outputs> {
        let workflow = document
            .workflow()
            .context("document does not contain a workflow")?;

        // We cannot evaluate a document with errors
        if document.has_errors() {
            return Err(anyhow!("cannot evaluate a document with errors").into());
        }

        self.perform_evaluation(document, inputs, root_dir.as_ref(), workflow.name())
            .await
    }

    /// Performs the evaluation of the workflow of the given document.
    ///
    /// This method skips checking the document (and its transitive imports) for
    /// analysis errors as the check occurs at the `evaluate` entrypoint.
    async fn perform_evaluation(
        &self,
        document: &Document,
        inputs: WorkflowInputs,
        root_dir: &Path,
        id: &str,
    ) -> EvaluationResult<Outputs> {
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

        let ast = match document.root().morph().ast() {
            Ast::V1(ast) => ast,
            _ => {
                return Err(
                    anyhow!("workflow evaluation is only supported for WDL 1.x documents").into(),
                );
            }
        };

        debug!(
            workflow_id = id,
            workflow_name = workflow.name(),
            document = document.uri().as_str(),
            "evaluating workflow",
        );

        // Find the workflow in the AST
        let definition = ast
            .workflows()
            .next()
            .expect("workflow should exist in the AST");

        // Build an evaluation graph for the workflow
        let mut diagnostics = Vec::new();

        // We need to provide inputs to the workflow graph builder to avoid adding
        // dependency edges from the default expressions if a value was provided
        let graph = WorkflowGraphBuilder::default()
            .build(&definition, &mut diagnostics, |name| inputs.contains(name));
        assert!(
            diagnostics.is_empty(),
            "workflow evaluation graph should have no diagnostics"
        );

        // Split the root subgraph for every conditional and scatter statement
        let mut subgraph = Subgraph::new(&graph);
        let subgraphs = subgraph.split(&graph);

        let max_concurrency = self
            .config
            .workflow
            .scatter
            .concurrency
            .unwrap_or_else(|| self.backend.max_concurrency());

        // Create the temp directory now as it may be needed for workflow evaluation
        let temp_dir = root_dir.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        // Write the inputs to the workflow's root directory
        write_json_file(root_dir.join(INPUTS_FILE), &inputs)?;

        let calls_dir = root_dir.join("calls");
        fs::create_dir_all(&calls_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        let document_path = document.path();
        let effective_output_dir = root_dir.to_path_buf();

        let mut base_dir = EvaluationPath::parent_of(&document_path).with_context(|| {
            format!("document `{document_path}` does not have a parent directory")
        })?;

        base_dir.make_absolute();

        let state = Arc::new(State {
            config: self.config.clone(),
            backend: self.backend.clone(),
            token: self.token.clone(),
            document: document.clone(),
            inputs,
            scopes: Default::default(),
            graph,
            subgraphs,
            base_dir,
            temp_dir,
            calls_dir,
            downloader: self.downloader.clone(),
        });

        // Evaluate the root graph to completion
        Self::evaluate_subgraph(
            state.clone(),
            Scopes::ROOT_INDEX,
            subgraph,
            max_concurrency,
            Arc::new(id.to_string()),
        )
        .await?;

        if let Some(cleanup_fut) = self
            .backend
            .cleanup(&effective_output_dir, state.token.clone())
        {
            cleanup_fut.await;
        }

        let mut outputs: Outputs = state.scopes.write().await.take(Scopes::OUTPUT_INDEX).into();
        if let Some(section) = definition.output() {
            let indexes: HashMap<_, _> = section
                .declarations()
                .enumerate()
                .map(|(i, d)| (d.name().hashable(), i))
                .collect();
            outputs.sort_by(move |a, b| indexes[a].cmp(&indexes[b]))
        }

        // Write the outputs to the workflow's root directory
        write_json_file(root_dir.join(OUTPUTS_FILE), &outputs)?;
        Ok(outputs)
    }

    /// Evaluates a subgraph to completion.
    ///
    /// Note that this method is not `async` because it is indirectly recursive.
    ///
    /// The boxed future breaks the cycle that would otherwise exist when trying
    /// to have the Rust compiler create an opaque type for the future returned
    /// by an `async` method.
    fn evaluate_subgraph(
        state: Arc<State>,
        scope: ScopeIndex,
        subgraph: Subgraph,
        max_concurrency: u64,
        id: Arc<String>,
    ) -> BoxFuture<'static, EvaluationResult<()>> {
        async move {
            let token = state.token.clone();
            let mut futures = JoinSet::new();
            match Self::perform_subgraph_evaluation(
                state,
                scope,
                subgraph,
                max_concurrency,
                id,
                &mut futures,
            )
            .await
            {
                Ok(_) => {
                    // There should be no more pending futures
                    assert!(futures.is_empty());
                    Ok(())
                }
                Err(e) => {
                    // Cancel any outstanding futures and join them
                    token.cancel();
                    futures.join_all().await;
                    Err(e)
                }
            }
        }
        .boxed()
    }

    /// Performs subgraph evaluation.
    ///
    /// This exists as a separate function from `evaluate_subgraph` so that we
    /// can gracefully cancel outstanding futures on error.
    async fn perform_subgraph_evaluation(
        state: Arc<State>,
        scope: ScopeIndex,
        mut subgraph: Subgraph,
        max_concurrency: u64,
        id: Arc<String>,
        futures: &mut JoinSet<EvaluationResult<NodeIndex>>,
    ) -> EvaluationResult<()> {
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
                match &state.graph[node] {
                    WorkflowGraphNode::Call(stmt) => {
                        let call_name = stmt
                            .alias()
                            .map(|a| a.name())
                            .unwrap_or_else(|| stmt.target().names().last().unwrap());
                        debug!(
                            workflow_id = id.as_str(),
                            workflow_name = state.document.workflow().unwrap().name(),
                            document = state.document.uri().as_str(),
                            call_name = call_name.text(),
                            "evaluation of call statement has completed",
                        )
                    }
                    WorkflowGraphNode::Conditional(stmt, _) => debug!(
                        workflow_id = id.as_str(),
                        workflow_name = state.document.workflow().unwrap().name(),
                        document = state.document.uri().as_str(),
                        expr = {
                            let e = stmt.expr();
                            e.text().to_string()
                        },
                        "evaluation of conditional statement has completed",
                    ),
                    WorkflowGraphNode::Scatter(stmt, _) => {
                        let variable = stmt.variable();
                        debug!(
                            workflow_id = id.as_str(),
                            workflow_name = state.document.workflow().unwrap().name(),
                            document = state.document.uri().as_str(),
                            variable = variable.text(),
                            "evaluation of scatter statement has completed",
                        )
                    }
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
                trace!(
                    workflow_id = id.as_str(),
                    workflow_name = state.document.workflow().unwrap().name(),
                    document = state.document.uri().as_str(),
                    "evaluating node `{n:?}` ({node:?})",
                    n = state.graph[node]
                );
                match &state.graph[node] {
                    WorkflowGraphNode::Input(decl) => Self::evaluate_input(&id, &state, decl)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?,
                    WorkflowGraphNode::Decl(decl) => Self::evaluate_decl(&id, &state, scope, decl)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?,
                    WorkflowGraphNode::Output(decl) => Self::evaluate_output(&id, &state, decl)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?,
                    WorkflowGraphNode::Conditional(stmt, _) => {
                        let id = id.clone();
                        let state = state.clone();
                        let stmt = stmt.clone();
                        futures.spawn(async move {
                            Self::evaluate_conditional(
                                id,
                                state,
                                scope,
                                node,
                                &stmt,
                                max_concurrency,
                            )
                            .await?;
                            Ok(node)
                        });
                        awaiting.insert(node);
                    }
                    WorkflowGraphNode::Scatter(stmt, _) => {
                        let id = id.clone();
                        let state = state.clone();
                        let stmt = stmt.clone();
                        futures.spawn(async move {
                            let token = state.token.clone();
                            let mut futures = JoinSet::new();
                            match Self::evaluate_scatter(
                                id,
                                state,
                                scope,
                                node,
                                &stmt,
                                max_concurrency,
                                &mut futures,
                            )
                            .await
                            {
                                Ok(_) => {
                                    // All futures should have completed
                                    assert!(futures.is_empty());
                                    Ok(node)
                                }
                                Err(e) => {
                                    // Cancel any outstanding futures and join them
                                    token.cancel();
                                    futures.join_all().await;
                                    Err(e)
                                }
                            }
                        });
                        awaiting.insert(node);
                    }
                    WorkflowGraphNode::Call(stmt) => {
                        let id = id.clone();
                        let state = state.clone();
                        let stmt = stmt.clone();
                        futures.spawn(async move {
                            Self::evaluate_call(&id, state, scope, &stmt).await?;
                            Ok(node)
                        });
                        awaiting.insert(node);
                    }
                    WorkflowGraphNode::ExitConditional(_) | WorkflowGraphNode::ExitScatter(_) => {
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

    /// Evaluates a workflow input.
    async fn evaluate_input(
        id: &str,
        state: &State,
        decl: &Decl<SyntaxNode>,
    ) -> Result<(), Diagnostic> {
        let name = decl.name();
        let expected_ty = crate::convert_ast_type_v1(&state.document, &decl.ty())?;
        let expr = decl.expr();

        // Either use the specified input or evaluate the input's expression
        let (value, span) = match state.inputs.get(name.text()) {
            Some(input) => (input.clone(), name.span()),
            None => {
                if let Some(expr) = expr {
                    debug!(
                        workflow_id = id,
                        workflow_name = state.document.workflow().unwrap().name(),
                        document = state.document.uri().as_str(),
                        input_name = name.text(),
                        "evaluating input",
                    );

                    (
                        Self::evaluate_expr(state, Scopes::ROOT_INDEX, &expr).await?,
                        expr.span(),
                    )
                } else {
                    assert!(expected_ty.is_optional(), "type should be optional");
                    (Value::new_none(expected_ty.clone()), name.span())
                }
            }
        };

        // Coerce the value to the expected type
        let mut value = value
            .coerce(None, &expected_ty)
            .map_err(|e| runtime_type_mismatch(e, &expected_ty, name.span(), &value.ty(), span))?;

        // Ensure paths exist for WDL 1.2+
        if state
            .document
            .version()
            .expect("document should have a version")
            >= SupportedVersion::V1(V1::Two)
        {
            value
                .visit_paths_mut(expected_ty.is_optional(), &mut |optional, value| {
                    value.ensure_path_exists(optional, state.base_dir.as_local())
                })
                .map_err(|e| {
                    decl_evaluation_failed(
                        e,
                        state
                            .document
                            .workflow()
                            .expect("should have workflow")
                            .name(),
                        false,
                        name.text(),
                        Some(Io::Input),
                        name.span(),
                    )
                })?;
        }

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
        id: &str,
        state: &State,
        scope: ScopeIndex,
        decl: &Decl<SyntaxNode>,
    ) -> Result<(), Diagnostic> {
        let name = decl.name();
        let expected_ty = crate::convert_ast_type_v1(&state.document, &decl.ty())?;
        let expr = decl.expr().expect("declaration should have expression");

        debug!(
            workflow_id = id,
            workflow_name = state.document.workflow().unwrap().name(),
            document = state.document.uri().as_str(),
            decl_name = name.text(),
            "evaluating private declaration",
        );

        // Evaluate the decl's expression
        let value = Self::evaluate_expr(state, scope, &expr).await?;

        // Coerce the value to the expected type
        let mut value = value.coerce(None, &expected_ty).map_err(|e| {
            runtime_type_mismatch(e, &expected_ty, name.span(), &value.ty(), expr.span())
        })?;

        // Ensure paths exist for WDL 1.2+
        if state
            .document
            .version()
            .expect("document should have a version")
            >= SupportedVersion::V1(V1::Two)
        {
            value
                .visit_paths_mut(expected_ty.is_optional(), &mut |optional, value| {
                    value.ensure_path_exists(optional, state.base_dir.as_local())
                })
                .map_err(|e| {
                    decl_evaluation_failed(
                        e,
                        state
                            .document
                            .workflow()
                            .expect("should have workflow")
                            .name(),
                        false,
                        name.text(),
                        None,
                        name.span(),
                    )
                })?;
        }

        state
            .scopes
            .write()
            .await
            .get_mut(scope)
            .insert(name.text(), value);
        Ok(())
    }

    /// Evaluates a workflow output.
    async fn evaluate_output(
        id: &str,
        state: &State,
        decl: &Decl<SyntaxNode>,
    ) -> Result<(), Diagnostic> {
        let name = decl.name();
        let expected_ty = crate::convert_ast_type_v1(&state.document, &decl.ty())?;
        let expr = decl.expr().expect("declaration should have expression");

        debug!(
            workflow_id = id,
            workflow_name = state.document.workflow().unwrap().name(),
            document = state.document.uri().as_str(),
            output_name = name.text(),
            "evaluating output",
        );

        // Evaluate the decl's expression
        let value = Self::evaluate_expr(state, Scopes::OUTPUT_INDEX, &expr).await?;

        // Coerce the value to the expected type
        let mut value = value.coerce(None, &expected_ty).map_err(|e| {
            runtime_type_mismatch(e, &expected_ty, name.span(), &value.ty(), expr.span())
        })?;

        // Finally ensure output files exist
        value
            .visit_paths_mut(expected_ty.is_optional(), &mut |optional, value| {
                let path = match value {
                    PrimitiveValue::File(path) => path,
                    PrimitiveValue::Directory(path) => path,
                    _ => unreachable!("only file and directory values should be visited"),
                };

                if !path::is_url(path.as_str()) && Path::new(path.as_str()).is_relative() {
                    bail!("relative path `{path}` cannot be used as a workflow output");
                }

                value.ensure_path_exists(optional, state.base_dir.as_local())
            })
            .map_err(|e| {
                decl_evaluation_failed(
                    e,
                    state
                        .document
                        .workflow()
                        .expect("should have workflow")
                        .name(),
                    false,
                    name.text(),
                    Some(Io::Output),
                    name.span(),
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
    async fn evaluate_conditional(
        id: Arc<String>,
        state: Arc<State>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: &ConditionalStatement<SyntaxNode>,
        max_concurrency: u64,
    ) -> EvaluationResult<()> {
        let expr = stmt.expr();

        debug!(
            workflow_id = id.as_str(),
            workflow_name = state.document.workflow().unwrap().name(),
            document = state.document.uri().as_str(),
            expr = expr.text().to_string(),
            "evaluating conditional statement",
        );

        // Evaluate the conditional expression
        let value = Self::evaluate_expr(&state, parent, &expr)
            .await
            .map_err(|d| EvaluationError::new(state.document.clone(), d))?;

        if value
            .coerce(None, &PrimitiveType::Boolean.into())
            .map_err(|e| {
                EvaluationError::new(
                    state.document.clone(),
                    if_conditional_mismatch(e, &value.ty(), expr.span()),
                )
            })?
            .unwrap_boolean()
        {
            debug!(
                workflow_id = id.as_str(),
                workflow_name = state.document.workflow().unwrap().name(),
                document = state.document.uri().as_str(),
                "conditional statement branch was taken and subgraph will be evaluated"
            );

            // Intentionally drop the write lock before evaluating the subgraph
            let scope = { state.scopes.write().await.alloc(parent) };

            // Evaluate the subgraph
            Self::evaluate_subgraph(
                state.clone(),
                scope,
                state.subgraphs[&entry].clone(),
                max_concurrency,
                id,
            )
            .await?;

            // Promote all values in the scope to the parent scope as optional
            let mut scopes = state.scopes.write().await;
            let (parent, child) = scopes.parent_mut(scope);
            for (name, value) in child.local() {
                parent.insert(name.to_string(), value.clone());
            }

            scopes.free(scope);
        } else {
            debug!(
                workflow_id = id.as_str(),
                workflow_name = state.document.workflow().unwrap().name(),
                document = state.document.uri().as_str(),
                "conditional statement branch was not taken and subgraph will be skipped"
            );

            // Conditional evaluated to false; set the expected names to `None` in the
            // parent scope
            let mut scopes = state.scopes.write().await;
            let parent = scopes.get_mut(parent);
            let scope = state
                .document
                .find_scope_by_position(
                    stmt.braced_scope_span()
                        .expect("should have braced scope span")
                        .start(),
                )
                .expect("should have scope");

            for (name, n) in scope.names() {
                if let Type::Call(ty) = n.ty() {
                    parent.insert(
                        name.to_string(),
                        CallValue::new_unchecked(
                            ty.promote(PromotionKind::Conditional),
                            Outputs::from_iter(
                                ty.outputs()
                                    .iter()
                                    .map(|(n, o)| (n.clone(), Value::new_none(o.ty().optional()))),
                            )
                            .into(),
                        ),
                    );
                } else {
                    parent.insert(name.to_string(), Value::new_none(n.ty().optional()));
                }
            }
        }

        Ok(())
    }

    /// Evaluates a workflow scatter statement.
    #[allow(clippy::too_many_arguments)]
    async fn evaluate_scatter(
        id: Arc<String>,
        state: Arc<State>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: &ScatterStatement<SyntaxNode>,
        max_concurrency: u64,
        futures: &mut JoinSet<EvaluationResult<(usize, ScopeIndex)>>,
    ) -> EvaluationResult<()> {
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
            for (name, value) in scopes.get_mut(scope).local().skip(2) {
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

        let variable = stmt.variable();
        let expr = stmt.expr();

        debug!(
            workflow_id = id.as_str(),
            workflow_name = state.document.workflow().unwrap().name(),
            document = state.document.uri().as_str(),
            variable = variable.text(),
            "evaluating scatter statement",
        );

        // Evaluate the scatter array expression
        let value = Self::evaluate_expr(&state, parent, &expr)
            .await
            .map_err(|d| EvaluationError::new(state.document.clone(), d))?;

        let array = value
            .as_array()
            .ok_or_else(|| {
                EvaluationError::new(
                    state.document.clone(),
                    type_is_not_array(&value.ty(), expr.span()),
                )
            })?
            .as_slice();

        let mut gathers: HashMap<_, Gather> = HashMap::new();
        for (i, value) in array.iter().enumerate() {
            if state.token.is_cancelled() {
                return Err(anyhow!("workflow evaluation has been cancelled").into());
            }

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
                let id = id.clone();
                futures.spawn(async move {
                    Self::evaluate_subgraph(state.clone(), scope, subgraph, max_concurrency, id)
                        .await?;

                    Ok((i, scope))
                });
            }

            // If we've reached the concurrency limit, await one of the futures to complete
            if futures.len() as u64 >= max_concurrency {
                await_next(futures, &state.scopes, &mut gathers, array.len()).await?;
            }
        }

        // Complete any outstanding futures
        while !futures.is_empty() {
            await_next(futures, &state.scopes, &mut gathers, array.len()).await?;
        }

        let mut scopes = state.scopes.write().await;
        let scope = scopes.get_mut(parent);
        for (name, gather) in gathers {
            scope.insert(name, gather.into_value());
        }

        Ok(())
    }

    /// Evaluates a workflow call statement.
    async fn evaluate_call(
        id: &str,
        state: Arc<State>,
        scope: ScopeIndex,
        stmt: &CallStatement<SyntaxNode>,
    ) -> EvaluationResult<()> {
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
            async fn evaluate(
                self,
                caller_id: &str,
                document: &Document,
                inputs: Inputs,
                root_dir: &Path,
                callee_id: &str,
            ) -> EvaluationResult<Outputs> {
                match self {
                    Evaluator::Task(task, evaluator) => {
                        debug!(caller_id, callee_id, "evaluating call to task");
                        evaluator
                            .perform_evaluation(
                                document,
                                task,
                                &inputs.unwrap_task_inputs(),
                                root_dir,
                                callee_id,
                            )
                            .await?
                            .outputs
                    }
                    Evaluator::Workflow(evaluator) => {
                        debug!(caller_id, callee_id, "evaluating call to workflow");
                        evaluator
                            .perform_evaluation(
                                document,
                                inputs.unwrap_workflow_inputs(),
                                root_dir,
                                callee_id,
                            )
                            .await
                    }
                }
            }
        }

        let alias = stmt.alias();
        let target = stmt.target();
        let mut names = target.names().peekable();
        let mut namespace = None;
        let mut target = None;

        // Resolve the target and namespace for the call
        while let Some(name) = names.next() {
            if names.peek().is_none() {
                target = Some(name);
                break;
            }

            if namespace.is_some() {
                return Err(EvaluationError::new(
                    state.document.clone(),
                    only_one_namespace(name.span()),
                ));
            }

            let ns = state.document.namespace(name.text()).ok_or_else(|| {
                EvaluationError::new(state.document.clone(), unknown_namespace(&name))
            })?;

            namespace = Some((name, ns));
        }

        let target = target.expect("expected at least one name");

        let alias = alias
            .as_ref()
            .map(|t| t.name())
            .unwrap_or_else(|| target.clone());

        debug!(
            workflow_id = id,
            workflow_name = state.document.workflow().unwrap().name(),
            document = state.document.uri().as_str(),
            call_name = alias.text(),
            "evaluating call statement",
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
            return Err(EvaluationError::new(
                state.document.clone(),
                recursive_workflow_call(target.text(), target.span()),
            ));
        }

        // Determine the inputs and evaluator to use for the task or workflow call
        let inputs = state.inputs.calls().get(alias.text()).cloned();
        let document = namespace
            .as_ref()
            .map(|(_, ns)| ns.document())
            .unwrap_or(&state.document);
        let (mut inputs, evaluator) = match document.task_by_name(target.text()) {
            Some(task) => (
                inputs.unwrap_or_else(|| Inputs::Task(Default::default())),
                Evaluator::Task(
                    task,
                    TaskEvaluator::new_unchecked(
                        state.config.clone(),
                        state.backend.clone(),
                        state.token.clone(),
                        state.downloader.clone(),
                    ),
                ),
            ),
            _ => match document.workflow() {
                Some(workflow) if workflow.name() == target.text() => (
                    inputs.unwrap_or_else(|| Inputs::Workflow(Default::default())),
                    Evaluator::Workflow(WorkflowEvaluator {
                        config: state.config.clone(),
                        backend: state.backend.clone(),
                        token: state.token.clone(),
                        downloader: state.downloader.clone(),
                    }),
                ),
                _ => {
                    return Err(EvaluationError::new(
                        state.document.clone(),
                        unknown_task_or_workflow(
                            namespace.as_ref().map(|(_, ns)| ns.span()),
                            target.text(),
                            target.span(),
                        ),
                    ));
                }
            },
        };

        // Evaluate the inputs
        let scatter_index = Self::evaluate_call_inputs(&state, stmt, scope, &mut inputs)
            .await
            .map_err(|d| EvaluationError::new(state.document.clone(), d))?;

        let dir = format!(
            "{alias}{sep}{scatter_index}",
            alias = alias.text(),
            sep = if scatter_index.is_empty() { "" } else { "-" },
        );

        let call_id = format_id(
            namespace.as_ref().map(|(n, _)| n.text()),
            target.text(),
            alias.text(),
            &scatter_index,
        );

        // Finally, evaluate the task or workflow and return the outputs
        let outputs = evaluator
            .evaluate(id, document, inputs, &state.calls_dir.join(&dir), &call_id)
            .await
            .map_err(|mut e| {
                if let EvaluationError::Source(e) = &mut e {
                    e.backtrace.push(CallLocation {
                        document: state.document.clone(),
                        span: stmt
                            .token::<CallKeyword<SyntaxToken>>()
                            .expect("should have call keyword")
                            .span(),
                    });
                }

                e
            })?
            .with_name(alias.text());

        let ty = state
            .document
            .workflow()
            .expect("should have workflow")
            .calls()
            .get(alias.text())
            .expect("should have call");
        state.scopes.write().await.get_mut(scope).insert(
            alias.text(),
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
        expr: &Expr<SyntaxNode>,
    ) -> Result<Value, Diagnostic> {
        let scopes = state.scopes.read().await;
        ExprEvaluator::new(WorkflowEvaluationContext::new(
            state,
            scopes.reference(scope),
        ))
        .evaluate_expr(expr)
        .await
    }

    /// Evaluates the call inputs of a call statement.
    ///
    /// Returns the scatter index for the provided scope.
    ///
    /// This takes a read lock on the scopes.
    async fn evaluate_call_inputs(
        state: &State,
        stmt: &CallStatement<SyntaxNode>,
        scope: ScopeIndex,
        inputs: &mut Inputs,
    ) -> Result<String, Diagnostic> {
        let scopes = state.scopes.read().await;
        for input in stmt.inputs() {
            let name = input.name();
            let value = match input.expr() {
                Some(expr) => {
                    let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                        state,
                        scopes.reference(scope),
                    ));

                    evaluator.evaluate_expr(&expr).await?
                }
                None => scopes
                    .reference(scope)
                    .lookup(name.text())
                    .cloned()
                    .ok_or_else(|| unknown_name(name.text(), name.span()))?,
            };

            let prev = inputs.set(input.name().text(), value);
            assert!(
                prev.is_none(),
                "attempted to override a specified call input"
            );
        }

        Ok(scopes.scatter_index(scope))
    }
}

#[cfg(test)]
#[cfg(feature = "codespan-reporting")]
mod test {
    use std::fs::read_to_string;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tokio::sync::broadcast::error::RecvError;
    use wdl_analysis::Analyzer;
    use wdl_analysis::Config as AnalysisConfig;
    use wdl_analysis::DiagnosticsConfig;

    use super::*;
    use crate::config::BackendConfig;

    #[tokio::test]
    async fn it_writes_input_and_output_files() {
        let root_dir = TempDir::new().expect("failed to create temporary directory");
        fs::write(
            root_dir.path().join("source.wdl"),
            r#"
version 1.2

task foo {
    input {
        String a
        Int b
        Array[String] c
    }

    command <<<>>>

    output {
        String x = a
        Int y = b
        Array[String] z = c
    }
}

workflow test {
    input {
        String a
        Int b
        Array[String] c
    }

    call foo {
        a = "foo",
        b = 10,
        c = ["foo", "bar", "baz"]
    }

    call foo as bar {
        a = "bar",
        b = 1,
        c = []
    }

    output {
        String x = a
        Int y = b
        Array[String] z = c
    }
}
"#,
        )
        .expect("failed to write WDL source file");

        // Analyze the source file
        let analyzer = Analyzer::new(
            AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
            |(), _, _, _| async {},
        );
        analyzer
            .add_directory(root_dir.path().to_path_buf())
            .await
            .expect("failed to add directory");
        let results = analyzer
            .analyze(())
            .await
            .expect("failed to analyze document");
        assert_eq!(results.len(), 1, "expected only one result");

        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(Default::default()),
            )]
            .into(),
            ..Default::default()
        };
        let evaluator = WorkflowEvaluator::new(config, CancellationToken::new(), None)
            .await
            .unwrap();

        // Evaluate the `test` workflow in `source.wdl` using the default local backend
        let mut inputs = WorkflowInputs::default();
        inputs.set("a", "qux".to_string());
        inputs.set("b", 1234);
        inputs.set(
            "c",
            Array::new(
                None,
                ArrayType::new(PrimitiveType::String),
                ["jam".to_string(), "cakes".to_string()],
            )
            .unwrap(),
        );
        let outputs_dir = root_dir.path().join("outputs");
        let outputs = evaluator
            .evaluate(
                results.first().expect("should have result").document(),
                inputs,
                &outputs_dir,
            )
            .await
            .map_err(|e| e.to_string())
            .expect("failed to evaluate workflow");
        assert_eq!(outputs.iter().count(), 3, "expected three outputs");

        // Check the workflow inputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("inputs.json"))
                .expect("failed to read workflow `inputs.json`"),
            "{\n  \"a\": \"qux\",\n  \"b\": 1234,\n  \"c\": [\n    \"jam\",\n    \"cakes\"\n  ]\n}"
        );

        // Check the workflow outputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("outputs.json"))
                .expect("failed to read workflow `outputs.json`"),
            "{\n  \"x\": \"qux\",\n  \"y\": 1234,\n  \"z\": [\n    \"jam\",\n    \"cakes\"\n  ]\n}"
        );

        // Check the `foo` call inputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("calls/foo/inputs.json"))
                .expect("failed to read foo `inputs.json`"),
            "{\n  \"a\": \"foo\",\n  \"b\": 10,\n  \"c\": [\n    \"foo\",\n    \"bar\",\n    \
             \"baz\"\n  ]\n}"
        );

        // Check the `foo` call outputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("calls/foo/outputs.json"))
                .expect("failed to read foo `outputs.json`"),
            "{\n  \"x\": \"foo\",\n  \"y\": 10,\n  \"z\": [\n    \"foo\",\n    \"bar\",\n    \
             \"baz\"\n  ]\n}"
        );

        // Check the `bar` call inputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("calls/bar/inputs.json"))
                .expect("failed to read bar `inputs.json`"),
            "{\n  \"a\": \"bar\",\n  \"b\": 1,\n  \"c\": []\n}"
        );

        // Check the `bar` call outputs.json
        assert_eq!(
            read_to_string(outputs_dir.join("calls/bar/outputs.json"))
                .expect("failed to read bar `outputs.json`"),
            "{\n  \"x\": \"bar\",\n  \"y\": 1,\n  \"z\": []\n}"
        );
    }

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
        let analyzer = Analyzer::new(
            AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
            |(), _, _, _| async {},
        );
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
            tasks_created: AtomicUsize,
            tasks_started: AtomicUsize,
            tasks_completed: AtomicUsize,
        }

        // Use a progress callback that simply increments the appropriate counter
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(Default::default()),
            )]
            .into(),
            ..Default::default()
        };
        let state = Arc::<State>::default();
        let events_state = state.clone();
        let (events_tx, mut events_rx) = broadcast::channel(100);
        let events = tokio::spawn(async move {
            loop {
                match events_rx.recv().await {
                    Ok(event) => match event {
                        Event::TaskCreated { name, tes_id, .. } => {
                            assert!(name.starts_with("t-"));
                            assert!(tes_id.is_none());
                            events_state.tasks_created.fetch_add(1, Ordering::SeqCst);
                        }
                        Event::TaskStarted { .. } => {
                            events_state.tasks_started.fetch_add(1, Ordering::SeqCst);
                        }
                        Event::TaskCompleted { exit_statuses, .. } => {
                            assert_eq!(exit_statuses.len(), 1);
                            assert_eq!(exit_statuses[0].code().expect("should have code"), 0);
                            events_state.tasks_completed.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => panic!("unexpected task event"),
                    },
                    Err(RecvError::Closed) => break,
                    Err(e) => panic!("failed to receive event: {e}"),
                }
            }
        });

        let evaluator = WorkflowEvaluator::new(config, CancellationToken::new(), Some(events_tx))
            .await
            .unwrap();

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
            )
            .await
            .map_err(|e| e.to_string())
            .expect("failed to evaluate workflow");

        drop(evaluator);
        events.await.expect("failed to await events");

        assert_eq!(outputs.iter().count(), 0, "expected no outputs");

        // Ensure the counters are what is expected based on the WDL
        assert_eq!(state.tasks_created.load(Ordering::SeqCst), 10);
        assert_eq!(state.tasks_started.load(Ordering::SeqCst), 10);
        assert_eq!(state.tasks_completed.load(Ordering::SeqCst), 10);
    }
}
