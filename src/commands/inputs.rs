use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use url::Url;
use wdl::{
    analysis::path_to_uri,
    ast::{
        AstToken, Document, SyntaxKind, Visitor,
        v1::{
            self,
            Expr::{self, Literal},
            LiteralExpr, LiteralNone, Type,
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
    #[clap(value_name = "output path")]
    pub output: Option<PathBuf>,

    #[arg(long)]
    #[clap(value_name = "include defaults")]
    pub include_defaults: bool,
}

pub async fn generate_inputs(args: InputsArgs) -> Result<()> {
    println!("{:?}", args);

    let results: Vec<wdl::analysis::AnalysisResult> =
        analyze(args.document.as_str(), vec![], false, false).await?;

    // println!("{:?}", results);

    let uri: Url = Url::parse(args.document.as_str()).unwrap_or_else(|_| {
        path_to_uri(args.document.as_str()).expect("file should be a local path")
    });

    // println!("{:?}", uri);

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context("failed to find document in analysis results")?;

    // println!("result: {:?}", result);

    let document = result.document();

    // println!("doc: {:?}", document);

    let doc_name = Path::new(&args.document)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&args.document);

    let (_path, name, inputs) = wdl::cli::parse_inputs(
        document,
        Some(doc_name),
        Some(Path::new(args.document.as_str())),
    )?;
    // if !diagnostics.is_empty() {
    //     for diagnostic in diagnostics {
    //         anyhow::bail!("Failed to parse WDL document: {:?}", diagnostic);
    //     }
    // }

    println!("{:?},{:?}, {:?}", _path, name, inputs);
    // workflow = document.workflows().first() or error "No workflow found"
    // inputs = workflow.input().declarations() or empty_list

    let mut template = serde_json::Map::new();

    let workflow_inputs: &wdl::engine::WorkflowInputs =
        inputs.as_workflow_inputs().expect("worflow input problem");

    for decl in workflow_inputs.iter() {
        let name = decl.0;
        let v: &wdl::engine::Value = decl.1;

        println!("{v}");
        let value = expr_to_json(v.clone());

        template.insert(name.to_string(), value);
    }

    let json_output = serde_json::to_string_pretty(&template)?;

    if let Some(output_path) = args.output {
        std::fs::write(output_path, json_output)?;
    } else {
        println!("{}", json_output);
    }

    Ok(())
}

fn is_optional(type_: &Type) -> bool {
    type_.is_optional()
}

fn expr_to_json(expr: wdl::engine::Value) -> Value {
    match expr {
        // Literal(literal) => match literal {
        //     LiteralExpr::Boolean(b) => Value::Bool(b.value()),
        //     LiteralExpr::Integer(i) => Value::Number(i.value().unwrap_or(0).into()),
        //     LiteralExpr::String(s) => Value::String(s.text().unwrap().as_str().to_string()),
        //     LiteralExpr::None(_) => Value::Null,
        //     _ => Value::Null,
        // },
        _ => Value::Null,
    }
}
