use anyhow::Result;
use clap::Parser;
use serde_json::{Value, json};
use std::path::PathBuf;
use wdl::{
    ast::{
        v1::{
            Expr::{self, Literal}, LiteralExpr, LiteralNone, Type
        }, AstToken, Document, SyntaxKind, Visitor
    },
    grammar::SyntaxTree,
};

#[derive(Parser, Debug)]
#[command(about = "Generate input JSON from a WDL document", version, about)]
pub struct InputsArgs {
    #[arg(required = true)]
    #[clap(value_name = "path")]
    pub document: String,

    #[arg(short, long, default_value = "output")]
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

    let mut template = serde_json::Map::new();

    let json_output = serde_json::to_string_pretty(&template)?;

    // todo:
    // 1. Walk through the AST tree
    // 2. Generate appropriate JSON structure
    // 3. Output to file or stdout (CLI)

    Ok(())
}

// ? idon't want to use the MapType::is_optional and ArrayType::is_optional ... etc for each type since they have the same logic.
fn is_optional(type_: &Type) -> bool {
    type_.is_optional()
}

fn expr_to_json(expr: &Expr) -> Value {
    match expr {
        Literal(literal) => match literal {
            LiteralExpr::Boolean(b) => Value::Bool(b.value()),
            LiteralExpr::Integer(i) => Value::Number(i.value().unwrap_or(0).into()),
            LiteralExpr::String(s) => Value::String(s.text().unwrap().as_str().to_string()),
            // LiteralExpr::None(_) => Value::Null,
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}
