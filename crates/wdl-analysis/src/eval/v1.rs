//! Evaluation graphs for WDL 1.x.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

use petgraph::algo::DfsSpace;
use petgraph::algo::has_path_connecting;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Visitable;
use smallvec::SmallVec;
use smallvec::smallvec;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::TokenText;
use wdl_ast::TreeNode;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::ConditionalStatementClause;
use wdl_ast::v1::Decl;
use wdl_ast::v1::Expr;
use wdl_ast::v1::NameRefExpr;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::TaskItem;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowItem;
use wdl_ast::v1::WorkflowStatement;
use wdl_ast::version::V1;

use crate::diagnostics::NameContext;
use crate::diagnostics::call_conflict;
use crate::diagnostics::name_conflict;
use crate::diagnostics::self_referential;
use crate::diagnostics::task_reference_cycle;
use crate::diagnostics::unknown_name;
use crate::diagnostics::workflow_reference_cycle;
use crate::document::TASK_VAR_NAME;

/// Represents a node in an task evaluation graph.
#[derive(Debug, Clone)]
pub enum TaskGraphNode<N: TreeNode = SyntaxNode> {
    /// The node is an input.
    Input(Decl<N>),
    /// The node is a private decl.
    Decl(Decl<N>),
    /// The node is an output decl.
    Output(Decl<N>),
    /// The node is a command section.
    Command(CommandSection<N>),
    /// The node is a `runtime` section.
    Runtime(RuntimeSection<N>),
    /// The node is a `requirements` section.
    Requirements(RequirementsSection<N>),
    /// The node is a `hints` section.
    Hints(TaskHintsSection<N>),
}

impl<N: TreeNode> TaskGraphNode<N> {
    /// Gets the context of the name introduced by the node.
    ///
    /// Returns `None` if the node did not introduce a name.
    fn context(&self) -> Option<NameContext> {
        match self {
            Self::Input(decl) => Some(NameContext::Input(decl.name().span())),
            Self::Decl(decl) => Some(NameContext::Decl(decl.name().span())),
            Self::Output(decl) => Some(NameContext::Output(decl.name().span())),
            Self::Command(_) | Self::Runtime(_) | Self::Requirements(_) | Self::Hints(_) => None,
        }
    }

    /// Gets the expression associated with the node.
    ///
    /// Returns `None` if the node has no expression.
    fn expr(&self) -> Option<Expr<N>> {
        match self {
            Self::Input(decl) | Self::Decl(decl) | Self::Output(decl) => decl.expr(),
            Self::Command(_) | Self::Runtime(_) | Self::Requirements(_) | Self::Hints(_) => None,
        }
    }
}

impl<N: TreeNode> fmt::Display for TaskGraphNode<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(decl) | Self::Decl(decl) | Self::Output(decl) => {
                write!(f, "`{name}`", name = decl.name().text())
            }
            Self::Command(_) => write!(f, "command section"),
            Self::Runtime(_) => write!(f, "runtime section"),
            Self::Requirements(_) => write!(f, "requirements section"),
            Self::Hints(_) => write!(f, "hints section"),
        }
    }
}

/// A builder for task evaluation graphs.
#[derive(Debug)]
pub struct TaskGraphBuilder<N: TreeNode = SyntaxNode> {
    /// The map of declaration names to node indexes in the graph.
    names: HashMap<TokenText<N::Token>, NodeIndex>,
    /// The command node index.
    command: Option<NodeIndex>,
    /// The runtime node index.
    runtime: Option<NodeIndex>,
    /// The requirements node index.
    requirements: Option<NodeIndex>,
    /// The hints node index.
    hints: Option<NodeIndex>,
    /// Space for DFS operations when building the graph.
    space: DfsSpace<NodeIndex, <DiGraph<TaskGraphNode<N>, ()> as Visitable>::Map>,
}

