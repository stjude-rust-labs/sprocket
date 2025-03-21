//! Implementation of the inputs command.

use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use indexmap::IndexMap;
use serde_json::Value;
use url::Url;
use wdl::analysis::document::Document;
use wdl::analysis::document::Input;
use wdl::analysis::path_to_uri;
use wdl::analysis::types::Type;
use wdl::ast::AstToken;
use wdl::ast::Diagnostic;
use wdl::ast::Document as AstDocument;
use wdl::cli::analyze;

/// Command-line arguments for generating input JSON from a WDL document
#[derive(Parser, Debug)]
#[command(about = "Generate input JSON from a WDL document", version, about)]
pub struct InputsArgs {
    #[arg(required = true)]
    #[clap(value_name = "input path")]
    /// Path to the WDL document to generate inputs for
    pub document: String,

    #[arg(short, long)]
    #[clap(value_name = "workflow or task name")]
    /// Name of the workflow or task to generate inputs for
    pub name: Option<String>,

    #[arg(short, long)]
    #[clap(value_name = "output path")]
    /// Path to save the generated input JSON file
    pub output: Option<PathBuf>,

    #[arg(short, long)]
    #[clap(value_name = "nested inputs", short = 'N')]
    /// Include nested inputs from called tasks
    pub nested_inputs: bool,

    #[arg(short, long)]
    #[clap(value_name = "hide defaults", short = 'D')]
    /// Hide inputs with default values
    pub hide_defaults: bool,

    #[arg(short, long)]
    #[clap(value_name = "hide expressions", short = 'E')]
    /// Hide inputs with non-literal default values
    pub hide_expressions: bool,
}

/// Generate input JSON from a WDL document
pub async fn generate_inputs(args: InputsArgs) -> Result<()> {
    let results: Vec<wdl::analysis::AnalysisResult> =
        analyze(args.document.as_str(), vec![], false, false).await?;

    let uri: Url = Url::parse(args.document.as_str()).unwrap_or_else(|_| {
        path_to_uri(args.document.as_str()).expect("file should be a local path")
    });

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context("failed to find document in analysis results")?;

    let document: &std::sync::Arc<Document> = result.document();

    let diagnostics: &[Diagnostic] = document.diagnostics();
    if diagnostics
        .iter()
        .any(|d| d.severity() == wdl::ast::Severity::Error)
    {
        anyhow::bail!("Failed to parse WDL document: {:?}", diagnostics);
    }

    let (input_defaults, literal_defaults) = collect_all_input_info(document);
    // Collect all input information in a single visit of the AST
    let mut template = serde_json::Map::new();

    // Collect inputs and their parent information
    let inputs_with_parents = collect_inputs_with_parents(&args, document)?;

    for (parent_name, name, input) in inputs_with_parents {
        // Skip if hide_defaults is true and input has any default
        if args.hide_defaults && input_defaults.get(name).copied().unwrap_or(false) {
            continue;
        }

        // Skip if hide_expressions is true and input has non-literal default
        if args.hide_expressions && !literal_defaults.get(name).copied().unwrap_or(false) {
            continue;
        }

        let v: &wdl::analysis::types::Type = input.ty();
        let key = format!("{}.{}", parent_name, name);
        let value = type_to_json(v);
        template.insert(key, value);
    }

    let json_output = serde_json::to_string_pretty(&template)?;

    if let Some(output_path) = args.output {
        std::fs::write(output_path, json_output)?;
    } else {
        println!("{}", json_output);
    }

    Ok(())
}

