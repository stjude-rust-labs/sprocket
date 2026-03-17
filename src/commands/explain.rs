//! Implementation of the `explain` subcommand.

use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Write;
use std::sync::LazyLock;

use anyhow::anyhow;
use clap::Parser;
use clap::ValueEnum;
use clap::builder::PossibleValuesParser;
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use wdl::analysis;
use wdl::lint;
use wdl::lint::ALL_TAG_NAMES;
use wdl::lint::ALL_TAGS;
use wdl::lint::Config;
use wdl::lint::Tag as WdlLintTag;

use crate::commands::CommandResult;

/// Usage string for the `explain` subcommand.
const USAGE: &str = "sprocket explain [RULE]
    sprocket explain --tag <TAG>
    sprocket explain --definitions";

/// All rule IDs sorted alphabetically.
pub static ALL_RULE_IDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut ids: Vec<String> = analysis::ALL_RULE_IDS
        .iter()
        .chain(lint::ALL_RULE_IDS.iter())
        .map(ToString::to_string)
        .collect();
    ids.sort();
    ids
});

/// The output format.
#[derive(ValueEnum, Copy, Clone, Debug, Default)]
pub enum Format {
    /// The default, human-readable output.
    #[default]
    Default,
    /// Machine-readable JSON.
    Json,
}

impl Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Default => write!(f, "default"),
            Format::Json => write!(f, "json"),
        }
    }
}

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about, after_help = generate_after_help(), override_usage = USAGE)]
pub struct Args {
    /// The name of the rule to explain.
    #[arg(required_unless_present_any = [
        "tag",
        "definitions",
        "list_all_rules",
        "list_all_tags"
    ],
        value_name = "RULE",
        value_parser = PossibleValuesParser::new(ALL_RULE_IDS.iter()),
        ignore_case = true,
        hide_possible_values = true,
    )]
    pub rule_name: Option<String>,

    /// List all rules with the given tag.
    #[arg(short, long, value_name = "TAG",
        conflicts_with_all = ["rule_name", "definitions"],
        value_parser = PossibleValuesParser::new(ALL_TAG_NAMES.iter()),
        ignore_case = true,
        hide_possible_values = true,
    )]
    pub tag: Option<String>,

    /// Display general WDL definitions.
    #[arg(long, conflicts_with_all = ["rule_name", "tag", "format"])]
    pub definitions: bool,

    /// Lists all rules and exits.
    #[arg(long, conflicts_with_all = ["list_all_tags"])]
    pub list_all_rules: bool,

    /// Lists all tags and exits.
    #[arg(long, conflicts_with_all = ["list_all_rules"])]
    pub list_all_tags: bool,

    /// The output format.
    #[arg(long, short, default_value_t = Format::default())]
    pub format: Format,
}

/// The crate that a lint rule is defined in.
#[derive(Copy, Clone, Debug, PartialEq, Serialize)]
pub enum RuleSource {
    /// Defined in `wdl-lint`.
    #[serde(rename = "wdl-lint")]
    WdlLint,
    /// Defined in `wdl-analysis`.
    #[serde(rename = "wdl-analysis")]
    WdlAnalysis,
}

/// A config field that applies to a lint rule.
#[derive(Debug, Serialize)]
pub struct ConfigField {
    /// The name of the field, as it appears in the config file.
    pub name: &'static str,
    /// A Markdown-formatted description of the field.
    pub description: &'static str,
    /// The default value of the field as a TOML string.
    pub default: String,
}

/// A lint rule, either from `wdl-lint` or `wdl-analysis`.
#[derive(Debug, Serialize)]
pub struct Rule {
    /// The crate that the rule is defined in.
    pub source: RuleSource,
    /// The unique ID for the rule.
    pub id: &'static str,
    /// Tags the rule is grouped under, if the crate supports them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// A short description of the rule (possibly Markdown formatted).
    pub description: &'static str,
    /// An extended descriptions of the rule (possibly Markdown formatted).
    pub explanation: &'static str,
    /// Markdown-formatted examples that would trigger the rule.
    pub examples: &'static [&'static str],
    /// An optional URL associated with the rule.
    pub url: Option<&'static str>,
    /// A list of rule IDs related to this rule, if the crate supports them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<&'static [&'static str]>,
    /// Crate-specific configuration fields that apply to this rule.
    pub config: Option<Vec<ConfigField>>,
}

impl Rule {
    /// Convert this rule to a string of the given format.
    fn format(&self, format: Format) -> String {
        match format {
            Format::Default => self.to_string(),
            Format::Json => serde_json::to_string(self).unwrap(),
        }
    }
}

impl Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{id}", id = self.id.bold().underline(),)?;

        match &self.tags {
            Some(tags) => writeln!(f, " [{}]", tags.join(", ").yellow())?,
            None => writeln!(f)?,
        }

        writeln!(f, "{desc}", desc = self.description)?;
        writeln!(f, "\n{explanation}", explanation = self.explanation)?;

        if let Some(url) = self.url {
            writeln!(f, "\n{url}", url = url.underline().blue())?;
        }

        if let Some(related) = self.related
            && !related.is_empty()
        {
            writeln!(f, "\n{}", "Related Rules:".bold())?;
            let mut sorted_related = related.iter().collect::<Vec<_>>();
            sorted_related.sort();
            for rule in sorted_related {
                writeln!(f, "  - {}", rule.cyan())?;
            }
        };

        Ok(())
    }
}