impl<N: TreeNode> TaskGraphBuilder<N> {
    /// Builds a new task evaluation graph.
    ///
    /// The nodes are [`TaskGraphNode`] and the edges represent a reverse
    /// dependency relationship (A -> B => "node A is depended on by B").
    ///
    /// The edge data indicates whether or not the edge is an implicit edge
    /// between the node and the command section.
    ///
    /// Commands implicitly depend on all inputs, environment variables, the
    /// requirements section, the runtime section, and the hints section.
    ///
    /// Outputs implicitly depend on the command section.
    pub fn build(
        mut self,
        version: SupportedVersion,
        task: &TaskDefinition<N>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> DiGraph<TaskGraphNode<N>, bool> {
        // Populate the declaration types and build a name reference graph
        let mut graph = DiGraph::default();
        let mut saw_inputs = false;
        let mut outputs = None;
        for item in task.items() {
            match item {
                TaskItem::Input(section) if !saw_inputs => {
                    saw_inputs = true;
                    for decl in section.declarations() {
                        self.add_named_node(
                            decl.name(),
                            TaskGraphNode::Input(decl),
                            &mut graph,
                            diagnostics,
                        );
                    }
                }
                TaskItem::Output(section) if outputs.is_none() => {
                    outputs = Some(section);
                }
                TaskItem::Declaration(decl) => {
                    self.add_named_node(
                        decl.name(),
                        TaskGraphNode::Decl(Decl::Bound(decl)),
                        &mut graph,
                        diagnostics,
                    );
                }
                TaskItem::Command(section) if self.command.is_none() => {
                    self.command = Some(graph.add_node(TaskGraphNode::Command(section)));
                }
                TaskItem::Runtime(section) if self.runtime.is_none() => {
                    self.runtime = Some(graph.add_node(TaskGraphNode::Runtime(section)));
                }
                TaskItem::Requirements(section)
                    if version >= SupportedVersion::V1(V1::Two)
                        && self.requirements.is_none()
                        && self.runtime.is_none() =>
                {
                    self.requirements = Some(graph.add_node(TaskGraphNode::Requirements(section)));
                }
                TaskItem::Hints(section)
                    if version >= SupportedVersion::V1(V1::Two)
                        && self.hints.is_none()
                        && self.runtime.is_none() =>
                {
                    self.hints = Some(graph.add_node(TaskGraphNode::Hints(section)));
                }
                _ => continue,
            }
        }

        // Add name reference edges before adding the outputs
        self.add_reference_edges(version, None, &mut graph, diagnostics);

        // Add the outputs
        let count = graph.node_count();
        if let Some(section) = outputs {
            for decl in section.declarations() {
                self.add_named_node(
                    decl.name(),
                    TaskGraphNode::Output(Decl::Bound(decl)),
                    &mut graph,
                    diagnostics,
                );
            }
        }

        // Add reference edges again, but only for the output declaration nodes
        self.add_reference_edges(version, Some(count), &mut graph, diagnostics);

        // Finally, add implicit edges to and from the command
        if let Some(command) = self.command {
            // The command section depends on the runtime section
            if let Some(runtime) = self.runtime {
                graph.update_edge(runtime, command, true);
            }

            // The command section depends on the requirements section
            if let Some(requirements) = self.requirements {
                graph.update_edge(requirements, command, true);
            }

            // The command section depends on the hints section
            if let Some(hints) = self.hints {
                graph.update_edge(hints, command, true);
            }

            // The command section depends on any input or environment variable declaration
            // All outputs depend on the command
            for index in self.names.values() {
                match &graph[*index] {
                    TaskGraphNode::Input(_) => {
                        if !graph.contains_edge(*index, command) {
                            graph.update_edge(*index, command, true);
                        }
                    }
                    TaskGraphNode::Decl(decl) if decl.env().is_some() => {
                        if !graph.contains_edge(*index, command) {
                            graph.update_edge(*index, command, true);
                        }
                    }
                    TaskGraphNode::Output(_) => {
                        graph.update_edge(command, *index, true);
                    }
                    _ => continue,
                }
            }
        }

        graph
    }

    /// Adds a named node to the graph.
    fn add_named_node(
        &mut self,
        name: Ident<N::Token>,
        node: TaskGraphNode<N>,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<NodeIndex> {
        // Check for conflicting nodes
        if let Some(existing) = self.names.get(name.text()) {
            diagnostics.push(name_conflict(
                name.text(),
                node.context().expect("node should have context").into(),
                graph[*existing]
                    .context()
                    .expect("node should have context")
                    .into(),
            ));
            return None;
        }

        let index = graph.add_node(node);
        self.names.insert(name.hashable(), index);
        Some(index)
    }

    /// Adds edges from task sections to declarations.
    fn add_section_edges(
        &mut self,
        from: NodeIndex,
        descendants: impl Iterator<Item = NameRefExpr<N>>,
        allow_task_var: bool,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Add edges for any descendant name references
        for r in descendants {
            let name = r.name();

            // Look up the name; we don't check for cycles here as decls can't
            // reference a section.
            match self.names.get(name.text()) {
                Some(to) => {
                    graph.update_edge(*to, from, false);
                }
                _ => {
                    if name.text() != TASK_VAR_NAME || !allow_task_var {
                        diagnostics.push(unknown_name(name.text(), name.span()));
                    }
                }
            }
        }
    }

    /// Adds name reference edges to the graph.
    fn add_reference_edges(
        &mut self,
        version: SupportedVersion,
        skip: Option<usize>,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Populate edges for any nodes that reference other nodes by name
        for from in graph.node_indices().skip(skip.unwrap_or(0)) {
            match graph[from].clone() {
                TaskGraphNode::Input(decl) | TaskGraphNode::Decl(decl) => {
                    if let Some(expr) = decl.expr() {
                        self.add_expr_edges(from, expr, false, graph, diagnostics);
                    }
                }
                TaskGraphNode::Output(decl) => {
                    if let Some(expr) = decl.expr() {
                        self.add_expr_edges(
                            from,
                            expr,
                            version >= SupportedVersion::V1(V1::Two),
                            graph,
                            diagnostics,
                        );
                    }
                }
                TaskGraphNode::Command(section) => {
                    // Add name references from the command section to any decls in scope
                    let section = section.clone();
                    for part in section.parts() {
                        if let CommandPart::Placeholder(p) = part {
                            self.add_section_edges(
                                from,
                                p.descendants(),
                                version >= SupportedVersion::V1(V1::Two),
                                graph,
                                diagnostics,
                            );
                        }
                    }
                }
                TaskGraphNode::Runtime(section) => {
                    // Add name references from the runtime section to any decls in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                        );
                    }
                }
                TaskGraphNode::Requirements(section) => {
                    // Add name references from the requirements section to any decls in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                        );
                    }
                }
                TaskGraphNode::Hints(section) => {
                    // Add name references from the hints section to any decls in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                        );
                    }
                }
            }
        }
    }

    /// Adds name reference edges for an expression.
    fn add_expr_edges(
        &mut self,
        from: NodeIndex,
        expr: Expr<N>,
        allow_task_var: bool,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for r in expr.descendants::<NameRefExpr<N>>() {
            let name = r.name();

            // Only add an edge if the name is known
            match self.names.get(name.text()) {
                Some(to) => {
                    // Check to see if the node is self-referential
                    if *to == from {
                        diagnostics.push(self_referential(
                            name.text(),
                            graph[from]
                                .context()
                                .expect("node should have context")
                                .span(),
                            name.span(),
                        ));
                        continue;
                    }

                    // Check for a dependency cycle
                    if has_path_connecting(graph as &_, from, *to, Some(&mut self.space)) {
                        diagnostics.push(task_reference_cycle(
                            &graph[from],
                            r.span(),
                            name.text(),
                            graph[*to]
                                .expr()
                                .expect("should have expr to form a cycle")
                                .span(),
                        ));
                        continue;
                    }

                    graph.update_edge(*to, from, false);
                }
                _ => {
                    if name.text() != TASK_VAR_NAME || !allow_task_var {
                        diagnostics.push(unknown_name(name.text(), name.span()));
                    }
                }
            }
        }
    }
}

