//! Implementation of the `dev flowchart mermaid` subcommand.

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
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;

/// Arguments for the `dev flowchart mermaid` subcommand.
#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The path or URL to a `WDL` document containing the workflow.
    #[clap(value_name = "SOURCE")]
    pub source: Source,

    /// The name of the workflow to render.
    ///
    /// If not specified and the document contains exactly one workflow, that
    /// workflow is used. If the document contains multiple workflows, this
    /// argument is required.
    #[clap(short, long, value_name = "NAME")]
    pub workflow: Option<String>,

    /// The maximum recursion depth when expanding called workflows.
    ///
    /// Defaults to unlimited. Pass `0` to disable expansion entirely (all
    /// called workflows render as plain nodes). Pass `1` to expand only direct
    /// callees, `2` for their callees, and so on.
    #[clap(long, value_name = "DEPTH")]
    pub depth: Option<usize>,

    /// The report mode for diagnostics.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// Runs the `dev flowchart mermaid` subcommand.
pub async fn mermaid(args: Args, config: &Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or_default();

    let results = Analysis::default()
        .add_source(args.source.clone())
        .fallback_version(config.common.wdl.fallback_version.into())
        .run(report_mode, colorize)
        .await
        .map_err(CommandError::from)?;

    // SAFETY: we added `args.source` as the only source above.
    let document = results
        .filter(&[&args.source])
        .next()
        .unwrap()
        .document()
        .clone();

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

    let ast_doc = document.root();
    let Ast::V1(v1) = ast_doc.ast() else {
        return Err(anyhow!("document does not use a supported WDL v1 version").into());
    };

    let mut workflows: Vec<_> = v1.workflows().collect();

    let workflow = match &args.workflow {
        Some(name) => workflows
            .into_iter()
            .find(|w| w.name().text() == name.as_str())
            .ok_or_else(|| anyhow!("no workflow named `{name}` was found in the document"))?,
        None => {
            if workflows.len() > 1 {
                let names: Vec<_> = workflows
                    .iter()
                    .map(|w| w.name().text().to_owned())
                    .collect();
                return Err(anyhow!(
                    "document contains multiple workflows (`{}`); use `--workflow` to specify one",
                    names.join("`, `")
                )
                .into());
            }
            workflows
                .pop()
                .ok_or_else(|| anyhow!("document does not contain any workflows"))?
        }
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

/// The root and exit points emitted for a statement block.
struct Block {
    roots: Vec<String>,
    exits: Vec<Exit>,
}

/// Counter-based context for generating unique `Mermaid` node IDs.
#[derive(Default)]
struct MermaidCtx {
    counter: usize,
    /// Tracks which workflow URIs have already been expanded to prevent
    /// infinite recursion through mutually-recursive workflows.
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

impl MermaidCtx {
    /// Returns a fresh unique node ID.
    fn next_id(&mut self) -> String {
        let id = format!("n{}", self.counter);
        self.counter += 1;
        id
    }

    /// Emits a sequence of statements and their dependency edges into `out`.
    fn emit_statements(
        &mut self,
        stmts: &[WorkflowStatement],
        doc: &Document,
        out: &mut String,
        indent: &str,
    ) -> Block {
        let statements: Vec<_> = stmts
            .iter()
            .filter_map(|statement| statement_key(statement).map(|key| (key, statement)))
            .collect();
        let keys: IndexSet<_> = statements.iter().map(|(key, _)| key.clone()).collect();

        for (key, statement) in &statements {
            let rendered = self.emit_statement(statement, doc, out, indent);
            self.emitted.insert(key.clone(), rendered);
        }

        for (target, _) in &statements {
            let Some(rendered) = self.emitted.get(target) else {
                continue;
            };
            for dependency in self.dependencies.get(target).into_iter().flatten() {
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
            }
        }

        let depended_on: IndexSet<_> = keys
            .iter()
            .flat_map(|key| self.dependencies.get(key).into_iter().flatten())
            .filter(|dependency| keys.contains(*dependency))
            .cloned()
            .collect();

        Block {
            roots: keys
                .iter()
                .filter(|key| {
                    self.dependencies
                        .get(*key)
                        .into_iter()
                        .flatten()
                        .all(|dependency| !keys.contains(dependency))
                })
                .filter_map(|key| self.emitted.get(key).map(|rendered| rendered.entry.clone()))
                .collect(),
            exits: keys
                .difference(&depended_on)
                .filter_map(|key| self.emitted.get(key))
                .flat_map(|rendered| rendered.exits.clone())
                .collect(),
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
                        // SAFETY: a call target always has at least one name component.
                        names.last().unwrap().clone()
                    });

                // If there are two name components, the first is a namespace.
                // Try to resolve it to a workflow so we can expand inline.
                if let Some(wf_def) = names
                    .first()
                    .zip(names.get(1))
                    .and_then(|(ns, task)| resolve_workflow(doc, ns, task))
                {
                    // SAFETY: `resolve_workflow` succeeded above, which requires `names` to
                    // have at least one element (the namespace component), and the namespace
                    // to exist in the document.
                    let callee = doc.namespace(names.first().unwrap()).unwrap().document();
                    return self.emit_workflow_call(
                        &label,
                        &target_str,
                        &wf_def,
                        callee,
                        out,
                        indent,
                    );
                }

                // Single-name call — resolve against the current document.
                if let Some(wf_def) = resolve_workflow_local(doc, &target_str) {
                    return self.emit_workflow_call(&label, &target_str, &wf_def, doc, out, indent);
                }

                // Task (leaf) node.
                self.emit_call_node(&label, &target_str, out, indent)
            }

            WorkflowStatement::Scatter(scatter) => {
                let var = scatter.variable().text().to_owned();
                let procs_id = self.next_id();
                let group_id = self.next_id();

                // SAFETY: `write!` on `String` never fails.
                writeln!(
                    out,
                    "{indent}{procs_id}@{{ shape: procs, label: \"scatter ({var})\" }}"
                )
                .unwrap();

                // SAFETY: `write!` on `String` never fails.
                writeln!(out, "{indent}subgraph {group_id}[\"scatter ({var})\"]").unwrap();

                let body: Vec<_> = scatter.statements().collect();
                let block = self.emit_statements(&body, doc, out, &format!("{indent}{INDENT}"));

                // SAFETY: `write!` on `String` never fails.
                writeln!(out, "{indent}end").unwrap();

                emit_edge(out, indent, &procs_id, &group_id, None);

                Rendered {
                    entry: procs_id,
                    exits: if block.exits.is_empty() {
                        vec![Exit::unlabeled(group_id)]
                    } else {
                        block.exits
                    },
                }
            }

            WorkflowStatement::Conditional(cond) => {
                let clauses: Vec<_> = cond.clauses().collect();
                let has_else = clauses
                    .iter()
                    .any(|c| c.kind() == ConditionalStatementClauseKind::Else);
                let is_multi_clause = clauses.len() > 1
                    || clauses
                        .first()
                        .is_some_and(|c| c.kind() != ConditionalStatementClauseKind::If);

                let diamond_id = self.next_id();

                if is_multi_clause {
                    // if/else-if/else: diamond says "if", conditions go on edges.
                    // SAFETY: `write!` on `String` never fails.
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
                        let block = self.emit_statements(&body, doc, out, indent);
                        for root in block.roots {
                            emit_edge(out, indent, &diamond_id, &root, Some(&edge_label));
                        }
                        if block.exits.is_empty() {
                            exits.push(Exit::labeled(diamond_id.clone(), edge_label));
                        } else {
                            exits.extend(block.exits);
                        }
                    }

                    // Without an `else`, the diamond is also an exit via the skip path.
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
                    let block = self.emit_statements(&body, doc, out, indent);
                    for root in block.roots {
                        emit_edge(out, indent, &diamond_id, &root, Some("yes"));
                    }
                    let mut exits = block.exits;
                    if exits.is_empty() {
                        exits.push(Exit::labeled(diamond_id.clone(), "yes"));
                    }

                    // The "no" path skips the body entirely.
                    exits.push(Exit::labeled(diamond_id.clone(), "no"));

                    Rendered {
                        entry: diamond_id,
                        exits,
                    }
                }
            }

            // SAFETY: declarations are filtered out before this method is called.
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

        Rendered {
            entry: node_id.clone(),
            exits: vec![Exit::unlabeled(node_id)],
        }
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

        // Cycle guard: render as a plain node if already expanding this workflow.
        if !self.expanded.insert(cycle_key) {
            return self.emit_call_node(label, target, out, indent);
        }

        // Depth guard: render as a plain node if the depth limit has been reached.
        if self.max_depth.is_some_and(|max| self.current_depth >= max) {
            return self.emit_call_node(label, target, out, indent);
        }

        self.current_depth += 1;

        let group_id = self.next_id();
        let subgraph_label = if target == label {
            label.to_owned()
        } else {
            format!("{label} ({target})")
        };

        // SAFETY: `write!` on `String` never fails.
        writeln!(out, "{indent}subgraph {group_id}[\"{subgraph_label}\"]").unwrap();

        let body: Vec<_> = wf_def.statements().collect();
        self.dependencies
            .extend(build_dependencies(wf_def, callee_doc));
        let block = self.emit_statements(&body, callee_doc, out, &format!("{indent}{INDENT}"));

        // SAFETY: `write!` on `String` never fails.
        writeln!(out, "{indent}end").unwrap();

        self.current_depth -= 1;

        Rendered {
            entry: group_id.clone(),
            exits: if block.exits.is_empty() {
                vec![Exit::unlabeled(group_id)]
            } else {
                block.exits
            },
        }
    }
}

