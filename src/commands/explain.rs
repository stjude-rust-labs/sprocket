//! Implementation of the explain command.

use anyhow::bail;
use clap::Parser;
use colored::Colorize;
use wdl::analysis;
use wdl::lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about, after_help = list_all_rules())]
pub struct Args {
    /// The name of the rule to explain.
    #[arg(required = true)]
    pub rule_name: String,
}

/// Lists all rules as a string for displaying after CLI help.
pub fn list_all_rules() -> String {
    let mut result = "Available rules:".to_owned();
    let analysis_rules = analysis::rules();
    let lint_rules = lint::rules();

    let mut indexes = (0..(analysis_rules.len() + lint_rules.len())).collect::<Vec<_>>();

    let id = |index: usize| {
        if index >= analysis_rules.len() {
            lint_rules[index - analysis_rules.len()].id()
        } else {
            analysis_rules[index].id()
        }
    };

    indexes.sort_by(|a, b| id(*a).cmp(id(*b)));

    for index in indexes {
        result.push_str(&format!("\n  - {}", id(index)));
    }

    result
}

/// Pretty prints a lint rule to a string.
pub fn pretty_print_lint_rule(rule: &dyn lint::Rule) {
    println!(
        "{id} {tags}",
        id = rule.id().bold().underline(),
        tags = format!("{}", rule.tags()).yellow()
    );
    println!("{desc}", desc = rule.description());
    println!("\n{explanation}", explanation = rule.explanation());

    if let Some(url) = rule.url() {
        println!("\n{url}", url = url.underline().blue());
    }
}

/// Pretty prints an analysis rule to a string.
pub fn pretty_print_analysis_rule(rule: &dyn analysis::Rule) {
    println!("{id}", id = rule.id().bold().underline());
    println!("{desc}", desc = rule.description());
    println!("\n{explanation}", explanation = rule.explanation());
}

/// Explains a lint rule.
pub fn explain(args: Args) -> anyhow::Result<()> {
    let name = args.rule_name;
    let lowercase_name = name.to_lowercase();

    match analysis::rules()
        .into_iter()
        .find(|rule| rule.id().to_lowercase() == lowercase_name)
    {
        Some(rule) => {
            pretty_print_analysis_rule(rule.as_ref());
        }
        None => {
            match lint::rules()
                .into_iter()
                .find(|rule| rule.id().to_lowercase() == lowercase_name)
            {
                Some(rule) => {
                    pretty_print_lint_rule(rule.as_ref());
                }
                None => {
                    println!("{rules}\n", rules = list_all_rules());
                    bail!("No rule found with the name `{name}`");
                }
            }
        }
    }

    Ok(())
}