impl<N: TreeNode> Default for TaskGraphBuilder<N> {
    fn default() -> Self {
        Self {
            names: Default::default(),
            command: Default::default(),
            runtime: Default::default(),
            requirements: Default::default(),
            hints: Default::default(),
            space: Default::default(),
        }
    }
}

/// Represents a node in an workflow evaluation graph.
#[derive(Debug, Clone)]
pub enum WorkflowGraphNode<N: TreeNode = SyntaxNode> {
    /// The node is an input.
    Input(Decl<N>),
    /// The node is a private decl.
    Decl(Decl<N>),
    /// The node is an output decl.
    Output(Decl<N>),
    /// The node is a conditional statement.
    ///
    /// Stores the AST node along with the exit node index.
    Conditional(ConditionalStatement<N>, NodeIndex),
    /// The node represents a specific clause within a conditional statement.
    ///
    /// Stores the clause AST node and exit node index.
    /// This allows each clause to have its own subgraph.
    ConditionalClause(ConditionalStatementClause<N>, NodeIndex),
    /// The node is a scatter statement.
    ///
    /// Stores the AST node along with the exit node index.
    Scatter(ScatterStatement<N>, NodeIndex),
    /// The node is a call statement.
    Call(CallStatement<N>),
    /// The node is an exit of a conditional statement.
    ///
    /// This is a special node that is paired with each conditional statement
    /// node.
    ///
    /// It is the point by which the conditional is being exited and the outputs
    /// of the statement are introduced into the parent scope.
    ExitConditional(ConditionalStatement<N>),
    /// The node is an exit of a scatter statement.
    ///
    /// This is a special node that is paired with each scatter statement node.
    ///
    /// It is the point by which the scatter is being exited and the outputs of
    /// the statement are introduced into the parent scope.
    ExitScatter(ScatterStatement<N>),
}

impl<N: TreeNode> WorkflowGraphNode<N> {
    /// Gets the context of the name introduced by the node.
    ///
    /// Returns `None` if the node did not introduce a name.
    pub fn context(&self) -> Option<NameContext> {
        match self {
            Self::Input(decl) => Some(NameContext::Input(decl.name().span())),
            Self::Decl(decl) => Some(NameContext::Decl(decl.name().span())),
            Self::Output(decl) => Some(NameContext::Output(decl.name().span())),
            Self::Scatter(statement, _) => {
                Some(NameContext::ScatterVariable(statement.variable().span()))
            }
            Self::Call(statement) => statement
                .alias()
                .map(|a| NameContext::Call(a.name().span()))
                .or_else(|| {
                    statement
                        .target()
                        .names()
                        .last()
                        .map(|t| NameContext::Call(t.span()))
                }),
            Self::Conditional(..)
            | Self::ConditionalClause(..)
            | Self::ExitConditional(_)
            | Self::ExitScatter(_) => None,
        }
    }

