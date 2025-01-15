//! Implementation of evaluation for V1 workflows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::future::Future;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Context;
use anyhow::anyhow;
use futures::FutureExt;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indexmap::IndexMap;
use petgraph::Direction;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Bfs;
use petgraph::visit::EdgeRef;
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
use wdl_analysis::document::Workflow;
use wdl_analysis::eval::v1::WorkflowGraphBuilder;
use wdl_analysis::eval::v1::WorkflowGraphNode;
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
use wdl_ast::v1::Decl;
use wdl_ast::v1::ScatterStatement;

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
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::output_evaluation_failed;
use crate::diagnostics::runtime_type_mismatch;
use crate::v1::ExprEvaluator;
use crate::v1::TaskEvaluator;

/// A "hidden" scope variable for representing the scope's scatter index.
///
/// This is only present in the scope created for a scatter statement.
///
/// The name is intentionally not a valid WDL identifier so that it cannot
/// conflict with any other variables in scope.
const SCATTER_INDEX_VAR: &str = "$idx";

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
        Array::new_unchecked(self.element_ty, self.elements)
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
struct Subgraph {
    /// The associated evaluation graph.
    graph: Rc<DiGraph<WorkflowGraphNode, ()>>,
    /// The nodes comprising the subgraph.
    ///
    /// Stores a node index mapped to the node's indegree count.
    nodes: HashMap<NodeIndex, usize>,
}

impl Subgraph {
    /// Constructs a new subgraph from the given evaluation graph.
    ///
    /// Initially, the subgraph will contain every node in the evaluation graph
    /// until it is split.
    fn new(graph: Rc<DiGraph<WorkflowGraphNode, ()>>) -> Self {
        let mut nodes = HashMap::with_capacity(graph.node_count());
        for index in graph.node_indices() {
            nodes.insert(
                index,
                graph.edges_directed(index, Direction::Incoming).count(),
            );
        }

        Self { graph, nodes }
    }

