//! Implementation of the `dev flowchart mermaid` subcommand.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write as _;

use anyhow::Context;
use anyhow::anyhow;
use indexmap::IndexSet;
use petgraph::Direction;
use wdl::analysis::Diagnostics;
use wdl::analysis::Document;
use wdl::analysis::eval::v1::WorkflowGraphBuilder;
use wdl::analysis::eval::v1::WorkflowGraphNode;
use wdl::ast::Ast;
use wdl::ast::AstNode as _;
use wdl::ast::AstToken as _;
use wdl::ast::Severity;
use wdl::ast::SyntaxNode;
use wdl::ast::v1::ConditionalStatementClauseKind;
use wdl::ast::v1::WorkflowDefinition;
use wdl::ast::v1::WorkflowStatement;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;

use crate::Config;
use crate::analysis::Source;
use crate::analysis::analyze_singular_source;
use crate::commands::CommandError;
use crate::commands::CommandResult;

/// Arguments for the `dev flowchart mermaid` subcommand.
#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The name of the workflow to render.
    #[arg(short, long, value_name = "WORKFLOW")]
    pub workflow: Option<String>,

    /// The path or URL to a `WDL` document containing the workflow.
    #[arg(value_name = "SOURCE")]
    pub source: Source,

    /// The maximum recursion depth when expanding called workflows.
    ///
    /// Defaults to unlimited. Pass `0` to disable expansion entirely (all
    /// called workflows render as plain nodes). Pass `1` to expand only direct
    /// callees, `2` for their callees, and so on.
    #[arg(long, value_name = "DEPTH")]
    pub depth: Option<usize>,

    /// The report mode for diagnostics.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// Runs the `dev flowchart mermaid` subcommand.
pub async fn mermaid(args: Args, config: &Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or_default();
    let document = analyze_singular_source(
        &args.source,
        config.common.wdl.fallback_version.into(),
        config.modules.clone(),
        config.common.wdl.feature_flags,
        config.common.ignore_filename(),
        report_mode,
        colorize,
    )
    .await
    .map_err(CommandError::from)?;

    let errors: Vec<_> = document
        .diagnostics()
        .filter(|d| d.severity() == Severity::Error)
        .collect();

    if !errors.is_empty() {
        let path = document.path().to_string();
        let source = document.root().text().to_string();
        emit_diagnostics(&path, &source, errors, report_mode, colorize)
            .context("failed to emit diagnostics")?;
        return Err(anyhow!("source contains analysis errors").into());
    }

    let local_workflows = document.local_workflows().count();
    if local_workflows == 0 {
        return Err(anyhow!("document does not contain any workflows").into());
    } else if local_workflows > 1 && args.workflow.is_none() {
        return Err(anyhow!(
            "document contains multiple workflows, specify one by name with `--workflow`"
        )
        .into());
    }

    let ast_doc = document.root();
    let Ast::V1(v1) = ast_doc.ast() else {
        return Err(anyhow!("document does not use a supported WDL v1 version").into());
    };

    let workflow = match args.workflow {
        Some(wf) => v1
            .workflows()
            .find(|w| w.name().text() == wf)
            .ok_or_else(|| anyhow!("document does not contain a workflow named `{wf}`"))?,
        None => v1.workflows().next().expect("length verified before"),
    };

    let diagram = render_mermaid(&workflow, &document, args.depth);
    println!("{diagram}");
    Ok(())
}

/// Four-space indent used at the top level of the flowchart body.
const INDENT: &str = "    ";

/// Renders a workflow definition as a `Mermaid` flowchart diagram string.
fn render_mermaid(
    workflow: &WorkflowDefinition,
    doc: &Document,
    max_depth: Option<usize>,
) -> String {
    let mut out = String::new();
    let name = workflow.name().text().to_owned();

    // SAFETY: `write!` on `String` never fails.
    writeln!(out, "---").unwrap();
    writeln!(out, "title: {name}").unwrap();
    writeln!(out, "---").unwrap();
    writeln!(out, "flowchart TD").unwrap();

    let statements: Vec<_> = workflow.statements().collect();
    let mut ctx = MermaidCtx {
        dependencies: build_dependencies(workflow, doc),
        max_depth,
        ..Default::default()
    };
    ctx.emit_statements(&statements, doc, &mut out, INDENT);

    out
}

