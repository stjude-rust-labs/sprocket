//! Implementation of the `dev flowchart mermaid` subcommand.

use std::collections::HashSet;
use std::fmt::Write as _;

use anyhow::Context;
use anyhow::anyhow;
use wdl::analysis::Document;
use wdl::ast::Ast;
use wdl::ast::AstNode as _;
use wdl::ast::AstToken as _;
use wdl::ast::Severity;
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
        max_depth,
        ..Default::default()
    };
    ctx.emit_statements(&statements, doc, &mut out, INDENT, &[]);

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
}

impl MermaidCtx {
    /// Returns a fresh unique node ID.
    fn next_id(&mut self) -> String {
        let id = format!("n{}", self.counter);
        self.counter += 1;
        id
    }

    /// Emits a sequence of statements into `out`.
    ///
    /// `incoming` carries the set of exit points (node IDs and optional edge
    /// labels) that should connect to the first statement. Returns the exit
    /// points of the last statement, or `incoming` when `stmts` is empty.
    fn emit_statements(
        &mut self,
        stmts: &[WorkflowStatement],
        doc: &Document,
        out: &mut String,
        indent: &str,
        incoming: &[Exit],
    ) -> Vec<Exit> {
        if stmts.is_empty() {
            return incoming.to_vec();
        }

        let mut exits = incoming.to_vec();
        for stmt in stmts {
            exits = self.emit_statement(stmt, doc, out, indent, &exits);
        }
        exits
    }

    /// Emits a single statement into `out` and returns its exit points.
    fn emit_statement(
        &mut self,
        stmt: &WorkflowStatement,
        doc: &Document,
        out: &mut String,
        indent: &str,
        incoming: &[Exit],
    ) -> Vec<Exit> {
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
                        incoming,
                    );
                }

                // Single-name call — resolve against the current document.
                if let Some(wf_def) = resolve_workflow_local(doc, &target_str) {
                    return self.emit_workflow_call(
                        &label,
                        &target_str,
                        &wf_def,
                        doc,
                        out,
                        indent,
                        incoming,
                    );
                }

                // Task (leaf) node.
                self.emit_call_node(&label, &target_str, out, indent, incoming)
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

                for exit in incoming {
                    emit_edge(out, indent, &exit.id, &procs_id, exit.label.as_deref());
                }

                // SAFETY: `write!` on `String` never fails.
                writeln!(out, "{indent}subgraph {group_id}[\"scatter ({var})\"]").unwrap();

                let body: Vec<_> = scatter.statements().collect();
                let inner_exits =
                    self.emit_statements(&body, doc, out, &format!("{indent}{INDENT}"), &[]);

                // SAFETY: `write!` on `String` never fails.
                writeln!(out, "{indent}end").unwrap();

                emit_edge(out, indent, &procs_id, &group_id, None);

                if inner_exits.is_empty() {
                    vec![Exit::unlabeled(group_id)]
                } else {
                    inner_exits
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

                    for exit in incoming {
                        emit_edge(out, indent, &exit.id, &diamond_id, exit.label.as_deref());
                    }

                    let mut all_exits: Vec<Exit> = Vec::new();

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
                        let body_incoming = vec![Exit::labeled(diamond_id.clone(), edge_label)];
                        let exits = self.emit_statements(&body, doc, out, indent, &body_incoming);

                        if exits.len() == 1 && exits[0].id == diamond_id {
                            all_exits.push(Exit::unlabeled(diamond_id.clone()));
                        } else {
                            all_exits.extend(exits);
                        }
                    }

                    // Without an `else`, the diamond is also an exit via the skip path.
                    if !has_else && !all_exits.iter().any(|e| e.id == diamond_id) {
                        all_exits.push(Exit::labeled(diamond_id, "no"));
                    }

                    all_exits
                } else {
                    // Plain `if` (no else): condition in diamond, yes/no edges.
                    let condition = clauses
                        .first()
                        .and_then(|c| c.expr())
                        .map(|e| escape(&e.inner().to_string()))
                        .unwrap_or_default();

                    // SAFETY: `write!` on `String` never fails.
                    writeln!(out, "{indent}{diamond_id}{{\"{condition}\"}}").unwrap();

                    for exit in incoming {
                        emit_edge(out, indent, &exit.id, &diamond_id, exit.label.as_deref());
                    }

                    let body: Vec<_> = clauses
                        .first()
                        .map(|c| c.statements().collect())
                        .unwrap_or_default();
                    let body_incoming = vec![Exit::labeled(diamond_id.clone(), "yes")];
                    let mut all_exits =
                        self.emit_statements(&body, doc, out, indent, &body_incoming);

                    // The "no" path skips the body entirely.
                    if !all_exits.iter().any(|e| e.id == diamond_id) {
                        all_exits.push(Exit::labeled(diamond_id, "no"));
                    }

                    all_exits
                }
            }

            WorkflowStatement::Declaration(_) => incoming.to_vec(),
        }
    }

    /// Emits a leaf call node (task or unresolvable target).
    fn emit_call_node(
        &mut self,
        label: &str,
        target: &str,
        out: &mut String,
        indent: &str,
        incoming: &[Exit],
    ) -> Vec<Exit> {
        let node_id = self.next_id();

        if target == label {
            // SAFETY: `write!` on `String` never fails.
            writeln!(out, "{indent}{node_id}[\"{label}\"]").unwrap();
        } else {
            // SAFETY: `write!` on `String` never fails.
            writeln!(out, "{indent}{node_id}[\"{label}<br/><i>{target}</i>\"]").unwrap();
        }

        for exit in incoming {
            emit_edge(out, indent, &exit.id, &node_id, exit.label.as_deref());
        }

        vec![Exit::unlabeled(node_id)]
    }

    /// Emits a called workflow as an expanded `subgraph`, then recursively
    /// descends into its body. Falls back to a leaf node when the workflow
    /// has already been expanded (cycle guard).
    #[expect(clippy::too_many_arguments)]
    fn emit_workflow_call(
        &mut self,
        label: &str,
        target: &str,
        wf_def: &WorkflowDefinition,
        callee_doc: &Document,
        out: &mut String,
        indent: &str,
        incoming: &[Exit],
    ) -> Vec<Exit> {
        let uri = callee_doc.uri().to_string();
        let cycle_key = format!("{uri}#{}", wf_def.name().text());

        // Cycle guard: render as a plain node if already expanding this workflow.
        if !self.expanded.insert(cycle_key) {
            return self.emit_call_node(label, target, out, indent, incoming);
        }

        // Depth guard: render as a plain node if the depth limit has been reached.
        if self.max_depth.is_some_and(|max| self.current_depth >= max) {
            return self.emit_call_node(label, target, out, indent, incoming);
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
        let inner_exits =
            self.emit_statements(&body, callee_doc, out, &format!("{indent}{INDENT}"), &[]);

        // SAFETY: `write!` on `String` never fails.
        writeln!(out, "{indent}end").unwrap();

        self.current_depth -= 1;

        for exit in incoming {
            emit_edge(out, indent, &exit.id, &group_id, exit.label.as_deref());
        }

        if inner_exits.is_empty() {
            vec![Exit::unlabeled(group_id)]
        } else {
            inner_exits
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

/// Escapes characters that have special meaning in `Mermaid` label strings.
fn escape(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
