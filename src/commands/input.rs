//! Implementation of the `input` command.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use serde_json::Map;
use serde_json::Value;
use url::Url;
use wdl::analysis::AnalysisResult;
use wdl::analysis::path_to_uri;
use wdl::analysis::types::CallKind;
use wdl::ast::AstNode;
use wdl::ast::AstToken;
use wdl::ast::SyntaxKind;
use wdl::ast::v1::Expr;
use wdl::ast::v1::InputSection;
use wdl::ast::v1::LiteralExpr;
use wdl::cli::analyze;

use crate::Mode;

/// Arguments for the `input` subcommand.
#[derive(Parser, Debug)]
pub struct InputArgs {
    /// The path to the WDL document or a directory containing WDL documents to
    /// validate.
    #[arg(value_name = "PATH or URL")]
    pub path: String,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,

    /// Show inputs with non-literal default values and non-required inputs.
    #[arg(long)]
    pub show_expressions: bool,

    /// Hide inputs with default values.
    #[arg(long)]
    pub hide_defaults: bool,

    /// Include task-level inputs.
    #[arg(
        long,
        long_help = "Includes inputs from tasks called in the workflow. When using this option \
                     may want to omit `--show-expressions` to avoid cluttering the output with \
                     task-level inputs."
    )]
    pub nested_inputs: bool,

    /// Output input template in YAML format.
    #[arg(long)]
    pub yaml: bool,
}

/// Process an input section
fn process_input_section(
    map: &mut Map<String, Value>,
    inputs: InputSection,
    show_expressions: bool,
    hide_defaults: bool,
    wf_name: &str,
) {
    let inputs = inputs.declarations();
    inputs.for_each(|i| match i {
        wdl::ast::v1::Decl::Bound(bound_decl) => {
            let name = bound_decl.name().text().to_string();
            let typ = bound_decl.ty();
            let value = bound_decl.expr();
            match typ.is_optional() {
                true => {
                    // Only render bound, optional inputs if
                    // hide_expressions and hide_defaults are false.
                    if show_expressions && !hide_defaults {
                        map.insert(format!("{wf_name}.{name}"), Value::String(typ.to_string()));
                    }
                }
                false => match value {
                    Expr::Literal(ref l) if !hide_defaults => match l {
                        LiteralExpr::Boolean(b) => {
                            map.insert(format!("{wf_name}.{name}"), Value::Bool(b.value()));
                        }
                        LiteralExpr::String(s) => {
                            if s.is_empty() {
                                map.insert(
                                    format!("{wf_name}.{name}"),
                                    Value::String("".to_string()),
                                );
                            } else {
                                let t = s.text();
                                let t = t.expect("should have text");
                                let mut text: String = "".to_string();
                                t.unescape_to(&mut text);
                                map.insert(format!("{wf_name}.{name}"), Value::String(text));
                            }
                        }
                        LiteralExpr::Integer(i) => {
                            map.insert(
                                format!("{wf_name}.{name}"),
                                Value::from(i.value().expect("should have a value")),
                            );
                        }
                        LiteralExpr::Float(f) => {
                            map.insert(
                                format!("{wf_name}.{name}"),
                                Value::from(f.value().expect("should have a value")),
                            );
                        }
                        _ => {
                            map.insert(
                                format!("{wf_name}.{name}"),
                                Value::String(format!(
                                    "{} (default = {})",
                                    typ,
                                    value.text()
                                )),
                            );
                        }
                    },
                    Expr::Negation(ref n) if !hide_defaults => {
                        // Negation isn't a literal, but might contain one.
                        if n.inner().children().count() != 1
                            || n.inner().first_child().expect("should have a child").kind()
                                != SyntaxKind::LiteralIntegerNode
                        {
                            if show_expressions && !hide_defaults {
                                map.insert(
                                    format!("{wf_name}.{name}"),
                                    Value::String(format!(
                                        "{} (default = {})",
                                        typ,
                                        value.text()
                                    )),
                                );
                            }
                        } else {
                            let value = n
                                .text()
                                .to_string()
                                .parse::<i64>()
                                .expect("should be an integer");
                            map.insert(format!("{wf_name}.{name}"), Value::from(value));
                        }
                    }
                    _ => {
                        if show_expressions && !hide_defaults {
                            map.insert(
                                format!("{wf_name}.{name}"),
                                Value::String(format!(
                                    "{} (default = {})",
                                    typ,
                                    value.text()
                                )),
                            );
                        }
                    }
                },
            }
        }
        wdl::ast::v1::Decl::Unbound(unbound_decl) => {
            let name = unbound_decl.name().text().to_string();
            let typ = unbound_decl.ty();
            match typ.is_optional() {
                true => {
                    // Only render unbound, optional inputs if
                    // hide_expressions and hide_defaults are false.
                    if show_expressions && !hide_defaults {
                        map.insert(format!("{wf_name}.{name}"), Value::Null);
                    }
                }
                false => {
                    // always render unbound and required inputs
                    map.insert(format!("{wf_name}.{name}"), Value::String(typ.to_string()));
                }
            }
        }
    });
}

/// Process a task and its inputs.
fn process_task(
    map: &mut Map<String, Value>,
    task: &wdl::ast::v1::TaskDefinition,
    specified: &HashSet<String>,
    show_expressions: bool,
    hide_defaults: bool,
    prefix: &str,
    call_name: &str,
) {
    let inputs = task.input();
    if let Some(inputs) = inputs {
        process_input_section(
            map,
            inputs,
            show_expressions,
            hide_defaults,
            format!("{prefix}.{call_name}").as_str(),
        );
        // Remove any inputs that are specified in the workflow
        // call.
        specified.iter().for_each(|s| {
            map.remove(format!("{prefix}.{call_name}.{s}").as_str());
        });
    }
}

