use anyhow::{Context, Result};
use clap::Parser;
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::{
    ops::Deref,
    path::{Path, PathBuf},
};
use url::Url;
use wdl::{
    analysis::{
        document::{Document, Input},
        path_to_uri,
        types::Type,
    },
    ast::{
        AstToken, Diagnostic, Document as AstDocument, SyntaxKind, Visitor,
        v1::{
            self,
            Expr::{self, Literal},
        },
    },
    cli::analyze,
    doc,
    grammar::SyntaxTree,
};

#[derive(Parser, Debug)]
#[command(about = "Generate input JSON from a WDL document", version, about)]
pub struct InputsArgs {
    #[arg(required = true)]
    #[clap(value_name = "input path")]
    pub document: String,

    #[arg(short, long)]
    #[clap(value_name = "workflow or task name")]
    pub name: Option<String>,

    #[arg(short, long)]
    #[clap(value_name = "output path")]
    pub output: Option<PathBuf>,

    #[arg(short, long)]
    #[clap(value_name = "hide defaults", short = 'd')]
    pub hide_defaults: bool,
    // #[arg(short, long)]
    // #[clap(value_name = "override expressions", short = '4')]
    // pub override_expressions: bool,
}

struct InputVisitor {
    inputs: IndexMap<String, Option<Expr>>, // input_name -> default expression (if any)
}


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

    let document = result.document();

    let diagnostics: &[Diagnostic] = document.diagnostics();
    if diagnostics.is_empty() {
        for diagnostic in diagnostics {
            if diagnostic.severity() == wdl::ast::Severity::Error {
                anyhow::bail!("Failed to parse WDL document: {:?}", diagnostic);
            }
        }
    }

    println!("document: {:?}", document);

    let mut template = serde_json::Map::new();

    // Collect inputs and their parent information
    let inputs_with_parents = collect_inputs_with_parents(&args, document)?;

    for (parent_name, name, input) in inputs_with_parents {
        // Skip this input if hide_defaults is true and the input has a default value
        if args.hide_defaults && has_default(document, parent_name, name) {
            continue;
        }

        let v: &wdl::analysis::types::Type = input.ty();

        // Format the key name based on prefix_names flag
        let key = format!("{}.{}", parent_name, name);

        println!("input name {} value {:?}", key, v);

        let value = type_to_json(&v);
        template.insert(key, value);
    }

    let json_output = serde_json::to_string_pretty(&template)?;

    if let Some(output_path) = args.output {
        std::fs::write(output_path, json_output)?;
    } else {
        println!("OUTPUT    {}", json_output);
    }

    Ok(())
}

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

fn has_default(document: &Document, parent_name: &str, input_name: &str) -> bool {
    // Check workflow inputs
    if let Some(workflow) = document.workflow() {
        let input_section: &IndexMap<String, Input> = workflow.inputs();
        for input in input_section {
            println!("workflow input: {:?}", input);
            if input.0.as_str() == input_name {
                println!("workflow input: {:?}", input.1.ty());
                return !input.1.ty().is_none();
            }
        }
    }

    // Check task inputs
    for task in document.tasks() {
        let input_section: &IndexMap<String, Input> = task.inputs();
        for input in input_section {
            println!("task input: {:?}", input);
            if input.0.as_str() == input_name {
                return !input.1.ty().is_none();
            }
        }
    }

    false
}

// Collects inputs with their parent names
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
                for (input_name, input) in workflow.inputs() {
                    result.push((workflow.name(), input_name.as_str(), input));
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
            for (input_name, input) in workflow.inputs() {
                result.push((workflow.name(), input_name.as_str(), input));
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
                    "Multiple tasks found in document but no name specified. Please provide a name using --name"
                ),
            }
        }
    }

    Ok(result)
}