    /// Splits this subgraph and returns a map of entry nodes to the
    /// corresponding subgraph.
    ///
    /// This subgraph is modified to replace any direct subgraphs with only the
    /// entry and exit nodes.
    fn split(&mut self) -> HashMap<NodeIndex, Subgraph> {
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
            graph: &DiGraph<WorkflowGraphNode, ()>,
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
            graph: &Rc<DiGraph<WorkflowGraphNode, ()>>,
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
                        subgraphs.insert(index, Subgraph {
                            graph: graph.clone(),
                            nodes,
                        });
                    }
                    _ => {}
                }
            }
        }

        let mut subgraphs = HashMap::new();
        split_recurse(&self.graph, &mut self.nodes, &mut subgraphs);
        subgraphs
    }

    /// Evaluates the subgraph to completion.
    ///
    /// Note that while graph evaluation is asynchronous, scatter evaluation is
    /// not concurrent (but spawned task processes are); progress is made on the
    /// subgraph and if a subgraph blocks waiting on a task execution,
    /// progress is made elsewhere on the graph when possible to do so.
    async fn evaluate(mut self, state: &State<'_>, scope: ScopeIndex) -> EvaluationResult<()> {
        let mut futures = FuturesUnordered::new();
        // The set of nodes being processed
        let mut processing: Vec<NodeIndex> = Vec::new();
        // The set of graph nodes being awaited on
        let mut awaiting: HashSet<NodeIndex> = HashSet::new();

        while !self.nodes.is_empty() {
            // Add nodes with indegree 0 that we aren't already waiting on
            processing.extend(self.nodes.iter().filter_map(|(node, indegree)| {
                if *indegree == 0 && !awaiting.contains(node) {
                    Some(*node)
                } else {
                    None
                }
            }));

            // If no graph nodes can be processed, await on any futures
            if processing.is_empty() {
                let node: EvaluationResult<NodeIndex> = futures
                    .next()
                    .await
                    .expect("should have a future to wait on");

                let node = node?;
                match &self.graph[node] {
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
                        "evaluation of scatter statement `{name}` has completed; removing from \
                         evaluation graph",
                        name = stmt.variable().as_str(),
                    ),
                    _ => unreachable!(),
                }

                awaiting.remove(&node);
                self.remove_node(node);

                // Continue to see if we can progress further in the subgraph; if not we'll
                // await more futures
                continue;
            }

            // Process the node now or push a future for later completion
            for node in processing.iter().copied() {
                trace!("evaluating node `{:?}` ({node:?})", self.graph[node]);
                match self.graph[node].clone() {
                    WorkflowGraphNode::Input(decl) => Self::evaluate_input(state, &decl)?,
                    WorkflowGraphNode::Decl(decl) => Self::evaluate_decl(state, scope, &decl)?,
                    WorkflowGraphNode::Output(decl) => Self::evaluate_output(state, &decl)?,
                    WorkflowGraphNode::Conditional(stmt, _) => {
                        futures.push(
                            async move {
                                Self::evaluate_conditional(state, scope, node, &stmt).await?;
                                Ok(node)
                            }
                            .boxed_local(),
                        );
                        awaiting.insert(node);
                    }
                    WorkflowGraphNode::Scatter(stmt, _) => {
                        futures.push(
                            async move {
                                Self::evaluate_scatter(state, scope, node, &stmt).await?;
                                Ok(node)
                            }
                            .boxed_local(),
                        );
                        awaiting.insert(node);
                    }
                    WorkflowGraphNode::Call(stmt) => {
                        futures.push(
                            async move {
                                Self::evaluate_call(state, scope, &stmt).await?;
                                Ok(node)
                            }
                            .boxed_local(),
                        );
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

                self.remove_node(node);
            }
        }

        Ok(())
    }

    /// Removes the given node from the subgraph.
    ///
    /// # Panics
    ///
    /// Panics if the node's indegree is not 0.
    fn remove_node(&mut self, node: NodeIndex) {
        let indegree = self.nodes.remove(&node);
        assert_eq!(
            indegree,
            Some(0),
            "removed a node with an indegree greater than 0"
        );

        // Decrement the indegrees of connected nodes
        for edge in self.graph.edges_directed(node, Direction::Outgoing) {
            if let Some(indegree) = self.nodes.get_mut(&edge.target()) {
                *indegree -= 1;
            }
        }
    }

    /// Evaluates a workflow input.
    fn evaluate_input(state: &State<'_>, decl: &Decl) -> EvaluationResult<()> {
        let name = decl.name();
        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;

        let mut scopes = state.scopes.borrow_mut();
        let (value, span) = match state.inputs.get(name.as_str()) {
            Some(input) => (input.clone(), name.span()),
            None => {
                if let Some(expr) = decl.expr() {
                    debug!(
                        "evaluating input `{name}` for workflow `{workflow}` in `{uri}`",
                        name = name.as_str(),
                        workflow = state.workflow.name(),
                        uri = state.document.uri(),
                    );

                    let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                        state.document,
                        scopes.reference(Scopes::OUTPUT_INDEX),
                        &state.work_dir,
                        &state.temp_dir,
                    ));
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

        scopes
            .get_mut(Scopes::ROOT_INDEX)
            .insert(name.as_str(), value);
        Ok(())
    }

    /// Evaluates a workflow output.
    fn evaluate_output(state: &State<'_>, decl: &Decl) -> EvaluationResult<()> {
        let name = decl.name();
        debug!(
            "evaluating output `{name}` for workflow `{workflow}` in `{uri}`",
            name = name.as_str(),
            workflow = state.workflow.name(),
            uri = state.document.uri()
        );

        let mut scopes = state.scopes.borrow_mut();
        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;
        let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
            state.document,
            scopes.reference(Scopes::OUTPUT_INDEX),
            &state.work_dir,
            &state.temp_dir,
        ));

        // First coerce the output value to the expected type
        let expr = decl.expr().expect("outputs should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;
        let mut value = value
            .coerce(&ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;

        // Finally, join any paths with the working directory, checking for existence
        value
            .join_paths(&state.work_dir, true, ty.is_optional())
            .map_err(|e| output_evaluation_failed(e, state.workflow.name(), false, &name))?;

        scopes
            .get_mut(Scopes::OUTPUT_INDEX)
            .insert(name.as_str(), value);
        Ok(())
    }

    /// Evaluates a workflow private declaration.
    fn evaluate_decl(state: &State<'_>, scope: ScopeIndex, decl: &Decl) -> EvaluationResult<()> {
        let name = decl.name();
        debug!(
            "evaluating private declaration `{name}` for workflow `{workflow}` in `{uri}`",
            name = name.as_str(),
            workflow = state.workflow.name(),
            uri = state.document.uri(),
        );

        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(state.document, &decl_ty)?;

        let mut scopes = state.scopes.borrow_mut();
        let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
            state.document,
            scopes.reference(scope),
            &state.work_dir,
            &state.temp_dir,
        ));

        let expr = decl.expr().expect("private decls should have expressions");
        let value = evaluator.evaluate_expr(&expr)?;
        let value = value
            .coerce(&ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;

        scopes.get_mut(scope).insert(name.as_str(), value);
        Ok(())
    }

    /// Evaluates a workflow conditional statement.
    async fn evaluate_conditional(
        state: &State<'_>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: &ConditionalStatement,
    ) -> EvaluationResult<()> {
        let expr = stmt.expr();

        debug!(
            "evaluating conditional statement `{expr}` for workflow `{workflow}` in `{uri}`",
            expr = expr.syntax().text(),
            workflow = state.workflow.name(),
            uri = state.document.uri()
        );

        let value = {
            let scopes = state.scopes.borrow();
            let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                state.document,
                scopes.reference(parent),
                &state.work_dir,
                &state.temp_dir,
            ));

            evaluator.evaluate_expr(&expr)?
        };

        if value
            .coerce(&PrimitiveType::Boolean.into())
            .map_err(|e| if_conditional_mismatch(e, &value.ty(), expr.span()))?
            .unwrap_boolean()
        {
            debug!("conditional statement branch was taken; evaluating subgraph");

            // Drop the borrow on `scopes` before evaluating the subgraph
            let scope = { state.scopes.borrow_mut().alloc(parent) };

            state.subgraphs[&entry]
                .clone()
                .evaluate(state, scope)
                .await?;

            // Promote all values in the scope to the parent scope as optional
            let mut scopes = state.scopes.borrow_mut();
            let (parent, child) = scopes.parent_mut(scope);
            for (name, value) in child.iter() {
                parent.insert(name.to_string(), value.clone_as_optional());
            }

            scopes.free(scope);
        } else {
            debug!("conditional statement branch was not taken; subgraph was skipped");

            // Conditional evaluated to false; set the expected names to `None` in the
            // parent scope
            let mut scopes = state.scopes.borrow_mut();
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
    async fn evaluate_scatter(
        state: &State<'_>,
        parent: ScopeIndex,
        entry: NodeIndex,
        stmt: &ScatterStatement,
    ) -> EvaluationResult<()> {
        /// Awaits the next future in the set of futures.
        async fn await_next<T: Future<Output = EvaluationResult<(usize, ScopeIndex)>>>(
            futures: &mut FuturesUnordered<T>,
            scopes: &RefCell<Scopes>,
            gathers: &mut HashMap<String, Gather>,
            capacity: usize,
        ) -> EvaluationResult<()> {
            let (index, scope) = futures
                .next()
                .await
                .expect("should have a future to wait on")?;

            // Append the result to the gather (the first two variables in scope are always
            // the scatter index and variable)
            let mut scopes = scopes.borrow_mut();
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

        let variable = stmt.variable();

        debug!(
            "evaluating scatter statement `{variable}` for workflow `{workflow}` in `{uri}`",
            variable = variable.as_str(),
            workflow = state.workflow.name(),
            uri = state.document.uri()
        );

        let expr = stmt.expr();
        let value = {
            let scopes = state.scopes.borrow();
            let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                state.document,
                scopes.reference(parent),
                &state.work_dir,
                &state.temp_dir,
            ));

            evaluator.evaluate_expr(&expr)?
        };

        let array = value
            .as_array()
            .ok_or_else(|| type_is_not_array(&value.ty(), expr.span()))?
            .as_slice();

        let max_concurrency = state.backend.max_concurrency();
        let mut futures = FuturesUnordered::new();
        let mut gathers: HashMap<_, Gather> = HashMap::new();
        for (i, value) in array.iter().enumerate() {
            // Allocate a scope
            let scope = {
                let mut scopes = state.scopes.borrow_mut();
                let index = scopes.alloc(parent);
                let scope = scopes.get_mut(index);
                scope.insert(
                    SCATTER_INDEX_VAR,
                    i64::try_from(i).map_err(|_| anyhow!("array index out of bounds"))?,
                );
                scope.insert(variable.as_str(), value.clone());
                index
            };

            // Evaluate the subgraph as a future
            futures.push(
                state.subgraphs[&entry]
                    .clone()
                    .evaluate(state, scope)
                    .map(move |r| r.map(|_| (i, scope))),
            );

            // If we've reached the concurrency limit, await one of the futures to complete
            if futures.len() >= max_concurrency {
                await_next(&mut futures, &state.scopes, &mut gathers, array.len()).await?;
            }
        }

        // Complete any outstanding futures
        while !futures.is_empty() {
            await_next(&mut futures, &state.scopes, &mut gathers, array.len()).await?;
        }

        let mut scopes = state.scopes.borrow_mut();
        let scope = scopes.get_mut(parent);
        for (name, gather) in gathers {
            scope.insert(name, gather.into_value());
        }

        Ok(())
    }

    /// Evaluates a workflow call statement.
    async fn evaluate_call(
        state: &State<'_>,
        scope: ScopeIndex,
        stmt: &CallStatement,
    ) -> EvaluationResult<()> {
        /// Abstracts evaluation for both task and workflow calls.
        enum Evaluator<'a> {
            /// Used to evaluate a task call.
            Task(&'a Task, TaskEvaluator<'a>),
            /// Used to evaluate a workflow call.
            Workflow(WorkflowEvaluator),
        }

        impl Evaluator<'_> {
            /// Runs evaluation with the given inputs.
            async fn evaluate(
                self,
                document: &Document,
                inputs: &Inputs,
                root_dir: &Path,
                id: &str,
            ) -> EvaluationResult<Outputs> {
                match self {
                    Evaluator::Task(task, mut evaluator) => {
                        debug!("evaluating call to task `{id}`");
                        evaluator
                            .evaluate(
                                document,
                                task,
                                inputs.as_task_inputs().expect("should be task inputs"),
                                root_dir,
                                id,
                            )
                            .await?
                            .outputs
                    }
                    Evaluator::Workflow(mut evaluator) => {
                        debug!("evaluating call to workflow `{id}`");
                        evaluator
                            .evaluate(
                                document,
                                inputs
                                    .as_workflow_inputs()
                                    .expect("should be workflow inputs"),
                                root_dir,
                            )
                            .await
                    }
                }
            }
        }

        let alias = stmt.alias().map(|a| a.name());
        let mut names = stmt.target().names().peekable();
        let mut namespace = None;
        let mut target = None;

        while let Some(n) = names.next() {
            if names.peek().is_none() {
                target = Some(n);
                break;
            }

            if namespace.is_some() {
                return Err(only_one_namespace(n.span()).into());
            }

            namespace = Some(
                state
                    .document
                    .namespace(n.as_str())
                    .ok_or_else(|| unknown_namespace(&n))?,
            );
        }

        let target = target.expect("expected at least one name");

        let alias = alias
            .as_ref()
            .map(|x| x.as_str())
            .unwrap_or_else(|| target.as_str());

        debug!(
            "evaluating call statement `{alias}` for workflow `{workflow}` in `{uri}`",
            workflow = state.workflow.name(),
            uri = state.document.uri(),
        );

        // Check for a directly recursive workflow call
        if namespace.is_none()
            && target.as_str()
                == state
                    .document
                    .workflow()
                    .expect("should have workflow")
                    .name()
        {
            return Err(recursive_workflow_call(&target).into());
        }

        let inputs = state.inputs.calls().get(alias).cloned();
        let document = namespace.map(|ns| ns.document()).unwrap_or(state.document);
        let (mut inputs, evaluator) = if let Some(task) = document.task_by_name(target.as_str()) {
            (
                inputs.unwrap_or_else(|| Inputs::Task(Default::default())),
                Evaluator::Task(task, TaskEvaluator::new(state.backend.as_ref())),
            )
        } else {
            match document.workflow() {
                Some(workflow) if workflow.name() == target.as_str() => (
                    inputs.unwrap_or_else(|| Inputs::Workflow(Default::default())),
                    Evaluator::Workflow(WorkflowEvaluator::new(state.backend.clone())),
                ),
                _ => {
                    return Err(
                        unknown_task_or_workflow(namespace.map(|ns| ns.span()), &target).into(),
                    );
                }
            }
        };

        for input in stmt.inputs() {
            let name = input.name();
            let scopes = state.scopes.borrow();
            let value = match input.expr() {
                Some(expr) => {
                    let mut evaluator = ExprEvaluator::new(WorkflowEvaluationContext::new(
                        state.document,
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

        let scatter_index = state.scopes.borrow().scatter_index(scope);

        let dir = format!(
            "{alias}{sep}{scatter_index}",
            sep = if scatter_index.is_empty() { "" } else { "-" },
        );

        let task_id = if alias != target.as_str() {
            format!(
                "{target}-{alias}{sep}{scatter_index}",
                target = target.as_str(),
                sep = if scatter_index.is_empty() { "" } else { "-" },
            )
        } else {
            format!(
                "{alias}{sep}{scatter_index}",
                sep = if scatter_index.is_empty() { "" } else { "-" },
            )
        };

        let outputs = evaluator
            .evaluate(document, &inputs, &state.calls_dir.join(&dir), &task_id)
            .await?
            .with_name(alias);

        let ty = state.workflow.calls().get(alias).expect("should have call");
        state.scopes.borrow_mut().get_mut(scope).insert(
            alias,
            Value::Call(CallValue::new_unchecked(ty.clone(), Arc::new(outputs))),
        );

        Ok(())
    }
}

/// Represents workflow evaluation state.
struct State<'a> {
    /// The document containing the workflow being evaluated.
    document: &'a Document,
    /// The workflow being evaluated.
    workflow: &'a Workflow,
    /// The workflow's inputs.
    inputs: &'a WorkflowInputs,
    /// The task execution backend.
    backend: &'a Arc<dyn TaskExecutionBackend>,
    /// The scopes used in workflow evaluation.
    scopes: RefCell<Scopes>,
    /// The map from graph node index to subgraph.
    subgraphs: HashMap<NodeIndex, Subgraph>,
    /// The workflow evaluation working directory path.
    work_dir: PathBuf,
    /// The workflow evaluation temp directory path.
    temp_dir: PathBuf,
    /// The calls directory path.
    calls_dir: PathBuf,
}

impl<'a> State<'a> {
    /// Constructs a new workflow evaluation state.
    fn new(
        document: &'a Document,
        workflow: &'a Workflow,
        inputs: &'a WorkflowInputs,
        backend: &'a Arc<dyn TaskExecutionBackend>,
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
            document,
            workflow,
            inputs,
            backend,
            scopes: Default::default(),
            subgraphs,
            work_dir,
            temp_dir,
            calls_dir,
        })
    }
}

