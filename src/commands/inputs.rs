use anyhow::Result;
use clap::Parser;
use serde_json::{Value, json};
use std::path::PathBuf;
use wdl::{
    ast::{
        AstToken, Document, SyntaxKind, Visitor,
        v1::{
            self,
            Expr::{self, Literal},
            LiteralExpr, LiteralNone, Type,
        },
    },
    doc,
    grammar::SyntaxTree,
};

#[derive(Parser, Debug)]
#[command(about = "Generate input JSON from a WDL document", version, about)]
pub struct InputsArgs {
    #[arg(required = true)]
    #[clap(value_name = "path")]
    pub document: String,

    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub include_defaults: bool,
}

pub async fn generate_inputs(args: InputsArgs) -> Result<()> {
    let source = std::fs::read_to_string(&args.document)?;
    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        for diagnostic in diagnostics {
            anyhow::bail!("Failed to parse WDL document: {:?}", diagnostic);
        }
    }

    // workflow = document.workflows().first() or error "No workflow found"
    // inputs = workflow.input().declarations() or empty_list

    // todo: handle multiple workflows
    let workflow = document
        .ast()
        .unwrap_v1()
        .workflows()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No workflow found in the document"))?;
    let inputs = workflow.input().map(|input| input.declarations()).unwrap();

    // todo handle tasks

    let mut template = serde_json::Map::new();

    for decl in inputs {
        let name = decl.name().as_str().to_string();
        // let ty = decl.ty();
        // Create a default expression if none is provided
        let value = if let Some(expr) = decl.expr() {
            expr_to_json(&expr)
        } else {
            Value::Null
        };

        // todo: workflow_name.input_name .. currently it's just input_name
        template.insert(name, value);
    }

    let json_output = serde_json::to_string_pretty(&template)?;

    if let Some(output_path) = args.output {
        std::fs::write(output_path, json_output)?;
    } else {
        // ? output in the console if no output path is provided is that good?
        println!("{}", json_output);
    }

    // todo:
    // 1. Walk through the AST tree
    // 2. Generate appropriate JSON structure
    // 3. Output to file or stdout (CLI)

    Ok(())
}

fn is_optional(type_: &Type) -> bool {
    type_.is_optional()
}

fn expr_to_json(expr: &Expr) -> Value {
    match expr {
        Literal(literal) => match literal {
            LiteralExpr::Boolean(b) => Value::Bool(b.value()),
            LiteralExpr::Integer(i) => Value::Number(i.value().unwrap_or(0).into()),
            LiteralExpr::String(s) => Value::String(s.text().unwrap().as_str().to_string()),
            LiteralExpr::None(_) => Value::Null,
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

/*
{
    "workflow_name": {
        "input_name": null,
        "input_name": null,
        "input_name": null
    }
}


*/