    /// Gets the inner node representation for the workflow graph node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Input(decl) | Self::Output(decl) | Self::Decl(decl) => decl.inner(),
            Self::Conditional(stmt, ..) => stmt.inner(),
            Self::ConditionalClause(stmt, ..) => stmt.inner(),
            Self::Scatter(stmt, ..) => stmt.inner(),
            Self::Call(stmt) => stmt.inner(),
            Self::ExitConditional(stmt) => stmt.inner(),
            Self::ExitScatter(stmt) => stmt.inner(),
        }
    }
}

impl<N: TreeNode> fmt::Display for WorkflowGraphNode<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(decl) | Self::Decl(decl) | Self::Output(decl) => {
                write!(f, "`{name}`", name = decl.name().text())
            }
            Self::Scatter(statement, _) => {
                write!(f, "`{name}`", name = statement.variable().text())
            }
            Self::Call(statement) => write!(
                f,
                "`{name}`",
                name = statement
                    .alias()
                    .map(|a| a.name())
                    .or_else(|| statement.target().names().last())
                    .expect("should have name")
                    .text()
            ),
            Self::Conditional(..) => write!(f, "conditional expression"),
            Self::ConditionalClause(clause, _) => {
                write!(f, "conditional clause ({})", clause.kind())
            }
            Self::ExitConditional(_) | Self::ExitScatter(_) => write!(f, "exit"),
        }
    }
}

/// The number of declarations to store in each [`SmallVec`].
///
/// You can think of this number as "what is the maximum reasonable number of
/// clauses a conditional might have". You want the size to be large enough that
/// _most_ conditionals will fit in it (avoiding spilling the references to the
/// heap) but _small_ enough that it doesn't put unnecessary pressure on the
/// stack size.
///
/// We chose `10` because it is fairly large while not being overly burdensome
/// on the stack.
const SMALLVEC_DECLS_LEN: usize = 10;

/// Represents a builder of workflow evaluation graphs.
#[derive(Debug)]
pub struct WorkflowGraphBuilder<N: TreeNode = SyntaxNode> {
    /// The map of declaration names to node indexes in the graph.
    names: HashMap<TokenText<N::Token>, SmallVec<[NodeIndex; SMALLVEC_DECLS_LEN]>>,
    /// A stack of scatter variable names.
    variables: Vec<Ident<N::Token>>,
    /// A map of AST syntax nodes to their entry and exit nodes in the graph.
    ///
    /// This is used to add edges to the graph for references to names that are
    /// nested inside of conditional or scatter statements.
    entry_exits: HashMap<N, (NodeIndex, NodeIndex)>,
    /// Space for DFS operations when building the graph.
    space: DfsSpace<NodeIndex, <DiGraph<WorkflowGraphNode<N>, ()> as Visitable>::Map>,
    /// The common ancestor finder used when building the graph.
    ancestor_finder: CommonAncestorFinder<N>,
    /// The set of compiled enum names that are valid references but do not
    /// create runtime dependencies.
    compiled_enum_names: HashSet<String>,
}

impl<N: TreeNode> WorkflowGraphBuilder<N> {
    /// Sets the compiled enum names for the builder.
    ///
    /// These are enum type names that are valid references but do not create
    /// runtime dependencies in the workflow graph.
    pub fn with_compiled_enum_names(mut self, names: HashSet<String>) -> Self {
        self.compiled_enum_names = names;
        self
    }

