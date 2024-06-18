use clap::Parser;
use colored::Colorize;
use wdl::lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about, after_help = list_all_rules())]
pub struct Args {
    /// The name of the rule to explain.
    #[arg(required = true)]
    pub rule_name: String,
}

pub fn list_all_rules() -> String {
    let mut result = "Available rules:".to_owned();
    for rule in lint::v1::rules() {
        result.push_str(&format!("\n  - {}", rule.id()));
    }
    result
}

pub fn pretty_print_rule(rule: &dyn lint::v1::Rule) -> String {
    let mut result = format!("{}", rule.id().bold().underline());
    result = format!("{}\n{}", result, rule.description());
    result = format!("{}\n{}", result, format!("{}", rule.tags()).yellow());
    result = format!("{}\n\n{}", result, rule.explanation());
    match rule.url() {
        Some(url) => format!("{}\n{}", result, url.underline().blue()),
        None => result,
    }
}

pub fn explain(args: Args) -> anyhow::Result<()> {
    let name = args.rule_name;

    let rule = lint::v1::rules()
        .into_iter()
        .find(|rule| rule.id().to_lowercase() == name.to_lowercase());

    match rule {
        Some(rule) => {
            println!("{}", pretty_print_rule(&*rule));
        }
        None => {
            println!("{}", list_all_rules());
            anyhow::bail!("No rule found with the name '{}'", name);
        }
    }

    Ok(())
}