/// An exit point from a node or group of nodes.
///
/// Carries an optional edge label to use when connecting this exit to the next
/// node in the graph.
#[derive(Clone)]
struct Exit {
    /// The `Mermaid` node ID of the exit point.
    id: String,
    /// Optional label to place on the edge leaving this exit.
    label: Option<String>,
}

impl Exit {
    fn unlabeled(id: String) -> Self {
        Self { id, label: None }
    }

    fn labeled(id: String, label: impl Into<String>) -> Self {
        Self {
            id,
            label: Some(label.into()),
        }
    }
}

/// The entry and exit points emitted for a workflow statement.
#[derive(Clone)]
struct Rendered {
    entry: String,
    exits: Vec<Exit>,
}

impl Rendered {
    /// Creates a rendered node for a leaf statement.
    fn node(id: String) -> Self {
        Self {
            entry: id.clone(),
            exits: vec![Exit::unlabeled(id)],
        }
    }
}

/// Counter-based context for generating unique `Mermaid` node IDs.
#[derive(Default)]
struct MermaidCtx {
    counter: usize,
    /// Tracks workflow URI-and-name keys that have already been expanded.
    expanded: HashSet<String>,
    /// Maximum recursion depth for workflow expansion. `None` means unlimited.
    max_depth: Option<usize>,
    /// Current recursion depth.
    current_depth: usize,
    /// Dependencies between visible workflow statements.
    dependencies: HashMap<SyntaxNode, IndexSet<SyntaxNode>>,
    /// Entry and exit points emitted for each visible workflow statement.
    emitted: HashMap<SyntaxNode, Rendered>,
}

/// A subgraph within the diagram.
struct SubGraph {
    /// The ID of the subgraph.
    group_id: String,
    /// The exit points emitted for the subgraph.
    exits: Vec<Exit>,
}

/// Labels for a conditional clause edge and optional subgraph.
#[derive(Copy, Clone)]
struct ClauseLabels<'a> {
    /// The label for the edge from the decision diamond.
    edge: &'a str,
    /// The label for the subgraph container.
    subgraph: &'a str,
}

impl<'a> ClauseLabels<'a> {
    /// Creates labels where edge and subgraph share the same name.
    fn same(label: &'a str) -> Self {
        Self {
            edge: label,
            subgraph: label,
        }
    }

    /// Creates labels with distinct edge and subgraph names.
    fn distinct(edge: &'a str, subgraph: &'a str) -> Self {
        Self { edge, subgraph }
    }
}

impl MermaidCtx {
    /// Returns a fresh unique node ID.
    fn next_id(&mut self) -> String {
        let id = format!("n{}", self.counter);
        self.counter += 1;
        id
    }