/// Represents a WDL V1 workflow evaluator.
pub struct WorkflowEvaluator {
    /// The associated task execution backend.
    backend: Arc<dyn TaskExecutionBackend>,
}

impl WorkflowEvaluator {
    /// Constructs a new workflow evaluator.
    pub fn new(backend: Arc<dyn TaskExecutionBackend>) -> Self {
        Self { backend }
    }

    /// Evaluates the workflow of the given document.
    ///
    /// Upon success, returns the evaluated workflow outputs.
    #[allow(clippy::redundant_closure_call)]
    pub async fn evaluate(
        &mut self,
        document: &Document,
        inputs: &WorkflowInputs,
        root_dir: &Path,
    ) -> EvaluationResult<Outputs> {
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

        let ast = match document.node().ast() {
            Ast::V1(ast) => ast,
            _ => {
                return Err(
                    anyhow!("workflow evaluation is only supported for WDL 1.x documents").into(),
                );
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
        let graph = Rc::new(WorkflowGraphBuilder::default().build(&definition, &mut diagnostics));
        if let Some(diagnostic) = diagnostics.pop() {
            return Err(diagnostic.into());
        }

        // Split the root subgraph for every conditional and scatter statement
        let mut root = Subgraph::new(graph);
        let subgraphs = root.split();

        // Evaluate the root graph to completion
        let state = State::new(
            document,
            workflow,
            inputs,
            &self.backend,
            subgraphs,
            root_dir,
        )?;
        root.evaluate(&state, Scopes::ROOT_INDEX).await?;

        // Take the output scope and return it
        let mut outputs: Outputs = state.scopes.borrow_mut().take(Scopes::OUTPUT_INDEX).into();
        if let Some(section) = definition.output() {
            let indexes: HashMap<_, _> = section
                .declarations()
                .enumerate()
                .map(|(i, d)| (TokenStrHash::new(d.name()), i))
                .collect();
            outputs.sort_by(move |a, b| indexes[a].cmp(&indexes[b]))
        }

        Ok(outputs)
    }
}