/// All lint rules from `wdl-lint`.
fn wdl_lint() -> impl Iterator<Item = Rule> {
    wdl::lint::rules(&wdl::lint::Config::default())
        .into_iter()
        .map(|rule| {
            let applicable_config_fields = Config::fields()
                .into_iter()
                .filter(|field| field.applicable_lints.contains(&rule.id()))
                .map(|field| ConfigField {
                    name: field.name,
                    description: field.description,
                    default: field.default,
                })
                .collect::<Vec<_>>();

            let applicable_config_fields = if applicable_config_fields.is_empty() {
                None
            } else {
                Some(applicable_config_fields)
            };

            Rule {
                source: RuleSource::WdlLint,
                id: rule.id(),
                tags: Some(rule.tags().iter().map(|tag| tag.to_string()).collect()),
                description: rule.description(),
                explanation: rule.explanation(),
                examples: rule.examples(),
                url: rule.url(),
                related: Some(rule.related_rules()),
                config: applicable_config_fields,
            }
        })
}

/// All lint rules from `wdl-analysis`.
fn wdl_analysis() -> impl Iterator<Item = Rule> {
    wdl::analysis::rules().into_iter().map(|rule| Rule {
        source: RuleSource::WdlAnalysis,
        id: rule.id(),
        tags: None,
        description: rule.description(),
        explanation: rule.explanation(),
        examples: rule.examples(),
        url: None,
        related: None,
        config: None,
    })
}

/// A line rule group tag.
#[derive(Debug, Serialize)]
pub struct Tag {
    /// The name of the tag.
    pub name: String,
    /// All lint rules grouped under this tag.
    pub applicable_lints: Vec<&'static str>,
}

/// Display all rules and tags.
fn generate_after_help() -> String {
    format!("{}\n\n{}", list_all_rules(), list_all_tags())
}

/// Lists all rules as a string for displaying.
pub fn list_all_rules() -> String {
    let mut result = String::from("Available rules:");

    for id in ALL_RULE_IDS.iter() {
        result.push_str(&format!("\n  - {id}"));
    }
    result
}

/// Collects all `wdl-lint` rule tags and their applicable lints.
pub fn collect_all_tags() -> HashMap<WdlLintTag, Tag> {
    let mut tags = HashMap::new();
    for tag in ALL_TAGS.iter() {
        tags.insert(
            *tag,
            Tag {
                name: tag.to_string(),
                applicable_lints: Vec::new(),
            },
        );
    }

    for rule in lint::rules(&Config::default()) {
        for tag in rule.tags().iter() {
            let _ = tags
                .entry(tag)
                .and_modify(|v| v.applicable_lints.push(rule.id()));
        }
    }

    for tag in tags.values_mut() {
        tag.applicable_lints.sort();
    }

    tags
}

/// Lists all tags as a string for displaying.
pub fn list_all_tags() -> String {
    let mut result = String::from("Available tags:");
    for tag in ALL_TAG_NAMES.iter() {
        write!(result, "\n  - {tag}").unwrap();
    }

    result
}

/// Explains a lint rule.
pub fn explain(args: Args) -> CommandResult<()> {
    if args.list_all_rules {
        match args.format {
            Format::Default => println!("{}", list_all_rules()),
            Format::Json => {
                let value =
                    serde_json::to_value(wdl_lint().chain(wdl_analysis()).collect::<Vec<_>>())
                        .map_err(anyhow::Error::from)?;
                println!("{value}")
            }
        }

        return Ok(());
    }

    if args.list_all_tags {
        match args.format {
            Format::Default => {
                println!("{}", list_all_tags());
            }
            Format::Json => {
                let mut all_tags = collect_all_tags().into_values().collect::<Vec<_>>();
                all_tags.sort_by(|a, b| a.name.cmp(&b.name));
                let value = Value::Array(
                    all_tags
                        .into_iter()
                        .map(|tag| serde_json::to_value(&tag).unwrap())
                        .collect(),
                );
                println!("{value}")
            }
        }

        return Ok(());
    }

    if args.definitions {
        println!("{}", lint::DEFINITIONS_TEXT);
        return Ok(());
    };

    if let Some(tag) = args.tag {
        let target = tag.parse::<WdlLintTag>().map_err(|_| {
            println!("{}", list_all_tags());
            anyhow!("invalid tag `{tag}`")
        })?;

        let Some(tag) = collect_all_tags().remove(&target) else {
            return Err(anyhow!("no rules found with the tag `{tag}`").into());
        };

        match args.format {
            Format::Default => {
                println!("Rules with the tag `{}`:", tag.name);
                for id in tag.applicable_lints {
                    println!("  - {id}");
                }
            }
            Format::Json => {
                let value = serde_json::to_value(&tag).map_err(anyhow::Error::from)?;
                println!("{value}");
            }
        }

        return Ok(());
    }

    if let Some(rule_name) = args.rule_name {
        let lowercase_name = rule_name.to_lowercase();

        match wdl_lint()
            .chain(wdl_analysis())
            .find(|rule| rule.id.to_lowercase() == lowercase_name)
        {
            Some(rule) => {
                print!("{}", rule.format(args.format));
            }
            None => {
                println!("{rules}\n", rules = list_all_rules());
                return Err(anyhow!("no rule found with the name `{rule_name}`").into());
            }
        }

        return Ok(());
    }

    unreachable!();
}