    /// Builds a new workflow evaluation graph.
    ///
    /// The nodes are [`WorkflowGraphNode`] and the edges represent a reverse
    /// dependency relationship (A -> B => "node A is depended on by B").
    pub fn build(
        mut self,
        workflow: &WorkflowDefinition<N>,
        diagnostics: &mut Vec<Diagnostic>,
        input_present: impl Fn(&str) -> bool,
    ) -> DiGraph<WorkflowGraphNode<N>, ()> {
        // Populate the declaration types and build a name reference graph
        let mut graph = DiGraph::new();
        let mut saw_inputs = false;
        let mut outputs = None;
        for item in workflow.items() {
            match item {
                WorkflowItem::Input(section) if !saw_inputs => {
                    saw_inputs = true;
                    for decl in section.declarations() {
                        self.add_named_node(
                            decl.name(),
                            WorkflowGraphNode::Input(decl),
                            &mut graph,
                            diagnostics,
                        );
                    }
                }
                WorkflowItem::Output(section) => {
                    outputs = Some(section);
                }
                WorkflowItem::Conditional(statement) => {
                    self.add_workflow_statement(
                        WorkflowStatement::Conditional(statement),
                        None,
                        &mut graph,
                        diagnostics,
                    );
                }
                WorkflowItem::Scatter(statement) => {
                    self.add_workflow_statement(
                        WorkflowStatement::Scatter(statement),
                        None,
                        &mut graph,
                        diagnostics,
                    );
                }
                WorkflowItem::Call(statement) => {
                    self.add_workflow_statement(
                        WorkflowStatement::Call(statement),
                        None,
                        &mut graph,
                        diagnostics,
                    );
                }
                WorkflowItem::Declaration(decl) => {
                    self.add_workflow_statement(
                        WorkflowStatement::Declaration(decl),
                        None,
                        &mut graph,
                        diagnostics,
                    );
                }
                _ => continue,
            }
        }

        // Add name reference edges before adding the outputs
        self.add_reference_edges(None, &mut graph, diagnostics, &input_present);

        let count = graph.node_count();
        if let Some(section) = outputs {
            for decl in section.declarations() {
                self.add_named_node(
                    decl.name(),
                    WorkflowGraphNode::Output(Decl::Bound(decl)),
                    &mut graph,
                    diagnostics,
                );
            }
        }

        // Add reference edges again, but only for the output declaration nodes
        self.add_reference_edges(Some(count), &mut graph, diagnostics, &input_present);
        graph
    }

    /// Adds nodes from a workflow statement to the graph.
    fn add_workflow_statement(
        &mut self,
        statement: WorkflowStatement<N>,
        parent_entry_exit: Option<(NodeIndex, NodeIndex)>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let entry_exit = match statement {
            WorkflowStatement::Conditional(statement) => {
                // Create the exit node for the entire conditional statement
                let exit = graph.add_node(WorkflowGraphNode::ExitConditional(statement.clone()));
                // Create the main entry node
                let entry = graph.add_node(WorkflowGraphNode::Conditional(statement.clone(), exit));

                graph.update_edge(entry, exit, ());

                self.entry_exits
                    .insert(statement.inner().clone(), (entry, exit));

                // Create a separate subgraph for each clause
                for clause in statement.clauses() {
                    // Create entry node for this specific clause
                    let clause_entry =
                        graph.add_node(WorkflowGraphNode::ConditionalClause(clause.clone(), exit));

                    // Connect main entry to clause entry node
                    graph.update_edge(entry, clause_entry, ());
                    // Connect clause entry to the condition's exit node
                    graph.update_edge(clause_entry, exit, ());

                    // Store the clause's entry/exit nodes for its statements
                    self.entry_exits
                        .insert(clause.inner().clone(), (clause_entry, exit));

                    // Add all statements within this clause
                    for statement in clause.statements() {
                        self.add_workflow_statement(
                            statement,
                            Some((clause_entry, exit)),
                            graph,
                            diagnostics,
                        );
                    }
                }

                Some((entry, exit))
            }
            WorkflowStatement::Scatter(statement) => {
                // Create the entry and exit nodes for the scatter statement
                // The exit node always depends on the entry node
                let exit = graph.add_node(WorkflowGraphNode::ExitScatter(statement.clone()));
                let entry = graph.add_node(WorkflowGraphNode::Scatter(statement.clone(), exit));
                graph.update_edge(entry, exit, ());
                self.entry_exits
                    .insert(statement.inner().clone(), (entry, exit));

                // Push the scatter variable onto the stack if it isn't already conflicting
                let variable = statement.variable();
                let pushed = match self.names.get(variable.text()) {
                    Some(existing) => {
                        // SAFETY: if this exists in the map, there will always
                        // be at least one element.
                        let first = existing[0];
                        diagnostics.push(name_conflict(
                            variable.text(),
                            NameContext::ScatterVariable(variable.span()).into(),
                            graph[first]
                                .context()
                                .expect("node should have context")
                                .into(),
                        ));
                        false
                    }
                    _ => {
                        self.variables.push(variable);
                        true
                    }
                };

                // Add all of the statement's statements
                for statement in statement.statements() {
                    self.add_workflow_statement(statement, Some((entry, exit)), graph, diagnostics);
                }

                if pushed {
                    self.variables.pop();
                }

                Some((entry, exit))
            }
            WorkflowStatement::Call(statement) => {
                let name = statement.alias().map(|a| a.name()).unwrap_or_else(|| {
                    statement
                        .target()
                        .names()
                        .last()
                        .expect("expected a last call target name")
                });

                self.add_named_node(
                    name,
                    WorkflowGraphNode::Call(statement.clone()),
                    graph,
                    diagnostics,
                )
                // The calls's node is both the entry and exit nodes
                .map(|i| (i, i))
            }
            WorkflowStatement::Declaration(decl) => self
                .add_named_node(
                    decl.name(),
                    WorkflowGraphNode::Decl(Decl::Bound(decl)),
                    graph,
                    diagnostics,
                )
                // The declaration's node is both the entry and exit nodes
                .map(|i| (i, i)),
        };

        // Add (reverse) dependency edges to parent entry from child entry and to child
        // exit from parent exit
        if let (Some((entry, exit)), Some((parent_entry, parent_exit))) =
            (entry_exit, parent_entry_exit)
        {
            graph.update_edge(parent_entry, entry, ());
            graph.update_edge(exit, parent_exit, ());
        }
    }

