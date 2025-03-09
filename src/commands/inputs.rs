use anyhow::{Context, Result};
use clap::Parser;
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use url::Url;
use wdl::{
    analysis::{path_to_uri, types::Type},
    ast::{
        AstToken, Document, SyntaxKind, Visitor,
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
    #[clap(value_name = "literal defaults")]
    pub literal_defaults: bool,

    #[arg(short, long)]
    #[clap(value_name = "override expressions")]
    pub override_expressions: bool,
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

    let diagnostics = document.diagnostics();
    if diagnostics.is_empty() {
        for diagnostic in diagnostics {
            if diagnostic.severity() == wdl::ast::Severity::Error {
                anyhow::bail!("Failed to parse WDL document: {:?}", diagnostic);
            }
        }
    }

    // search the document to match a task or workflow by name
    let input_section: &IndexMap<String, wdl::analysis::document::Input> = if let Some(name) =
        args.name
    {
        if let Some(task) = document.task_by_name(&name) {
            task.inputs()
        } else if let Some(workflow) = document.workflow() {
            if workflow.name() == name {
                workflow.inputs()
            } else {
                anyhow::bail!("No task or workflow found with name '{}'", name);
            }
        } else {
            anyhow::bail!("No task or workflow found with name '{}'", name);
        }
    } else {
        // If no name is provided, try workflow first
        if let Some(workflow) = document.workflow() {
            workflow.inputs()
        } else {
            // No workflow - look for exactly one task
            let tasks: Vec<_> = document.tasks().collect();
            println!("tasks: {:?}", tasks);
            match tasks.len() {
                0 => anyhow::bail!("No workflow or tasks found in document"),
                1 => tasks[0].inputs(),
                _ => anyhow::bail!(
                    "Multiple tasks found in document but no name specified. Please provide a name using --name"
                ),
            }
        }
    };

    let mut template = serde_json::Map::new();

    for decl in input_section {
        let name = decl.0;
        let input: &wdl::analysis::document::Input = decl.1;
        let v: &wdl::analysis::types::Type = input.ty();

        println!("input name {} value {:?}", name, v);

        let value = type_to_json(&v);

        template.insert(name.to_string(), value);
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
            // ? how should i handle other primitive types?
            wdl::analysis::types::PrimitiveType::Boolean => Value::Bool(false),
            wdl::analysis::types::PrimitiveType::Integer => Value::Number(0.into()),
            // wdl::analysis::types::PrimitiveType::Float => Value::Number(0.0.into()),
            wdl::analysis::types::PrimitiveType::String => Value::String("".to_string()),
            // wdl::analysis::types::PrimitiveType::File => Value::String("".to_string()),
            // wdl::analysis::types::PrimitiveType::Directory => Value::String("".to_string()),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}