/// Process a workflow and its inputs.
#[allow(clippy::only_used_in_recursion,clippy::too_many_arguments)]
fn process_workflow(
    map: &mut Map<String, Value>,
    document: &wdl::analysis::document::Document,
    workflow_analysis: &wdl::analysis::document::Workflow,
    workflow_ast: &wdl::ast::v1::WorkflowDefinition,
    specified: &HashSet<String>,
    show_expressions: bool,
    hide_defaults: bool,
    nested_inputs: bool,
    prefix: &str,
    results: &Vec<AnalysisResult>,
) {
    let inputs = workflow_ast.input();
    if let Some(inputs) = inputs {
        process_input_section(map, inputs, show_expressions, hide_defaults, prefix);
    }

    // If the user wants nested inputs and the workflow allows it, process the
    // calls.
    if workflow_analysis.allows_nested_inputs() && nested_inputs {
        let calls = workflow_analysis.calls();
        calls.iter().for_each(|(call_name, call)| {
            match call.kind() {
                CallKind::Task => {
                    let namespace = call.namespace();
                    let name = call.name();
                    let specified = call.specified();
                    match namespace {
                        Some(namespace) => {
                            // task is imported
                            let ns = document
                                .namespace(namespace)
                                .expect("should have a namespace");
                            let doc = ns.document().root();
                            let ast = doc.ast();
                            let ast = ast.as_v1().expect("should be V1 ast");
                            let task = ast
                                .tasks()
                                .find(|t| t.name().inner().text() == name)
                                .expect("should have a task");
                            process_task(
                                map,
                                &task,
                                specified,
                                show_expressions,
                                hide_defaults,
                                prefix,
                                call_name,
                            );
                        }
                        None => {
                            // task is in this document
                            let root = document.root();
                            let ast = root.ast();
                            let ast = ast.as_v1().expect("should be V1 ast");
                            let task = ast
                                .tasks()
                                .find(|t| t.name().inner().to_string() == name)
                                .expect("should have a task");
                            process_task(
                                map,
                                &task,
                                specified,
                                show_expressions,
                                hide_defaults,
                                prefix,
                                call_name,
                            );
                        }
                    };
                }
                CallKind::Workflow => {
                    // workflow is imported
                    let namespace = call
                        .namespace()
                        .expect("workflow calls should have a namespace name");
                    let name = call.name();

                    let namespace = document
                        .namespace(namespace)
                        .expect("should have a namespace");

                    let root = namespace.document().root();
                    let ast = root.ast();
                    let ast = ast.as_v1().expect("should be V1 ast");
                    let wf_ast = ast
                        .workflows()
                        .find(|w| w.name().inner().text() == name)
                        .expect("should have a workflow");

                    process_workflow(
                        map,
                        namespace.document(),
                        namespace
                            .document()
                            .workflow()
                            .expect("should have a workflow"),
                        &wf_ast,
                        call.specified(),
                        show_expressions,
                        hide_defaults,
                        nested_inputs,
                        format!("{prefix}.{call_name}").as_str(),
                        results,
                    );
                    // Remove any inputs that are specified in the workflow
                    specified.iter().for_each(|s| {
                        map.remove(format!("{prefix}.{call_name}.{s}").as_str());
                    });
                }
            }
        })
    }
}

/// Generate a map of inputs for a WDL document.
async fn generate_inputs(
    file: &String,
    show_expressions: bool,
    hide_defaults: bool,
    nested_inputs: bool,
) -> Result<Map<String, Value>> {
    let path = Path::new(file);

    let remote_file = Url::parse(file).is_ok();
    let uri = if remote_file {
        Url::parse(file).unwrap()
    } else {
        path_to_uri(path).unwrap()
    };

    let results = match analyze(file, vec![], false, false).await {
        Ok(results) => results,
        Err(e) => {
            bail!("failed to analyze WDL document: {}", e);
        }
    };

    // Find the result the matches our input file.
    let root = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .expect("should have a root");
    let document = root.document();

    // Create an empty map to store the inputs.
    let mut map = Map::new();

    // If the document has a workflow, check it.
    match document.workflow() {
        Some(workflow) => {
            let wf_name = workflow.name();

            let root = document.root();
            let ast = root.ast();
            let ast = ast.as_v1().expect("should be V1 ast");
            let wf_ast = ast
                .workflows()
                .find(|w| w.name().inner().text() == wf_name)
                .expect("should have a workflow");

            let specified = HashSet::new();
            process_workflow(
                &mut map,
                document,
                workflow,
                &wf_ast,
                &specified,
                show_expressions,
                hide_defaults,
                nested_inputs,
                wf_name,
                &results,
            );
        }
        None => bail!("no workflow found"),
    }
    Ok(map)
}

/// Displays the input schema for a WDL document.
pub async fn input(args: InputArgs) -> Result<()> {
    let path = args.path;

    let inputs = generate_inputs(
        &path,
        args.show_expressions,
        args.hide_defaults,
        args.nested_inputs,
    )
    .await
    .unwrap();

    if args.yaml {
        let yaml = serde_yaml::to_string(&inputs)?;
        println!("{}", yaml);
    } else {
        let json = serde_json::to_string_pretty(&inputs)?;
        println!("{}", json);
    }

    Ok(())
}
