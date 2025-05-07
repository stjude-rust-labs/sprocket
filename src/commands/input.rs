//! Implementation of the `input` command.

use std::collections::HashSet;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use serde_json::Map;
use serde_json::Value;
use wdl::analysis::types::CallKind;
use wdl::ast::AstNode;
use wdl::ast::AstToken;
use wdl::ast::SyntaxKind;
use wdl::ast::v1::Expr;
use wdl::ast::v1::InputSection;
use wdl::ast::v1::LiteralExpr;
use wdl::ast::v1::Type;
use wdl::cli::Analysis;
use wdl::cli::analysis::AnalysisResults;
use wdl::cli::analysis::Source;

/// Arguments for the `input` subcommand.
#[derive(Parser, Debug)]
pub struct InputArgs {
    /// The path to the WDL document for which to generate an input template.
    #[arg(value_name = "PATH or URL")]
    pub path: Source,

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

/// Compute key name for map
fn compute_key_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Process an expression
fn process_expression(
    expr: &Expr,
    name: &str,
    typ: Type,
    show_expressions: bool,
    hide_defaults: bool,
    prefix: &str,
) -> (String, Value) {
    match expr {
        Expr::Literal(l) if !hide_defaults => match l {
            LiteralExpr::Boolean(b) => (compute_key_name(prefix, name), Value::Bool(b.value())),
            LiteralExpr::String(s) => {
                if s.is_empty() {
                    (
                        compute_key_name(prefix, name),
                        Value::String("".to_string()),
                    )
                } else {
                    let t = s.text();
                    let t = t.expect("should have text");
                    let mut text: String = "".to_string();
                    t.unescape_to(&mut text);
                    (compute_key_name(prefix, name), Value::String(text))
                }
            }
            LiteralExpr::Integer(i) => (
                compute_key_name(prefix, name),
                Value::from(i.value().expect("should have a value")),
            ),
            LiteralExpr::Float(f) => (
                compute_key_name(prefix, name),
                Value::from(f.value().expect("should have a value")),
            ),
            LiteralExpr::Struct(s) => {
                // Convert the struct to a map and store that.
                let mut map_value = Map::new();
                s.items().for_each(|f| {
                    let (name, value) = f.name_value();
                    let (key, value) = process_expression(
                        &value,
                        name.text(),
                        typ.clone(),
                        show_expressions,
                        hide_defaults,
                        "",
                    );
                    map_value.insert(key, value);
                });
                (compute_key_name(prefix, name), Value::Object(map_value))
            }
            _ => (
                compute_key_name(prefix, name),
                Value::String(format!("{} (default = {})", typ, expr.text())),
            ),
        },
        Expr::Negation(n) if !hide_defaults => {
            // Negation isn't a literal, but might contain one.
            if n.inner().children().count() != 1
                || n.inner().first_child().expect("should have a child").kind()
                    != SyntaxKind::LiteralIntegerNode
            {
                if show_expressions && !hide_defaults {
                    (
                        compute_key_name(prefix, name),
                        Value::String(format!("{} (default = {})", typ, expr.text())),
                    )
                } else {
                    ("".to_string(), Value::Null)
                }
            } else {
                let value = n
                    .text()
                    .to_string()
                    .parse::<i64>()
                    .expect("should be an integer");
                (compute_key_name(prefix, name), Value::from(value))
            }
        }
        _ => {
            if show_expressions && !hide_defaults {
                (
                    compute_key_name(prefix, name),
                    Value::String(format!("{} (default = {})", typ, expr.text())),
                )
            } else {
                ("".to_string(), Value::Null)
            }
        }
    }
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
                false => {
                    let (key, value) = process_expression(
                        &value,
                        &name,
                        typ,
                        show_expressions,
                        hide_defaults,
                        wf_name,
                    );
                    if !key.is_empty() {
                        map.insert(key, value);
                    }
                }
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
#[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
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
    results: &AnalysisResults,
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
    file: &Source,
    show_expressions: bool,
    hide_defaults: bool,
    nested_inputs: bool,
) -> Result<Map<String, Value>> {
    // Parse the WDL document.
    let results = match Analysis::default()
        .extend_sources(vec![file.clone()])
        .run()
        .await
    {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    // Find the result the matches our input file.
    let root = results
        .filter(&[file])
        .next()
        .expect("should have a matching result");
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
