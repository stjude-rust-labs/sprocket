use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use indexmap::IndexMap;
use serde_json::Value;
use serde_json::json;
use url::Url;
use wdl::analysis::document::Document;
use wdl::analysis::document::Input;
use wdl::analysis::path_to_uri;
use wdl::analysis::types::Type;
use wdl::ast::AstToken;
use wdl::ast::Diagnostic;
use wdl::ast::Document as AstDocument;
use wdl::ast::SupportedVersion;
use wdl::ast::SyntaxKind;
use wdl::ast::VisitReason;
use wdl::ast::Visitor;
use wdl::ast::v1::Expr::Literal;
use wdl::ast::v1::Expr::{self};
use wdl::ast::v1::InputSection;
use wdl::ast::v1::{self};
use wdl::cli::analyze;
use wdl::doc;

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

type InputDefaultMap = IndexMap<String, bool>; // input_name -> has_default

struct InputVisitor {
    inputs: InputDefaultMap,
}

impl Visitor for InputVisitor {
    type State = ();

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &AstDocument,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            self.inputs.clear(); // Reset the list when starting a new document
            println!("visiting document {:?}", version);
        } else if reason == VisitReason::Exit {
            println!("Exiting document {:?}", version);
        }
    }

    fn input_section(
        &mut self,
        _state: &mut Self::State,
        reason: VisitReason,
        section: &InputSection,
    ) {
        if reason == VisitReason::Enter {
            // Iterate over all declarations in the input section
            for decl in section.declarations() {
                let name = decl.name().as_str().to_string();
                // Check if the declaration has an expression (i.e., a default value)
                let has_default: bool = decl.expr().is_some();
                self.inputs.insert(name, has_default);
            }
        }
    }
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

    let document: &std::sync::Arc<Document> = result.document();

    let diagnostics: &[Diagnostic] = document.diagnostics();
    if diagnostics
        .iter()
        .any(|d| d.severity() == wdl::ast::Severity::Error)
    {
        anyhow::bail!("Failed to parse WDL document: {:?}", diagnostics);
    }

    let mut template = serde_json::Map::new();

    // Collect inputs and their parent information
    let inputs_with_parents = collect_inputs_with_parents(&args, document)?;

    for (parent_name, name, input) in inputs_with_parents {
        // Skip this input if hide_defaults is true and the input has a default value
        if args.hide_defaults && has_default(document, args.document.as_str(), name) {
            continue;
        }

        let v: &wdl::analysis::types::Type = input.ty();

        // Format the key name based on prefix_names flag
        let key = format!("{}.{}", parent_name, name);

        println!(
            "input name {} value {:?}, is default {:?}",
            key,
            v,
            has_default(document, &args.document, name)
        );

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

fn has_default(document: &Document, document_path: &str, input_name: &str) -> bool {
    let ast_doc: AstDocument = document.node();

    let mut visitor = InputVisitor {
        inputs: IndexMap::new(),
    };

    ast_doc.visit(&mut (), &mut visitor);

    visitor.inputs.get(input_name).cloned().unwrap_or(false)
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
                    "Multiple tasks found in document but no name specified. Please provide a \
                     name using --name"
                ),
            }
        }
    }

    Ok(result)
}