    /// Adds a named node to the graph.
    fn add_named_node(
        &mut self,
        name: Ident<N::Token>,
        node: WorkflowGraphNode<N>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<NodeIndex> {
        // Check for a conflicting name, either from a declaration or from a scatter
        // variable
        let (context, cont) = match self.names.get(name.text()) {
            Some(existing) => {
                let mut conflicting_context = None;

                for idx in existing {
                    let existing = &graph[*idx];

                    // Allow conditionals where the names are duplicated across
                    // clauses but not within them.
                    if let (Some(existing_parent), Some(new_parent)) =
                        (existing.inner().parent(), node.inner().parent())
                        && let (Some(existing_grandparent), Some(new_grandparent)) =
                            (existing_parent.parent(), new_parent.parent())
                        && matches!(
                            existing_grandparent.kind(),
                            SyntaxKind::ConditionalStatementNode
                        )
                        && existing_parent != new_parent
                        && existing_grandparent == new_grandparent
                    {
                        continue;
                    }

                    conflicting_context = existing.context();
                    break;
                }

                if let Some(context) = conflicting_context {
                    (Some(context), false)
                } else {
                    (None, true)
                }
            }
            _ => {
                match self.variables.iter().find(|i| i.text() == name.text()) {
                    Some(existing) => {
                        // Conflict with a scatter variable; we continue to add the node so that any
                        // declaration overrides the scatter variable
                        (Some(NameContext::ScatterVariable(existing.span())), true)
                    }
                    _ => {
                        // No conflict
                        (None, true)
                    }
                }
            }
        };

        // Check to see if a diagnostic should be added
        if let Some(context) = context {
            let diagnostic = match &node {
                WorkflowGraphNode::Call(call) => {
                    call_conflict(&name, context, call.alias().is_none())
                }
                _ => name_conflict(
                    name.text(),
                    node.context().expect("node should have context").into(),
                    context.into(),
                ),
            };

            diagnostics.push(diagnostic);

            if !cont {
                return None;
            }
        }

        let index = graph.add_node(node);
        self.names.entry(name.hashable()).or_default().push(index);
        Some(index)
    }

    /// Adds name reference edges to the graph.
    fn add_reference_edges(
        &mut self,
        skip: Option<usize>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Vec<Diagnostic>,
        input_present: impl Fn(&str) -> bool,
    ) {
        // Populate edges for any nodes that reference other nodes by name
        for from in graph.node_indices().skip(skip.unwrap_or(0)) {
            match graph[from].clone() {
                WorkflowGraphNode::Input(decl) => {
                    // Only add edges for default expressions if the input wasn't provided
                    if !input_present(decl.name().text())
                        && let Some(expr) = decl.expr()
                    {
                        self.add_expr_edges(from, expr, graph, diagnostics);
                    }
                }

                WorkflowGraphNode::Decl(decl) | WorkflowGraphNode::Output(decl) => {
                    if let Some(expr) = decl.expr() {
                        self.add_expr_edges(from, expr, graph, diagnostics);
                    }
                }
                WorkflowGraphNode::Conditional(statement, _) => {
                    for clause in statement.clauses() {
                        let Some(expr) = clause.expr() else { continue };
                        self.add_expr_edges(from, expr, graph, diagnostics);
                    }
                }
                WorkflowGraphNode::ConditionalClause(..) => {
                    // The expression edges for conditional clauses are handled
                    // in the [`WorkflowGraphNode::Conditional`] case.
                }
                WorkflowGraphNode::Scatter(statement, _) => {
                    self.add_expr_edges(from, statement.expr(), graph, diagnostics);
                }
                WorkflowGraphNode::Call(statement) => {
                    // Add edges for the input expressions
                    // If an input does not have an expression, add an edge to the name
                    for input in statement.inputs() {
                        let name = input.name();
                        match input.expr() {
                            Some(expr) => {
                                self.add_expr_edges(from, expr, graph, diagnostics);
                            }
                            _ => {
                                if let Some(nodes) =
                                    self.find_nodes_by_name(name.text(), input.inner().clone())
                                {
                                    // Check for a dependency cycle
                                    for to in nodes {
                                        if has_path_connecting(
                                            graph as &_,
                                            from,
                                            to,
                                            Some(&mut self.space),
                                        ) {
                                            diagnostics.push(workflow_reference_cycle(
                                                &graph[from],
                                                name.span(),
                                                name.text(),
                                                graph[to]
                                                    .context()
                                                    .expect("node should have context")
                                                    .span(),
                                            ));
                                            continue;
                                        }

                                        self.add_dependency_edge(from, to, graph);
                                    }
                                }
                            }
                        }
                    }

                    // Add edges to other the requested calls
                    for after in statement.after() {
                        let name = after.name();
                        if let Some(nodes) =
                            self.find_nodes_by_name(name.text(), after.inner().clone())
                        {
                            for to in nodes {
                                // Check for a dependency cycle
                                if has_path_connecting(graph as &_, from, to, Some(&mut self.space))
                                {
                                    diagnostics.push(workflow_reference_cycle(
                                        &graph[from],
                                        name.span(),
                                        name.text(),
                                        graph[to]
                                            .context()
                                            .expect("node should have context")
                                            .span(),
                                    ));
                                    continue;
                                }

                                self.add_dependency_edge(from, to, graph);
                            }
                        }
                    }
                }
                WorkflowGraphNode::ExitConditional(_) | WorkflowGraphNode::ExitScatter(_) => {
                    continue;
                }
            }
        }
    }

    /// Adds name reference edges for an expression.
    fn add_expr_edges(
        &mut self,
        from: NodeIndex,
        expr: Expr<N>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for r in expr.inner().descendants().filter_map(NameRefExpr::cast) {
            let name = r.name();

            // Only add an edge if the name is known
            match self.find_nodes_by_name(name.text(), expr.inner().clone()) {
                Some(nodes) => {
                    for to in nodes {
                        // Check to see if the node is self-referential
                        if to == from {
                            diagnostics.push(self_referential(
                                name.text(),
                                graph[from]
                                    .context()
                                    .expect("node should have a context")
                                    .span(),
                                name.span(),
                            ));
                            continue;
                        }

                        // Check for a dependency cycle
                        if has_path_connecting(graph as &_, from, to, Some(&mut self.space)) {
                            diagnostics.push(workflow_reference_cycle(
                                &graph[from],
                                r.span(),
                                name.text(),
                                graph[to]
                                    .context()
                                    .expect("node should have context")
                                    .span(),
                            ));
                            continue;
                        }

                        self.add_dependency_edge(from, to, graph);
                    }
                }
                _ => {
                    // Check if this is a compiled enum name
                    // These are valid references but don't create runtime dependencies
                    if !self.compiled_enum_names.contains(name.text()) {
                        diagnostics.push(unknown_name(name.text(), name.span()));
                    }
                }
            }
        }
    }

    /// Adds a dependency edge between two nodes.
    ///
    /// Dependency edges can only be formed between nodes at the same "scope".
    ///
    /// This works by walking up the AST ancestors looking for a common ancestor
    /// (A) of the two nodes.
    ///
    /// For the child of A that is an ancestor of `to` (or `to` itself), we use
    /// the exit node of that child if there is one.
    ///
    /// For the child of A this is an ancestor of `from` (or `from` itself), we
    /// use the entry node of that child if there is one.
    ///
    /// If either child does not have an entry/exit node, the original nodes are
    /// used.
    fn add_dependency_edge(
        &mut self,
        from: NodeIndex,
        to: NodeIndex,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
    ) {
        assert!(from != to, "cannot add a self dependency edge");

        let (from, to) = match self.ancestor_finder.find_children_of_common_ancestor(
            graph[from].inner().ancestors(),
            graph[to].inner().ancestors(),
            SyntaxKind::WorkflowDefinitionNode,
        ) {
            Some((f, t)) => {
                let from = self
                    .entry_exits
                    .get(&f)
                    .map(|(entry, _)| *entry)
                    .unwrap_or(from);
                let to = self
                    .entry_exits
                    .get(&t)
                    .map(|(_, exit)| *exit)
                    .unwrap_or(to);
                (from, to)
            }
            _ => (from, to),
        };

        if from == to {
            // No need to add an edge when the entry and exit are the same node
            // This can occur for scatter variables referenced within the scatter body
            return;
        }

        // Add the actual edge in reverse order
        graph.update_edge(to, from, ());
    }

    /// Finds a node in the graph by name for the referencing expression.
    ///
    /// This takes into account finding a scatter variable that's in scope.
    fn find_nodes_by_name(
        &self,
        name: &str,
        expr: N,
    ) -> Option<SmallVec<[NodeIndex; SMALLVEC_DECLS_LEN]>> {
        // If the name came from a declaration or call, return the node
        if let Some(result) = self.names.get(name) {
            return Some(result.to_owned());
        }

        // Otherwise, we need to walk up the parent chain looking for a scatter variable
        // with the name
        let mut current = expr;
        while let Some(parent) = current.parent() {
            if let SyntaxKind::ScatterStatementNode = parent.kind() {
                let statement = ScatterStatement::cast(parent.clone()).expect("node should cast");
                let variable = statement.variable();
                if variable.text() == name {
                    // Return the entry node for the scatter statement
                    return Some(smallvec![self.entry_exits[&parent].0]);
                }
            }

            current = parent;
        }

        None
    }
}

impl<N: TreeNode> Default for WorkflowGraphBuilder<N> {
    fn default() -> Self {
        Self {
            names: Default::default(),
            variables: Default::default(),
            entry_exits: Default::default(),
            space: Default::default(),
            ancestor_finder: Default::default(),
            compiled_enum_names: Default::default(),
        }
    }
}

/// A helper for finding the children of a common ancestor in the AST.
///
/// This exists so we can reuse previously allocated space when adding
/// dependency edges.
#[derive(Debug)]
struct CommonAncestorFinder<N: TreeNode = SyntaxNode> {
    /// The stack of ancestors for the `first` node.
    first: Vec<N>,
    /// The stack of ancestors for the `second` node.
    second: Vec<N>,
}

impl<N: TreeNode> CommonAncestorFinder<N> {
    /// Finds the children of a common ancestor in two list of ancestors.
    fn find_children_of_common_ancestor(
        &mut self,
        first: impl Iterator<Item = N>,
        second: impl Iterator<Item = N>,
        stop: SyntaxKind,
    ) -> Option<(N, N)> {
        self.first.clear();
        for ancestor in first {
            self.first.push(ancestor.clone());
            if ancestor.kind() == stop {
                break;
            }
        }

        self.second.clear();
        for ancestor in second {
            self.second.push(ancestor.clone());
            if ancestor.kind() == stop {
                break;
            }
        }

        for (first, second) in self.first.iter().rev().zip(self.second.iter().rev()) {
            if first == second {
                continue;
            }

            return Some((first.clone(), second.clone()));
        }

        None
    }
}

impl<N: TreeNode> Default for CommonAncestorFinder<N> {
    fn default() -> Self {
        Self {
            first: Default::default(),
            second: Default::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use wdl_ast::Document;

    use super::*;

    #[test]
    fn test_input_dependency_handling() {
        let source = r#"
        version 1.1

        task my_task {
            input {
                Int i
            }

            command <<<>>>

            output {
                Int out = i
            }
        }

        workflow foo {
            input {
                Int x = 10
                Int y = t1.out
            }

            call my_task as t1 { input: i = x }
            call my_task as t2 { input: i = y }
        }
        "#;

        let (document, diagnostics) = Document::parse(source);
        assert!(
            diagnostics.is_empty(),
            "parsing should succeed without diagnostics"
        );

        let workflow = document
            .ast()
            .into_v1()
            .expect("document should be v1")
            .workflows()
            .next()
            .expect("document should have a workflow");

        let mut diagnostics = Vec::new();

        // Testing without providing inputs i.e. static analysis
        let graph = WorkflowGraphBuilder::default().build(&workflow, &mut diagnostics, |_| false);

        let t1_out = graph
            .node_indices()
            .find(|i| {
                if let WorkflowGraphNode::Call(call) = &graph[*i] {
                    call.alias().map(|a| a.name().text().to_string()) == Some("t1".to_string())
                } else {
                    false
                }
            })
            .expect("t1 node not found");

        let y = graph
            .node_indices()
            .find(|i| {
                if let WorkflowGraphNode::Input(input) = &graph[*i] {
                    input.name().text() == "y"
                } else {
                    false
                }
            })
            .expect("y node not found");

        assert!(
            graph.contains_edge(t1_out, y),
            "y should depend on t1.out when input 'y' is not provided"
        );

        let y_input = graph
            .node_indices()
            .find(|i| {
                if let WorkflowGraphNode::Input(input) = &graph[*i] {
                    input.name().text() == "y"
                } else {
                    false
                }
            })
            .expect("y node not found");

        let t2 = graph
            .node_indices()
            .find(|i| {
                if let WorkflowGraphNode::Call(call) = &graph[*i] {
                    call.alias().map(|a| a.name().text().to_string()) == Some("t2".to_string())
                } else {
                    false
                }
            })
            .expect("t2 node not found");

        assert!(graph.contains_edge(y_input, t2), "t2 should depend on y");

        // Testing with providing input y i.e. runtime analysis - case for wdl_engine
        let mut diagnostics = Vec::new();
        let graph =
            WorkflowGraphBuilder::default().build(&workflow, &mut diagnostics, |name| name == "y");

        assert!(
            !graph.contains_edge(t1_out, y),
            "y should not depend on t1.out when input 'y' is provided"
        );

        assert!(
            graph.contains_edge(y_input, t2),
            "t2 should depend on y even when input y is provided"
        );
    }
}
