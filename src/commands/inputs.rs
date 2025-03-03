use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use wdl::ast::Document;

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
    println!("{:?}", source);
    println!("{:?}", document);
    println!("{:?}", diagnostics);
    Ok(())
}