    /// Emits a subgraph with the given `label` containing the given
    /// sequence of workflow statements.
    fn emit_subgraph(
        &mut self,
        label: &str,
        stmts: &[WorkflowStatement],
        doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> SubGraph {
        let group_id = self.next_id();
        // SAFETY: `write!` on `String` never fails.
        writeln!(out, "{indent}subgraph {group_id}[\"{label}\"]").unwrap();
        let exits = self.emit_statements(stmts, doc, out, &format!("{indent}{INDENT}"));
        // SAFETY: `write!` on `String` never fails.
        writeln!(out, "{indent}end").unwrap();

        SubGraph { group_id, exits }
    }

    /// Emits a sequence of statements and their dependency edges into `out`.
    fn emit_statements(
        &mut self,
        stmts: &[WorkflowStatement],
        doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> Vec<Exit> {
        let statements: Vec<_> = stmts
            .iter()
            .filter_map(|statement| statement_key(statement).map(|key| (key, statement)))
            .collect();
        let keys: IndexSet<_> = statements.iter().map(|(key, _)| key.clone()).collect();

        for (key, statement) in &statements {
            let rendered = self.emit_statement(statement, doc, out, indent);
            self.emitted.insert(key.clone(), rendered);
        }

        let mut depended_on = IndexSet::new();

        // The evaluation graph lifts cross-scope references to the enclosing
        // statements, so the only dependencies outside this block are the
        // block's own ancestors; those are represented by the parent instead.
        for (target, _) in &statements {
            let Some(rendered) = self.emitted.get(target) else {
                continue;
            };
            let deps_in_keys: Vec<_> = self
                .dependencies
                .get(target)
                .into_iter()
                .flatten()
                .filter(|dep| keys.contains(*dep))
                .collect();

            for dependency in deps_in_keys {
                let Some(source) = self.emitted.get(dependency) else {
                    continue;
                };
                for exit in &source.exits {
                    emit_edge(
                        out,
                        indent,
                        &exit.id,
                        &rendered.entry,
                        exit.label.as_deref(),
                    );
                }
                depended_on.insert(dependency.clone());
            }
        }

        keys.difference(&depended_on)
            .filter_map(|key| self.emitted.get(key))
            .flat_map(|rendered| rendered.exits.clone())
            .collect()
    }

    /// Emits the statements within a conditional clause.
    ///
    /// For clauses with:
    ///
    /// * No statements, this returns an exit from `diamond_id` with the edge
    ///   label.
    /// * A single statement, the statement is emitted and connected from
    ///   `diamond_id` with the edge label.
    /// * Multiple visible statements, they are emitted in a subgraph.
    fn emit_conditional_clause(
        &mut self,
        diamond_id: &str,
        labels: ClauseLabels<'_>,
        stmts: &[WorkflowStatement],
        doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> Vec<Exit> {
        let statements: Vec<_> = stmts
            .iter()
            .filter(|s| !matches!(s, WorkflowStatement::Declaration(_)))
            .collect();

        match statements.len() {
            0 => vec![Exit::labeled(diamond_id.to_owned(), labels.edge)],
            1 => {
                let stmt = statements[0];
                let rendered = self.emit_statement(stmt, doc, out, indent);
                if let Some(key) = statement_key(stmt) {
                    self.emitted.insert(key, rendered.clone());
                }
                emit_edge(out, indent, diamond_id, &rendered.entry, Some(labels.edge));
                rendered.exits
            }
            _ => {
                let subgraph = self.emit_subgraph(labels.subgraph, stmts, doc, out, indent);
                emit_edge(
                    out,
                    indent,
                    diamond_id,
                    &subgraph.group_id,
                    Some(labels.edge),
                );
                if subgraph.exits.is_empty() {
                    vec![Exit::unlabeled(subgraph.group_id)]
                } else {
                    subgraph.exits
                }
            }
        }
    }

    /// Emits a single statement into `out` and returns its exit points.
    fn emit_statement(
        &mut self,
        stmt: &WorkflowStatement,
        doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> Rendered {
        match stmt {
            WorkflowStatement::Call(call) => {
                let names: Vec<_> = call.target().names().map(|n| n.text().to_owned()).collect();

                let target_str = names.join(".");
                let label = call
                    .alias()
                    .map(|a| a.name().text().to_owned())
                    .unwrap_or_else(|| {
                        // SAFETY: a call target always has at least one name
                        // component.
                        names.last().unwrap().clone()
                    });

                let callee_doc_and_wf = match names.as_slice() {
                    [ns, callable] => doc.namespace(ns).and_then(|n| {
                        resolve_workflow_local(n.document(), callable).map(|wf| (n.document(), wf))
                    }),
                    [callable] => resolve_workflow_local(doc, callable).map(|wf| (doc, wf)),
                    _ => None,
                };

                if let Some((callee_doc, wf_def)) = callee_doc_and_wf {
                    return self.emit_workflow_call(
                        &label,
                        &target_str,
                        &wf_def,
                        callee_doc,
                        out,
                        indent,
                    );
                }

                // Task (leaf) node.
                self.emit_call_node(&label, &target_str, out, indent)
            }

            WorkflowStatement::Scatter(scatter) => {
                let var = scatter.variable().text().to_owned();
                let procs_id = self.next_id();

                // SAFETY: `write!` on `String` never fails.
                writeln!(
                    out,
                    "{indent}{procs_id}@{{ shape: procs, label: \"scatter ({var})\" }}"
                )
                .unwrap();

                let body: Vec<_> = scatter.statements().collect();
                let subgraph =
                    self.emit_subgraph(&format!("scatter ({var})"), &body, doc, out, indent);

                emit_edge(out, indent, &procs_id, &subgraph.group_id, None);

                Rendered {
                    entry: procs_id,
                    exits: if subgraph.exits.is_empty() {
                        vec![Exit::unlabeled(subgraph.group_id)]
                    } else {
                        subgraph.exits
                    },
                }
            }

            WorkflowStatement::Conditional(cond) => {
                let clauses: Vec<_> = cond.clauses().collect();
                let has_else = clauses
                    .iter()
                    .any(|c| c.kind() == ConditionalStatementClauseKind::Else);
                let is_multi_clause = clauses.len() > 1;

                let diamond_id = self.next_id();

                if is_multi_clause {
                    // if/else-if/else: diamond says "if", conditions go on
                    // edges. SAFETY: `write!` on `String`
                    // never fails.
                    writeln!(out, "{indent}{diamond_id}{{\"if\"}}").unwrap();

                    let mut exits = Vec::new();

                    for clause in &clauses {
                        let edge_label = match clause.kind() {
                            ConditionalStatementClauseKind::If => clause
                                .expr()
                                .map(|e| format!("if {}", escape(&e.inner().to_string())))
                                .unwrap_or_else(|| "if".to_owned()),
                            ConditionalStatementClauseKind::ElseIf => clause
                                .expr()
                                .map(|e| format!("else if {}", escape(&e.inner().to_string())))
                                .unwrap_or_else(|| "else if".to_owned()),
                            ConditionalStatementClauseKind::Else => "else".to_owned(),
                        };

                        let body: Vec<_> = clause.statements().collect();
                        let clause_exits = self.emit_conditional_clause(
                            &diamond_id,
                            ClauseLabels::same(&edge_label),
                            &body,
                            doc,
                            out,
                            indent,
                        );
                        exits.extend(clause_exits);
                    }

                    // Without an `else`, the diamond is also an exit via the
                    // skip path.
                    if !has_else {
                        exits.push(Exit::labeled(diamond_id.clone(), "no"));
                    }

                    Rendered {
                        entry: diamond_id,
                        exits,
                    }
                } else {
                    // Plain `if` (no else): condition in diamond, yes/no edges.
                    let condition = clauses
                        .first()
                        .and_then(|c| c.expr())
                        .map(|e| escape(&e.inner().to_string()))
                        .unwrap_or_default();

                    // SAFETY: `write!` on `String` never fails.
                    writeln!(out, "{indent}{diamond_id}{{\"{condition}\"}}").unwrap();

                    let body: Vec<_> = clauses
                        .first()
                        .map(|c| c.statements().collect())
                        .unwrap_or_default();
                    let mut exits = self.emit_conditional_clause(
                        &diamond_id,
                        ClauseLabels::distinct("yes", "then"),
                        &body,
                        doc,
                        out,
                        indent,
                    );

                    // The "no" path skips the body entirely.
                    exits.push(Exit::labeled(diamond_id.clone(), "no"));

                    Rendered {
                        entry: diamond_id,
                        exits,
                    }
                }
            }

            // SAFETY: declarations are filtered out before this method is
            // called.
            WorkflowStatement::Declaration(_) => unreachable!(),
        }
    }

    /// Emits a leaf call node (task or unresolvable target).
    fn emit_call_node(
        &mut self,
        label: &str,
        target: &str,
        out: &mut String,
        indent: &str,
    ) -> Rendered {
        let node_id = self.next_id();

        if target == label {
            // SAFETY: `write!` on `String` never fails.
            writeln!(out, "{indent}{node_id}[\"{label}\"]").unwrap();
        } else {
            // SAFETY: `write!` on `String` never fails.
            writeln!(out, "{indent}{node_id}[\"{label}<br/><i>{target}</i>\"]").unwrap();
        }

        Rendered::node(node_id)
    }

    /// Emits a called workflow as an expanded `subgraph`, then recursively
    /// descends into its body. Falls back to a leaf node when the workflow
    /// has already been expanded (cycle guard).
    fn emit_workflow_call(
        &mut self,
        label: &str,
        target: &str,
        wf_def: &WorkflowDefinition,
        callee_doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> Rendered {
        let uri = callee_doc.uri().to_string();
        let cycle_key = format!("{uri}#{}", wf_def.name().text());

        // Depth guard: render as a plain node if the depth limit has been
        // reached. This is checked before the cycle guard so that a workflow
        // skipped here can still be expanded by a shallower call.
        if self.max_depth.is_some_and(|max| self.current_depth >= max) {
            return self.emit_call_node(label, target, out, indent);
        }

        // Render as a plain node if this workflow has already been expanded.
        if !self.expanded.insert(cycle_key) {
            return self.emit_call_node(label, target, out, indent);
        }

        self.current_depth += 1;

        let subgraph_label = if target == label {
            label.to_owned()
        } else {
            format!("{label} ({target})")
        };

        let body: Vec<_> = wf_def.statements().collect();
        self.dependencies
            .extend(build_dependencies(wf_def, callee_doc));
        let subgraph = self.emit_subgraph(&subgraph_label, &body, callee_doc, out, indent);

        self.current_depth -= 1;

        Rendered {
            entry: subgraph.group_id.clone(),
            exits: if subgraph.exits.is_empty() {
                vec![Exit::unlabeled(subgraph.group_id)]
            } else {
                subgraph.exits
            },
        }
    }
}

/// Emits a single directed edge, with an optional label.
///
/// Labels are quoted so that characters such as `|` do not end the label
/// early. Callers must pass labels that are already escaped.
fn emit_edge(out: &mut String, indent: &str, from: &str, to: &str, label: Option<&str>) {
    // SAFETY: `write!` on `String` never fails.
    if let Some(lbl) = label {
        writeln!(out, "{indent}{from} -->|\"{lbl}\"| {to}").unwrap();
    } else {
        writeln!(out, "{indent}{from} --> {to}").unwrap();
    }
}

/// Attempts to find a workflow by name in a document's AST.
fn resolve_workflow_local(doc: &Document, name: &str) -> Option<WorkflowDefinition> {
    // Only expand if the analysis confirms it is a workflow (not a task).
    doc.local_workflow_by_name(name)?;

    // SAFETY: the analysis confirmed a workflow exists in this document, so
    // the AST must be V1; a non-V1 AST cannot produce a workflow entry.
    let Ast::V1(v1) = doc.root().ast() else {
        unreachable!()
    };

    v1.workflows().find(|w| w.name().text() == name)
}

/// Builds dependencies between visible statements from the evaluation graph.
fn build_dependencies(
    workflow: &WorkflowDefinition,
    doc: &Document,
) -> HashMap<SyntaxNode, IndexSet<SyntaxNode>> {
    let mut diagnostics = Diagnostics::default();
    let graph = WorkflowGraphBuilder::default().build(
        workflow,
        &mut diagnostics,
        |_| false,
        |name| doc.struct_by_name(name).is_some() || doc.enum_by_name(name).is_some(),
    );
    debug_assert!(
        diagnostics.is_empty(),
        "the analyzed workflow should produce a valid evaluation graph"
    );

    let mut dependencies = HashMap::<_, IndexSet<_>>::new();
    for target_index in graph.node_indices() {
        let target_node = &graph[target_index];
        if !matches!(
            target_node,
            WorkflowGraphNode::Call(_)
                | WorkflowGraphNode::Conditional(..)
                | WorkflowGraphNode::Scatter(..)
        ) {
            continue;
        }
        let target = target_node.inner().clone();
        let mut pending: Vec<_> = graph
            .neighbors_directed(target_index, Direction::Incoming)
            .collect();
        let mut visited = HashSet::new();

        while let Some(source_index) = pending.pop() {
            if !visited.insert(source_index) {
                continue;
            }

            let source_node = &graph[source_index];
            if !matches!(
                source_node,
                WorkflowGraphNode::Input(_)
                    | WorkflowGraphNode::Decl(_)
                    | WorkflowGraphNode::Output(_)
                    | WorkflowGraphNode::ConditionalClause(..)
            ) && source_node.inner() != &target
            {
                dependencies
                    .entry(target.clone())
                    .or_default()
                    .insert(source_node.inner().clone());
            } else {
                pending.extend(graph.neighbors_directed(source_index, Direction::Incoming));
            }
        }
    }

    dependencies
}

/// Gets the syntax node that identifies a visible workflow statement.
fn statement_key(statement: &WorkflowStatement) -> Option<SyntaxNode> {
    (!matches!(statement, WorkflowStatement::Declaration(_))).then(|| statement.inner().clone())
}

/// Escapes text for use inside a quoted `Mermaid` label.
///
/// Whitespace runs (including newlines) collapse to a single space, and
/// characters with special meaning become `Mermaid` entity codes. `#` is
/// escaped first because `Mermaid` treats `#...;` as an entity code.
fn escape(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('#', "#35;")
        .replace('&', "#amp;")
        .replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
}
