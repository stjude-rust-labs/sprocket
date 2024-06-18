use clap::Parser;
use colored::Colorize;
use wdl::lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The name of the rule to explain.
    #[arg(required = true)]
    pub rule_name: String,
}

pub fn list_all_rules() {
    println!("{}", "Available rules:".bold().underline().green());
    for rule in lint::v1::rules() {
        println!("{}", rule.id().green());
    }
}

pub fn pretty_print_rule(rule: &dyn lint::v1::Rule) {
    println!("{}", rule.id().bold().underline());
    println!("{}", rule.description());
    println!("{}", format!("{}", rule.tags()).yellow());
    println!();
    println!("{}", rule.explanation());
    match rule.url() {
        Some(url) => println!("{}", url.underline().blue()),
        None => {}
    }
}

pub fn explain(args: Args) -> anyhow::Result<()> {
    let name = args.rule_name;

    let rule = lint::v1::rules()
        .into_iter()
        .find(|rule| rule.id().to_lowercase() == name.to_lowercase());

    match rule {
        Some(rule) => {
            pretty_print_rule(&*rule);
        }
        None => {
            list_all_rules();
            anyhow::bail!("No rule found with the name '{}'", name);
        }
    }

    Ok(())
}
