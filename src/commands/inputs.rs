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
use wdl::analysis::types::Optional;
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
    /// Path to the WDL document to generate inputs for, can be a local file
    /// path or remote URL (http/https)
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

    #[arg(short, long)]
    #[clap(value_name = "only required", short = 'R')]
    /// Only include required inputs
    pub only_required: bool,
}

/// Generate input JSON from a WDL document
pub async fn generate_inputs(args: InputsArgs) -> Result<()> {
    // Check if the document path is a remote URL or local file
    let is_remote = args.document.starts_with("http://") || args.document.starts_with("https://");

    // Analyze the document - the analyze function should handle both local and
    // remote files
    let results: Vec<wdl::analysis::AnalysisResult> =
        analyze(args.document.as_str(), vec![], false, false).await?;

    // Parse the document path into a URI that can be used to find the document in
    // analysis results
    let uri: Url = if is_remote {
        // For remote files, directly parse as URL
        Url::parse(args.document.as_str()).context("Failed to parse remote URL")?
    } else {
        // For local files, convert the path to a URI
        path_to_uri(args.document.as_str()).context("Failed to convert local path to URI")?
    };

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context(
            "Failed to find document in analysis results. If using a remote file, ensure it's \
             accessible and valid WDL.",
        )?;

    let document: &std::sync::Arc<Document> = result.document();

    let diagnostics: &[Diagnostic] = document.diagnostics();
    if diagnostics
        .iter()
        .any(|d| d.severity() == wdl::ast::Severity::Error)
    {
        anyhow::bail!(
            "Failed to parse WDL document: {:?}. Ensure the file is valid WDL syntax and \
             accessible.",
            diagnostics
        );
    }

    let (input_defaults, literal_defaults, default_expressions) = collect_all_input_info(document);
    let mut template = serde_json::Map::new();

    // Collect inputs and their parent information
    let inputs_with_parents = collect_inputs_with_parents(&args, document)?;

    for (parent_name, name, input) in inputs_with_parents {
        let v: &wdl::analysis::types::Type = input.ty();
        let key = format!("{}.{}", parent_name, name);

        let is_required = !input.ty().is_optional();
        let has_default = input_defaults.get(name).copied().unwrap_or(false);
        let is_literal_default = literal_defaults.get(name).copied().unwrap_or(false);

        // Required inputs are always rendered
        if is_required {
            let type_str = type_to_string(input.ty());
            template.insert(key, Value::String(type_str));
            continue;
        }

        // Optional inputs behavior
        if args.only_required {
            // Skip optional inputs if only-required is set
            continue;
        }

        if has_default {
            if is_literal_default {
                // For literal defaults, parse and use the actual value
                // Skip if --only-required is provided (already checked above)
                if let Some(expr_str) = default_expressions.get(name) {
                    let literal_value = parse_literal_expression(expr_str);
                    template.insert(key, literal_value);
                }
            } else {
                // For complex expressions
                if !args.hide_expressions {
                    // Format as "<WDL type> default: <parsed expression>"
                    if let Some(expr_str) = default_expressions.get(name) {
                        let type_str = type_to_string(v);
                        let value_str = format!("{} default: {}", type_str, expr_str);
                        template.insert(key, Value::String(value_str));
                    }
                }
            }
        } else {
            // Optional inputs with no default expression get null
            template.insert(key, Value::Null);
        }
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
fn collect_all_input_info(
    document: &Document,
) -> (
    IndexMap<String, bool>,
    IndexMap<String, bool>,
    IndexMap<String, String>,
) {
    let ast_doc: AstDocument = document.node();

    let mut input_defaults: IndexMap<String, bool> = IndexMap::new();
    let mut literal_defaults: IndexMap<String, bool> = IndexMap::new();
    let mut default_values: IndexMap<String, String> = IndexMap::new();

    // Process workflows
    for workflow in ast_doc.ast().unwrap_v1().workflows() {
        if let Some(input) = workflow.input() {
            for decl in input.declarations() {
                let name = decl.name().as_str().to_string();

                // Track if it has a default
                let has_default = decl.expr().is_some();
                input_defaults.insert(name.clone(), has_default);

                // Check if the default is a literal and store the expression
                if let Some(expr) = decl.expr() {
                    let is_literal = matches!(expr, wdl::ast::v1::Expr::Literal(_));
                    literal_defaults.insert(name.clone(), is_literal);

                    // Store the default expression as a string
                    default_values.insert(name, expr.syntax().to_string());
                } else {
                    literal_defaults.insert(name, false);
                }
            }
        }
    }

    // Process tasks
    for task in ast_doc.ast().unwrap_v1().tasks() {
        if let Some(input) = task.input() {
            for decl in input.declarations() {
                let name = decl.name().as_str().to_string();

                // Track if it has a default
                let has_default = decl.expr().is_some();
                input_defaults.insert(name.clone(), has_default);

                // Check if the default is a literal and store the expression
                if let Some(expr) = decl.expr() {
                    let is_literal = matches!(expr, wdl::ast::v1::Expr::Literal(_));
                    literal_defaults.insert(name.clone(), is_literal);

                    // Store the default expression as a string
                    default_values.insert(name, expr.syntax().to_string());
                } else {
                    literal_defaults.insert(name, false);
                }
            }
        }
    }

    (input_defaults, literal_defaults, default_values)
}

/// Collects inputs with their parent names
fn collect_inputs_with_parents<'a>(
    args: &'a InputsArgs,
    document: &'a Document,
) -> Result<Vec<(String, &'a str, &'a Input)>> {
    let mut result: Vec<(String, &str, &'a Input)> = Vec::new();

    if let Some(name) = &args.name {
        // Specific task or workflow requested
        if let Some(task) = document.task_by_name(name) {
            // Error if nested-inputs is used with a task
            if args.nested_inputs {
                anyhow::bail!("--nested-inputs is only valid for workflows, not tasks");
            }

            for (input_name, input) in task.inputs() {
                result.push((task.name().to_string(), input_name, input));
            }
        } else if let Some(workflow) = document.workflow() {
            if workflow.name() == name {
                // Add workflow inputs
                for (input_name, input) in workflow.inputs() {
                    result.push((workflow.name().to_string(), input_name.as_str(), input));
                }

                // If nested_inputs is true, process called tasks correctly
                if args.nested_inputs {
                    collect_nested_inputs(workflow, document, &mut result)?;
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
                result.push((workflow.name().to_string(), input_name.as_str(), input));
            }

            // If nested_inputs is true, process called tasks correctly
            if args.nested_inputs {
                collect_nested_inputs(workflow, document, &mut result)?;
            }
        } else {
            // No workflow - look for exactly one task
            let tasks: Vec<_> = document.tasks().collect();
            match tasks.len() {
                0 => anyhow::bail!("No workflow or tasks found in document"),
                1 => {
                    let task = &tasks[0];
                    // Error if nested-inputs is used with a task
                    if args.nested_inputs {
                        anyhow::bail!("--nested-inputs is only valid for workflows, not tasks");
                    }

                    for (input_name, input) in task.inputs() {
                        result.push((task.name().to_string(), input_name, input));
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

/// Collects nested inputs from workflow calls according to the WDL spec
fn collect_nested_inputs<'a>(
    workflow: &'a wdl::analysis::document::Workflow,
    document: &'a Document,
    result: &mut Vec<(String, &'a str, &'a Input)>,
) -> Result<()> {
    // Analyze AST to access declarations
    let ast_doc: AstDocument = document.node();

    // Process each call in the workflow
    for call in workflow.calls() {
        let name_called: &String = call.0;
        let type_called: &wdl::analysis::types::CallType = call.1;

        // Find the called task
        let called_task = document
            .task_by_name(name_called)
            .ok_or(anyhow::anyhow!("Called task not found: {}", name_called))?;

        let provided_inputs: std::collections::HashSet<&str> = type_called
            .inputs()
            .iter()
            .map(|(name, _input)| name.as_str())
            .collect();

        // Debug what inputs are specified in the call
        println!(
            "Call to {}: Provided inputs: {:?}",
            name_called, provided_inputs
        );

        for (input_name, input) in called_task.inputs() {
            if !provided_inputs.contains(input_name.as_str()) {
                // Check if optional (either by type or default)
                let is_optional_type = input.ty().is_optional();
                let has_default = task_input_has_default(&ast_doc, called_task.name(), input_name);
                let is_not_required = is_optional_type || has_default;

                println!(
                    "  Input {}: optional={}, has_default={}, will_include={}",
                    input_name, is_optional_type, has_default, is_not_required
                );

                if is_not_required {
                    // Use String::from or to_string() to convert workflow name to String first
                    let workflow_name = workflow.name().to_string();
                    // Store format string results directly in the result vector as owned Strings
                    let call_instance_name = format!("{}.{}", workflow_name, name_called);
                    // Change the function signature to accept owned Strings rather than &'a str
                    result.push((call_instance_name, input_name, input));
                }
            }
        }
    }

    Ok(())
}

/// Helper function to check if a task input has a default value
fn task_input_has_default(ast_doc: &AstDocument, task_name: &str, input_name: &str) -> bool {
    // Find the task with the given name
    if let Some(task) = ast_doc
        .ast()
        .unwrap_v1()
        .tasks()
        .find(|t| t.name().as_str() == task_name)
    {
        // Check if the task has an input section
        if let Some(input_section) = task.input() {
            // Find the declaration with the given name
            if let Some(decl) = input_section
                .declarations()
                .find(|d| d.name().as_str() == input_name)
            {
                // Check if the declaration has an expression (default value)
                return decl.expr().is_some();
            }
        }
    }
    false
}

/// Convert a WDL type to a string representation
fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Primitive(ty, _is_optional) => {
            let type_str = match ty {
                wdl::analysis::types::PrimitiveType::Boolean => "Boolean",
                wdl::analysis::types::PrimitiveType::Integer => "Integer",
                wdl::analysis::types::PrimitiveType::Float => "Float",
                wdl::analysis::types::PrimitiveType::String => "String",
                wdl::analysis::types::PrimitiveType::File => "File",
                wdl::analysis::types::PrimitiveType::Directory => "Directory",
            };
            type_str.to_string()
        }
        _ => "Unknown".to_string(),
    }
}

/// Parse a literal expression string into a JSON value
fn parse_literal_expression(expr_str: &str) -> Value {
    // Basic parsing for common literal types
    if expr_str == "true" {
        Value::Bool(true)
    } else if expr_str == "false" {
        Value::Bool(false)
    } else if let Ok(num) = expr_str.parse::<i64>() {
        Value::Number(num.into())
    } else if let Ok(num) = expr_str.parse::<f64>() {
        // This is approximate since serde_json::Number doesn't have a simple from_f64
        if let Some(num_value) = serde_json::Number::from_f64(num) {
            Value::Number(num_value)
        } else {
            Value::String(expr_str.to_string())
        }
    } else {
        // Handle string literals by removing quotes
        if expr_str.starts_with('"') && expr_str.ends_with('"') && expr_str.len() >= 2 {
            Value::String(expr_str[1..expr_str.len() - 1].to_string())
        } else {
            Value::String(expr_str.to_string())
        }
    }
}