/// Emits a single directed edge, with an optional label.
fn emit_edge(out: &mut String, indent: &str, from: &str, to: &str, label: Option<&str>) {
    // SAFETY: `write!` on `String` never fails.
    if let Some(lbl) = label {
        writeln!(out, "{indent}{from} -->|{lbl}| {to}").unwrap();
    } else {
        writeln!(out, "{indent}{from} --> {to}").unwrap();
    }
}

/// Attempts to resolve a two-part call target (`ns.name`) to a workflow AST
/// node in the analysis document's namespaces.
fn resolve_workflow(doc: &Document, ns: &str, name: &str) -> Option<WorkflowDefinition> {
    let ns_document = doc.namespace(ns)?.document();
    resolve_workflow_local(ns_document, name)
}

/// Attempts to find a workflow by name in a document's AST.
fn resolve_workflow_local(doc: &Document, name: &str) -> Option<WorkflowDefinition> {
    // Only expand if the analysis confirms it is a workflow (not a task).
    doc.workflow().filter(|w| w.name() == name)?;

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

/// Escapes characters that have special meaning in `Mermaid` label strings.
fn escape(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn render(source: &str) -> String {
        // SAFETY: the operating system can create a temporary directory for the test.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.wdl");
        // SAFETY: the temporary directory exists and is writable for the test.
        std::fs::write(&path, source).unwrap();
        // SAFETY: `path` identifies the WDL file created above.
        let source: Source = path.to_string_lossy().parse().unwrap();
        let results = Analysis::default()
            .add_source(source.clone())
            .run(Mode::default(), false)
            .await
            // SAFETY: the local test document and its imports are valid.
            .unwrap();
        let document = results
            .filter(&[&source])
            .next()
            // SAFETY: the test source was added to this analysis.
            .unwrap()
            .document()
            .clone();
        let workflow = document
            .root()
            .ast()
            .into_v1()
            // SAFETY: each test source declares a supported WDL version.
            .unwrap()
            .workflows()
            .next()
            // SAFETY: each test source declares one workflow.
            .unwrap();

        render_mermaid(&workflow, &document, Some(0))
    }

    #[tokio::test]
    async fn renders_calls_from_their_dependencies() {
        let source = r#"
version 1.0

task produce {
    command <<<
        echo value
    >>>

    output {
        String value = stdout()
    }
}

task consume {
    input {
        String value
    }

    command <<<
        echo ~{value}
    >>>
}

workflow test {
    call produce
    call consume as left { input: value = produce.value }
    call consume as right { input: value = produce.value }
}
"#;

        let diagram = render(source).await;

        assert!(diagram.contains("n0 --> n1"));
        assert!(diagram.contains("n0 --> n2"));
        assert!(!diagram.contains("n1 --> n2"));
    }

    #[tokio::test]
    async fn preserves_both_paths_through_declaration_only_conditionals() {
        let diagram = render(
            r#"
version 1.0

task consume {
    input { String value }
    command <<< echo ~{value} >>>
}

workflow test {
    input { Boolean enabled }
    if (enabled) {
        String selected = "yes"
    }
    call consume { input: value = select_first([selected, "no"]) }
}
"#,
        )
        .await;

        assert!(diagram.contains("n0 -->|yes| n1"));
        assert!(diagram.contains("n0 -->|no| n1"));
    }

    #[tokio::test]
    async fn preserves_fallthrough_from_multi_clause_conditionals() {
        let diagram = render(
            r#"
version 1.3

task consume {
    input { String value }
    command <<< echo ~{value} >>>
}

workflow test {
    input { String mode }
    if (mode == "a") {
        String selected = "a"
    } else if (mode == "b") {
        String selected = "b"
    }
    call consume { input: value = select_first([selected, "none"]) }
}
"#,
        )
        .await;

        assert!(diagram.contains("n0 -->|no| n1"));
    }

    #[tokio::test]
    async fn renders_deterministically() {
        let source = r#"
version 1.0

task produce {
    command <<< echo value >>>
    output { String value = stdout() }
}

task combine {
    input {
        String left
        String right
    }
    command <<< echo ~{left} ~{right} >>>
}

workflow test {
    call produce as left
    call produce as right
    call combine { input:
        left = left.value,
        right = right.value,
    }
}
"#;
        let expected = render(source).await;

        for _ in 0..16 {
            assert_eq!(render(source).await, expected);
        }
    }
}
