//! Implementation of the explain command.

use std::collections::HashSet;

use anyhow::Ok;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use colored::Colorize;
use wdl::analysis;
use wdl::lint;
use wdl::lint::Tag;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about, after_help = generate_after_help())]
pub struct Args {
    /// The name of the rule to explain.
    #[arg(required_unless_present_any = ["tag", "definitions"], value_name = "RULE_NAME")]
    pub rule_name: Option<String>,

    /// List all rules with the given tag.
    #[arg(short, long, value_name = "TAG", conflicts_with_all = ["rule_name", "definitions"])]
    pub tag: Option<String>,

    /// Display general WDL definitions.
    #[arg(long, conflicts_with_all = ["rule_name", "tag"])]
    pub definitions: bool,
}

fn generate_after_help() -> String {
    format!("{}\n\n{}", list_all_rules(), list_all_tags())
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

/// Lists all tags as a string for displaying after CLI help.
pub fn list_all_tags() -> String {
    let mut result = "Available tags:".to_owned();
    let lint_rules = lint::rules();

    let mut tags: HashSet<Tag> = HashSet::new();
    for rule in lint_rules {
        for tag in rule.tags().iter() {
            tags.insert(tag);
        }
    }

    let mut tags: Vec<Tag> = tags.into_iter().collect();
    tags.sort_unstable_by(|a, b| a.to_string().cmp(&b.to_string()));

    for tag in tags {
        result.push_str(&format!("\n  - {}", tag));
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

    let related = rule.related_rules();
    if !related.is_empty() {
        println!("\n{}", "Related Rules:".bold());
        let mut sorted_related = related.iter().collect::<Vec<_>>();
        sorted_related.sort_unstable_by(|a, b| a.cmp(b));
        sorted_related.iter().for_each(|rule| {
            println!("  - {}", rule.cyan());
        });
    };
}

/// Pretty prints an analysis rule to a string.
pub fn pretty_print_analysis_rule(rule: &dyn analysis::Rule) {
    println!("{id}", id = rule.id().bold().underline());
    println!("{desc}", desc = rule.description());
    println!("\n{explanation}", explanation = rule.explanation());
}

/// Explains a lint rule.
pub fn explain(args: Args) -> anyhow::Result<()> {
    if args.definitions {
        println!("{}", lint::DEFINITIONS_TEXT);
        return Ok(());
    };

    if let Some(tag) = args.tag {
        let target = tag.parse::<Tag>().map_err(|_| {
            println!("{}\n", list_all_tags());
            anyhow!("Invalid tag '{}'", tag)
        })?;

        let rules = lint::rules()
            .into_iter()
            .filter(|rule| rule.tags().contains(target))
            .collect::<Vec<_>>();

        if rules.is_empty() {
            println!("{}\n", list_all_tags());
            bail!("No rules found with the tag `{}`", tag);
        } else {
            println!("Rules with the tag `{}`:", tag);
            let mut rule_ids = rules.iter().map(|rule| rule.id()).collect::<Vec<_>>();
            rule_ids.sort_unstable_by(|a, b| a.cmp(b));
            for id in rule_ids {
                println!("  - {}", id);
            }
        }
        return Ok(());
    }

    if let Some(rule_name) = args.rule_name {
        let lowercase_name = rule_name.to_lowercase();

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
                        bail!("No rule found with the name `{rule_name}`");
                    }
                }
            }
        }

        Ok(())
    } else {
        bail!("Invalid arguments: either a rule_name or a tag must be provided");
    }
}