/// Collects all input information in a single pass through the AST
fn collect_all_input_info(document: &Document) -> (IndexMap<String, bool>, IndexMap<String, bool>) {
    let ast_doc: AstDocument = document.node();

    let mut input_defaults: IndexMap<String, bool> = IndexMap::new();
    let mut literal_defaults: IndexMap<String, bool> = IndexMap::new();

    // Process workflows
    for workflow in ast_doc.ast().unwrap_v1().workflows() {
        if let Some(input) = workflow.input() {
            let workflow_literal_defaults: IndexMap<String, bool> = input
                .declarations()
                .map(|decl| {
                    let name = decl.name().as_str().to_string();
                    let default = decl.expr().is_some();
                    (name, default)
                })
                .collect();

            // Copy values to the function-level maps
            literal_defaults.extend(workflow_literal_defaults);

            let workflow_input_defaults: IndexMap<String, bool> = input
                .declarations()
                .map(|decl| {
                    let name = decl.name().as_str().to_string();
                    // ? should we check if the default is a literal here too?
                    let default = decl.ty().is_optional();
                    (name, default)
                })
                .collect();

            input_defaults.extend(workflow_input_defaults);
        }
    }

    // Process tasks
    for task in ast_doc.ast().unwrap_v1().tasks() {
        if let Some(input) = task.input() {
            let task_literal_defaults: IndexMap<String, bool> = input
                .declarations()
                .map(|decl| {
                    let name = decl.name().as_str().to_string();
                    let default = decl.expr().is_some();
                    (name, default)
                })
                .collect();

            literal_defaults.extend(task_literal_defaults);

            let task_input_defaults: IndexMap<String, bool> = input
                .declarations()
                .map(|decl| {
                    let name = decl.name().as_str().to_string();
                    let default = decl.ty().is_optional();
                    (name, default)
                })
                .collect();

            input_defaults.extend(task_input_defaults);
        }
    }

    println!("input_defaults: {:?}", input_defaults);
    println!("literal_defaults: {:?}", literal_defaults);
    (input_defaults, literal_defaults)
}

/// Converts a WDL type to its JSON representation
fn type_to_json(ty: &Type) -> Value {
    match ty {
        Type::Primitive(ty, _bool) => match ty {
            wdl::analysis::types::PrimitiveType::Boolean => Value::String("Boolean".to_string()),
            wdl::analysis::types::PrimitiveType::Integer => Value::String("Integer".to_string()),
            wdl::analysis::types::PrimitiveType::Float => Value::String("Float".to_string()),
            wdl::analysis::types::PrimitiveType::String => Value::String("String".to_string()),
            wdl::analysis::types::PrimitiveType::File => Value::String("File".to_string()),
            wdl::analysis::types::PrimitiveType::Directory => {
                Value::String("Directory".to_string())
            }
        },

        _ => Value::Null,
    }
}

/// Collects inputs with their parent names
fn collect_inputs_with_parents<'a>(
    args: &'a InputsArgs,
    document: &'a Document,
) -> Result<Vec<(&'a str, &'a str, &'a Input)>> {
    let mut result: Vec<(&str, &str, &Input)> = Vec::new();

    if let Some(name) = &args.name {
        // Specific task or workflow requested
        if let Some(task) = document.task_by_name(name) {
            for (input_name, input) in task.inputs() {
                result.push((task.name(), input_name, input));
            }
        } else if let Some(workflow) = document.workflow() {
            if workflow.name() == name {
                // Add workflow inputs
                for (input_name, input) in workflow.inputs() {
                    result.push((workflow.name(), input_name.as_str(), input));
                }

                // If nested_inputs is true, add all called task inputs
                if args.nested_inputs {
                    for task in document.tasks() {
                        for (input_name, input) in task.inputs() {
                            result.push((task.name(), input_name, input));
                        }
                    }
                }
            } else {
                anyhow::bail!("No task or workflow found with name '{}'", name);
            }
        } else {
            anyhow::bail!("No task or workflow found with name '{}'", name);
        }
    } else {
        // No name provided, try workflow first
        if let Some(workflow) = document.workflow() {
            // Add workflow inputs
            for (input_name, input) in workflow.inputs() {
                result.push((workflow.name(), input_name.as_str(), input));
            }

            // If nested_inputs is true, add all task inputs
            if args.nested_inputs {
                for task in document.tasks() {
                    for (input_name, input) in task.inputs() {
                        result.push((task.name(), input_name, input));
                    }
                }
            }
        } else {
            // No workflow - look for exactly one task
            let tasks: Vec<_> = document.tasks().collect();
            match tasks.len() {
                0 => anyhow::bail!("No workflow or tasks found in document"),
                1 => {
                    let task = &tasks[0];
                    for (input_name, input) in task.inputs() {
                        result.push((task.name(), input_name.as_str(), input));
                    }
                }
                _ => anyhow::bail!(
                    "Multiple tasks found in document but no name specified. Please provide a \
                     name using --name"
                ),
            }
        }
    }

    Ok(result)
}
