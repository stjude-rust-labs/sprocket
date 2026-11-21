//! Evaluation graphs for WDL 1.x.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::fmt::Debug;

use petgraph::algo::DfsSpace;
use petgraph::algo::has_path_connecting;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Visitable;
use smallvec::SmallVec;
use smallvec::smallvec;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
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
use wdl_ast::v1::ValidationBlock;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowItem;
use wdl_ast::v1::WorkflowStatement;
use wdl_ast::version::V1;

use crate::Diagnostics;
use crate::diagnostics::NameContext;
use crate::diagnostics::call_conflict;
use crate::diagnostics::input_validation_dependency;
use crate::diagnostics::input_validation_reference;
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
    /// The node is the `validation` block of the input section.
    ///
    /// Input validation is evaluated after the inputs it references and
    /// before any other non-input node in the task.
    InputValidation(ValidationBlock<N>),
    /// The node is the `validation` block of the output section.
    ///
    /// Output validation is evaluated after the command and all outputs.
    OutputValidation(ValidationBlock<N>),
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
            Self::Command(_)
            | Self::Runtime(_)
            | Self::Requirements(_)
            | Self::Hints(_)
            | Self::InputValidation(_)
            | Self::OutputValidation(_) => None,
        }
    }

    /// Gets the expression associated with the node.
    ///
    /// Returns `None` if the node has no expression.
    fn expr(&self) -> Option<Expr<N>> {
        match self {
            Self::Input(decl) | Self::Decl(decl) | Self::Output(decl) => decl.expr(),
            Self::Command(_)
            | Self::Runtime(_)
            | Self::Requirements(_)
            | Self::Hints(_)
            | Self::InputValidation(_)
            | Self::OutputValidation(_) => None,
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
            Self::InputValidation(_) => write!(f, "input validation block"),
            Self::OutputValidation(_) => write!(f, "output validation block"),
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
    /// The input validation node index.
    input_validation: Option<NodeIndex>,
    /// The output validation node index.
    output_validation: Option<NodeIndex>,
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
    ///
    /// The output validation block (if present) implicitly depends on the
    /// command section and every output.
    ///
    /// Every node that is not an input implicitly depends on the input
    /// validation block (if present); this ensures input validation is
    /// evaluated before any private declaration, section, or output.
    pub fn build(
        mut self,
        version: SupportedVersion,
        task: &TaskDefinition<N>,
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
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

                    if let Some(block) = section.validation() {
                        self.input_validation =
                            Some(graph.add_node(TaskGraphNode::InputValidation(block)));
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
        self.add_reference_edges(version, None, &mut graph, diagnostics, &custom_type_present);

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

            if let Some(block) = section.validation() {
                self.output_validation =
                    Some(graph.add_node(TaskGraphNode::OutputValidation(block)));
            }
        }

        // Add reference edges again, but only for the output nodes
        self.add_reference_edges(
            version,
            Some(count),
            &mut graph,
            diagnostics,
            &custom_type_present,
        );

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

            // The command section depends on any input or private declaration;
            // all outputs depend on the command
            for index in self.names.values() {
                match &graph[*index] {
                    TaskGraphNode::Input(_) => {
                        if !graph.contains_edge(*index, command) {
                            graph.update_edge(*index, command, true);
                        }
                    }
                    TaskGraphNode::Decl(_) => {
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

        // The output validation depends on the command and every output
        if let Some(validation) = self.output_validation {
            if let Some(command) = self.command
                && !graph.contains_edge(command, validation)
            {
                graph.update_edge(command, validation, true);
            }

            for index in self.names.values() {
                if matches!(graph[*index], TaskGraphNode::Output(_))
                    && !graph.contains_edge(*index, validation)
                {
                    graph.update_edge(*index, validation, true);
                }
            }
        }

        // Every non-input node depends on the input validation
        if let Some(validation) = self.input_validation {
            let indexes: Vec<_> = graph.node_indices().collect();
            for index in indexes {
                if matches!(
                    graph[index],
                    TaskGraphNode::Input(_) | TaskGraphNode::InputValidation(_)
                ) {
                    continue;
                }

                // Don't add an edge that would introduce a cycle; this is only
                // possible when the input validation incorrectly references a
                // non-input (which is reported as an error)
                if has_path_connecting(&graph, index, validation, Some(&mut self.space)) {
                    continue;
                }

                graph.update_edge(validation, index, true);
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
        diagnostics: &mut Diagnostics,
    ) -> Option<NodeIndex> {
        // Check for conflicting nodes
        if let Some(existing) = self.names.get(name.text()) {
            diagnostics.add(name_conflict(
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
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
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
                    if (name.text() != TASK_VAR_NAME || !allow_task_var)
                        && !custom_type_present(name.text())
                    {
                        diagnostics.add(unknown_name(name.text(), name.span()));
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
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
    ) {
        // Populate edges for any nodes that reference other nodes by name
        for from in graph.node_indices().skip(skip.unwrap_or(0)) {
            match graph[from].clone() {
                TaskGraphNode::Input(decl) | TaskGraphNode::Decl(decl) => {
                    if let Some(expr) = decl.expr() {
                        self.add_expr_edges(
                            from,
                            expr,
                            false,
                            graph,
                            diagnostics,
                            &custom_type_present,
                        );
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
                            &custom_type_present,
                        );
                    }
                }
                TaskGraphNode::Command(section) => {
                    // Add name references from the command section to any decls
                    // in scope
                    let section = section.clone();
                    for part in section.parts() {
                        if let CommandPart::Placeholder(p) = part {
                            self.add_section_edges(
                                from,
                                p.descendants(),
                                version >= SupportedVersion::V1(V1::Two),
                                graph,
                                diagnostics,
                                &custom_type_present,
                            );
                        }
                    }
                }
                TaskGraphNode::Runtime(section) => {
                    // Add name references from the runtime section to any decls
                    // in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                            &custom_type_present,
                        );
                    }
                }
                TaskGraphNode::Requirements(section) => {
                    // Add name references from the requirements section to any
                    // decls in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                            &custom_type_present,
                        );
                    }
                }
                TaskGraphNode::Hints(section) => {
                    // Add name references from the hints section to any decls
                    // in scope
                    let section = section.clone();
                    for item in section.items() {
                        self.add_section_edges(
                            from,
                            item.descendants(),
                            version >= SupportedVersion::V1(V1::Three),
                            graph,
                            diagnostics,
                            &custom_type_present,
                        );
                    }
                }
                TaskGraphNode::InputValidation(block) => {
                    self.add_input_validation_edges(
                        from,
                        &block,
                        graph,
                        diagnostics,
                        &custom_type_present,
                    );
                }
                TaskGraphNode::OutputValidation(block) => {
                    // Output validation has the same visibility as outputs
                    for clause in block.clauses() {
                        for expr in Expr::children(clause.inner()) {
                            self.add_expr_edges(
                                from,
                                expr,
                                version >= SupportedVersion::V1(V1::Two),
                                graph,
                                diagnostics,
                                &custom_type_present,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Adds name reference edges for an input validation block.
    ///
    /// Input validation clauses may only reference input declarations and the
    /// default expressions of any referenced input may only (transitively)
    /// reference other input declarations.
    fn add_input_validation_edges(
        &mut self,
        from: NodeIndex,
        block: &ValidationBlock<N>,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
    ) {
        let mut cache = HashMap::new();
        for clause in block.clauses() {
            for expr in Expr::children(clause.inner()) {
                for r in expr.descendants::<NameRefExpr<N>>() {
                    let name = r.name();
                    let Some(to) = self.names.get(name.text()).copied() else {
                        if !custom_type_present(name.text()) {
                            diagnostics.add(unknown_name(name.text(), name.span()));
                        }
                        continue;
                    };

                    // The validation node has no outgoing edges at this point,
                    // so adding the edge cannot introduce a cycle
                    graph.update_edge(to, from, false);

                    if !matches!(graph[to], TaskGraphNode::Input(_)) {
                        diagnostics.add(input_validation_reference(
                            name.text(),
                            name.span(),
                            graph[to].context().expect("node should have context"),
                        ));
                        continue;
                    }

                    let dependency = cache
                        .entry(to)
                        .or_insert_with(|| {
                            self.find_non_input_dependency(to, graph, &mut HashSet::new())
                        })
                        .clone();

                    if let Some((dependency, span, context)) = dependency {
                        diagnostics.add(input_validation_dependency(
                            name.text(),
                            name.span(),
                            &dependency,
                            span,
                            context,
                        ));
                    }
                }
            }
        }
    }

    /// Finds the first name referenced (transitively) by the given input's
    /// default expression that is not an input.
    ///
    /// Returns the name, the span of the reference, and the context of the
    /// referenced name.
    fn find_non_input_dependency(
        &self,
        input: NodeIndex,
        graph: &DiGraph<TaskGraphNode<N>, bool>,
        visited: &mut HashSet<NodeIndex>,
    ) -> Option<(String, wdl_ast::Span, NameContext)> {
        if !visited.insert(input) {
            return None;
        }

        let expr = graph[input].expr()?;
        for r in expr.descendants::<NameRefExpr<N>>() {
            let name = r.name();
            let Some(index) = self.names.get(name.text()).copied() else {
                // Unknown names are reported elsewhere
                continue;
            };

            match &graph[index] {
                TaskGraphNode::Input(_) => {
                    if let Some(dependency) = self.find_non_input_dependency(index, graph, visited)
                    {
                        return Some(dependency);
                    }
                }
                node => {
                    return Some((
                        name.text().to_string(),
                        name.span(),
                        node.context().expect("node should have context"),
                    ));
                }
            }
        }

        None
    }

    /// Adds name reference edges for an expression.
    fn add_expr_edges(
        &mut self,
        from: NodeIndex,
        expr: Expr<N>,
        allow_task_var: bool,
        graph: &mut DiGraph<TaskGraphNode<N>, bool>,
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
    ) {
        for r in expr.descendants::<NameRefExpr<N>>() {
            let name = r.name();

            // Only add an edge if the name is known
            match self.names.get(name.text()) {
                Some(to) => {
                    // Check to see if the node is self-referential
                    if *to == from {
                        diagnostics.add(self_referential(
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
                        diagnostics.add(task_reference_cycle(
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
                    if (name.text() != TASK_VAR_NAME || !allow_task_var)
                        && !custom_type_present(name.text())
                    {
                        diagnostics.add(unknown_name(name.text(), name.span()));
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
            input_validation: Default::default(),
            output_validation: Default::default(),
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
    /// The node is the `validation` block of the input section.
    ///
    /// Input validation is evaluated after the inputs it references and
    /// before any root-level declaration, call, conditional, scatter, or
    /// output in the workflow.
    InputValidation(ValidationBlock<N>),
    /// The node is the `validation` block of the output section.
    ///
    /// Output validation is evaluated after all outputs.
    OutputValidation(ValidationBlock<N>),
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
            | Self::ExitScatter(_)
            | Self::InputValidation(_)
            | Self::OutputValidation(_) => None,
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
            Self::InputValidation(block) | Self::OutputValidation(block) => block.inner(),
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
            Self::InputValidation(_) => write!(f, "input validation block"),
            Self::OutputValidation(_) => write!(f, "output validation block"),
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
}

impl<N: TreeNode> WorkflowGraphBuilder<N> {
    /// Builds a new workflow evaluation graph.
    ///
    /// The nodes are [`WorkflowGraphNode`] and the edges represent a reverse
    /// dependency relationship (A -> B => "node A is depended on by B").
    ///
    /// The output validation block (if present) depends on every output.
    ///
    /// Every root-level declaration, call, conditional, scatter, output, and
    /// the output validation block depend on the input validation block (if
    /// present); this ensures input validation is evaluated before any of the
    /// workflow's body. Nodes nested within conditional and scatter statements
    /// are never given direct edges from the input validation block.
    pub fn build(
        mut self,
        workflow: &WorkflowDefinition<N>,
        diagnostics: &mut Diagnostics,
        input_present: impl Fn(&str) -> bool,
        custom_type_present: impl Fn(&str) -> bool,
    ) -> DiGraph<WorkflowGraphNode<N>, ()> {
        // Populate the declaration types and build a name reference graph
        let mut graph = DiGraph::new();
        let mut saw_inputs = false;
        let mut outputs = None;
        let mut input_validation = None;
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

                    if let Some(block) = section.validation() {
                        input_validation =
                            Some(graph.add_node(WorkflowGraphNode::InputValidation(block)));
                    }
                }
                WorkflowItem::Output(section) if outputs.is_none() => {
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
        self.add_reference_edges(
            None,
            &mut graph,
            diagnostics,
            &input_present,
            &custom_type_present,
        );

        let count = graph.node_count();
        let mut output_validation = None;
        if let Some(section) = outputs {
            for decl in section.declarations() {
                self.add_named_node(
                    decl.name(),
                    WorkflowGraphNode::Output(Decl::Bound(decl)),
                    &mut graph,
                    diagnostics,
                );
            }

            if let Some(block) = section.validation() {
                output_validation =
                    Some(graph.add_node(WorkflowGraphNode::OutputValidation(block)));
            }
        }

        // Add reference edges again, but only for the output nodes
        self.add_reference_edges(
            Some(count),
            &mut graph,
            diagnostics,
            &input_present,
            &custom_type_present,
        );

        // The output validation depends on every output
        if let Some(validation) = output_validation {
            let outputs: Vec<_> = graph
                .node_indices()
                .filter(|i| matches!(graph[*i], WorkflowGraphNode::Output(_)))
                .collect();
            for output in outputs {
                graph.update_edge(output, validation, ());
            }
        }

        // Every root-level node depends on the input validation
        if let Some(validation) = input_validation {
            let indexes: Vec<_> = graph.node_indices().collect();
            for index in indexes {
                let gated = match &graph[index] {
                    WorkflowGraphNode::Decl(_)
                    | WorkflowGraphNode::Call(_)
                    | WorkflowGraphNode::Conditional(..)
                    | WorkflowGraphNode::Scatter(..) => graph[index]
                        .inner()
                        .parent()
                        .map(|p| p.kind() == SyntaxKind::WorkflowDefinitionNode)
                        .unwrap_or(false),
                    WorkflowGraphNode::Output(_) | WorkflowGraphNode::OutputValidation(_) => true,
                    WorkflowGraphNode::Input(_)
                    | WorkflowGraphNode::ConditionalClause(..)
                    | WorkflowGraphNode::ExitConditional(_)
                    | WorkflowGraphNode::ExitScatter(_)
                    | WorkflowGraphNode::InputValidation(_) => false,
                };

                // Don't add an edge that would introduce a cycle; this is only
                // possible when the input validation incorrectly references a
                // non-input (which is reported as an error)
                if !gated || has_path_connecting(&graph, index, validation, Some(&mut self.space)) {
                    continue;
                }

                graph.update_edge(validation, index, ());
            }
        }

        graph
    }

    /// Adds nodes from a workflow statement to the graph.
    fn add_workflow_statement(
        &mut self,
        statement: WorkflowStatement<N>,
        parent_entry_exit: Option<(NodeIndex, NodeIndex)>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Diagnostics,
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

                // Push the scatter variable onto the stack if it isn't already
                // conflicting
                let variable = statement.variable();
                let pushed = match self.names.get(variable.text()) {
                    Some(existing) => {
                        // SAFETY: if this exists in the map, there will always
                        // be at least one element.
                        let first = existing[0];
                        diagnostics.add(name_conflict(
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

        // Add (reverse) dependency edges to parent entry from child entry and
        // to child exit from parent exit
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
        diagnostics: &mut Diagnostics,
    ) -> Option<NodeIndex> {
        // Check for a conflicting name, either from a declaration or from a
        // scatter variable
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
                        // Conflict with a scatter variable; we continue to add
                        // the node so that any
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

            diagnostics.add(diagnostic);

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
        diagnostics: &mut Diagnostics,
        input_present: impl Fn(&str) -> bool,
        custom_type_present: impl Fn(&str) -> bool,
    ) {
        // Populate edges for any nodes that reference other nodes by name
        for from in graph.node_indices().skip(skip.unwrap_or(0)) {
            match graph[from].clone() {
                WorkflowGraphNode::Input(decl) => {
                    // Only add edges for default expressions if the input
                    // wasn't provided
                    if !input_present(decl.name().text())
                        && let Some(expr) = decl.expr()
                    {
                        self.add_expr_edges(from, expr, graph, diagnostics, &custom_type_present);
                    }
                }

                WorkflowGraphNode::Decl(decl) | WorkflowGraphNode::Output(decl) => {
                    if let Some(expr) = decl.expr() {
                        self.add_expr_edges(from, expr, graph, diagnostics, &custom_type_present);
                    }
                }
                WorkflowGraphNode::Conditional(statement, _) => {
                    for clause in statement.clauses() {
                        let Some(expr) = clause.expr() else { continue };
                        self.add_expr_edges(from, expr, graph, diagnostics, &custom_type_present);
                    }
                }
                WorkflowGraphNode::ConditionalClause(..) => {
                    // The expression edges for conditional clauses are handled
                    // in the [`WorkflowGraphNode::Conditional`] case.
                }
                WorkflowGraphNode::Scatter(statement, _) => {
                    self.add_expr_edges(
                        from,
                        statement.expr(),
                        graph,
                        diagnostics,
                        &custom_type_present,
                    );
                }
                WorkflowGraphNode::Call(statement) => {
                    // Add edges for the input expressions
                    // If an input does not have an expression, add an edge to
                    // the name
                    for input in statement.inputs() {
                        let name = input.name();
                        match input.expr() {
                            Some(expr) => {
                                self.add_expr_edges(
                                    from,
                                    expr,
                                    graph,
                                    diagnostics,
                                    &custom_type_present,
                                );
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
                                            diagnostics.add(workflow_reference_cycle(
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

                    // Add edges to the "after" calls
                    for after in statement.after() {
                        let name = after.name();
                        if let Some(nodes) =
                            self.find_nodes_by_name(name.text(), after.inner().clone())
                        {
                            for to in nodes {
                                // Check for a dependency cycle
                                if has_path_connecting(graph as &_, from, to, Some(&mut self.space))
                                {
                                    diagnostics.add(workflow_reference_cycle(
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
                WorkflowGraphNode::InputValidation(block) => {
                    // Edges are always added regardless of whether or not the
                    // referenced inputs are present
                    for clause in block.clauses() {
                        for expr in Expr::children(clause.inner()) {
                            self.add_expr_edges(
                                from,
                                expr.clone(),
                                graph,
                                diagnostics,
                                &custom_type_present,
                            );
                            self.check_input_validation_expr(&expr, graph, diagnostics);
                        }
                    }
                }
                WorkflowGraphNode::OutputValidation(block) => {
                    for clause in block.clauses() {
                        for expr in Expr::children(clause.inner()) {
                            self.add_expr_edges(
                                from,
                                expr,
                                graph,
                                diagnostics,
                                &custom_type_present,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Checks that an expression of an input validation clause only references
    /// input declarations whose default expressions only (transitively)
    /// reference other input declarations.
    ///
    /// This check is syntactic and does not depend on which inputs are present.
    fn check_input_validation_expr(
        &self,
        expr: &Expr<N>,
        graph: &DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Diagnostics,
    ) {
        for r in expr.descendants::<NameRefExpr<N>>() {
            let name = r.name();
            let Some(nodes) = self.find_nodes_by_name(name.text(), expr.inner().clone()) else {
                // Unknown names are reported elsewhere
                continue;
            };

            for to in nodes {
                if !matches!(graph[to], WorkflowGraphNode::Input(_)) {
                    if let Some(context) = graph[to].context() {
                        diagnostics.add(input_validation_reference(
                            name.text(),
                            name.span(),
                            context,
                        ));
                    }
                    break;
                }

                if let Some((dependency, span, context)) =
                    self.find_non_input_dependency(to, graph, &mut HashSet::new())
                {
                    diagnostics.add(input_validation_dependency(
                        name.text(),
                        name.span(),
                        &dependency,
                        span,
                        context,
                    ));
                    break;
                }
            }
        }
    }

    /// Finds the first name referenced (transitively) by the given input's
    /// default expression that is not an input.
    ///
    /// Returns the name, the span of the reference, and the context of the
    /// referenced name.
    fn find_non_input_dependency(
        &self,
        input: NodeIndex,
        graph: &DiGraph<WorkflowGraphNode<N>, ()>,
        visited: &mut HashSet<NodeIndex>,
    ) -> Option<(String, wdl_ast::Span, NameContext)> {
        if !visited.insert(input) {
            return None;
        }

        let WorkflowGraphNode::Input(decl) = &graph[input] else {
            return None;
        };

        let expr = decl.expr()?;
        for r in expr.descendants::<NameRefExpr<N>>() {
            let name = r.name();
            let Some(nodes) = self.find_nodes_by_name(name.text(), expr.inner().clone()) else {
                // Unknown names are reported elsewhere
                continue;
            };

            for index in nodes {
                match &graph[index] {
                    WorkflowGraphNode::Input(_) => {
                        if let Some(dependency) =
                            self.find_non_input_dependency(index, graph, visited)
                        {
                            return Some(dependency);
                        }
                    }
                    node => {
                        if let Some(context) = node.context() {
                            return Some((name.text().to_string(), name.span(), context));
                        }
                    }
                }
            }
        }

        None
    }

    /// Adds name reference edges for an expression.
    fn add_expr_edges(
        &mut self,
        from: NodeIndex,
        expr: Expr<N>,
        graph: &mut DiGraph<WorkflowGraphNode<N>, ()>,
        diagnostics: &mut Diagnostics,
        custom_type_present: impl Fn(&str) -> bool,
    ) {
        for r in expr.inner().descendants().filter_map(NameRefExpr::cast) {
            let name = r.name();

            // Only add an edge if the name is known
            match self.find_nodes_by_name(name.text(), expr.inner().clone()) {
                Some(nodes) => {
                    for to in nodes {
                        // Check to see if the node is self-referential
                        if to == from {
                            diagnostics.add(self_referential(
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
                            diagnostics.add(workflow_reference_cycle(
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
                    // Check if name points to a custom type(a struct or an
                    // enum).
                    if !custom_type_present(name.text()) {
                        diagnostics.add(unknown_name(name.text(), name.span()));
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
            // This can occur for scatter variables referenced within the
            // scatter body
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
        // We need to walk up the parent chain looking for a scatter variable
        // with a matching name before looking at names in scope; a
        // scatter variable may shadow names declared outside of it, but
        // an inner declaration cannot shadow an outer scatter variable
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

        // If the name came from a declaration or call, return the node
        if let Some(result) = self.names.get(name) {
            return Some(result.to_owned());
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
mod tests {
    use petgraph::Direction;
    use wdl_ast::Document;

    use super::*;

    #[test_log::test]
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

        let (document, diagnostics) = Document::parse(source, None);
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

        let mut diagnostics = Diagnostics::default();

        // Testing without providing inputs i.e. static analysis
        let graph = WorkflowGraphBuilder::default().build(
            &workflow,
            &mut diagnostics,
            |_| false,
            |_| false,
        );

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

        // Testing with providing input y i.e. runtime analysis - case for
        // wdl_engine
        let mut diagnostics = Diagnostics::default();
        let graph = WorkflowGraphBuilder::default().build(
            &workflow,
            &mut diagnostics,
            |name| name == "y",
            |_| false,
        );

        assert!(
            !graph.contains_edge(t1_out, y),
            "y should not depend on t1.out when input 'y' is provided"
        );

        assert!(
            graph.contains_edge(y_input, t2),
            "t2 should depend on y even when input y is provided"
        );
    }

    /// Parses the given source and returns the first workflow and task.
    fn parse(source: &str) -> (Option<WorkflowDefinition>, Option<TaskDefinition>) {
        let (document, diagnostics) = Document::parse(source, None);
        assert!(
            diagnostics.is_empty(),
            "parsing should succeed without diagnostics: {diagnostics:?}"
        );

        let ast = document.ast().into_v1().expect("document should be v1");
        (ast.workflows().next(), ast.tasks().next())
    }

    #[test]
    fn test_workflow_validation_edges() {
        let source = r#"
        version 1.4

        task my_task {
            command <<<>>>

            output {
                Int out = 1
            }
        }

        workflow foo {
            input {
                Int x = 10
                Int y = x

                validation {
                    true: "literal only",
                    y > 0: "y must be positive",
                }
            }

            Int a = 1
            call my_task as t1

            scatter (i in range(2)) {
                Int nested = i
                call my_task as t2
            }

            if (true) {
                Int conditional = 1
            }

            output {
                Int out = t1.out

                validation {
                    out > 0: "out must be positive",
                }
            }
        }
        "#;

        let (workflow, _) = parse(source);
        let workflow = workflow.expect("should have workflow");

        for provided in [false, true] {
            let mut diagnostics = Diagnostics::default();
            let graph = WorkflowGraphBuilder::default().build(
                &workflow,
                &mut diagnostics,
                |name| provided && name == "y",
                |_| false,
            );
            assert!(
                diagnostics.is_empty(),
                "graph should build without diagnostics: {diagnostics:?}"
            );

            let find = |pred: &dyn Fn(&WorkflowGraphNode) -> bool| {
                graph
                    .node_indices()
                    .find(|i| pred(&graph[*i]))
                    .expect("node should exist")
            };
            let named = |kind: &str, name: &str| {
                graph
                    .node_indices()
                    .find(|i| match &graph[*i] {
                        WorkflowGraphNode::Input(d) if kind == "input" => d.name().text() == name,
                        WorkflowGraphNode::Decl(d) if kind == "decl" => d.name().text() == name,
                        WorkflowGraphNode::Output(d) if kind == "output" => d.name().text() == name,
                        WorkflowGraphNode::Call(c) if kind == "call" => {
                            c.alias().map(|a| a.name().text().to_string()).as_deref() == Some(name)
                        }
                        _ => false,
                    })
                    .expect("node should exist")
            };

            let iv = find(&|n| matches!(n, WorkflowGraphNode::InputValidation(_)));
            let ov = find(&|n| matches!(n, WorkflowGraphNode::OutputValidation(_)));
            let scatter = find(&|n| matches!(n, WorkflowGraphNode::Scatter(..)));
            let conditional = find(&|n| matches!(n, WorkflowGraphNode::Conditional(..)));

            // The input validation depends on the referenced input regardless
            // of whether or not the input was provided
            assert!(graph.contains_edge(named("input", "y"), iv));
            assert!(!graph.contains_edge(named("input", "x"), iv));
            assert!(!graph.contains_edge(iv, named("input", "x")));

            // Root-level nodes are gated by the input validation
            assert!(graph.contains_edge(iv, named("decl", "a")));
            assert!(graph.contains_edge(iv, named("call", "t1")));
            assert!(graph.contains_edge(iv, scatter));
            assert!(graph.contains_edge(iv, conditional));
            assert!(graph.contains_edge(iv, named("output", "out")));
            assert!(graph.contains_edge(iv, ov));

            // Nested nodes are never directly gated
            assert!(!graph.contains_edge(iv, named("decl", "nested")));
            assert!(!graph.contains_edge(iv, named("call", "t2")));
            assert!(!graph.contains_edge(iv, named("decl", "conditional")));

            // The output validation depends on the outputs
            assert!(graph.contains_edge(named("output", "out"), ov));
            assert_eq!(
                graph.edges_directed(ov, Direction::Outgoing).count(),
                0,
                "output validation should have no dependents"
            );
        }
    }

    #[test]
    fn test_task_validation_edges() {
        let source = r#"
        version 1.4

        task foo {
            input {
                Int x
                Int y = 1

                validation {
                    x > 0: "x must be positive",
                }
            }

            Int a = 1

            command <<<>>>

            requirements {
                container: "ubuntu"
            }

            output {
                Int out = a

                validation {
                    out > 0: "out must be positive",
                }
            }
        }
        "#;

        let (_, task) = parse(source);
        let task = task.expect("should have task");

        let mut diagnostics = Diagnostics::default();
        let graph = TaskGraphBuilder::default().build(
            SupportedVersion::V1(V1::Four),
            &task,
            &mut diagnostics,
            |_| false,
        );
        assert!(
            diagnostics.is_empty(),
            "graph should build without diagnostics: {diagnostics:?}"
        );

        let find = |pred: &dyn Fn(&TaskGraphNode) -> bool| {
            graph
                .node_indices()
                .find(|i| pred(&graph[*i]))
                .expect("node should exist")
        };
        let edge = |from, to| {
            graph
                .find_edge(from, to)
                .map(|e| graph[e])
                .unwrap_or_else(|| panic!("expected edge from {from:?} to {to:?}"))
        };

        let x = find(&|n| matches!(n, TaskGraphNode::Input(d) if d.name().text() == "x"));
        let y = find(&|n| matches!(n, TaskGraphNode::Input(d) if d.name().text() == "y"));
        let a = find(&|n| matches!(n, TaskGraphNode::Decl(d) if d.name().text() == "a"));
        let out = find(&|n| matches!(n, TaskGraphNode::Output(_)));
        let command = find(&|n| matches!(n, TaskGraphNode::Command(_)));
        let requirements = find(&|n| matches!(n, TaskGraphNode::Requirements(_)));
        let iv = find(&|n| matches!(n, TaskGraphNode::InputValidation(_)));
        let ov = find(&|n| matches!(n, TaskGraphNode::OutputValidation(_)));

        // Name reference edges are explicit
        assert!(
            !edge(x, iv),
            "input validation reference should be explicit"
        );
        assert!(
            !edge(out, ov),
            "output validation reference should be explicit"
        );

        // Gating edges are implicit
        for node in [a, command, requirements, out, ov] {
            assert!(edge(iv, node), "gating edge should be implicit");
        }
        assert!(!graph.contains_edge(iv, x));
        assert!(!graph.contains_edge(iv, y));

        // The output validation implicitly depends on the command
        assert!(edge(command, ov), "command edge should be implicit");
    }
}
